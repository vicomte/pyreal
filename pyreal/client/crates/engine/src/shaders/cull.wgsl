// The cull pass: one invocation per instance tests its bounding sphere against the frustum, its
// cell's screen rectangle from the portal walk and the depth pyramid of the ground, and appends
// each surviving instance to every bucket of its mesh, raising that bucket's instance count. One
// indirect draw per bucket then draws the survivors.

struct Instance {
    m0: vec4<f32>,
    m1: vec4<f32>,
    m2: vec4<f32>,
    p0: vec4<f32>,
    p1: vec4<f32>,
    p2: vec4<f32>,
    mesh: u32,
    object: u32,
    cell: u32,
    pad: u32,
}

struct MeshInfo {
    // xyz: the bounding sphere's centre; w: its radius.
    sphere: vec4<f32>,
    first_bucket: u32,
    // The bucket count; the top byte is how the mesh turns to the eye (its degrade mode).
    bucket_count: u32,
    // The coarser mesh drawn from `lod_switch` metres on (none, or nothing drawn).
    lod_next: u32,
    lod_switch: f32,
    // x: its triangles' mean edge, model units (0: not known).
    edge: vec4<f32>,
}

// Whether a caster's sphere, swept the length of the cascade along its shadow, misses the
// cascade's slice of the view: wholly past one of the view's sides, or before or past the slice.
fn shades_nothing(c: vec3<f32>, r: f32) -> bool {
    let e = c + params.fall.xyz;
    for (var p = 0u; p < 4u; p++) {
        let pl = params.view_side[p];
        if (dot(pl.xyz, c) + pl.w < -r && dot(pl.xyz, e) + pl.w < -r) {
            return true;
        }
    }
    let dc = dot(params.slice.xyz, c - params.eye.xyz);
    let de = dot(params.slice.xyz, e - params.eye.xyz);
    if (dc + r < params.slice.w && de + r < params.slice.w) {
        return true;
    }
    return dc - r > params.fall.w && de - r > params.fall.w;
}

// How far `level` is along its switch, 1 at it: the distance's share of the switch, or sooner
// where the level's triangles at `scale` and `dm` metres are finer than the pixel limit, though
// never sooner than the game's own distance: the limit only takes back what Object Detail adds.
fn switch_share(level: MeshInfo, ds: f32, dm: f32, scale: f32) -> f32 {
    let t = ds / level.lod_switch;
    if (params.pixel.x <= 0.0 || level.edge.x <= 0.0 || level.lod_next == LOD_NOTHING) {
        return t;
    }
    let fine = dm * params.pixel.x / (level.edge.x * scale);
    return max(t, min(fine, t * params.pixel.y));
}

// The mesh word of an empty slot.
const DEAD: u32 = 0xFFFFFFFFu;
// The band before a switch two levels are crossfaded over, a share of the switch distance: the
// finer level is never drawn past its switch.
const LOD_BAND: f32 = 0.25;
const LOD_NONE: u32 = 0xFFFFFFFFu;
const LOD_NOTHING: u32 = 0xFFFFFFFEu;
// The degrade mode of a level of pictures from round the model.
const IMPOSTOR: u32 = 6u;
// Within this many metres of the eye a caster is drawn at the eye's level, coarser by twice it.
const CASTER_SAME: f32 = 30.0;
// The band before the loaded land's edge a tree crossfades to its card over, a share of it.
const EDGE_BAND: f32 = 0.12;

struct BucketInfo {
    list_base: u32,
    material: u32,
}

struct DrawArgs {
    index_count: u32,
    instance_count: atomic<u32>,
    first_index: u32,
    base_vertex: i32,
    first_instance: u32,
}

struct Params {
    planes: array<vec4<f32>, 6>,
    view_proj: mat4x4<f32>,
    // Static slots used; dynamic slots used, from `dynamic_base`.
    static_count: u32,
    // 1 when the pyramid holds this frame's ground.
    use_hiz: u32,
    hiz_levels: u32,
    dynamic_count: u32,
    // The pyramid's level-0 size in texels.
    hiz_size: vec2<f32>,
    // What a distance is multiplied by before the degrade switches: the screen-size scale of
    // this view against the client's, over the bias (zero keeps every mesh at its near level).
    lod_scale: f32,
    dynamic_base: u32,
    // xyz: the eye; w: for a cascade, how far its receivers lie from its sun-side plane, metres;
    // for the view, where a tree must be its card, as the far rings past the loaded land draw it
    // (0: nowhere).
    eye: vec4<f32>,
    // x: 1 when culling for a sun's cascade (no rooms, no rectangles, no pyramid, no stats), 2
    // for the shelter map (the rooms too); y: the first of its draw arguments; z: where its
    // visible list starts; w: how much farther its casters seem to the degrade (float bits).
    target_list: vec4<u32>,
    // x: a drawn pixel's angle times the pixels a level's mean edge must cover (0: no such
    // limit), so a level gives way where its triangles grow finer than the picture shows; y: the
    // Object Detail scale, at least 1, which the limit never brings a switch nearer than.
    pixel: vec4<f32>,
    // For a cascade: the view's four sides; its slice of the view (xyz the view's forward, w where
    // its receivers start); and the way and length its shadows fall (w where the slice ends).
    view_side: array<vec4<f32>, 4>,
    slice: vec4<f32>,
    fall: vec4<f32>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> instances: array<Instance>;
@group(0) @binding(2) var<storage, read> meshes: array<MeshInfo>;
@group(0) @binding(3) var<storage, read> mesh_buckets: array<u32>;
@group(0) @binding(4) var<storage, read> buckets: array<BucketInfo>;
@group(0) @binding(5) var<storage, read_write> args: array<DrawArgs>;
@group(0) @binding(6) var<storage, read_write> visible: array<vec2<u32>>;
// Visible instances, visible triangles, instances the pyramid hid, instances drawn coarser.
@group(0) @binding(7) var<storage, read_write> stats: array<atomic<u32>, 4>;
// Per cell slot, the screen rectangle (x0, y0, x1, y1) it is seen through; empty when unseen.
@group(0) @binding(8) var<storage, read> cell_rects: array<vec4<f32>>;

// Whether the sphere lies wholly outside the part of the screen `r` covers.
fn outside_rect(r: vec4<f32>, center: vec3<f32>, radius: f32) -> bool {
    if (r.x >= r.z || r.y >= r.w) {
        return true;
    }
    if (all(r == vec4<f32>(-1.0, -1.0, 1.0, 1.0))) {
        return false;
    }
    let m = transpose(params.view_proj);
    let planes = array<vec4<f32>, 4>(
        m[0] - r.x * m[3],
        r.z * m[3] - m[0],
        m[1] - r.y * m[3],
        r.w * m[3] - m[1],
    );
    for (var k = 0u; k < 4u; k++) {
        let p = planes[k];
        if (dot(p.xyz, center) + p.w < -radius * length(p.xyz)) {
            return true;
        }
    }
    return false;
}

@group(1) @binding(0) var hiz: texture_2d<f32>;

// Whether the ground hides the sphere: its box's nearest depth is behind the farthest depth of
// the pyramid texels its screen rectangle covers, read at the level where that is two by two.
fn occluded(center: vec3<f32>, radius: f32) -> bool {
    if (params.use_hiz == 0u) {
        return false;
    }
    var lo = vec2<f32>(1e9);
    var hi = vec2<f32>(-1e9);
    var nearest = 0.0;
    for (var i = 0u; i < 8u; i++) {
        let s = vec3<f32>(f32(i & 1u), f32((i >> 1u) & 1u), f32((i >> 2u) & 1u)) * 2.0 - 1.0;
        let c = params.view_proj * vec4<f32>(center + s * radius, 1.0);
        if (c.w <= 1e-4) {
            return false;
        }
        let ndc = c.xyz / c.w;
        lo = min(lo, ndc.xy);
        hi = max(hi, ndc.xy);
        nearest = max(nearest, ndc.z);
    }
    let uv_lo = clamp(vec2<f32>(lo.x, -hi.y) * 0.5 + 0.5, vec2<f32>(0.0), vec2<f32>(1.0));
    let uv_hi = clamp(vec2<f32>(hi.x, -lo.y) * 0.5 + 0.5, vec2<f32>(0.0), vec2<f32>(1.0));
    let px_lo = uv_lo * params.hiz_size;
    let px_hi = uv_hi * params.hiz_size;
    let extent = max(px_hi.x - px_lo.x, px_hi.y - px_lo.y);
    let level = min(u32(ceil(log2(max(extent, 1.0)))), params.hiz_levels - 1u);
    let last = vec2<i32>(textureDimensions(hiz, level)) - 1;
    let scale = 1.0 / f32(1u << level);
    let a = min(vec2<i32>(floor(px_lo * scale)), last);
    let b = min(vec2<i32>(floor(px_hi * scale)), last);
    let far = min(
        min(textureLoad(hiz, a, level).r, textureLoad(hiz, vec2<i32>(b.x, a.y), level).r),
        min(textureLoad(hiz, vec2<i32>(a.x, b.y), level).r, textureLoad(hiz, b, level).r),
    );
    return nearest < far;
}

@compute @workgroup_size(64)
fn cull(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.static_count + params.dynamic_count) {
        return;
    }
    // The static slots, then the dynamic region's.
    var i = id.x;
    if (i >= params.static_count) {
        i = params.dynamic_base + (i - params.static_count);
    }
    let inst = instances[i];
    if (inst.mesh == DEAD) {
        return;
    }
    let shadow = params.target_list.x != 0u;
    // The sun never reaches the rooms, so nothing in them casts its shadow.
    if (params.target_list.x == 1u && (inst.cell & 0xFFFFFFu) != 0u) {
        return;
    }
    let mesh = meshes[inst.mesh];
    let c = mesh.sphere.xyz;
    let h = vec4<f32>(c, 1.0);
    let center = vec3<f32>(dot(inst.m0, h), dot(inst.m1, h), dot(inst.m2, h));
    let sx = length(vec3<f32>(inst.m0.x, inst.m1.x, inst.m2.x));
    let sy = length(vec3<f32>(inst.m0.y, inst.m1.y, inst.m2.y));
    let sz = length(vec3<f32>(inst.m0.z, inst.m1.z, inst.m2.z));
    let radius = mesh.sphere.w * max(sx, max(sy, sz));
    for (var p = 0u; p < 6u; p++) {
        let pl = params.planes[p];
        if (dot(pl.xyz, center) + pl.w < -radius) {
            return;
        }
    }
    if (!shadow && outside_rect(cell_rects[inst.cell & 0xFFFFFFu], center, radius)) {
        return;
    }
    if (occluded(center, radius)) {
        atomicAdd(&stats[2], 1u);
        return;
    }
    // A sun's caster whose shadow cannot reach what its cascade covers of the view.
    if (params.target_list.x == 1u && shades_nothing(center, radius)) {
        return;
    }
    // The degrade level its distance over its height scale asks for (the client's measure),
    // walking the chain from its near mesh. Over a band before a switch the eye's view and the
    // cascades draw both levels, each through the other's screen door; a cascade's casters turn
    // to a picture or to nothing at once. A cascade's casters take a coarser level than the
    // eye's, but never vanish where the eye still sees them.
    var level = mesh;
    let dm = distance(center, params.eye.xyz);
    let d = dm / max(sz, 1e-3) * params.lod_scale;
    var ds = d;
    if (shadow) {
        // Near the eye a caster is the level the eye sees, so nothing it shades on itself moves.
        ds = d * mix(1.0, bitcast<f32>(params.target_list.w), smoothstep(CASTER_SAME, 2.0 * CASTER_SAME, dm));
    }
    let sunward = dot(params.planes[4].xyz, center) + params.planes[4].w + radius < params.eye.w;
    var fade = 0u;
    var coarser = LOD_NONE;
    for (var k = 0u; k < 8u; k++) {
        if (level.lod_next == LOD_NONE) {
            break;
        }
        let fades = !shadow || (params.target_list.x == 1u && level.lod_next < LOD_NOTHING
            && (meshes[level.lod_next].bucket_count >> 24u) != IMPOSTOR);
        let share = switch_share(level, ds, dm, radius / max(mesh.sphere.w, 1e-6));
        let lo = select(1.0, 1.0 - LOD_BAND, fades);
        if (share < lo) {
            break;
        }
        if (fades && share < 1.0) {
            let t = (share - lo) / LOD_BAND;
            fade = max(u32(round(t * 127.0)), 1u);
            coarser = level.lod_next;
            break;
        }
        if (level.lod_next == LOD_NOTHING) {
            if (shadow && d < level.lod_switch) {
                break;
            }
            return;
        }
        // Nor turn to a picture where the eye still sees its shape, unless it lies wholly sunward
        // of the cascade's receivers.
        let picture = (meshes[level.lod_next].bucket_count >> 24u) == IMPOSTOR;
        if (shadow && !sunward && picture && d < level.lod_switch) {
            break;
        }
        level = meshes[level.lod_next];
        if (k == 0u && !shadow) {
            atomicAdd(&stats[3], 1u);
        }
    }
    // Toward the loaded land's edge a tree hands over to its card, which its far ring draws.
    let edge = params.eye.w;
    if (!shadow && edge > 0.0 && dm > edge * (1.0 - EDGE_BAND)
        && (level.bucket_count >> 24u) != IMPOSTOR) {
        var at = level;
        var card = LOD_NONE;
        for (var k = 0u; k < 8u; k++) {
            if (at.lod_next >= LOD_NOTHING) {
                break;
            }
            let next = meshes[at.lod_next];
            if ((next.bucket_count >> 24u) == IMPOSTOR) {
                card = at.lod_next;
                break;
            }
            at = next;
        }
        if (card != LOD_NONE) {
            let t = clamp((dm - edge * (1.0 - EDGE_BAND)) / (edge * EDGE_BAND), 0.0, 1.0);
            let code = max(u32(round(t * 127.0)), 1u);
            if (t >= 1.0) {
                level = meshes[card];
                coarser = LOD_NONE;
                fade = 0u;
            } else if (coarser == card) {
                fade = max(fade, code);
            } else {
                // Of a pair crossfading, the level more of it shows.
                if (coarser < LOD_NOTHING && fade >= 64u) {
                    level = meshes[coarser];
                }
                coarser = card;
                fade = code;
            }
        }
    }
    if (!shadow) {
        atomicAdd(&stats[0], 1u);
    }
    append(i, level, fade, shadow);
    if (coarser < LOD_NOTHING) {
        append(i, meshes[coarser], fade | 0x80u, shadow);
    }
}

// Appends instance `i` to every bucket of `level`, the level's billboard mode above the material
// and its crossfade code (0 none; under 0x80 the finer level's share faded, from it the coarser's
// shown) between.
fn append(i: u32, level: MeshInfo, fade: u32, shadow: bool) {
    let turn = (level.bucket_count & 0xFF000000u) | (fade << 16u);
    for (var k = 0u; k < (level.bucket_count & 0xFFFFFFu); k++) {
        let b = mesh_buckets[level.first_bucket + k];
        let a = b + params.target_list.y;
        let slot = atomicAdd(&args[a].instance_count, 1u);
        let info = buckets[b];
        visible[params.target_list.z + info.list_base + slot] = vec2<u32>(i, info.material | turn);
        if (!shadow) {
            atomicAdd(&stats[1], args[a].index_count / 3u);
        }
    }
}
