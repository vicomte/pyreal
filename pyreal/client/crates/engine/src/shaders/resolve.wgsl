// The temporal resolve: each frame is drawn a fraction of a pixel off the last (eight Halton
// offsets) and laid into a history at the output's size, followed there by each surface's motion,
// held to the colours round it, and dropped where the surface was hidden last frame (the material
// under it then differs) or left the screen. At a render scale under one this rebuilds the
// picture at the output's size; at one it is the anti-aliasing. Ported from render-hifi's
// `fs_upscale`. The spatial path enlarges the picture along its edges (EASU) instead.

struct Resolve {
    // The drawn picture's size and its reciprocal.
    input: vec4<f32>,
    // The output's size and its reciprocal.
    output: vec4<f32>,
    // xy: this frame's jitter, normalised device coordinates; z: 1 when the history may be used;
    // w: the share of the current frame while nothing moves.
    jitter: vec4<f32>,
    // x: the share at and past `y` pixels of motion; z: 1 when last frame's materials are this
    // size and can reject the history; w unused.
    tune: vec4<f32>,
    // This frame's unjittered device coordinates and depth to last frame's clip space.
    reproject: mat4x4<f32>,
}

@group(0) @binding(0) var current: texture_2d<f32>;
@group(0) @binding(1) var depth: texture_depth_2d;
@group(0) @binding(2) var motion: texture_2d<f32>;
@group(0) @binding(3) var ids: texture_2d<u32>;
@group(0) @binding(4) var prev_ids: texture_2d<u32>;
@group(0) @binding(5) var history: texture_2d<f32>;
@group(0) @binding(6) var lin: sampler;
@group(0) @binding(7) var<uniform> rs: Resolve;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

fn to_ycocg(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        0.25 * c.r + 0.5 * c.g + 0.25 * c.b,
        0.5 * c.r - 0.5 * c.b,
        -0.25 * c.r + 0.5 * c.g - 0.25 * c.b,
    );
}

fn from_ycocg(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(c.x + c.y - c.z, c.x + c.z, c.x - c.y - c.z);
}

fn inside(p: vec2<i32>) -> vec2<i32> {
    return clamp(p, vec2<i32>(0), vec2<i32>(rs.input.xy) - vec2<i32>(1));
}

// How far the surface at drawn pixel `p` moved since last frame, in texture coordinates, this
// frame's place less the last's: its own where it drew one, else the camera's (the sky).
fn motion_at(p: vec2<i32>) -> vec2<f32> {
    let d = textureLoad(depth, p, 0);
    if (d > 0.0) {
        return textureLoad(motion, p, 0).xy;
    }
    let uv = (vec2<f32>(p) + vec2<f32>(0.5)) * rs.input.zw;
    let now = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0) - rs.jitter.xy;
    let last = rs.reproject * vec4<f32>(now, 0.0, 1.0);
    if (abs(last.w) <= 1e-9) {
        return vec2<f32>(0.0);
    }
    return (now - last.xy / last.w) * vec2<f32>(0.5, -0.5);
}

// The history at `uv` through a Catmull-Rom filter of five bilinear samples.
fn history_at(uv: vec2<f32>) -> vec4<f32> {
    let texel = rs.output.zw;
    let at = uv * rs.output.xy - vec2<f32>(0.5);
    let base = floor(at);
    let f = at - base;
    let w0 = f * (-0.5 + f * (1.0 - 0.5 * f));
    let w1 = 1.0 + f * f * (-2.5 + 1.5 * f);
    let w2 = f * (0.5 + f * (2.0 - 1.5 * f));
    let w3 = f * f * (-0.5 + 0.5 * f);
    let w12 = w1 + w2;
    let o12 = w2 / w12;
    let lo = vec2<f32>(0.5);
    let hi = rs.output.xy - vec2<f32>(0.5);
    let t0 = clamp(base - vec2<f32>(0.5), lo, hi) * texel;
    let t3 = clamp(base + vec2<f32>(2.5), lo, hi) * texel;
    let t12 = clamp(base + vec2<f32>(0.5) + o12, lo, hi) * texel;
    var c = textureSampleLevel(history, lin, vec2<f32>(t12.x, t0.y), 0.0) * (w12.x * w0.y);
    c += textureSampleLevel(history, lin, vec2<f32>(t0.x, t12.y), 0.0) * (w0.x * w12.y);
    c += textureSampleLevel(history, lin, vec2<f32>(t12.x, t12.y), 0.0) * (w12.x * w12.y);
    c += textureSampleLevel(history, lin, vec2<f32>(t3.x, t12.y), 0.0) * (w3.x * w12.y);
    c += textureSampleLevel(history, lin, vec2<f32>(t12.x, t3.y), 0.0) * (w12.x * w3.y);
    let weight = w12.x * w0.y + w0.x * w12.y + w12.x * w12.y + w3.x * w12.y + w12.x * w3.y;
    return max(c / max(weight, 1e-4), vec4<f32>(0.0));
}

// `h` brought inside the box `lo`..`hi` along the line toward its middle.
fn clip_into(lo: vec3<f32>, hi: vec3<f32>, h: vec3<f32>) -> vec3<f32> {
    let middle = 0.5 * (hi + lo);
    let half = 0.5 * (hi - lo) + vec3<f32>(1e-5);
    let v = h - middle;
    let units = abs(v / half);
    let most = max(units.x, max(units.y, units.z));
    if (most > 1.0) {
        return middle + v / most;
    }
    return h;
}

// Whether last frame's material round drawn place `at` (pixels) was `id`: else the surface now
// seen was hidden there, or is new.
fn was_seen(at: vec2<f32>, id: u32) -> bool {
    let base = vec2<i32>(floor(at - vec2<f32>(0.5)));
    for (var k = 0; k < 4; k++) {
        let t = inside(base + vec2<i32>(k & 1, k >> 1u));
        if (textureLoad(prev_ids, t, 0).r == id) {
            return true;
        }
    }
    return false;
}

// The blend at the output's size: the drawn pixels round where an output pixel falls, weighted by
// how near their jittered samples stand, and the history followed to it. The current frame's
// share goes by how near its nearest sample stood, so the history fills over the jitter's
// sequence. Written with the frames' worth it holds in `w`.
@fragment
fn fs_temporal(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let drawn = rs.input.xy;
    let uv = p.xy * rs.output.zw;
    let at = uv * drawn;
    let shift = vec2<f32>(-rs.jitter.x, rs.jitter.y) * drawn * 0.5;
    let middle = vec2<i32>(floor(at - shift));
    let per_drawn = rs.output.xy / drawn;
    var sum = vec3<f32>(0.0);
    var weight = 0.0;
    var hit = 0.0;
    var m1 = vec3<f32>(0.0);
    var m2 = vec3<f32>(0.0);
    var lo = vec3<f32>(1e9);
    var hi = vec3<f32>(-1e9);
    var nearest = -1.0;
    var nearest_at = middle;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let q = inside(middle + vec2<i32>(x, y));
            let c = to_ycocg(textureLoad(current, q, 0).rgb);
            let off = vec2<f32>(q) + vec2<f32>(0.5) + shift - at;
            let w = exp(-2.29 * dot(off, off));
            sum += c * w;
            weight += w;
            let o = off * per_drawn;
            hit = max(hit, exp(-2.29 * dot(o, o)));
            m1 += c;
            m2 += c * c;
            lo = min(lo, c);
            hi = max(hi, c);
            // Reverse depth: the nearest is the largest.
            let d = textureLoad(depth, q, 0);
            if (d > nearest) {
                nearest = d;
                nearest_at = q;
            }
        }
    }
    let centre = sum / max(weight, 1e-4);
    let moved = motion_at(nearest_at);
    let last = uv - moved;
    var kept = rs.jitter.z;
    if (any(last < vec2<f32>(0.0)) || any(last > vec2<f32>(1.0))) {
        kept = 0.0;
    }
    if (kept > 0.5 && rs.tune.z > 0.5 && nearest > 0.0) {
        let id = textureLoad(ids, nearest_at, 0).r;
        if (!was_seen((vec2<f32>(nearest_at) + vec2<f32>(0.5)) - moved * drawn, id)) {
            kept = 0.0;
        }
    }
    let pixels = length(moved * rs.output.xy);
    let steady = mix(rs.jitter.w, rs.tune.x, clamp(pixels / max(rs.tune.y, 1e-3), 0.0, 1.0));
    let most = 1.0 / max(steady, 1e-3);
    let mean = m1 / 9.0;
    let spread = sqrt(max(m2 / 9.0 - mean * mean, vec3<f32>(0.0)));
    let box_lo = max(lo, mean - spread * 1.25);
    let box_hi = min(hi, mean + spread * 1.25);
    let held = history_at(last);
    let h = clip_into(box_lo, box_hi, to_ycocg(held.rgb));
    let frames = min(held.w, most) * kept;
    var share = hit / max(frames + hit, 1e-4);
    if (kept < 0.5) {
        share = 1.0;
    }
    let held_now = min(frames + hit, most);
    return vec4<f32>(max(from_ycocg(mix(h, centre, share)), vec3<f32>(0.0)), held_now);
}

// The drawn picture enlarged to the output along its edges.
@fragment
fn fs_easu(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let top = vec2<i32>(rs.input.xy) - vec2<i32>(1);
    let at = p.xy * rs.output.zw * rs.input.xy;
    return vec4<f32>(easu(current, at, top, false), 1.0);
}
