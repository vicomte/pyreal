// Portals in the world (`gates.rs`, `puffs::GATE`): a smoke whirl's arms (`smoke.wgsl`'s, Dark
// Vapor's) on a funnel turned to face the eye, leaning a little off it and going round, the arms
// winding in to a dense heart, their noise drawn in along them so they turn faster the nearer they
// come. They glow in the portal's colour, brightest in a ring round its darker eye, over the
// funnel's see-through body, shaded by its own light from above and darker down its throat. The
// light is its own, so it reads alike by day and by night.

const GATE_KIND: u32 = 7u;
// As a smoke whirl's: how much its arms wind (radians over its radius), how fast they turn
// (radians a second) and draw in (radii a second); its noise's ring, its cells across its radius
// and how much denser its heart is.
const GATE_WHIRL: vec3<f32> = vec3<f32>(8.0, 0.9, 0.22);
const GATE_RING: f32 = 2.5;
// Its arms in all, and their sets: how many arms each holds, its share of the turning rate, and
// its strength.
const GATE_ALL: f32 = 8.0;
const GATE_SETS: array<vec3<f32>, 3> = array<vec3<f32>, 3>(
    vec3<f32>(3.0, 1.0, 1.0),
    vec3<f32>(3.0, 0.5, 0.45),
    vec3<f32>(2.0, 1.5, 0.75),
);
// How far an arm's curve bends (radians) and how often along its length; the narrowest and widest
// arms (a power on their profile) and how much each swells and narrows along its length.
const GATE_BEND: vec2<f32> = vec2<f32>(0.35, 6.0);
const GATE_WIDTH: vec2<f32> = vec2<f32>(0.45, 1.9);
const GATE_SWELL: f32 = 0.35;
// Where the arms end (over its radius): about this far, give or take two waves round it.
const GATE_END: vec3<f32> = vec3<f32>(0.8, 0.11, 0.07);
// How much of an arm's strength lies between it and the next, where they overlap.
const GATE_OVERLAP: f32 = 0.3;
const GATE_CELLS: f32 = 3.0;
const GATE_HEART: f32 = 1.5;
// Its half thickness at its heart and its rim, over its radius; how far noise wears its edge and
// how sharp a lump's edge is.
const GATE_THICK: vec2<f32> = vec2<f32>(0.16, 0.36);
const GATE_ERODE: f32 = 0.6;
const GATE_EDGE: f32 = 0.35;
// Its extinction (a radius) at its densest; its glow in multiples of its colour; its eye's radius
// (over its radius) and the glow it keeps; the bright ring round the eye (radius, width, gain) and
// how white it burns.
const GATE_DENSE: f32 = 3.0;
const GATE_GLOW: f32 = 1.8;
const GATE_EYE: vec2<f32> = vec2<f32>(0.12, 0.2);
const GATE_RIM: vec3<f32> = vec3<f32>(0.22, 0.1, 1.4);
const GATE_WHITE: f32 = 0.15;
// The colours (their brightest channel) under which its glow dims, to the share it keeps.
const GATE_DIM: vec3<f32> = vec3<f32>(0.45, 0.7, 0.2);
// How deep its funnel is (over its radius): how far its heart lies behind its rim.
const GATE_FUNNEL: f32 = 1.0;
// How much faster than its clock it turns; how far its axis leans off the eye (a tangent) and how
// fast the lean goes round (radians a second), so its shape shows.
const GATE_PACE: f32 = 1.3;
const GATE_SWAY: vec2<f32> = vec2<f32>(0.3, 0.45);
// Where in its thickness (over the half thickness) its smoke is read, in front and behind.
const GATE_DEPTH: f32 = 0.6;
// Its body, the funnel's face under the arms, lit from above by its own light: how much of what is
// behind it it hides and its glow (in multiples of its colour); the share its unlit walls keep and
// how much darker its throat is; its bright lip (where over its radius, width, gain); where it fades
// out (over its radius, from and to).
const GATE_BODY: vec2<f32> = vec2<f32>(0.55, 0.6);
const GATE_SHADE: vec2<f32> = vec2<f32>(0.1, 0.9);
const GATE_LIP: vec3<f32> = vec3<f32>(0.68, 0.12, 0.4);
const GATE_SKIN: vec2<f32> = vec2<f32>(0.55, 0.86);
// How much of the body's shading the arms take.
const GATE_SELF: f32 = 0.8;
// How much of the daylight its body gives back.
const GATE_DAY: f32 = 0.6;

// How far its funnel's face lies behind its rim at `out` (over its radius) from its middle: its
// heart sunk back away from the eye, so its arms run down into it.
fn gate_sunk(out: f32, radius: f32) -> f32 {
    let x = 1.0 - min(out, 1.0);
    return -GATE_FUNNEL * radius * x * x;
}

// Its smoke at `q` (from its middle, metres): extinction a metre (x) and glow (y).
fn gate_at(q: vec3<f32>, axis: vec3<f32>, u: vec3<f32>, v: vec3<f32>, radius: f32, seed: f32,
           time: f32) -> vec2<f32> {
    let flat = q - axis * dot(q, axis);
    let out = length(flat) / radius;
    if (out >= 1.0) {
        return vec2<f32>(0.0);
    }
    // How far it lies off its funnel's face.
    let along = dot(q, axis) - gate_sunk(out, radius);
    let cells = GATE_CELLS / radius;
    // Wound evenly, its noise laid round the axis and drawn in.
    let angle = atan2(dot(flat, v), dot(flat, u));
    let turn = angle - GATE_WHIRL.x * out + time * GATE_WHIRL.y;
    let ring = smokes_cs(turn) * GATE_RING;
    let nq = vec3<f32>(ring, (out + time * GATE_WHIRL.z) * GATE_CELLS + along * cells * 0.5) + seed;
    let warp = vec3<f32>(smoke_value(nq * 0.6 + 3.1), smoke_value(nq * 0.6 + 8.7), 0.5) - 0.5;
    let n = smokes_fbm3(nq + warp * 1.4);
    // Its arms in sets, each set turning at its own rate and as strong as it is, so arms slide
    // past and over one another. Each arm its own: its curve bends as it winds, it is wider or
    // thinner than the next, swelling and narrowing along its length, and it ends at its own reach,
    // so the rim is ragged, not round.
    let spread = smoothstep(0.05, 0.3, out);
    let wound = angle - GATE_WHIRL.x * out + GATE_BEND.x * sin(out * GATE_BEND.y + angle * 2.0 + seed);
    var joined = 0.0;
    var reach = 0.0;
    for (var j = 0; j < 3; j++) {
        let count = GATE_SETS[j].x;
        let bent = wound + time * GATE_WHIRL.y * GATE_SETS[j].y + f32(j) * 2.1 + seed;
        let lane = floor(count * bent / 6.2831853 + 0.5);
        let which = lane - count * floor(lane / count) + f32(j) * 3.0;
        let own = fract(sin(which * 12.9898 + seed) * 43758.5453);
        let width = mix(GATE_WIDTH.x, GATE_WIDTH.y, own) * (1.0 + GATE_SWELL * sin(out * 9.0 + which * 1.7));
        // As narrow as one of all of them would be in a single set.
        let narrow = (GATE_ALL / count) * (GATE_ALL / count);
        let shape = pow(0.5 + 0.5 * cos(count * bent), narrow / width);
        let end = GATE_END.x + GATE_END.y * sin(3.0 * bent + seed) + GATE_END.z * sin(5.0 * bent + seed * 2.0);
        let tip = smoothstep(end + 0.04, end - 0.22, out);
        joined = 1.0 - (1.0 - joined) * (1.0 - shape * tip * GATE_SETS[j].z);
        reach = max(reach, tip * pow(shape, 0.25));
    }
    let arms = mix(1.0, mix(GATE_OVERLAP * reach, 1.0, joined), spread);
    let tip = mix(1.0, reach, spread);
    let thick = radius * mix(GATE_THICK.x, GATE_THICK.y, out);
    let heart = 1.0 + GATE_HEART * exp(-out / 0.3);
    let whirl = arms * heart * exp(-(along * along) / (thick * thick));
    let ball = (1.0 - out * out * out) * smoothstep(1.0, 0.7, out);
    let base = 1.0 - exp(-ball * 2.5);
    let dense = smoothstep(0.0, GATE_EDGE, base - GATE_ERODE * (1.0 - n)) * whirl * tip * GATE_DENSE / radius;
    let rim = exp(-pow((out - GATE_RIM.x) / GATE_RIM.y, 2.0)) * GATE_RIM.z;
    let eye = mix(GATE_EYE.y, 1.0, smoothstep(GATE_EYE.x * 0.5, GATE_EYE.x * 1.5, out));
    return vec2<f32>(dense, (0.35 + 0.9 * n) * (1.0 + rim) * eye * arms * tip);
}

// The sun's and sky's light outdoors.
fn gate_daylight() -> vec3<f32> {
    if (has(F_PHYSICAL)) {
        return frame.sun_light.rgb * max(frame.sun_dir.z, 0.0) + frame.sky_light.rgb;
    }
    return pow(frame.sun.rgb, vec3<f32>(2.2)) * max(frame.sun_dir.z, 0.0)
        + pow(min(frame.ambient.rgb, vec3<f32>(1.0)), vec3<f32>(2.2));
}

// The portal of the run `in` draws: its light, pre-exposed and premultiplied, and how much of
// what is behind it shows through.
fn gate_volume(in: PuffOut) -> vec4<f32> {
    let p = particles[in.index];
    let g = particles[p.material & 0x7FFFu];
    let c = vec3<f32>(g.m0.w, g.m1.w, g.m2.w);
    let radius = max(g.m2.y, 0.1);
    let seed = g.m1.x * 97.0;
    let time = frame.wind.w * GATE_PACE;
    // Turned to face the eye, whichever side it is seen from, its axis leaning a little off it and
    // going round.
    var axis = normalize(vec3<f32>(g.m1.y, g.m2.z, g.m2.x));
    if (distance(frame.camera.xyz, c) > 1e-3) {
        let e = normalize(frame.camera.xyz - c);
        let side = normalize(cross(e, vec3<f32>(0.0, 0.0, 1.0)) + vec3<f32>(1e-4, 1e-4, 0.0));
        let w = time * GATE_SWAY.y + seed;
        axis = normalize(e + GATE_SWAY.x * (cos(w) * side + sin(w) * cross(side, e)));
    }
    let tint = puff_tint(g.flags);
    let heart_tint = vec3<f32>(g.m0.y, g.m0.z, g.m1.z);
    let strength = clamp(g.alpha, 0.0, 1.0);
    // A dark portal's (a shadow's) smoke barely glows.
    let lit = mix(GATE_DIM.z, 1.0, smoothstep(GATE_DIM.x, GATE_DIM.y, max(max(tint.r, tint.g), tint.b)));
    let eye = frame.camera.xyz;
    let dir = normalize(in.world - eye);
    let wall = ice_room(in.clip.xy, 0.0);
    // Seen face on, the ray crosses the funnel nearly along its axis, where its smoke hardly
    // changes: it is read where the ray meets the funnel's face, through its whole thickness there.
    let dn = dot(dir, axis);
    if (abs(dn) < 0.05) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let to = dot(c - eye, axis);
    var t = to / dn;
    for (var k = 0; k < 5; k++) {
        let r = eye + dir * t - c;
        t = (to + gate_sunk(length(r - axis * dot(r, axis)) / radius, radius)) / dn;
    }
    let q = eye + dir * t - c;
    let s = length(q - axis * dot(q, axis)) / radius;
    if (t <= 0.0 || s >= 1.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let u = normalize(cross(axis, vec3<f32>(0.0, 0.0, 1.0)) + vec3<f32>(1e-4, 1e-4, 0.0));
    let v = cross(axis, u);
    // Its smoke along the ray in front of its middle and behind it, each through half its
    // thickness (a Gaussian's integral), as much of it as stands before what is behind it.
    let thick = radius * mix(GATE_THICK.x, GATE_THICK.y, s);
    let step = dir * (thick * GATE_DEPTH / abs(dn));
    let a = gate_at(q - step, axis, u, v, radius, seed, time);
    let b = gate_at(q + step, axis, u, v, radius, seed, time);
    let fade = exp(GATE_DEPTH * GATE_DEPTH);
    let here = vec2<f32>((a.x + b.x) * 0.5 * fade, (a.y + b.y) * 0.5);
    let span = thick / abs(dn);
    let before = smoothstep(-span, span, wall - t);
    let through = exp(-here.x * span * 1.7724539 * before);
    // Its body: darker where its walls turn away from the eye and from the light above, and down
    // its throat; brighter on its lip. The arms lie over it, shaded alike in part.
    let flat = q - axis * dot(q, axis);
    let n = normalize(axis - flat / max(s * radius, 1e-4) * (2.0 * GATE_FUNNEL * (1.0 - s)));
    let key = smoothstep(-0.45, 0.45, dot(n, vec3<f32>(0.0, 0.0, 1.0)));
    let deep = (1.0 - s) * (1.0 - s);
    let shade = mix(GATE_SHADE.x, 1.0, pow(max(dot(n, -dir), 0.0), 2.0) * key) * (1.0 - GATE_SHADE.y * deep);
    let lip = exp(-pow((s - GATE_LIP.x) / GATE_LIP.y, 2.0)) * GATE_LIP.z;
    let hue = mix(heart_tint, tint, smoothstep(0.15, 0.6, s));
    let rim = exp(-pow((s - GATE_RIM.x) / GATE_RIM.y, 2.0));
    let glow = mix(hue, vec3<f32>(max(max(hue.r, hue.g), hue.b)), GATE_WHITE * rim * lit) * here.y * GATE_GLOW * lit
        * mix(1.0, shade, GATE_SELF);
    let ragged = 0.03 * sin(5.0 * atan2(dot(flat, v), dot(flat, u)) + seed);
    let cover = GATE_BODY.x * smoothstep(GATE_SKIN.y, GATE_SKIN.x, s + ragged) * before;
    let body = tint * (shade + lip) * GATE_BODY.y * lit * cover;
    let light = glow * (1.0 - through) + body * through;
    let clear = 1.0 - fog_amount(distance(c.xy, frame.camera.xy));
    let day = min(tint * gate_daylight() * GATE_DAY * (shade + lip) * cover * clear * pre_exposure(), vec3<f32>(HDR_MOST)) * through;
    let shown = (additive_light(light, 0.0, c) + day) * strength;
    return vec4<f32>(shown, mix(1.0, through * (1.0 - cover), strength));
}
