// Liquid: a water golem's (or a blood golem's) particles drawn as balls of liquid in place of
// sprites, each merged smoothly with the balls of its own body near it, drawn with the ice over a
// copy of the frame behind them. A ball's card covers it; the merged surface is marched along the
// view inside the ball's reach and drawn by the ball nearest it, which writes its depth. The
// liquid bends what lies behind it and takes its colour by its depth, mirrors the sky at a glance
// and glints in the sun. A ball grows with its particle and shrinks away as the particle fades.

// A ball's radius a unit of its particle's scale, and how far apart balls still merge, metres.
const BLOB_SIZE: f32 = 0.42;
// A blood ball's, smaller: blood hangs in drops (as particles.rs's `BLOOD_SIZE`).
const BLOOD_SIZE: f32 = 0.28;
const BLOB_MERGE: f32 = 0.12;
// The most neighbours a ball's surface is merged from, and the march's steps.
const BLOB_NEAR: u32 = 24u;
const BLOB_STEPS: i32 = 32;
// Water's reflectance head on, and how rough its face is.
const BLOB_F0: f32 = 0.02;
const BLOB_ROUGH: f32 = 0.05;
// The colour and strength of a water golem's glow from within.
const BLOB_WATER_GLOW: vec3<f32> = vec3<f32>(0.02, 0.12, 1.0);
const BLOB_WATER_LUMINOSITY: f32 = 0.1;
// Its ripples: how many to a metre, how fast they run, and how much they bend its face.
const BLOB_RIPPLE_SCALE: f32 = 7.0;
const BLOB_RIPPLE_SPEED: f32 = 1.6;
const BLOB_RIPPLE: f32 = 1.2;

struct BlobOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) @interpolate(flat) index: u32,
}

struct BlobLit {
    @location(0) colour: vec4<f32>,
    @builtin(frag_depth) depth: f32,
}

// Particle `i` as a ball: centre and radius.
fn blob_ball(i: u32) -> vec4<f32> {
    let p = particles[i];
    let size = select(BLOB_SIZE, BLOOD_SIZE, ((p.material >> 23u) & 3u) == 1u);
    let r = length(p.m0.xyz) * size * sqrt(clamp(p.alpha, 0.0, 1.0));
    return vec4<f32>(p.m0.w, p.m1.w, p.m2.w, r);
}

// What a metre of the liquid takes from each channel, and the light it scatters back out.
fn blob_extinction(kind: u32) -> vec3<f32> {
    if (kind == 1u) {
        return vec3<f32>(13.0, 90.0, 90.0);
    }
    return vec3<f32>(10.0, 3.4, 0.5);
}

fn blob_scatter(kind: u32) -> vec3<f32> {
    if (kind == 1u) {
        return vec3<f32>(0.2, 0.004, 0.004);
    }
    return vec3<f32>(0.008, 0.06, 0.26);
}

// Polynomial smooth minimum.
fn blob_smin(a: f32, b: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
    return mix(b, a, h) - k * h * (1.0 - h);
}

@vertex
fn vs_blob(@builtin(vertex_index) vi: u32, @builtin(instance_index) i: u32) -> BlobOut {
    let b = blob_ball(i);
    let reach = b.w + BLOB_MERGE;
    let to = b.xyz - frame.camera.xyz;
    let d = max(length(to), 1e-3);
    let fwd = to / d;
    let right = normalize(cross(fwd, vec3<f32>(0.0, 0.0, 1.0)) + vec3<f32>(1e-4, 0.0, 0.0));
    let up = cross(right, fwd);
    // On the ball's near side, wide enough to cover its outline in perspective.
    let near = max(d - reach, 0.05);
    let half = reach * d / sqrt(max(d * d - reach * reach, 1e-4)) * near / d * 1.05;
    let corners = array<vec2<f32>, 6>(vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
                                      vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0));
    let q = corners[vi];
    let world = frame.camera.xyz + fwd * near + (right * q.x + up * q.y) * half;
    var out: BlobOut;
    out.clip = frame.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    out.index = i;
    if (b.w <= 0.0 || d <= reach) {
        out.clip = vec4<f32>(0.0, 0.0, -2.0, 1.0);
    }
    return out;
}

@fragment
fn fs_blob(in: BlobOut) -> BlobLit {
    let p = particles[in.index];
    let first = p.material & 0x7FFFu;
    let count = (p.material >> 15u) & 0xFFu;
    let kind = (p.material >> 23u) & 3u;
    let own = blob_ball(in.index);
    let reach = own.w + BLOB_MERGE;
    let eye = frame.camera.xyz;
    let dir = normalize(in.world - eye);
    // Where the view crosses the ball's reach.
    let oc = eye - own.xyz;
    let bq = dot(oc, dir);
    let cq = dot(oc, oc) - reach * reach;
    let disc = bq * bq - cq;
    if (disc <= 0.0) {
        discard;
    }
    let sq = sqrt(disc);
    let t_in = max(-bq - sq, 0.0);
    let t_out = -bq + sq;
    // The balls of its body whose reach the view crosses inside this ball's.
    var near: array<vec4<f32>, BLOB_NEAR>;
    var ids: array<u32, BLOB_NEAR>;
    var n_near = 0u;
    for (var k = 0u; k < count && n_near < BLOB_NEAR; k++) {
        let j = first + k;
        let b = blob_ball(j);
        let rj = b.w + BLOB_MERGE;
        let tc = dot(b.xyz - eye, dir);
        if (b.w > 0.0 && distance(b.xyz, eye + dir * tc) < rj && tc > t_in - rj && tc < t_out + rj) {
            near[n_near] = b;
            ids[n_near] = j;
            n_near++;
        }
    }
    // March the merged surface.
    var t = t_in;
    var hit = false;
    var x = eye + dir * t;
    for (var s = 0; s < BLOB_STEPS; s++) {
        x = eye + dir * t;
        var f = 1e6;
        for (var k = 0u; k < n_near; k++) {
            f = blob_smin(f, distance(x, near[k].xyz) - near[k].w, BLOB_MERGE);
        }
        if (f < 0.002) {
            hit = true;
            break;
        }
        t += max(f, 0.004);
        if (t > t_out) {
            break;
        }
    }
    if (!hit) {
        discard;
    }
    // Drawn once, by the ball nearest the surface.
    var best = 1e6;
    var owner = in.index;
    for (var k = 0u; k < n_near; k++) {
        let f = distance(x, near[k].xyz) - near[k].w;
        if (f < best) {
            best = f;
            owner = ids[k];
        }
    }
    if (owner != in.index) {
        discard;
    }
    // Its normal, the merged field's slope.
    let e = 0.004;
    var g = vec3<f32>(0.0);
    let taps = array<vec3<f32>, 4>(vec3<f32>(1.0, -1.0, -1.0), vec3<f32>(-1.0, -1.0, 1.0),
                                   vec3<f32>(-1.0, 1.0, -1.0), vec3<f32>(1.0, 1.0, 1.0));
    for (var a = 0; a < 4; a++) {
        let y = x + taps[a] * e;
        var f = 1e6;
        for (var k = 0u; k < n_near; k++) {
            f = blob_smin(f, distance(y, near[k].xyz) - near[k].w, BLOB_MERGE);
        }
        g += taps[a] * f;
    }
    // Ripples running over it.
    let w = x * BLOB_RIPPLE_SCALE + vec3<f32>(0.0, 0.0, frame.wind.w * BLOB_RIPPLE_SPEED);
    let h0 = smoke_value(w);
    let ripple = vec3<f32>(smoke_value(w + vec3<f32>(0.3, 0.0, 0.0)) - h0,
                           smoke_value(w + vec3<f32>(0.0, 0.3, 0.0)) - h0,
                           smoke_value(w + vec3<f32>(0.0, 0.0, 0.3)) - h0);
    let n = normalize(normalize(g) - ripple * BLOB_RIPPLE);
    let v = -dir;
    let cos_v = saturate(dot(n, v));
    let dist = t;
    let px = in.clip.xy;
    let indoor = p.cell != 0u;
    let pre = pre_exposure();

    // Through it: as deep as the ball, bent by its face.
    let thick = clamp(min(ice_room(px, dist), own.w * mix(0.6, 2.0, cos_v) + 0.08), 0.02, 2.0);
    let bent = refract(dir, n, 1.0 / 1.33);
    let out_at = frame.view_proj * vec4<f32>(x + bent * max(thick, 0.3), 1.0);
    var uv = px / frame.params.zw;
    if (out_at.w > 1e-3) {
        let sp = out_at.xy / out_at.w;
        let off = clamp(vec2<f32>(sp.x * 0.5 + 0.5, 0.5 - sp.y * 0.5) - uv, vec2<f32>(-0.08), vec2<f32>(0.08));
        if (ice_room((uv + off) * frame.params.zw, dist) > 0.0) {
            uv += off;
        }
    }
    let seen = textureSampleLevel(ice_behind, samp_clamp, uv, 0.0).rgb;
    let through = exp(-blob_extinction(kind) * thick);

    // What lights it.
    let depth = view_depth_at(x);
    var sun_vis = 0.0;
    if (!indoor) {
        sun_vis = 1.0;
        if (has(F_SHADOWS)) {
            sun_vis = sun_shadow(x, n, depth, px);
        }
    }
    var sun = frame.sun_light.rgb;
    var sky = frame.sky_light.rgb;
    if (!has(F_PHYSICAL)) {
        sun = pow(frame.sun.rgb, vec3<f32>(2.2));
        sky = pow(min(frame.ambient.rgb, vec3<f32>(1.0)), vec3<f32>(2.2));
    }
    if (indoor) {
        sky = frame.indoor_light.rgb;
    }
    var light_in = sun * max(frame.sun_dir.z, 0.0) * sun_vis + sky;
    var lamp_spec = vec3<f32>(0.0);
    if (has(F_LAMPS)) {
        let lamp = lamp_light(x, n, v, BLOB_ROUGH, vec3<f32>(BLOB_F0), select(0u, G_INDOOR, indoor),
                              depth, px, false);
        light_in += select(lamps_held(lamp.diffuse, light_in), lamp.diffuse, indoor);
        lamp_spec = lamp.specular;
    }
    var body = seen * through + blob_scatter(kind) * light_in * (1.0 - exp(-4.0 * thick)) * pre;
    // A water golem's water glows a little from within, as its sprites did.
    if (kind == 0u) {
        body += glow_of(BLOB_WATER_GLOW, BLOB_WATER_LUMINOSITY) * (1.0 - exp(-3.0 * thick)) * pre;
    }

    // The sky it mirrors, and the sun's glint.
    var r = reflect(dir, n);
    var mirrored = sky;
    if (!indoor && frame.switches.x != 0u) {
        r.z = max(r.z, 0.1);
        if (has(F_ATMOSPHERE)) {
            mirrored = sky_radiance(r);
        } else {
            mirrored = sky_colour(r);
        }
    }
    let fresnel = BLOB_F0 + (1.0 - BLOB_F0) * pow(1.0 - cos_v, 5.0);
    var c = mix(body, mirrored * pre, fresnel);
    let ndl = dot(n, frame.sun_dir.xyz);
    if (ndl > 0.0) {
        c += ggx(n, v, frame.sun_dir.xyz, BLOB_ROUGH, vec3<f32>(BLOB_F0)) * ndl * sun * sun_vis * pre;
    }
    c += lamp_spec * pre;
    // The world behind came through the air already; the liquid's own light is seen through it.
    if (!indoor) {
        let behind = seen * through * (1.0 - fresnel);
        let own_light = c - behind;
        var aired = own_light;
        if (has(F_ATMOSPHERE)) {
            aired = aerial(own_light / max(pre, 1e-30), x, dir) * pre;
        } else {
            aired = mix(own_light, sky_colour(dir) * pre, fog_amount(distance(x.xy, eye.xy)));
        }
        c = behind + aired;
    }
    if (frame.umbrae[0].w > 0.0) {
        c *= umbra(x);
    }
    let clip = frame.view_proj * vec4<f32>(x, 1.0);
    var out: BlobLit;
    out.colour = vec4<f32>(min(c, vec3<f32>(HDR_MOST)), 1.0);
    out.depth = clip.z / clip.w;
    return out;
}
