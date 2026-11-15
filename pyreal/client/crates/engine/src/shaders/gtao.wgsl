// Ground-truth ambient occlusion (render-hifi's horizon search, after Jimenez et al.), in compute
// at half size over the G-buffer's depth: in a few slices round each texel, the highest horizon
// on either side within a metre or so, integrated against the surface's normal from the depth (a
// material's normal leans into its own grooves and would bury them). The slices turn each frame
// for `ao_filter.wgsl` to gather. Writes the occlusion and the view depth.

@group(1) @binding(0) var g_depth: texture_depth_2d;
@group(1) @binding(1) var ao_out: texture_storage_2d<rgba16float, write>;
// x: the frame's index.
@group(1) @binding(2) var<uniform> ao_frame: vec4<u32>;

const HALF_PI: f32 = 1.57079632;
const SLICES: u32 = 3u;
const STEPS: u32 = 6u;
// The search's radius, metres, and the most it covers on screen, pixels.
const RADIUS: f32 = 1.2;
const MOST_PIXELS: f32 = 96.0;

fn view_point(p: vec2<f32>, d: f32) -> vec3<f32> {
    let ndc = vec2<f32>(p.x / frame.params.z * 2.0 - 1.0, 1.0 - p.y / frame.params.w * 2.0);
    let h = frame.inv_view_proj * vec4<f32>(ndc, d, 1.0);
    return (frame.view * vec4<f32>(h.xyz / h.w, 1.0)).xyz;
}

fn ign(p: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(p, vec2<f32>(0.06711056, 5.83715e-3))));
}

fn point_at(q: vec2<i32>, full: vec2<i32>) -> vec3<f32> {
    let c = clamp(q, vec2<i32>(0), full - 1);
    return view_point(vec2<f32>(c) + 0.5, textureLoad(g_depth, c, 0));
}

// Whether the texel at `q` holds a surface rather than the sky (depth 0).
fn solid_at(q: vec2<i32>, full: vec2<i32>) -> bool {
    return textureLoad(g_depth, clamp(q, vec2<i32>(0), full - 1), 0) > 0.0;
}

// One way's step across the surface at `p`: the nearer of the two sides, a side on the sky never
// taken, and a step along the screen at `p`'s depth when both are sky.
fn depth_step(px: vec2<i32>, p: vec3<f32>, d: f32, way: vec2<i32>, full: vec2<i32>) -> vec3<f32> {
    let after_ok = solid_at(px + way, full);
    let before_ok = solid_at(px - way, full);
    if (!after_ok && !before_ok) {
        return view_point(vec2<f32>(px + way) + 0.5, d) - p;
    }
    let after = point_at(px + way, full);
    let before = point_at(px - way, full);
    let take_after = after_ok && (!before_ok || abs(after.z - p.z) < abs(p.z - before.z));
    return select(p - before, after - p, take_after);
}

// The surface's normal from the depth round `px`: each way, the nearer of the two sides.
fn depth_normal(px: vec2<i32>, p: vec3<f32>, d: f32, full: vec2<i32>) -> vec3<f32> {
    let dx = depth_step(px, p, d, vec2<i32>(1, 0), full);
    let dy = depth_step(px, p, d, vec2<i32>(0, 1), full);
    return normalize(cross(dy, dx));
}

@compute @workgroup_size(8, 8)
fn gtao(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(ao_out);
    if (any(id.xy >= size)) {
        return;
    }
    let full = vec2<i32>(frame.params.zw);
    let px = min(vec2<i32>(id.xy * 2u), full - 1);
    let d = textureLoad(g_depth, px, 0);
    if (d <= 0.0) {
        textureStore(ao_out, id.xy, vec4<f32>(1.0, 0.0, 0.0, 1.0));
        return;
    }
    let centre = vec2<f32>(px) + 0.5;
    let p = view_point(centre, d);
    let depth = -p.z;
    var n = depth_normal(px, p, d, full);
    let v = normalize(-p);
    if (dot(n, v) < 0.0) {
        n = -n;
    }
    // Pixels a metre covers at this depth.
    let per_metre = frame.params.w * 0.5 / (frame.cluster_z.w * depth);
    let screen_radius = min(RADIUS * per_metre, MOST_PIXELS);
    if (screen_radius < 2.0) {
        textureStore(ao_out, id.xy, vec4<f32>(1.0, depth, 0.0, 1.0));
        return;
    }
    let origin = p + n * (depth * 0.001);
    let turn = 5.588238 * f32(ao_frame.x % 64u);
    let rot = ign(vec2<f32>(id.xy) + turn);
    let jitter = ign(vec2<f32>(id.xy) + vec2<f32>(5.3, 1.7) + turn);
    var visibility = 0.0;
    for (var s = 0u; s < SLICES; s++) {
        let phi = (f32(s) + rot) * PI / f32(SLICES);
        let omega = vec2<f32>(cos(phi), sin(phi));
        let along = view_point(centre + omega, d) - p;
        let dir = normalize(along);
        let ortho = dir - dot(dir, v) * v;
        let axis = normalize(cross(ortho, v));
        let pn = n - axis * dot(n, axis);
        let pn_len = length(pn);
        let sign_n = select(-1.0, 1.0, dot(ortho, pn) >= 0.0);
        let cos_n = clamp(dot(pn, v) / max(pn_len, 1e-5), 0.0, 1.0);
        let ang_n = sign_n * acos(cos_n);
        let low_plus = cos(ang_n + HALF_PI);
        let low_minus = cos(ang_n - HALF_PI);
        var hc_plus = low_plus;
        var hc_minus = low_minus;
        for (var k = 0u; k < STEPS; k++) {
            var t = (f32(k) + fract(jitter + f32(k) * 0.618034)) / f32(STEPS);
            t = t * t * screen_radius + 1.5;
            for (var side = 0; side < 2; side++) {
                let q = centre + omega * t * select(-1.0, 1.0, side == 0);
                let qi = vec2<i32>(q);
                if (any(qi < vec2<i32>(0)) || any(qi >= full)) {
                    continue;
                }
                let dq = textureLoad(g_depth, qi, 0);
                if (dq <= 0.0) {
                    continue;
                }
                let delta = view_point(vec2<f32>(qi) + 0.5, dq) - origin;
                let len = length(delta);
                // Fades to nothing past the radius, so a far wall does not shade the floor.
                let w = clamp((RADIUS * 1.25 - len) / (RADIUS * 0.5), 0.0, 1.0);
                if (side == 0) {
                    hc_plus = max(hc_plus, mix(low_plus, dot(delta / len, v), w));
                } else {
                    hc_minus = max(hc_minus, mix(low_minus, dot(delta / len, v), w));
                }
            }
        }
        var h0 = -acos(clamp(hc_minus, -1.0, 1.0));
        var h1 = acos(clamp(hc_plus, -1.0, 1.0));
        h0 = ang_n + clamp(h0 - ang_n, -HALF_PI, HALF_PI);
        h1 = ang_n + clamp(h1 - ang_n, -HALF_PI, HALF_PI);
        let sin_n = sin(ang_n);
        let arc0 = (cos_n + 2.0 * h0 * sin_n - cos(2.0 * h0 - ang_n)) * 0.25;
        let arc1 = (cos_n + 2.0 * h1 * sin_n - cos(2.0 * h1 - ang_n)) * 0.25;
        visibility += pn_len * (arc0 + arc1);
    }
    let ao = clamp(visibility / f32(SLICES), 0.0, 1.0);
    textureStore(ao_out, id.xy, vec4<f32>(ao, depth, 0.0, 1.0));
}
