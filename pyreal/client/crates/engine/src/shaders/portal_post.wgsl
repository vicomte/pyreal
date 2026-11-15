// Portal space's bloom and its last step: the tone map, the colour split, the interface laid over.

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> VsOut {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    var o: VsOut;
    o.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    o.uv = vec2<f32>(x, y);
    return o;
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var src_lin: sampler;

fn karis(c: vec3<f32>) -> f32 {
    return 1.0 / (1.0 + max(c.r, max(c.g, c.b)));
}

@fragment
fn fs_down_first(in: VsOut) -> @location(0) vec4<f32> {
    let px = 1.0 / vec2<f32>(textureDimensions(src));
    let a = textureSampleLevel(src, src_lin, in.uv + px * vec2<f32>(-1.0, -1.0), 0.0).rgb;
    let b = textureSampleLevel(src, src_lin, in.uv + px * vec2<f32>(1.0, -1.0), 0.0).rgb;
    let c = textureSampleLevel(src, src_lin, in.uv + px * vec2<f32>(-1.0, 1.0), 0.0).rgb;
    let d = textureSampleLevel(src, src_lin, in.uv + px * vec2<f32>(1.0, 1.0), 0.0).rgb;
    let wa = karis(a);
    let wb = karis(b);
    let wc = karis(c);
    let wd = karis(d);
    return vec4<f32>((a * wa + b * wb + c * wc + d * wd) / (wa + wb + wc + wd), 1.0);
}

@fragment
fn fs_down(in: VsOut) -> @location(0) vec4<f32> {
    let px = 1.0 / vec2<f32>(textureDimensions(src));
    var s = textureSampleLevel(src, src_lin, in.uv, 0.0).rgb * 0.5;
    s = s + textureSampleLevel(src, src_lin, in.uv + px * vec2<f32>(-1.0, -1.0), 0.0).rgb * 0.125;
    s = s + textureSampleLevel(src, src_lin, in.uv + px * vec2<f32>(1.0, -1.0), 0.0).rgb * 0.125;
    s = s + textureSampleLevel(src, src_lin, in.uv + px * vec2<f32>(-1.0, 1.0), 0.0).rgb * 0.125;
    s = s + textureSampleLevel(src, src_lin, in.uv + px * vec2<f32>(1.0, 1.0), 0.0).rgb * 0.125;
    return vec4<f32>(s, 1.0);
}

@fragment
fn fs_up(in: VsOut) -> @location(0) vec4<f32> {
    let px = 1.0 / vec2<f32>(textureDimensions(src));
    var s = textureSampleLevel(src, src_lin, in.uv, 0.0).rgb * 4.0;
    s = s + textureSampleLevel(src, src_lin, in.uv + px * vec2<f32>(-1.0, 0.0), 0.0).rgb * 2.0;
    s = s + textureSampleLevel(src, src_lin, in.uv + px * vec2<f32>(1.0, 0.0), 0.0).rgb * 2.0;
    s = s + textureSampleLevel(src, src_lin, in.uv + px * vec2<f32>(0.0, -1.0), 0.0).rgb * 2.0;
    s = s + textureSampleLevel(src, src_lin, in.uv + px * vec2<f32>(0.0, 1.0), 0.0).rgb * 2.0;
    s = s + textureSampleLevel(src, src_lin, in.uv + px * vec2<f32>(-1.0, -1.0), 0.0).rgb;
    s = s + textureSampleLevel(src, src_lin, in.uv + px * vec2<f32>(1.0, -1.0), 0.0).rgb;
    s = s + textureSampleLevel(src, src_lin, in.uv + px * vec2<f32>(-1.0, 1.0), 0.0).rgb;
    s = s + textureSampleLevel(src, src_lin, in.uv + px * vec2<f32>(1.0, 1.0), 0.0).rgb;
    return vec4<f32>(s / 16.0, 1.0);
}

// ---- the last step ----

struct Finish {
    // Colour split, vignette, bloom, exposure.
    look: vec4<f32>,
    // Whether the interface is sRGB-encoded, whether there is one, unused, unused.
    ui: vec4<f32>,
}

@group(0) @binding(0) var<uniform> F: Finish;
@group(0) @binding(1) var hdr: texture_2d<f32>;
@group(0) @binding(2) var bloom: texture_2d<f32>;
@group(0) @binding(3) var ui_tex: texture_2d<f32>;
@group(0) @binding(4) var fin_lin: sampler;

fn encode(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

fn decode(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

// The tunnel's picture at `uv`, scaled up through a Catmull-Rom curve: nine bilinear taps that
// keep its thin lines sharper than one.
fn sharp(uv: vec2<f32>) -> vec3<f32> {
    let size = vec2<f32>(textureDimensions(hdr));
    let pos = uv * size;
    let tc = floor(pos - 0.5) + 0.5;
    let f = pos - tc;
    // Never past the four texels about it: no dark rings round the bright lights.
    let i = vec2<i32>(tc);
    let n0 = textureLoad(hdr, i, 0).rgb;
    let n1 = textureLoad(hdr, i + vec2<i32>(1, 0), 0).rgb;
    let n2 = textureLoad(hdr, i + vec2<i32>(0, 1), 0).rgb;
    let n3 = textureLoad(hdr, i + vec2<i32>(1, 1), 0).rgb;
    let lo = min(min(n0, n1), min(n2, n3));
    let hi = max(max(n0, n1), max(n2, n3));
    let w0 = f * (-0.5 + f * (1.0 - 0.5 * f));
    let w1 = 1.0 + f * f * (-2.5 + 1.5 * f);
    let w2 = f * (0.5 + f * (2.0 - 1.5 * f));
    let w3 = f * f * (-0.5 + 0.5 * f);
    let w12 = w1 + w2;
    let a = (tc - 1.0) / size;
    let b = (tc + w2 / w12) / size;
    let e = (tc + 2.0) / size;
    var c = textureSampleLevel(hdr, fin_lin, vec2<f32>(a.x, a.y), 0.0).rgb * w0.x * w0.y;
    c = c + textureSampleLevel(hdr, fin_lin, vec2<f32>(b.x, a.y), 0.0).rgb * w12.x * w0.y;
    c = c + textureSampleLevel(hdr, fin_lin, vec2<f32>(e.x, a.y), 0.0).rgb * w3.x * w0.y;
    c = c + textureSampleLevel(hdr, fin_lin, vec2<f32>(a.x, b.y), 0.0).rgb * w0.x * w12.y;
    c = c + textureSampleLevel(hdr, fin_lin, vec2<f32>(b.x, b.y), 0.0).rgb * w12.x * w12.y;
    c = c + textureSampleLevel(hdr, fin_lin, vec2<f32>(e.x, b.y), 0.0).rgb * w3.x * w12.y;
    c = c + textureSampleLevel(hdr, fin_lin, vec2<f32>(a.x, e.y), 0.0).rgb * w0.x * w3.y;
    c = c + textureSampleLevel(hdr, fin_lin, vec2<f32>(b.x, e.y), 0.0).rgb * w12.x * w3.y;
    c = c + textureSampleLevel(hdr, fin_lin, vec2<f32>(e.x, e.y), 0.0).rgb * w3.x * w3.y;
    return clamp(c, lo, hi);
}

@fragment
fn fs_finish(in: VsOut) -> @location(0) vec4<f32> {
    let d = in.uv - 0.5;
    var c = sharp(in.uv);
    // The colours split towards the edges only in the rush at the end.
    if (F.look.x > 0.0) {
        let split = F.look.x * dot(d, d);
        c.r = textureSampleLevel(hdr, fin_lin, in.uv - d * split, 0.0).r;
        c.b = textureSampleLevel(hdr, fin_lin, in.uv + d * split, 0.0).b;
    }
    c = c + textureSampleLevel(bloom, fin_lin, in.uv, 0.0).rgb * F.look.z;
    c = c * (1.0 - F.look.y * dot(d, d) * 2.0);
    var mapped = vec3<f32>(1.0) - exp(-max(c, vec3<f32>(0.0)) * F.look.w);
    if (F.ui.y > 0.5) {
        let u = textureLoad(ui_tex, vec2<i32>(in.pos.xy), 0);
        if (F.ui.x > 0.5) {
            mapped = decode(u.rgb + (1.0 - u.a) * encode(mapped));
        } else {
            mapped = u.rgb + (1.0 - u.a) * mapped;
        }
    }
    // A little noise against banding in the dark gradients.
    let k = fract(sin(dot(in.pos.xy, vec2<f32>(12.9898, 78.233))) * 43758.5453) - 0.5;
    mapped = decode(clamp(encode(mapped) + k / 255.0, vec3<f32>(0.0), vec3<f32>(1.0)));
    return vec4<f32>(mapped, 1.0);
}
