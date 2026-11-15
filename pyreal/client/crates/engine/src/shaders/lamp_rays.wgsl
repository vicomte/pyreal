// The nearest shadowed lamps' shadows, ray traced: for each texel of a half-size target, a ray
// from the surface toward each of up to four lamps through the world's solid surfaces, its end
// jittered over the lamp's flame so the shadows soften. The lamp's own object (a brazier's bowl,
// a lantern's frame) never shadows it.

@group(1) @binding(0) var g_depth: texture_depth_2d;
@group(1) @binding(1) var g_normal: texture_2d<f32>;
@group(1) @binding(2) var world_rays: acceleration_structure;
@group(1) @binding(3) var shadow_out: texture_storage_2d<rgba8unorm, write>;
@group(1) @binding(4) var<storage, read> lamps: array<Lamp>;
// The lamps in slots 0 to 3 (0xFFFFFFFF for none).
@group(1) @binding(5) var<uniform> slots: vec4<u32>;

fn unproject_px(p: vec2<f32>, d: f32) -> vec3<f32> {
    let ndc = vec2<f32>(p.x / frame.params.z * 2.0 - 1.0, 1.0 - p.y / frame.params.w * 2.0);
    let h = frame.inv_view_proj * vec4<f32>(ndc, d, 1.0);
    return h.xyz / h.w;
}

fn hash(p: vec2<u32>) -> f32 {
    var h = p.x * 1664525u + p.y * 1013904223u;
    h = (h ^ (h >> 16u)) * 2246822519u;
    h = (h ^ (h >> 13u)) * 3266489917u;
    return f32((h ^ (h >> 16u)) & 0xffffu) / 65535.0;
}

@compute @workgroup_size(8, 8)
fn trace(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(shadow_out);
    if (any(id.xy >= size)) {
        return;
    }
    let px = min(id.xy * 2u, vec2<u32>(frame.params.zw) - 1u);
    let d = textureLoad(g_depth, vec2<i32>(px), 0);
    var vis = vec4<f32>(1.0);
    var rq: ray_query;
    if (d > 0.0) {
        let p = vec2<f32>(px) + 0.5;
        let world = unproject_px(p, d);
        let n = oct_decode(textureLoad(g_normal, vec2<i32>(px), 0).xy);
        let dist = distance(world, frame.camera.xyz);
        let origin = world + n * (0.02 + dist * 0.002);
        for (var k = 0u; k < 4u; k++) {
            let at = slots[k];
            if (at == 0xFFFFFFFFu) {
                continue;
            }
            let l = lamps[at];
            let own = l.object & 0xFFFFFFu;
            // Two rays, to opposite points of the flame, a little differently each texel.
            let j = vec3<f32>(hash(id.xy + vec2<u32>(k * 7u, 3u)), hash(id.xy + vec2<u32>(k * 11u, 5u)),
                              hash(id.xy + vec2<u32>(k * 13u, 9u))) - 0.5;
            var lit = 0.0;
            for (var r = 0u; r < 2u; r++) {
                let target_p = l.position + j * select(-0.12, 0.12, r == 0u);
                let to = target_p - origin;
                let len = length(to);
                if (len >= l.range || dot(to, n) <= 0.0) {
                    lit += 0.5;
                    continue;
                }
                let reach = len - 0.1;
                if (reach <= 0.05) {
                    lit += 0.5;
                    continue;
                }
                rayQueryInitialize(&rq, world_rays, RayDesc(0x4u, 0xFFu, 0.0, reach, origin, to / len));
                while (rayQueryProceed(&rq)) {
                    let c = rayQueryGetCandidateIntersection(&rq);
                    if (c.instance_custom_data != own) {
                        rayQueryConfirmIntersection(&rq);
                    }
                }
                let hit = rayQueryGetCommittedIntersection(&rq);
                if (hit.kind == RAY_QUERY_INTERSECTION_NONE) {
                    lit += 0.5;
                }
            }
            vis[k] = lit;
        }
    }
    textureStore(shadow_out, id.xy, vis);
}
