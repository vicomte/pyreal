// The composite at the output's size: the world's picture (rebuilt by the temporal resolve or
// MetalFX, enlarged by EASU, or as drawn) sharpened by its contrast where it was rebuilt, then
// the UI's picture over it, texel for texel, premultiplied.

struct Composite {
    // x: how the picture is read (0 texel for texel, 1 filtered up from the drawn size, 2 already
    // at the output's size and sharpened by RCAS); y: 1 with a UI; z: 1 when the UI is laid over
    // in sRGB encoding; w unused.
    mode: vec4<u32>,
    // x: the sharpening (1 the most); yzw unused.
    tune: vec4<f32>,
    // The picture's size and its reciprocal.
    picture: vec4<f32>,
    // The output's size and its reciprocal.
    output: vec4<f32>,
}

@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var ui: texture_2d<f32>;
@group(0) @binding(2) var lin: sampler;
@group(0) @binding(3) var<uniform> cs: Composite;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

@fragment
fn fs_composite(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let px = vec2<i32>(p.xy);
    let top = vec2<i32>(cs.picture.xy) - vec2<i32>(1);
    var c: vec3<f32>;
    switch cs.mode.x {
        case 1u: {
            // Filtered up, then sharpened by its four neighbours, held to their range.
            let t = cs.picture.zw;
            let uv = p.xy * cs.output.zw;
            c = textureSampleLevel(picture, lin, uv, 0.0).rgb;
            let n = textureSampleLevel(picture, lin, uv + vec2<f32>(0.0, -t.y), 0.0).rgb;
            let s = textureSampleLevel(picture, lin, uv + vec2<f32>(0.0, t.y), 0.0).rgb;
            let w = textureSampleLevel(picture, lin, uv + vec2<f32>(-t.x, 0.0), 0.0).rgb;
            let e = textureSampleLevel(picture, lin, uv + vec2<f32>(t.x, 0.0), 0.0).rgb;
            let lo = min(min(min(n, s), min(w, e)), c);
            let hi = max(max(max(n, s), max(w, e)), c);
            c = clamp(c + cs.tune.x * (c - (n + s + w + e) * 0.25), lo, hi);
        }
        case 2u: {
            c = rcas(picture, px, top, cs.tune.x);
        }
        default: {
            c = textureLoad(picture, clamp(px, vec2<i32>(0), top), 0).rgb;
        }
    }
    // Where the UI covers nothing, the world as it is (past SDR white on an EDR output).
    let u = textureLoad(ui, px, 0);
    if (cs.mode.y != 0u && u.a > 0.0) {
        if (cs.mode.z != 0u) {
            c = srgb_decode(u.rgb + srgb_encode(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0))) * (1.0 - u.a));
        } else {
            c = u.rgb + c * (1.0 - u.a);
        }
    }
    return vec4<f32>(c, 1.0);
}

fn srgb_encode(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

fn srgb_decode(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}
