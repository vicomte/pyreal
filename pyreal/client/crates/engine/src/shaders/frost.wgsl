// The world behind the interface's see-through grounds, made small and blurred, for the composite
// to lay under them (UI Blur).

struct Frost {
    // The output's pixel size (1/w, 1/h), the target's texel size.
    texel: vec4<f32>,
    // x: the blur's width (sigma) in target texels; y: the taps each side; z: output pixels a
    // target texel spans; w unused.
    blur: vec4<f32>,
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var lin: sampler;
@group(0) @binding(2) var<uniform> F: Frost;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

// A target texel: the mean of the output pixels it spans, one filtered tap for each two by two.
@fragment
fn fs_down(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = p.xy * F.texel.zw;
    let o = F.texel.xy;
    let n = max(i32(F.blur.z) / 2, 1);
    var c = vec3<f32>(0.0);
    for (var j = 0; j < n; j = j + 1) {
        for (var i = 0; i < n; i = i + 1) {
            let at = vec2<f32>(f32(2 * i - n + 1), f32(2 * j - n + 1));
            c = c + textureSampleLevel(src, lin, uv + at * o, 0.0).rgb;
        }
    }
    return vec4<f32>(c / f32(n * n), 1.0);
}

fn blur(p: vec2<f32>, dir: vec2<i32>) -> vec4<f32> {
    let at = vec2<i32>(p);
    let top = vec2<i32>(textureDimensions(src)) - vec2<i32>(1);
    let n = i32(F.blur.y);
    let k = -0.5 / (F.blur.x * F.blur.x);
    var sum = vec3<f32>(0.0);
    var weight = 0.0;
    for (var i = -n; i <= n; i = i + 1) {
        let w = exp(f32(i * i) * k);
        sum = sum + w * textureLoad(src, clamp(at + dir * i, vec2<i32>(0), top), 0).rgb;
        weight = weight + w;
    }
    return vec4<f32>(sum / weight, 1.0);
}

@fragment
fn fs_across(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return blur(p.xy, vec2<i32>(1, 0));
}

@fragment
fn fs_down_blur(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return blur(p.xy, vec2<i32>(0, 1));
}
