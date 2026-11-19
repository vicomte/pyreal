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

// [`pic_grad`] at mip `level`.
fn pic_level(slot: u32, uv: vec2<f32>, level: f32, hold: u32) -> vec4<f32> {
    switch (hold & 3u) {
        case 1u: { return textureSampleLevel(textures[slot], samp_clamp_u, uv, level); }
        case 2u: { return textureSampleLevel(textures[slot], samp_clamp_v, uv, level); }
        case 3u: { return textureSampleLevel(textures[slot], samp_clamp, uv, level); }
        default: { return textureSampleLevel(textures[slot], samp_repeat, uv, level); }
    }
}
