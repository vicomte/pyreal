// A preview space's bodies of fire (`preview.rs`'s fire pass): the world's `body_fire.wgsl`, fed
// the preview's camera, clock and depth through the names the world's frame gives them, its light
// added over the preview's picture (sRGB-encoded numbers, premultiplied).

struct Frame {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    // xyz: the eye.
    camera: vec4<f32>,
    // w: seconds on the space's clock.
    sun_light: vec4<f32>,
    // zw: the picture's size.
    params: vec4<f32>,
}

@group(0) @binding(0) var<uniform> frame: Frame;
// The depth the preview's bodies left.
@group(0) @binding(1) var g_depth: texture_depth_2d;

// A preview has no air between the eye and its bodies.
fn fog_amount(dist: f32) -> f32 {
    return 0.0;
}

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn fire_encode(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

@vertex
fn vs_fire(@builtin(vertex_index) vi: u32, @builtin(instance_index) i: u32) -> FlameOut {
    let f = flames[i];
    let corner = array<vec2<f32>, 6>(vec2<f32>(-1.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(-1.0, 1.0),
                                     vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0))[vi];
    var out: FlameOut;
    out.index = i;
    out.kind = f.flags & 255u;
    out.seed = f.seed;
    // It flutters as its lamp does, as `flames.wgsl`'s `vs_flame` has it.
    out.flick = vec4<f32>(flicker(3u, f.seed, f.reach * 30.0), 0.0);
    out.size = f.height;
    out.clip = body_fire_card(f.base, f.height, corner);
    out.world = f.base;
    return out;
}

@fragment
fn fs_fire(in: FlameOut) -> @location(0) vec4<f32> {
    let light = body_fire_light(in);
    // Its hue kept where its heart passes white, as the world's bloom would carry it.
    let shown = light / max(max(light.r, max(light.g, light.b)), 1.0);
    return vec4<f32>(fire_encode(shown), 0.0);
}
