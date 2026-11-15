// Each further level from the one before.

@group(0) @binding(0) var src_level: texture_2d<f32>;
@group(0) @binding(1) var dst_level: texture_storage_2d<r32float, write>;

@compute @workgroup_size(8, 8)
fn hiz_down(@builtin(global_invocation_id) id: vec3<u32>) {
    let dst = textureDimensions(dst_level);
    if (any(id.xy >= dst)) {
        return;
    }
    let src = textureDimensions(src_level);
    let s = span(id.xy, src, dst);
    var d = 1.0;
    for (var y = 0u; y < s.y; y++) {
        for (var x = 0u; x < s.x; x++) {
            let at = min(id.xy * 2u + vec2<u32>(x, y), src - 1u);
            d = min(d, textureLoad(src_level, at, 0).r);
        }
    }
    textureStore(dst_level, id.xy, vec4<f32>(d, 0.0, 0.0, 0.0));
}
