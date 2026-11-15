// The gathered occlusion blurred 5 by 5 at its half size, each texel weighed by how near it lies
// to the plane the centre's depth leans along, wider where few frames are gathered yet.

@group(1) @binding(0) var ao_in: texture_2d<f32>;
@group(1) @binding(1) var ao_out: texture_storage_2d<rgba8unorm, write>;

// The view depth's step toward `o` from the centre `c`: the nearer of the two sides.
fn lean(t: vec2<i32>, o: vec2<i32>, c: f32, size: vec2<i32>) -> f32 {
    let a = textureLoad(ao_in, clamp(t + o, vec2<i32>(0), size - 1), 0).g - c;
    let b = c - textureLoad(ao_in, clamp(t - o, vec2<i32>(0), size - 1), 0).g;
    return select(b, a, abs(a) < abs(b));
}

@compute @workgroup_size(8, 8)
fn blur(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = vec2<i32>(textureDimensions(ao_out));
    let t = vec2<i32>(id.xy);
    if (any(t >= size)) {
        return;
    }
    let c = textureLoad(ao_in, t, 0);
    if (c.g <= 0.0) {
        textureStore(ao_out, t, vec4<f32>(1.0));
        return;
    }
    let gx = lean(t, vec2<i32>(1, 0), c.g, size);
    let gy = lean(t, vec2<i32>(0, 1), c.g, size);
    let spread = mix(2.5, 1.4, clamp(c.b / 8.0, 0.0, 1.0));
    var sum = 0.0;
    var weight = 0.0;
    for (var y = -2; y <= 2; y++) {
        for (var x = -2; x <= 2; x++) {
            let q = textureLoad(ao_in, clamp(t + vec2<i32>(x, y), vec2<i32>(0), size - 1), 0);
            if (q.g <= 0.0) {
                continue;
            }
            let plane = c.g + gx * f32(x) + gy * f32(y);
            let w = exp(-f32(x * x + y * y) / (2.0 * spread * spread))
                * exp(-abs(q.g - plane) / (c.g * 0.01 + 0.005));
            sum += q.r * w;
            weight += w;
        }
    }
    textureStore(ao_out, t, vec4<f32>(sum / max(weight, 1e-6)));
}
