// The order-independent translucency laid over the picture: the weighted mean of what was drawn
// at each pixel, covering it by one less what still shows through, and the additive light on top.

@group(1) @binding(0) var oit_accum: texture_2d<f32>;
@group(1) @binding(1) var oit_reveal: texture_2d<f32>;
@group(1) @binding(2) var oit_add: texture_2d<f32>;

// Premultiplied: blended as `src + dst * (1 - src.a)`.
@fragment
fn fs_oit(in: FsIn) -> @location(0) vec4<f32> {
    let px = vec2<i32>(in.pos.xy);
    let reveal = textureLoad(oit_reveal, px, 0).r;
    let add = textureLoad(oit_add, px, 0).rgb;
    if (reveal >= 0.999 && all(add <= vec3<f32>(0.0))) {
        discard;
    }
    let a = textureLoad(oit_accum, px, 0);
    let c = min(a.rgb / clamp(a.a, 1e-5, 5e4), vec3<f32>(HDR_MOST));
    let cover = 1.0 - reveal;
    return vec4<f32>(min(c * cover + add, vec3<f32>(HDR_MOST)), cover);
}
