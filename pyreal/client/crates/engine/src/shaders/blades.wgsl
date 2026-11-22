// The grass under the cover (cover.rs `Blades`): clumps of blades placed on the GPU each frame in
// square tiles round the eye, a workgroup a tile, a tile out of reach or out of view passed over
// whole. A tile's candidates are hashes of the tile and their number, so the same clumps stand in
// the same places on every visit. Each terrain type grows its own share and height of grass; it
// thins in the bare patches the cover's own noise lays, toward roads, buildings and objects, on slopes and
// at the water, and grows in by size, never whole at once. Near the eye clumps are dense and each
// a few curved blades, thinning with distance; past the near reach fewer, wider clumps of single
// blades stand in for them, the near shrinking away as the far grow in. Each clump takes the
// ground's colour under it.

// Metres a side of a tile.
const BLADE_TILE: f32 = 8.0;
// Each terrain type's grass: x the share of clumps kept, y how tall, metres, z how dry its tips.
const BLADE_GROWS = array<vec3<f32>, 32>(
    vec3<f32>(0.0), vec3<f32>(1.0, 0.34, 0.25), vec3<f32>(0.0), vec3<f32>(1.0, 0.42, 0.1),
    vec3<f32>(0.6, 0.5, 0.05), vec3<f32>(0.08, 0.22, 0.3), vec3<f32>(0.0), vec3<f32>(0.0),
    vec3<f32>(0.25, 0.22, 0.55), vec3<f32>(0.85, 0.3, 0.4), vec3<f32>(0.0), vec3<f32>(0.0),
    vec3<f32>(0.0), vec3<f32>(0.1, 0.2, 0.7), vec3<f32>(0.0), vec3<f32>(0.0),
    vec3<f32>(0.0), vec3<f32>(0.0), vec3<f32>(0.0), vec3<f32>(0.0),
    vec3<f32>(0.0), vec3<f32>(0.3, 0.26, 0.35), vec3<f32>(0.0), vec3<f32>(0.0),
    vec3<f32>(0.0), vec3<f32>(0.0), vec3<f32>(0.0), vec3<f32>(0.0),
    vec3<f32>(1.0, 0.36, 0.2), vec3<f32>(0.3, 0.26, 0.35), vec3<f32>(0.0), vec3<f32>(0.0));

// The clumps (near from the first, far from `cp.rooms.y`, then the flowers and the stones from
// `cp.scatter_rooms.x`), how many of each, their draws.
@group(1) @binding(11) var<storage, read_write> clumps: array<Clump>;
@group(1) @binding(12) var<storage, read_write> clump_counts: array<atomic<u32>, 4>;
@group(1) @binding(13) var<storage, read_write> clump_args: array<u32, 16>;
// Each terrain type's ground colour (linear; w 1 where known).
@group(1) @binding(14) var<uniform> blade_grounds: array<vec4<f32>, 32>;

fn tile_hash(t: vec2<i32>, k: u32) -> u32 {
    return cover_mix((bitcast<u32>(t.x) * 73856093u) ^ (bitcast<u32>(t.y) * 19349663u) ^ (k * 0x9E3779B9u));
}

@compute @workgroup_size(64, 1, 1)
fn blades(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_id) local: vec3<u32>) {
    if (!has_land()) {
        return;
    }
    let near_reach = cp.blades.x;
    let far_reach = cp.blades.y;
    let hand = max(near_reach * 0.25, 4.0);
    // The tile, laid on the world's own metres.
    let first = floor((cp.eye.xy + cp.world.xy - far_reach) / BLADE_TILE);
    let tile = first + vec2<f32>(wg.xy);
    let ti = vec2<i32>(tile);
    let corner = tile * BLADE_TILE - cp.world.xy;
    let middle = corner + BLADE_TILE * 0.5;
    let tile_d = distance(middle, cp.eye.xy);
    if (tile_d - BLADE_TILE > far_reach) {
        return;
    }
    let at = vec3<f32>(middle, land_height(middle).x + 0.3);
    for (var i = 0u; i < 6u; i++) {
        let pl = cp.planes[i];
        if (dot(pl.xyz, at) + pl.w < -BLADE_TILE * 1.1) {
            return;
        }
    }
    let area = BLADE_TILE * BLADE_TILE;
    let near_count = select(0u, u32(cp.blades.z * area + 0.5), tile_d - BLADE_TILE < near_reach);
    let far_count = select(0u, u32(cp.blades.w * area + 0.5), tile_d + BLADE_TILE > near_reach - hand);
    for (var k = local.x; k < near_count + far_count; k += 64u) {
        let far = k >= near_count;
        let h = tile_hash(ti, k + select(0u, 0x5BD1E995u, far));
        let h1 = cover_mix(h);
        let h2 = cover_mix(h1);
        let p = corner + vec2<f32>(cover_unit(h), cover_unit(h1)) * BLADE_TILE;
        let flat_d = distance(p, cp.eye.xy);
        // The near thin a little with distance and shrink away into the far, which grow in where
        // the near go and shrink away toward their reach.
        var scale: f32;
        var keep: f32;
        if (!far) {
            scale = 1.0 - smoothstep(near_reach - hand, near_reach, flat_d);
            keep = mix(1.0, 0.35, smoothstep(8.0, near_reach, flat_d));
        } else {
            scale = smoothstep(near_reach - hand, near_reach, flat_d)
                * (1.0 - smoothstep(far_reach * 0.8, far_reach, flat_d));
            keep = mix(0.9, 0.25, smoothstep(near_reach, far_reach, flat_d));
        }
        if (scale <= 0.02) {
            continue;
        }
        let q = (p - frame.land.xy) / LAND_CELL;
        let c = vec2<i32>(floor(q));
        if (any(c < vec2<i32>(0)) || any(c >= vec2<i32>(frame.land.zw) - 1)) {
            continue;
        }
        let f = q - floor(q);
        let a = land_vertex(c);
        let b = land_vertex(c + vec2<i32>(1, 0));
        let d = land_vertex(c + vec2<i32>(0, 1));
        let e = land_vertex(c + vec2<i32>(1, 1));
        let wt = vec4<f32>((1.0 - f.x) * (1.0 - f.y), f.x * (1.0 - f.y), (1.0 - f.x) * f.y, f.x * f.y);
        let types = vec4<u32>(a.word, b.word, d.word, e.word) & vec4<u32>(LW_TYPE);
        var grows = vec3<f32>(0.0);
        var colour = vec4<f32>(0.0);
        var patchy = 0.0;
        for (var j = 0u; j < 4u; j++) {
            let g = BLADE_GROWS[types[j]];
            grows += vec3<f32>(g.x, g.x * g.yz) * wt[j];
            let gc = blade_grounds[types[j]];
            colour += vec4<f32>(gc.rgb, 1.0) * gc.w * wt[j];
            patchy += PATCHY[types[j]] * wt[j];
        }
        if (grows.x <= 0.0) {
            continue;
        }
        let tall_here = grows.y / grows.x;
        let dry_here = grows.z / grows.x;
        let w = cover_world(p);
        var share = grows.x * patches(w, patchy) * keep * sand_bare(vec4<u32>(a.word, b.word, d.word, e.word), f, p);
        // Off the road, and thinning toward a building's walls.
        let roads = on_road(a.word) | (on_road(b.word) << 1u) | (on_road(e.word) << 2u) | (on_road(d.word) << 3u);
        let road = road_keep(roads, f, p);
        share *= road * smoothstep(0.2, 2.0, building_gap(c, p)) * object_keep(c, p);
        let u = cover_unit(h2);
        if (share <= u) {
            continue;
        }
        // None in the water or at its very edge.
        let inside = dot(vec4<f32>(a.inside, b.inside, d.inside, e.inside), wt);
        if (inside > -0.8 - u * 1.5) {
            continue;
        }
        let cut = (a.word & LW_SW_TO_NE) != 0u;
        let hs = vec4<f32>(a.height, b.height, d.height, e.height);
        let z = on_triangles(cut, hs, f) + drawn_offset(p, cut);
        // Less on slopes, none on scree.
        let sx = on_triangles(cut, hs, clamp(f + vec2<f32>(0.02, 0.0), vec2<f32>(0.0), vec2<f32>(1.0)))
            - on_triangles(cut, hs, clamp(f - vec2<f32>(0.02, 0.0), vec2<f32>(0.0), vec2<f32>(1.0)));
        let sy = on_triangles(cut, hs, clamp(f + vec2<f32>(0.0, 0.02), vec2<f32>(0.0), vec2<f32>(1.0)))
            - on_triangles(cut, hs, clamp(f - vec2<f32>(0.0, 0.02), vec2<f32>(0.0), vec2<f32>(1.0)));
        let slope = length(vec2<f32>(sx, sy)) / (0.04 * LAND_CELL);
        share *= 1.0 - smoothstep(0.45, 0.67, slope);
        // It grows in from nothing as the share kept passes its number.
        let grow = clamp((share - u) / 0.12, 0.0, 1.0);
        if (grow <= 0.02) {
            continue;
        }
        let foot = vec3<f32>(p, z - 0.03);
        // Within view, its height about it.
        var seen = true;
        for (var i = 0u; i < 6u; i++) {
            let pl = cp.planes[i];
            if (dot(pl.xyz, foot) + pl.w < -1.0) {
                seen = false;
            }
        }
        if (!seen) {
            continue;
        }
        // Taller in patches a few metres across, as the cover is, and each its own.
        let tall = tall_here * scale * grow * mix(0.78, 1.18, cover_unit(cover_mix(h2)))
            * mix(0.62, 1.28, world_noise(w + vec2<f32>(29.0, 61.0), 3.5));
        // A little drier here, a little lusher there.
        let dry = clamp(dry_here + (world_noise(w, 23.0) - 0.5) * 0.5, 0.0, 1.0);
        var tint = 0u;
        if (colour.w > 0.5) {
            let t = clamp(colour.rgb / colour.w * mix(0.92, 1.06, world_noise(w + vec2<f32>(3.0, 9.0), 11.0))
                / BLADE_RANGE, vec3<f32>(0.0), vec3<f32>(1.0));
            let qc = vec3<u32>(round(t * 1023.0));
            tint = qc.x | (qc.y << 10u) | (qc.z << 20u) | (1u << 30u);
        }
        let which = select(0u, 1u, far);
        let slot = atomicAdd(&clump_counts[which], 1u);
        let room = select(cp.rooms.x, cp.rooms.z, far);
        if (slot >= room) {
            continue;
        }
        let wide = min(inverseSqrt(max(keep, 0.2)), 2.0);
        clumps[select(0u, cp.rooms.y, far) + slot] = Clump(vec4<f32>(foot, tall), h2, tint, wide,
            u32(dry * 255.0 + 0.5) | (which << 8u));
    }
}

// The clumps' draws' instance counts, then the flowers' and the stones'.
fn finish_blades() {
    let rooms = vec4<u32>(cp.rooms.x, cp.rooms.z, cp.scatter_rooms.y, cp.scatter_rooms.z);
    for (var i = 0u; i < 4u; i++) {
        clump_args[i * 4u + 1u] = min(atomicLoad(&clump_counts[i]), rooms[i]);
    }
}
