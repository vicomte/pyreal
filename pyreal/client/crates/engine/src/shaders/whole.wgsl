// Laying a picture over a whole target: the UI's over black, with no world drawn (its numbers
// decoded, so an sRGB target stores them as they are), or a finished frame copied.

@group(0) @binding(0) var whole: texture_2d<f32>;

@vertex
fn vs_whole(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

fn decode(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

@fragment
fn fs_ui_alone(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let u = textureLoad(whole, vec2<i32>(p.xy), 0);
    return vec4<f32>(decode(u.rgb), 1.0);
}

@fragment
fn fs_copy(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(textureLoad(whole, vec2<i32>(p.xy), 0).rgb, 1.0);
}
