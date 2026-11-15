// Cobwebs as fine silk (webs.rs): each thread a ribbon widened to a pixel at least, thinner ones
// faded by how much of the pixel they cover, stirred by the bodies moving near; among them a faint
// gauze.

struct Stir {
    // x: the springs in use; y: the clock, seconds (wrapped).
    head: vec4<f32>,
    // xyz: a stirring body's middle; w: how far its stir reaches.
    at: array<vec4<f32>, 8>,
    // xyz: how far its spring has pushed the silk; w: how fast it moves.
    push: array<vec4<f32>, 8>,
}

@group(3) @binding(11) var<uniform> stir: Stir;

// How much of its line a strand covers, and the least a strand thinner than a pixel keeps.
const SILK_OPACITY: f32 = 0.65;
const SILK_LEAST: f32 = 0.35;
// How brightly the silk glints along its strands toward a lamp, and passes light from behind.
const SILK_SHEEN: f32 = 0.5;
const SILK_THROUGH: f32 = 0.15;
// How much the gauze covers at most.
const GAUZE: f32 = 0.02;
// Metres off where the gauze has faded out.
const GAUZE_FAR: f32 = 6.0;
// A thread takes light from every side, not from the front alone.
const SILK_ROUND: f32 = 1.4;
// Where the silk's light (pre-exposed) starts to be eased, and the most it reaches before its
// glint.
const SILK_KNEE: f32 = 0.6;
const SILK_MOST: f32 = 0.95;
// Threads thinner than this (metres) are fine, and gone past this far (metres).
const SILK_FINE: f32 = 0.001;
const SILK_FINE_FAR: f32 = 9.0;
// How far the freest silk droops under its own weight, metres.
const SILK_DROOP: f32 = 0.025;
// The most pixels round a moving strand marked reactive, so the history lets go where it was.
const SILK_TRAIL: f32 = 3.0;

struct WebOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    // A strand's way along, or the gauze's normal.
    @location(1) tangent: vec3<f32>,
    // Pixels from the strand's middle, the half width of its core (below 0 for the gauze), and
    // how much of it shows.
    @location(2) across: f32,
    @location(3) core: f32,
    @location(4) dim: f32,
    @location(5) @interpolate(flat) cell: u32,
    @location(6) @interpolate(flat) fade: u32,
    @location(7) @interpolate(flat) material: u32,
}

// Where the silk at `p` (normal `n`, `free` how freely it hangs) is pushed: a slow drift, and each
// spring's push where its body is near; and in w how fast it moves, metres a second. The air
// moves alike at neighbouring threads.
fn silk_push(p: vec3<f32>, n: vec3<f32>, free: f32, phase: f32) -> vec4<f32> {
    let t = stir.head.y;
    let near = dot(p, vec3<f32>(1.3, 1.7, 2.1));
    var d = n * (0.005 * sin(t * 0.7 + near) + 0.0025 * sin(t * 1.9 + near * 2.3));
    var moving = 0.0;
    let count = u32(stir.head.x);
    for (var k = 0u; k < count; k++) {
        let a = stir.at[k];
        let q = stir.push[k];
        let w = 1.0 - smoothstep(0.35 * a.w, a.w, length((p - a.xyz) * vec3<f32>(1.0, 1.0, 0.6)));
        if (w <= 0.0) {
            continue;
        }
        let vary = 0.7 + 0.3 * sin(t * 2.3 + near * 1.7);
        let flutter = min(q.w, 2.0) * 0.006 * sin(t * 14.0 + phase * 5.0 + dot(p, vec3<f32>(9.0, 7.0, 5.0)));
        d += w * (q.xyz * vary + n * flutter);
        moving += w * q.w;
    }
    return vec4<f32>(d, moving) * free;
}

@vertex
fn vs_web(v: VsIn, @builtin(instance_index) entry: u32) -> WebOut {
    let e = visible[entry];
    let inst = instances[e.x];
    var p = xform(inst.m0, inst.m1, inst.m2, v.position);
    let t = normalize(xform_dir(inst.m0, inst.m1, inst.m2, oct_decode(v.normal)));
    var out: WebOut;
    out.cell = inst.cell & 0xFFFFFFu;
    out.fade = fade_word(inst.cell >> 24u, (e.y >> 16u) & 0xFFu, e.x);
    out.material = e.y & 0xFFFFu;
    if ((v.extra & 0xFFFFu) == 0u) {
        // The gauze, stirred with its threads, gone past a few metres (its whole web at once).
        p += silk_push(p, t, v.uv.y, 0.0).xyz - vec3<f32>(0.0, 0.0, SILK_DROOP * v.uv.y);
        let far = distance(vec3<f32>(inst.m0.w, inst.m1.w, inst.m2.w), frame.camera.xyz);
        out.clip = select(frame.view_proj * vec4<f32>(p, 1.0), vec4<f32>(0.0, 0.0, -1.0, 1.0), far > GAUZE_FAR + 3.0);
        out.world = p;
        out.tangent = t;
        out.core = -1.0;
        let fade = 1.0 - smoothstep(0.5 * GAUZE_FAR, GAUZE_FAR, distance(p, frame.camera.xyz));
        out.dim = f32(v.extra >> 16u) / 65535.0 * fade;
        return out;
    }
    let n = normalize(xform_dir(inst.m0, inst.m1, inst.m2, oct_decode(unpack2x16snorm(v.extra2))));
    let phase = f32(v.extra >> 16u) * (6.2831853 / 65536.0);
    p.z -= SILK_DROOP * v.uv.y;
    let rest = p;
    let pushed = silk_push(p, n, v.uv.y, phase);
    p += pushed.xyz;
    let width = f32(v.extra & 0xFFFFu) * 1e-5;
    // The strand across the screen: its width there in pixels, drawn a pixel wide at least.
    let half_size = frame.params.zw * 0.5;
    let c0 = frame.view_proj * vec4<f32>(p, 1.0);
    let c1 = frame.view_proj * vec4<f32>(p + t * 0.05, 1.0);
    var side = cross(t, frame.camera.xyz - p);
    side = select(n, normalize(side), length(side) > 1e-6);
    let c2 = frame.view_proj * vec4<f32>(p + side * width, 1.0);
    let s0 = c0.xy / max(c0.w, 1e-4) * half_size;
    let s1 = c1.xy / max(c1.w, 1e-4) * half_size;
    let s2 = c2.xy / max(c2.w, 1e-4) * half_size;
    let px = length(s2 - s0);
    var dir = s1 - s0;
    dir = select(vec2<f32>(1.0, 0.0), normalize(dir), length(dir) > 1e-5);
    let drawn = max(px, 1.0);
    // How far it moved on screen since last frame (the camera's share, and the stir's over a
    // sixtieth of a second), which the reactive band round it covers.
    let c3 = frame.prev_view_proj * vec4<f32>(rest, 1.0);
    let s3 = c3.xy / max(c3.w, 1e-4) * half_size;
    let moved = length(s0 - s3) + pushed.w / 60.0 * px / max(width, 1e-6);
    let half = drawn * 0.5 + 0.5 + min(moved, SILK_TRAIL);
    let offset = vec2<f32>(-dir.y, dir.x) * v.uv.x * half;
    out.clip = vec4<f32>(c0.xy + offset / half_size * c0.w, c0.zw);
    // Fine threads fade out with distance, and past it are not drawn (their whole web at once).
    let far = distance(vec3<f32>(inst.m0.w, inst.m1.w, inst.m2.w), frame.camera.xyz);
    let fine = width < SILK_FINE;
    if (fine && far > SILK_FINE_FAR) {
        out.clip = vec4<f32>(0.0, 0.0, -1.0, 1.0);
    }
    out.world = p;
    out.tangent = t;
    out.across = v.uv.x * half;
    out.core = drawn * 0.5;
    // Strands differ a little in how much dust they hold.
    out.dim = max(px / drawn, SILK_LEAST) * (0.6 + 0.4 * fract(phase * 13.7));
    if (fine) {
        out.dim *= 1.0 - smoothstep(0.6 * SILK_FINE_FAR, SILK_FINE_FAR, distance(p, frame.camera.xyz));
    }
    return out;
}

// A thread's glint toward light from `l`: along its length (Kajiya and Kay), and through it from
// behind.
fn silk_glint(t: vec3<f32>, v: vec3<f32>, l: vec3<f32>) -> f32 {
    let h = normalize(l + v);
    let th = dot(t, h);
    let along = pow(max(1.0 - th * th, 0.0), 80.0);
    let through = pow(max(dot(-v, l), 0.0), 12.0);
    return along * SILK_SHEEN + through * SILK_THROUGH;
}

// The glint toward the nearest lamps, each by its light here, as a share of the light the silk
// takes.
fn silk_sheen(t: vec3<f32>, v: vec3<f32>, world: vec3<f32>, indoor: bool) -> f32 {
    var sum = 0.0;
    var weight = 0.0;
    if (has(F_LAMPS)) {
        for (var k = 0; k < 4; k++) {
            let key = frame.key_lamps[k];
            let to = key.xyz - world;
            let r = length(to);
            if (key.w <= 0.0 || r >= key.w || r < 1e-3) {
                continue;
            }
            let w = frame.key_power[k] / max(r * r, 0.25) * (1.0 - smoothstep(0.5 * key.w, key.w, r));
            sum += w * silk_glint(t, v, to / r);
            weight += w;
        }
    }
    if (!indoor && has(F_PHYSICAL)) {
        let w = frame.sun_light.r + frame.sun_light.g + frame.sun_light.b;
        sum += w * silk_glint(t, v, frame.sun_dir.xyz);
        weight += w;
    }
    return sum / max(weight, 1e-4);
}

// How much of the pixel the silk covers: a thread by its line, the gauze by its fade.
fn silk_cover(in: WebOut) -> f32 {
    if (in.core < 0.0) {
        return in.dim * GAUZE;
    }
    return clamp(in.core + 0.5 - abs(in.across), 0.0, 1.0) * in.dim * SILK_OPACITY;
}

// The silk's colour at coverage `a`.
fn silk_seen(in: WebOut, a: f32) -> vec4<f32> {
    let m = materials[in.material];
    let v = normalize(frame.camera.xyz - in.world);
    let indoor = in.cell != 0u;
    let bits = select(0u, G_INDOOR, indoor);
    if (in.core < 0.0) {
        let n = normalize(in.tangent) * select(-1.0, 1.0, dot(in.tangent, v) >= 0.0);
        let lit = forward_light(m.base_color.rgb, in.world, n, v, m.roughness, 0.0, 0.0, bits, in.clip.xy);
        return vec4<f32>(silk_held(through_air(lit, in.world, -v, indoor)), a);
    }
    let t = normalize(in.tangent);
    var n = v - t * dot(v, t);
    n = select(v, normalize(n), length(n) > 1e-4);
    let lit = forward_light(m.base_color.rgb, in.world, n, v, m.roughness, 0.0, 0.0, bits, in.clip.xy);
    let seen = silk_held(through_air(lit * SILK_ROUND, in.world, -v, indoor));
    return vec4<f32>(seen * (1.0 + silk_sheen(t, v, in.world, indoor)), a);
}

// A lit colour (pre-exposed) eased under a light grey past a knee, so silk by a lamp never
// glares.
fn silk_held(c: vec3<f32>) -> vec3<f32> {
    let l = luma(c);
    if (l <= SILK_KNEE) {
        return c;
    }
    let over = l - SILK_KNEE;
    return c * ((SILK_KNEE + over / (1.0 + over / (SILK_MOST - SILK_KNEE))) / l);
}

@fragment
fn fs_web(in: WebOut) -> SortedOut {
    if (screened(in.clip.xy, in.fade)) {
        discard;
    }
    let a = silk_cover(in);
    // Threads a pixel wide move too far between frames for the history to follow: the band
    // round each, where it was a moment ago, is marked too.
    var line = clamp(in.dim * SILK_OPACITY * 2.0, 0.0, 1.0);
    if (in.core < 0.0) {
        line = a * 4.0;
    }
    if (a < 0.006) {
        if (line < 0.01) {
            discard;
        }
        var o: SortedOut;
        o.colour = vec4<f32>(0.0);
        o.reactive = vec4<f32>(line);
        return o;
    }
    var o = sorted_out(silk_seen(in, a), false);
    o.reactive = vec4<f32>(line);
    return o;
}

@fragment
fn fs_web_oit(in: WebOut) -> OitOut {
    if (screened(in.clip.xy, in.fade)) {
        discard;
    }
    let a = silk_cover(in);
    if (a < 0.006) {
        discard;
    }
    return oit_out(silk_seen(in, a), false, in.world);
}
