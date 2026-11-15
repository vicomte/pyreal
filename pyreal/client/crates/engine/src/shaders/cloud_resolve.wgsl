// The volumetric clouds' own history: each frame's march (its steps offset afresh each frame) laid
// into what the frames before left, followed as the camera turns and held to the colours round
// it, so the clouds settle soft whatever rebuilds the frame. `cloud_keep` keeps the result.

@group(1) @binding(0) var marched: texture_2d<f32>;
@group(1) @binding(1) var history: texture_2d<f32>;
@group(1) @binding(2) var lin: sampler;
@group(1) @binding(3) var resolved: texture_storage_2d<rgba16float, write>;

// The share of this frame's march in the result.
const CLOUD_NEW: f32 = 0.1;

@compute @workgroup_size(8, 8)
fn cloud_resolve(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(resolved);
    if (any(id.xy >= size)) {
        return;
    }
    let p = vec2<i32>(id.xy);
    let now = textureLoad(marched, p, 0);
    // The colours round it: their mean and spread.
    var m1 = vec4<f32>(0.0);
    var m2 = vec4<f32>(0.0);
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let q = clamp(p + vec2<i32>(dx, dy), vec2<i32>(0), vec2<i32>(size) - 1);
            let c = textureLoad(marched, q, 0);
            m1 += c;
            m2 += c * c;
        }
    }
    m1 /= 9.0;
    let sigma = sqrt(max(m2 / 9.0 - m1 * m1, vec4<f32>(0.0)));
    // Where this texel's line looked last frame: the clouds are far enough to follow the turn alone.
    let uv = (vec2<f32>(id.xy) + 0.5) / vec2<f32>(size);
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let h = frame.inv_view_proj * vec4<f32>(ndc, 1.0, 1.0);
    let d = vec4<f32>(h.xyz / h.w - frame.camera.xyz, 0.0);
    let last = frame.prev_view_proj * d;
    var out = now;
    if (last.w > 1e-6 && air.cloud_storm.w > 0.5) {
        let at = vec2<f32>(last.x / last.w * 0.5 + 0.5, 0.5 - last.y / last.w * 0.5);
        if (all(at >= vec2<f32>(0.0)) && all(at <= vec2<f32>(1.0))) {
            let held = clamp(textureSampleLevel(history, lin, at, 0.0), m1 - 1.25 * sigma, m1 + 1.25 * sigma);
            out = mix(held, now, CLOUD_NEW);
        }
    }
    textureStore(resolved, id.xy, out);
}

@compute @workgroup_size(8, 8)
fn cloud_keep(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(resolved);
    if (any(id.xy >= size)) {
        return;
    }
    textureStore(resolved, id.xy, textureLoad(marched, vec2<i32>(id.xy), 0));
}
