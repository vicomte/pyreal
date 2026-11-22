// Rain and snow: drops and flakes fixed in the world, a lattice of them filling a box round the
// eye (each wrapped back into the box as it leaves it, fading at its edges), carried as far as
// they have fallen and drifted with the wind (summed on the CPU, so a change of wind never makes
// them jump); a drop drawn as the streak a shutter blurs it into, a flake as a soft round blot
// smeared along its path, each no thinner than a pixel and hidden behind whatever stands nearer.
// None falls under a roof (the shelter map), in the room the eye stands in, or within a metre of
// the eye. On the day's own weather each falls as rain or snow by the land under it.

struct Precip {
    // x: the box's side, metres; y: drops in it; z: how fast they fall, metres a second; w: 1 snow.
    a: vec4<f32>,
    // xy: the wind's push, metres a second; z: the clock, seconds (wrapped); w: how much falls.
    b: vec4<f32>,
    // x: the shutter, seconds; y: a drop's or a flake's width, metres; zw: the target's size.
    c: vec4<f32>,
    // xy: how far the wind has carried them (wrapped to the box); z: how far they have fallen
    // at the box's speed (wrapped to twenty boxes); w: how many are drawn, the last fading.
    d: vec4<f32>,
    // x: the near box's side, metres; y: every this many flakes one is in it (0: none); zw: how far
    // the wind has carried them, wrapped to it.
    e: vec4<f32>,
    // x: how far they have fallen, wrapped to twenty near boxes; y: which fall (0 every one, 1 rain
    // over land that is not snowy, 2 snow over land that is); z: how far a storm has come in;
    // w: its gust, -1 to 1.
    f: vec4<f32>,
    // The room the eye stands in, whose roof keeps the drops off: its planes (inside where
    // `n · p + d >= 0`), and how many (none: outdoors).
    room: array<vec4<f32>, 16>,
    room_planes: vec4<u32>,
}

@group(1) @binding(0) var<uniform> precip: Precip;
@group(1) @binding(1) var shelter_map: texture_depth_2d;

const PRECIP_WRAP: f32 = 1200.0;
const PRECIP_TAU: f32 = 6.2831853;

// Whether `p` is inside the eye's room.
fn in_room(p: vec3<f32>) -> bool {
    let n = min(precip.room_planes.x, 16u);
    if (n == 0u) {
        return false;
    }
    for (var k = 0u; k < n; k++) {
        let pl = precip.room[k];
        if (dot(pl.xyz, p) + pl.w < 0.0) {
            return false;
        }
    }
    return true;
}

fn precip_hash(i: u32) -> vec3<f32> {
    var h = i * 747796405u + 2891336453u;
    var out = vec3<f32>(0.0);
    for (var k = 0u; k < 3u; k++) {
        h = h * 747796405u + 2891336453u;
        let w = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
        out[k] = f32((w >> 22u) ^ w) / 4294967295.0;
    }
    return out;
}

// An angular rate, radians a second, nudged to a whole number of turns over the clock's wrap.
fn turns(rate: f32) -> f32 {
    return max(round(rate * PRECIP_WRAP / PRECIP_TAU), 1.0) * PRECIP_TAU / PRECIP_WRAP;
}

// The wind's eddies at render-space `p`, time `t`: a slow swirl the neighbouring flakes share,
// metres to move them by.
fn eddy(p: vec3<f32>, t: f32) -> vec3<f32> {
    let a = sin(p.y * 0.19 + t * turns(0.53) + sin(p.z * 0.31 + t * turns(0.29)) * 1.3);
    let b = sin(p.x * 0.23 - t * turns(0.47) + sin(p.y * 0.27 - t * turns(0.31)) * 1.3);
    let c = sin(p.x * 0.13 + p.y * 0.11 + t * turns(0.37));
    return vec3<f32>(a, b, 0.4 * c) * 0.5;
}

// A flake's own flutter at time `t`: a spiral about its path and a little bob, by its hashes.
fn flutter(r: vec3<f32>, t: f32, calm: f32) -> vec3<f32> {
    let ang = t * turns((0.3 + 0.5 * r.x) * PRECIP_TAU) + r.y * PRECIP_TAU;
    let radius = (0.04 + 0.16 * r.z) * calm;
    return vec3<f32>(cos(ang), sin(ang), 0.25 * sin(ang * 2.0 + r.x * 9.0)) * radius;
}

struct POut {
    @builtin(position) clip: vec4<f32>,
    // A drop: x along the streak, y across it, -1..1. A flake: pixels from its middle along its
    // smear and across it.
    @location(0) at: vec2<f32>,
    @location(1) fade: f32,
    // A flake's: half its smear's length and its radius, pixels; how soft its edge is; how
    // brightly the sun shines through it. A drop's: half its width drawn and its core's width,
    // pixels, and its core's alpha.
    @location(2) @interpolate(flat) shape: vec4<f32>,
}

@vertex
fn vs_precip(@builtin(vertex_index) v: u32, @builtin(instance_index) i: u32) -> POut {
    var out: POut;
    out.clip = vec4<f32>(2.0, 2.0, 2.0, 1.0);
    out.at = vec2<f32>(0.0);
    out.fade = 0.0;
    out.shape = vec4<f32>(0.0);
    let snow = precip.a.w > 0.5;
    let stride = u32(precip.e.y);
    let near_box = snow && stride > 0u && (i % stride) == 0u;
    var side = precip.a.x;
    var carried = precip.d.xyz;
    if (near_box) {
        side = precip.e.x;
        carried = vec3<f32>(precip.e.zw, precip.f.x);
    }
    let r = precip_hash(i);
    let r2 = precip_hash(i ^ 0x5bd1e995u);
    let t = precip.b.z;
    let storm = precip.f.z;
    // Each falls at a whole number of twentieths of the speed, so the fall's wrap is whole boxes
    // for all; the pace is hashed apart from the height, or the drops would gather into sheets.
    // Rain keeps to 17 to 23 of them, snow strays from 12 to 28.
    var pace = (17.0 + min(floor(r2.x * 7.0), 6.0)) / 20.0;
    if (snow) {
        pace = (12.0 + min(floor(r2.x * 17.0), 16.0)) / 20.0;
    }
    let vel = vec3<f32>(precip.b.xy, -precip.a.z * pace);
    let q = r * side + vec3<f32>(carried.xy, -carried.z * pace);
    let eye = frame.camera.xyz;
    let lo = eye - vec3<f32>(side * 0.5);
    var p = q - side * floor((q - lo) / side);
    let rel = (p - eye) / (side * 0.5);
    let edge = 1.0 - smoothstep(0.7, 1.0, max(abs(rel.x), max(abs(rel.y), abs(rel.z))));
    // What a flake's flutter and the eddies moved it by over the shutter.
    var swept = vec3<f32>(0.0);
    var r3 = vec3<f32>(0.0);
    if (snow) {
        r3 = precip_hash(i ^ 0x2c1b3c6du);
        let calm = 1.0 - 0.7 * storm;
        let swirl = 0.35 + 0.8 * storm;
        let then = t - precip.c.x;
        let now = flutter(r3, t, calm) + eddy(p, t) * swirl;
        let before = flutter(r3, then, calm) + eddy(p, then) * swirl;
        p += now;
        swept = now - before;
    }
    let dist = distance(p, eye);
    var near = smoothstep(0.8, 2.0, dist);
    if (snow) {
        near = smoothstep(0.3, 0.8, dist);
    }
    let last = clamp((precip.d.w - f32(i)) / max(precip.d.w * 0.05, 1.0), 0.0, 1.0);
    var fade = edge * near * precip.b.w * last;
    // Under the ground, under a roof, or in the room the eye stands in, nothing; what a wall or
    // a roof stands in front of is hidden by the depth.
    if (has_land()) {
        let g = vec2<i32>(floor((p.xy - frame.land.xy) / LAND_CELL));
        if (p.z < land_vertex(g).height - 1.0) {
            fade = 0.0;
        }
    }
    if (fade <= 0.001 || in_room(p) || sheltered(p, 0.1) > 0.5) {
        return out;
    }
    // Rain over land that is not snowy, snow over land that is, mixed where they meet.
    let gate = precip.f.y;
    if (gate > 0.5) {
        let cut = 0.3 + 0.4 * precip_hash(i ^ 0x68e31da4u).x;
        let snowy = land_snowy(p.xy) > cut;
        if (snowy != (gate > 1.5)) {
            return out;
        }
    }
    let a = frame.view_proj * vec4<f32>(p, 1.0);
    if (a.w <= 0.05) {
        return out;
    }
    let size = precip.c.zw;
    let px = 2.0 / size;
    // A pixel's height in metres at this distance (the view's tangent is `cluster_z.w`).
    let metre_px = a.w * px.y * frame.cluster_z.w;
    if (snow) {
        // A flake's width: most one or two centimetres, a few clumps four; a blizzard's finer.
        let width = (0.01 + 0.03 * pow(r2.y, 3.0)) * (1.0 - 0.35 * storm);
        let b = frame.view_proj * vec4<f32>(p - vel * precip.c.x - swept, 1.0);
        if (b.w <= 0.05) {
            return out;
        }
        let half_size = size * 0.5;
        let pa = a.xy / a.w * half_size;
        let pb = b.xy / b.w * half_size;
        var dir = pa - pb;
        var len = length(dir);
        if (len < 1e-3) {
            dir = vec2<f32>(0.0, 1.0);
            len = 0.0;
        } else {
            dir /= len;
        }
        // No thinner than a pixel across: a smaller flake keeps its light, spread fainter.
        let true_r = width * 0.5 / metre_px;
        let rp = max(true_r, 0.9);
        fade *= min(1.0, (true_r * true_r) / (rp * rp));
        // The eye sees a flake as a dot; only a fast one smears, and never far.
        let hl = min(len * 0.5, 2.0 * rp);
        let corner = array<vec2<f32>, 6>(
            vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
            vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
        )[v % 6u];
        let across = vec2<f32>(-dir.y, dir.x);
        let local = vec2<f32>(corner.x * (hl + rp + 1.0), corner.y * (rp + 1.0));
        let mid = (pa + pb) * 0.5;
        let s = (mid + dir * local.x + across * local.y) / half_size;
        out.clip = vec4<f32>(s * a.w, a.z, a.w);
        out.at = local;
        // The sun through the flakes the eye looks toward it past.
        let mu = max(dot(normalize(p - eye), frame.sun_dir.xyz), 0.0);
        let glow = pow(mu, 14.0) * 1.6 + pow(mu, 3.0) * 0.25;
        // Flakes passing the eye close by are out of focus: larger, softer and fainter.
        let blur = 1.0 - smoothstep(0.6, 2.2, dist);
        out.shape = vec4<f32>(hl, rp, mix(0.35, 0.0, blur), glow);
        fade *= mix(1.0, 0.45, blur);
        // A smear keeps the flake's light, spread along it.
        fade /= 1.0 + hl / (PRECIP_TAU * 0.5 * rp);
    } else {
        let corner = array<vec2<f32>, 6>(
            vec2<f32>(0.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
            vec2<f32>(0.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
        )[v % 6u];
        let b = frame.view_proj * vec4<f32>(p - vel * precip.c.x, 1.0);
        // A streak reaching behind the eye would smear across the view: left out.
        if (b.w <= 0.05) {
            return out;
        }
        let sa = a.xy / a.w;
        let sb = b.xy / b.w;
        var dir = (sb - sa) * size;
        if (dot(dir, dir) < 1.0) {
            dir = vec2<f32>(0.0, -1.0);
        }
        // Its core at least a pixel across, drawn a pixel wider each side and filtered there, so
        // a thin streak stays one even line at any slant.
        let true_px = precip.c.y / max(metre_px, 1e-6);
        let core = max(true_px, 1.0);
        let half_px = core * 0.5 + 1.0;
        let across = normalize(vec2<f32>(-dir.y, dir.x)) * px * half_px;
        let s = mix(sa, sb, corner.x) + across * corner.y;
        let w = mix(a.w, b.w, corner.x);
        let z = mix(a.z / a.w, b.z / b.w, corner.x);
        out.clip = vec4<f32>(s * w, z * w, w);
        out.at = vec2<f32>(corner.x * 2.0 - 1.0, corner.y);
        // Its light across it as before: a thin streak's spread over the pixel it covers.
        out.shape = vec4<f32>(half_px, core, 0.275 * max(true_px, 1.2) / core, 0.0);
    }
    out.fade = fade;
    return out;
}

// A drop's colour, and how much it changes the pixel for the upscaler's reactive mask: it moves
// on every frame, so the history must not hold it.
struct PrecipOut {
    @location(0) colour: vec4<f32>,
    @location(1) reactive: vec4<f32>,
}

@fragment
fn fs_precip(in: POut) -> PrecipOut {
    let snow = precip.a.w > 0.5;
    let sun = frame.sun_light.rgb * max(frame.sun_dir.z, 0.0);
    var alpha: f32;
    var colour: vec3<f32>;
    if (snow) {
        let hl = in.shape.x;
        let rp = in.shape.y;
        let d = length(vec2<f32>(max(abs(in.at.x) - hl, 0.0), in.at.y)) / rp;
        alpha = (1.0 - smoothstep(in.shape.z, 1.0 + 0.5 / rp, d)) * 0.9;
        // Lit from all round, the bright snow below too.
        colour = frame.sky_light.rgb * 1.6 + sun * (0.35 + in.shape.w);
    } else {
        // The core's cover of each pixel, its edges a pixel's filter wide.
        let off = abs(in.at.y) * in.shape.x;
        let cover = clamp(in.shape.y * 0.5 + 0.5 - off, 0.0, 1.0);
        alpha = cover * (1.0 - smoothstep(0.6, 1.0, abs(in.at.x))) * in.shape.z;
        colour = frame.sky_light.rgb * 1.3 + sun * 0.06;
    }
    alpha *= in.fade;
    if (alpha <= 0.0005) {
        discard;
    }
    let c = min(colour * pre_exposure(), vec3<f32>(HDR_MOST));
    var o: PrecipOut;
    o.colour = vec4<f32>(c * alpha, alpha);
    // A flake is a pixel or two that moves its own way: the history must let it through whole.
    o.reactive = vec4<f32>(clamp(alpha * select(3.0, 10.0, snow), 0.0, 1.0));
    return o;
}
