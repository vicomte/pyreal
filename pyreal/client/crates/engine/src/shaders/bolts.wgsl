// Bolts: lightning in a creature's cloud (`bolts.rs`). Each strike's channel is a run of
// segments, records past the particles (`Particles::bolts`): ends, core width, its share of the
// strike's brightness, colour and flash. A segment is drawn as a thin ribbon held between its two
// ends, turned to the eye only about its own length, so the channel stays where it struck and
// never turns: a hot core a pixel or more wide over a glow in the strike's colour, added, bright
// enough to bloom, dimmed by the cloud between it and the eye. Each strike's chord also trails
// its body's volume run as a line of light (flag 1), which `bolt_light` gives a volume's march.

// How bright a channel's core is on screen at a full flash, in multiples of white; its glow's
// reaches (metres) and shares of that.
const BOLT_GAIN: f32 = 9.0;
const BOLT_GLOW: vec4<f32> = vec4<f32>(0.12, 0.5, 0.9, 0.4);
// How far round a strike's chord its light is whole inside the cloud, metres, and how far it
// carries past that.
const BOLT_LIGHT_CORE: f32 = 0.9;
const BOLT_LIGHT_REACH: f32 = 6.0;
// The least of a channel seen through its cloud (what scatters out round it).
const BOLT_SEEN: f32 = 0.08;

// How many of a volume run's entries are balls: its strikes' lines come last (flag 1).
fn puff_count(first: u32, count: u32) -> u32 {
    var n = count;
    while (n > 0u && (particles[first + n - 1u].flags & 1u) != 0u) {
        n--;
    }
    return n;
}

// The light the strikes in a volume run give at `x`: each its chord's light, whole within a core
// round it, falling away past it.
fn bolt_light(x: vec3<f32>, first: u32, count: u32) -> vec3<f32> {
    var light = vec3<f32>(0.0);
    for (var k = puff_count(first, count); k < count; k++) {
        let r = particles[first + k];
        let a = r.m0.xyz;
        let b = r.m1.xyz;
        let ab = b - a;
        let t = clamp(dot(x - a, ab) / max(dot(ab, ab), 1e-6), 0.0, 1.0);
        let d = distance(x, a + ab * t);
        let core = BOLT_LIGHT_CORE * BOLT_LIGHT_CORE;
        light += r.m2.rgb * (core / (d * d + core)) * exp(-d / BOLT_LIGHT_REACH);
    }
    return light;
}

struct BoltOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    // x across the segment, y along it from its start, metres.
    @location(1) at: vec2<f32>,
    // Its length, core width, share and flash.
    @location(2) @interpolate(flat) shape: vec4<f32>,
    @location(3) @interpolate(flat) colour: vec3<f32>,
    // How much of it its own cloud lets through to the eye, found once at its middle.
    @location(4) @interpolate(flat) seen: f32,
}

@vertex
fn vs_bolt(@builtin(vertex_index) vi: u32, @builtin(instance_index) i: u32) -> BoltOut {
    let r = particles[i];
    let a = r.m0.xyz;
    let b = r.m1.xyz;
    let len = max(distance(a, b), 1e-4);
    let along = (b - a) / len;
    let mid = (a + b) * 0.5;
    var side = cross(along, normalize(frame.camera.xyz - mid));
    if (length(side) < 1e-4) {
        side = cross(along, vec3<f32>(0.0, 0.0, 1.0)) + vec3<f32>(1e-3, 0.0, 0.0);
    }
    side = normalize(side);
    // As wide as its glow reaches (a fine piece its narrow glow, a glow's piece its wide one), and
    // a few pixels however far it is.
    let reach = select(BOLT_GLOW.x, BOLT_GLOW.y, r.m0.w < 0.0);
    let half = max(reach * BOLT_CARD, distance(frame.camera.xyz, mid) * 0.006);
    let corners = array<vec2<f32>, 6>(vec2<f32>(-1.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
                                      vec2<f32>(-1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0));
    let q = corners[vi];
    let y = mix(-half, len + half, q.y);
    let world = a + along * y + side * q.x * half;
    var out: BoltOut;
    out.clip = frame.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    out.at = vec2<f32>(q.x * half, y);
    out.shape = vec4<f32>(len, r.m0.w, r.m1.w, r.m2.w);
    out.colour = r.m2.rgb;
    out.seen = 1.0;
    if (r.m2.w <= 1e-3) {
        out.clip = vec4<f32>(0.0, 0.0, -2.0, 1.0);
        return out;
    }
    // Dimmed by its own cloud between it and the eye.
    if (r.material != 0xFFFFFFFFu) {
        let first = r.material & 0x7FFFu;
        let n = puff_count(first, (r.material >> 15u) & 0xFFu);
        let kind = (r.material >> 23u) & PUFF_KINDS;
        var thick = 1.5;
        if (kind >= PUFF_SMOKE && kind != PUFF_SNOW && kind != PUFF_HAZE) {
            thick = smokes_kind(kind).thick;
        }
        let tau = smokes_depth(mid, normalize(frame.camera.xyz - mid), first, n) * thick * 0.5;
        out.seen = mix(exp(-tau), 1.0, BOLT_SEEN);
    }
    return out;
}

// How far round a channel the upscaler is told the picture changes from frame to frame (the
// cloud flashing round it), metres; and a segment's card's half width over its wide glow's reach.
const BOLT_REACTIVE: f32 = 1.2;
const BOLT_CARD: f32 = 2.5;

// About erf(x), within a few thousandths.
fn bolt_erf(x: f32) -> f32 {
    let t = clamp(x, -3.0, 3.0);
    return tanh(1.128 * t + 0.1009 * t * t * t);
}

// A segment's share of a whole channel's light `r` wide at `x` across and `y` along it (of `len`):
// a Gaussian across, cut where the segment ends, so a channel of many short segments glows as
// one line rather than once for each.
fn bolt_line(x: f32, y: f32, len: f32, r: f32) -> f32 {
    return exp(-(x * x) / (r * r)) * 0.5 * (bolt_erf((len - y) / r) - bolt_erf(-y / r));
}

fn bolt_seen(in: BoltOut) -> vec4<f32> {
    let len = in.shape.x;
    // Its core never thinner than a pixel, so a far channel does not break up.
    let w = max(in.shape.y, fwidth(in.at.x) * 0.7);
    // A fine piece's core and narrow glow, or a glow piece's wide glow.
    var core = 0.0;
    var glow = 0.0;
    if (in.shape.y < 0.0) {
        glow = BOLT_GLOW.w * bolt_line(in.at.x, in.at.y, len, BOLT_GLOW.y);
    } else {
        core = bolt_line(in.at.x, in.at.y, len, w);
        glow = BOLT_GLOW.z * bolt_line(in.at.x, in.at.y, len, BOLT_GLOW.x);
    }
    var c = (mix(in.colour, vec3<f32>(1.0), 0.85) * core + in.colour * glow) * BOLT_GAIN * in.shape.z * in.shape.w;
    c *= in.seen;
    // It fades where it meets what stands behind it, and into the fog.
    let px = vec2<i32>(in.clip.xy);
    let dz = textureLoad(g_depth, px, 0);
    if (dz > 0.0) {
        let ndc = vec2<f32>(in.clip.x / frame.params.z * 2.0 - 1.0, 1.0 - in.clip.y / frame.params.w * 2.0);
        let h = frame.inv_view_proj * vec4<f32>(ndc, dz, 1.0);
        let behind = view_depth_at(h.xyz / h.w) - view_depth_at(in.world);
        c *= clamp(behind / 0.3, 0.0, 1.0);
    }
    c *= 1.0 - fog_amount(distance(in.world.xy, frame.camera.xy));
    return vec4<f32>(c, 1.0);
}

@fragment
fn fs_bolt(in: BoltOut) -> SortedOut {
    var out = sorted_out(bolt_seen(in), true);
    let dy = max(max(-in.at.y, in.at.y - in.shape.x), 0.0);
    let d = length(vec2<f32>(in.at.x, dy));
    let near = clamp(in.shape.w * in.shape.z, 0.0, 1.0) * exp(-d / BOLT_REACTIVE);
    out.reactive = max(out.reactive, vec4<f32>(near));
    return out;
}

@fragment
fn fs_bolt_oit(in: BoltOut) -> OitOut {
    return oit_out(bolt_seen(in), true, in.world);
}
