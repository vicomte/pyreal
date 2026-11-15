// Ground cover placed on the GPU each frame: one invocation per point of each
// kind's grid near the eye, a workgroup to a tile of its points, a tile out of reach or out of
// view passed over whole. The grid is laid on the world's own metres and every choice (where in
// its square a plant stands, whether it grows, its size and turn) is a hash of the kind and the
// grid point, so the same plants stand in the same places on every visit, whatever the eye. A
// plant grows where the terrain types about it are the kind's (as a share across the cell, the
// edge wandering with the hash), off roads, buildings, objects and water, thinning on slopes
// toward the steepest its kind allows, in bare patches and (the sparse kinds) drifts by the
// world's noise, and toward a road as the ground draws it; it grows in by size as the share kept
// passes its number, taller in patches. Its level comes from its distance over its scale (three
// of shape, then a card); over the cards' last quarter it shrinks away. Within a band about a
// level's end it is drawn at both levels, each through the other's screen door. The survivors
// append to their level's list, and a second pass writes each draw's instance count.

// The band before a level's end it is crossfaded into the next over, a share of the end
// (cover.rs `BAND`).
const COVER_BAND: f32 = 0.25;
// Samples a side of a cell's road field (cover.rs `ROAD_SIDE`).
const ROAD_SIDE: u32 = 25u;
// Metres from the road's edge over which the cover grows back to its full.
const ROAD_FADE: f32 = 3.0;

struct CoverParams {
    planes: array<vec4<f32>, 6>,
    // xyz: the eye; w: how tall the cover grows, 1 the pack's own (the kinds' reach and ends are
    // sized to the settings).
    eye: vec4<f32>,
    // xy: where render space's origin lies in the world, metres; z: the draws; w: the kinds.
    world: vec4<f32>,
    // The grass under it (blades.wgsl): x how far its near clumps reach, y its far, metres; z and
    // w their candidates a square metre.
    blades: vec4<f32>,
    // The room for its near clumps, where the far start, the room for them; w the grass's tiles
    // across.
    rooms: vec4<u32>,
    // The flowers and stones among it (scatter.wgsl): x how far the flowers reach, y the stones,
    // metres; z and w their candidates a square metre.
    scatter: vec4<f32>,
    // Where the flowers start in the clumps' list, the room for them and for the stones; w their
    // tiles across.
    scatter_rooms: vec4<u32>,
    // x: the terrain types a pack keeps the pebbles off, a bit each; y: 1 where it keeps them off
    // the road's edges.
    bare: vec4<u32>,
}

@group(1) @binding(0) var<uniform> cp: CoverParams;
@group(1) @binding(1) var<storage, read> cover_kinds: array<CoverKind>;
@group(1) @binding(2) var<storage, read_write> counts: array<atomic<u32>>;
@group(1) @binding(3) var<storage, read_write> plants: array<CoverPlant>;
// Each draw's counter and its indirect arguments.
@group(1) @binding(4) var<storage, read> draw_counters: array<u32>;
@group(1) @binding(5) var<storage, read_write> draw_args: array<u32>;
// The loaded buildings' footprints (where each stands and its x axis; its walls' box in its own
// frame), then each land cell's first and count of the footprints whose clearing reaches it.
@group(1) @binding(6) var<storage, read> footprints: array<vec4<f32>>;
@group(1) @binding(7) var<storage, read> footprint_cells: array<vec2<u32>>;
@group(1) @binding(8) var<storage, read> footprint_list: array<u32>;
// Landscape Smoothing's offset at the drawn ground's vertices (land.rs `drawn`), `DRAWN_SUB` a
// cell (drawn.rs `SUB`); a lone nought without it.
@group(1) @binding(9) var<storage, read> drawn_offsets: array<f32>;

// Each set of road corners' (SW 1, SE 2, NE 4, NW 8) distance from the edge of the road drawn on
// the cell, metres, negative on it (cover.rs `road_fields`): `ROAD_SIDE` a side, east then north.
@group(1) @binding(10) var<storage, read> road_fields: array<f32>;

// What the objects standing on the land clear of it (cover.rs `clearing_words`): the land's
// vertices across and along, where the lists and the discs start, each vertex's cell's first and
// count, the lists, then each disc's x, y and radius.
@group(1) @binding(15) var<storage, read> cover_clearings: array<u32>;

const DRAWN_SUB: i32 = 12;

// The drawn ground's offset at render-space `p`, on its triangles (cut as their cell is `cut`).
fn drawn_offset(p: vec2<f32>, cut: bool) -> f32 {
    if (arrayLength(&drawn_offsets) < 4u) {
        return 0.0;
    }
    let side = (vec2<i32>(frame.land.zw) - 1) * DRAWN_SUB + 1;
    let q = (p - frame.land.xy) / (LAND_CELL / f32(DRAWN_SUB));
    let c = vec2<i32>(floor(q));
    if (any(c < vec2<i32>(0)) || any(c >= side - 1)) {
        return 0.0;
    }
    let k = u32(c.y * side.x + c.x);
    let w = u32(side.x);
    let d = vec4<f32>(drawn_offsets[k], drawn_offsets[k + 1u], drawn_offsets[k + w], drawn_offsets[k + w + 1u]);
    return on_triangles(cut, d, q - floor(q));
}

// How far `p` lies outside the nearest building's walls about land cell `c`, metres; far when
// none is near.
fn building_gap(c: vec2<i32>, p: vec2<f32>) -> f32 {
    let r = footprint_cells[u32(c.y) * u32(frame.land.z) + u32(c.x)];
    var best = 1.0e9;
    for (var k = 0u; k < min(r.y, 16u); k++) {
        let i = footprint_list[r.x + k];
        let at = footprints[i * 2u];
        let b = footprints[i * 2u + 1u];
        let d = p - at.xy;
        let l = vec2<f32>(dot(d, at.zw), dot(d, vec2<f32>(-at.w, at.z)));
        best = min(best, length(max(max(b.xy - l, l - b.zw), vec2<f32>(0.0))));
    }
    return best;
}

// The share of cover kept at render-space `p` in land cell `c` by the objects standing there:
// none a hand inside an object's edge, all a little past it, the line wandering a little.
fn object_keep(c: vec2<i32>, p: vec2<f32>) -> f32 {
    let w = u32(frame.land.z);
    if (arrayLength(&cover_clearings) < 4u || cover_clearings[0] != w
        || cover_clearings[1] != u32(frame.land.w)) {
        return 1.0;
    }
    let r = 4u + 2u * (u32(c.y) * w + u32(c.x));
    let n = min(cover_clearings[r + 1u], 16u);
    if (n == 0u) {
        return 1.0;
    }
    let first = cover_clearings[2] + cover_clearings[r];
    let wander = (land_noise(p + vec2<f32>(23.9, 7.1), 0.8) - 0.5) * 0.3;
    var keep = 1.0;
    for (var k = 0u; k < n; k++) {
        let at = cover_clearings[3] + cover_clearings[first + k] * 3u;
        let d = bitcast<vec3<f32>>(vec3<u32>(cover_clearings[at], cover_clearings[at + 1u],
                                             cover_clearings[at + 2u]));
        keep = min(keep, smoothstep(-0.3, 0.15, distance(p, d.xy) - d.z + wander));
    }
    return keep;
}

fn cover_hash(p: vec2<i32>, seed: u32, what: u32) -> f32 {
    var h = (bitcast<u32>(p.x) * 0x8DA6B343u) ^ (bitcast<u32>(p.y) * 0xD8163841u)
        ^ (seed * 0xCB1AB31Fu) ^ (what * 0x9E3779B9u);
    h = (h ^ (h >> 15u)) * 0x2C1B3C6Du;
    h = (h ^ (h >> 12u)) * 0x297A2D39u;
    h = h ^ (h >> 15u);
    return f32(h >> 8u) / 16777216.0;
}

// Of what a kind asks, the share it grows on sand (yellow, grey, rock-strewn): the desert is
// mostly bare.
const SAND_SHARE: f32 = 0.05;

fn grows_on(k: CoverKind, word: u32) -> f32 {
    let t = word & LW_TYPE;
    let on = f32((k.grounds >> t) & 1u);
    return select(on, on * SAND_SHARE, t >= 10u && t <= 12u);
}

fn on_road(word: u32) -> u32 {
    return u32(((word >> LW_ROAD_SHIFT) & 3u) != 0u);
}

// How far `f` (a share across its cell) lies from the edge of the road drawn on the cell, metres,
// negative on it, its corners' road bits `roads`.
fn road_gap(roads: u32, f: vec2<f32>) -> f32 {
    if (roads == 0u || arrayLength(&road_fields) < 16u * ROAD_SIDE * ROAD_SIDE) {
        return 1.0e9;
    }
    let q = clamp(f, vec2<f32>(0.0), vec2<f32>(1.0)) * f32(ROAD_SIDE - 1u);
    let i = min(vec2<u32>(q), vec2<u32>(ROAD_SIDE - 2u));
    let t = q - vec2<f32>(i);
    let k = (roads * ROAD_SIDE + i.y) * ROAD_SIDE + i.x;
    return mix(mix(road_fields[k], road_fields[k + 1u], t.x),
               mix(road_fields[k + ROAD_SIDE], road_fields[k + ROAD_SIDE + 1u], t.x), t.y);
}

// The share of plants kept at render-space `p` by the road: none on it, all `ROAD_FADE` from its
// edge, the line wandering so it thins in patches.
fn road_keep(roads: u32, f: vec2<f32>, p: vec2<f32>) -> f32 {
    let gap = road_gap(roads, f) + (land_noise(p + vec2<f32>(3.7, 21.3), 2.5) - 0.5) * 2.0
        + (land_noise(p + vec2<f32>(17.9, 2.3), 0.9) - 0.5) * 0.8;
    return smoothstep(0.0, ROAD_FADE, gap);
}

// How patchily each terrain type grows, 0 evenly to 1 in clumps with bare ground between.
const PATCHY = array<f32, 32>(0.9, 0.15, 0.9, 0.05, 0.6, 0.9, 0.9, 0.9, 0.9, 0.75, 0.9, 0.9, 0.9,
    0.9, 0.9, 0.3, 0.3, 0.3, 0.3, 0.3, 0.3, 0.6, 0.3, 0.3, 0.3, 0.3, 0.3, 0.3, 0.15, 0.6, 0.3, 0.3);
// Grid points a side of a tile, one workgroup's.
const COVER_TILE: u32 = 8u;
// How much thinner the dense kinds stand over the grass (blades.wgsl).
const TUFTS_OVER_GRASS: f32 = 0.3;
// Of the share kept, the span over which a plant grows in from nothing.
const GROW_IN: f32 = 0.12;

// The share of plants kept at world `w` by bare patches a few metres across, more of them where
// the ground grows patchily (`patchy`).
fn patches(w: vec2<f32>, patchy: f32) -> f32 {
    let n = land_noise(w, 6.0) * 0.65 + land_noise(w + vec2<f32>(17.0, 5.0), 2.2) * 0.35;
    let lo = mix(0.15, 0.36, patchy);
    return smoothstep(lo, lo + 0.12, n);
}

@compute @workgroup_size(8, 8, 1)
fn place(@builtin(workgroup_id) wg: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>,
         @builtin(local_invocation_id) local: vec3<u32>) {
    if (!has_land()) {
        return;
    }
    // The tile, and the kind whose tiles it is among.
    let tile = wg.y * groups.x + wg.x;
    var kind = 0u;
    for (var i = 1u; i < u32(cp.world.w); i++) {
        if (tile >= cover_kinds[i].tiles.y) {
            kind = i;
        }
    }
    let k = cover_kinds[kind];
    let t = tile - k.tiles.y;
    if (t >= k.tiles.x * k.tiles.x) {
        return;
    }
    let reach = k.reach;
    let s = k.step;
    let first = floor((cp.eye.xy + cp.world.xy - reach) / s);
    let corner = vec2<f32>(f32(t % k.tiles.x), f32(t / k.tiles.x)) * f32(COVER_TILE);
    // The tile, out of reach or out of view, is passed over whole.
    let half = f32(COVER_TILE) * s * 0.5;
    let middle = (first + corner) * s + half - cp.world.xy;
    if (distance(middle, cp.eye.xy) > reach + half * 1.42) {
        return;
    }
    let at = vec3<f32>(middle, land_height(middle).x + k.height * k.scale_hi * cp.eye.w * 0.5);
    let bound = half * 1.6 + k.radius * k.scale_hi * max(cp.eye.w, 1.0);
    for (var i = 0u; i < 6u; i++) {
        let pl = cp.planes[i];
        if (dot(pl.xyz, at) + pl.w < -bound) {
            return;
        }
    }
    let g = first + corner + vec2<f32>(local.xy);
    let gi = vec2<i32>(g);
    // Where in its square it stands, in the world, then in render space.
    let jitter = vec2<f32>(cover_hash(gi, k.seed, 0u), cover_hash(gi, k.seed, 1u));
    let p = (g + jitter) * s - cp.world.xy;
    let flat_d = distance(p, cp.eye.xy);
    if (flat_d > reach) {
        return;
    }
    // The land about it.
    let q = (p - frame.land.xy) / LAND_CELL;
    let c = vec2<i32>(floor(q));
    if (any(c < vec2<i32>(0)) || any(c >= vec2<i32>(frame.land.zw) - 1)) {
        return;
    }
    let f = q - floor(q);
    let a = land_vertex(c);
    let b = land_vertex(c + vec2<i32>(1, 0));
    let d = land_vertex(c + vec2<i32>(0, 1));
    let e = land_vertex(c + vec2<i32>(1, 1));
    let h = cover_hash(gi, k.seed, 4u);
    // None in the water or at its very edge.
    let inside = mix(mix(a.inside, b.inside, f.x), mix(d.inside, e.inside, f.x), f.y);
    if (inside > -1.0 - h * 1.5) {
        return;
    }
    // Bare within a few tens of centimetres of a building's walls and full a few metres out, the
    // line wandering a little so it never reads as the box.
    let gap = building_gap(c, p) + (land_noise(p + vec2<f32>(13.1, 5.7), 2.5) - 0.5) * 0.8;
    let clearing = smoothstep(0.3, 3.0, gap) * object_keep(c, p);
    let share = mix(mix(grows_on(k, a.word), grows_on(k, b.word), f.x),
                    mix(grows_on(k, d.word), grows_on(k, e.word), f.x), f.y) * clearing;
    // The edge of what it grows on wanders a little, the same each visit.
    let wander = (land_noise(p, 7.0) - 0.5) * 0.5 * min(share * 4.0, 1.0);
    var keep = share + wander * clearing;
    if (keep <= h) {
        return;
    }
    // Bare patches, by how patchily the ground about it grows; the sparse kinds in drifts.
    let w = cover_world(p);
    let patchy = mix(mix(PATCHY[a.word & LW_TYPE], PATCHY[b.word & LW_TYPE], f.x),
                     mix(PATCHY[d.word & LW_TYPE], PATCHY[e.word & LW_TYPE], f.x), f.y);
    keep *= patches(w, patchy);
    if (s > 1.4) {
        let o = vec2<f32>(f32(k.seed & 255u), f32((k.seed >> 8u) & 255u)) * 1.37;
        keep *= 2.0 * smoothstep(0.35, 0.65, land_noise(w + o, 5.0));
    } else {
        // The dense kinds stand thinner where the grass under them grows.
        let under = mix(mix(BLADE_GROWS[a.word & LW_TYPE].x, BLADE_GROWS[b.word & LW_TYPE].x, f.x),
                        mix(BLADE_GROWS[d.word & LW_TYPE].x, BLADE_GROWS[e.word & LW_TYPE].x, f.x), f.y);
        keep *= 1.0 - TUFTS_OVER_GRASS * clamp(under, 0.0, 1.0);
    }
    // Thinning in toward a road as the ground draws it.
    let roads = on_road(a.word) | (on_road(b.word) << 1u) | (on_road(e.word) << 2u) | (on_road(d.word) << 3u);
    let road = road_keep(roads, f, p);
    keep *= road;
    if (keep <= h) {
        return;
    }
    let cut = (a.word & LW_SW_TO_NE) != 0u;
    let hs = vec4<f32>(a.height, b.height, d.height, e.height);
    // On Landscape Smoothing's drawn ground, at its height and slope.
    let z = on_triangles(cut, hs, f) + drawn_offset(p, cut);
    // The ground's slope on its triangle.
    let ex = on_triangles(cut, hs, clamp(f + vec2<f32>(0.02, 0.0), vec2<f32>(0.0), vec2<f32>(1.0)))
        - on_triangles(cut, hs, clamp(f - vec2<f32>(0.02, 0.0), vec2<f32>(0.0), vec2<f32>(1.0)))
        + drawn_offset(p + vec2<f32>(0.48, 0.0), cut) - drawn_offset(p - vec2<f32>(0.48, 0.0), cut);
    let ny = on_triangles(cut, hs, clamp(f + vec2<f32>(0.0, 0.02), vec2<f32>(0.0), vec2<f32>(1.0)))
        - on_triangles(cut, hs, clamp(f - vec2<f32>(0.0, 0.02), vec2<f32>(0.0), vec2<f32>(1.0)))
        + drawn_offset(p + vec2<f32>(0.0, 0.48), cut) - drawn_offset(p - vec2<f32>(0.0, 0.48), cut);
    let slope = vec2<f32>(ex, ny) / (0.04 * LAND_CELL);
    // Thinning over the last twelve degrees before the steepest it grows on.
    let steep = acos(clamp(k.steepest, -1.0, 1.0));
    keep *= 1.0 - smoothstep(steep - 0.21, steep, acos(inverseSqrt(1.0 + dot(slope, slope))));
    // It grows in from nothing as the share kept passes its number, never whole at once.
    var grown = clamp((keep - h) / GROW_IN, 0.0, 1.0);
    // Taller in patches a few metres across, and each its own; smaller toward the edge of what it
    // grows on and toward a road.
    let size = clamp(0.5 + (land_noise(w + vec2<f32>(29.0, 61.0), 3.5) - 0.5) * 1.5
        + (cover_hash(gi, k.seed, 2u) - 0.5) * 0.7, 0.0, 1.0);
    var scale = mix(k.scale_lo, k.scale_hi, size) * (0.6 + 0.4 * clamp(share, 0.0, 1.0))
        * (0.7 + 0.3 * road);
    let foot = vec3<f32>(p, z);
    let dist = distance(foot + vec3<f32>(0.0, 0.0, k.height * scale * cp.eye.w * 0.5), cp.eye.xyz);
    // Over the cards' last quarter each shrinks away by its number.
    let far = smoothstep(k.ends[3] * 0.7, k.ends[3], dist / scale);
    grown *= clamp((1.0 - far - cover_hash(gi, k.seed, 5u)) / 0.15, 0.0, 1.0);
    if (grown < 0.05) {
        return;
    }
    scale *= grown;
    let dd = dist / scale;
    var level = 4u;
    for (var l = 0u; l < 4u; l++) {
        if (level == 4u && dd < k.ends[l]) {
            level = l;
        }
    }
    if (level == 4u) {
        return;
    }
    // Within the band before its level's end, the next level (one with room) shares it.
    var code = 0u;
    var next = 4u;
    let lo = k.ends[level] * (1.0 - COVER_BAND);
    if (level < 3u && dd > lo) {
        code = max(u32(round((dd - lo) / (k.ends[level] * COVER_BAND) * 127.0)), 1u);
        next = level + 1u;
        while (next < 3u && k.ends[next] <= k.ends[level]) {
            next++;
        }
    }
    let centre = foot + vec3<f32>(0.0, 0.0, k.height * scale * cp.eye.w * 0.5);
    let r = k.radius * scale * max(cp.eye.w, 1.0);
    for (var i = 0u; i < 6u; i++) {
        let pl = cp.planes[i];
        if (dot(pl.xyz, centre) + pl.w < -r) {
            return;
        }
    }
    let turn = cover_hash(gi, k.seed, 3u) * 6.2831853;
    grow(k, kind, level, CoverPlant(vec4<f32>(foot, scale), turn, kind, level, code));
    if (next < 4u) {
        grow(k, kind, next, CoverPlant(vec4<f32>(foot, scale), turn, kind, next, code | 0x80u));
    }
}

// Appends `plant` to its kind's level's list, while it has room.
fn grow(k: CoverKind, kind: u32, level: u32, plant: CoverPlant) {
    let slot = atomicAdd(&counts[k.count_first + level], 1u);
    if (slot >= k.caps[level]) {
        return;
    }
    plants[k.bases[level] + slot] = plant;
}

// Each draw's instance count: its level's plants, no more than its list holds.
@compute @workgroup_size(64, 1, 1)
fn finish(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x == 0u) {
        finish_blades();
    }
    if (id.x >= u32(cp.world.z)) {
        return;
    }
    let counter = draw_counters[id.x * 2u];
    let cap = draw_counters[id.x * 2u + 1u];
    draw_args[id.x * 5u + 1u] = min(atomicLoad(&counts[counter]), cap);
}
