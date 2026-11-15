// The ambient light on a lit point: the probes' bounce and sky light, blended in by how far the
// probes round it are trusted, and the ambient occlusion (half size, read depth-aware).

@group(3) @binding(6) var ao_tex: texture_2d<f32>;
@group(3) @binding(7) var<storage, read> probes: array<Probe>;
@group(3) @binding(8) var<uniform> grid: ProbeGrid;

// The occlusion at target pixel `px`, from the four half-size texels round it.
fn ambient_occlusion(px: vec2<f32>) -> f32 {
    let h = px * 0.5 - 0.5;
    let b = vec2<i32>(floor(h));
    let f = h - floor(h);
    let last = vec2<i32>(textureDimensions(ao_tex)) - 1;
    let here = textureLoad(g_depth, vec2<i32>(px), 0);
    var sum = 0.0;
    var weight = 0.0;
    for (var k = 0; k < 4; k++) {
        let o = vec2<i32>(k & 1, k >> 1u);
        let t = clamp(b + o, vec2<i32>(0), last);
        let dt = textureLoad(g_depth, min(t * 2, vec2<i32>(frame.params.zw) - 1), 0);
        let near = select(1.0 - f.x, f.x, o.x == 1) * select(1.0 - f.y, f.y, o.y == 1);
        let w = near * exp(-abs(dt - here) / max(here * 0.02, 1e-7)) + 1e-4;
        sum += textureLoad(ao_tex, t, 0).r * w;
        weight += w;
    }
    return sum / weight;
}

// The probes' irradiance at `world` on a surface facing `n` (rgb), and how much of it is to be
// trusted (a): trilinear over the eight probes round the point, each weighed by whether it sees
// the point's face and by whether it is trusted.
fn probe_light(world: vec3<f32>, n: vec3<f32>) -> vec4<f32> {
    let p = world + n * 0.3;
    let g = p / grid.spacing.xyz;
    let c0 = vec3<i32>(floor(g));
    let f = g - floor(g);
    var sum = vec3<f32>(0.0);
    var weight = 0.0;
    var trust = 0.0;
    for (var k = 0u; k < 8u; k++) {
        let o = vec3<i32>(i32(k & 1u), i32((k >> 1u) & 1u), i32(k >> 2u));
        let cell = c0 + o;
        let rel = cell - grid.base.xyz;
        if (any(rel < vec3<i32>(0)) || any(rel >= vec3<i32>(grid.dims.xyz))) {
            continue;
        }
        let pr = probes[probe_slot(cell, grid.dims.xyz)];
        if (pr.state.w <= 0.0 || any(vec3<i32>(pr.state.xyz) != cell)) {
            continue;
        }
        let tri = mix(1.0 - f.x, f.x, f32(o.x)) * mix(1.0 - f.y, f.y, f32(o.y))
            * mix(1.0 - f.z, f.z, f32(o.z));
        let to = normalize(vec3<f32>(cell) * grid.spacing.xyz - world);
        let facing = max(dot(to, n) * 0.5 + 0.5, 0.0);
        let w = tri * (facing * facing + 0.02) * pr.state.w;
        sum += probe_eval(pr, n) * w;
        weight += w;
        trust += tri * pr.state.w;
    }
    if (weight <= 0.0) {
        return vec4<f32>(0.0);
    }
    // The grid's last cells fade to the flat sky light, so its edge never shows.
    let rel = (g - vec3<f32>(grid.base.xyz)) / vec3<f32>(grid.dims.xyz);
    let edge = min(min(rel, 1.0 - rel).x, min(rel, 1.0 - rel).y) * f32(grid.dims.x);
    return vec4<f32>(sum / weight, clamp(trust, 0.0, 1.0) * clamp(edge - 0.5, 0.0, 1.0));
}
