struct AirParams {
    // x: the air's density at the eye's height, per metre; y: how much of the sun it scatters
    // (the god rays' level); z: how fast it thins with height, metres; w: the height it thins from.
    density: vec4<f32>,
    // x: the first slice's distance; y: the last's; z: slices; w: how forward the sun scatters (g).
    slices: vec4<f32>,
    // x: the near plane; y: the slices' jitter this frame (0..1); z: the sky's share; w: how much of
    // the air the apply pass lays over the picture.
    more: vec4<f32>,
    // By day: x how much more the sun scatters with it on the horizon, y with the eye in shade,
    // z near the eye, w how far that reaches, metres.
    shafts: vec4<f32>,
    // x: how strongly the shafts streak from the sun on screen; y: how much thinner the air is
    // with the sun low by day; zw unused.
    streaks: vec4<f32>,
}

// The apply pass: the air between the eye and each pixel laid over it.
@group(1) @binding(0) var air_sum: texture_3d<f32>;
@group(1) @binding(1) var air_linear: sampler;
@group(1) @binding(2) var air_depth: texture_depth_2d;
@group(1) @binding(3) var<uniform> air_apply: AirParams;

// How many points along the line to the sun the streaks look at.
const STREAK_SAMPLES: i32 = 40;
// How much each counts less than the one before, how far along the line a pixel looks (a share of
// it), and how tight the bright sky about the sun is.
const STREAK_DECAY: f32 = 0.96;
const STREAK_REACH: f32 = 0.8;
const STREAK_TIGHT: f32 = 10.0;

// How much open sky shows at `uv`, the brighter the nearer the sun at `sun`.
fn open_sky(uv: vec2<f32>, sun: vec2<f32>, aspect: f32) -> f32 {
    let size = vec2<f32>(textureDimensions(air_depth));
    let px = vec2<i32>(clamp(uv, vec2<f32>(0.0), vec2<f32>(0.9999)) * size);
    if (textureLoad(air_depth, px, 0) > 0.0) {
        return 0.0;
    }
    let off = (uv - sun) * vec2<f32>(aspect, 1.0);
    return exp(-dot(off, off) * STREAK_TIGHT);
}

// Shafts of sunlight streaking from the sun where open sky shows past leaves, branches and roofs
// (render-hifi's): each pixel looks back along the line to the sun on screen and gathers the open
// sky it passes, nearer counting more.
fn streaks(p: vec2<f32>) -> vec3<f32> {
    // Most with the sun low, as the air's shafts.
    let low = 1.0 - clamp(frame.sun_dir.z / 0.5, 0.0, 1.0);
    let strength = air_apply.streaks.x * frame.sun_dir.w * mix(0.2, 1.0, smoothstep(0.0, 0.3, low));
    let clip = frame.view_proj * vec4<f32>(frame.sun_dir.xyz, 0.0);
    if (strength <= 0.0 || frame.sun_dir.z <= 0.0 || clip.w <= 1e-4) {
        return vec3<f32>(0.0);
    }
    let sun = vec2<f32>(clip.x / clip.w * 0.5 + 0.5, 0.5 - clip.y / clip.w * 0.5);
    // Out to half a screen past its edge the streaks still reach in.
    let past = max(max(max(-sun.x, sun.x - 1.0), max(-sun.y, sun.y - 1.0)), 0.0);
    let shown = clamp(1.0 - past / 0.5, 0.0, 1.0);
    if (shown <= 0.0) {
        return vec3<f32>(0.0);
    }
    let size = vec2<f32>(textureDimensions(air_depth));
    let uv = p / size;
    let aspect = size.x / max(size.y, 1.0);
    let toward = (sun - uv) * STREAK_REACH / f32(STREAK_SAMPLES);
    let ign = fract(52.9829189 * fract(dot(p, vec2<f32>(0.06711056, 0.00583715))));
    var at = uv + toward * fract(ign + air_apply.more.y);
    var weight = 1.0;
    var gathered = 0.0;
    for (var i = 0; i < STREAK_SAMPLES; i++) {
        gathered += open_sky(at, sun, aspect) * weight;
        weight *= STREAK_DECAY;
        at += toward;
    }
    return frame.sun_light.rgb * (gathered * strength * shown / f32(STREAK_SAMPLES));
}

@fragment
fn fs_air(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let px = vec2<i32>(p.xy);
    let depth = textureLoad(air_depth, px, 0);
    let a = air_apply.slices;
    // The sky's pixels take the whole of the slices, but by a low sun only the shafts' near part,
    // so the sky about it keeps its blue.
    let low = smoothstep(0.0, 0.3, 1.0 - clamp(frame.sun_dir.z / 0.5, 0.0, 1.0));
    let sky = mix(a.y, 0.3 * air_apply.shafts.w, frame.sun_dir.w * low);
    let d = select(sky, air_apply.more.x / depth, depth > 0.0);
    let k = log(clamp(d, a.x, a.y) / a.x) / log(a.y / a.x) * a.z - 1.0;
    let w = (clamp(k, 0.0, a.z - 1.0) + 0.5) / a.z;
    let uv = p.xy / vec2<f32>(textureDimensions(air_depth));
    var s = textureSampleLevel(air_sum, air_linear, vec3<f32>(uv, w), 0.0);
    let near = clamp(d / a.x, 0.0, 1.0);
    // Nearer than the first slice, its share by distance; then the share laid over the picture.
    let laid = near * air_apply.more.w;
    s = vec4<f32>(s.rgb * laid, mix(1.0, s.a, laid));
    let shafts = streaks(p.xy) * air_apply.more.w;
    // Added in the frame's exposure: the HDR target is drawn pre-exposed.
    return vec4<f32>(min((s.rgb + shafts) * pre_exposure(), vec3<f32>(HDR_MOST)), s.a);
}
