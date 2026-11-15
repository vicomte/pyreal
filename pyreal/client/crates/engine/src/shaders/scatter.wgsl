// Wildflowers and stones among the grass (cover.rs `scatter_reach`), placed on the GPU each frame
// in square tiles round the eye as the grass is, a workgroup a tile, a tile out of reach or view
// passed over whole. Each candidate is a hash of its tile and number, so the same flowers and
// stones lie in the same places on every visit. Flowers grow on grassland in drifts a few metres
// across, each stretch of meadow a few colours of its terrain's palette; stones lie on bare
// patches, dirt, sand and rock and in a band along the road's edges, few on the road itself. Both
// keep the grass's rules at buildings, objects, the water and on slopes, and come and go by size. Their
// records go in the clumps' list past the grass's: flowers from `cp.scatter_rooms.x`, stones after.

// Metres a side of a tile.
const SCATTER_TILE: f32 = 8.0;
// Each terrain type's flowers: x the share of candidates kept in a drift, y how tall, metres, a
// little over its grass so the heads stand among the tips.
// Grassland's comes to about a fifth of a flower a square metre.
const FLOWER_GROWS = array<vec2<f32>, 32>(
    vec2<f32>(0.0), vec2<f32>(0.7, 0.46), vec2<f32>(0.0), vec2<f32>(0.75, 0.54),
    vec2<f32>(0.15, 0.6), vec2<f32>(0.0), vec2<f32>(0.0), vec2<f32>(0.0),
    vec2<f32>(0.04, 0.3), vec2<f32>(0.45, 0.42), vec2<f32>(0.0), vec2<f32>(0.0),
    vec2<f32>(0.0), vec2<f32>(0.0), vec2<f32>(0.0), vec2<f32>(0.0),
    vec2<f32>(0.0), vec2<f32>(0.0), vec2<f32>(0.0), vec2<f32>(0.0),
    vec2<f32>(0.0), vec2<f32>(0.1, 0.36), vec2<f32>(0.0), vec2<f32>(0.0),
    vec2<f32>(0.0), vec2<f32>(0.0), vec2<f32>(0.0), vec2<f32>(0.0),
    vec2<f32>(0.4, 0.48), vec2<f32>(0.1, 0.36), vec2<f32>(0.0), vec2<f32>(0.0));
// Each terrain type's flower colours, four `FLOWER_COLOURS` a nibble each: three for its
// stretches of meadow, then one a few flowers take.
const FLOWER_PALETTES = array<u32, 32>(
    0u, 0x3210u, 0u, 0x4301u, 0x2016u, 0u, 0u, 0u, 0x5106u, 0x2561u, 0u, 0u, 0u, 0u, 0u, 0u,
    0u, 0u, 0u, 0u, 0u, 0x7230u, 0u, 0u, 0u, 0u, 0u, 0u, 0x7620u, 0x7230u, 0u, 0u);
// The flowers' colours (linear), muted to sit in the grass, and each one's head: 0 rays round an
// eye, 1 a cup, 2 a spike, 3 a flat umbel.
const FLOWER_COLOURS = array<vec4<f32>, 8>(
    vec4<f32>(0.58, 0.58, 0.52, 0.0),
    vec4<f32>(0.6, 0.44, 0.06, 1.0),
    vec4<f32>(0.34, 0.24, 0.48, 2.0),
    vec4<f32>(0.15, 0.22, 0.52, 1.0),
    vec4<f32>(0.52, 0.08, 0.05, 1.0),
    vec4<f32>(0.58, 0.24, 0.05, 0.0),
    vec4<f32>(0.52, 0.5, 0.38, 3.0),
    vec4<f32>(0.5, 0.26, 0.33, 2.0));
// Each terrain type's share of stone candidates kept, before the bare patches and roads add theirs.
const STONE_GROWS = array<f32, 32>(0.7, 0.01, 0.0, 0.0, 0.03, 0.25, 0.3, 0.5, 0.4, 0.04, 0.12, 0.2,
    0.8, 0.6, 0.55, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.08, 0.0, 0.0, 0.2, 0.5, 0.5, 0.0, 0.02, 0.06,
    0.3, 0.4);
// How wide pebbles and stones lie, metres.
const PEBBLE_SIZE: vec2<f32> = vec2<f32>(0.035, 0.1);
const STONE_SIZE: vec2<f32> = vec2<f32>(0.15, 0.3);

// A colour as the clumps' word holds it (`BLADE_RANGE`), marked known.
fn scatter_colour(c: vec3<f32>) -> u32 {
    let q = vec3<u32>(round(clamp(c / BLADE_RANGE, vec3<f32>(0.0), vec3<f32>(1.0)) * 1023.0));
    return q.x | (q.y << 10u) | (q.z << 20u) | (1u << 30u);
}

@compute @workgroup_size(64, 1, 1)
fn scatter(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_id) local: vec3<u32>) {
    if (!has_land()) {
        return;
    }
    let flower_reach = cp.scatter.x;
    let stone_reach = cp.scatter.y;
    let reach = max(flower_reach, stone_reach);
    let first = floor((cp.eye.xy + cp.world.xy - reach) / SCATTER_TILE);
    let tile = first + vec2<f32>(wg.xy);
    let ti = vec2<i32>(tile);
    let corner = tile * SCATTER_TILE - cp.world.xy;
    let middle = corner + SCATTER_TILE * 0.5;
    let tile_d = distance(middle, cp.eye.xy);
    if (tile_d - SCATTER_TILE > reach) {
        return;
    }
    let at = vec3<f32>(middle, land_height(middle).x + 0.3);
    for (var i = 0u; i < 6u; i++) {
        let pl = cp.planes[i];
        if (dot(pl.xyz, at) + pl.w < -SCATTER_TILE * 1.1) {
            return;
        }
    }
    let area = SCATTER_TILE * SCATTER_TILE;
    let flowers = select(0u, u32(cp.scatter.z * area + 0.5), tile_d - SCATTER_TILE < flower_reach);
    let stones = select(0u, u32(cp.scatter.w * area + 0.5), tile_d - SCATTER_TILE < stone_reach);
    for (var k = local.x; k < flowers + stones; k += 64u) {
        let stone = k >= flowers;
        let h = tile_hash(ti, k + select(0x2545F491u, 0x7F4A7C15u, stone));
        let h1 = cover_mix(h);
        let h2 = cover_mix(h1);
        let h3 = cover_mix(h2);
        let p = corner + vec2<f32>(cover_unit(h), cover_unit(h1)) * SCATTER_TILE;
        let flat_d = distance(p, cp.eye.xy);
        // Small pebbles go nearer than stones; each goes by shrinking, a few at a time.
        var size = 0.0;
        var own = flower_reach;
        if (stone) {
            let r = cover_unit(h3);
            size = select(mix(PEBBLE_SIZE.x, PEBBLE_SIZE.y, r * r / 0.81),
                          mix(STONE_SIZE.x, STONE_SIZE.y, (r - 0.9) / 0.1), r > 0.9);
            own = stone_reach * clamp(size / 0.2, 0.5, 1.0);
        }
        let fade = 1.0 - smoothstep(own * 0.5, own, flat_d);
        let scale = clamp((fade - cover_unit(cover_mix(h3)) * 0.7) / 0.3, 0.0, 1.0);
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
        var grows = vec2<f32>(0.0);
        var strewn = 0.0;
        var bare = 0.0;
        var colour = vec4<f32>(0.0);
        var patchy = 0.0;
        var most = 0u;
        for (var j = 0u; j < 4u; j++) {
            let g = FLOWER_GROWS[types[j]];
            grows += vec2<f32>(g.x, g.x * g.y) * wt[j];
            strewn += STONE_GROWS[types[j]] * wt[j];
            bare += f32((cp.bare.x >> types[j]) & 1u) * wt[j];
            let gc = blade_grounds[types[j]];
            colour += vec4<f32>(gc.rgb, 1.0) * gc.w * wt[j];
            patchy += PATCHY[types[j]] * wt[j];
            if (wt[j] > wt[most]) {
                most = j;
            }
        }
        let w = cover_world(p);
        let roads = on_road(a.word) | (on_road(b.word) << 1u) | (on_road(e.word) << 2u) | (on_road(d.word) << 3u);
        // What the bare patches leave.
        let kept = patches(w, patchy);
        var share: f32;
        if (stone) {
            // On bare patches and stony ground, and in a band along the road's edge.
            let gap = road_gap(roads, f) + (land_noise(w + vec2<f32>(5.3, 11.9), 1.7) - 0.5) * 1.2;
            var verge = smoothstep(-1.0, 0.0, gap) * (1.0 - smoothstep(1.0, 3.0, gap));
            var on_road = 0.04 * (1.0 - smoothstep(-0.8, -0.4, gap));
            if (cp.bare.y != 0u) {
                verge = 0.0;
                on_road = 0.0;
            }
            // A pack keeps them off its grounds that ask, the road's edges aside.
            share = max((strewn + 0.3 * (1.0 - kept)) * (1.0 - bare), 0.9 * verge) + on_road;
            share *= mix(0.5, 1.2, land_noise(w + vec2<f32>(47.0, 3.0), 4.0));
        } else {
            if (grows.x <= 0.0) {
                continue;
            }
            // In small drifts over about a sixth of the meadow, a rare one between.
            let n = land_noise(w + vec2<f32>(31.0, 77.0), 6.0) * 0.7 + land_noise(w + vec2<f32>(41.0, 13.0), 2.5) * 0.3;
            share = grows.x * max(smoothstep(0.66, 0.76, n), 0.005) * kept * road_keep(roads, f, p);
        }
        share *= smoothstep(0.2, 2.0, building_gap(c, p)) * object_keep(c, p);
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
        let slope = vec2<f32>(sx, sy) / (0.04 * LAND_CELL);
        share *= 1.0 - smoothstep(0.45, 0.67, length(slope));
        // It grows in from nothing as the share kept passes its number.
        let grow = clamp((share - u) / 0.12, 0.0, 1.0);
        if (grow <= 0.02) {
            continue;
        }
        let foot = vec3<f32>(p, z);
        var record: Clump;
        if (stone) {
            // The ground's colour, greyer, each stone lighter or darker and a little warm or cool.
            var g = vec3<f32>(0.1, 0.1, 0.08);
            if (colour.w > 0.5) {
                g = colour.rgb / colour.w;
            }
            let hs2 = cover_mix(h3 ^ 0x51ED27u);
            let grey = mix(vec3<f32>(luma(g)), g, 0.5) * mix(0.75, 1.45, cover_unit(hs2))
                * mix(vec3<f32>(1.05, 1.0, 0.92), vec3<f32>(0.95, 0.98, 1.05), cover_unit(cover_mix(hs2)));
            let tilt = vec2<u32>(round(clamp(slope, vec2<f32>(-1.0), vec2<f32>(1.0)) * 127.0 + 127.0));
            record = Clump(vec4<f32>(foot, size * scale * mix(0.6, 1.0, grow)), h3,
                           scatter_colour(grey * 1.25), 1.0, tilt.x | (tilt.y << 8u));
        } else {
            // A stretch of meadow's colours: one of its terrain's three, now and then the fourth.
            let palette = FLOWER_PALETTES[types[most]];
            let region = land_noise(w + vec2<f32>(71.0, 19.0), 14.0);
            var pick = min(u32(region * 3.0), 2u);
            let v = cover_unit(cover_mix(h3 ^ 0x68E31DA4u));
            if (v < 0.12) {
                pick = 3u;
            } else if (v < 0.3) {
                pick = (pick + 1u) % 3u;
            }
            let fc = FLOWER_COLOURS[(palette >> (pick * 4u)) & 7u];
            let tall = grows.y / grows.x * mix(0.75, 1.2, cover_unit(cover_mix(h1 ^ 0x3C6EF372u)))
                * mix(0.62, 1.28, land_noise(w + vec2<f32>(29.0, 61.0), 3.5)) * scale * grow;
            let head = mix(0.032, 0.05, cover_unit(cover_mix(h2 ^ 0x1B873593u))) * mix(0.5, 1.0, scale);
            record = Clump(vec4<f32>(foot, tall), h3,
                           scatter_colour(fc.rgb * mix(0.88, 1.1, cover_unit(cover_mix(h)))), head,
                           u32(fc.w));
        }
        // Within view, its height about it.
        let lift = select(record.at.w * max(cp.eye.w, 1.0) + 0.15, record.at.w, stone);
        var seen = true;
        for (var i = 0u; i < 6u; i++) {
            let pl = cp.planes[i];
            if (dot(pl.xyz, foot) + pl.w < -lift) {
                seen = false;
            }
        }
        if (!seen) {
            continue;
        }
        let which = select(2u, 3u, stone);
        let slot = atomicAdd(&clump_counts[which], 1u);
        let room = select(cp.scatter_rooms.y, cp.scatter_rooms.z, stone);
        if (slot >= room) {
            continue;
        }
        clumps[cp.scatter_rooms.x + select(0u, cp.scatter_rooms.y, stone) + slot] = record;
    }
}
