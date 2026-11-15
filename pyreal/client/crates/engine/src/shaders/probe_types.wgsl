// The bounce light's probes: a grid round the eye, each probe the light arriving from every way
// as first-order spherical harmonics per colour, and how far it is to be trusted. The grid scrolls
// with the eye a cell at a time; a probe's slot is its cell modulo the grid.

struct ProbeGrid {
    // The grid's first cell (its lowest corner), in cells; w: the probes' lamps.
    base: vec4<i32>,
    // Probes across, along, up; w: the frame's share of the probes being traced (its index).
    dims: vec4<u32>,
    // The spacing, metres (x, y, z); w: the least of a new trace a settled probe takes.
    spacing: vec4<f32>,
}

// One probe: red, green and blue (each the constant then the three linear terms), then its
// cell (xyz) and its weight (w: 0 untraced or buried, 1 trusted, eased between), and its traces
// so far (life.x).
struct Probe {
    r: vec4<f32>,
    g: vec4<f32>,
    b: vec4<f32>,
    state: vec4<f32>,
    life: vec4<f32>,
}

fn probe_slot(cell: vec3<i32>, dims: vec3<u32>) -> u32 {
    let d = vec3<i32>(dims);
    let s = ((cell % d) + d) % d;
    return u32(s.x) + u32(s.y) * dims.x + u32(s.z) * dims.x * dims.y;
}

// The irradiance (scene units) a probe gives a surface facing `n`: the cosine lobe's convolution
// of the radiance (stored times pi), A0 = pi and A1 = 2 pi / 3, over pi.
fn probe_eval(p: Probe, n: vec3<f32>) -> vec3<f32> {
    let y0 = 0.282095;
    let y1 = 0.488603;
    let k1 = 2.0 / 3.0;
    let sh = vec4<f32>(y0, y1 * k1 * n.x, y1 * k1 * n.y, y1 * k1 * n.z);
    return max(vec3<f32>(dot(p.r, sh), dot(p.g, sh), dot(p.b, sh)), vec3<f32>(0.0));
}
