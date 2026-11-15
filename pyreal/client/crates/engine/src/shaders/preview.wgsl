// A preview space: a few meshes from the pools, lit by one distant light and an ambient, drawn into
// a picture of the UI's (sRGB-encoded numbers, premultiplied) over a clear background.

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec2<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) extra: u32,
    @location(4) extra2: u32,
}

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) @interpolate(flat) material: u32,
    // How much of it shows (a particle's fade), 0 to 1.
    @location(3) @interpolate(flat) fade: f32,
}

struct PreviewFrame {
    view_proj: mat4x4<f32>,
    // xyz: toward the light; w unused.
    light_dir: vec4<f32>,
    // rgb: the light's colour times its intensity; w unused.
    light: vec4<f32>,
    // rgb: the ambient light; w unused.
    ambient: vec4<f32>,
}

// One drawn sub-mesh: its transform's rows and its material.
struct Record {
    m0: vec4<f32>,
    m1: vec4<f32>,
    m2: vec4<f32>,
    material: vec4<u32>,
}

@group(0) @binding(0) var<uniform> pf: PreviewFrame;
@group(0) @binding(1) var<storage, read> records: array<Record>;

fn oct_decode(e: vec2<f32>) -> vec3<f32> {
    var n = vec3<f32>(e, 1.0 - abs(e.x) - abs(e.y));
    if (n.z < 0.0) {
        let s = select(vec2<f32>(-1.0), vec2<f32>(1.0), n.xy >= vec2<f32>(0.0));
        n = vec3<f32>((1.0 - abs(n.yx)) * s, n.z);
    }
    return normalize(n);
}

@vertex
fn vs(v: VsIn, @builtin(instance_index) k: u32) -> VsOut {
    let r = records[k];
    let h = vec4<f32>(v.position, 1.0);
    let world = vec3<f32>(dot(r.m0, h), dot(r.m1, h), dot(r.m2, h));
    let n = oct_decode(v.normal);
    var out: VsOut;
    out.clip = pf.view_proj * vec4<f32>(world, 1.0);
    out.normal = vec3<f32>(dot(r.m0.xyz, n), dot(r.m1.xyz, n), dot(r.m2.xyz, n));
    out.uv = v.uv;
    out.material = r.material.x;
    out.fade = bitcast<f32>(r.material.y);
    return out;
}

fn encode(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

fn shade(in: VsOut) -> vec4<f32> {
    let m = materials[in.material];
    var c = m.base_color;
    if ((m.flags & M_HAS_BASE) != 0u) {
        c *= textureSample(textures[m.tex_base], samp_repeat, in.uv);
    }
    if (c.a < m.alpha_cutoff) {
        discard;
    }
    var lit = c.rgb;
    if ((m.flags & M_LUMINOUS) == 0u) {
        let n = normalize(in.normal);
        let d = max(dot(n, pf.light_dir.xyz), 0.0);
        lit = c.rgb * (pf.ambient.rgb + pf.light.rgb * d);
    }
    var a = c.a;
    if ((m.flags & (M_TRANSLUCENT | M_ADDITIVE)) == 0u) {
        a = 1.0;
    }
    if ((m.flags & M_ADDITIVE) != 0u && (m.flags & M_ALPHA) == 0u) {
        // Added light: its whole picture, over nothing kept behind it.
        return vec4<f32>(encode(clamp(lit * in.fade, vec3<f32>(0.0), vec3<f32>(1.0))), 0.0);
    }
    a *= in.fade;
    return vec4<f32>(encode(clamp(lit, vec3<f32>(0.0), vec3<f32>(1.0))) * a, a);
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    return shade(in);
}
