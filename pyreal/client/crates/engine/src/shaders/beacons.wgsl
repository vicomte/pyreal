// A lighthouse beacon (`beacons.rs`): a giant magical white light. Its heart is a ball of light,
// a white-hot core in a wide halo, swelling slowly and crossed by turning rays; its shaft a soft
// column of light rising into the sky, shimmering as it climbs; retail's burst and wisps round
// the heart drawn as white light, faded out at their cards' edges so their many cards make one.

const BEACON_WISP: u32 = 0u;
const BEACON_HEART: u32 = 1u;
const BEACON_SHAFT: u32 = 2u;

// Its light's colour, a cool white, and its core's, white; how bright each part is, in multiples
// of white on screen.
const BEACON_WHITE: vec3<f32> = vec3<f32>(0.86, 0.93, 1.0);
const BEACON_WISP_GAIN: f32 = 0.28;
const BEACON_CORE_GAIN: f32 = 1.4;
const BEACON_HALO_GAIN: f32 = 0.2;
const BEACON_RAY_GAIN: f32 = 0.3;
const BEACON_SHAFT_GAIN: f32 = 0.4;
// Where a wisp's card starts to fade out, of its half width.
const BEACON_EDGE: f32 = 0.4;
// How fast it swells, radians a second, and by how much; how fast its rays turn.
const BEACON_PULSE: f32 = 1.1;
const BEACON_SWELL: f32 = 0.12;
const BEACON_TURN: f32 = 0.15;
// The share of its light the heart keeps with the eye at its middle, whole from half again its
// reach out; a wisp's with the eye at its middle, whole from this far out, metres.
const BEACON_INSIDE: f32 = 0.1;
const BEACON_WISP_NEAR: f32 = 12.0;
// The heart's reach over its distance past which its halo and rays thin, so near it they never
// fill the view (the eye would stop down to nothing round them).
const BEACON_WIDE: f32 = 0.35;

// The share of the heart's reach over which what stands before it hides it: its glow is not a point.
const BEACON_HIDDEN: f32 = 0.35;

// How much of the heart at `centre`, reach `r`, shows past what stands in the pixel: none where a
// wall stands before it, all where nothing does. Its card is drawn before the eye, so the depth
// test cannot hide it.
fn beacon_shown(clip: vec4<f32>, centre: vec3<f32>, r: f32) -> f32 {
    let d = textureLoad(g_depth, vec2<i32>(clip.xy), 0);
    if (d <= 0.0) {
        return 1.0;
    }
    let ndc = vec2<f32>(clip.x / frame.params.z * 2.0 - 1.0, 1.0 - clip.y / frame.params.w * 2.0);
    let h = frame.inv_view_proj * vec4<f32>(ndc, d, 1.0);
    let eye = frame.camera.xyz;
    let wall = distance(h.xyz / h.w, eye);
    let at = dot(centre - eye, normalize(h.xyz / h.w - eye));
    return smoothstep(at - r * BEACON_HIDDEN, at + r * BEACON_HIDDEN, wall);
}

fn beacon_part(flags: u32) -> u32 {
    return (flags >> 13u) & 3u;
}

// A corner of a beacon's card: a wisp's as its picture's; the heart's turned to the eye round its
// light's reach; the shaft's stood up from the heart and turned about its height to face the eye.
fn beacon_card(p: Particle, world: vec3<f32>, uv: vec2<f32>) -> vec3<f32> {
    let centre = vec3<f32>(p.m0.w, p.m1.w, p.m2.w);
    let part = beacon_part(p.flags);
    if (part == BEACON_HEART) {
        return mist_card(centre, p.m0.x, uv);
    }
    if (part == BEACON_SHAFT) {
        let to_eye = frame.camera.xyz - centre;
        let right = normalize(cross(vec3<f32>(0.0, 0.0, 1.0), to_eye) + vec3<f32>(1e-4, 0.0, 0.0));
        return centre + right * (uv.x * 2.0 - 1.0) * p.m0.x + vec3<f32>(0.0, 0.0, (1.0 - uv.y) * p.m2.z);
    }
    return world;
}

// A beacon's light: its picture `c` and coverage `a` for a wisp.
fn beacon_seen(in: PartOut, c: vec4<f32>, a: f32, soft: f32) -> Seen {
    let t = frame.sun_light.w;
    let swell = 1.0 + BEACON_SWELL * sin(t * BEACON_PULSE);
    let q = vec2<f32>(in.uv.x * 2.0 - 1.0, 1.0 - in.uv.y * 2.0);
    let part = beacon_part(in.flags);
    var light = vec3<f32>(0.0);
    var cover = soft;
    if (part == BEACON_HEART) {
        let r2 = dot(q, q);
        if (r2 >= 1.0) {
            discard;
        }
        let edge = exp(-5.0);
        let halo = max(exp(-r2 * 5.0) - edge, 0.0) / (1.0 - edge);
        let core = exp(-r2 * 160.0 / swell);
        // Two sets of slow rays, turning against each other, thinning out to the halo's edge.
        let ang = atan2(q.y, q.x);
        let rays = (pow(abs(cos(ang * 3.0 + t * BEACON_TURN)), 24.0)
            + 0.6 * pow(abs(cos(ang * 4.0 - t * BEACON_TURN * 1.4 + 0.7)), 32.0)) * (1.0 - sqrt(r2)) * halo;
        // Dimmed as the eye comes inside it, so one standing on the tower sees by it.
        let shown = beacon_shown(in.clip, in.center, in.size);
        if (shown <= 0.0) {
            discard;
        }
        let d = distance(frame.camera.xyz, in.center);
        let inside = mix(BEACON_INSIDE, 1.0, smoothstep(0.0, in.size * 1.5, d));
        let wide = in.size / max(d, 1e-3) / BEACON_WIDE;
        let spread = 1.0 / (1.0 + wide * wide);
        light = (BEACON_WHITE * (halo * BEACON_HALO_GAIN + rays * BEACON_RAY_GAIN) * swell * spread
            + vec3<f32>(1.0) * core * BEACON_CORE_GAIN) * inside * shown;
    } else if (part == BEACON_SHAFT) {
        let h = 1.0 - in.uv.y;
        let edge = exp(-6.0);
        let across = max(exp(-q.x * q.x * 6.0) - edge, 0.0) / (1.0 - edge);
        let along = smoothstep(0.0, 0.06, h) * pow(1.0 - h, 1.8);
        let shimmer = 0.55 + 0.9 * smoke_fbm(vec3<f32>(q.x * 1.5, h * 14.0 - t * 1.2, t * 0.15));
        light = BEACON_WHITE * across * along * shimmer * BEACON_SHAFT_GAIN * swell;
    } else {
        let e = abs(q);
        let fade = (1.0 - smoothstep(BEACON_EDGE, 1.0, e.x)) * (1.0 - smoothstep(BEACON_EDGE, 1.0, e.y));
        let near = mix(BEACON_INSIDE, 1.0, smoothstep(0.0, BEACON_WISP_NEAR, distance(frame.camera.xyz, in.center)));
        light = BEACON_WHITE * luma(c.rgb) * BEACON_WISP_GAIN * swell * near;
        cover = a * fade * soft;
    }
    var out: Seen;
    out.additive = true;
    out.colour = vec4<f32>(light * (1.0 - fog_amount(distance(in.world.xy, frame.camera.xy))), cover);
    return out;
}
