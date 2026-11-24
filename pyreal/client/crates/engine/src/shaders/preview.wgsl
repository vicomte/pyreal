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
    // A creature's light's word and flags (`preview::PreviewDraw::glow`); nought for none.
    @location(9) @interpolate(flat) glow_word: u32,
    @location(10) @interpolate(flat) glow_flags: u32,
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
    // x: seconds on the space's clock; y: how many lamps.
    time: vec4<f32>,
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
// The lamps its bodies carry, the world's records (`lamps::lamp_bytes`).
@group(0) @binding(2) var<storage, read> lamps: array<Lamp>;

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
    out.glow_word = r.material.z;
    out.glow_flags = r.material.w;
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

// The bodies' lamps' outdoor light against the directional light's.
const PREVIEW_LAMP_SHARE: f32 = 0.25;
// How brightly a lamp's glowing part shows (`lamp_light.wgsl`'s `LAMP_GLOW`).
const PREVIEW_LAMP_GLOW: f32 = 1.5;

// The light of the bodies' lamps on a point at `world` facing `n`, as the world's outdoor lamps
// light it (`lamp_light.wgsl`), flickering: x the light it takes, y the glow round a lamp's flame
// that shows over its colour.
fn preview_lamps(world: vec3<f32>, n: vec3<f32>) -> array<vec3<f32>, 2> {
    var lit = vec3<f32>(0.0);
    var glow = vec3<f32>(0.0);
    for (var k = 0u; k < u32(pf.time.y); k++) {
        let l = lamps[k];
        let to = l.position - world;
        let d = max(length(to), 1e-2);
        if (d >= l.range) {
            continue;
        }
        let flame = flicker(l.flags & 255u, l.slot >> 8u, l.range);
        let took = clamp((dot(n, to / d) + 0.1) / 1.1, 0.0, 1.0);
        lit += outdoor_lamp(l, d) * flame * took;
        // Its glass and flame, a hand round it.
        let part = select(0.5, 0.35, (l.flags & 2048u) != 0u) * select(1.0, 0.0, (l.flags & 255u) == 2u);
        if (d < part) {
            let g = 1.0 - d / part;
            glow = max(glow, outdoor_colour(l) * flame * g * g * (3.0 - 2.0 * g) * outdoor_share(l));
        }
    }
    return array<vec3<f32>, 2>(lit * PREVIEW_LAMP_SHARE, glow * PREVIEW_LAMP_GLOW);
}

// The particle pass's names for a creature's light (`particles.wgsl`'s `glow_seen`).
struct PartOut {
    uv: vec2<f32>,
    material: u32,
    glow: vec4<f32>,
    flags: u32,
    alpha: f32,
    world: vec3<f32>,
}

struct Seen {
    colour: vec4<f32>,
    additive: bool,
}

// An additive picture's light, as the world shows it on screen: never dimmer than its picture.
fn additive_light(c: vec3<f32>, emissive: f32, world: vec3<f32>) -> vec3<f32> {
    return c * (1.0 + emissive);
}

// A creature's light seen from `in`, from its word as `particles.wgsl`'s `vs_particle` reads it.
fn preview_glow(in: VsOut) -> vec4<f32> {
    let w = in.glow_word;
    var p: PartOut;
    p.uv = in.uv;
    p.material = in.material;
    p.flags = in.glow_flags;
    p.alpha = f32(w & 255u) / 255.0;
    p.world = in.world;
    let g = vec3<f32>(f32(w >> 28u), f32((w >> 24u) & 15u), f32((w >> 20u) & 15u)) / 15.0;
    p.glow = vec4<f32>(pow(g, vec3<f32>(2.2)),
                       floor(f32((w >> 8u) & 255u) / 255.0 * 254.0) + f32((w >> 16u) & 15u) / 15.0 * 0.999);
    let s = glow_seen(p, 1.0);
    let light = s.colour.rgb * s.colour.a;
    // Its hue kept where its heart passes white, as the world's bloom would carry it.
    let shown = light / max(max(light.r, max(light.g, light.b)), 1.0);
    return vec4<f32>(encode(shown), 0.0);
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
        let lamp = preview_lamps(in.world, n);
        lit = c.rgb * (pf.ambient.rgb + pf.light.rgb * d + lamp[0] + lamp[1]);
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
    if ((m.flags & M_ADDITIVE) != 0u) {
        // Added light, over nothing kept behind it: its whole picture, or weighed by its alpha
        // where it has one (an Undead's flame, an Empyrean's eyes), as the world adds it.
        let k = select(1.0, c.a, (m.flags & M_ALPHA) != 0u) * in.fade;
        return vec4<f32>(encode(clamp(lit * k, vec3<f32>(0.0), vec3<f32>(1.0))), 0.0);
    }
    a *= in.fade;
    return vec4<f32>(encode(clamp(lit, vec3<f32>(0.0), vec3<f32>(1.0))) * a, a);
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    if ((in.glow_flags & (1u << 30u)) != 0u) {
        return preview_glow(in);
    }
    return shade(in);
}
