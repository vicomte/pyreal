// The material table: the bindless pictures, their samplers, the material records and the ground
// pictures' companions.

@group(2) @binding(0) var textures: binding_array<texture_2d<f32>, 16384>;
@group(2) @binding(1) var samp_repeat: sampler;
@group(2) @binding(2) var samp_clamp: sampler;
// Held to the picture's edge across and repeated down, and the other way round.
@group(2) @binding(5) var samp_clamp_u: sampler;
@group(2) @binding(6) var samp_clamp_v: sampler;
@group(2) @binding(3) var<storage, read> materials: array<Material>;
// Per texture slot, the maps that go with a ground picture: normal, surface, steep slopes' picture
// and its metres a repeat (as bits); then where its height is kept (the H_IN bits), and its detail
// grain's picture, metres a repeat and strength (bits; none where the metres are nought).
struct Companions {
    maps: vec4<u32>,
    more: vec4<u32>,
}
@group(2) @binding(4) var<storage, read> companions: array<Companions>;

// A ground picture's height: in its alpha, or in its surface companion's blue.
const H_IN_ALPHA: u32 = 1u;
const H_IN_SURFACE: u32 = 2u;

// A polygon's picture held to its edge across (`HOLD_U`) or down (`HOLD_V`), where it lies within
// one repeat that way: repeating would bleed its far edge into this one (0x8A02012C's wall).
const HOLD_U: u32 = 1u;
const HOLD_V: u32 = 2u;

// Picture `slot` at `uv`, held to its edge along the axes `hold` names.
fn pic_grad(slot: u32, uv: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>, hold: u32) -> vec4<f32> {
    switch (hold & 3u) {
        case 1u: { return textureSampleGrad(textures[slot], samp_clamp_u, uv, dx, dy); }
        case 2u: { return textureSampleGrad(textures[slot], samp_clamp_v, uv, dx, dy); }
        case 3u: { return textureSampleGrad(textures[slot], samp_clamp, uv, dx, dy); }
        default: { return textureSampleGrad(textures[slot], samp_repeat, uv, dx, dy); }
    }
}

// A projected material's share for the planes across x, y and z at normal `n`.
fn tri_weights(n: vec3<f32>) -> vec3<f32> {
    let b = pow(abs(n), vec3<f32>(4.0));
    return b / max(b.x + b.y + b.z, 1e-5);
}

// Picture `slot` laid along the world's three axes at `p`, one repeat over `size`, blended by the
// planes' shares `b`.
fn tri_pic(slot: u32, p: vec3<f32>, wdx: vec3<f32>, wdy: vec3<f32>, size: f32,
           b: vec3<f32>) -> vec4<f32> {
    let k = 1.0 / size;
    return textureSampleGrad(textures[slot], samp_repeat, p.yz * k, wdx.yz * k, wdy.yz * k) * b.x
        + textureSampleGrad(textures[slot], samp_repeat, p.xz * k, wdx.xz * k, wdy.xz * k) * b.y
        + textureSampleGrad(textures[slot], samp_repeat, p.xy * k, wdx.xy * k, wdy.xy * k) * b.z;
}

// `n` bent by normal picture `slot` laid as `tri_pic` lays it, each plane's bend along its own
// two axes, `k` its strength.
fn tri_bend(slot: u32, p: vec3<f32>, wdx: vec3<f32>, wdy: vec3<f32>, size: f32, b: vec3<f32>,
            n: vec3<f32>, k: f32) -> vec3<f32> {
    let s = 1.0 / size;
    let tx = (textureSampleGrad(textures[slot], samp_repeat, p.yz * s, wdx.yz * s, wdy.yz * s).rg * 2.0 - 1.0) * k;
    let ty = (textureSampleGrad(textures[slot], samp_repeat, p.xz * s, wdx.xz * s, wdy.xz * s).rg * 2.0 - 1.0) * k;
    let tz = (textureSampleGrad(textures[slot], samp_repeat, p.xy * s, wdx.xy * s, wdy.xy * s).rg * 2.0 - 1.0) * k;
    return normalize(n + vec3<f32>(0.0, tx.x, tx.y) * b.x + vec3<f32>(ty.x, 0.0, ty.y) * b.y
        + vec3<f32>(tz.x, tz.y, 0.0) * b.z);
}

// [`pic_grad`] at mip `level`.
fn pic_level(slot: u32, uv: vec2<f32>, level: f32, hold: u32) -> vec4<f32> {
    switch (hold & 3u) {
        case 1u: { return textureSampleLevel(textures[slot], samp_clamp_u, uv, level); }
        case 2u: { return textureSampleLevel(textures[slot], samp_clamp_v, uv, level); }
        case 3u: { return textureSampleLevel(textures[slot], samp_clamp, uv, level); }
        default: { return textureSampleLevel(textures[slot], samp_repeat, uv, level); }
    }
}

// How far a face picture's height (its edge map's alpha) pushes the head out at its highest,
// metres from flat (mid-grey); and the map level it is read at, smooth for the cut it moves.
const FACE_PUSH: f32 = 0.024;
const FACE_PUSH_LEVEL: f32 = 1.0;
// A point that may move: one of an eye or mouth picture's (crate::face_detail::shape_head).
const FACE_MOVES: u32 = 65536u;

// A player's head point pushed along its normal by its face picture's height (a beard, a brow,
// the lips, the eyes' domes), as far as its weight (above the low
// byte of its first extra word, kept as how far short of whole) allows: nothing at a picture's
// border, so neighbouring pictures meet unmoved.
fn face_push(material: u32, local: vec3<f32>, normal: vec3<f32>, uv: vec2<f32>, extra: u32) -> vec3<f32> {
    let m = materials[material];
    if ((m.flags & M_FACE) == 0u || (extra & FACE_MOVES) == 0u) {
        return local;
    }
    let w = 1.0 - f32((extra >> 8u) & 255u) / 255.0;
    let e = textureSampleLevel(textures[m.views >> 16u], samp_clamp, uv, FACE_PUSH_LEVEL);
    // An eye's dome stands half as far: an eyeball shows a few millimetres past its lids.
    return local + normal * (e.a - 0.5) * FACE_PUSH * w * (1.0 - 0.5 * e.b);
}

// How dark a model's skin goes where what it wears hides all its sky (`shade_inside`).
const INSIDE_SHADE: f32 = 0.85;
