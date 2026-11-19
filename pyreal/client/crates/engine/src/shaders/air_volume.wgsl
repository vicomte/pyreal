// The air's light, a froxel at a time: a grid over the view, its
// slices spaced out with distance, each cell lit by the sun through its cascades (where walls,
// roofs and leaves break the light: the shafts) and by the sky, and summed from the eye outward
// as light scattered toward the eye and the share of what lies behind that still shows. The
// apply pass reads the sum at each pixel's distance.

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

// The sky's share in the air at full night: the night dome's light over the night sky's.
const NIGHT_AIR: f32 = 3.0 / 18.0;

// How forward the shafts' extra light scatters.
const SHAFT_FORWARD: f32 = 0.75;

@group(1) @binding(0) var air_shadow: texture_depth_2d;
@group(1) @binding(1) var air_cmp: sampler_comparison;
@group(1) @binding(2) var air_out: texture_storage_3d<rgba16float, write>;
@group(1) @binding(3) var<uniform> fog: AirParams;

// The `k`th slice's far side, metres from the eye.
fn slice_distance(k: f32) -> f32 {
    return fog.slices.x * pow(fog.slices.y / fog.slices.x, k / fog.slices.z);
}

// Henyey-Greenstein's phase.
fn phase(cos_t: f32, g: f32) -> f32 {
    let g2 = g * g;
    return (1.0 - g2) / (12.566371 * pow(max(1.0 + g2 - 2.0 * g * cos_t, 1e-4), 1.5));
}

// How much of the sun reaches `p`, `d` metres from the eye: one compared tap of its cascade.
fn air_sun(p: vec3<f32>, d: f32) -> f32 {
    if ((frame.switches.w & F_SHADOWS) == 0u || d >= frame.cascade_far.w) {
        return 1.0;
    }
    var c = 0u;
    for (var k = 0u; k < 3u; k++) {
        if (d >= frame.cascade_far[k]) {
            c = k + 1u;
        }
    }
    let clip = frame.cascades[c] * vec4<f32>(p, 1.0);
    let lo = vec2<f32>(f32(c & 1u), f32(c >> 1u)) * 0.5;
    let uv = (clip.xy * vec2<f32>(0.5, -0.5) + 0.5) * 0.5 + lo;
    if (any(uv <= lo) || any(uv >= lo + vec2<f32>(0.5)) || clip.z >= 1.0) {
        return 1.0;
    }
    return textureSampleCompareLevel(air_shadow, air_cmp, uv, clip.z - frame.cascade_texel[c] / frame.cascade_depth[c]);
}

// How much of the air round the eye stands in shade, under a canopy or among walls: there the
// few beams that reach it show most (render-hifi's).
fn eye_shade() -> f32 {
    var shaded = 0.0;
    for (var k = 0u; k < 8u; k++) {
        let a = f32(k) * 0.7853982;
        let round = frame.camera.xyz + vec3<f32>(cos(a) * 6.0, sin(a) * 6.0, 2.0 + f32(k % 2u) * 4.0);
        shaded += 1.0 - air_sun(round, 6.0);
    }
    return shaded / 8.0;
}

@compute @workgroup_size(8, 8, 1)
fn froxels(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(air_out);
    if (id.x >= size.x || id.y >= size.y) {
        return;
    }
    let uv = (vec2<f32>(id.xy) + 0.5) / vec2<f32>(size.xy);
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let eye = frame.camera.xyz;
    let sun = frame.sun_light.rgb;
    // By night the sky's dome is far darker than the light it gives (sky.rs), and the air takes
    // its light from the dome: a night's ground is not veiled as a day's.
    let sky = frame.sky_light.rgb * fog.more.z * mix(NIGHT_AIR, 1.0, frame.sun_dir.w);
    // By a low sun the air sends on more of it, the lower the more, and more in shade.
    let low = 1.0 - clamp(frame.sun_dir.z / 0.5, 0.0, 1.0);
    let most = fog.shafts.z * (1.0 + fog.shafts.x * low) * (1.0 + fog.shafts.y * eye_shade());
    let day = mix(1.0, most, smoothstep(0.0, 0.3, low));
    // The low sun's air is thinner, all but the shafts near the eye: mornings stay clear.
    let clear = 1.0 - fog.streaks.y * frame.sun_dir.w * (1.0 - smoothstep(0.3, 0.9, frame.sun_dir.z));
    var scattered = vec3<f32>(0.0);
    var through = 1.0;
    var last = 0.0;
    let slices = u32(fog.slices.z);
    for (var k = 0u; k < slices; k++) {
        let far = slice_distance(f32(k) + 1.0);
        // A point within the slice, jittered frame to frame.
        let d = mix(last, far, fog.more.y);
        let h = frame.inv_view_proj * vec4<f32>(ndc, fog.more.x / max(d, 1e-3), 1.0);
        let p = h.xyz / h.w;
        let view_dir = normalize(p - eye);
        let thin = exp(-max(p.z - fog.density.w, 0.0) / fog.density.z);
        let sigma = fog.density.x * thin * clear;
        let ds = far - last;
        let t = exp(-sigma * ds);
        let near = 1.0 - smoothstep(0.5 * fog.shafts.w, fog.shafts.w, d);
        // The shafts' extra light is sent on more narrowly, so it shows toward the sun.
        let cos_t = dot(view_dir, frame.sun_dir.xyz);
        let shafts = (day - 1.0) * near * frame.sun_dir.w / clear * phase(cos_t, SHAFT_FORWARD);
        let lit = sun * (phase(cos_t, fog.slices.w) + shafts) * air_sun(p, d) * fog.density.y + sky * 0.5;
        scattered += through * lit * (1.0 - t);
        through *= t;
        last = far;
        textureStore(air_out, vec3<u32>(id.xy, k), vec4<f32>(scattered, through));
    }
}

