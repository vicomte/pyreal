// Waterfalls (`falls::frame_falls`). A sheet is a strip of rows down the path its lip's water
// takes: from the pool above where it stops short of the edge, thrown off the edge and pulled
// down until it lands, then down the slope to the pool, each row a quad between two lips. Its water streams down it in ropes
// of noise moving at the water's own speed, glassy and clear at the lip, whitening as it falls,
// thinning into torn streaks at its sides and breaking up where it lands. Foam spreads over the
// pool from where it lands, and soft puffs of mist roll up from there, each swelling and fading
// over a few seconds. All of it is lit as the translucent world is.

struct Fall {
    // A sheet's row: its top corners (w how far along the run, metres) and its bottom ones (w
    // seconds fallen there). Foam: a and b where its ends' water lands, c and d which way that
    // water runs. Mist: a its puff's foot (w its radius), c which way the water runs.
    a: vec4<f32>,
    b: vec4<f32>,
    c: vec4<f32>,
    d: vec4<f32>,
    // A sheet's row: seconds fallen at its top corners, and the share of its fall at its top and
    // bottom.
    times: vec4<f32>,
    // The run's length, a card's half width (foam: its reach; mist: its radius), and a seed.
    more: vec4<f32>,
    // x its kind (0 foam, 1 sheet, 2 mist); y 1 in a room, 2 a sheet starting under the pool above
    // the lip; z the wrapped clock and w how strongly
    // the sun makes a bow in the spray (f32 bits).
    kind: vec4<u32>,
}

@group(3) @binding(15) var<storage, read> falls: array<Fall>;

// The water: deep where it runs clear, and its white where the air churns into it.
const FALL_DEEP: vec3<f32> = vec3<f32>(0.04, 0.12, 0.14);
const FALL_WHITE: vec3<f32> = vec3<f32>(0.7, 0.8, 0.86);
// How far down its fall (a share of it) the water turns white, and how torn that edge is.
const FALL_WHITENS: vec2<f32> = vec2<f32>(0.18, 0.8);
// How much of what is behind it clear water and white water hide.
const FALL_CLEAR: f32 = 0.55;
const FALL_SOLID: f32 = 0.96;
// How wide its frayed sides are, metres, and over how deep it fades into what is behind it.
const FALL_FRAY: f32 = 4.0;
const FALL_SOFT: f32 = 0.3;
// Its ropes' size across, cycles a metre, and along, cycles a second of fall.
const FALL_ROPES: vec2<f32> = vec2<f32>(0.45, 1.6);
// Foam: how strong at its heart, and how fast it spreads, metres a second.
const FALL_FOAM_STRENGTH: f32 = 0.85;
const FALL_FOAM_SPREAD: f32 = 1.3;
// How far over the water's level it lies, metres, over the waves.
const FALL_FOAM_LIFT: f32 = 0.3;
// Mist: a puff's life, seconds; how much it hides at its heart; how far it rises and swells.
const FALL_MIST_LIFE: f32 = 5.0;
const FALL_MIST_OPACITY: f32 = 0.5;
const FALL_MIST_RISE: f32 = 0.9;
const FALL_MIST_ALBEDO: vec3<f32> = vec3<f32>(0.92, 0.95, 0.97);
// The spray's rainbow: the angles from the point opposite the sun its violet and red edges lie
// at, degrees, and how bright it is beside the sunlit mist.
const FALL_BOW_ANGLES: vec2<f32> = vec2<f32>(39.8, 43.0);
const FALL_BOW_GAIN: f32 = 0.8;

struct FallOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    // A sheet: metres along its run, seconds fallen. Foam: metres along, metres out from where it
    // lands. Mist: across its card, -1 to 1.
    @location(1) at: vec2<f32>,
    // A sheet: the share of its fall; mist: its life's fade.
    @location(2) share: f32,
    @location(3) normal: vec3<f32>,
    @location(4) @interpolate(flat) index: u32,
    // Mist: its light, worked out once for the puff.
    @location(5) @interpolate(flat) light: vec3<f32>,
}

fn fall_hash(c: vec2<i32>) -> f32 {
    var h = (bitcast<u32>(c.x) * 0x8da6b343u) ^ (bitcast<u32>(c.y) * 0xd8163841u);
    h = (h ^ (h >> 15u)) * 0x2c1b3c6du;
    h ^= h >> 12u;
    return f32(h & 0xFFFFFFu) / 16777215.0;
}

fn fall_noise(p: vec2<f32>) -> f32 {
    let i = vec2<i32>(floor(p));
    let f = fract(p);
    let s = f * f * (3.0 - 2.0 * f);
    return mix(mix(fall_hash(i), fall_hash(i + vec2<i32>(1, 0)), s.x),
               mix(fall_hash(i + vec2<i32>(0, 1)), fall_hash(i + vec2<i32>(1, 1)), s.x), s.y);
}

fn fall_fbm(p: vec2<f32>) -> f32 {
    return fall_noise(p) * 0.55 + fall_noise(p * 2.03 + 17.0) * 0.3 + fall_noise(p * 4.11 + 41.0) * 0.15;
}

fn fall_clock(f: Fall) -> f32 {
    return bitcast<f32>(f.kind.z);
}

// The rainbow's colour seen along `dir` with the sun toward `sun`, its strength `bow`: violet on
// its inner edge to red on its outer, soft at both.
fn fall_bow(dir: vec3<f32>, sun: vec3<f32>, bow: f32) -> vec3<f32> {
    if (bow <= 0.0) {
        return vec3<f32>(0.0);
    }
    let angle = degrees(acos(clamp(dot(dir, -sun), -1.0, 1.0)));
    let x = (angle - FALL_BOW_ANGLES.x) / (FALL_BOW_ANGLES.y - FALL_BOW_ANGLES.x);
    if (x <= 0.0 || x >= 1.0) {
        return vec3<f32>(0.0);
    }
    let hue = 0.75 * (1.0 - x);
    let rgb = clamp(abs(fract(vec3<f32>(hue) + vec3<f32>(0.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0 - 3.0) - 1.0,
                    vec3<f32>(0.0), vec3<f32>(1.0));
    return rgb * sin(3.14159 * x) * bow;
}

@vertex
fn vs_fall(@builtin(vertex_index) vi: u32, @builtin(instance_index) i: u32) -> FallOut {
    let f = falls[i];
    let c = array<vec2<f32>, 6>(vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
                                vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0))[vi % 6u];
    var out: FallOut;
    out.index = i;
    out.share = 0.0;
    out.light = vec3<f32>(0.0);
    var world: vec3<f32>;
    if (f.kind.x == 1u) {
        world = mix(mix(f.a.xyz, f.b.xyz, c.x), mix(f.c.xyz, f.d.xyz, c.x), c.y);
        out.normal = normalize(cross(f.b.xyz - f.a.xyz, mix(f.c.xyz - f.a.xyz, f.d.xyz - f.b.xyz, 0.5)));
        out.at = vec2<f32>(mix(f.a.w, f.b.w, c.x), mix(mix(f.times.x, f.times.y, c.x), mix(f.c.w, f.d.w, c.x), c.y));
        out.share = mix(f.times.z, f.times.w, c.y);
    } else if (f.kind.x == 0u) {
        // Foam: over the pool each side of where the water lands, a little over the water.
        let s = c.x;
        let out_from = (c.y * 2.0 - 1.0) * f.more.y;
        let dir = normalize(mix(f.c.xy, f.d.xy, s));
        world = mix(f.a.xyz, f.b.xyz, s) + vec3<f32>(dir * out_from, FALL_FOAM_LIFT);
        out.normal = vec3<f32>(0.0, 0.0, 1.0);
        out.at = vec2<f32>(mix(f.a.w, f.b.w, s), out_from);
    } else {
        // Mist: a card turned to the eye, rising and swelling over its life, out from the fall.
        let life = fract(fall_clock(f) / FALL_MIST_LIFE + f.more.z * 0.618);
        let r = f.a.w * mix(0.6, 1.25, life);
        let centre = f.a.xyz + vec3<f32>(f.c.xy * f.a.w * (0.2 + 0.5 * life), f.a.w * (0.3 + FALL_MIST_RISE * life));
        let to_eye = normalize(frame.camera.xyz - centre);
        let right = normalize(cross(vec3<f32>(0.0, 0.0, 1.0), to_eye) + vec3<f32>(1e-4, 0.0, 0.0));
        let up = cross(to_eye, right);
        let q = c * 2.0 - 1.0;
        world = centre + (right * q.x + up * q.y) * r;
        out.normal = to_eye;
        out.at = q;
        out.share = sin(3.14159 * life);
        // Lit through, as a cloud is: the sun, the sky, never a mirror.
        let bits = select(G_FOLIAGE | G_NO_MIRROR, G_FOLIAGE | G_NO_MIRROR | G_INDOOR, (f.kind.y & 1u) != 0u);
        out.light = shade_lit(FALL_MIST_ALBEDO, to_eye, to_eye, 1.0, 0.0, 0.0, bits, 1.0, 1.0,
                              vec3<f32>(0.0), vec4<f32>(0.0));
    }
    out.world = world;
    out.clip = frame.view_proj * vec4<f32>(world, 1.0);
    return out;
}

// How much of a piece shows at its pixel beside what stands behind it, faded over `reach`.
fn fall_soft(in: FallOut, reach: f32) -> f32 {
    let d = textureLoad(g_depth, vec2<i32>(in.clip.xy), 0);
    if (d <= 0.0) {
        return 1.0;
    }
    let ndc = vec2<f32>(in.clip.x / frame.params.z * 2.0 - 1.0, 1.0 - in.clip.y / frame.params.w * 2.0);
    let h = frame.inv_view_proj * vec4<f32>(ndc, d, 1.0);
    let behind = view_depth_at(h.xyz / h.w) - view_depth_at(in.world);
    return clamp(behind / reach, 0.0, 1.0);
}

fn sheet_seen(in: FallOut, f: Fall, dir: vec3<f32>, bits: u32, indoor: bool) -> vec4<f32> {
    let clock = fall_clock(f);
    let seed = f.more.z;
    // The water's pattern moves down with it: a second into the fall now was a second less a
    // second ago.
    let p = vec2<f32>(in.at.x * FALL_ROPES.x + seed, (in.at.y - clock) * FALL_ROPES.y);
    let ropes = fall_noise(p);
    let streaks = fall_noise(vec2<f32>(p.x * 3.7 + 5.0, p.y * 2.3));
    let fine = fall_noise(vec2<f32>(p.x * 9.0 + 9.0, p.y * 5.0));
    let water = ropes * 0.55 + streaks * 0.3 + fine * 0.15;
    // Whiter as it falls, torn where the ropes run thin.
    let white = smoothstep(FALL_WHITENS.x, FALL_WHITENS.y, in.share + (water - 0.5) * 1.3);
    // Between its ropes the water runs thin.
    let thick = smoothstep(0.2, 0.75, water);
    var a = mix(FALL_CLEAR, FALL_SOLID, white) * mix(mix(0.55, 0.92, white), 1.12, thick);
    // Frayed at the run's ends, broken up where it lands.
    let edge = min(in.at.x, f.more.x - in.at.x) + (streaks - 0.5) * FALL_FRAY;
    a *= smoothstep(0.0, FALL_FRAY, edge);
    a *= 1.0 - smoothstep(0.9, 1.0, in.share + (ropes - 0.5) * 0.1);
    // Water that starts under the pool above needs no fade in at its top.
    if ((f.kind.y & 2u) == 0u) {
        a *= smoothstep(0.0, 0.03, in.share);
    }
    a = clamp(a, 0.0, 1.0) * fall_soft(in, FALL_SOFT);
    if (a <= 0.003) {
        discard;
    }
    var n = normalize(in.normal);
    if (dot(n, dir) > 0.0) {
        n = -n;
    }
    // The ropes bend its surface across.
    let tilt = (streaks - 0.5) * 0.5 * (1.0 - white);
    let across = normalize(cross(n, vec3<f32>(0.0, 0.0, 1.0)) + vec3<f32>(1e-4, 0.0, 0.0));
    n = normalize(n + across * tilt);
    let albedo = mix(FALL_DEEP, FALL_WHITE * mix(0.6, 1.0, streaks), white * mix(0.7, 1.0, fine));
    var lit = forward_light(albedo, in.world, n, -dir, mix(0.12, 0.7, white), 0.0, 0.0, bits, in.clip.xy);
    // Clear water mirrors the sky.
    let fresnel = 0.02 + 0.98 * pow(1.0 - clamp(dot(n, -dir), 0.0, 1.0), 5.0);
    if (!indoor) {
        lit += sky_colour(reflect(dir, n)) * mix(fresnel, 0.08, white) * 0.8;
    }
    return vec4<f32>(through_air(lit, in.world, dir, indoor), a);
}

fn foam_seen(in: FallOut, f: Fall, dir: vec3<f32>, bits: u32, indoor: bool) -> vec4<f32> {
    let clock = fall_clock(f);
    let reach = f.more.y;
    let out_from = abs(in.at.y);
    // Spreading rings of churned water, thinning outward.
    let p = vec2<f32>(in.at.x * 0.5 + f.more.z, (out_from - clock * FALL_FOAM_SPREAD) * 0.7);
    let churn = fall_fbm(p);
    let bubbles = fall_noise(vec2<f32>(in.at.x * 2.3, (out_from - clock * FALL_FOAM_SPREAD) * 2.3) + 31.0);
    let heart = 1.0 - smoothstep(0.0, 1.0, out_from / reach);
    let edge = min(in.at.x, f.more.x - in.at.x);
    var a = smoothstep(0.5, 0.8, churn * 0.7 + bubbles * 0.3 + heart * 0.3) * heart * FALL_FOAM_STRENGTH;
    a *= smoothstep(-1.0, FALL_FRAY, edge + (churn - 0.5) * FALL_FRAY);
    if (a <= 0.003) {
        discard;
    }
    var lit = forward_light(FALL_WHITE * mix(0.7, 1.0, bubbles), in.world, vec3<f32>(0.0, 0.0, 1.0), -dir,
                            0.8, 0.0, 0.0, bits, in.clip.xy);
    if (!indoor) {
        lit += sky_colour(reflect(dir, vec3<f32>(0.0, 0.0, 1.0))) * 0.08;
    }
    return vec4<f32>(through_air(lit, in.world, dir, indoor), a);
}

fn mist_puff_seen(in: FallOut, f: Fall, dir: vec3<f32>, bits: u32, indoor: bool) -> vec4<f32> {
    let r2 = dot(in.at, in.at);
    if (r2 >= 1.0) {
        discard;
    }
    let clock = fall_clock(f);
    let seed = f.more.z;
    let billow = fall_fbm(in.at * 1.6 + vec2<f32>(seed * 3.1, seed * 1.7 - clock * 0.12));
    let body = (1.0 - r2) * (1.0 - r2);
    var a = smoothstep(0.15, 0.75, billow * 0.7 + body * 0.6) * body * FALL_MIST_OPACITY * in.share;
    // Thin near the eye, so standing in it is a haze, not a wall.
    let near = distance(frame.camera.xyz, in.world);
    a *= smoothstep(0.6, 1.6, near / max(f.a.w, 1.0));
    a *= fall_soft(in, 1.5);
    if (a <= 0.003) {
        discard;
    }
    // Sunlight bent through the drops: a faint bow where the spray lies at its angle.
    var light = in.light;
    if (!indoor) {
        light += fall_bow(dir, frame.sun_dir.xyz, bitcast<f32>(f.kind.w)) * frame.sun_light.rgb * FALL_BOW_GAIN;
    }
    return vec4<f32>(through_air(light, in.world, dir, indoor), a);
}

fn fall_seen(in: FallOut) -> vec4<f32> {
    let f = falls[in.index];
    let dir = normalize(in.world - frame.camera.xyz);
    let indoor = (f.kind.y & 1u) != 0u;
    let bits = select(0u, G_INDOOR, indoor);
    if (f.kind.x == 1u) {
        return sheet_seen(in, f, dir, bits, indoor);
    }
    if (f.kind.x == 0u) {
        return foam_seen(in, f, dir, bits, indoor);
    }
    return mist_puff_seen(in, f, dir, bits, indoor);
}

@fragment
fn fs_fall(in: FallOut) -> SortedOut {
    var out = sorted_out(fall_seen(in), false);
    // The moving water keeps no history in the upscaler.
    out.reactive = vec4<f32>(clamp(out.colour.a, 0.0, 1.0));
    return out;
}

@fragment
fn fs_fall_oit(in: FallOut) -> OitOut {
    return oit_out(fall_seen(in), false, in.world);
}
