// The UI's picture: textured triangles in clip space, in the order asked, each laid over what is
// there premultiplied by its alpha. Everything is in the pictures' own sRGB encoding and blended as
// those numbers, as the old fixed-function devices blended a UI.

struct VsIn {
    @location(0) position: vec3<f32>,
    // 0xAARRGGBB, as the UI packs it.
    @location(1) colour: u32,
    @location(2) uv: vec2<f32>,
}

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) colour: vec4<f32>,
    @location(1) uv: vec2<f32>,
}

@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

@vertex
fn vs(v: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = vec4<f32>(v.position.xy, 0.0, 1.0);
    let c = v.colour;
    out.colour = vec4<f32>(
        f32((c >> 16u) & 0xFFu),
        f32((c >> 8u) & 0xFFu),
        f32(c & 0xFFu),
        f32((c >> 24u) & 0xFFu),
    ) / 255.0;
    out.uv = v.uv;
    return out;
}

// The picture times the vertex colour.
fn image_of(colour: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    let c = textureSample(picture, samp, uv) * colour;
    return vec4<f32>(c.rgb * c.a, c.a);
}

// Text: the vertex's colour, the picture giving only how much of the pixel a glyph covers.
fn text_of(colour: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    let a = textureSample(picture, samp, uv).a * colour.a;
    return vec4<f32>(colour.rgb * a, a);
}

// A selection highlight: the UI's picture cannot see the world under it, so a pale wash stands in
// for inverting it.
fn invert_of(colour: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    let a = textureSample(picture, samp, uv).a * colour.a * 0.45;
    return vec4<f32>(vec3<f32>(a), a);
}

// A picture already premultiplied by its alpha (a preview space's), faded by the vertex alpha.
fn premultiplied_of(colour: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    return textureSample(picture, samp, uv) * colour.a;
}

@fragment
fn fs_image(in: VsOut) -> @location(0) vec4<f32> {
    return image_of(in.colour, in.uv);
}

@fragment
fn fs_text(in: VsOut) -> @location(0) vec4<f32> {
    return text_of(in.colour, in.uv);
}

@fragment
fn fs_invert(in: VsOut) -> @location(0) vec4<f32> {
    return invert_of(in.colour, in.uv);
}

@fragment
fn fs_premultiplied(in: VsOut) -> @location(0) vec4<f32> {
    return premultiplied_of(in.colour, in.uv);
}

// What stands in the world: drawn once the world is, against its depth (reverse Z with no far
// plane), each pixel hidden where the world stands nearer than the triangle's distance.
struct WorldDepth {
    // xy: the world's depth's size over the picture's; z: the near plane, metres.
    params: vec4<f32>,
}

@group(1) @binding(0) var world_depth: texture_depth_2d;
@group(1) @binding(1) var<uniform> wd: WorldDepth;

struct WorldOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) colour: vec4<f32>,
    @location(1) uv: vec2<f32>,
    // How far from the eye, metres; zero is never hidden.
    @location(2) distance: f32,
}

@vertex
fn vs_world(v: VsIn) -> WorldOut {
    var out: WorldOut;
    out.clip = vec4<f32>(v.position.xy, 0.0, 1.0);
    let c = v.colour;
    out.colour = vec4<f32>(
        f32((c >> 16u) & 0xFFu),
        f32((c >> 8u) & 0xFFu),
        f32(c & 0xFFu),
        f32((c >> 24u) & 0xFFu),
    ) / 255.0;
    out.uv = v.uv;
    out.distance = v.position.z;
    return out;
}

fn hidden(in: WorldOut) -> bool {
    if (in.distance <= 0.0) {
        return false;
    }
    let size = vec2<i32>(textureDimensions(world_depth));
    let at = clamp(vec2<i32>(in.clip.xy * wd.params.xy), vec2<i32>(0), size - 1);
    let d = textureLoad(world_depth, at, 0);
    if (d <= 0.0) {
        return false;
    }
    let world = wd.params.z / d;
    // A little grace, so a mark lying on the ground is not lost in it.
    return world < in.distance * 0.98 - 0.15;
}

@fragment
fn fs_world_image(in: WorldOut) -> @location(0) vec4<f32> {
    let c = image_of(in.colour, in.uv);
    if (hidden(in)) { discard; }
    return c;
}

@fragment
fn fs_world_text(in: WorldOut) -> @location(0) vec4<f32> {
    let c = text_of(in.colour, in.uv);
    if (hidden(in)) { discard; }
    return c;
}

@fragment
fn fs_world_invert(in: WorldOut) -> @location(0) vec4<f32> {
    let c = invert_of(in.colour, in.uv);
    if (hidden(in)) { discard; }
    return c;
}

@fragment
fn fs_world_premultiplied(in: WorldOut) -> @location(0) vec4<f32> {
    let c = premultiplied_of(in.colour, in.uv);
    if (hidden(in)) { discard; }
    return c;
}
