// Flowers and stones among the grass (scatter.wgsl), drawn into the G-buffer from the clumps'
// list. A flower: a stem of two segments bent by the same wind as the grass and pushed aside by
// bodies, a leaf either side of it and a head of six petals (rays round an eye, a cup, a spike or
// a flat umbel) in its own colour. A stone: a low faceted rock of two rings and a top, its corners
// jittered by its seed, laid on the ground's slope and sunk into it, darker underneath. Both are
// lit as the ground under them is and cast no shadow.

// Their materials in the ids target, which no material takes.
const FLOWER_ID: u32 = 0xFFFFFF01u;
const STONE_ID: u32 = 0xFFFFFF02u;
// A flower's stem and leaves (linear), and the stem's width, metres.
const FLOWER_STEM: vec3<f32> = vec3<f32>(0.075, 0.1, 0.035);
const STEM_WIDTH: f32 = 0.009;

// A colour from the clumps' word (`BLADE_RANGE`).
fn scatter_unpack(word: u32) -> vec3<f32> {
    return vec3<f32>(vec3<u32>(word, word >> 10u, word >> 20u) & vec3<u32>(1023u)) / 1023.0 * BLADE_RANGE;
}

struct FlowerCorner {
    p: vec3<f32>,
    n: vec3<f32>,
    // 1 on a petal, 0 on the stem and leaves; how much of the eye's colour; its brightness.
    petal: f32,
    eye: f32,
    shade: f32,
}

// A point `up` (0 the foot, 1 the head) along a stem from `root` to `tip` bent through `bend`.
fn stem_point(root: vec3<f32>, bend: vec3<f32>, tip: vec3<f32>, up: f32) -> vec3<f32> {
    let s = 1.0 - up;
    return s * s * root + 2.0 * s * up * bend + up * up * tip;
}

// Corner `vertex` of flower `cl` at `t` seconds into the clock's wrap, `raise` times its height;
// pushed by the bodies where they stand, or where they stood last frame (`before`).
fn flower_corner(cl: Clump, vertex: u32, t: f32, raise: f32, before: bool) -> FlowerCorner {
    let h0 = cover_mix(cl.seed);
    let h1 = cover_mix(h0);
    let h2 = cover_mix(h1);
    let root = cl.at.xyz;
    let tall = max(cl.at.w * raise, 0.01);
    let facing = cover_unit(h0) * COVER_TAU;
    let fwd = vec2<f32>(cos(facing), sin(facing));
    let side = vec3<f32>(-fwd.y, fwd.x, 0.0);
    let up = vec3<f32>(0.0, 0.0, 1.0);
    // Leaning its own way and with the wind, its length kept.
    var reach = fwd * tall * sin(mix(0.05, 0.3, cover_unit(h1))) + cover_tip(root.xy, tall, h0, t);
    let l = length(reach);
    if (l > 0.8 * tall) {
        reach *= 0.8 * tall / l;
    }
    let rise = sqrt(max(tall * tall - dot(reach, reach), 0.04 * tall * tall));
    let tip = root + vec3<f32>(reach, rise);
    let bend = root + vec3<f32>(reach * 0.2, rise * 0.55);
    let toward = normalize(tip - bend);
    var out = FlowerCorner(vec3<f32>(0.0), up, 0.0, 0.0, 1.0);
    var lift = 1.0;
    if (vertex < 12u) {
        // The stem: two segments, its face turned to the eye.
        let c = vertex % 6u;
        let level = vertex / 6u + u32(c == 2u || c == 3u || c == 5u);
        let s = select(-1.0, 1.0, c == 1u || c == 4u || c == 5u);
        let along = f32(level) * 0.5;
        let at = stem_point(root, bend, tip, along);
        var across = cross(toward, frame.camera.xyz - at);
        across = select(side, normalize(across), dot(across, across) > 1e-8);
        out.p = at + across * s * STEM_WIDTH * 0.5 * (1.0 - 0.4 * along);
        out.n = normalize(mix(normalize(cross(across, toward)), up, 0.6));
        out.shade = mix(0.55, 1.0, along);
        lift = along;
    } else if (vertex < 18u) {
        // A leaf either side, from low on the stem out and up.
        let s = select(-1.0, 1.0, vertex >= 15u);
        let c = (vertex - 12u) % 3u;
        let out_dir = side * s;
        let lo = stem_point(root, bend, tip, 0.04) - out_dir * 0.006;
        let hi = stem_point(root, bend, tip, 0.04) + out_dir * 0.006;
        let leaf_tip = stem_point(root, bend, tip, 0.3) + out_dir * tall * 0.14;
        var ps = array<vec3<f32>, 3>(lo, hi, leaf_tip);
        out.p = ps[c];
        out.n = normalize(up + out_dir * 0.3);
        out.shade = select(0.8, 1.2, c == 2u);
        lift = select(0.0, 0.3, c == 2u);
    } else {
        // The head: six petals round the stem's tip, its face tilted its own way.
        let k = (vertex - 18u) / 3u;
        let c = (vertex - 18u) % 3u;
        let shape = cl.look & 3u;
        let dist = distance(frame.camera.xyz, tip);
        let r = cl.wide * (1.0 + dist / 30.0);
        let face = normalize(mix(up, toward, 0.6) + vec3<f32>(fwd * 0.25, 0.0));
        let e1 = normalize(cross(face, side));
        let e2 = cross(face, e1);
        let a = cover_unit(h2) * COVER_TAU + COVER_TAU * f32(k) / 6.0;
        out.petal = 1.0;
        out.n = face;
        if (shape == 2u) {
            // A spike: three narrowing cards crossed along the stem's top.
            let qa = f32(k / 2u) * 1.0472 + a;
            let dq = e1 * cos(qa) + e2 * sin(qa);
            let len = r * 2.6;
            let bl = tip - toward * len * 0.15 - dq * r * 0.5;
            let br = tip - toward * len * 0.15 + dq * r * 0.5;
            let tl = tip + toward * len - dq * r * 0.3;
            let tr = tip + toward * len + dq * r * 0.3;
            var ps = array<vec3<f32>, 6>(bl, br, tl, tl, br, tr);
            let idx = (k & 1u) * 3u + c;
            out.p = ps[idx];
            out.shade = select(0.75, 1.05, idx == 2u || idx == 3u || idx == 5u);
            out.n = normalize(cross(dq, toward));
        } else {
            // Rays round an eye, a cup or an umbel: how wide each petal, how far up its rim.
            var half = 0.2;
            var cup = -0.15;
            var reach_r = r;
            var eye = 1.0;
            if (shape == 1u) {
                half = 0.45;
                cup = 0.45;
                reach_r = r * 0.85;
            } else if (shape == 3u) {
                half = 0.5236;
                cup = -0.1;
                reach_r = r * 1.25;
                eye = 0.3;
            }
            let rim0 = tip + (e1 * cos(a - half) + e2 * sin(a - half)) * reach_r + face * r * cup;
            let rim1 = tip + (e1 * cos(a + half) + e2 * sin(a + half)) * reach_r + face * r * cup;
            var ps = array<vec3<f32>, 3>(tip + face * 0.003, rim0, rim1);
            out.p = ps[c];
            out.eye = select(0.0, eye, c == 0u);
            out.shade = select(1.0, 0.9, c == 0u);
        }
    }
    out.p += pushed_aside(out.p, lift, tall, before);
    // Turned to the eye and up, as the grass is, so it shades as the ground under it.
    var n = out.n;
    if (dot(n, frame.camera.xyz - out.p) < 0.0) {
        n = -n;
    }
    out.n = normalize(mix(n, up, select(0.45, 0.5, out.petal > 0.5)));
    return out;
}

// The colour of a flower's eye, from its petals' and its head's shape.
fn flower_eye(petal: vec3<f32>, shape: u32) -> vec3<f32> {
    if (shape == 0u) {
        // Pale rays round a yellow eye; bright ones round a dark one.
        let pale = min(petal.b / max(petal.r, 1e-4), 1.0);
        return mix(vec3<f32>(0.1, 0.05, 0.015), vec3<f32>(0.4, 0.27, 0.03), smoothstep(0.5, 0.8, pale));
    }
    if (shape == 1u) {
        return petal * 0.45;
    }
    return petal * 0.85;
}

@vertex
fn vs_flower(@builtin(vertex_index) vertex: u32, @builtin(instance_index) i: u32) -> VsOut {
    let cl = grass_clumps[i];
    let raise = bitcast<f32>(pushers.count.y);
    let now = flower_corner(cl, vertex, frame.shore.z, raise, false);
    let was = flower_corner(cl, vertex, frame.shore.w, raise, true);
    var out: VsOut;
    out.clip = frame.view_proj * vec4<f32>(now.p, 1.0);
    out.world = now.p;
    out.normal = now.n;
    out.uv = vec2<f32>(now.petal, now.eye);
    out.cur = out.clip;
    out.prev = frame.prev_view_proj * vec4<f32>(was.p, 1.0);
    out.material = FLOWER_ID;
    out.extra = cl.colour;
    out.shade = now.shade;
    out.cell = 0u;
    out.object = 0u;
    out.fade = 0u;
    out.turn = mix(0.85, 1.15, cover_unit(cover_mix(cl.seed ^ 0x3C6EF372u)));
    out.ax = flower_eye(scatter_unpack(cl.colour), cl.look & 3u);
    out.ay = vec3<f32>(0.0);
    return out;
}

@fragment
fn fs_flower(in: VsOut) -> GOut {
    let petal = mix(scatter_unpack(in.extra), in.ax, clamp(in.uv.y, 0.0, 1.0));
    let c = mix(FLOWER_STEM * in.turn, petal, clamp(in.uv.x, 0.0, 1.0)) * in.shade;
    var g = pack(c, normalize(in.normal), 0.8, 0.0, 0.0, 0u, FLOWER_ID, in.cur, in.prev);
    g.object = 0u;
    return g;
}

// Corner `i` of a stone seeded `seed`, a unit wide (0 to 5 its lower ring, 6 to 11 its upper,
// 12 its top), its foot level with the ground.
fn stone_point(i: u32, seed: u32) -> vec3<f32> {
    let h = cover_mix(seed ^ (i * 0x9E3779B9u));
    let h1 = cover_mix(h);
    let squat = mix(0.28, 0.62, cover_unit(cover_mix(seed ^ 0x27D4EB2Fu)));
    let stretch = mix(1.0, 1.5, cover_unit(cover_mix(seed ^ 0x165667B1u)));
    if (i == 12u) {
        let j = vec2<f32>(cover_unit(h), cover_unit(h1)) - 0.5;
        return vec3<f32>(j.x * 0.4 * stretch, j.y * 0.4, mix(0.8, 1.0, cover_unit(cover_mix(h1))) * squat);
    }
    let upper = i >= 6u;
    let a = (f32(i % 6u) + select(0.0, 0.5, upper) + (cover_unit(h) - 0.5) * 0.7) * COVER_TAU / 6.0;
    let r = select(mix(0.32, 0.58, cover_unit(h1)), mix(0.2, 0.42, cover_unit(h1)), upper);
    let z = select(-0.35, mix(0.25, 0.7, cover_unit(cover_mix(h1))), upper);
    return vec3<f32>(cos(a) * r * stretch, sin(a) * r, z * squat);
}

// Which corner of the stone corner `c` of triangle `t` is: twelve round its side, six atop.
fn stone_index(t: u32, c: u32) -> u32 {
    let i = select(t / 2u, t - 12u, t >= 12u);
    let j = (i + 1u) % 6u;
    var ids = array<u32, 3>(6u + i, 6u + j, 12u);
    if (t < 12u && (t & 1u) == 0u) {
        ids = array<u32, 3>(i, j, 6u + i);
    } else if (t < 12u) {
        ids = array<u32, 3>(6u + i, j, 6u + j);
    }
    return ids[c];
}

@vertex
fn vs_stone(@builtin(vertex_index) vertex: u32, @builtin(instance_index) i: u32) -> VsOut {
    let cl = grass_clumps[i];
    let t = vertex / 3u;
    let c = vertex % 3u;
    let q0 = stone_point(stone_index(t, 0u), cl.seed);
    let q1 = stone_point(stone_index(t, 1u), cl.seed);
    let q2 = stone_point(stone_index(t, 2u), cl.seed);
    var qs = array<vec3<f32>, 3>(q0, q1, q2);
    let q = qs[c];
    let squat = mix(0.28, 0.62, cover_unit(cover_mix(cl.seed ^ 0x27D4EB2Fu)));
    // Each face's own normal, outward.
    var facet = normalize(cross(q1 - q0, q2 - q0));
    if (dot(facet, (q0 + q1 + q2) / 3.0 - vec3<f32>(0.0, 0.0, 0.2 * squat)) < 0.0) {
        facet = -facet;
    }
    // Laid on the ground's slope, turned its own way.
    let slope = (vec2<f32>(f32(cl.look & 255u), f32((cl.look >> 8u) & 255u)) - 127.0) / 127.0;
    let z_axis = normalize(vec3<f32>(-slope, 1.0));
    let turn = cover_unit(cover_mix(cl.seed)) * COVER_TAU;
    var x_axis = vec3<f32>(cos(turn), sin(turn), 0.0);
    x_axis = normalize(x_axis - z_axis * dot(x_axis, z_axis));
    let y_axis = cross(z_axis, x_axis);
    let size = cl.at.w;
    let sink = size * 0.1 * length(slope);
    let world = cl.at.xyz + (x_axis * q.x + y_axis * q.y + z_axis * q.z) * size - vec3<f32>(0.0, 0.0, sink);
    let n = normalize(x_axis * facet.x + y_axis * facet.y + z_axis * facet.z);
    var out: VsOut;
    out.clip = frame.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    out.normal = normalize(mix(n, z_axis, 0.3));
    out.uv = vec2<f32>(0.0);
    out.cur = out.clip;
    out.prev = frame.prev_view_proj * vec4<f32>(world, 1.0);
    out.material = STONE_ID;
    out.extra = cl.colour;
    // Darker toward its foot, each face a little lighter or darker.
    out.shade = mix(0.45, 1.0, smoothstep(-0.35, 0.9, q.z / squat))
        * mix(0.88, 1.1, cover_unit(cover_mix(cl.seed ^ (t * 0x85EBCA6Bu))));
    out.cell = 0u;
    out.object = 0u;
    out.fade = 0u;
    out.turn = 0.0;
    out.ax = vec3<f32>(0.0);
    out.ay = vec3<f32>(0.0);
    return out;
}

@fragment
fn fs_stone(in: VsOut) -> GOut {
    let c = scatter_unpack(in.extra) * in.shade;
    var g = pack(c, normalize(in.normal), 0.75, 0.0, 0.0, 0u, STONE_ID, in.cur, in.prev);
    g.object = 0u;
    return g;
}
