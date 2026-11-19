// Flames where objects burn (`flames::frame_flames`): a card per flame turned to the eye about
// the upright, its fire scrolling noise shaped by its kind (a candle's teardrop, a torch's
// licking tongue) under a temperature ramp, white-yellow at the core to red at the tips, drawn
// additively bright enough to bloom. It breathes with its lamp's flicker and leans with the wind
// outdoors. A brazier's bed of fire is drawn as its fire puffs (`particles.wgsl`'s `fire_seen`), a
// hearth's as a body of fire (`body_fire.wgsl`).

struct Flame {
    base: vec3<f32>,
    height: f32,
    // Bits 0-7 the kind (1 candle, 2 torch, 4 a body of fire, 5 one of its points, 6 a
    // length of its trail, 7 a hearth's body of fire, 8 a wreath's); bit 8 in a room; bit 9
    // flickering; bits 10-15 a body's whirl, a hearth's lamp's reach in metres; bits 16-31 its
    // count of points.
    flags: u32,
    seed: u32,
    reach: f32,
    // A body's colour, linear bytes; a point's tip width, its bits (`body_fire.wgsl`); a
    // coloured torch's colour, linear bytes, nought for fire's own.
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
    // A coloured torch's colour, linear bytes; nought for fire's own.
    @location(7) @interpolate(flat) colour: u32,
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
        if (kind == 4u || kind == 8u) {
            flick = flicker(3u, f.seed, f.reach * 30.0);
        } else if (kind == 7u) {
            // A hearth's, as the lamp of its fire.
            flick = flicker(1u, f.seed, f32((f.flags >> 10u) & 63u));
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
    if (kind == 4u || kind == 7u || kind == 8u) {
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
    out.colour = f.colour;
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

// A torch's flame of another colour at temperature `t`: deeper at its edges, its own colour, then
// a pale core, nearly as bright as fire at that heat.
fn tinted_fire(t: f32, tint: vec3<f32>) -> vec3<f32> {
    let deep = tint * tint;
    var c = mix(deep * 0.3, deep, smoothstep(0.0, 0.3, t));
    c = mix(c, tint, smoothstep(0.25, 0.6, t));
    c = mix(c, mix(tint, vec3<f32>(1.0), 0.45), smoothstep(0.6, 0.95, t));
    let luma = vec3<f32>(0.2126, 0.7152, 0.0722);
    return c * 0.8 * sqrt(dot(fire_colour(t), luma) / max(dot(c, luma), 1e-3));
}

// The linear colour in a flame's bytes, red lowest.
fn flame_tint(colour: u32) -> vec3<f32> {
    return vec3<f32>(f32(colour & 255u), f32((colour >> 8u) & 255u), f32((colour >> 16u) & 255u)) / 255.0;
}

// How ragged a torch's tongues are (a candle's one), how bright its heart, and how many sparks
// it throws.
const TORCH_ROUGH: f32 = 2.0;
const TORCH_CORE: f32 = 0.35;
const TORCH_SPARKS: u32 = 8u;

// The sparks a torch throws at `p` (as `flame_light`'s): motes rising from its flame and swaying
// as they cool, each life a new path, about half of them lit at once.
fn torch_sparks(p: vec2<f32>, seed: u32) -> f32 {
    let t = frame.sun_light.w;
    var light = 0.0;
    for (var k = 0u; k < TORCH_SPARKS; k++) {
        let kk = f32(k);
        let period = 0.8 + 0.7 * fire_hash(vec2<f32>(kk, 3.0), seed);
        let age = t / period + fire_hash(vec2<f32>(kk, 7.0), seed);
        let life = fract(age);
        let born = floor(age);
        let h1 = fire_hash(vec2<f32>(kk, born), seed + 29u);
        let h2 = fire_hash(vec2<f32>(born, kk), seed + 31u);
        if (h2 < 0.45) {
            continue;
        }
        let x = (h1 - 0.5) * 0.7 + (h2 - 0.72) * 2.4 * life + 0.15 * sin(life * 8.0 + h1 * 6.3);
        let y = 0.3 + life * 0.95;
        // Across in heights (a flame width is 0.22 of its height), a mote some 3% of it wide.
        let q = vec2<f32>((p.x - x) * 0.22, p.y - y) / 0.015;
        light += exp(-dot(q, q)) * pow(1.0 - life, 1.5) * smoothstep(0.0, 0.08, life) * 0.8;
    }
    return light;
}

// One tongue of fire at `p` (x across in its widths, y up its height), licked by noise rising
// through it at `speed`, `rough` as ragged, and swaying `sway` at its tip: its density and heat.
fn tongue(p: vec2<f32>, seed: u32, speed: f32, sway: f32, rough: f32) -> vec2<f32> {
    let t = frame.sun_light.w;
    let s = f32(seed & 1023u) * 0.37;
    let n1 = fire_noise(vec2<f32>(p.x * 2.2 + s, p.y * 3.0 - t * speed), seed);
    let n2 = fire_noise(vec2<f32>(p.x * 5.0 - s, p.y * 7.0 - t * speed * 1.9), seed + 7u);
    let turb = (n1 * 0.65 + n2 * 0.35 - 0.5) * rough;
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
fn flame_light(p: vec2<f32>, kind: u32, seed: u32, flick: vec4<f32>, colour: u32) -> vec3<f32> {
    let t = frame.sun_light.w;
    let s = f32(seed & 1023u) * 0.37;
    // It stands taller as it flares, and sways from its foot, leaning with the wind.
    let y = p.y / mix(0.8, 1.15, clamp(flick.r - 0.6, 0.0, 1.0));
    let sway = (fire_noise(vec2<f32>(t * 1.1, s), seed + 3u) - 0.5) * 0.5 + flick.a;
    var f = vec2<f32>(0.0);
    if (kind == 1u) {
        f = tongue(vec2<f32>(p.x, y), seed, 4.5, sway * 0.5, 1.0);
    } else {
        // A torch: a tall ragged tongue with lower ones licking up beside it, one wandering.
        f = tongue(vec2<f32>(p.x, y), seed, 3.8, sway, TORCH_ROUGH);
        for (var k = 0u; k < 3u; k++) {
            let wander = (fire_noise(vec2<f32>(t * 0.9, s + 11.0), seed + 19u) - 0.5) * 0.9;
            let side = select(select(-0.7, 0.7, k == 1u), wander, k == 2u);
            let h = mix(0.35, 0.85, fire_noise(vec2<f32>(t * 2.4 + f32(k) * 5.0, s), seed + 13u + k));
            f = max(f, tongue(vec2<f32>((p.x - side) / 0.6, y / h), seed + 17u + k, 4.4, sway * 1.3,
                              TORCH_ROUGH) * 0.85);
        }
    }
    // A torch's fire is redder than a candle's.
    var heat = fire_colour(f.y * select(1.15, 1.0, kind == 2u));
    var halo = vec3<f32>(1.0, 0.45, 0.12);
    if (colour != 0u) {
        let tint = flame_tint(colour);
        heat = tinted_fire(f.y * 1.15, tint);
        halo = tint * 0.6;
    }
    var c = heat * f.x * (0.35 + 2.6 * f.y * f.y);
    if (kind == 2u) {
        // A torch's white-hot heart low in its flame, and sparks thrown up from it.
        let core = exp(-(p.x * p.x * 3.0 + (y - 0.16) * (y - 0.16) * 40.0));
        var hot = vec3<f32>(1.0, 0.85, 0.55);
        var ember = vec3<f32>(1.0, 0.5, 0.12);
        if (colour != 0u) {
            let tint = flame_tint(colour);
            hot = mix(tint, vec3<f32>(1.0), 0.6);
            ember = mix(tint, vec3<f32>(1.0), 0.35);
        }
        c += hot * core * TORCH_CORE * f.x;
        c += ember * torch_sparks(p, seed);
    }
    // The air round it glowing.
    let r2 = p.x * p.x * 0.25 + (p.y - 0.3) * (p.y - 0.3) * 1.6;
    let card = (1.0 - smoothstep(1.2, 2.2, abs(p.x))) * smoothstep(-0.35, -0.1, p.y)
        * (1.0 - smoothstep(1.0, 1.5, p.y));
    c += halo * FLAME_HALO * exp(-r2 * 4.0) * card;
    return c * flick.rgb;
}

fn flame_seen(in: FlameOut) -> vec4<f32> {
    if (in.kind == 4u || in.kind == 7u || in.kind == 8u) {
        return vec4<f32>(body_fire_light(in), 1.0);
    }
    var c = flame_light(in.at, in.kind, in.seed, in.flick, in.colour) * FLAME_GAIN;
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
