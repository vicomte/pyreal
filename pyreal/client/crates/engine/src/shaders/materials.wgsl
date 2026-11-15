// The material table: the bindless pictures, their samplers, the material records and the ground
// pictures' companions.

@group(2) @binding(0) var textures: binding_array<texture_2d<f32>, 16384>;
@group(2) @binding(1) var samp_repeat: sampler;
@group(2) @binding(2) var samp_clamp: sampler;
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
