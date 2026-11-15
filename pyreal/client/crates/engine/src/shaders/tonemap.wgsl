// The tone map: the HDR target with its bloom, exposed and through a tone curve, to the output.
// The neutral curve leaves the low and middle range as it is, so the dats' pictures keep their
// colours, and rolls off only the highlights; ACES (render-hifi's fit) and AgX are the filmic
// ones.

@group(1) @binding(0) var hdr: texture_2d<f32>;
@group(1) @binding(1) var bloom: texture_2d<f32>;
@group(1) @binding(2) var lin: sampler;
// [0] the exposure in force (the auto-exposure's).
@group(1) @binding(3) var<storage, read> exposure: array<f32, 4>;
// For the highlights: the depth, and each pixel's object plus one.
@group(1) @binding(4) var t_depth: texture_depth_2d;
@group(1) @binding(5) var t_objects: texture_2d<u32>;

// The selected object's outline and target ring, and the hovered one's outline.
// The bloom's strength in the dark (an exposure from the first to the second, log2): glows, lamps
// and a life stone's crystal halo against the night.
const NIGHT_BLOOM: vec3<f32> = vec3<f32>(1.0, 3.0, 8.0);

const SELECTED: vec3<f32> = vec3<f32>(1.0, 0.78, 0.25);
const HOVERED: vec3<f32> = vec3<f32>(0.85, 0.9, 1.0);

// How much of an outline in `colour` pixel `px` takes for object `id` (plus one): the pixels
// round it that are the object while it is not, within two pixels.
fn outline(px: vec2<i32>, id: u32) -> f32 {
    let here = textureLoad(t_objects, px, 0).r;
    let top = vec2<i32>(frame.params.zw) - vec2<i32>(1);
    var near = 0.0;
    for (var y = -2; y <= 2; y++) {
        for (var x = -2; x <= 2; x++) {
            let q = clamp(px + vec2<i32>(x, y), vec2<i32>(0), top);
            if (textureLoad(t_objects, q, 0).r == id) {
                let r = length(vec2<f32>(f32(x), f32(y)));
                near = max(near, clamp(2.6 - r, 0.0, 1.0));
            }
        }
    }
    if (here == id) {
        return 0.0;
    }
    return near;
}

// Object `id` (plus one) over pixel `px` brightened a little in `colour`: its own shading kept, so
// a dark object is not washed flat.
fn lift(m: vec3<f32>, px: vec2<i32>, id: u32, colour: vec3<f32>, share: f32) -> vec3<f32> {
    if (textureLoad(t_objects, px, 0).r != id) {
        return m;
    }
    return m * (vec3<f32>(1.0) + colour * share);
}

// The world point under pixel `p` at depth `d`.
fn world_at(p: vec2<f32>, d: f32) -> vec3<f32> {
    let ndc = vec2<f32>(p.x / frame.params.z * 2.0 - 1.0, 1.0 - p.y / frame.params.w * 2.0);
    let h = frame.inv_view_proj * vec4<f32>(ndc, d, 1.0);
    return h.xyz / h.w;
}

// The step to the world point one pixel along `step` from `w`, taken to whichever side lies on
// the same surface (the shorter one), so a silhouette's edge doesn't bend it.
fn surface_step(px: vec2<i32>, p: vec2<f32>, w: vec3<f32>, step: vec2<i32>) -> vec3<f32> {
    let last = vec2<i32>(textureDimensions(t_depth)) - 1;
    let a = clamp(px + step, vec2<i32>(0), last);
    let b = clamp(px - step, vec2<i32>(0), last);
    let to_a = world_at(p + vec2<f32>(step), textureLoad(t_depth, a, 0)) - w;
    let to_b = w - world_at(p - vec2<f32>(step), textureLoad(t_depth, b, 0));
    return select(to_b, to_a, dot(to_a, to_a) < dot(to_b, to_b));
}

// How much a ring round `c` may lie on the world point `w`, whose surface steps a pixel across and
// down are `sx` and `sy`: none on walls and the sides of things, none on a floor above or below.
fn ground_share(w: vec3<f32>, c: vec3<f32>, slack: f32, sx: vec3<f32>, sy: vec3<f32>) -> f32 {
    let up = abs(normalize(cross(sx, sy)).z);
    // As high or low as the slope under it climbs from the middle, so steep hillsides keep it.
    let climb = sqrt(max(1.0 - up * up, 0.0)) / max(up, 0.2);
    if (abs(w.z - c.z) > slack + length(w.xy - c.xy) * (climb + 0.3)) {
        return 0.0;
    }
    return smoothstep(0.2, 0.35, up);
}

// The target ring under the selected object: a band on whatever stands round its feet.
fn ring(px: vec2<i32>, p: vec2<f32>) -> f32 {
    let d = textureLoad(t_depth, px, 0);
    if (d <= 0.0 || textureLoad(t_objects, px, 0).r == frame.select.x) {
        return 0.0;
    }
    let w = world_at(p, d);
    let c = frame.ring;
    let share = ground_share(w, c.xyz, 0.6 + c.w * 0.3,
                             surface_step(px, p, w, vec2<i32>(1, 0)),
                             surface_step(px, p, w, vec2<i32>(0, 1)));
    if (share <= 0.0) {
        return 0.0;
    }
    let r = length(w.xy - c.xy) / max(c.w, 0.05);
    let band = 1.0 - smoothstep(0.0, 0.07, abs(r - 1.0));
    let glow = (1.0 - smoothstep(0.85, 1.0, r)) * smoothstep(0.55, 1.0, r) * 0.25;
    return max(band, glow) * share;
}

// The waypoint's ring: a gold band in a dark edge, so it reads on snow, grass and the night,
// laid at half strength over the ground. Linear: the output shows it as sRGB (255, 184, 40).
const WAYPOINT_FILL: vec3<f32> = vec3<f32>(1.0, 0.48, 0.021);
const WAYPOINT_OPACITY: f32 = 0.5;
const WAYPOINT_EDGE: vec3<f32> = vec3<f32>(0.05, 0.035, 0.02);

// How far `q` stands outside the edge from `a` to `b` of a shape wound anticlockwise.
fn outside_edge(q: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    return dot(q - a, normalize(vec2<f32>(b.y - a.y, a.x - b.x)));
}

// How much of the waypoint's ring pixel `px` takes: x its fill, y its dark edge. The ring lies on
// whatever stands round its middle, the object it rings left bare, with an arrowhead on its rim
// the way to the waypoint.
fn waypoint_ring(px: vec2<i32>, p: vec2<f32>) -> vec2<f32> {
    let c = frame.waypoint;
    if (c.w <= 0.0) {
        return vec2<f32>(0.0);
    }
    let d = textureLoad(t_depth, px, 0);
    let around = bitcast<u32>(frame.waypoint_way.z);
    if (d <= 0.0 || (around != 0u && textureLoad(t_objects, px, 0).r == around)) {
        return vec2<f32>(0.0);
    }
    let w = world_at(p, d);
    let sx = surface_step(px, p, w, vec2<i32>(1, 0));
    let sy = surface_step(px, p, w, vec2<i32>(0, 1));
    let level = ground_share(w, c.xyz, 0.8, sx, sy);
    if (level <= 0.0) {
        return vec2<f32>(0.0);
    }
    // A pixel's size on the ground, in ring radii, for the edges' softness.
    let across = max(length(sx), length(sy));
    let px_size = clamp(across / c.w, 0.002, 0.08);
    let q = (w.xy - c.xy) / c.w;
    let band = abs(length(q) - 1.0) - 0.055;
    // The arrowhead, in the way's own frame: along it, and across it to the left.
    let way = frame.waypoint_way.xy;
    let a = vec2<f32>(dot(q, way), dot(q, vec2<f32>(-way.y, way.x)));
    let tip = vec2<f32>(1.5, 0.0);
    let left = vec2<f32>(0.86, 0.32);
    let right = vec2<f32>(0.86, -0.32);
    let arrow = max(max(outside_edge(a, right, tip), outside_edge(a, tip, left)),
                    outside_edge(a, left, right));
    let shape = min(band, arrow);
    let edge_w = max(2.0 * px_size, 0.035);
    let fill = 1.0 - smoothstep(-px_size, px_size, shape);
    let edge = 1.0 - smoothstep(edge_w - px_size, edge_w + px_size, shape);
    return vec2<f32>(fill, edge) * level;
}

fn aces(x: vec3<f32>) -> vec3<f32> {
    // The ACES filmic curve fitted with its input and output transforms (Hill).
    let m1 = mat3x3<f32>(
        vec3<f32>(0.59719, 0.07600, 0.02840),
        vec3<f32>(0.35458, 0.90834, 0.13383),
        vec3<f32>(0.04823, 0.01566, 0.83777),
    );
    let m2 = mat3x3<f32>(
        vec3<f32>(1.60475, -0.10208, -0.00327),
        vec3<f32>(-0.53108, 1.10813, -0.07276),
        vec3<f32>(-0.07367, -0.00605, 1.07602),
    );
    let v = m1 * x;
    let a = v * (v + 0.0245786) - 0.000090537;
    let b = v * (0.983729 * v + 0.4329510) + 0.238081;
    return clamp(m2 * (a / b), vec3<f32>(0.0), vec3<f32>(1.0));
}

// Khronos PBR Neutral's shoulder, without its toe: below the knee the picture is exactly what
// was lit, as the client's own frame is.
// How far the neutral curve's toe lowers the dark: PBR Neutral's 0.04 offset, eased in below 0.08.
const TOE: f32 = 0.04;

fn neutral(c_in: vec3<f32>) -> vec3<f32> {
    return neutral_to(c_in, 1.0, TOE);
}

// The neutral shoulder rolling off toward `top` instead of white: on a display with headroom
// (EDR), the highlights reach past SDR white, the picture below the knee unchanged.
fn neutral_to(c_in: vec3<f32>, top: f32, toe: f32) -> vec3<f32> {
    let start = 0.8 - 0.04;
    let desaturation = 0.15;
    var c = c_in;
    // The curve's toe (Khronos PBR Neutral's): the darkest few percent eased down, so shadows
    // read dark rather than a grey veil.
    // Only where the sky is seen: a sealed room's light lies wholly in the toe, which would
    // swallow its walls. Its share of the darkest channel is taken from the brightness, the hue
    // kept: taken from each channel, it deepened every dark colour (a faint purple to a strong one).
    if (frame.switches.x != 0u && toe > 0.0) {
        let x = min(c.r, min(c.g, c.b));
        let t = select(toe, x - x * x / (4.0 * toe), x < 2.0 * toe);
        let l = luma(c);
        c *= max(l - t, 0.0) / max(l, 1e-9);
    }
    let peak = max(c.r, max(c.g, c.b));
    if (peak < start) {
        return c;
    }
    let d = top - start;
    let new_peak = top - d * d / (peak + d - start);
    c *= new_peak / peak;
    let g = 1.0 - 1.0 / (desaturation * (peak - new_peak) + 1.0);
    return mix(c, vec3<f32>(new_peak), g);
}

// A highlight's brightness eased on a log scale past one, its hue kept: a lamp pool at night stays
// the lamp's colour rather than the neutral curve's white.
fn shoulder(c: vec3<f32>) -> vec3<f32> {
    let peak = max(c.r, max(c.g, c.b));
    if (peak <= 1.0) {
        return c;
    }
    return c * ((1.0 + log(peak)) / peak);
}

fn agx_curve(x: vec3<f32>) -> vec3<f32> {
    let x2 = x * x;
    let x4 = x2 * x2;
    return 15.5 * x4 * x2 - 40.14 * x4 * x + 31.96 * x4 - 6.868 * x2 * x + 0.4298 * x2
        + 0.1191 * x - 0.00232;
}

// AgX (Sobotka's base look, render-hifi's fit), back to linear for the output's encoding.
fn agx(c: vec3<f32>) -> vec3<f32> {
    let inset = mat3x3<f32>(
        vec3<f32>(0.842479062253094, 0.0423282422610123, 0.0423756549057051),
        vec3<f32>(0.0784335999999992, 0.878468636469772, 0.0784336),
        vec3<f32>(0.0792237451477643, 0.0791661274605434, 0.879142973793104),
    );
    let outset = mat3x3<f32>(
        vec3<f32>(1.19687900512017, -0.0528968517574562, -0.0529716355144438),
        vec3<f32>(-0.0980208811401368, 1.15190312990417, -0.0980434501171241),
        vec3<f32>(-0.0990297440797205, -0.0989611768448433, 1.15107367264116),
    );
    let lo = -12.47393;
    let hi = 4.026069;
    var v = inset * max(c, vec3<f32>(1e-10));
    v = clamp((log2(v) - lo) / (hi - lo), vec3<f32>(0.0), vec3<f32>(1.0));
    v = outset * agx_curve(v);
    return clamp(pow(max(v, vec3<f32>(0.0)), vec3<f32>(2.2)), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_tonemap(in: FsIn) -> @location(0) vec4<f32> {
    let px = vec2<i32>(in.pos.xy);
    // The HDR target was drawn pre-exposed; the exposure in force replaces that.
    let e = exposure[0] / max(pre_exposure(), 1e-30);
    var c = textureLoad(hdr, px, 0).rgb;
    let b = textureSampleLevel(bloom, lin, in.pos.xy / frame.params.zw, 0.0).rgb;
    let dark = smoothstep(NIGHT_BLOOM.y, NIGHT_BLOOM.z, log2(max(exposure[0], 1e-30)));
    c += b * mix(frame.params.y, max(NIGHT_BLOOM.x, frame.params.y), dark);
    if (has(F_PHYSICAL)) {
        // Night vision: under the dimmest light the eye's rods take over, and colour fades
        // toward a blue-grey (a moonlit field, not a purple one).
        let l = luma(c) / max(pre_exposure(), 1e-30);
        let rods = (1.0 - smoothstep(2.0e-5, 2.0e-3, l)) * 0.25;
        c = mix(c, vec3<f32>(luma(c)) * vec3<f32>(0.92, 0.96, 1.1), rods);
    }
    c *= e;
    // The camera's w names the curve: 0 neutral, 1 ACES, 2 AgX.
    var m: vec3<f32>;
    if (frame.camera.w > 1.5) {
        m = agx(c);
    } else if (frame.camera.w > 0.5) {
        m = aces(c);
    } else {
        // The display's headroom over SDR white (`lists.w`, 1 with none): an EDR output lets
        // the highlights past white; the night's shoulder eases only what is past the headroom.
        let top = max(bitcast<f32>(frame.lists.w), 1.0);
        // A night lies wholly in the toe, which would swallow it: the toe eases off in the dark.
        m = clamp(neutral_to(shoulder(c / top) * top, top, TOE * (1.0 - dark)), vec3<f32>(0.0),
                  vec3<f32>(top));
    }
    // The highlights, over the picture.
    if (frame.select.y != 0u && frame.select.y != frame.select.x) {
        m = mix(lift(m, px, frame.select.y, HOVERED, 0.2), HOVERED, outline(px, frame.select.y) * 0.8);
    }
    if (frame.select.x != 0u) {
        var tint = SELECTED;
        if (frame.select.w != 0u) {
            tint = unpack4x8unorm(frame.select.w).rgb;
        }
        m = mix(lift(m, px, frame.select.x, tint, 0.3), tint, outline(px, frame.select.x));
        if (frame.select.z != 0u) {
            m = mix(m, tint, ring(px, in.pos.xy));
        }
    }
    // The waypoint's ring round the player, over the picture.
    let wp = waypoint_ring(px, in.pos.xy);
    // Blended as the screen shows it, so half strength looks like half.
    if (wp.y > 0.0) {
        var shown = pow(max(m, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.2));
        shown = mix(shown, pow(WAYPOINT_EDGE, vec3<f32>(1.0 / 2.2)), wp.y * 0.7 * WAYPOINT_OPACITY);
        shown = mix(shown, pow(WAYPOINT_FILL, vec3<f32>(1.0 / 2.2)), wp.x * WAYPOINT_OPACITY);
        m = pow(shown, vec3<f32>(2.2));
    }
    // About a quarter of an output level of dither, so the sky's gradients do not band.
    let n = fract(52.9829189 * fract(dot(in.pos.xy, vec2<f32>(0.06711056, 5.83715e-3)))) - 0.5;
    return vec4<f32>(max(m + n / 128.0 * sqrt(max(m, vec3<f32>(1e-4))), vec3<f32>(0.0)), 1.0);
}
