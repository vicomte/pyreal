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
    @location(4) world: vec3<f32>,
    // The vertex's own place, for a face's skin noise.
    @location(5) local: vec3<f32>,
    // A face picture's weight from its border (crate::face_detail::shape_head), else 1.
    @location(6) border: f32,
    // The axes the picture is held to its edge along: a face's both, the rest none.
    @location(7) @interpolate(flat) hold: u32,
    // A model's baked shade inside what it wears, 0 to 1.
    @location(8) inside: f32,
}

struct PreviewFrame {
    view_proj: mat4x4<f32>,
    // xyz: toward the light; w unused.
    light_dir: vec4<f32>,
    // rgb: the light's colour times its intensity; w unused.
    light: vec4<f32>,
    // rgb: the ambient light; w unused.
    ambient: vec4<f32>,
    // xyz: the eye; w unused.
    eye: vec4<f32>,
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
    let h = vec4<f32>(face_push(r.material.x, v.position, oct_decode(v.normal), v.uv, v.extra), 1.0);
    let world = vec3<f32>(dot(r.m0, h), dot(r.m1, h), dot(r.m2, h));
    let n = oct_decode(v.normal);
    var out: VsOut;
    out.clip = pf.view_proj * vec4<f32>(world, 1.0);
    out.normal = vec3<f32>(dot(r.m0.xyz, n), dot(r.m1.xyz, n), dot(r.m2.xyz, n));
    out.uv = v.uv;
    out.material = r.material.x;
    out.fade = bitcast<f32>(r.material.y);
    out.world = world;
    out.local = v.position;
    out.hold = 0u;
    out.border = 1.0;
    if ((materials[r.material.x].flags & M_FACE) != 0u) {
        out.border = 1.0 - f32((v.extra >> 8u) & 255u) / 255.0;
        // A face picture lies inside its picture: held at its edges, its far side never bleeds in.
        out.hold = 3u;
    }
    out.inside = f32(v.extra2 & 255u) / 255.0;
    return out;
}

fn encode(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

// A tangent-space normal `t` on the picture laid over a surface of normal `n` (gbuffer.wgsl's).
fn preview_bend(n: vec3<f32>, t: vec3<f32>, wdx: vec3<f32>, wdy: vec3<f32>, dx: vec2<f32>,
        dy: vec2<f32>) -> vec3<f32> {
    let p2 = cross(wdy, n);
    let p1 = cross(n, wdx);
    let tu = p2 * dx.x + p1 * dy.x;
    let tv = p2 * dx.y + p1 * dy.y;
    let s = inverseSqrt(max(max(dot(tu, tu), dot(tv, tv)), 1e-20));
    return normalize(tu * s * t.x - tv * s * t.y + n * t.z);
}

// A face's light through skin, as the lighting pass's.
const PREVIEW_SKIN_WRAP: f32 = 0.35;
const PREVIEW_SKIN_THROUGH: vec3<f32> = vec3<f32>(0.75, 0.32, 0.22);

fn shade(in: VsOut) -> vec4<f32> {
    let m = materials[in.material];
    let dx = dpdx(in.uv);
    let dy = dpdy(in.uv);
    let wdx = dpdx(in.world);
    let wdy = dpdy(in.world);
    var c = m.base_color;
    if ((m.flags & M_HAS_BASE) != 0u) {
        c *= pic_grad(m.tex_base, in.uv, dx, dy, in.hold);
    }
    let is_face = (m.flags & M_FACE) != 0u;
    var face = Face(c, 0.0, 0.0, 0.0, vec3<f32>(0.0, 0.0, 1.0));
    if (is_face) {
        face = face_detail(m, c, in.uv, dx, dy, in.hold, in.local, wdx, wdy, in.border);
        c = face.c;
    }
    if (c.a < m.alpha_cutoff) {
        discard;
    }
    var lit = c.rgb;
    if ((m.flags & M_LUMINOUS) == 0u) {
        c = vec4<f32>(c.rgb * (1.0 - INSIDE_SHADE * in.inside), c.a);
        var n = normalize(in.normal);
        if (is_face) {
            n = preview_bend(n, face.t, wdx, wdy, dx, dy);
        }
        let ndl = dot(n, pf.light_dir.xyz);
        let d = max(ndl, 0.0);
        lit = c.rgb * (pf.ambient.rgb + pf.light.rgb * d);
        if (is_face) {
            // Light through skin past the shadow edge, reddened; and the sheen of skin, hair and
            // wet eyes from the one light.
            let wrap = max((ndl + PREVIEW_SKIN_WRAP) / (1.0 + PREVIEW_SKIN_WRAP), 0.0) - d;
            lit += c.rgb * PREVIEW_SKIN_THROUGH * pf.light.rgb * wrap * face.skin;
            let v = normalize(pf.eye.xyz - in.world);
            let h = normalize(pf.light_dir.xyz + v);
            let rough = mix(m.roughness, face.rough, face.region);
            let r4 = rough * rough * rough * rough;
            let shine = clamp(2.0 / max(r4, 1e-4) - 2.0, 2.0, 1024.0);
            let spec = pow(max(dot(n, h), 0.0), shine) * (shine + 8.0) / 25.13 * 0.04;
            lit += pf.light.rgb * min(spec, 4.0) * d * face.region;
        }
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
