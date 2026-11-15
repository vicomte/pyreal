// The far terrain: one 16 by 16 grid patch per quadtree node, its heights from the field's
// point-sampled mip of the node's level, its vertices morphing toward the next coarser grid near
// the end of the level's range (CDLOD). Inside the near square the exact cells draw the ground.

struct Node {
    origin: vec2<f32>,
    spacing: f32,
    level: u32,
    // Where the morph starts and where it is whole.
    morph: vec2<f32>,
    pad: vec2<f32>,
}

@group(1) @binding(0) var heights: texture_2d<f32>;
@group(1) @binding(1) var colours: texture_2d<f32>;
@group(1) @binding(2) var samp: sampler;
@group(1) @binding(3) var<storage, read> nodes: array<Node>;
@group(1) @binding(4) var<uniform> far: FarParams;

// Catmull-Rom weights at `t`.
fn catmull(t: f32) -> vec4<f32> {
    let t2 = t * t;
    let t3 = t2 * t;
    return 0.5 * vec4<f32>(-t3 + 2.0 * t2 - t, 3.0 * t3 - 5.0 * t2 + 2.0, -3.0 * t3 + 4.0 * t2 + t, t3 - t2);
}

// The height at `xy` on level `level`'s grid, bilinear between its vertices, or with Landscape
// Smoothing a Catmull-Rom surface through them.
fn height_at(xy: vec2<f32>, level: u32) -> f32 {
    let p = (xy - far.origin) / (far.texel * f32(1u << level));
    let last = vec2<i32>(textureDimensions(heights, level)) - 1;
    let i = vec2<i32>(floor(p));
    let f = p - floor(p);
    if (far.smoothed != 0u) {
        let wx = catmull(f.x);
        let wy = catmull(f.y);
        var h = 0.0;
        for (var b = 0; b < 4; b++) {
            var row = 0.0;
            for (var a = 0; a < 4; a++) {
                let at = clamp(i + vec2<i32>(a - 1, b - 1), vec2<i32>(0), last);
                row += wx[a] * textureLoad(heights, at, level).r;
            }
            h += wy[b] * row;
        }
        return h;
    }
    let a = clamp(i, vec2<i32>(0), last);
    let b = clamp(i + 1, vec2<i32>(0), last);
    let h00 = textureLoad(heights, a, level).r;
    let h10 = textureLoad(heights, vec2<i32>(b.x, a.y), level).r;
    let h01 = textureLoad(heights, vec2<i32>(a.x, b.y), level).r;
    let h11 = textureLoad(heights, b, level).r;
    return mix(mix(h00, h10, f.x), mix(h01, h11, f.x), f.y);
}

struct FarOut {
    @builtin(position) @invariant clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) cur: vec4<f32>,
    @location(3) prev: vec4<f32>,
    @location(4) @interpolate(flat) level: u32,
}

@vertex
fn vs_far(@location(0) grid: vec2<f32>, @builtin(instance_index) n: u32) -> FarOut {
    let node = nodes[n];
    let at = node.origin + grid * node.spacing;
    let d = distance(frame.camera.xyz, vec3<f32>(at, height_at(at, node.level)));
    let k = clamp((d - node.morph.x) / (node.morph.y - node.morph.x), 0.0, 1.0);
    let g = grid - fract(grid * 0.5) * 2.0 * k;
    let xy = node.origin + g * node.spacing;
    let z = height_at(xy, node.level);
    let s = node.spacing;
    let dx = height_at(xy - vec2<f32>(s, 0.0), node.level) - height_at(xy + vec2<f32>(s, 0.0), node.level);
    let dy = height_at(xy - vec2<f32>(0.0, s), node.level) - height_at(xy + vec2<f32>(0.0, s), node.level);
    var out: FarOut;
    let world = vec3<f32>(xy, z);
    out.world = world;
    out.normal = normalize(vec3<f32>(dx, dy, 2.0 * s));
    out.clip = frame.view_proj * vec4<f32>(world, 1.0);
    out.cur = out.clip;
    out.prev = frame.prev_view_proj * vec4<f32>(world, 1.0);
    out.level = node.level;
    return out;
}

struct FarG {
    @location(0) albedo: vec4<f32>,
    @location(1) normal: vec2<f32>,
    @location(2) material: vec4<f32>,
    @location(3) motion: vec2<f32>,
    @location(4) id: u32,
    @location(5) object: u32,
}

fn in_near_square(p: vec2<f32>) -> bool {
    return all(p > far.near_min) && all(p < far.near_max);
}

// The depth prepass: the same vertices, no outputs.
@fragment
fn fs_far_depth(in: FarOut) {
    if (in_near_square(in.world.xy)) {
        discard;
    }
}

@fragment
fn fs_far(in: FarOut) -> FarG {
    if (in_near_square(in.world.xy)) {
        discard;
    }
    let uv = (in.world.xy - far.origin) / (far.texel * far.size) + 0.5 / far.size;
    let s = textureSampleLevel(colours, samp, uv, f32(in.level));
    var c = s.rgb;
    // Under water drawn as water the water's picture is the bed, silt, as the near ground has it.
    if (has(F_WATER)) {
        c = mix(vec3<f32>(0.15, 0.13, 0.09) * clamp(luma(c) / 0.12, 0.6, 1.5), c, s.a);
    }
    var g: FarG;
    g.albedo = vec4<f32>(c, 1.0);
    g.normal = oct_encode(normalize(in.normal));
    g.material = vec4<f32>(0.9, 0.0, 0.0, f32(G_TERRAIN) / 255.0);
    g.motion = motion_of(in.cur, in.prev);
    g.id = far.material;
    g.object = 0u;
    return g;
}
