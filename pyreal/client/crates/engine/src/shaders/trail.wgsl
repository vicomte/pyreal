// The trail map (snow.rs): each print in the snow stamped as a boot's sole, how deep it presses
// (red) and the snow heaped round its edge (green), the deepest of the overlapping kept.

struct TrailParams {
    // x: one over how far the map reaches round its centre, metres.
    a: vec4<f32>,
}

struct Print {
    // xy: where its middle lies from the map's centre; zw: the way its toe points.
    a: vec4<f32>,
    // x: -1 a left foot, 1 a right; y: how much it ran; z: how much is left unfilled.
    b: vec4<f32>,
}

@group(0) @binding(0) var<uniform> trail: TrailParams;
@group(0) @binding(1) var<storage, read> prints: array<Print>;

struct TOut {
    @builtin(position) clip: vec4<f32>,
    // Metres along the toe's way and across it toward the foot's inner side.
    @location(0) at: vec2<f32>,
    @location(1) @interpolate(flat) look: vec2<f32>,
}

@vertex
fn vs_print(@builtin(vertex_index) v: u32, @builtin(instance_index) i: u32) -> TOut {
    let p = prints[i];
    let corner = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    )[v % 6u];
    let along = p.a.zw;
    let across = vec2<f32>(-along.y, along.x);
    let local = vec2<f32>(corner.x * 0.24 + 0.01, corner.y * 0.11);
    let at = p.a.xy + along * local.x + across * local.y;
    var out: TOut;
    out.clip = vec4<f32>(at * trail.a.x, 0.0, 1.0);
    // A right foot's inner side is to its left.
    out.at = vec2<f32>(local.x, local.y * p.b.x);
    out.look = p.b.yz;
    return out;
}

// An ellipse's distance, roughly metres, from `q`: middle `c`, half-axes `r`.
fn ellipse(q: vec2<f32>, c: vec2<f32>, r: vec2<f32>) -> f32 {
    return (length((q - c) / r) - 1.0) * min(r.x, r.y);
}

fn soft_min(a: f32, b: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
    return mix(b, a, h) - k * h * (1.0 - h);
}

@fragment
fn fs_print(in: TOut) -> @location(0) vec4<f32> {
    let q = in.at;
    let run = in.look.x;
    let left = in.look.y;
    // The ball of the foot, a little to its inner side, and the heel, joined by the arch.
    let ball = ellipse(q, vec2<f32>(0.05, 0.006), vec2<f32>(0.09, 0.05));
    let heel = ellipse(q, vec2<f32>(-0.09, 0.0), vec2<f32>(0.052, 0.045));
    let d = soft_min(ball, heel, 0.06);
    // Its walls slope: snow crumbles in at the edge.
    let dent = 1.0 - smoothstep(-0.022, 0.008, d);
    // A walker comes down on the heel, a runner on the toe.
    let toe = smoothstep(-0.13, 0.13, q.x);
    let press = mix(mix(1.0, 0.85, toe), mix(0.45, 1.15, toe), run);
    // The snow pushed out round it, thrown forward off a runner's toe.
    var heap = exp(-pow((d - 0.02) / 0.014, 2.0)) * (1.0 - dent);
    heap *= 1.0 + 0.9 * run * smoothstep(0.04, 0.16, q.x);
    return vec4<f32>(dent * press * left, heap * left * left, 0.0, 0.0);
}
