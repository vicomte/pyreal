// Smoke and dust (`puffs::SMOKE`, `puffs::DUST`): a creature's puffs, one run of the puffs' list
// a body, marched by `fs_puff`'s card as one volume. Each puff is a soft ball, `(1 - s^2)^2` over
// its reach, as dense as the puff is opaque; the sum is worn away at its thin edges by billowing
// noise laid out from where the body stands. Each step is lit by the sun and the sky through the
// smoke's own thickness toward them (the balls' integrals in closed form at a few points along
// the view, and one look toward the light through its own lumps), many times scattered; the
// sun's shadow, the lamps and the probes are read once at its mean depth. A strike of lightning
// (`bolts.wgsl`) lights it from within along the bolt.

// A ball's reach over its picture's half width at the puff's scale.
const SMOKES_REACH: f32 = 0.85;
// How strong a puff is at full opacity: as two of the client's puffs, as the volume was tuned.
const SMOKES_STRENGTH: f32 = 2.0;
// How much of the lamps' light it takes: its creature's own light stands inside it, so it glows
// round that and no further.
const SMOKES_LAMPS: f32 = 0.12;
// How much deeper a tinted smoke's hue is than its picture's mean (dust keeps its own).
const SMOKES_HUE: f32 = 1.8;
// A whirl (`puffs::VORTEX`): how much its arms wind in (radians over its size), how
// fast it turns (radians a second) and draws in (metres a second); its noise's ring, its arms and
// how much denser its heart is.
const SMOKES_WHIRLING: u32 = 5u;
const SMOKES_WHIRL: vec3<f32> = vec3<f32>(3.0, 0.9, 0.5);
const SMOKES_WHIRL_RING: f32 = 2.5;
const SMOKES_WHIRL_ARMS: f32 = 3.0;
const SMOKES_WHIRL_HEART: f32 = 1.5;
const SMOKES_WHIRL_STEPS: i32 = 24;
// How much of a strike's light it gives back at the least (a dark smoke still glows round a
// strike), and how bright that is on screen, in multiples of white.
const SMOKES_FLASHED: f32 = 0.5;
const SMOKES_FLASH: f32 = 0.8;
// A smoke's size, which its noise and its lumps are scaled by, over its largest puff's reach.
const SMOKES_SIZE: f32 = 0.6;
const SMOKES_STEPS: i32 = 12;
// The points along the view the light's path through the smoke is found at.
const SMOKES_TAPS: i32 = 6;
const SMOKES_NEAR: u32 = 24u;
// The light that comes through a thick smoke scattered many times: its share, and how much
// thinner the smoke is to it.
const SMOKES_MULTIPLE: vec2<f32> = vec2<f32>(0.3, 0.25);
// The albedo of a black picture's smoke (soot still takes some light), how far toward the light
// its own lumps shade it (over its puffs' mean reach), and the share of the balls' smooth
// thickness counted beyond them.
const SMOKES_DARKEST: f32 = 0.008;
// How sharp a lump's edge is: the eroded thickness over which it goes from none to whole.
const SMOKES_EDGE: f32 = 0.35;
// How deep (over its puffs' mean reach) it thins out into what stands behind it.
const SMOKES_SOFT: f32 = 0.25;
// The puffs' mean reach (metres) under which a smoke is worn less, and its finest noise, cells a
// metre.
const SMOKES_WISP: f32 = 1.2;
const SMOKES_FINEST: f32 = 3.0;
// The samples a whirl's noise is smeared over, and its grains a metre.
const SMOKES_STREAKS: i32 = 3;
const SMOKES_GRAIN: f32 = 22.0;
const SMOKES_LUMP: f32 = 0.6;
const SMOKES_FAR: f32 = 0.6;

// A kind's look: its thickness (optical depth through a whole puff), how much of its picture's
// colour it scatters, how far noise wears its edge, its noise cells over its puffs' mean reach,
// how fast it turns and rises (radians and cells a second), its grit, and its forward scattering.
struct SmokesKind {
    thick: f32,
    albedo: f32,
    erode: f32,
    cells: f32,
    swirl: f32,
    rise: f32,
    grit: f32,
    forward: f32,
    twist: f32,
    // The power its picture's colour is raised to (under one lifts a dark grey), and how far
    // round (radians) its noise is smeared by its whirl.
    lift: f32,
    streak: f32,
}

fn smokes_kind(kind: u32) -> SmokesKind {
    if (kind == 4u) {
        // Dust: thinner, sandy, grainy, whirling.
        return SmokesKind(1.0, 0.8, 0.95, 3.2, 0.9, 0.25, 0.7, 0.35, 1.5, 1.0, 1.4);
    }
    // Smoke: thick, dark as its picture, rolling up.
    return SmokesKind(3.0, 1.0, 0.9, 3.0, 0.15, 0.35, 0.0, 0.45, 0.0, 0.85, 0.0);
}

// Particle `i` as a ball: centre and reach.
fn smokes_ball(i: u32) -> vec4<f32> {
    let p = particles[i];
    return vec4<f32>(p.m0.w, p.m1.w, p.m2.w, p.m0.x * SMOKES_REACH);
}

// A puff's colour (its picture's), linear.
fn puff_tint(flags: u32) -> vec3<f32> {
    let c = vec3<f32>(f32((flags >> 27u) & 31u) / 31.0, f32((flags >> 21u) & 63u) / 63.0,
                      f32((flags >> 16u) & 31u) / 31.0);
    return pow(c, vec3<f32>(2.2));
}

// How thick the smoke's balls are along the ray from `x` toward `l` (unit), out of it: each
// ball's `(a - v^2)^2` integrated over its chord.
fn smokes_depth(x: vec3<f32>, l: vec3<f32>, first: u32, count: u32) -> f32 {
    var tau = 0.0;
    for (var k = 0u; k < count; k++) {
        let b = smokes_ball(first + k);
        let o = (x - b.xyz) / b.w;
        let along = dot(o, l);
        let a = 1.0 - (dot(o, o) - along * along);
        if (a <= 0.0) {
            continue;
        }
        let h = sqrt(a);
        if (along >= h) {
            continue;
        }
        let v0 = max(along, -h);
        let g1 = a * a * h - 2.0 * a * h * h * h / 3.0 + h * h * h * h * h / 5.0;
        let g0 = a * a * v0 - 2.0 * a * v0 * v0 * v0 / 3.0 + v0 * v0 * v0 * v0 * v0 / 5.0;
        tau += SMOKES_STRENGTH * clamp(particles[first + k].alpha, 0.0, 1.0) * (g1 - g0);
    }
    return tau;
}

// Light come through optical depth `tau`, with what many scatterings bring through a thick smoke.
fn smokes_through(tau: f32) -> f32 {
    return mix(exp(-tau), exp(-tau * SMOKES_MULTIPLE.y), SMOKES_MULTIPLE.x);
}

fn smokes_phase(c: f32, g: f32) -> f32 {
    return (1.0 - g * g) / pow(max(1.0 + g * g - 2.0 * g * c, 1e-4), 1.5);
}

// A smoke's noise: its middle and kind, the turn of its swirl now, its cells a metre, how far the
// noise has been carried, its seed and its puffs' mean reach.
struct SmokesField {
    centre: vec3<f32>,
    kind: SmokesKind,
    turn: vec2<f32>,
    cells: f32,
    carried: vec3<f32>,
    seed: f32,
    size: f32,
    // Its colour along this view: its puffs' there, by how much of each the view crosses.
    albedo: vec3<f32>,
    // A whirl's axis (none: not a whirl).
    axis: vec3<f32>,
}

// Three octaves of value noise, 0 to 1.
fn smokes_fbm3(p: vec3<f32>) -> f32 {
    var q = p;
    var sum = 0.0;
    var w = 0.5;
    for (var k = 0; k < 3; k++) {
        sum += w * smoke_value(q);
        q = vec3<f32>(q.y * 1.6 + q.x * 1.2, q.x * 1.6 - q.y * 1.2, q.z * 1.7) + 11.3;
        w *= 0.5;
    }
    return sum / 0.875;
}

fn smokes_cs(a: f32) -> vec2<f32> {
    return vec2<f32>(cos(a), sin(a));
}

// The smoke at `x`: x its extinction a metre, y its grains' darkening of its colour; `fine` with
// its whirl's streaks and its grains.
fn smokes_at(x: vec3<f32>, f: SmokesField, near: ptr<function, array<u32, SMOKES_NEAR>>, n_near: u32,
           fine: bool) -> vec2<f32> {
    var shape = 0.0;
    var rho = 0.0;
    for (var k = 0u; k < n_near; k++) {
        let j = (*near)[k];
        let b = smokes_ball(j);
        let q = (x - b.xyz) / b.w;
        let s2 = dot(q, q);
        if (s2 < 1.0) {
            let w = SMOKES_STRENGTH * clamp(particles[j].alpha, 0.0, 1.0) * (1.0 - s2) * (1.0 - s2);
            shape += w;
            rho += w / b.w;
        }
    }
    if (shape <= 1e-4) {
        return vec2<f32>(0.0, 1.0);
    }
    let fk = f.kind;
    // Turned about its middle, the inner part further (a whirl's spiral), rising, carried.
    let r = x - f.centre;
    let twist = fk.twist * exp(-length(r.xy) / max(f.size, 0.1));
    let tw = smokes_cs(twist);
    let a = vec2<f32>(r.x * f.turn.x - r.y * f.turn.y, r.x * f.turn.y + r.y * f.turn.x);
    let spun = vec3<f32>(a.x * tw.x - a.y * tw.y, a.x * tw.y + a.y * tw.x, r.z);
    var nq = (spun - f.carried) * f.cells + f.seed;
    // A whirl: its noise laid round its axis, flowing in and turning faster the nearer the eye.
    var whirl = 1.0;
    if (dot(f.axis, f.axis) > 0.5) {
        let along = dot(r, f.axis);
        let q = r - f.axis * along;
        let rr = length(q);
        let u = normalize(cross(f.axis, vec3<f32>(0.0, 0.0, 1.0)) + vec3<f32>(1e-4, 1e-4, 0.0));
        let v = cross(f.axis, u);
        let time = frame.wind.w;
        // Wound evenly (not tighter at the heart), so a body's bob barely turns it.
        let turn = atan2(dot(q, v), dot(q, u)) - SMOKES_WHIRL.x * rr / max(f.size, 0.1)
            + time * SMOKES_WHIRL.y;
        let ring = smokes_cs(turn) * SMOKES_WHIRL_RING;
        nq = vec3<f32>(ring, (rr + time * SMOKES_WHIRL.z) * f.cells + along * f.cells * 0.5) + f.seed;
        // A flat disc thickening toward its rim, drawn together into a dense heart, its arms
        // standing apart more the further out they wind.
        let out = rr / max(f.size, 0.1);
        let arm = 0.5 + 0.5 * cos(SMOKES_WHIRL_ARMS * turn);
        let arms = mix(1.0, mix(0.3, 1.0, arm), smoothstep(0.1, 0.7, out));
        let thick = max(f.size * (0.5 + 0.6 * out), 0.05);
        let heart = 1.0 + SMOKES_WHIRL_HEART * exp(-out / 0.3);
        whirl = arms * heart * exp(-(along * along) / (thick * thick));
    }
    let warp = vec3<f32>(smoke_value(nq * 0.6 + 3.1), smoke_value(nq * 0.6 + 8.7), 0.5) - 0.5;
    var billow = smokes_fbm3(nq + warp * 1.4);
    if (fine && fk.streak > 0.0) {
        // A whirl's noise smeared round its turn, into streaks.
        for (var k = 1; k < SMOKES_STREAKS; k++) {
            let o = smokes_cs(fk.streak * f32(k) / f32(SMOKES_STREAKS - 1));
            let b = vec3<f32>(spun.x * o.x - spun.y * o.y, spun.x * o.y + spun.y * o.x, spun.z);
            billow += smokes_fbm3((b - f.carried) * f.cells + f.seed + warp * 1.4);
        }
        // The mean of several is flatter than one: its contrast given back.
        billow = 0.5 + (billow / f32(SMOKES_STREAKS) - 0.5) * 2.0;
    }
    // Billows: rounded lumps with clear gaps between them.
    let n = smoothstep(0.25, 0.75, billow);
    let base = 1.0 - exp(-shape * 2.0);
    // A small smoke (a wisp) is worn less, or nothing would be left of it.
    let erode = fk.erode * clamp(f.size / SMOKES_WISP, 0.35, 1.0);
    var dense = smoothstep(0.0, SMOKES_EDGE, base - erode * (1.0 - n)) * whirl;
    var albedo = 1.0;
    if (fine && fk.grit > 0.0) {
        // Grains: specks a few centimetres across, darker and denser than the dust round them.
        let g = smoothstep(0.62, 0.85, smoke_value(spun * SMOKES_GRAIN + f.seed));
        dense = max(dense, g * smoothstep(0.05, 0.4, base)) * mix(1.0, 1.0 + 4.0 * g, fk.grit);
        albedo *= mix(1.0, 1.0 - 0.6 * g, fk.grit);
    }
    return vec2<f32>(fk.thick * dense * rho / shape, albedo);
}

// The smoke of the run `in` draws (a puff kind of `SMOKE` or more): its light, pre-exposed and
// premultiplied, and how much of what is behind it shows through.
fn smokes_volume(in: PuffOut) -> vec4<f32> {
    let p = particles[in.index];
    let first = p.material & 0x7FFFu;
    let all = (p.material >> 15u) & 0xFFu;
    let count = puff_count(first, all);
    let fk = smokes_kind((p.material >> 23u) & 31u);
    let eye = frame.camera.xyz;
    let dir = normalize(in.world - eye);
    let px = in.clip.xy;
    var t0 = 0.0;
    var wall = ice_room(px, 0.0);
    var t1 = wall;
    // Its size: a share of its emitters' largest puff's reach, steady while it lives.
    var size = 0.0;
    for (var k = 0u; k < count; k++) {
        size = max(size, particles[first + k].m2.y);
    }
    size *= SMOKES_REACH * SMOKES_SIZE;
    // It thins where it meets what stands behind it.
    let soft = max(size * SMOKES_SOFT, 0.05);
    // The balls the view crosses, and the span they cover.
    var near: array<u32, SMOKES_NEAR>;
    var n_near = 0u;
    var lo = t1;
    var hi = t0;
    var tint = vec3<f32>(0.0);
    var tint_w = 0.0;
    for (var k = 0u; k < count && n_near < SMOKES_NEAR; k++) {
        let b = smokes_ball(first + k);
        let tc = dot(b.xyz - eye, dir);
        let off = distance(b.xyz, eye + dir * tc);
        if (off < b.w && tc + b.w > t0 && tc - b.w < t1) {
            let half = sqrt(b.w * b.w - off * off);
            lo = min(lo, tc - half);
            hi = max(hi, tc + half);
            near[n_near] = first + k;
            n_near++;
            let w = clamp(particles[first + k].alpha, 0.0, 1.0) * half + 1e-5;
            tint += puff_tint(particles[first + k].flags) * w;
            tint_w += w;
        }
    }
    t0 = max(t0, lo);
    t1 = min(t1, hi);
    if (n_near == 0u || t1 <= t0) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    // The light's path through the smoke, toward the sun and up toward the sky, at a few points.
    let indoor = p.cell != 0u;
    let l = frame.sun_dir.xyz;
    var tau_sun: array<f32, SMOKES_TAPS>;
    var tau_up: array<f32, SMOKES_TAPS>;
    for (var k = 0; k < SMOKES_TAPS; k++) {
        let x = eye + dir * mix(t0, t1, (f32(k) + 0.5) / f32(SMOKES_TAPS));
        tau_sun[k] = select(smokes_depth(x, l, first, count), 0.0, indoor);
        tau_up[k] = smokes_depth(x, vec3<f32>(0.0, 0.0, 1.0), first, count);
    }
    // The noise: turning about the smoke's middle, rising, carried by the wind.
    let time = frame.wind.w;
    let cells = min(fk.cells / max(size, 0.05), SMOKES_FINEST);
    let carried = vec3<f32>(frame.wind.xy * time * 0.25, time * fk.rise / cells);
    // Its picture's colour, its hue deepened (a tinted smoke takes coloured light as its picture
    // shows it; a grey one is unchanged).
    let hue = pow(tint / max(tint_w, 1e-5), vec3<f32>(fk.lift));
    let deep = max(mix(vec3<f32>(luma(hue)), hue, select(SMOKES_HUE, 1.0, fk.grit > 0.0)), vec3<f32>(0.0));
    let albedo = mix(vec3<f32>(SMOKES_DARKEST), vec3<f32>(1.0), deep) * fk.albedo;
    // Laid out from where the body stands, which moves only as it does.
    // A whirl's axis, from its first puff (the same in all of them).
    var axis = vec3<f32>(0.0);
    if (((p.material >> 23u) & PUFF_KINDS) == SMOKES_WHIRLING) {
        axis = vec3<f32>(particles[first].m1.y, particles[first].m2.z, particles[first].m2.x);
    }
    let f = SmokesField(puff_body(first), fk, smokes_cs(time * fk.swirl), cells, carried, 0.0, size, albedo, axis);
    let jitter = fract(52.9829189 * fract(dot(px, vec2<f32>(0.06711056, 0.00583715))) + frame.wind.w * 61.8034);
    // A whirl's thin disc is marched finer, so its rim does not flicker.
    let steps = max(i32(in.steps), select(SMOKES_STEPS, SMOKES_WHIRL_STEPS, dot(axis, axis) > 0.5));
    let dt = (t1 - t0) / f32(steps);
    // How far toward the sun and the sky its own lumps are looked for.
    let reach = size * SMOKES_LUMP;
    var through = 1.0;
    var sun_taken = 0.0;
    var sky_taken = 0.0;
    var flashed = vec3<f32>(0.0);
    let struck = all > count;
    // The lamps (its creature's own glow among them) at its near end, its middle and its far end,
    // from every side.
    var lamp_at: array<vec3<f32>, 3>;
    for (var k = 0; k < 3; k++) {
        lamp_at[k] = vec3<f32>(0.0);
        if (has(F_LAMPS)) {
            let x = eye + dir * mix(t0, t1, f32(k) * 0.5);
            lamp_at[k] = lamps_held_open(lamp_light(x, -dir, -dir, 1.0, vec3<f32>(0.0),
                                    G_VOLUME_LIT | select(0u, G_INDOOR, indoor), view_depth_at(x), px,
                                    false).diffuse * SMOKES_LAMPS, indoor);
        }
    }
    var lamp_taken = vec3<f32>(0.0);
    var mean = vec3<f32>(0.0);
    var taken = 0.0;
    for (var s = 0; s < steps; s++) {
        let t = t0 + (f32(s) + jitter) * dt;
        let x = eye + dir * t;
        let here = smokes_at(x, f, &near, n_near, true);
        if (here.x <= 1e-4) {
            continue;
        }
        // The light's path at this step: the balls between the taps, and its own lumps near by.
        let at = clamp((t - t0) / max(t1 - t0, 1e-4) * f32(SMOKES_TAPS) - 0.5, 0.0, f32(SMOKES_TAPS - 1));
        let k0 = i32(floor(at));
        let k1 = min(k0 + 1, SMOKES_TAPS - 1);
        let w = at - f32(k0);
        var ts = mix(tau_sun[k0], tau_sun[k1], w) * fk.thick * SMOKES_FAR;
        var tu = mix(tau_up[k0], tau_up[k1], w) * fk.thick * SMOKES_FAR;
        // Its own lumps: one look toward the sun (or, in a room, up toward the light).
        let toward = select(l, vec3<f32>(0.0, 0.0, 1.0), indoor);
        let lump = smokes_at(x + toward * reach * 0.5, f, &near, n_near, false).x * reach;
        ts += lump;
        tu += lump * select(0.5, 1.0, indoor);
        let step_through = exp(-here.x * clamp((wall - t) / soft, 0.0, 1.0) * dt);
        let took = through * (1.0 - step_through) * here.y;
        sun_taken += took * smokes_through(ts);
        sky_taken += took * smokes_through(tu);
        let u = clamp((t - t0) / max(t1 - t0, 1e-4) * 2.0, 0.0, 2.0);
        lamp_taken += took * mix(mix(lamp_at[0], lamp_at[1], min(u, 1.0)), lamp_at[2], max(u - 1.0, 0.0));
        // Lit from within by its lightning, along each strike.
        if (struck) {
            flashed += took * bolt_light(x, first, all);
        }
        mean += x * (through - through * step_through);
        taken += through - through * step_through;
        through *= step_through;
        if (through < 0.02) {
            break;
        }
    }
    let alpha = 1.0 - through;
    if (alpha < 0.003) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let m = mean / max(taken, 1e-5);
    // The light at its mean depth.
    let depth = view_depth_at(m);
    var sun = frame.sun_light.rgb;
    var sky = frame.sky_light.rgb;
    if (!has(F_PHYSICAL)) {
        sun = pow(frame.sun.rgb, vec3<f32>(2.2));
        sky = pow(min(frame.ambient.rgb, vec3<f32>(1.0)), vec3<f32>(2.2));
    }
    var sun_vis = 0.0;
    if (!indoor) {
        sun_vis = 1.0;
        if (has(F_SHADOWS)) {
            sun_vis = sun_shadow(m, l, depth, px);
        }
    }
    var around = sky * 0.8;
    if (indoor) {
        around = frame.indoor_light.rgb;
    }
    if (has(F_GI)) {
        let b = probe_light(m, vec3<f32>(0.0, 0.0, 1.0));
        if (indoor) {
            around = mix(around, around * 0.5 + b.rgb, b.a);
        } else {
            around = mix(around, b.rgb, b.a);
        }
    }
    let cos_t = dot(dir, l);
    let phase = mix(smokes_phase(cos_t, 0.7), smokes_phase(cos_t, -0.2), 1.0 - fk.forward);
    let lit = f.albedo * (sun_taken * sun * sun_vis * phase + sky_taken * around + lamp_taken) / alpha;
    var colour = through_air(lit, m, dir, indoor);
    if (frame.umbrae[0].w > 0.0) {
        colour *= umbra(m);
    }
    // The strikes' light it scatters, on screen as bright as the channels are.
    let flash = max(f.albedo, vec3<f32>(SMOKES_FLASHED)) * flashed * SMOKES_FLASH;
    puff_flashed = clamp(luma(flash) * 4.0, 0.0, 1.0);
    return vec4<f32>(colour * alpha + flash, through);
}

