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
// Each terrain type's picture (slot, repeats a metre as bits; slot all ones: none), the road's last.
@group(1) @binding(5) var<uniform> far_grounds: array<vec4<u32>, 33>;
@group(2) @binding(0) var textures: binding_array<texture_2d<f32>, 16384>;
@group(2) @binding(1) var samp_repeat: sampler;

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
    var n = normalize(in.normal);
    // The distant ground: every ring bends by the finest heights, takes the near ground's tint and
    // the pictures' grain, then the shared looks.
    if (has(F_DISTANT) && far_heights_on()) {
        let foot = distance(in.world, frame.camera.xyz) * 2.0 * frame.cluster_z.w / frame.params.w;
        n = far_relief(in.world, far_normal(in.world.xy, max(LAND_CELL, foot * 2.0)));
        c *= far_macro_tint(in.world.xy) * far_grain(in.world.xy, dpdx(in.world.xy), dpdy(in.world.xy), foot);
        c = far_look(in.world, c, n);
    }
    var g: FarG;
    g.albedo = vec4<f32>(c, 1.0);
    g.normal = oct_encode(n);
    g.material = vec4<f32>(0.9, 0.0, 0.0, f32(G_TERRAIN) / 255.0);
    g.motion = motion_of(in.cur, in.prev);
    g.id = far.material;
    g.object = 0u;
    return g;
}

// The near ground's large-scale tint (gbuffer.wgsl's `macro_tint`), so the rings meet it.
fn far_macro_tint(p: vec2<f32>) -> vec3<f32> {
    let a = land_noise(p + vec2<f32>(91.3, 12.9), 57.0);
    let b = land_noise(p - vec2<f32>(7.1, 55.3), 197.0);
    let bright = 0.9 + 0.2 * (a * 0.6 + b * 0.4);
    let warm = (b - 0.5) * 0.12;
    return bright * vec3<f32>(1.0 + warm, 1.0, 1.0 - warm);
}

// The ground pictures' own grain, read 6 and 17 times larger about their mean, from the terrain
// type of the nearest vertex; laid as the near ground lays it, so the two meet.
fn far_grain(p: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>, foot: f32) -> vec3<f32> {
    let q = (p - far_rings.origin) / LAND_CELL;
    let word = far_vertex(vec2<i32>(round(q))).word;
    let road = (word >> LW_ROAD_SHIFT) & 3u;
    let e = far_grounds[select(word & LW_TYPE, 32u, road != 0u)];
    if (e.x == 0xFFFFFFFFu) {
        return vec3<f32>(1.0);
    }
    let per_m = bitcast<f32>(e.y);
    let keep = grain_keep(per_m, foot);
    if (keep.x + keep.y <= 0.0) {
        return vec3<f32>(1.0);
    }
    let s = vec2<f32>(1.0, -1.0) * per_m;
    let mean = textureSampleLevel(textures[e.x], samp_repeat, vec2<f32>(0.5), GRAIN_MEAN_LEVEL).rgb;
    var coarse = mean * (1.0 - 0.6 * keep.x - 0.4 * keep.y);
    if (keep.x > 0.0) {
        let s6 = s / 6.0;
        coarse += textureSampleGrad(textures[e.x], samp_repeat, p * s6 + grain_origin(s6), dx * s6, dy * s6).rgb * 0.6 * keep.x;
    }
    if (keep.y > 0.0) {
        let s17 = s / 17.0;
        coarse += textureSampleGrad(textures[e.x], samp_repeat, p * s17 + grain_origin(s17), dx * s17, dy * s17).rgb * 0.4 * keep.y;
    }
    return clamp(coarse / max(mean, vec3<f32>(1e-3)), vec3<f32>(0.45), vec3<f32>(1.7));
}

// Render space's origin's share of a picture laid `s` times a metre (gbuffer.wgsl's `origin_uv`).
fn grain_origin(s: vec2<f32>) -> vec2<f32> {
    return fract(vec2<f32>(origin_block()) * fract(192.0 * s));
}
