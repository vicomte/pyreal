// Creatures of fire (`body_fire.rs`): a body's record in `flames` is its ball (centre, radius),
// its flames' width and its whirl, followed by its points, each a cone of flame licking up from
// where an emitter rides its parts (or a trail's particle hangs), swinging round as it rises.
// The eye's ray through the ball is marched: the cones merged, torn by noise rising through them,
// glowing from the picture's colour at the edges to white-hot at the heart, added to the frame.

// Steps through a body's ball; how much light its fire gives, and how much of what is behind it
// a depth of its fire hides (a flame's width hides most).
const FIRE_STEPS: u32 = 36u;
// The most of a body's cones one ray passes near.
const FIRE_NEAR: u32 = 24u;
const FIRE_GLOW: f32 = 0.5;
const FIRE_THICK: f32 = 1.5;
// The most its light comes to on screen, in multiples of white.
const FIRE_PEAK: f32 = 1.6;
// How much of its green (and less of its blue) orange fire keeps against the torches'.
const BODY_FIRE_ORANGE: f32 = 0.5;

// A card over the ball at `c` of radius `r`, turned to the eye; the whole screen when the eye is
// in it or so near it fills a wide view (a card at its near face, turned to its far-off centre,
// would miss the fire under the eye). `corner` x -1 to 1, y 0 to 1.
fn body_fire_card(c: vec3<f32>, r: f32, corner: vec2<f32>) -> vec4<f32> {
    let to = c - frame.camera.xyz;
    let d = length(to);
    let xy = vec2<f32>(corner.x, corner.y * 2.0 - 1.0);
    if (d < r * 1.5 + 0.3) {
        return vec4<f32>(xy, 1.0, 1.0);
    }
    let fwd = to / d;
    var right = cross(fwd, vec3<f32>(0.0, 0.0, 1.0));
    right = select(vec3<f32>(1.0, 0.0, 0.0), normalize(right), length(right) > 1e-4);
    let up = cross(right, fwd);
    // The ball's outline, on a plane at its near face, so nothing inside it is cut by the depth.
    let near = d - r;
    let s = r * near / sqrt(d * d - r * r);
    return frame.view_proj * vec4<f32>(c - fwd * r + (right * xy.x + up * xy.y) * s, 1.0);
}

fn fire_hash3(p: vec3<f32>) -> f32 {
    let q = bitcast<vec3<u32>>(vec3<i32>(floor(p)));
    var h = (q.x * 0x9E3779B1u) ^ (q.y * 0x85EBCA77u) ^ (q.z * 0xC2B2AE3Du);
    h = (h ^ (h >> 15u)) * 0x27D4EB2Fu;
    h ^= h >> 13u;
    return f32(h & 0xFFFFu) / 65535.0;
}

// Smooth value noise, 0 to 1.
fn fire_noise3(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = p - i;
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(fire_hash3(i), fire_hash3(i + vec3<f32>(1.0, 0.0, 0.0)), u.x);
    let b = mix(fire_hash3(i + vec3<f32>(0.0, 1.0, 0.0)), fire_hash3(i + vec3<f32>(1.0, 1.0, 0.0)), u.x);
    let c = mix(fire_hash3(i + vec3<f32>(0.0, 0.0, 1.0)), fire_hash3(i + vec3<f32>(1.0, 0.0, 1.0)), u.x);
    let d = mix(fire_hash3(i + vec3<f32>(0.0, 1.0, 1.0)), fire_hash3(i + vec3<f32>(1.0, 1.0, 1.0)), u.x);
    return mix(mix(a, b, u.y), mix(c, d, u.y), u.z);
}

// A cone of flame up the z axis, `h` high, `r1` wide at its foot and `r2` at its tip: distance.
fn fire_cone(p: vec3<f32>, r1: f32, r2: f32, h: f32) -> f32 {
    let b = (r1 - r2) / h;
    let a = sqrt(max(1.0 - b * b, 0.0));
    let q = vec2<f32>(length(p.xy), p.z);
    let k = dot(q, vec2<f32>(-b, a));
    if (k < 0.0) {
        return length(q) - r1;
    }
    if (k > a * h) {
        return length(q - vec2<f32>(0.0, h)) - r2;
    }
    return dot(q, vec2<f32>(a, b)) - r1;
}

// The cones `near` (`m` of them, their records) at `p`: the distance to their merged surface,
// and how near their feet it is (1 at a foot, 0 at a tip). A whirling cone's axis swings round
// at `spin` radians a second, a turn every `twist` metres up.
fn fire_field(p: vec3<f32>, near: ptr<function, array<u32, FIRE_NEAR>>, m: u32, blend: f32,
              spin: f32, twist: f32) -> vec2<f32> {
    var d = 1e9;
    var heat = 0.0;
    var weight = 0.0;
    let t = frame.sun_light.w;
    for (var k = 0u; k < m; k++) {
        let pt = flames[(*near)[k]];
        var q = p - pt.base;
        var dk = 0.0;
        var foot = 0.0;
        if ((pt.flags & 255u) == 6u) {
            // A length of trail, burning cooler than the body.
            let run = vec3<f32>(pt.height, bitcast<f32>(pt.colour), bitcast<f32>(pt.seed));
            let along = clamp(dot(q, run) / max(dot(run, run), 1e-6), 0.0, 1.0);
            dk = length(q - run * along) - pt.reach;
            foot = 0.35;
        } else {
            let swing = bitcast<f32>(pt.seed);
            if (swing > 0.0) {
                let a = spin * t - q.z * 6.2831853 / twist;
                q = vec3<f32>(q.xy - swing * vec2<f32>(cos(a), sin(a)) * smoothstep(0.0, 0.3, q.z / pt.height), q.z);
            }
            dk = fire_cone(q, pt.reach, bitcast<f32>(pt.colour), pt.height);
            foot = 1.0 - clamp(q.z / pt.height, 0.0, 1.0);
        }
        // A smooth union, so limbs' fires run together.
        let h = clamp(0.5 + 0.5 * (dk - d) / blend, 0.0, 1.0);
        d = mix(dk, d, h) - blend * h * (1.0 - h);
        let w = exp(-max(dk, 0.0) / blend);
        heat += w * foot;
        weight += w;
    }
    return vec2<f32>(d, heat / max(weight, 1e-4));
}

// Fire's colour at temperature `t` (0 to 1) in the colour of its picture `tint`: deep and dark
// at its edges, the picture's colour, then paling white-hot; as bright at each heat as the
// torches' fire. Orange fire is the torches' held deeper orange, short of yellow but at its core,
// and a little dimmer.
fn body_fire_colour(t: f32, tint: vec3<f32>) -> vec3<f32> {
    let warm = fire_colour(t) * mix(0.35, 1.0, smoothstep(0.0, 0.5, t));
    if (tint.r >= tint.g && tint.g >= tint.b) {
        let orange = fire_colour(t * 0.8) * vec3<f32>(1.0, BODY_FIRE_ORANGE, BODY_FIRE_ORANGE * 0.1);
        // Half way back to the torches' brightness: brighter, it would pale to pink on screen.
        return orange * sqrt(luma(warm) / max(luma(orange), 1e-4));
    }
    let p = pow(max(tint, vec3<f32>(1e-3)), vec3<f32>(mix(3.0, 0.2, t)));
    let c = mix(p, mix(tint, vec3<f32>(1.0), 0.7), smoothstep(0.8, 1.0, t) * 0.4);
    return c * luma(warm) / max(luma(c), 1e-4);
}

// The light a body of fire adds along the eye's ray through pixel `in.clip`.
fn body_fire_light(in: FlameOut) -> vec3<f32> {
    let body = flames[in.index];
    let n = body.flags >> 16u;
    let centre = body.base;
    let tint = vec3<f32>(f32(body.colour & 255u), f32((body.colour >> 8u) & 255u),
                         f32((body.colour >> 16u) & 255u)) / 255.0;
    let t = frame.sun_light.w;
    let px = vec2<i32>(in.clip.xy);
    let ndc = vec2<f32>(in.clip.x / frame.params.z * 2.0 - 1.0, 1.0 - in.clip.y / frame.params.w * 2.0);
    let eye = frame.camera.xyz;
    let far = frame.inv_view_proj * vec4<f32>(ndc, 0.5, 1.0);
    let dir = normalize(far.xyz / far.w - eye);
    // The fire's scale (its flames' mean width, from its emitters) and its whirl.
    let size = max(body.reach, 0.02);
    let spin = f32((body.flags >> 10u) & 63u) * 0.25;
    let twist = size * 8.0;
    let blend = size * 1.4;
    let tear = size * 1.8;
    // The cones the ray passes near, and where it is near any, cut short by what stands there.
    var near: array<u32, FIRE_NEAR>;
    var m = 0u;
    var t0 = 1e9;
    var t1 = -1e9;
    for (var k = 1u; k <= n; k++) {
        let pt = flames[in.index + k];
        var mid = pt.base + vec3<f32>(0.0, 0.0, pt.height * 0.5);
        var reach = pt.height * 0.5 + pt.reach + bitcast<f32>(pt.seed) + tear + blend * 0.25;
        if ((pt.flags & 255u) == 6u) {
            let run = vec3<f32>(pt.height, bitcast<f32>(pt.colour), bitcast<f32>(pt.seed));
            mid = pt.base + run * 0.5;
            reach = length(run) * 0.5 + pt.reach + tear + blend * 0.25;
        }
        let oc = eye - mid;
        let b = dot(oc, dir);
        let h = b * b - (dot(oc, oc) - reach * reach);
        if (h > 0.0 && -b + sqrt(h) > 0.0 && m < FIRE_NEAR) {
            near[m] = in.index + k;
            m++;
            t0 = min(t0, -b - sqrt(h));
            t1 = max(t1, -b + sqrt(h));
        }
    }
    t0 = max(t0, 0.0);
    let depth = textureLoad(g_depth, px, 0);
    var solid = 1e9;
    if (depth > 0.0) {
        let s = frame.inv_view_proj * vec4<f32>(ndc, depth, 1.0);
        solid = distance(s.xyz / s.w, eye);
        t1 = min(t1, solid);
    }
    if (m == 0u || t1 <= t0) {
        return vec3<f32>(0.0);
    }
    let s = f32(body.seed & 1023u) * 0.731;
    // Noise rising through it, stretched upright, so it licks, as fast as a flame that wide
    // rises (about the root of its width times gravity): a big one rolls slowly.
    let stretch = vec3<f32>(1.0, 1.0, 0.5) / (size * 1.3);
    let rise = t * 0.45 * sqrt(19.6 * size) * stretch.z;
    let dt = (t1 - t0) / f32(FIRE_STEPS);
    let jitter = fract(52.9829189 * fract(dot(in.clip.xy, vec2<f32>(0.06711056, 0.00583715))) + t * 7.31);
    var light = vec3<f32>(0.0);
    var through = 1.0;
    var at = t0 + dt * jitter;
    for (var k = 0u; k < FIRE_STEPS; k++) {
        if (at >= t1) {
            break;
        }
        let p = eye + dir * at;
        let f = fire_field(p, &near, m, blend, spin, twist);
        // Far outside the fire: on toward it.
        if (f.x > tear) {
            at += max(dt, (f.x - tear) * 0.8);
            continue;
        }
        var o = p - centre;
        // A whirling fire's noise turns round its axis, wound into a spiral up it.
        if (spin > 0.0) {
            let a = spin * t - o.z * 6.2831853 / twist;
            let cs = vec2<f32>(cos(a), sin(a));
            o = vec3<f32>(o.x * cs.x - o.y * cs.y, o.x * cs.y + o.y * cs.x, o.z);
        }
        let q = o * stretch + vec3<f32>(s, -s, -rise);
        let n1 = fire_noise3(q);
        let n2 = fire_noise3(q * 2.3 + vec3<f32>(0.0, rise * 0.45, -rise * 0.7));
        let n3 = fire_noise3(q * 5.1 + vec3<f32>(rise * 0.55, 0.0, -rise * 1.3));
        let turb = n1 * 0.55 + n2 * 0.3 + n3 * 0.15 - 0.5;
        // Torn more the higher it licks; a whirling fire wound in two bands up it.
        var d = f.x + turb * tear * (0.4 + 1.3 * (1.0 - f.y));
        if (spin > 0.0) {
            d += sin(2.0 * atan2(o.y, o.x) + n1 * 2.0) * size * 0.5;
        }
        let dense = clamp(-d / (size * 0.3), 0.0, 1.0);
        if (dense > 0.0) {
            let heat = clamp(f.y * 0.45 + dense * 0.55 - 0.4 + (n2 - 0.5) * 0.6, 0.0, 1.0);
            // Fading where it meets what stands in it.
            let k = dense * dt / size * clamp((solid - at) / (size * 0.6), 0.0, 1.0);
            light += through * body_fire_colour(heat, tint) * (0.06 + 2.6 * pow(heat, 2.5)) * k;
            through *= exp(-FIRE_THICK * k);
            if (through < 0.02) {
                break;
            }
        }
        at += dt;
    }
    light *= FIRE_GLOW * in.flick.rgb * FLAME_GAIN;
    // Rolled off toward FIRE_PEAK with its hue kept, so its heart blooms without paling to white.
    light /= 1.0 + max(light.r, max(light.g, light.b)) / FIRE_PEAK;
    return light * (1.0 - fog_amount(distance(centre.xy, frame.camera.xy)));
}
