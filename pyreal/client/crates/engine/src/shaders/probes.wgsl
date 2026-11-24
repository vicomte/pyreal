// The probes' update: a workgroup per probe, a ray per invocation over a Fibonacci sphere turned
// at random each trace.
// A ray that hits a surface takes its picture's colour (the hit triangle's texture coordinates
// read from the shared pools) lit by the sun (a shadow ray), the frame's nearest lamps and the
// probes' own light from last time (so light bounces again and again over the frames); a ray that
// escapes takes the sky's light, none in a dungeon. A probe whose rays mostly meet backs of
// surfaces is buried in a wall or the ground and is not trusted.

@group(1) @binding(0) var world_rays: acceleration_structure;
@group(1) @binding(1) var<storage, read_write> probes: array<Probe>;
@group(1) @binding(2) var<storage, read> vertex_words: array<u32>;
@group(1) @binding(3) var<storage, read> index_words: array<u32>;
// Per instance of the top level, where its mesh's draws start in `geoms`.
@group(1) @binding(4) var<storage, read> ray_instances: array<u32>;
// Per draw: its first index and its material.
@group(1) @binding(5) var<storage, read> geoms: array<vec2<u32>>;
@group(1) @binding(6) var<uniform> grid: ProbeGrid;
@group(1) @binding(7) var<storage, read> lamps: array<Lamp>;

const RAYS: u32 = 32u;
// The probes round the eye traced every frame: across, along, up.
const NEAR: vec4<u32> = vec4<u32>(12u, 12u, 4u, 0u);
const REACH: f32 = 80.0;
// A ray brings at most this many times the trace's mean light, so one ray meeting a lamp's hot
// spot does not light the probe for a second.
const FIREFLY: f32 = 6.0;
// The share of rays meeting backs over which a probe goes from trusted to buried.
const BURIED: vec2<f32> = vec2<f32>(0.15, 0.35);
// A pool vertex's words: position (3), normal, texture coordinates (2), two more.
const VERTEX_WORDS: u32 = 8u;

var<workgroup> sums: array<vec4<f32>, 128>;
var<workgroup> mean_light: array<f32, 32>;

fn vertex_position(i: u32) -> vec3<f32> {
    let w = i * VERTEX_WORDS;
    return vec3<f32>(bitcast<f32>(vertex_words[w]), bitcast<f32>(vertex_words[w + 1u]),
                     bitcast<f32>(vertex_words[w + 2u]));
}

fn vertex_uv(i: u32) -> vec2<f32> {
    let w = i * VERTEX_WORDS + 4u;
    return vec2<f32>(bitcast<f32>(vertex_words[w]), bitcast<f32>(vertex_words[w + 1u]));
}

fn mix_bits(x: u32) -> u32 {
    var h = x;
    h ^= h >> 16u;
    h *= 0x7feb352du;
    h ^= h >> 15u;
    h *= 0x846ca68bu;
    h ^= h >> 16u;
    return h;
}

// A uniformly random turn for `seed` (Shoemake's), as a quaternion.
fn random_turn(seed: u32) -> vec4<f32> {
    let a = mix_bits(seed);
    let b = mix_bits(a);
    let c = mix_bits(b);
    let u = vec3<f32>(f32(a >> 8u), f32(b >> 8u), f32(c >> 8u)) / 16777216.0;
    let s1 = sqrt(1.0 - u.x);
    let s2 = sqrt(u.x);
    let t1 = 2.0 * PI * u.y;
    let t2 = 2.0 * PI * u.z;
    return vec4<f32>(s1 * sin(t1), s1 * cos(t1), s2 * sin(t2), s2 * cos(t2));
}

fn turned(q: vec4<f32>, v: vec3<f32>) -> vec3<f32> {
    return v + 2.0 * cross(q.xyz, cross(q.xyz, v) + q.w * v);
}

fn probe_position(cell: vec3<i32>) -> vec3<f32> {
    return vec3<f32>(cell) * grid.spacing.xyz;
}

// The grid's first frame (the probes started again, or the time of day set): last time's light
// is forgotten.
fn afresh() -> bool {
    return (grid.dims.w >> 8u) == 0u;
}

// The probes' light at `p` on a surface facing `n`, last time's (zero where untrusted).
fn bounce_at(p: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    let cell = vec3<i32>(round(p / grid.spacing.xyz));
    let rel = cell - grid.base.xyz;
    if (afresh() || any(rel < vec3<i32>(0)) || any(rel >= vec3<i32>(grid.dims.xyz))) {
        return vec3<f32>(0.0);
    }
    let pr = probes[probe_slot(cell, grid.dims.xyz)];
    if (pr.state.w <= 0.0 || any(vec3<i32>(pr.state.xyz) != cell)) {
        return vec3<f32>(0.0);
    }
    return probe_eval(pr, n) * pr.state.w;
}

// The light leaving a surface the ray met: (radiance times pi, 1 for its back).
fn hit_light(dir: vec3<f32>, hit: RayIntersection) -> vec4<f32> {
    let g = geoms[ray_instances[hit.instance_index] + hit.geometry_index];
    let first = g.x + hit.primitive_index * 3u;
    let i0 = index_words[first];
    let i1 = index_words[first + 1u];
    let i2 = index_words[first + 2u];
    let b = vec3<f32>(1.0 - hit.barycentrics.x - hit.barycentrics.y, hit.barycentrics);
    let uv = vertex_uv(i0) * b.x + vertex_uv(i1) * b.y + vertex_uv(i2) * b.z;
    let p0 = vertex_position(i0);
    let local_n = cross(vertex_position(i1) - p0, vertex_position(i2) - p0);
    var n = normalize((hit.object_to_world * vec4<f32>(local_n, 0.0)).xyz);
    let back = dot(n, dir) > 0.0;
    if (back) {
        n = -n;
    }
    let m = materials[g.y];
    var albedo = m.base_color.rgb;
    if ((m.flags & M_TERRAIN) != 0u) {
        albedo = vec3<f32>(0.22, 0.24, 0.15);
    } else if ((m.flags & M_HAS_BASE) != 0u) {
        albedo *= textureSampleLevel(textures[m.tex_base], samp_repeat, uv, 3.0).rgb;
    }
    let at = (hit.object_to_world * vec4<f32>(p0 * b.x + vertex_position(i1) * b.y
        + vertex_position(i2) * b.z, 1.0)).xyz;
    var e = bounce_at(at + n * 0.3, n);
    var sq: ray_query;
    // The sun, where the outside is seen, through a shadow ray.
    if (frame.switches.x != 0u) {
        let ndl = dot(n, frame.sun_dir.xyz);
        if (ndl > 0.0) {
            rayQueryInitialize(&sq, world_rays,
                               RayDesc(0x5u, 0xFFu, 0.02, 300.0, at + n * 0.05, frame.sun_dir.xyz));
            rayQueryProceed(&sq);
            if (rayQueryGetCommittedIntersection(&sq).kind == RAY_QUERY_INTERSECTION_NONE) {
                e += frame.sun_light.rgb * ndl;
            }
        }
    }
    // The lamps nearest the eye, wherever it looks, unshadowed; outdoors held under the sky's.
    var outdoors = vec3<f32>(0.0);
    let count = u32(max(grid.base.w, 0));
    for (var k = 0u; k < count; k++) {
        let l = lamps[k];
        let to = l.position - at;
        let d2 = max(dot(to, to), 0.25);
        let d = sqrt(d2);
        if (d < l.range) {
            let facing = max(dot(n, to / d), 0.0);
            if ((l.flags & 256u) == 0u && has(F_PHYSICAL)) {
                // Outdoors, the same light the lamp throws directly.
                outdoors += outdoor_lamp(l, d) * outdoor_lamp_scale() * facing;
            } else {
                // A room's lamp as it lights the room directly, held to its colour.
                e += room_lamp(l, dot(to, to)) * facing;
            }
        }
    }
    e += lamps_held(outdoors, e);
    var glow = vec3<f32>(0.0);
    if ((m.flags & M_LUMINOUS) != 0u) {
        glow = albedo * m.emissive.x * EMISSIVE_UNITS;
    }
    return vec4<f32>(albedo * e + glow, select(0.0, 1.0, back));
}

// The light a ray that escapes brings: the sky's, as the dats' ambient spreads it over the
// upper half, so an open level surface takes exactly the sky's light; none in a dungeon.
fn miss_light(dir: vec3<f32>) -> vec3<f32> {
    if (frame.switches.x == 0u) {
        return vec3<f32>(0.0);
    }
    return frame.sky_light.rgb * smoothstep(-0.05, 0.1, dir.z);
}

@compute @workgroup_size(32)
fn update(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_index) li: u32) {
    let total = grid.dims.x * grid.dims.y * grid.dims.z;
    // Each frame traces a share of the probes: every probe once in `dims.w`'s low byte frames.
    let share = max(grid.dims.w & 0xFFu, 1u);
    let d = vec3<i32>(grid.dims.xyz);
    // The probes round the eye, every frame, after the share.
    let near_first = grid.base.xyz + d / 2 - vec3<i32>(NEAR.xyz) / 2;
    let in_share = (total + share - 1u) / share;
    var cell: vec3<i32>;
    var slot: u32;
    var every = false;
    if (wg.x >= in_share) {
        let n = wg.x - in_share;
        if (share == 1u || n >= NEAR.x * NEAR.y * NEAR.z) {
            return;
        }
        cell = near_first + vec3<i32>(vec3<u32>(n % NEAR.x, (n / NEAR.x) % NEAR.y,
                                                n / (NEAR.x * NEAR.y)));
        slot = probe_slot(cell, grid.dims.xyz);
        every = true;
    } else {
        slot = wg.x * share + ((grid.dims.w >> 8u) % share);
        if (slot >= total) {
            return;
        }
        // The cell this slot holds now.
        let s = vec3<i32>(i32(slot % grid.dims.x), i32((slot / grid.dims.x) % grid.dims.y),
                          i32(slot / (grid.dims.x * grid.dims.y)));
        cell = grid.base.xyz + (((s - grid.base.xyz) % d) + d) % d;
        let rel = cell - near_first;
        if (share > 1u && all(rel >= vec3<i32>(0)) && all(rel < vec3<i32>(NEAR.xyz))) {
            return;
        }
    }
    let origin = probe_position(cell);
    // A Fibonacci sphere, turned at random for this probe and frame.
    let k = f32(li) + 0.5;
    let z = 1.0 - 2.0 * k / f32(RAYS);
    let r = sqrt(max(1.0 - z * z, 0.0));
    let a = k * 2.39996323;
    let turn = random_turn(slot * 0x9E3779B9u + (grid.dims.w >> 8u));
    let dir = turned(turn, vec3<f32>(r * cos(a), r * sin(a), z));
    var rq: ray_query;
    rayQueryInitialize(&rq, world_rays, RayDesc(0x1u, 0xFFu, 0.05, REACH, origin, dir));
    rayQueryProceed(&rq);
    let hit = rayQueryGetCommittedIntersection(&rq);
    var light = vec4<f32>(miss_light(dir), 0.0);
    if (hit.kind != RAY_QUERY_INTERSECTION_NONE) {
        light = hit_light(dir, hit);
    }
    // Each ray held to a few times the trace's mean.
    mean_light[li] = luma(light.rgb);
    workgroupBarrier();
    for (var step = 16u; step > 0u; step >>= 1u) {
        if (li < step) {
            mean_light[li] += mean_light[li + step];
        }
        workgroupBarrier();
    }
    let most = FIREFLY * mean_light[0] / f32(RAYS);
    let l = luma(light.rgb);
    if (l > most) {
        light = vec4<f32>(light.rgb * (most / l), light.w);
    }
    let y = vec4<f32>(0.282095, 0.488603 * dir);
    let norm = 4.0 * PI / f32(RAYS);
    sums[li] = light.r * y * norm;
    sums[li + 32u] = light.g * y * norm;
    sums[li + 64u] = light.b * y * norm;
    sums[li + 96u] = vec4<f32>(light.w, 0.0, 0.0, 0.0);
    workgroupBarrier();
    for (var step = 16u; step > 0u; step >>= 1u) {
        if (li < step) {
            for (var c = 0u; c < 4u; c++) {
                sums[li + c * 32u] += sums[li + step + c * 32u];
            }
        }
        workgroupBarrier();
    }
    if (li != 0u) {
        return;
    }
    var p = probes[slot];
    let moved = any(vec3<i32>(p.state.xyz) != cell);
    let traces = select(p.life.x, 0.0, moved || afresh());
    let backs = sums[96].x / f32(RAYS);
    let trusted = 1.0 - smoothstep(BURIED.x, BURIED.y, backs);
    // A running mean over its first traces, then a slow blend: no one trace pops.
    let least = select(grid.spacing.w, grid.spacing.w / f32(share), every);
    let keep = 1.0 - max(1.0 / (traces + 1.0), least);
    p.r = mix(sums[0], p.r, keep);
    p.g = mix(sums[32], p.g, keep);
    p.b = mix(sums[64], p.b, keep);
    p.state = vec4<f32>(vec3<f32>(cell), mix(trusted, p.state.w, keep));
    p.life = vec4<f32>(min(traces + 1.0, 255.0), 0.0, 0.0, 0.0);
    probes[slot] = p;
}
