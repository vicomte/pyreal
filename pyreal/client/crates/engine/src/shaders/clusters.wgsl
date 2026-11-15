// The light clusters: the view cut into tiles across and depth slices along (logarithmic), and
// for each cluster the frame's lamps whose reach touches its box, in view space. One workgroup a
// cluster; its invocations share the lamps between them.

@group(1) @binding(0) var<storage, read> lamps: array<Lamp>;
@group(1) @binding(1) var<storage, read_write> cluster_counts: array<u32>;
@group(1) @binding(2) var<storage, read_write> cluster_lamps: array<u32>;

var<workgroup> found: atomic<u32>;

// The view distance slice `k` of the clusters begins at.
fn slice_start(k: u32) -> f32 {
    let n = frame.cluster_z.x;
    let f = frame.cluster_z.y;
    return n * pow(f / n, f32(k) / f32(frame.cluster.z));
}

// The view-space point under target pixel `p` at view distance `z`.
fn view_point(p: vec2<f32>, z: f32) -> vec3<f32> {
    let ndc = vec2<f32>(p.x / frame.params.z * 2.0 - 1.0, 1.0 - p.y / frame.params.w * 2.0);
    return vec3<f32>(ndc.x * frame.cluster_z.z * z, ndc.y * frame.cluster_z.w * z, -z);
}

@compute @workgroup_size(64)
fn build(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_index) li: u32) {
    let tiles = frame.cluster.xy;
    let cluster = wg.x + wg.y * tiles.x + wg.z * tiles.x * tiles.y;
    if (li == 0u) {
        atomicStore(&found, 0u);
    }
    let tile = f32(frame.lists.y);
    let p0 = vec2<f32>(wg.xy) * tile;
    let p1 = min(p0 + tile, frame.params.zw);
    let z0 = slice_start(wg.z);
    let z1 = slice_start(wg.z + 1u);
    var lo = vec3<f32>(1e30);
    var hi = vec3<f32>(-1e30);
    for (var k = 0u; k < 8u; k++) {
        let p = select(p0, p1, vec2<bool>((k & 1u) != 0u, (k & 2u) != 0u));
        let q = view_point(p, select(z0, z1, (k & 4u) != 0u));
        lo = min(lo, q);
        hi = max(hi, q);
    }
    workgroupBarrier();
    let count = frame.cluster.w;
    for (var i = li; i < count; i += 64u) {
        let l = lamps[i];
        let c = (frame.view * vec4<f32>(l.position, 1.0)).xyz;
        let d = max(max(lo - c, c - hi), vec3<f32>(0.0));
        if (dot(d, d) <= l.range * l.range) {
            let k = atomicAdd(&found, 1u);
            if (k < CLUSTER_MOST) {
                cluster_lamps[cluster * CLUSTER_MOST + k] = i;
            }
        }
    }
    workgroupBarrier();
    if (li == 0u) {
        cluster_counts[cluster] = min(atomicLoad(&found), CLUSTER_MOST);
    }
}
