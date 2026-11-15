// The lamps' light on a lit point: its cluster's lamps, each an inverse-square light windowed to
// its reach (the client's: its colour over the distance, cut at its range), flickering with the
// game's clock, and shadowed where a ray-traced shadow slot says.

// The ambient a room sealed from the sky takes in retail's sum, in its colour space.
const ROOM_AMBIENT: f32 = 0.52;

@group(3) @binding(2) var<storage, read> lamps: array<Lamp>;
@group(3) @binding(3) var<storage, read> cluster_counts: array<u32>;
@group(3) @binding(4) var<storage, read> cluster_lamps: array<u32>;
// The nearest shadowed lamps' visibility, one channel a slot.
@group(3) @binding(5) var lamp_shadow: texture_2d<f32>;

struct LampLight {
    // Irradiance (scene units), or the client's light to sum in its colour space.
    diffuse: vec3<f32>,
    specular: vec3<f32>,
    // How a lamp's own glass and flame glow round it: its colour, on screen over the surface's.
    glow: vec3<f32>,
}

// How brightly a lamp's glowing part shows, in multiples of its surface's colour on screen.
const LAMP_GLOW: f32 = 1.5;

// Gradient noise at `t`, about -1 to 1, for a flame's `seed` and `channel` (render-hifi's flicker).
fn flame_noise(seed: u32, channel: u32, t: f32) -> f32 {
    let at = floor(t);
    let f = t - at;
    let i = u32(i32(at));
    var h0 = i * 0x9E3779B1u + seed + channel * 0x85EBCA77u;
    var h1 = (i + 1u) * 0x9E3779B1u + seed + channel * 0x85EBCA77u;
    h0 = (h0 ^ (h0 >> 15u)) * 0xC2B2AE3Du;
    h0 = (h0 ^ (h0 >> 13u)) * 0x27D4EB2Fu;
    h0 ^= h0 >> 16u;
    h1 = (h1 ^ (h1 >> 15u)) * 0xC2B2AE3Du;
    h1 = (h1 ^ (h1 >> 13u)) * 0x27D4EB2Fu;
    h1 ^= h1 >> 16u;
    let a = (f32(h0) / 4294967295.0 * 4.0 - 2.0) * f;
    let b = (f32(h1) / 4294967295.0 * 4.0 - 2.0) * (f - 1.0);
    let s = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    return clamp(a + (b - a) * s, -1.0, 1.0);
}

// What a flickering lamp's colour is multiplied by now (render-hifi's Flame Flicker): a flame's
// band-limited dance, faster the smaller it is, ducking in gusts; embers' slow breath. Less light
// is redder light.
fn flicker(kind: u32, seed: u32, range: f32) -> vec3<f32> {
    if (kind == 0u) {
        return vec3<f32>(1.0);
    }
    let t = frame.sun_light.w;
    var k = 1.0;
    if (kind == 1u || kind == 3u) {
        // A flame about a twentieth as wide as its light reaches; a creature of fire's (3) twice
        // as quick and deeper, ducking harder and oftener, as a candle flutters.
        let lively = kind == 3u;
        let size = clamp(range / 1.5 * 0.05, 0.03, 2.0);
        let pulse = clamp(select(1.5, 3.0, lively) / sqrt(size), select(0.8, 2.0, lively), select(4.0, 8.0, lively));
        let wander = flame_noise(seed, 0u, t * pulse) * 0.57 + flame_noise(seed, 1u, t * pulse * 2.13) * 0.29
            + flame_noise(seed, 2u, t * pulse * 4.37) * 0.14;
        let gust = max(flame_noise(seed, 3u, t * select(0.31, 0.9, lively)) - 0.25, 0.0) / 0.75;
        k = 1.0 + select(0.32, 0.5, lively) * wander - select(0.14, 0.3, lively) * gust * gust;
    } else {
        k = 1.0 + 0.24 * flame_noise(seed, 7u, t * 0.35) + 0.08 * flame_noise(seed, 8u, t * 0.9);
    }
    let d = k - 1.0;
    return k * vec3<f32>(1.0, max(1.0 + 0.35 * d, 0.0), max(1.0 + 0.8 * d, 0.0));
}

// The cluster holding target pixel `px` at view distance `depth`, or none past the last slice.
fn cluster_of(px: vec2<f32>, depth: f32) -> u32 {
    let n = frame.cluster_z.x;
    let f = frame.cluster_z.y;
    if (depth >= f || frame.cluster.w == 0u) {
        return 0xFFFFFFFFu;
    }
    let s = u32(clamp(floor(log(max(depth, n) / n) / log(f / n) * f32(frame.cluster.z)), 0.0,
                      f32(frame.cluster.z - 1u)));
    let tile = vec2<u32>(px / f32(frame.lists.y));
    let t = min(tile, frame.cluster.xy - 1u);
    return t.x + t.y * frame.cluster.x + s * frame.cluster.x * frame.cluster.y;
}

// `traced`: the point is the G-buffer's at `px`, so the ray-traced shadows there are its own
// (a translucent surface's are not).
fn lamp_light(world: vec3<f32>, n: vec3<f32>, v: vec3<f32>, rough: f32, f0: vec3<f32>,
              bits: u32, depth: f32, px: vec2<f32>, traced: bool) -> LampLight {
    var out: LampLight;
    out.diffuse = vec3<f32>(0.0);
    out.specular = vec3<f32>(0.0);
    out.glow = vec3<f32>(0.0);
    let cluster = cluster_of(px, depth);
    if (cluster == 0xFFFFFFFFu) {
        return out;
    }
    let indoor = (bits & G_INDOOR) != 0u;
    let foliage = (bits & G_FOLIAGE) != 0u;
    let count = cluster_counts[cluster];
    var shadow = vec4<f32>(1.0);
    if (traced && has(F_LAMP_RAYS)) {
        // The half-size shadows, a tent over the four texels round the pixel.
        let h = px * 0.5 - 0.5;
        let b = vec2<i32>(floor(h));
        let f = h - floor(h);
        let last = vec2<i32>(textureDimensions(lamp_shadow)) - 1;
        // Each texel counts by its nearness and by how like the pixel's its depth is, so no
        // shadow (or light) crosses an edge.
        var sum = vec4<f32>(0.0);
        var weight = 0.0;
        let here = textureLoad(g_depth, vec2<i32>(px), 0);
        for (var k = 0; k < 4; k++) {
            let o = vec2<i32>(k & 1, k >> 1u);
            let t = clamp(b + o, vec2<i32>(0), last);
            let dt = textureLoad(g_depth, min(t * 2, vec2<i32>(frame.params.zw) - 1), 0);
            let near = select(1.0 - f.x, f.x, o.x == 1) * select(1.0 - f.y, f.y, o.y == 1);
            let like = exp(-abs(dt - here) / max(here * 0.02, 1e-7));
            let w = near * like + 1e-4;
            sum += textureLoad(lamp_shadow, t, 0) * w;
            weight += w;
        }
        shadow = sum / weight;
    }
    // Retail's sum of a room's lamps (each its colour over the distance), as the physical light's
    // floor indoors.
    var summed = vec3<f32>(0.0);
    // Magic lights' share of it, whitened by retail's clamp rather than held to their hue.
    var magic_summed = vec3<f32>(0.0);
    // Outdoors, the same sum of the client's lamp light (its colour over the distance).
    var outdoors = vec3<f32>(0.0);
    for (var k = 0u; k < count; k++) {
        let l = lamps[cluster_lamps[cluster * CLUSTER_MOST + k]];
        // A lamp in a room lights the rooms; one outdoors, the outdoors.
        if (((l.flags & 256u) != 0u) != indoor) {
            continue;
        }
        let to = l.position - world;
        let d2 = max(dot(to, to), 1e-4);
        let d = sqrt(d2);
        if (d >= l.range) {
            continue;
        }
        let dir = to / d;
        let ndl = dot(n, dir);
        var lit = max(ndl, 0.0);
        if (foliage) {
            lit = mix(lit, abs(ndl), 0.7);
        }
        // A volume takes a lamp from every side, and neither magic light nor one within it (a
        // creature's own).
        let volume = (bits & (G_VOLUME | G_VOLUME_LIT)) != 0u;
        if ((bits & G_VOLUME) != 0u) {
            if (magic(l)) {
                continue;
            }
            lit = smoothstep(0.5, 2.0, d);
        } else if (volume) {
            lit = 1.0;
        }
        let flame = flicker(l.flags & 255u, l.slot >> 8u, l.range);
        var vis = 1.0;
        let slot = l.slot & 255u;
        if (slot < 4u) {
            vis *= shadow[slot];
        }
        if (has(F_PHYSICAL) && !indoor) {
            // A blade of grass takes it as the ground under it does, with no gain of its own.
            let face = select(n, vec3<f32>(0.0, 0.0, 1.0), foliage);
            let took = select(clamp((dot(face, dir) + 0.1) / 1.1, 0.0, 1.0), lit, volume);
            // Grass over the ground takes no more of it than the ground's darker face would.
            let share = select(1.0, 0.45, foliage);
            outdoors += outdoor_lamp(l, d) * flame * took * vis * share;
            // Its glass and flame, a hand round it (render-hifi's glowing part).
            let part = select(0.5, 0.35, (l.flags & 2048u) != 0u) * select(1.0, 0.0, (l.flags & 255u) == 2u);
            if (d < part) {
                let g = 1.0 - d / part;
                out.glow = max(out.glow, outdoor_colour(l) * flame * g * g * (3.0 - 2.0 * g));
            }
        } else if (has(F_PHYSICAL)) {
            // Its colour in retail's colour space, its brightest channel one.
            let top = max(max(l.retail.r, max(l.retail.g, l.retail.b)), 1e-4);
            let tint = pow(l.retail.rgb / top, vec3<f32>(1.0 / 2.2));
            let e = room_lamp(l, d2) * flame * vis;
            {
                // The client's burn of a cell's lights: wrapped half way round, over the square
                // of the distance past a metre, to 1.3 of its falloff, each no more than its colour.
                let wrapped = select(max(ndl + 0.5, 0.0) * 0.6666667, lit, volume);
                let reach = max(1.0 - d / (l.range * (1.3 / 1.5)), 0.0);
                let strength = select(top, min(top, MAGIC_INTENSITY), magic(l));
                let one = min(tint * strength * reach * wrapped / max(d2, 1.0), tint) * flame;
                if (magic(l)) {
                    magic_summed += one;
                } else {
                    summed += one;
                }
            }
            out.diffuse += e * lit;
            if (ndl > 0.0) {
                out.specular += ggx(n, v, dir, rough, f0) * ndl * e;
            }
        } else {
            // The client's: its colour over the distance (inverse beyond a metre, as Direct3D's
            // attenuation of 0, 1, 0 is), to its range.
            out.diffuse += l.retail * flame * (lit / max(d, 1.0)) * (1.0 - smoothstep(0.85, 1.0, d / l.range)) * vis;
        }
    }
    if (!indoor && has(F_PHYSICAL)) {
        out.diffuse += outdoors * outdoor_lamp_scale();
    }
    if (indoor && has(F_PHYSICAL)) {
        // Retail sums a room's ambient and its lamps in its own colour space and clamps: the light
        // between the pools is far more than an inverse square leaves. That sum, made linear at
        // the room's scale, is the least a room's point takes. Held under one by its brightest
        // channel, so a lamp's pool keeps its warmth rather than whitening.
        let scale = room_whole();
        // A sealed room's ambient as retail's dungeons read, not the client's bare 0.2.
        let ambient = select(frame.indoor.rgb, max(frame.indoor.rgb, vec3<f32>(ROOM_AMBIENT)), frame.indoor.w > 0.5);
        let sum = ambient + summed;
        let held = min(sum / max(max(sum.r, max(sum.g, sum.b)), 1.0) + magic_summed, vec3<f32>(1.0));
        let implied = pow(held, vec3<f32>(2.2)) * scale;
        out.diffuse = max(out.diffuse, implied - frame.indoor_light.rgb);
    }
    return out;
}
