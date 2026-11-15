// Bloom: a chain of halving downsamples (thirteen taps, the first taking only what is past white
// and weighted against fireflies) and tent upsamples each blended into the level below, in
// compute. The levels' weights sum to one, the finer ones heaviest.

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var lin: sampler;
@group(0) @binding(2) var dst: texture_storage_2d<rgba16float, write>;
// The upsample's level of the down chain, blended into.
@group(0) @binding(3) var add: texture_2d<f32>;
// The world's depth (0 where the sky is), read by the first level.
@group(0) @binding(4) var t_depth: texture_depth_2d;

// Where the glow begins, exposed (the target is drawn pre-exposed), and its knee's half width.
const THRESHOLD: f32 = 1.0;
const KNEE: f32 = 0.5;
// The share of each level the coarser ones take.
const SCATTER: f32 = 0.7;
// What the sky blooms as, of its light: a twilight sky far past white over half the frame would
// haze the land, while the sun's disc still glows.
const SKY: f32 = 0.0625;

fn s(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(src, lin, uv, 0.0).rgb;
}

// What of `c` is past white, eased in over the knee; by its brightest channel, so red lava counts.
fn past(c: vec3<f32>) -> vec3<f32> {
    let m = max(c.r, max(c.g, c.b));
    let soft = clamp(m - THRESHOLD + KNEE, 0.0, 2.0 * KNEE);
    let w = max(soft * soft / (4.0 * KNEE), m - THRESHOLD) / max(m, 1e-4);
    return c * w;
}

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn karis(c: vec3<f32>) -> f32 {
    return 1.0 / (1.0 + luma(c));
}

// A tap of the source: the first level takes only what is past white, the sky dimmed.
fn tap(uv: vec2<f32>, first: bool) -> vec3<f32> {
    if (first) {
        let size = vec2<i32>(textureDimensions(t_depth));
        let px = clamp(vec2<i32>(uv * vec2<f32>(size)), vec2<i32>(0), size - 1);
        let sky = select(1.0, SKY, textureLoad(t_depth, px, 0) <= 0.0);
        return past(s(uv) * sky);
    }
    return s(uv);
}

fn down_at(id: vec2<u32>, first: bool) {
    let size = textureDimensions(dst);
    if (any(id >= size)) {
        return;
    }
    let t = 1.0 / vec2<f32>(textureDimensions(src));
    let uv = (vec2<f32>(id) + 0.5) / vec2<f32>(size);
    let a = tap(uv + t * vec2<f32>(-2.0, -2.0), first);
    let b = tap(uv + t * vec2<f32>(0.0, -2.0), first);
    let c = tap(uv + t * vec2<f32>(2.0, -2.0), first);
    let d = tap(uv + t * vec2<f32>(-2.0, 0.0), first);
    let e = tap(uv, first);
    let f = tap(uv + t * vec2<f32>(2.0, 0.0), first);
    let g = tap(uv + t * vec2<f32>(-2.0, 2.0), first);
    let h = tap(uv + t * vec2<f32>(0.0, 2.0), first);
    let k = tap(uv + t * vec2<f32>(2.0, 2.0), first);
    let l = tap(uv + t * vec2<f32>(-1.0, -1.0), first);
    let m = tap(uv + t * vec2<f32>(1.0, -1.0), first);
    let n = tap(uv + t * vec2<f32>(-1.0, 1.0), first);
    let o = tap(uv + t * vec2<f32>(1.0, 1.0), first);
    var r: vec3<f32>;
    if (first) {
        let g0 = (a + b + d + e) * 0.25;
        let g1 = (b + c + e + f) * 0.25;
        let g2 = (d + e + g + h) * 0.25;
        let g3 = (e + f + h + k) * 0.25;
        let g4 = (l + m + n + o) * 0.25;
        let w0 = karis(g0) * 0.125;
        let w1 = karis(g1) * 0.125;
        let w2 = karis(g2) * 0.125;
        let w3 = karis(g3) * 0.125;
        let w4 = karis(g4) * 0.5;
        r = (g0 * w0 + g1 * w1 + g2 * w2 + g3 * w3 + g4 * w4) / max(w0 + w1 + w2 + w3 + w4, 1e-4);
    } else {
        r = e * 0.125 + (a + c + g + k) * 0.03125 + (b + d + f + h) * 0.0625
            + (l + m + n + o) * 0.125;
    }
    // A not-a-number texel never spreads.
    r = select(vec3<f32>(0.0), r, r == r);
    textureStore(dst, id, vec4<f32>(clamp(r, vec3<f32>(0.0), vec3<f32>(60000.0)), 1.0));
}

@compute @workgroup_size(8, 8)
fn down_first(@builtin(global_invocation_id) id: vec3<u32>) {
    down_at(id.xy, true);
}

@compute @workgroup_size(8, 8)
fn down(@builtin(global_invocation_id) id: vec3<u32>) {
    down_at(id.xy, false);
}

@compute @workgroup_size(8, 8)
fn up(@builtin(global_invocation_id) gid: vec3<u32>) {
    let id = gid.xy;
    let size = textureDimensions(dst);
    if (any(id >= size)) {
        return;
    }
    let t = 1.0 / vec2<f32>(textureDimensions(src));
    let uv = (vec2<f32>(id) + 0.5) / vec2<f32>(size);
    var r = s(uv) * 4.0;
    r += (s(uv + vec2<f32>(-t.x, 0.0)) + s(uv + vec2<f32>(t.x, 0.0)) + s(uv + vec2<f32>(0.0, -t.y))
        + s(uv + vec2<f32>(0.0, t.y))) * 2.0;
    r += s(uv - t) + s(uv + vec2<f32>(t.x, -t.y)) + s(uv + vec2<f32>(-t.x, t.y)) + s(uv + t);
    let below = textureLoad(add, vec2<i32>(id), 0).rgb;
    textureStore(dst, id, vec4<f32>(mix(below, r / 16.0, SCATTER), 1.0));
}
