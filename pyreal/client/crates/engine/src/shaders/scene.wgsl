// The scene's records, for the passes that draw meshes (the material table is materials.wgsl).

@group(1) @binding(0) var<storage, read> instances: array<Instance>;
@group(1) @binding(1) var<storage, read> visible: array<vec2<u32>>;
@group(1) @binding(2) var<storage, read> cells: array<TerrainCell>;
// Room volumes: a box (min with the plane count's bits, max with the reach past which the room
// is not drawn and the ground stays), then planes, inside where dot(n, p) + d >= 0; and each
// terrain cell's list of the volumes over it.
@group(1) @binding(3) var<storage, read> cuts: array<vec4<f32>>;
@group(1) @binding(4) var<storage, read> cut_lists: array<u32>;
