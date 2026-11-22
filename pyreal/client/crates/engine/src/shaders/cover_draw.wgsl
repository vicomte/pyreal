// Ground cover drawn into the G-buffer from the list the cover pass wrote: each plant's level of
// its kind, turned and scaled, bent by the wind (`cover_tip`) and pushed aside by bodies passing; its card
// (the last level) turned to face the eye. The material is in the vertices. Its colour is drawn
// toward the ground's it stands on, so it sits on the ground as the ground's own grass.

struct Pushers {
    // x: how many bodies push the grass aside; y: how tall the cover grows (f32 bits), 1 the
    // pack's own.
    count: vec4<u32>,
    // Where each stands (its feet), and where it stood last frame.
    at: array<vec4<f32>, 8>,
    was: array<vec4<f32>, 8>,
}

@group(3) @binding(0) var<storage, read> cover_plants: array<CoverPlant>;
@group(3) @binding(1) var<storage, read> cover_kinds: array<CoverKind>;
@group(3) @binding(2) var<uniform> pushers: Pushers;
// Each terrain type's ground colour (linear; w 1 where known).
@group(3) @binding(3) var<uniform> ground_colours: array<vec4<f32>, 32>;

// How much of its colour the cover takes from the ground under it, and how much brighter than
// the ground it is.
const COVER_TINT: f32 = 0.7;
const COVER_LIFT: f32 = 1.5;
// How much of Grass Height's raise widens a plant too.
const COVER_WIDEN: f32 = 0.4;
// The tint's scale in the word it is handed on in (ten bits a channel).
const TINT_RANGE: f32 = 4.0;

// The ground's colour at `p` across its cell's corners, w how much of it is known.
fn ground_colour(p: vec2<f32>) -> vec4<f32> {
    if (!has_land()) {
        return vec4<f32>(0.0);
    }
    let q = (p - frame.land.xy) / LAND_CELL;
    let c = vec2<i32>(floor(q));
    let f = clamp(q - floor(q), vec2<f32>(0.0), vec2<f32>(1.0));
    let w = vec4<f32>((1.0 - f.x) * (1.0 - f.y), f.x * (1.0 - f.y), (1.0 - f.x) * f.y, f.x * f.y);
    let words = vec4<u32>(land_vertex(c).word, land_vertex(c + vec2<i32>(1, 0)).word,
                          land_vertex(c + vec2<i32>(0, 1)).word,
                          land_vertex(c + vec2<i32>(1, 1)).word);
    var sum = vec4<f32>(0.0);
    for (var k = 0u; k < 4u; k++) {
        let g = ground_colours[words[k] & LW_TYPE];
        sum += vec4<f32>(g.rgb, 1.0) * g.w * w[k];
    }
    return vec4<f32>(sum.rgb / max(sum.w, 1e-4), sum.w);
}

// The colour a plant of mean brightness `albedo` is drawn toward at `foot`, per unit of its own
// brightness, as ten bits a channel; none where the ground is not known.
fn tint_word(foot: vec2<f32>, albedo: f32) -> u32 {
    let g = ground_colour(foot);
    if (g.w < 0.5 || albedo <= 0.0) {
        return 0u;
    }
    let t = clamp(g.rgb * (COVER_TINT * COVER_LIFT / albedo) / TINT_RANGE, vec3<f32>(0.0),
                  vec3<f32>(1.0));
    let q = vec3<u32>(round(t * 1023.0));
    return q.x | (q.y << 10u) | (q.z << 20u) | (1u << 30u);
}

// How far the bodies about it push a point of a plant `tall` metres high, `rise` up it, aside
// and down: where they stand, or where they stood last frame (`before`).
fn pushed_aside(p: vec3<f32>, rise: f32, tall: f32, before: bool) -> vec3<f32> {
    var out = vec3<f32>(0.0);
    for (var b = 0u; b < min(pushers.count.x, 8u); b++) {
        let at = select(pushers.at[b], pushers.was[b], before);
        let d = p.xy - at.xy;
        let r = length(d);
        if (r < 1.1 && abs(p.z - at.z) < 2.0) {
            let f = (1.0 - r / 1.1) * (1.0 - r / 1.1) * clamp(rise, 0.0, 1.0);
            let away = select(vec2<f32>(1.0, 0.0), d / max(r, 1e-4), r > 1e-4);
            out += vec3<f32>(away * f * min(0.6, tall * 0.8), -f * tall * 0.45);
        }
    }
    return out;
}

@vertex
fn vs_cover(v: VsIn, @builtin(instance_index) i: u32) -> VsOut {
    let plant = cover_plants[i];
    let k = cover_kinds[plant.kind];
    let s = plant.at.w;
    let foot = plant.at.xyz;
    let c = cos(plant.turn);
    let sn = sin(plant.turn);
    var local = v.position;
    var turn = 0.0;
    if (plant.level == 3u) {
        let to = frame.camera.xyz - foot;
        turn = atan2(to.y, to.x) - plant.turn;
        local = facing(local, turn);
    }
    // Grass Height raises it, widening it less so its picture keeps its shape.
    let raise = bitcast<f32>(pushers.count.y);
    local = vec3<f32>(local.xy * mix(1.0, raise, COVER_WIDEN), local.z * raise);
    let turned = vec3<f32>(c * local.x - sn * local.y, sn * local.x + c * local.y, local.z) * s;
    var world = foot + turned;
    var prev = world;
    let rise = v.position.z / max(k.height, 0.01);
    let tall = k.height * s * raise;
    if (k.wind > 0.0) {
        let seed = bitcast<u32>(plant.turn);
        world += cover_bend(cover_tip(foot.xy, tall, seed, frame.shore.z) * k.wind, rise, tall);
        prev += cover_bend(cover_tip(foot.xy, tall, seed, frame.shore.w) * k.wind, rise, tall);
    }
    world += pushed_aside(world, rise, tall, false);
    prev += pushed_aside(prev, rise, tall, true);
    let n = oct_decode(v.normal);
    var out: VsOut;
    out.clip = frame.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    out.normal = normalize(vec3<f32>(c * n.x - sn * n.y, sn * n.x + c * n.y, n.z));
    out.uv = v.uv;
    out.cur = out.clip;
    out.prev = frame.prev_view_proj * vec4<f32>(prev, 1.0);
    out.material = v.extra;
    out.extra = tint_word(foot.xy, k.albedo);
    out.shade = f32(v.extra2 & 255u) / 255.0;
    out.cell = 0u;
    out.object = 0u;
    out.fade = fade_word(0u, plant.fade, bitcast<u32>(plant.turn));
    out.turn = turn;
    out.ax = vec3<f32>(c, sn, 0.0);
    out.ay = vec3<f32>(-sn, c, 0.0);
    return out;
}

// Flowers' petals (well off green: red, pink or blue) keep their own colour.
fn tint_share(c: vec3<f32>) -> f32 {
    let red = (c.r - c.g) / max(c.r, 1e-4);
    let blue = (c.b - c.g) / max(c.b, 1e-4);
    return (1.0 - smoothstep(0.35, 0.6, red)) * (1.0 - smoothstep(0.0, 0.25, blue));
}

@fragment
fn fs_cover(in: VsOut, @builtin(front_facing) front: bool) -> GOut {
    var g = object_surface(in, front, 0u);
    var c = g.albedo.rgb * in.shade;
    if (in.extra != 0u) {
        // Its own light and dark over the ground's colour.
        let t = vec3<f32>(vec3<u32>(in.extra, in.extra >> 10u, in.extra >> 20u) & vec3<u32>(1023u))
            / 1023.0 * TINT_RANGE;
        let k = COVER_TINT * tint_share(c);
        c = c * (1.0 - k) + luma(c) * t * (k / COVER_TINT);
    }
    g.albedo = vec4<f32>(c, g.albedo.a);
    return g;
}

// The grass under the cover: each clump (blades.wgsl) a few blades from one root, each a strip
// bent on a curve from its root to its tip, leaning its own way and bent by the same wind as the
// cover, rounded across so it shades softly. Its colour is the ground's under it, drawn as the
// cover's is, dark at the root and lighter toward the tip, drier at the tip in dry clumps.

@group(3) @binding(4) var<storage, read> grass_clumps: array<Clump>;

// Blades a clump and segments a blade, near and far (cover.rs `NEAR_BLADES`, `FAR_BLADES`), and
// a blade's width at its root, metres.
const NEAR_BLADE: vec2<u32> = vec2<u32>(5u, 4u);
const FAR_BLADE: vec2<u32> = vec2<u32>(3u, 1u);
const BLADE_WIDTH: vec2<f32> = vec2<f32>(0.045, 0.11);
// The blades' material in the ids target, which no material takes.
const BLADE_ID: u32 = 0xFFFFFF00u;
// The colour of the cover's own pictures (linear), which the ground's is mixed with, and how much
// brighter the blades are drawn to stand level with the cover's lit leaves.
const BLADE_OWN: vec3<f32> = vec3<f32>(0.1, 0.14, 0.04);
const BLADE_LIFT: f32 = 1.2;

struct BladeCorner {
    p: vec3<f32>,
    n: vec3<f32>,
    // How far up the blade, 0 at the root to 1 at the tip; across it, -1 to 1.
    up: f32,
    side: f32,
    // The blade's own shade.
    shade: f32,
}

// Corner `vertex` of clump `cl` at `t` seconds into the clock's wrap, `raise` times its height;
// pushed by the bodies where they stand, or where they stood last frame (`before`).
fn blade_corner(cl: Clump, vertex: u32, t: f32, raise: f32, before: bool) -> BladeCorner {
    let far = (cl.look & 256u) != 0u;
    let segs = select(NEAR_BLADE.y, FAR_BLADE.y, far);
    let quads = (segs - 1u) * 6u;
    let per = quads + 3u;
    let blade = vertex / per;
    let v = vertex % per;
    var level: u32;
    var side: f32;
    if (v < quads) {
        // Two triangles a segment: (0, -) (0, +) (1, -), (1, -) (0, +) (1, +).
        let c = v % 6u;
        level = v / 6u + u32(c == 2u || c == 3u || c == 5u);
        side = select(-1.0, 1.0, c == 1u || c == 4u || c == 5u);
    } else {
        let c = v - quads;
        level = select(segs - 1u, segs, c == 2u);
        side = select(select(-1.0, 1.0, c == 1u), 0.0, c == 2u);
    }
    let up = f32(level) / f32(segs);
    let hb = cover_mix(cl.seed + blade * 0x9E3779B9u);
    let h1 = cover_mix(hb);
    let h2 = cover_mix(h1);
    let h3 = cover_mix(h2);
    // Where it grows from the clump's root, which way it faces and leans, how tall.
    let spread = select(0.12, 0.3, far) * cl.wide * sqrt(cover_unit(hb));
    let around = cover_unit(h1) * COVER_TAU;
    let root = cl.at.xyz + vec3<f32>(cos(around), sin(around), 0.0) * spread;
    let facing = cover_unit(h2) * COVER_TAU;
    let fwd = vec2<f32>(cos(facing), sin(facing));
    let across = vec3<f32>(-fwd.y, fwd.x, 0.0);
    let tall = max(cl.at.w * raise * mix(0.6, 1.15, cover_unit(h3)), 0.01);
    let lean = mix(0.15, 0.6, cover_unit(cover_mix(h3)));
    var reach = fwd * tall * sin(lean) + cover_tip(root.xy, tall, hb, t);
    reach = cover_hold(reach, tall);
    // A curve from the root to the tip, its length kept: stiff at the root, bent at the tip.
    let rise = sqrt(max(tall * tall - dot(reach, reach), 0.04 * tall * tall));
    let tip = root + vec3<f32>(reach, rise);
    let bend = root + vec3<f32>(reach * 0.15, rise * 0.6);
    let s = 1.0 - up;
    var p = s * s * root + 2.0 * s * up * bend + up * up * tip;
    let tangent = normalize(2.0 * s * (bend - root) + 2.0 * up * (tip - bend) + vec3<f32>(0.0, 0.0, 1e-4));
    let width = select(BLADE_WIDTH.x, BLADE_WIDTH.y, far) * cl.wide * (1.0 - pow(up, 1.3));
    p += across * side * width * 0.5;
    p += pushed_aside(p, up, tall, before);
    // Rounded across: its face turned to the eye, leaned out at its edges, and turned up (the
    // more toward the root) so the field shades as the ground under it does.
    var face = normalize(cross(tangent, across));
    if (dot(face, frame.camera.xyz - p) < 0.0) {
        face = -face;
    }
    let n = normalize(mix(normalize(face + across * side * 0.5), vec3<f32>(0.0, 0.0, 1.0),
                          mix(0.8, 0.45, up)));
    return BladeCorner(p, n, up, side, mix(0.9, 1.1, cover_unit(cover_mix(h1 ^ 0x68E31DA4u))));
}

@vertex
fn vs_blade(@builtin(vertex_index) vertex: u32, @builtin(instance_index) i: u32) -> VsOut {
    let cl = grass_clumps[i];
    let raise = bitcast<f32>(pushers.count.y);
    let now = blade_corner(cl, vertex, frame.shore.z, raise, false);
    let was = blade_corner(cl, vertex, frame.shore.w, raise, true);
    var out: VsOut;
    out.clip = frame.view_proj * vec4<f32>(now.p, 1.0);
    out.world = now.p;
    out.normal = now.n;
    out.uv = vec2<f32>(now.up, now.side);
    out.cur = out.clip;
    out.prev = frame.prev_view_proj * vec4<f32>(was.p, 1.0);
    out.material = BLADE_ID;
    out.extra = cl.colour;
    out.shade = now.up;
    out.cell = 0u;
    out.object = 0u;
    out.fade = 0u;
    out.turn = now.shade;
    // The clump's dryness, its shade and how warm it is.
    let h = cover_mix(cl.seed ^ 0x3C6EF372u);
    let straw = select(0.0, 0.85, cover_unit(cover_mix(cl.seed ^ 0x68E31DA4u)) < 0.25);
    out.ax = vec3<f32>(max(f32(cl.look & 255u) / 255.0, straw), mix(0.86, 1.12, cover_unit(h)),
                       cover_unit(cover_mix(h)));
    out.ay = vec3<f32>(0.0);
    return out;
}

@fragment
fn fs_blade(in: VsOut) -> GOut {
    var ground = vec3<f32>(0.11, 0.13, 0.05);
    if ((in.extra >> 30u) != 0u) {
        ground = vec3<f32>(vec3<u32>(in.extra, in.extra >> 10u, in.extra >> 20u) & vec3<u32>(1023u))
            / 1023.0 * BLADE_RANGE;
    }
    // The ground's colour as the cover takes it, a little warmer or cooler clump by clump.
    var base = (ground * COVER_TINT * COVER_LIFT + BLADE_OWN * (1.0 - COVER_TINT)) * BLADE_LIFT;
    base *= mix(vec3<f32>(1.06, 1.0, 0.86), vec3<f32>(0.92, 1.02, 1.06), in.ax.z) * in.ax.y * in.turn;
    // Dark at the root, where the field shades itself; lighter toward the tip, and yellower the
    // drier the clump.
    let up = clamp(in.shade, 0.0, 1.0);
    let dry_tip = base * vec3<f32>(1.32, 1.2, 0.7);
    let tip = mix(base * vec3<f32>(1.08, 1.1, 0.94), dry_tip, in.ax.x * smoothstep(0.45, 1.0, up));
    var c = mix(base * 0.7, tip * 1.08, smoothstep(0.0, 1.0, up));
    c *= 1.0 - 0.12 * in.uv.y * in.uv.y;
    var g = pack(c, normalize(in.normal), 0.85, 0.0, 0.0, 0u, BLADE_ID, in.cur, in.prev);
    g.object = 0u;
    return g;
}
