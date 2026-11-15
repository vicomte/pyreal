// The water's depth laid into the world's, where the water was drawn.

@group(0) @binding(0) var water_depth: texture_depth_2d;

@fragment
fn fs_water_merge(in: FsIn) -> @builtin(frag_depth) f32 {
    let d = textureLoad(water_depth, vec2<i32>(in.pos.xy), 0);
    if (d <= 0.0) {
        discard;
    }
    return d;
}
