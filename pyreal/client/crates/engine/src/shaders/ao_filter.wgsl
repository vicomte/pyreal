// The ambient occlusion's denoise, at its half size: each frame's turned search gathered over the
// frames (reprojected by the motion, dropped where last frame's depth disagrees), then a small
// blur that keeps to the surface (`ao_blur.wgsl`).

// The most frames gathered.
const AO_FRAMES: f32 = 16.0;

@group(1) @binding(0) var ao_raw: texture_2d<f32>;
@group(1) @binding(1) var ao_last: texture_2d<f32>;
@group(1) @binding(2) var g_motion: texture_2d<f32>;
@group(1) @binding(3) var g_depth: texture_depth_2d;
@group(1) @binding(4) var ao_gathered: texture_storage_2d<rgba16float, write>;
// x: the frame's index; y: 1 when there is no history.
@group(1) @binding(5) var<uniform> ao_frame: vec4<u32>;

// This frame's search over last frame's gathered: occlusion, view depth, frames gathered.
@compute @workgroup_size(8, 8)
fn gather(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = vec2<i32>(textureDimensions(ao_gathered));
    let t = vec2<i32>(id.xy);
    if (any(t >= size)) {
        return;
    }
    let cur = textureLoad(ao_raw, t, 0);
    if (cur.g <= 0.0) {
        textureStore(ao_gathered, t, vec4<f32>(1.0, 0.0, 0.0, 1.0));
        return;
    }
    // The search's neighbourhood on the same surface, which the history is held near.
    var lo = cur.r;
    var hi = cur.r;
    for (var k = 0; k < 9; k++) {
        let o = vec2<i32>(k % 3 - 1, k / 3 - 1);
        let q = textureLoad(ao_raw, clamp(t + o, vec2<i32>(0), size - 1), 0);
        if (abs(q.g - cur.g) < cur.g * 0.05) {
            lo = min(lo, q.r);
            hi = max(hi, q.r);
        }
    }
    let full = vec2<i32>(frame.params.zw);
    let px = min(t * 2, full - 1);
    let here = (vec2<f32>(px) + 0.5) / frame.params.zw;
    let uv = here - frame.jitter.xy * vec2<f32>(0.5, -0.5) - textureLoad(g_motion, px, 0).xy;
    // Where it was, in last frame's texels, and how far from the eye it was then.
    let h = (uv * frame.params.zw - 0.5) * 0.5;
    let ndc = vec2<f32>(here.x * 2.0 - 1.0, 1.0 - here.y * 2.0);
    let w = frame.inv_view_proj * vec4<f32>(ndc, textureLoad(g_depth, px, 0), 1.0);
    let was = (frame.prev_view_proj * vec4<f32>(w.xyz / w.w, 1.0)).w;
    let b = vec2<i32>(floor(h));
    let f = h - floor(h);
    var sum = 0.0;
    var frames = 0.0;
    var weight = 0.0;
    if (ao_frame.y == 0u) {
        for (var k = 0; k < 4; k++) {
            let o = vec2<i32>(k & 1, k >> 1u);
            let q = b + o;
            if (any(q < vec2<i32>(0)) || any(q >= size)) {
                continue;
            }
            let last = textureLoad(ao_last, q, 0);
            let near = mix(1.0 - f.x, f.x, f32(o.x)) * mix(1.0 - f.y, f.y, f32(o.y));
            if (last.g > 0.0 && abs(last.g - was) < was * 0.04 + 0.02) {
                sum += last.r * near;
                frames += last.b * near;
                weight += near;
            }
        }
    }
    var ao = cur.r;
    var n = 1.0;
    if (weight > 0.05) {
        let kept = clamp(sum / weight, lo - 0.1, hi + 0.1);
        n = min(frames / weight + 1.0, AO_FRAMES);
        ao = mix(kept, cur.r, 1.0 / n);
    }
    textureStore(ao_gathered, t, vec4<f32>(ao, cur.g, n, 1.0));
}
