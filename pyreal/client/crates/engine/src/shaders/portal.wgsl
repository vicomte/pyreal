// Portal space: a bending tube flown through, drawn in HDR at a capped size, with the world's last
// picture warped into it on the way in.

struct Portal {
    // xyz the eye in the tube's frame, w the clock.
    eye: vec4<f32>,
    // xyz the camera's axes; right.w the horizontal half-view tangent, up.w the vertical's,
    // fwd.w how much of the tunnel shows.
    right: vec4<f32>,
    up: vec4<f32>,
    fwd: vec4<f32>,
    // The spine's two waves across and two up: amplitudes, frequencies, phases.
    path_a: vec4<f32>,
    path_f: vec4<f32>,
    path_p: vec4<f32>,
    // Radius, swell, swell frequency, swell phase.
    shape: vec4<f32>,
    // Colours: the walls (w spin), the filaments (w strength), the sparks (w density), the light
    // at the end (w strength).
    deep: vec4<f32>,
    streak: vec4<f32>,
    spark: vec4<f32>,
    glow: vec4<f32>,
    // xyz the light at the end, w its reach.
    light: vec4<f32>,
    // The hole: its centre (NDC), radius, side (+1 the world outside, -1 inside, 0 none).
    hole: vec4<f32>,
    // Twist, flash, speed (m/s), streaks.
    warp: vec4<f32>,
    // Aspect, whether a world picture is bound, the rim's strength, a pixel's width at a metre.
    view: vec4<f32>,
    // The lights flying with the eye: where each is (w its size), and where it was a moment ago
    // (w its brightness).
    motes: array<vec4<f32>, 12>,
    trails: array<vec4<f32>, 12>,
}

@group(0) @binding(0) var<uniform> P: Portal;
@group(0) @binding(1) var world_tex: texture_2d<f32>;
@group(0) @binding(2) var lin: sampler;

const PI: f32 = 3.14159265;
const FAR: f32 = 420.0;
const MOTES: i32 = 12;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> VsOut {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    var o: VsOut;
    o.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    o.uv = vec2<f32>(x, y);
    return o;
}

fn hash3(p: vec3<f32>) -> f32 {
    var q = fract(p * 0.1031);
    q = q + dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn hash2(p: vec2<f32>) -> f32 {
    var q = fract(vec3<f32>(p.xyx) * 0.1031);
    q = q + dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn noise3(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(hash3(i), hash3(i + vec3<f32>(1.0, 0.0, 0.0)), u.x);
    let b = mix(hash3(i + vec3<f32>(0.0, 1.0, 0.0)), hash3(i + vec3<f32>(1.0, 1.0, 0.0)), u.x);
    let c = mix(hash3(i + vec3<f32>(0.0, 0.0, 1.0)), hash3(i + vec3<f32>(1.0, 0.0, 1.0)), u.x);
    let d = mix(hash3(i + vec3<f32>(0.0, 1.0, 1.0)), hash3(i + vec3<f32>(1.0, 1.0, 1.0)), u.x);
    return mix(mix(a, b, u.y), mix(c, d, u.y), u.z);
}

fn fbm(p: vec3<f32>) -> f32 {
    var s = 0.0;
    var a = 0.5;
    var q = p;
    for (var i = 0; i < 3; i = i + 1) {
        s = s + a * noise3(q);
        q = q * 2.03 + vec3<f32>(1.7, 9.2, 3.1);
        a = a * 0.5;
    }
    return s;
}

// Thin bright lines where the noise crosses its middle.
fn ridge(p: vec3<f32>) -> f32 {
    var s = 0.0;
    var a = 0.6;
    var q = p;
    for (var i = 0; i < 3; i = i + 1) {
        let n = 1.0 - abs(noise3(q) * 2.0 - 1.0);
        s = s + a * n * n;
        q = q * 2.1 + vec3<f32>(4.1, 1.3, 7.7);
        a = a * 0.45;
    }
    return s;
}

fn spine(z: f32) -> vec2<f32> {
    return vec2<f32>(
        P.path_a.x * sin(z * P.path_f.x + P.path_p.x) + P.path_a.y * sin(z * P.path_f.y + P.path_p.y),
        P.path_a.z * sin(z * P.path_f.z + P.path_p.z) + P.path_a.w * sin(z * P.path_f.w + P.path_p.w),
    );
}

fn radius(z: f32) -> f32 {
    return P.shape.x * (1.0 + P.shape.y * sin(z * P.shape.z + P.shape.w));
}

// How far inside the wall a point is.
fn inside(p: vec3<f32>) -> f32 {
    return radius(p.z) - length(p.xy - spine(p.z));
}

// The wall's light where a ray meets it.
fn wall(p: vec3<f32>, rd: vec3<f32>, t: f32) -> vec3<f32> {
    let time = P.eye.w;
    let spin = P.deep.w;
    let q = p.xy - spine(p.z);
    let ang = atan2(q.y, q.x);
    // Each tunnel's own pattern.
    let v = p.z + P.path_p.y * 157.0;
    let a = ang + v * 0.04 * spin + time * 0.5 * spin;
    let cs = vec2<f32>(cos(a), sin(a));
    let n = fbm(vec3<f32>(cs * 1.4, v * 0.03 - time * 0.2));
    let r = ridge(vec3<f32>(cs * 2.0 + vec2<f32>(n, -n) * 0.9, v * 0.018 - time * 0.35));
    let filaments = pow(clamp(r, 0.0, 1.0), 9.0);
    let hue = noise3(vec3<f32>(cs * 0.8, v * 0.008 + time * 0.05));
    // The walls drift between their own colour and a paler one.
    let alt = P.streak.rgb * length(P.deep.rgb) * 0.9;
    let deep = mix(P.deep.rgb, alt, smoothstep(0.35, 0.75, hue));
    var col = deep * (0.15 + 1.6 * n * n) + P.streak.rgb * filaments * P.streak.w;
    // Bands round the tube, for the speed.
    let band = pow(0.5 + 0.5 * cos(v * (2.0 * PI / 18.0)), 30.0);
    col = col + P.streak.rgb * band * 0.35 * (0.4 + n);
    // Sparks stuck to the wall, twinkling.
    let cell = vec2<f32>(floor((ang + PI) * 7.0), floor(v / 2.5));
    let h = hash2(cell);
    if (h > 1.0 - P.spark.w) {
        let f = vec2<f32>(fract((ang + PI) * 7.0), fract(v / 2.5)) - 0.5;
        let tw = 0.5 + 0.5 * sin(time * 9.0 + h * 60.0);
        col = col + P.spark.rgb * exp(-dot(f, f) * 60.0) * (2.0 + 6.0 * tw);
    }
    // Grazing walls catch more of the light.
    let nrm = vec3<f32>(-q / max(length(q), 1e-3), 0.0);
    let facing = abs(dot(rd, nrm));
    col = col * (0.55 + 0.9 * (1.0 - facing));
    // The light at the end washes the walls near it.
    let near = exp(-abs(p.z - P.light.z) / 45.0) * P.glow.w;
    col = col + P.glow.rgb * P.deep.rgb * near * 3.0;
    // The flying lights cast their colour on the walls they pass.
    for (var i = 0; i < MOTES; i = i + 1) {
        let m = P.motes[i];
        let d = p - m.xyz;
        col = col + P.spark.rgb * P.trails[i].w * 0.3 / (dot(d, d) + 3.0);
    }
    // Far walls fade to the tunnel's own haze.
    let haze = 1.0 - exp(-t * 0.006);
    return mix(col, P.deep.rgb * 0.35, haze);
}

// The inner veil's wisps where a ray crosses it: colour and cover.
fn veil(p: vec3<f32>) -> vec4<f32> {
    let time = P.eye.w;
    let spin = -P.deep.w;
    let q = p.xy - spine(p.z);
    let ang = atan2(q.y, q.x) + p.z * 0.06 * spin + time * 0.9 * spin;
    let cs = vec2<f32>(cos(ang), sin(ang));
    let n = fbm(vec3<f32>(cs * 1.1, p.z * 0.025 - time * 0.5));
    let w = smoothstep(0.45, 0.85, n);
    return vec4<f32>(mix(P.deep.rgb * 2.0, P.streak.rgb * 0.5, w * w) * (0.5 + w), w * 0.5);
}

// The flying lights, each a glowing point drawn out along where it was a moment ago, seen along
// a ray that meets nothing nearer than `far`.
fn lights(ro: vec3<f32>, rd: vec3<f32>, far: f32) -> vec3<f32> {
    var col = vec3<f32>(0.0);
    for (var i = 0; i < MOTES; i = i + 1) {
        let k = P.trails[i].w;
        if (k <= 0.0) {
            continue;
        }
        let a = P.motes[i].xyz;
        let ab = P.trails[i].xyz - a;
        let w0 = ro - a;
        let b = dot(rd, ab);
        let c = dot(ab, ab);
        let denom = c - b * b;
        var u = 0.0;
        if (denom > 1e-6) {
            u = clamp((dot(ab, w0) - b * dot(rd, w0)) / denom, 0.0, 1.0);
        }
        let q = a + ab * u;
        let along = dot(q - ro, rd);
        if (along <= 0.2 || along >= far) {
            continue;
        }
        let miss = length(ro + rd * along - q);
        // Never smaller than about a pixel, its light kept the same.
        let s = max(P.motes[i].w, along * P.view.w * 1.2);
        let keep = (P.motes[i].w * P.motes[i].w) / (s * s);
        let x = miss * miss / (s * s);
        col = col + P.spark.rgb * k * keep * (exp(-x) * 14.0 + 0.3 / (1.0 + x * 0.6));
        // A white-hot heart.
        col = col + vec3<f32>(1.0, 0.85, 1.0) * k * keep * exp(-x * 4.0) * 6.0;
    }
    return col;
}

fn tunnel(ndc: vec2<f32>) -> vec3<f32> {
    let rd = normalize(P.fwd.xyz + ndc.x * P.right.w * P.right.xyz + ndc.y * P.up.w * P.up.xyz);
    let ro = P.eye.xyz;
    var t = 0.2;
    var hit = false;
    var veil_t = -1.0;
    var mist = 0.0;
    for (var i = 0; i < 56; i = i + 1) {
        let p = ro + rd * t;
        let d = inside(p);
        if (d < 0.01 * t + 0.01) {
            hit = true;
            break;
        }
        let r = radius(p.z);
        if (veil_t < 0.0 && d < r * 0.3) {
            veil_t = t;
        }
        let step = max(d * 0.8, 0.05);
        mist = mist + step * exp(-(r - d) * 0.6) * exp(-t * 0.012);
        t = t + step;
        if (t > FAR) {
            break;
        }
    }
    // A ray out of steps short of the far haze is at a wall it grazes: drawn as one, not as a gap.
    if (!hit && t < FAR) {
        hit = true;
    }
    if (hit) {
        // Settled onto the wall, so its patterns do not step along the edges.
        for (var k = 0; k < 2; k = k + 1) {
            t = max(t + inside(ro + rd * t) * 0.9, 0.2);
        }
    }
    var col: vec3<f32>;
    if (hit) {
        col = wall(ro + rd * t, rd, t);
    } else {
        col = P.deep.rgb * 0.35;
    }
    // The core of the tube glows faintly.
    col = col + P.streak.rgb * mist * 0.003;
    // The light at the end, seen where the walls do not stand before it.
    let to = P.light.xyz - ro;
    let along = dot(to, rd);
    if (along > 0.0 && (!hit || along < t)) {
        let miss = length(to - rd * along);
        let reach = P.light.w;
        col = col + P.glow.rgb * P.glow.w * (exp(-miss * miss / (reach * reach)) * 6.0
            + exp(-miss / (reach * 6.0)) * 0.15);
    }
    if (veil_t > 0.0 && veil_t < t) {
        let v = veil(ro + rd * veil_t);
        col = mix(col, v.rgb, v.a);
    }
    col = col + lights(ro, rd, select(FAR, t, hit));
    return col;
}

// What the tone map makes of an HDR value, undone: a picture already mapped goes back to HDR.
fn unmap(c: vec3<f32>) -> vec3<f32> {
    return -log(vec3<f32>(1.0) - min(c, vec3<f32>(0.996)));
}

fn world_at(uv: vec2<f32>) -> vec3<f32> {
    return unmap(textureSampleLevel(world_tex, lin, uv, 0.0).rgb);
}

// Fast lines streaming out from where the tunnel goes.
fn streaks(ndc: vec2<f32>, centre: vec2<f32>) -> f32 {
    let d = (ndc - centre) * vec2<f32>(P.view.x, 1.0);
    let r = length(d);
    let a = atan2(d.y, d.x);
    let bins = 240.0;
    let b = floor((a + PI) / (2.0 * PI) * bins);
    let h = hash2(vec2<f32>(b, 7.0));
    if (h < 0.55) {
        return 0.0;
    }
    let across = abs(fract((a + PI) / (2.0 * PI) * bins) - 0.5);
    let s = fract(log(r + 0.02) * 1.4 - P.eye.w * (0.6 + h * 1.2) * (P.warp.z / 60.0) + h * 13.0);
    let line = smoothstep(0.0, 0.05, s) * (1.0 - smoothstep(0.08, 0.22, s));
    return line * (1.0 - smoothstep(0.15, 0.5, across)) * smoothstep(0.1, 0.9, r) * (h - 0.55) * 2.2;
}

@fragment
fn fs_portal(in: VsOut) -> @location(0) vec4<f32> {
    let ndc = vec2<f32>(in.uv.x * 2.0 - 1.0, 1.0 - in.uv.y * 2.0);
    let aspect = P.view.x;
    let span = length(vec2<f32>(aspect, 1.0));
    let side = P.hole.w;
    let off = (ndc - P.hole.xy) * vec2<f32>(aspect, 1.0) / span;
    let r = length(off);
    let h = P.hole.z;
    let edge = 0.015 + 0.03 * h;
    // How much of this pixel is the world: outside the hole as it opens.
    var world = 0.0;
    if (side > 0.5) {
        world = smoothstep(h - edge, h + edge, r);
    }
    var col = vec3<f32>(0.0);
    if (world < 0.999) {
        col = tunnel(ndc) * P.fwd.w;
        col = col + P.streak.rgb * streaks(ndc, P.hole.xy) * P.warp.w * P.fwd.w;
    }
    if (world > 0.001) {
        // The world near the rim is wound round, and pushed out as the hole opens.
        let ang = atan2(off.y, off.x);
        let wind = P.warp.x * exp(-max(r - h, 0.0) * 3.5);
        let src_r = max(r - h, 0.0) / (1.0 - 0.5 * min(h, 1.0));
        let a2 = ang + wind;
        let src = P.hole.xy + vec2<f32>(cos(a2), sin(a2)) * src_r * span / vec2<f32>(aspect, 1.0);
        let uv = vec2<f32>(src.x * 0.5 + 0.5, 0.5 - src.y * 0.5);
        var w = P.glow.rgb * P.glow.w * 2.0;
        if (P.view.y > 0.5) {
            // A little colour split where the world is pulled hardest.
            let pull = clamp(abs(wind) * 0.003, 0.0, 0.004);
            let dir = normalize(off + vec2<f32>(1e-4, 0.0)) * pull;
            w = vec3<f32>(world_at(uv + dir).r, world_at(uv).g, world_at(uv - dir).b);
        }
        col = mix(col, w, world);
    }
    // The rim burns where the two meet.
    if (abs(side) > 0.5) {
        let ang = atan2(off.y, off.x);
        let ragged = (noise3(vec3<f32>(cos(ang) * 3.0, sin(ang) * 3.0, P.eye.w * 2.5)) - 0.5) * edge * 2.0;
        let x = (r - h - ragged) / edge;
        let core = exp(-x * x * 4.0);
        let halo = exp(-x * x * 0.35);
        col = col + (P.streak.rgb * 3.0 * core + P.spark.rgb * 0.35 * halo) * P.view.z;
    }
    col = col + (P.glow.rgb + P.streak.rgb) * 0.5 * P.warp.y;
    return vec4<f32>(col, 1.0);
}

