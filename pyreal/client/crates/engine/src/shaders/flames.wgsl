// Flames where objects burn (`flames::frame_flames`): a card per flame turned to the eye about
// the upright, its fire scrolling noise shaped by its kind (a candle's teardrop, a torch's
// licking tongue) under a temperature ramp, white-yellow at the core to red at the tips, drawn
// additively bright enough to bloom. It breathes with its lamp's flicker and leans with the wind
// outdoors. A brazier's bed of fire is drawn as its fire puffs (`particles.wgsl`'s `fire_seen`).

struct Flame {
    base: vec3<f32>,
    height: f32,
    // Bits 0-7 the kind (1 candle, 2 torch, 4 a body of fire, 5 one of its points, 6 a
    // length of its trail); bit 8 in a room; bit 9 flickering; bits 10-15 a body's whirl; bits
    // 16-31 its count of points.
    flags: u32,
    seed: u32,
    reach: f32,
    // A body's colour, linear bytes; a point's tip width, its bits (`body_fire.wgsl`).
    colour: u32,
}

@group(3) @binding(12) var<storage, read> flames: array<Flame>;

// How bright a flame's hottest part is on screen, in multiples of white.
const FLAME_GAIN: f32 = 5.0;
// How far round a flame its glow reaches, in flame widths, and how bright it is.
const FLAME_HALO: f32 = 0.18;

struct FlameOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    // x across the flame (its width one), y up it (its height one).
    @location(1) at: vec2<f32>,
    @location(2) @interpolate(flat) kind: u32,
    @location(3) @interpolate(flat) seed: u32,
    // rgb its flicker now; a how far it leans.
    @location(4) @interpolate(flat) flick: vec4<f32>,
    @location(5) @interpolate(flat) size: f32,
    @location(6) @interpolate(flat) index: u32,
}

// A flame's half width over its height, and how far its card reaches past it each side.
fn flame_width(kind: u32) -> f32 {
    return select(0.22, 0.32, kind == 1u);
}

@vertex
fn vs_flame(@builtin(vertex_index) vi: u32, @builtin(instance_index) i: u32) -> FlameOut {
    let f = flames[i];
    var out: FlameOut;
    let kind = f.flags & 255u;
    var flick = vec3<f32>(1.0);
    if ((f.flags & 512u) != 0u) {
        // A body flutters as its lamp does (its reach is its flames' width; its light reaches
        // thirty of them).
        if (kind == 4u) {
            flick = flicker(3u, f.seed, f.reach * 30.0);
        } else {
            flick = flicker(1u, f.seed, f.reach);
        }
    }
    // The card: two flame widths each side for the glow, from below its base to over its tip.
    let corner = array<vec2<f32>, 6>(vec2<f32>(-1.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(-1.0, 1.0),
                                     vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0))[vi];
    out.index = i;
    out.kind = kind;
    out.seed = f.seed;
    out.flick = vec4<f32>(flick, 0.0);
    out.size = f.height;
    // A point of a body's fire draws nothing of its own; a body, a card over its ball.
    if (kind == 5u || kind == 6u) {
        out.clip = vec4<f32>(2.0, 2.0, 2.0, 1.0);
        return out;
    }
    if (kind == 4u) {
        out.clip = body_fire_card(f.base, f.height, corner);
        out.world = f.base;
        return out;
    }
    let half = flame_width(kind) * 2.2;
    let at = vec2<f32>(corner.x * half, mix(-0.35, 1.5, corner.y));
    let to_eye = frame.camera.xyz - f.base;
    var side = vec3<f32>(-to_eye.y, to_eye.x, 0.0);
    side = select(vec3<f32>(1.0, 0.0, 0.0), normalize(side), length(side) > 1e-4);
    // Outdoors the wind leans it along the card.
    var lean = 0.0;
    if ((f.flags & 256u) == 0u) {
        lean = clamp(dot(frame.wind.xy, side.xy) * (0.25 + 0.2 * frame.wind.z), -0.5, 0.5);
    }
    let world = f.base + side * (at.x * f.height) + vec3<f32>(0.0, 0.0, at.y * f.height);
    out.clip = frame.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    out.at = vec2<f32>(at.x / flame_width(kind), at.y);
    out.kind = kind;
    out.seed = f.seed;
    out.flick = vec4<f32>(flick, lean);
    out.size = f.height;
    return out;
}

fn fire_hash(p: vec2<f32>, seed: u32) -> f32 {
    let q = bitcast<vec2<u32>>(vec2<i32>(floor(p)));
    var h = (q.x * 0x9E3779B1u) ^ (q.y * 0x85EBCA77u);
    h ^= seed * 0xC2B2AE3Du;
    h = (h ^ (h >> 15u)) * 0x27D4EB2Fu;
    h ^= h >> 13u;
    return f32(h & 0xFFFFu) / 65535.0;
}

// Smooth value noise, 0 to 1.
fn fire_noise(p: vec2<f32>, seed: u32) -> f32 {
    let i = floor(p);
    let f = p - i;
    let u = f * f * (3.0 - 2.0 * f);
    let a = fire_hash(i, seed);
    let b = fire_hash(i + vec2<f32>(1.0, 0.0), seed);
    let c = fire_hash(i + vec2<f32>(0.0, 1.0), seed);
    let d = fire_hash(i + vec2<f32>(1.0, 1.0), seed);
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// Fire's colour at temperature `t` (0 to 1): red embers, orange, yellow, a white-yellow core.
fn fire_colour(t: f32) -> vec3<f32> {
    var c = mix(vec3<f32>(0.35, 0.03, 0.0), vec3<f32>(1.0, 0.3, 0.03), smoothstep(0.0, 0.3, t));
    c = mix(c, vec3<f32>(1.0, 0.62, 0.16), smoothstep(0.25, 0.6, t));
    return mix(c, vec3<f32>(1.0, 0.9, 0.62), smoothstep(0.6, 0.95, t));
}

// One tongue of fire at `p` (x across in its widths, y up its height), licked by noise rising
// through it at `speed` and swaying `sway` at its tip: its density and heat.
fn tongue(p: vec2<f32>, seed: u32, speed: f32, sway: f32) -> vec2<f32> {
    let t = frame.sun_light.w;
    let s = f32(seed & 1023u) * 0.37;
    let n1 = fire_noise(vec2<f32>(p.x * 2.2 + s, p.y * 3.0 - t * speed), seed);
    let n2 = fire_noise(vec2<f32>(p.x * 5.0 - s, p.y * 7.0 - t * speed * 1.9), seed + 7u);
    let turb = n1 * 0.65 + n2 * 0.35 - 0.5;
    let x = p.x - (sway + turb * 0.9) * p.y * p.y;
    // A teardrop: round at its foot, drawn to a point.
    let yc = clamp(p.y, 0.0, 1.0);
    let w = sqrt(clamp((p.y + 0.08) / 0.3, 0.0, 1.0)) * pow(1.0 - yc, 0.75);
    let d = abs(x) / max(w, 1e-3);
    let shape = 1.0 - smoothstep(0.55, 1.0, d - turb * 0.5 * yc);
    let tip = 1.0 - smoothstep(0.55, 1.0, p.y + turb * 0.6);
    let foot = smoothstep(-0.12, 0.04, p.y);
    let dense = clamp(shape * tip * foot, 0.0, 1.0);
    return vec2<f32>(dense, dense * (1.0 - 0.75 * yc) * (1.0 - 0.6 * clamp(d, 0.0, 1.0)));
}

// A flame's light at `p` (x across, its width one; y up, its height one): rgb, in multiples of
// white on screen before its gain.
fn flame_light(p: vec2<f32>, kind: u32, seed: u32, flick: vec4<f32>) -> vec3<f32> {
    let t = frame.sun_light.w;
    let s = f32(seed & 1023u) * 0.37;
    // It stands taller as it flares, and sways from its foot, leaning with the wind.
    let y = p.y / mix(0.8, 1.15, clamp(flick.r - 0.6, 0.0, 1.0));
    let sway = (fire_noise(vec2<f32>(t * 1.1, s), seed + 3u) - 0.5) * 0.5 + flick.a;
    var f = vec2<f32>(0.0);
    if (kind == 1u) {
        f = tongue(vec2<f32>(p.x, y), seed, 4.5, sway * 0.5);
    } else {
        // A torch: a tall tongue with lower ones licking up beside it.
        f = tongue(vec2<f32>(p.x, y), seed, 3.2, sway);
        for (var k = 0u; k < 2u; k++) {
            let side = select(-0.45, 0.45, k == 1u);
            let h = mix(0.45, 0.75, fire_noise(vec2<f32>(t * 1.7 + f32(k) * 5.0, s), seed + 13u + k));
            f = max(f, tongue(vec2<f32>((p.x - side) / 0.6, y / h), seed + 17u + k, 3.6, sway) * 0.8);
        }
    }
    var c = fire_colour(f.y * 1.15) * f.x * (0.35 + 2.6 * f.y * f.y);
    // The air round it glowing.
    let r2 = p.x * p.x * 0.25 + (p.y - 0.3) * (p.y - 0.3) * 1.6;
    let card = (1.0 - smoothstep(1.2, 2.2, abs(p.x))) * smoothstep(-0.35, -0.1, p.y)
        * (1.0 - smoothstep(1.0, 1.5, p.y));
    c += vec3<f32>(1.0, 0.45, 0.12) * FLAME_HALO * exp(-r2 * 4.0) * card;
    return c * flick.rgb;
}

fn flame_seen(in: FlameOut) -> vec4<f32> {
    if (in.kind == 4u) {
        return vec4<f32>(body_fire_light(in), 1.0);
    }
    var c = flame_light(in.at, in.kind, in.seed, in.flick) * FLAME_GAIN;
    // It fades where it meets what stands behind it, and into the fog.
    let px = vec2<i32>(in.clip.xy);
    let d = textureLoad(g_depth, px, 0);
    if (d > 0.0) {
        let ndc = vec2<f32>(in.clip.x / frame.params.z * 2.0 - 1.0, 1.0 - in.clip.y / frame.params.w * 2.0);
        let h = frame.inv_view_proj * vec4<f32>(ndc, d, 1.0);
        let behind = view_depth_at(h.xyz / h.w) - view_depth_at(in.world);
        c *= clamp(behind / max(0.3 * in.size, 0.02), 0.0, 1.0);
    }
    c *= 1.0 - fog_amount(distance(in.world.xy, frame.camera.xy));
    return vec4<f32>(c, 1.0);
}

@fragment
fn fs_flame(in: FlameOut) -> SortedOut {
    return sorted_out(flame_seen(in), true);
}

@fragment
fn fs_flame_oit(in: FlameOut) -> OitOut {
    return oit_out(flame_seen(in), true, in.world);
}
