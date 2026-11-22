// The rain on the ground (weather.rs): what a roof keeps it off, by the shelter map (the height of
// the highest static surface over each point round the eye, seen straight down; the including
// shader binds `shelter_map`), and the wet ground's soak, puddles, ripples and splashes
// (render-hifi's weather, on the engine's frame).

// The ripples' and splashes' clock wraps over this many seconds (`weather::RIPPLE_WRAP`).
const RIPPLE_WRAP: f32 = 600.0;

// The height of the highest static surface over render-space `q`, or far below anything where
// nothing stands or the map does not reach.
fn shelter_height(q: vec2<f32>) -> f32 {
    let span = frame.rain.x;
    if (span <= 0.0) {
        return -1.0e9;
    }
    let s = (q - frame.shelter.xy) / frame.shelter.z;
    if (abs(s.x) >= 1.0 || abs(s.y) >= 1.0) {
        return -1.0e9;
    }
    let side = i32(textureDimensions(shelter_map).x);
    let uv = vec2<f32>(s.x * 0.5 + 0.5, 0.5 - s.y * 0.5);
    let t = clamp(vec2<i32>(uv * f32(side)), vec2<i32>(0), vec2<i32>(side - 1));
    let d = textureLoad(shelter_map, t, 0);
    if (d >= 1.0) {
        return -1.0e9;
    }
    return frame.shelter.w - d * span;
}

// Whether something stands more than `margin` metres over `p`: 1 sheltered, 0 open.
fn sheltered(p: vec3<f32>, margin: f32) -> f32 {
    return select(0.0, 1.0, shelter_height(p.xy) > p.z + margin);
}

// The share of the ground round `p` something shelters, over a patch half a metre across whose
// middle wanders with the world's noise, so the dry ground under a roof's edge ends in a soft,
// uneven line rather than in the map's texels.
fn shelter_share(p: vec3<f32>, margin: f32) -> f32 {
    if (frame.rain.x <= 0.0) {
        return 0.0;
    }
    // Wholly open or wholly under a roof as far as the patch can wander: no edge to soften.
    var k = 0.0;
    for (var c = 0; c < 4; c++) {
        let o = vec2<f32>(f32(c & 1), f32(c >> 1u)) * 1.2 - vec2<f32>(0.6);
        k += sheltered(vec3<f32>(p.xy + o, p.z), margin);
    }
    if (k == 0.0 || k == 4.0) {
        return k * 0.25;
    }
    let wander = vec2<f32>(land_noise(p.xy, 0.77), land_noise(p.xy + vec2<f32>(37.1, 11.3), 0.77))
        - vec2<f32>(0.5);
    let centre = p.xy + wander * 0.7;
    var n = 0.0;
    for (var j = -1; j <= 1; j++) {
        for (var i = -1; i <= 1; i++) {
            let o = vec2<f32>(f32(i), f32(j)) * 0.25;
            n += sheltered(vec3<f32>(centre + o, p.z), margin);
        }
    }
    return smoothstep(0.0, 1.0, n / 9.0);
}

// How soaked the rain leaves a surface facing `n`: the ground's wetness (`jitter.z`), most where
// it faces the sky.
fn rain_soak(n: vec3<f32>) -> f32 {
    return frame.jitter.z * mix(0.3, 1.0, smoothstep(0.2, 0.8, n.z));
}

// One row of four of the land field's heights, from vertex `g` east.
fn land_row(g: vec2<i32>) -> vec4<f32> {
    return vec4<f32>(land_vertex(g).height, land_vertex(g + vec2<i32>(1, 0)).height,
                     land_vertex(g + vec2<i32>(2, 0)).height, land_vertex(g + vec2<i32>(3, 0)).height);
}

// Whether a field vertex's record `word` lies on a road, 0 or 1.
fn land_road(word: u32) -> f32 {
    return f32(((word >> LW_ROAD_SHIFT) & 3u) != 0u);
}

// How level the land lies at render-space `p` (its normal's rise, from the field's corners
// round it), and how far below the land round it (a hollow, metres; under nothing on a rise) and
// how much of it is road where it lies level enough (`least` rise) to ask.
fn land_lie(p: vec2<f32>, least: f32) -> vec3<f32> {
    let q = (p - frame.land.xy) / LAND_CELL;
    let c = vec2<i32>(floor(q));
    let f = q - floor(q);
    let sw = land_vertex(c);
    let se = land_vertex(c + vec2<i32>(1, 0));
    let nw = land_vertex(c + vec2<i32>(0, 1));
    let ne = land_vertex(c + vec2<i32>(1, 1));
    let gx = mix(se.height - sw.height, ne.height - nw.height, f.y) / LAND_CELL;
    let gy = mix(nw.height - sw.height, ne.height - se.height, f.x) / LAND_CELL;
    let rise = inverseSqrt(1.0 + gx * gx + gy * gy);
    if (rise < least) {
        return vec3<f32>(0.0, rise, 0.0);
    }
    // Each of the cell's corners against the mean of the vertices round it.
    let r0 = land_row(c + vec2<i32>(-1, -1));
    let r1 = land_row(c + vec2<i32>(-1, 0));
    let r2 = land_row(c + vec2<i32>(-1, 1));
    let r3 = land_row(c + vec2<i32>(-1, 2));
    let lo = r0 + r1 + r2;
    let hi = r1 + r2 + r3;
    let m = vec4<f32>(lo.x + lo.y + lo.z, lo.y + lo.z + lo.w, hi.x + hi.y + hi.z, hi.y + hi.z + hi.w)
        / 9.0 - vec4<f32>(sw.height, se.height, nw.height, ne.height);
    let road = vec4<f32>(land_road(sw.word), land_road(se.word), land_road(nw.word), land_road(ne.word));
    let at = vec4<f32>((1.0 - f.x) * (1.0 - f.y), f.x * (1.0 - f.y), (1.0 - f.x) * f.y, f.x * f.y);
    return vec3<f32>(dot(m, at), rise, dot(road, at));
}

// How much of the open ground at `world` (normal `n`) stands in a puddle (x), and in the damp rim
// round one (y): where a noise laid on the world dips, deeper in the land's hollows and on the
// road, only where the land lies level, and near enough to be seen as puddles (farther off a
// sheet of them reads as snow). They fill as the rain goes on (`rain.w`): the water rises through
// the noise, so each puddle grows out from its deepest point, and shrinks back as it dries.
fn puddle_at(world: vec3<f32>, n: vec3<f32>) -> vec2<f32> {
    let fill = frame.rain.w;
    let near = 1.0 - smoothstep(70.0, 150.0, distance(world, frame.camera.xyz));
    if (fill <= 0.0 || !has_land() || n.z < 0.8 || near <= 0.0) {
        return vec2<f32>(0.0);
    }
    let lip = mix(0.86, 0.66, sqrt(fill));
    let lay = land_noise(world.xy, 5.0) * 0.55 + land_noise(world.xy + vec2<f32>(13.7, 4.1), 2.1) * 0.3;
    // Not even the deepest hollow on a road brings the water up to here.
    if (lay + 0.15 + 0.22 < lip - 0.07) {
        return vec2<f32>(0.0);
    }
    let lie = land_lie(world.xy, 0.965);
    let level = smoothstep(0.965, 0.993, lie.y) * smoothstep(0.82, 0.95, n.z);
    if (level <= 0.0) {
        return vec2<f32>(0.0);
    }
    let depth = lay + land_noise(world.xy + vec2<f32>(3.3, 27.9), 0.7) * 0.15
        + 0.16 * clamp(lie.x / 1.5, -1.0, 1.0) + 0.06 * lie.z;
    return vec2<f32>(smoothstep(lip, lip + 0.02, depth), smoothstep(lip - 0.07, lip, depth))
        * level * near;
}

// A world lattice point's two hashes, 0..1.
fn rain_hash2(i: vec2<i32>, salt: i32) -> vec2<f32> {
    return vec2<f32>(land_hash(i + vec2<i32>(salt * 7919, salt * 131)),
                     land_hash(i.yx + vec2<i32>(salt * 31, 977 - salt)));
}

// A rate, cycles a second, nudged to a whole number of cycles over the clock's wrap.
fn whole_rate(rate: f32) -> f32 {
    return max(round(rate * RIPPLE_WRAP), 1.0) / RIPPLE_WRAP;
}

// The rings of the drops landing in a puddle at render-space `p`: a slope to bend its level by.
fn rain_ripples(p: vec2<f32>) -> vec2<f32> {
    let t = frame.rain.y;
    let cell = 0.45;
    let o = origin_cells(cell);
    let s = p / cell + o.zw;
    // A ring spreads no farther than 0.6 of a cell from its drop: the four cells nearest hold it.
    let base = floor(s - vec2<f32>(0.5));
    var g = vec2<f32>(0.0);
    for (var k = 0; k < 4; k++) {
        let c = base + vec2<f32>(f32(k & 1), f32(k >> 1u));
        let r = rain_hash2(vec2<i32>(c) + vec2<i32>(o.xy), 11);
        let phase = fract(t * whole_rate(1.4 + r.x * 0.8) + r.y);
        let d = s - (c + vec2<f32>(0.2) + r * 0.6);
        let len = length(d);
        let x = len - phase * 0.6;
        let ring = sin(x * 30.0) * exp(-x * x * 120.0) * (1.0 - phase) * (1.0 - phase);
        if (len > 1e-4) {
            g += d / len * ring;
        }
    }
    return g * 0.1;
}

// The splashes where drops land on open ground at render-space `p`, a pixel `footprint` metres
// across: how bright, 0..1.
fn rain_splashes(p: vec2<f32>, footprint: f32) -> f32 {
    let t = frame.rain.y;
    let size = 0.2;
    let o = origin_cells(size);
    let s = p / size + o.zw;
    // A splash reaches a quarter of a cell from its drop: the four cells nearest hold it.
    let base = floor(s - vec2<f32>(0.5));
    var sum = 0.0;
    for (var k = 0; k < 4; k++) {
        let c = base + vec2<f32>(f32(k & 1), f32(k >> 1u));
        let r = rain_hash2(vec2<i32>(c) + vec2<i32>(o.xy), 23);
        let phase = fract(t * whole_rate(1.7 + r.y) + r.x * 7.0);
        let life = 0.16;
        if (phase < life) {
            let a = phase / life;
            let d = length(s - (c + vec2<f32>(0.15) + r * 0.7)) * size;
            // A crown that widens and fades, and the drop's bright landing point.
            let radius = 0.006 + a * 0.035;
            let width = max(0.004, footprint * 0.7);
            let ring = exp(-pow((d - radius) / width, 2.0)) * (1.0 - a);
            let landing = exp(-pow(d / max(0.006, footprint), 2.0)) * (1.0 - a) * (1.0 - a);
            // Drops smaller than a pixel give their light to the pixel as a whole.
            let cover = clamp(0.03 / max(footprint, 0.001), 0.0, 1.0);
            sum += (ring * 0.5 + landing * 0.6) * cover;
        }
    }
    return clamp(sum, 0.0, 1.0);
}

// The level a puddle at `p` lies at, `rings` of the way bent by the drops' rings.
fn puddle_normal(p: vec2<f32>, rings: f32) -> vec3<f32> {
    if (rings <= 0.0) {
        return vec3<f32>(0.0, 0.0, 1.0);
    }
    return normalize(vec3<f32>(-rain_ripples(p) * rings, 1.0));
}

// How much of what surrounds it a puddle `dist` metres off seen along `v` (toward the eye, over
// normal `n`) mirrors: the water's Fresnel, held up a little looking down so a puddle never goes
// dull, and held down farther off and at a glance, where the overcast would turn it into a white
// sheet.
fn puddle_mirror(n: vec3<f32>, v: vec3<f32>, dist: f32) -> f32 {
    let cosv = clamp(dot(n, v), 0.0, 1.0);
    let f = 0.02 + 0.98 * pow(1.0 - cosv, 5.0);
    let hold = mix(0.8, 0.35, smoothstep(8.0, 40.0, dist)) * mix(0.6, 1.0, smoothstep(0.03, 0.25, cosv));
    return clamp(f * 1.2 + 0.12, 0.0, 1.0) * hold;
}

// How far the drops' rings reach on a puddle `dist` metres off, as hard as it rains.
fn puddle_rings(dist: f32) -> f32 {
    return (1.0 - smoothstep(6.0, 18.0, dist)) * frame.rain.z;
}

// Whether a roof stands over the ray from `p` along `r` within a few metres: under an eave a
// puddle mirrors the shade beneath it.
fn roofed_ray(p: vec3<f32>, r: vec3<f32>) -> f32 {
    var roofed = 0.0;
    for (var k = 0; k < 4; k++) {
        roofed = max(roofed, sheltered(p + r * (0.75 * exp2(f32(k))), 0.1));
    }
    return roofed;
}

// How snowy the land is at render-space `p`, 0 to 1: its snowy terrain types (`frame.snow.x`) at
// the field's vertices round it, blended across the cell, the line between wandering with the
// world's noise.
fn land_snowy(p: vec2<f32>) -> f32 {
    return land_typed(p, bitcast<u32>(frame.snow.x));
}

// How much of the land at render-space `p` is of the terrain types `mask` (one bit a type),
// 0 to 1, blended as [`land_snowy`] blends. Past the land field it reads the far field, whose
// clamped edge would otherwise run on to the horizon.
fn land_typed(p: vec2<f32>, mask: u32) -> f32 {
    if (mask == 0u) {
        return 0.0;
    }
    var q = (p - frame.land.xy) / LAND_CELL;
    let far = !has_land() || any(q < vec2<f32>(0.0)) || any(q >= frame.land.zw - 1.0);
    if (far) {
        if (!far_heights_on()) {
            return 0.0;
        }
        q = (p - far_rings.origin) / LAND_CELL;
    }
    let c = vec2<i32>(floor(q));
    let f = q - floor(q);
    let s = vec4<f32>(
        f32((mask >> (water_vertex(c, far).word & LW_TYPE)) & 1u),
        f32((mask >> (water_vertex(c + vec2<i32>(1, 0), far).word & LW_TYPE)) & 1u),
        f32((mask >> (water_vertex(c + vec2<i32>(0, 1), far).word & LW_TYPE)) & 1u),
        f32((mask >> (water_vertex(c + vec2<i32>(1, 1), far).word & LW_TYPE)) & 1u));
    if (all(s == vec4<f32>(0.0)) || all(s == vec4<f32>(1.0))) {
        return s.x;
    }
    let u = f * f * (3.0 - 2.0 * f);
    let b = mix(mix(s.x, s.y, u.x), mix(s.z, s.w, u.x), u.y);
    let wander = (land_noise(p, 7.0) - 0.5) * 0.5 + (land_noise(p + vec2<f32>(17.3, 5.1), 2.3) - 0.5) * 0.2;
    return smoothstep(0.35, 0.65, b + wander);
}
