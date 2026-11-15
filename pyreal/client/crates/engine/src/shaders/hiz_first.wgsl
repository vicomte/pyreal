// Level 0 from the depth target.

@group(0) @binding(0) var src_depth: texture_depth_2d;
@group(0) @binding(1) var dst_first: texture_storage_2d<r32float, write>;

@compute @workgroup_size(8, 8)
fn hiz_first(@builtin(global_invocation_id) id: vec3<u32>) {
    let dst = textureDimensions(dst_first);
    if (any(id.xy >= dst)) {
        return;
    }
    let src = textureDimensions(src_depth);
    let s = span(id.xy, src, dst);
    var d = 1.0;
    for (var y = 0u; y < s.y; y++) {
        for (var x = 0u; x < s.x; x++) {
            let at = min(id.xy * 2u + vec2<u32>(x, y), src - 1u);
            d = min(d, textureLoad(src_depth, at, 0));
        }
    }
    textureStore(dst_first, id.xy, vec4<f32>(d, 0.0, 0.0, 0.0));
}
