// The backdrop of the screens before the world: the slate void of portal space with its violet
// lights floating through it, and the interface laid over it.

struct Drift {
    // Seconds, width over height, width, height.
    clock: vec4<f32>,
    // Whether the interface is sRGB-encoded, whether there is one, the blur's width under a ground
    // in pixels (UI Blur), unused.
    ui: vec4<f32>,
}

@group(0) @binding(0) var<uniform> D: Drift;
@group(0) @binding(1) var ui_tex: texture_2d<f32>;
// UI Blur: the interface's see-through grounds, the lights blurred under them.
@group(0) @binding(2) var<uniform> grounds: Grounds;

struct Grounds {
    // x: how many.
    count: vec4<u32>,
    // Left, top, right, bottom in pixels.
    rects: array<vec4<f32>, 16>,
    // Their strengths, four to a row.
    strength: array<vec4<f32>, 4>,
}

// How strongly what is under pixel `p` is blurred: the strongest ground over it.
fn frosted(p: vec2<f32>) -> f32 {
    var s = 0.0;
    for (var i = 0u; i < min(grounds.count.x, 16u); i = i + 1u) {
        let r = grounds.rects[i];
        if (p.x >= r.x && p.y >= r.y && p.x < r.z && p.y < r.w) {
            s = max(s, grounds.strength[i / 4u][i % 4u]);
        }
    }
    return s;
}

const DEEP: vec3<f32> = vec3<f32>(0.006, 0.019, 0.032);
const SLATE: vec3<f32> = vec3<f32>(0.14, 0.27, 0.42);
const VIOLET: vec3<f32> = vec3<f32>(0.72, 0.16, 1.25);

@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

fn hash4(p: vec2<f32>) -> vec4<f32> {
    var q = fract(vec4<f32>(p.x, p.y, p.x, p.y) * vec4<f32>(0.1031, 0.1030, 0.0973, 0.1099));
    q = q + dot(q, q.wzxy + 33.33);
    return fract((q.xxyz + q.yzzw) * q.zywx);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash4(i).x;
    let b = hash4(i + vec2<f32>(1.0, 0.0)).x;
    let c = hash4(i + vec2<f32>(0.0, 1.0)).x;
    let d = hash4(i + vec2<f32>(1.0, 1.0)).x;
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    var s = 0.0;
    var a = 0.5;
    var q = p;
    for (var i = 0; i < 4; i = i + 1) {
        s = s + a * noise(q);
        q = q * 2.03 + vec2<f32>(7.1, 3.7);
        a = a * 0.5;
    }
    return s;
}

// How many lights float in the void.
const LIGHTS: i32 = 56;

// The lights of portal space floating in the void, as they fly with the traveller there: violet
// points with a white-hot heart, each pulsing, circling slowly and drifting up, the near ones
// larger, brighter and quicker than the far.
// Under a ground each light is blurred exactly: a Gaussian `blur` wide laid over its own widens it,
// as dim as it is wider.
fn lights(q: vec2<f32>, t: f32, blur: f32) -> vec3<f32> {
    let aspect = D.clock.y;
    let px = 1.0 / D.clock.w;
    var col = vec3<f32>(0.0);
    for (var i = 0; i < LIGHTS; i = i + 1) {
        let h = hash4(vec2<f32>(f32(i) * 1.37, 7.0));
        let g = hash4(vec2<f32>(f32(i) * 2.11, 19.0));
        let near = h.w * h.w;
        let speed = 0.3 + 0.9 * near;
        // Wrapped across a margin wider than the screen, so none pops in or out in view.
        let span = vec2<f32>(aspect + 0.4, 1.4);
        let drift = vec2<f32>(0.004 * (g.x - 0.5), -0.012) * speed * t;
        let base = fract(h.xy + drift / span) * span - span * 0.5;
        let turn = (0.10 + 0.25 * g.y) * select(-1.0, 1.0, g.z < 0.5);
        let a = g.w * 6.2832 + t * turn;
        let orbit = mix(0.015, 0.06, h.z) * vec2<f32>(cos(a), sin(a));
        let r = length(q - (base + orbit));
        let size = mix(0.0022, 0.0075, near);
        let s0 = max(size, px * 0.8);
        let s = sqrt(s0 * s0 + blur * blur);
        let x = r * r / (s * s);
        let dim = (s0 * s0) / (s * s);
        let pulse = 0.7 + 0.3 * sin(t * mix(0.6, 1.6, g.x) + h.z * 40.0);
        let k = pulse * mix(0.35, 1.0, near) * dim;
        col = col + VIOLET * k * (exp(-x) * 14.0 + 0.3 / (1.0 + x * 0.6));
        col = col + vec3<f32>(1.0, 0.85, 1.0) * k * exp(-x * 4.0) * 6.0;
    }
    return col;
}

fn encode(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

fn decode(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

@fragment
fn fs_drift(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let t = D.clock.x;
    let uv = pos.xy / D.clock.zw;
    let q = (uv - 0.5) * vec2<f32>(D.clock.y, 1.0);
    // The void: deep slate, a slow haze of paler slate and violet through it.
    let haze = fbm(q * 1.6 + vec2<f32>(t * 0.008, -t * 0.012));
    let veil = fbm(q * 3.1 - vec2<f32>(t * 0.015, t * 0.006) + haze);
    var c = DEEP;
    c = c + SLATE * 0.07 * smoothstep(0.35, 0.85, haze);
    c = c + VIOLET * 0.03 * smoothstep(0.45, 0.9, veil) * smoothstep(0.3, 0.8, haze);
    var blur = 0.0;
    if (grounds.count.x > 0u) {
        blur = frosted(pos.xy) * D.ui.z / D.clock.w;
    }
    c = c + lights(q, t, blur);
    let d = uv - 0.5;
    c = c * (1.0 - 1.4 * dot(d, d));
    var mapped = vec3<f32>(1.0) - exp(-max(c, vec3<f32>(0.0)) * 1.6);
    if (D.ui.y > 0.5) {
        let u = textureLoad(ui_tex, vec2<i32>(pos.xy), 0);
        if (D.ui.x > 0.5) {
            mapped = decode(u.rgb + (1.0 - u.a) * encode(mapped));
        } else {
            mapped = u.rgb + (1.0 - u.a) * mapped;
        }
    }
    // A little noise against banding in the dark gradients.
    let k = fract(sin(dot(pos.xy, vec2<f32>(12.9898, 78.233))) * 43758.5453) - 0.5;
    mapped = decode(clamp(encode(mapped) + k / 255.0, vec3<f32>(0.0), vec3<f32>(1.0)));
    return vec4<f32>(mapped, 1.0);
}
