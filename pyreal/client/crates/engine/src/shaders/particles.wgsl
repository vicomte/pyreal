// Particles: each one its emitter's picture at its frame and scale, turned to the eye as its
// degrade mode turns it, drawn instanced from this frame's particle list (sorted far to near on
// the CPU). Lit as a translucent surface facing the eye, wrapped as a leaf is so its back is not
// black; an additive one adds its light. Each fades where it meets what stands behind it. Smoke
// is drawn as a soft billow of churning noise, lit by the scene as a small cloud is. Ground mist
// is a flattened ball of fog turned to the eye, as thick as the ray's run through it before it
// meets the ground, so grass and stones cut it smoothly. A bed of fire's puffs are churning balls
// of flame, hot while young and cooling to red as they rise and age.

struct Particle {
    m0: vec4<f32>,
    m1: vec4<f32>,
    m2: vec4<f32>,
    // The material, its turn's mode in the top byte.
    material: u32,
    // 1 - translucency; a creature's light's colour word (`glow::Glow::word`); a fire puff's
    // colour (`particles::fire_word`).
    alpha: f32,
    cell: u32,
    // 1: one sprite standing for a lamp's light; 2: smoke; 3: fire; bits 2-11 its age (of
    // 1023); 4096: mist; bits 13-29 its seed; bit 30 a creature's light, bit 31 its being a star
    // (alone, a lighthouse beacon's part, in its seed's low two bits: `beacons.wgsl`).
    // Smoke and mist at once: a mote of dust drawn into a portal.
    flags: u32,
}

@group(1) @binding(5) var<storage, read> particles: array<Particle>;

// How deep, metres a unit of scale, a particle fades out into what stands behind it.
const SOFT_DEPTH: f32 = 0.2;
const SMOKE_SOFT_DEPTH: f32 = 0.6;

// Smoke's colour and the most of what is behind it a billow hides.
const SMOKE_ALBEDO: vec3<f32> = vec3<f32>(0.36, 0.37, 0.40);
const SMOKE_OPACITY: f32 = 0.8;
// How far a billow turns over its life, radians, and how fast its noise churns, units a second:
// slow beside its rise, as real smoke's eddies are.
const SMOKE_ROLL: f32 = 0.3;
const SMOKE_CHURN: f32 = 0.05;

// Mist's colour, how thick it is a metre at its heart, and its height over its reach.
const MIST_ALBEDO: vec3<f32> = vec3<f32>(0.8, 0.8, 0.8);
const MIST_DENSITY: f32 = 0.04;
const MIST_HEIGHT: f32 = 0.3;

struct PartOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) @interpolate(flat) material: u32,
    @location(3) @interpolate(flat) alpha: f32,
    @location(4) @interpolate(flat) cell: u32,
    @location(5) @interpolate(flat) size: f32,
    @location(6) @interpolate(flat) flags: u32,
    @location(7) @interpolate(flat) center: vec3<f32>,
    // A creature's light's colour and strength.
    @location(8) @interpolate(flat) glow: vec4<f32>,
}

// A creature's light (`glow.rs`), a ball's and a star's: how bright its core is beside its halo
// and how tight beside its picture's spread, and how strong its halo is; how much of its heart is
// whitened; a star's rays' thinness and strength.
const GLOW_BALL: vec3<f32> = vec3<f32>(1.6, 0.4, 1.0);
const GLOW_STAR: vec3<f32> = vec3<f32>(3.0, 0.3, 0.8);
const GLOW_WHITE: f32 = 0.5;
// The strengths over which a picture's heart grows from a haze to a hot core.
const GLOW_FAINT: vec2<f32> = vec2<f32>(0.05, 0.4);
const GLOW_RAY_THIN: f32 = 60.0;
const GLOW_RAY: f32 = 1.0;
// How bright a creature's light shows over its picture's heart, and how much it dims as it ages.
const GLOW_GAIN: f32 = 1.4;
const GLOW_AGE_DIM: f32 = 0.5;

// A fire puff: its hottest heart's brightness, in multiples of white on screen; how fast flame
// climbs through it and churns, its noise's units a second.
const FIRE_GAIN: f32 = 0.6;
const FIRE_RISE: f32 = 1.4;
const FIRE_CHURN: f32 = 0.6;
// How much cooler a puff burns a unit of its scale.
const FIRE_COOL: f32 = 0.3;
// Retail's fire puff's colour (its picture's mean, linear, brightest one): fire of this colour
// takes the ramp as it is.
const FIRE_PUFF_TINT: vec3<f32> = vec3<f32>(1.0, 0.22, 0.058);

// A lamp's light sprite: its picture's brightness in firelight, at this share.
const LIGHT_SPRITE: vec3<f32> = vec3<f32>(1.0, 0.5, 0.2);
const LIGHT_SPRITE_GAIN: f32 = 1.0;
// A magic lamp's halo (a blue or purple light sprite): its share of its picture, so the night's
// lifted toe shows it as faint as retail does.
const MAGIC_HALO: f32 = 0.5;

// A mote of dust drawn into a portal (`gates.rs`): the purple it turns as it enters, how bright,
// and how much brighter as it crosses; the sun it scatters; the fewest pixels across it is drawn,
// and how strong it is (spread over its pixels and its streak).
const MOTE: u32 = 4098u;
const MOTE_PURPLE: vec3<f32> = vec3<f32>(0.42, 0.08, 1.0);
const MOTE_GLOW: f32 = 3.0;
const MOTE_FLARE: f32 = 2.5;
// How much of the sun it scatters, and how much of that ahead.
const MOTE_SCATTER: f32 = 0.4;
const MOTE_FORWARD: f32 = 0.6;
const MOTE_PIXELS: f32 = 2.5;
const MOTE_STRENGTH: f32 = 1.5;

@vertex
fn vs_particle(v: VsIn, @builtin(instance_index) i: u32) -> PartOut {
    let p = particles[i];
    let m = billboard(array<vec4<f32>, 3>(p.m0, p.m1, p.m2), p.material >> 24u);
    var world = xform(m[0], m[1], m[2], v.position);
    let center = vec3<f32>(p.m0.w, p.m1.w, p.m2.w);
    let mote = (p.flags & MOTE) == MOTE;
    var thinned = 1.0;
    if (mote) {
        // Never under a pixel or two across, its strength spread over them.
        let r = length(p.m0.xyz);
        let px = distance(center, frame.camera.xyz) * 1.2 / max(frame.params.w, 1.0);
        let grown = max(r, px * MOTE_PIXELS * 0.5);
        let streak = vec3<f32>(p.m1.y, p.m2.z, p.m2.x);
        let long = grown + length(streak) * 0.5;
        thinned = min(MOTE_STRENGTH * r * r / (grown * sqrt(grown * long)), 1.0);
        world = mote_card(center, grown, streak, v.uv);
    } else if ((p.flags & 4096u) != 0u) {
        world = mist_card(center, length(p.m0.xyz), v.uv);
    } else if ((p.flags >> 30u) == 2u) {
        world = beacon_card(p, world, v.uv);
    }
    var out: PartOut;
    out.clip = frame.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    out.uv = v.uv;
    out.material = p.material & 0xFFFFFFu;
    out.alpha = p.alpha;
    out.glow = vec4<f32>(0.0);
    if (mote) {
        // Its look's word: its dust's colour, how purple it is and how much of it shows.
        let w = bitcast<u32>(p.alpha);
        out.alpha = f32(w & 255u) / 255.0 * thinned;
        let g = vec3<f32>(f32(w >> 28u), f32((w >> 24u) & 15u), f32((w >> 20u) & 15u)) / 15.0;
        out.glow = vec4<f32>(pow(g, vec3<f32>(2.2)), f32((w >> 8u) & 255u) / 255.0);
    } else if (((p.flags >> 30u) & 1u) != 0u) {
        let w = bitcast<u32>(p.alpha);
        out.alpha = f32(w & 255u) / 255.0;
        let g = vec3<f32>(f32(w >> 28u), f32((w >> 24u) & 15u), f32((w >> 20u) & 15u)) / 15.0;
        // Strength in the whole part, spread in the fraction.
        out.glow = vec4<f32>(pow(g, vec3<f32>(2.2)),
                             floor(f32((w >> 8u) & 255u) / 255.0 * 254.0) + f32((w >> 16u) & 15u) / 15.0 * 0.999);
    }
    out.cell = p.cell;
    out.size = length(p.m0.xyz);
    out.flags = p.flags;
    out.center = center;
    return out;
}

// A corner of the card round a mote of reach `r` at `c`, turned to the eye and drawn out behind
// it along its `streak`.
fn mote_card(c: vec3<f32>, r: f32, streak: vec3<f32>, uv: vec2<f32>) -> vec3<f32> {
    let to_eye = normalize(frame.camera.xyz - c);
    let flat = streak - to_eye * dot(streak, to_eye);
    let len = length(flat);
    var along = normalize(cross(vec3<f32>(0.0, 0.0, 1.0), to_eye) + vec3<f32>(1e-4, 0.0, 0.0));
    if (len > r * 0.1) {
        along = flat / len;
    }
    let side = cross(to_eye, along);
    return c - flat * 0.5 + along * (uv.x * 2.0 - 1.0) * (r + len * 0.5) + side * (1.0 - uv.y * 2.0) * r;
}

// A mote of dust: a soft round speck lit as the dust it is, glowing purple as it enters.
fn mote_seen(in: PartOut, soft: f32) -> Seen {
    let q = vec2<f32>(in.uv.x * 2.0 - 1.0, 1.0 - in.uv.y * 2.0);
    let r2 = dot(q, q);
    if (r2 >= 1.0) {
        discard;
    }
    let dir = normalize(in.world - frame.camera.xyz);
    let indoor = in.cell != 0u;
    let bits = G_FOLIAGE | select(0u, G_INDOOR, indoor);
    // Dust scatters the sun ahead of it: it glints as the eye looks toward the light.
    let l = frame.sun_dir.xyz;
    var sun = frame.sun_light.rgb;
    if (!has(F_PHYSICAL)) {
        sun = pow(frame.sun.rgb, vec3<f32>(2.2));
    }
    var sun_vis = select(1.0, 0.0, indoor);
    if (!indoor && has(F_SHADOWS)) {
        sun_vis = sun_shadow(in.world, l, view_depth_at(in.world), in.clip.xy);
    }
    let scatter = in.glow.rgb * sun * sun_vis * smokes_phase(dot(dir, l), MOTE_FORWARD) * MOTE_SCATTER;
    let lit = through_air(forward_light(in.glow.rgb, in.world, -dir, -dir, 0.9, 0.0, 0.0, bits, in.clip.xy)
                          + scatter, in.world, dir, indoor);
    // Brightest as it crosses into the portal.
    let k = in.glow.a;
    let glow = additive_light(MOTE_PURPLE * MOTE_GLOW * (1.0 + MOTE_FLARE * 4.0 * k * (1.0 - k)), 0.0, in.world);
    var out: Seen;
    out.additive = false;
    out.colour = vec4<f32>(mix(lit, glow, in.glow.a), clamp(in.alpha, 0.0, 1.0) * (1.0 - r2 * r2) * soft);
    return out;
}

// A corner of the card covering a mist ball of reach `r` at `c`: turned to the eye and drawn on
// the ball's near side, so the ground in front of its heart does not hide it.
fn mist_card(c: vec3<f32>, r: f32, uv: vec2<f32>) -> vec3<f32> {
    let to = c - frame.camera.xyz;
    let d = max(length(to), 1e-3);
    let v = to / d;
    let right = normalize(cross(v, vec3<f32>(0.0, 0.0, 1.0)) + vec3<f32>(1e-4, 0.0, 0.0));
    let up = cross(right, v);
    let near = max(d - r, 0.3);
    var half = near * 4.0;
    if (d > r * 1.05) {
        half = min(r * near / sqrt(d * d - r * r) * 1.05, half);
    }
    let q = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    return frame.camera.xyz + v * near + (right * q.x + up * q.y) * half;
}

fn smoke_hash(p: vec3<f32>) -> f32 {
    var q = fract(p * 0.1031);
    q += dot(q, q.zyx + 31.32);
    return fract((q.x + q.y) * q.z);
}

fn smoke_value(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(mix(smoke_hash(i), smoke_hash(i + vec3<f32>(1.0, 0.0, 0.0)), u.x),
                mix(smoke_hash(i + vec3<f32>(0.0, 1.0, 0.0)), smoke_hash(i + vec3<f32>(1.0, 1.0, 0.0)), u.x), u.y);
    let b = mix(mix(smoke_hash(i + vec3<f32>(0.0, 0.0, 1.0)), smoke_hash(i + vec3<f32>(1.0, 0.0, 1.0)), u.x),
                mix(smoke_hash(i + vec3<f32>(0.0, 1.0, 1.0)), smoke_hash(i + vec3<f32>(1.0, 1.0, 1.0)), u.x), u.y);
    return mix(a, b, u.z);
}

fn smoke_fbm(p: vec3<f32>) -> f32 {
    var q = p;
    var sum = 0.0;
    var w = 0.5;
    for (var k = 0; k < 4; k++) {
        sum += w * smoke_value(q);
        q = vec3<f32>(q.y * 1.6 + q.x * 1.2, q.x * 1.6 - q.y * 1.2, q.z * 1.7) + 11.3;
        w *= 0.5;
    }
    return sum / 0.9375;
}

// A smoke billow's thickness at `q` (-1 to 1 across its card), 0 to 1: a ball whose edge the
// churning noise eats into, more as it ages. It rolls with its age and churns with the clock.
fn smoke_density(q: vec2<f32>, age: f32, seed: f32) -> f32 {
    let turn = seed * 0.37 + age * SMOKE_ROLL;
    let cs = vec2<f32>(cos(turn), sin(turn));
    let r = vec2<f32>(q.x * cs.x - q.y * cs.y, q.x * cs.y + q.y * cs.x);
    let z = frame.wind.w * SMOKE_CHURN + fract(seed * 0.0137) * 40.0;
    let p = vec3<f32>(r * 1.7, z);
    let warp = vec2<f32>(smoke_value(p * 0.9 + 3.1), smoke_value(p * 0.9 + 8.7)) - 0.5;
    let n = smoke_fbm(vec3<f32>(r * 2.2 + warp * 0.7, z * 1.3));
    let ball = 1.0 - length(q + warp * 0.3);
    return clamp(ball * 1.45 + (n - 0.5) * 1.35 - 0.1 - 0.3 * age, 0.0, 1.0);
}

// A smoke particle: a billow lit as a small cloud. Its normal is a ball's bent by the churn, so
// each lobe rounds toward the light, and what lies between it and the sun shades it.
fn smoke_seen(in: PartOut, soft: f32) -> Seen {
    let q = vec2<f32>(in.uv.x * 2.0 - 1.0, 1.0 - in.uv.y * 2.0);
    let age = f32((in.flags >> 2u) & 1023u) / 1023.0;
    let seed = f32(in.flags >> 13u);
    let d = smoke_density(q, age, seed);
    if (d <= 0.0) {
        discard;
    }
    let dir = normalize(in.world - frame.camera.xyz);
    let right = normalize(cross(dir, vec3<f32>(0.0, 0.0, 1.0)) + vec3<f32>(1e-4, 0.0, 0.0));
    let up = cross(right, dir);
    let e = 0.08;
    let g = vec2<f32>(smoke_density(q + vec2<f32>(e, 0.0), age, seed) - d,
                      smoke_density(q + vec2<f32>(0.0, e), age, seed) - d) / e;
    let bulge = sqrt(max(1.0 - dot(q, q), 0.15));
    let n = normalize(right * (q.x - g.x * 0.25) + up * (q.y - g.y * 0.25) - dir * bulge);
    let sun = vec2<f32>(dot(frame.sun_dir.xyz, right), dot(frame.sun_dir.xyz, up));
    let shade = smoke_density(q + sun * 0.3, age, seed);
    let indoor = in.cell != 0u;
    let bits = G_FOLIAGE | select(0u, G_INDOOR, indoor);
    let lit = forward_light(SMOKE_ALBEDO, in.world, n, -dir, 1.0, 0.0, 0.0, bits, in.clip.xy)
        * mix(1.0, 0.55, shade);
    var out: Seen;
    out.additive = false;
    // It swells into being rather than appearing whole.
    let a = smoothstep(0.0, 0.6, d) * smoothstep(0.0, 0.08, age) * in.alpha * SMOKE_OPACITY * soft;
    out.colour = vec4<f32>(through_air(lit, in.world, dir, indoor), a);
    return out;
}

// Fire's colour at heat `h` (0 to 1), as a torch's flame's: red embers, orange, yellow, a
// white-yellow heart; turned to the hue of a fire of another colour than retail's puff.
fn fire_ramp(h: f32, tint: vec3<f32>) -> vec3<f32> {
    var c = mix(vec3<f32>(0.35, 0.03, 0.0), vec3<f32>(1.0, 0.3, 0.03), smoothstep(0.0, 0.3, h));
    c = mix(c, vec3<f32>(1.0, 0.62, 0.16), smoothstep(0.25, 0.6, h));
    c = mix(c, vec3<f32>(1.0, 0.9, 0.62), smoothstep(0.6, 0.95, h));
    let turn = sqrt(clamp(tint / FIRE_PUFF_TINT, vec3<f32>(0.02), vec3<f32>(50.0)));
    let turned = c * turn / max(turn.r, max(turn.g, turn.b));
    return turned * luma(c) / max(luma(turned), 1e-4);
}

// A fire puff: a ball of flame torn by noise rising through it, hottest at its heart while it is
// young, cooling at its rim, its top and as it ages. Retail adds its picture whole, so its
// translucency does not fade it; it swells in and dies away over its life.
fn fire_seen(in: PartOut, soft: f32) -> Seen {
    let q = vec2<f32>(in.uv.x * 2.0 - 1.0, 1.0 - in.uv.y * 2.0);
    let age = f32((in.flags >> 2u) & 1023u) / 1023.0;
    let s = fract(f32(in.flags >> 13u) * 0.0137) * 40.0;
    let w = bitcast<u32>(in.alpha);
    let tint = vec3<f32>(f32(w & 255u), f32((w >> 8u) & 255u), f32((w >> 16u) & 255u)) / 255.0;
    let t = frame.sun_light.w;
    let flow = vec3<f32>(q.x * 1.5 + s, q.y * 1.5 - t * FIRE_RISE, t * FIRE_CHURN + s);
    let warp = vec2<f32>(smoke_value(flow + 3.1), smoke_value(flow * 1.3 + 8.7)) - 0.5;
    let n = smoke_fbm(vec3<f32>(q.x * 2.6 + warp.x * 1.2 + s,
                                q.y * 2.0 + warp.y * 1.2 - t * FIRE_RISE * 1.7, t * FIRE_CHURN * 1.5));
    // A ball, rounder below, which the noise draws up into licks.
    let r = length(vec2<f32>(q.x, q.y * select(1.0, 0.75, q.y > 0.0)) + warp * 0.5);
    let edge = 1.0 - smoothstep(0.7, 1.0, max(abs(q.x), abs(q.y)));
    let d = clamp((1.0 - r) * 1.7 + (n - 0.5) * 1.6 - 0.2 - 0.3 * age, 0.0, 1.0) * edge;
    if (d <= 0.0) {
        discard;
    }
    // A large billow burns cooler than a small tongue.
    let heat = clamp(d * (1.0 - 0.8 * age) / (1.0 + FIRE_COOL * in.size) - 0.15 * max(q.y, 0.0), 0.0,
                     1.0);
    let life = smoothstep(0.0, 0.1, age) * (1.0 - smoothstep(0.65, 1.0, age));
    let light = fire_ramp(heat, tint) * smoothstep(0.0, 0.5, d) * (0.35 + 1.3 * heat * heat) * life
        * FIRE_GAIN;
    var out: Seen;
    out.additive = true;
    out.colour = vec4<f32>(light * (1.0 - fog_amount(distance(in.world.xy, frame.camera.xy))), soft);
    return out;
}

// The particle's colour and coverage, and whether it adds its light.
// A bank of ground mist: the fog of a flattened ball the ray runs through before it meets what
// stands behind (its thickness `1 - r^2`, integrated in closed form), stirred by slow noise and
// lit at the middle of the run.
fn mist_seen(in: PartOut) -> Seen {
    let r = in.size;
    let axes = vec3<f32>(r, r, clamp(r * MIST_HEIGHT, 0.4, 2.0));
    let eye = frame.camera.xyz;
    let dir = normalize(in.world - eye);
    let o = (eye - in.center) / axes;
    let dv = dir / axes;
    let a2 = dot(dv, dv);
    let b = dot(o, dv);
    let c = dot(o, o);
    let disc = b * b - a2 * (c - 1.0);
    if (disc <= 0.0) {
        discard;
    }
    let sq = sqrt(disc);
    let t0 = max((-b - sq) / a2, 0.0);
    var t1 = (-b + sq) / a2;
    let d = textureLoad(g_depth, vec2<i32>(in.clip.xy), 0);
    if (d > 0.0) {
        let ndc = vec2<f32>(in.clip.x / frame.params.z * 2.0 - 1.0, 1.0 - in.clip.y / frame.params.w * 2.0);
        let h = frame.inv_view_proj * vec4<f32>(ndc, d, 1.0);
        t1 = min(t1, dot(h.xyz / h.w - eye, dir));
    }
    if (t1 <= t0) {
        discard;
    }
    let f0 = t0 - (a2 * t0 * t0 * t0 / 3.0 + b * t0 * t0 + c * t0);
    let f1 = t1 - (a2 * t1 * t1 * t1 / 3.0 + b * t1 * t1 + c * t1);
    let mid = eye + dir * (0.5 * (t0 + t1));
    let t = frame.wind.w;
    let stir = smoke_fbm(mid * 0.22 + vec3<f32>(frame.wind.xy * t * -0.04, t * 0.03));
    let tau = max(f1 - f0, 0.0) * MIST_DENSITY * in.alpha * mix(0.3, 1.5, stir);
    // Its picture's colour, whole, tints it.
    let m = materials[in.material];
    var tint = m.base_color.rgb;
    if ((m.flags & M_HAS_BASE) != 0u) {
        tint *= textureSampleLevel(textures[m.tex_base], samp_repeat, vec2<f32>(0.5), 16.0).rgb;
    }
    tint = mix(vec3<f32>(1.0), tint / max(max(tint.r, max(tint.g, tint.b)), 1e-4), 0.4);
    let indoor = in.cell != 0u;
    let bits = G_FOLIAGE | select(0u, G_INDOOR, indoor);
    let lit = forward_light(MIST_ALBEDO * tint, mid, vec3<f32>(0.0, 0.0, 1.0), -dir, 1.0, 0.0, 0.0, bits,
                            in.clip.xy);
    var out: Seen;
    out.additive = false;
    out.colour = vec4<f32>(through_air(lit, mid, dir, indoor), 1.0 - exp(-tau));
    return out;
}

// A creature's light: a core of its picture's colour whitening at its heart, in a soft halo that
// falls away to nothing at its card's edge; a star crossed by two thin rays at its own turn. It is
// as bright as its picture's heart, so a faint haze stays faint.
fn glow_seen(in: PartOut, soft: f32) -> Seen {
    let q = vec2<f32>(in.uv.x * 2.0 - 1.0, 1.0 - in.uv.y * 2.0);
    let r2 = dot(q, q);
    if (r2 >= 1.0) {
        discard;
    }
    let m = materials[in.material];
    let tint = in.glow.rgb;
    let strength = floor(in.glow.a) / 254.0;
    let spread = fract(in.glow.a);
    let star = (in.flags >> 31u) != 0u;
    let k = select(GLOW_BALL, GLOW_STAR, star);
    // A soft ball holding half its light within its picture's spread, faded to nothing at the
    // card's edge; a hot heart a share of that across.
    let width = log(2.0) / max(spread * spread, 0.004);
    let edge = exp(-width);
    let halo = max(exp(-r2 * width) - edge, 0.0) / (1.0 - edge) * k.z;
    // A faint picture is a haze of light, with no hot heart.
    let core = exp(-r2 * width / (k.y * k.y)) * k.x * smoothstep(GLOW_FAINT.x, GLOW_FAINT.y, strength);
    var rays = 0.0;
    if (star) {
        let turn = f32((in.flags >> 13u) & 0x1FFFFu) * 0.000479;
        let cs = vec2<f32>(cos(turn), sin(turn));
        let a = abs(vec2<f32>(q.x * cs.x - q.y * cs.y, q.x * cs.y + q.y * cs.x));
        rays = (exp(-a.y * GLOW_RAY_THIN) * (1.0 - a.x) * (1.0 - a.x)
            + exp(-a.x * GLOW_RAY_THIN) * (1.0 - a.y) * (1.0 - a.y)) * GLOW_RAY;
    }
    let age = f32((in.flags >> 2u) & 1023u) / 1023.0;
    let hot = mix(tint, vec3<f32>(1.0), GLOW_WHITE);
    let light = (tint * (halo + rays) + hot * core) * strength * GLOW_GAIN * (1.0 - GLOW_AGE_DIM * age);
    var out: Seen;
    out.additive = true;
    out.colour = vec4<f32>(additive_light(light, m.emissive.x, in.world), clamp(in.alpha, 0.0, 1.0) * soft);
    return out;
}

fn particle_seen(in: PartOut) -> Seen {
    let mote = (in.flags & MOTE) == MOTE;
    if ((in.flags & 4096u) != 0u && !mote) {
        return mist_seen(in);
    }
    let smoke = (in.flags & 2u) != 0u;
    let m = materials[in.material];
    var c = m.base_color;
    if ((m.flags & M_HAS_BASE) != 0u) {
        c *= textureSampleGrad(textures[m.tex_base], samp_repeat, in.uv, dpdx(in.uv), dpdy(in.uv));
    }
    // A shadow's eyes carry their glow above one (`particles::EYE_GLOW`).
    let glow = max(in.alpha, 1.0);
    var a = c.a * min(in.alpha, 1.0);
    // Retail adds a plain additive picture whole (`ONE / ONE`): its translucency does not fade it.
    if ((m.flags & (M_ADDITIVE | M_ALPHA)) == M_ADDITIVE) {
        a = 1.0;
    }
    let px = vec2<i32>(in.clip.xy);
    let d = textureLoad(g_depth, px, 0);
    var soft = 1.0;
    if (d > 0.0) {
        let ndc = vec2<f32>(in.clip.x / frame.params.z * 2.0 - 1.0, 1.0 - in.clip.y / frame.params.w * 2.0);
        let h = frame.inv_view_proj * vec4<f32>(ndc, d, 1.0);
        let behind = view_depth_at(h.xyz / h.w) - view_depth_at(in.world);
        let reach = select(SOFT_DEPTH, SMOKE_SOFT_DEPTH, smoke) * in.size;
        soft = clamp(behind / max(reach, 0.02), 0.0, 1.0);
    }
    if ((in.flags & 3u) == 3u) {
        return fire_seen(in, soft);
    }
    if (mote) {
        return mote_seen(in, soft);
    }
    if ((in.flags >> 30u) == 2u) {
        return beacon_seen(in, c, a, soft);
    }
    if (smoke) {
        return smoke_seen(in, soft);
    }
    if (((in.flags >> 30u) & 1u) != 0u) {
        return glow_seen(in, soft);
    }
    a *= soft;
    let dir = normalize(in.world - frame.camera.xyz);
    let indoor = in.cell != 0u;
    var out: Seen;
    out.additive = (m.flags & M_ADDITIVE) != 0u;
    if (out.additive) {
        var light = c.rgb;
        if ((in.flags & 1u) != 0u) {
            if (((in.flags >> 13u) & 1u) == 0u) {
                // A magic lamp's halo: its own colour at retail's strength, never raised by the night.
                light = c.rgb * (1.0 + m.emissive.x) * (1.0 - fog_amount(distance(in.world.xy, frame.camera.xy)));
                if (has(F_PHYSICAL)) {
                    light *= MAGIC_HALO;
                } else {
                    light = min(light * pre_exposure(), vec3<f32>(HDR_MOST));
                }
                out.colour = vec4<f32>(light, a);
                return out;
            }
            // Warm and dim, the whole sprite flickering with its flame.
            let seed = bitcast<u32>(floor(in.center.x * 4.0)) ^ (bitcast<u32>(floor(in.center.y * 4.0)) * 7919u);
            light = luma(c.rgb) * LIGHT_SPRITE * LIGHT_SPRITE_GAIN * flicker(1u, seed, 6.0);
        }
        out.colour = vec4<f32>(additive_light(light * glow, m.emissive.x, in.world), a);
        return out;
    }
    let bits = G_FOLIAGE | select(0u, G_INDOOR, indoor);
    // A luminous blended picture is self-lit as retail draws it, never glowing past it.
    let self_lit = c.rgb * m.emissive.x / max(pre_exposure(), 1e-6);
    let lit = max(forward_light(c.rgb, in.world, -dir, -dir, 0.9, 0.0, 0.0, bits, in.clip.xy),
                  self_lit);
    out.colour = vec4<f32>(through_air(lit, in.world, dir, indoor), a);
    return out;
}

// Smoke's and mist's share of the upscaler's reactive mask over their coverage: they drift slowly
// and softly, so the picture behind keeps its history and does not shimmer through them.
const SOFT_REACTIVE: f32 = 0.3;

@fragment
fn fs_particle(in: PartOut) -> SortedOut {
    let s = particle_seen(in);
    var out = sorted_out(s.colour, s.additive);
    if ((in.flags & 3u) == 2u || (in.flags & 4096u) != 0u) {
        out.reactive = vec4<f32>(clamp(out.colour.a * SOFT_REACTIVE, 0.0, 1.0));
    }
    return out;
}

@fragment
fn fs_particle_oit(in: PartOut) -> OitOut {
    let s = particle_seen(in);
    return oit_out(s.colour, s.additive, in.world);
}
