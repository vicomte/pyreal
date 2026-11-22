// Translucent surfaces, drawn after lighting over the HDR target, depth tested against the
// opaque world but not written. They take the light the opaque world takes: the sun with its
// cascades' shadow, the clustered lamps (their ray-traced shadows belong to the opaque pixels,
// so not those), the probes' bounce, the sky. Drawn sorted far to near, each object whole, or
// order-independently (weighted, blended: McGuire and Bavoil 2013) into an accumulation and a
// revealage target the `oit` pass lays over the picture.

// The frame's depth, read for the lamps' and the sun's light and the particles' soft edges (the
// pass holds it read-only).
@group(3) @binding(9) var g_depth: texture_depth_2d;

// What additive light (flames, sparks, portals) gives off per unit of its picture, scene units:
// a flame's brightness, which holds its own against daylight.
const ADDITIVE_UNITS: f32 = 0.6;
// The most additive light shows on screen, in multiples of white.
const ADDITIVE_MOST: f32 = 2.5;

// The light a translucent point takes, pre-exposed (no fog or air yet).
fn forward_light(albedo: vec3<f32>, world: vec3<f32>, n: vec3<f32>, v: vec3<f32>, rough: f32,
                 metal: f32, emissive: f32, bits: u32, px: vec2<f32>) -> vec3<f32> {
    let depth = view_depth_at(world);
    var sun_vis = 1.0;
    if (has(F_SHADOWS) && (bits & G_INDOOR) == 0u && dot(n, frame.sun_dir.xyz) > -0.2) {
        sun_vis = sun_shadow(world, n, depth, px);
    }
    var lamp = LampLight(vec3<f32>(0.0), vec3<f32>(0.0), vec3<f32>(0.0));
    if (has(F_LAMPS)) {
        lamp = lamp_light(world, n, v, rough, mix(vec3<f32>(0.04), albedo, metal), bits, depth, px,
                          false);
    } else if ((bits & G_INDOOR) != 0u && has(F_PHYSICAL)) {
        lamp.diffuse = max(room_floor(vec3<f32>(0.0), vec3<f32>(0.0)), vec3<f32>(0.0));
    }
    var bounce = vec4<f32>(0.0);
    if (has(F_GI)) {
        bounce = probe_light(world, n);
    }
    return shade_lit(albedo, n, v, rough, metal, emissive, bits | G_GLOWS, sun_vis, 1.0, lamp.diffuse,
                     bounce)
        + lamp.specular + albedo * lamp.glow * (LAMP_GLOW / max(pre_exposure(), 1e-6));
}

// An additive picture's light, pre-exposed: as bright as a flame, never dimmer on screen than
// its picture (as retail adds it: a portal's swirl shows at noon), and at night no brighter than
// a few times white, so a torch does not bloom into an orb.
fn additive_light(c: vec3<f32>, emissive: f32, world: vec3<f32>) -> vec3<f32> {
    let lit = c * (1.0 + emissive) * (1.0 - fog_amount(distance(world.xy, frame.camera.xy)));
    if (has(F_PHYSICAL)) {
        return lit * clamp(ADDITIVE_UNITS * pre_exposure(), 1.0, ADDITIVE_MOST);
    }
    return min(lit * pre_exposure(), vec3<f32>(HDR_MOST));
}

// A lit colour seen through the air (outdoors), pre-exposed and held in range.
fn through_air(lit: vec3<f32>, world: vec3<f32>, dir: vec3<f32>, indoor: bool) -> vec3<f32> {
    var c = lit;
    if (!indoor) {
        if (has(F_ATMOSPHERE)) {
            c = aerial(c, world, dir);
        } else {
            c = mix(c, sky_colour(dir), fog_amount(distance(world.xy, frame.camera.xy)));
        }
    }
    return min(c * pre_exposure(), vec3<f32>(HDR_MOST));
}

// A translucent surface's colour and coverage, and whether it adds its light.
struct Seen {
    colour: vec4<f32>,
    additive: bool,
}

// A shadow's smoke: its noise's cells a metre on its model.
const SMOKE_CELLS: f32 = 2.5;
// The noise's drift up, cells a second (whole cells over the clock's ten-minute wrap).
const SMOKE_DRIFT: f32 = 0.15;
// How far the noise moves the edge, and the view's glance off the surface where it is whole.
const SMOKE_FRAY: f32 = 0.8;
const SMOKE_EDGE: vec2<f32> = vec2<f32>(0.0, 0.35);
// What is left where the edge frays away, so a plate seen edge on keeps its shape.
const SMOKE_THINNEST: f32 = 0.2;
// How much denser than its share the smoke is where it is whole.
const SMOKE_DENSE: f32 = 1.3;
// How many times their picture a shadow's eyes (its bright, saturated red) glow.
const SMOKE_EYES: f32 = 3.0;

fn fray_hash(c: vec3<i32>) -> f32 {
    var h = (bitcast<u32>(c.x) * 0x8DA6B343u) ^ (bitcast<u32>(c.y) * 0xD8163841u)
        ^ (bitcast<u32>(c.z) * 0xCB1AB31Fu);
    h = (h ^ (h >> 15u)) * 0x2C1B3C6Du;
    h = h ^ (h >> 13u);
    return f32(h & 0xFFFFu) / 65535.0;
}

// Value noise over cells, its height wrapped every `wrap` cells.
fn fray_noise(p: vec3<f32>, wrap: i32) -> f32 {
    let i = vec3<i32>(floor(p));
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let z0 = ((i.z % wrap) + wrap) % wrap;
    let z1 = (z0 + 1) % wrap;
    var c: array<f32, 8>;
    for (var k = 0; k < 8; k++) {
        let o = vec3<i32>(k & 1, (k >> 1) & 1, 0);
        c[k] = fray_hash(vec3<i32>(i.xy + o.xy, select(z0, z1, (k & 4) != 0)));
    }
    let x0 = mix(mix(c[0], c[1], u.x), mix(c[2], c[3], u.x), u.y);
    let x1 = mix(mix(c[4], c[5], u.x), mix(c[6], c[7], u.x), u.y);
    return mix(x0, x1, u.z);
}

// How much of a shadow's smoke covers a point: its share, frayed at its edges by noise on the model.
fn smoke_cover(in: VsOut, n: vec3<f32>, v: vec3<f32>) -> f32 {
    let body = 1.0 - f32(in.fade & 0xFFu) / 255.0;
    let rise = frame.shore.z * SMOKE_DRIFT;
    let wrap = i32(round(600.0 * SMOKE_DRIFT));
    let p = in.local * SMOKE_CELLS - vec3<f32>(0.0, 0.0, rise);
    let q = 0.65 * fray_noise(p, wrap) + 0.35 * fray_noise(p * 2.03 + vec3<f32>(17.0, 31.0, 0.0), wrap * 2);
    let glance = abs(dot(n, v));
    let edge = smoothstep(SMOKE_EDGE.x, SMOKE_EDGE.y, glance + (q - 0.5) * SMOKE_FRAY);
    return min(body * SMOKE_DENSE, 1.0) * mix(SMOKE_THINNEST, 1.0, edge);
}

fn translucent_seen(in: VsOut, front: bool) -> Seen {
    let dx = dpdx(in.uv);
    let dy = dpdy(in.uv);
    let m = materials[in.material];
    // A one-sided surface shows its front only, as retail culls it.
    if (web_card_hidden(m.flags) || (!front && (m.flags & M_TWO_SIDED) == 0u)) {
        discard;
    }
    var c = m.base_color;
    if ((m.flags & M_HAS_BASE) != 0u) {
        c *= pic_grad(m.tex_base, in.uv, dx, dy, in.extra);
    }
    var n = normalize(in.normal);
    if (!front) {
        n = -n;
    }
    let dir = normalize(in.world - frame.camera.xyz);
    let indoor = in.cell != 0u;
    var out: Seen;
    out.additive = (m.flags & M_ADDITIVE) != 0u;
    // Its object's own fade, seen through rather than screened (a shadow's smoke is its body).
    let shown = 1.0 - f32(in.fade & 0xFFu) / 255.0;
    if (out.additive) {
        out.colour = vec4<f32>(additive_light(c.rgb, m.emissive.x, in.world) * shown, c.a);
        return out;
    }
    var emissive = m.emissive.x;
    if ((m.flags & M_SMOKE) != 0u) {
        if ((m.flags & M_CUTOUT) != 0u && c.a < m.alpha_cutoff) {
            discard;
        }
        let eyes = smoothstep(0.12, 0.3, c.r - 2.0 * max(c.g, c.b));
        c.a = mix(smoke_cover(in, n, -dir), 1.0, eyes);
        emissive = max(emissive, SMOKE_EYES * eyes);
    } else {
        c.a *= shown;
    }
    let lit = forward_light(c.rgb, in.world, n, -dir, m.roughness, m.metal, emissive,
                            select(0u, G_INDOOR, indoor), in.clip.xy);
    out.colour = vec4<f32>(through_air(lit, in.world, dir, indoor), c.a);
    return out;
}

// Whether a pixel is screened out: by its level's crossfade only, its object's own fade being
// seen through.
fn smoke_screened(in: VsOut) -> bool {
    return screened(in.clip.xy, in.fade & ~0xFFu);
}

// A sorted translucent draw's colour, and how much it changes the pixel for the upscaler's
// reactive mask: a blended draw's coverage, or the light an added draw adds (a faint glow none,
// so the picture behind keeps its history).
struct SortedOut {
    @location(0) colour: vec4<f32>,
    @location(1) reactive: vec4<f32>,
}

// The added brightness (pre-exposed) under which an added draw marks nothing, and at which it
// marks the pixel whole.
const ADDED_REACTIVE: vec2<f32> = vec2<f32>(0.02, 0.3);

fn sorted_out(c: vec4<f32>, additive: bool) -> SortedOut {
    var o: SortedOut;
    o.colour = c;
    var r = clamp(c.a, 0.0, 1.0);
    if (additive) {
        r = smoothstep(ADDED_REACTIVE.x, ADDED_REACTIVE.y, luma(max(c.rgb, vec3<f32>(0.0))) * r);
    }
    o.reactive = vec4<f32>(r);
    return o;
}

@fragment
fn fs_translucent(in: VsOut, @builtin(front_facing) front: bool) -> SortedOut {
    if (smoke_screened(in)) {
        discard;
    }
    let s = translucent_seen(in, front);
    return sorted_out(s.colour, s.additive);
}

// A translucent surface's object, plus one, into the objects target, so a pick on it names it as
// retail's polygon test does. Written wherever the surface is drawn, whatever its picture's alpha.
@fragment
fn fs_translucent_object(in: VsOut, @builtin(front_facing) front: bool) -> @location(0) u32 {
    let m = materials[in.material];
    if (in.object == 0u || smoke_screened(in) || web_card_hidden(m.flags)
        || (!front && (m.flags & M_TWO_SIDED) == 0u)) {
        discard;
    }
    return in.object;
}

// The order-independent targets: the picture (additive light goes straight to it), the weighted
// sum of premultiplied colours, and how much of what is behind still shows.
struct OitOut {
    @location(0) hdr: vec4<f32>,
    @location(1) accum: vec4<f32>,
    @location(2) reveal: f32,
}

// A surface's weight in the sum: nearer and more opaque counts for more (the paper's eq. 7).
fn oit_weight(a: f32, world: vec3<f32>) -> f32 {
    let d = view_depth_at(world);
    let near = 10.0 / (1e-5 + pow(d / 5.0, 2.0) + pow(d / 200.0, 6.0));
    return a * clamp(near, 1e-2, 3e3);
}

fn oit_out(c: vec4<f32>, additive: bool, world: vec3<f32>) -> OitOut {
    var out: OitOut;
    if (additive) {
        out.hdr = c;
        out.accum = vec4<f32>(0.0);
        out.reveal = 0.0;
        return out;
    }
    let a = clamp(c.a, 0.0, 1.0);
    let w = oit_weight(a, world);
    out.hdr = vec4<f32>(0.0);
    out.accum = vec4<f32>(c.rgb * a, a) * w;
    out.reveal = a;
    return out;
}

@fragment
fn fs_translucent_oit(in: VsOut, @builtin(front_facing) front: bool) -> OitOut {
    if (smoke_screened(in)) {
        discard;
    }
    let s = translucent_seen(in, front);
    return oit_out(s.colour, s.additive, in.world);
}

// A faded object's own surfaces ([`crate::scene::Scene::sorted_faded`]), drawn after the air:
// their depth first, so only the nearest layer is seen, then that layer lit as the G-buffer's
// surface would be, through its own air, and laid over by its share. Its motion is kept where it
// is the more of the pixel.
fn faded_unscreened(in: VsOut) -> VsOut {
    var o = in;
    o.fade = in.fade & ~0xFFu;
    return o;
}

// The share of what is behind a faded surface covers.
fn faded_share(in: VsOut) -> f32 {
    return 1.0 - f32(in.fade & 0xFFu) / 255.0;
}

@fragment
fn fs_faded_depth(in: VsOut) {
    let m = materials[in.material];
    if (web_card_hidden(m.flags)) {
        discard;
    }
    if ((m.flags & (M_CUTOUT | M_HAS_BASE)) == (M_CUTOUT | M_HAS_BASE)) {
        let a = m.base_color.a * pic_grad(m.tex_base, in.uv, dpdx(in.uv), dpdy(in.uv), in.extra).a;
        if (a < m.alpha_cutoff) {
            discard;
        }
    }
}

// The faded surface's colour, pre-exposed, and its motion.
struct Faded {
    colour: vec3<f32>,
    motion: vec2<f32>,
}

fn faded_seen(in: VsOut, front: bool) -> Faded {
    var g = object_surface(faded_unscreened(in), front, in.extra);
    let m = materials[in.material];
    let albedo = g.albedo.rgb * (1.0 - INSIDE_SHADE * in.shade);
    let n = oct_decode(g.normal);
    var bits = u32(g.material.w * 255.0 + 0.5) & ~G_GLOWS;
    // A face's metal channel carries its skin.
    let metal = select(g.material.y, 0.0, (m.flags & M_FACE) != 0u);
    let rough = g.material.x;
    let dir = normalize(in.world - frame.camera.xyz);
    let v = -dir;
    let depth = view_depth_at(in.world);
    var sun_vis = 1.0;
    if (has(F_SHADOWS) && (bits & G_INDOOR) == 0u && dot(n, frame.sun_dir.xyz) > -0.2) {
        sun_vis = sun_shadow(in.world, n, depth, in.clip.xy);
    }
    var lamp = LampLight(vec3<f32>(0.0), vec3<f32>(0.0), vec3<f32>(0.0));
    if (has(F_LAMPS)) {
        lamp = lamp_light(in.world, n, v, rough, mix(vec3<f32>(0.04), albedo, metal), bits, depth,
                          in.clip.xy, false);
    } else if ((bits & G_INDOOR) != 0u && has(F_PHYSICAL)) {
        lamp.diffuse = max(room_floor(vec3<f32>(0.0), vec3<f32>(0.0)), vec3<f32>(0.0));
    }
    // What its relief keeps from its key light.
    if ((bits & G_KEY_LAMP) != 0u) {
        lamp.diffuse *= g.albedo.a;
        lamp.specular *= g.albedo.a;
    } else {
        sun_vis *= g.albedo.a;
    }
    var bounce = vec4<f32>(0.0);
    if (has(F_GI)) {
        bounce = probe_light(in.world, n);
    }
    let lit = shade_lit(albedo, n, v, rough, metal, g.material.z, bits, sun_vis, 1.0, lamp.diffuse,
                        bounce)
        + lamp.specular + albedo * lamp.glow * (LAMP_GLOW / max(pre_exposure(), 1e-6));
    var out: Faded;
    out.colour = through_air(lit, in.world, dir, (bits & G_INDOOR) != 0u);
    out.motion = g.motion;
    return out;
}

// How much a faded surface marks the reactive mask: most where it and what is behind share the
// pixel evenly, as only one of them can lend the pixel its motion.
fn faded_reactive(a: f32) -> f32 {
    return FADED_REACTIVE * (1.0 - abs(2.0 * a - 1.0));
}

const FADED_REACTIVE: f32 = 0.5;

struct FadedOut {
    @location(0) colour: vec4<f32>,
    @location(1) reactive: vec4<f32>,
    // The motion, kept (by its alpha) where the surface is the more of the pixel.
    @location(2) motion: vec4<f32>,
    // Its object, for the pick.
    @location(3) object: u32,
}

// A pre-exposed colour as the screen shows it, near enough: the neutral curve's shoulder, then
// the display's gamma; and back.
const FADED_KNEE: f32 = 0.76;
const FADED_GAMMA: f32 = 2.2;

fn faded_shown(c: vec3<f32>) -> vec3<f32> {
    let d = 1.0 - FADED_KNEE;
    let x = max(c, vec3<f32>(0.0));
    let rolled = 1.0 - d * d / (x + vec3<f32>(d - FADED_KNEE));
    return pow(select(x, rolled, x > vec3<f32>(FADED_KNEE)), vec3<f32>(1.0 / FADED_GAMMA));
}

fn faded_unshown(c: vec3<f32>) -> vec3<f32> {
    let d = 1.0 - FADED_KNEE;
    let y = pow(clamp(c, vec3<f32>(0.0), vec3<f32>(0.999)), vec3<f32>(FADED_GAMMA));
    return select(y, d * d / (1.0 - y) - vec3<f32>(d - FADED_KNEE), y > vec3<f32>(FADED_KNEE));
}

// Laid over the frame behind it (the copy at `ice_behind`) by its share as the screen shows them
// both, as retail blended: by light, a dark body against a bright sky would all but vanish.
@fragment
fn fs_faded(in: VsOut, @builtin(front_facing) front: bool) -> FadedOut {
    let s = faded_seen(in, front);
    let a = faded_share(in);
    let behind = textureLoad(ice_behind, vec2<i32>(in.clip.xy), 0).rgb;
    let shown = mix(faded_shown(behind), faded_shown(s.colour), a);
    var o: FadedOut;
    o.colour = vec4<f32>(min(faded_unshown(shown), vec3<f32>(HDR_MOST)), 1.0);
    o.reactive = vec4<f32>(faded_reactive(a));
    o.motion = vec4<f32>(s.motion, 0.0, select(0.0, 1.0, a >= 0.5));
    o.object = in.object;
    return o;
}

// Glacier ice (a pack material's `ice`), drawn after the water over a copy of the frame behind
// it: that frame bent by the ice's face and drunk of its red and green by the ice the view
// crosses (thin at an edge, deep blue through a body), a faint blue light scattered inside, and
// the sky mirrored at a glancing angle with the sun's glint. It writes its depth.
@group(3) @binding(13) var ice_behind: texture_2d<f32>;

// What a metre of ice takes from each channel (Beer–Lambert): red and green far more than blue.
const ICE_EXTINCTION: vec3<f32> = vec3<f32>(2.4, 1.0, 0.3);
// The most ice a view is taken to cross, metres.
const ICE_DEEPEST: f32 = 2.5;
// The colour of the light ice scatters back out, of what lights it.
const ICE_SCATTER: vec3<f32> = vec3<f32>(0.01, 0.07, 0.2);
// Ice's reflectance head on, and at a glance (its weathered faces never mirror whole).
const ICE_F0: f32 = 0.018;
const ICE_F90: f32 = 0.25;
// The night ice: its body glowing at this many times the world's light by night.
const ICE_NIGHT_GLOW: f32 = 40.0;
// That glow's colour, deep where thick and paler where thin (render-hifi's, linear).
const ICE_GLOW_DEEP: vec3<f32> = vec3<f32>(0.00004, 0.0154, 0.105);
const ICE_GLOW_PALE: vec3<f32> = vec3<f32>(0.0014, 0.237, 0.547);
// Clear ice in its own colour (a pack material's `clear`): the most crossed, metres, and the depth
// that takes its picture's hue. How clear it is and how it bends light are its material's
// (emissive y and z): the rest of its light is its own lit colour, clouded.
const TINTED_DEEPEST: f32 = 0.9;
const TINTED_HUE_DEPTH: f32 = 0.6;
// How far the colours part through a gem, of its bend past ice's.
const TINTED_FIRE: f32 = 0.07;

fn ice_unproject(px: vec2<f32>, d: f32) -> vec3<f32> {
    let ndc = vec2<f32>(px.x / frame.params.z * 2.0 - 1.0, 1.0 - px.y / frame.params.w * 2.0);
    let h = frame.inv_view_proj * vec4<f32>(ndc, d, 1.0);
    return h.xyz / h.w;
}

// How far past `dist` along the view the opaque world stands at pixel `px`; far for the sky.
fn ice_room(px: vec2<f32>, dist: f32) -> f32 {
    let q = vec2<i32>(clamp(px, vec2<f32>(0.0), frame.params.zw - 1.0));
    let d = textureLoad(g_depth, q, 0);
    if (d <= 0.0) {
        return 1.0e6;
    }
    return distance(ice_unproject(vec2<f32>(q) + 0.5, d), frame.camera.xyz) - dist;
}

@fragment
fn fs_ice(in: VsOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    let m = materials[in.material];
    // A creature of ice is already see-through: up to half its described translucency is kept
    // out of the screen door, which would only fleck it.
    var fade = in.fade;
    if ((m.flags & M_TINTED_ICE) != 0u && (fade & 0xFFu) <= 128u) {
        fade &= ~0xFFu;
    }
    if (screened(in.clip.xy, fade)) {
        discard;
    }
    let dx = dpdx(in.uv);
    let dy = dpdy(in.uv);
    var n = normalize(in.normal);
    if (!front) {
        n = -n;
    }
    if ((m.flags & M_HAS_NORMAL) != 0u) {
        let t = textureSampleGrad(textures[m.tex_normal], samp_repeat, in.uv, dx, dy).rg;
        n = bend(n, unpack_normal(t), dpdx(in.world), dpdy(in.world), dx, dy);
    }
    // The picture's light and dark vary the ice's depth a little; its cut-out is kept.
    var tone = 0.5;
    var pic_rgb = m.base_color.rgb;
    if ((m.flags & M_HAS_BASE) != 0u) {
        let pic = textureSampleGrad(textures[m.tex_base], samp_repeat, in.uv, dx, dy);
        if ((m.flags & M_CUTOUT) != 0u && pic.a < m.alpha_cutoff) {
            discard;
        }
        tone = luma(pic.rgb);
        pic_rgb = pic.rgb * m.base_color.rgb;
    }
    // Glacier blue, or the picture's own hue (its brightest channel whole).
    let tinted = (m.flags & M_TINTED_ICE) != 0u;
    var extinction = ICE_EXTINCTION;
    var scatter = ICE_SCATTER;
    var deepest = ICE_DEEPEST;
    var index = 1.31;
    var milk = 0.0;
    if (tinted) {
        index = max(m.emissive.z, 1.0);
        milk = 1.0 - saturate(m.emissive.y);
        let hue = clamp(pic_rgb / max(max(pic_rgb.r, max(pic_rgb.g, pic_rgb.b)), 1e-3), vec3<f32>(0.03), vec3<f32>(1.0));
        extinction = -log(mix(vec3<f32>(1.0), hue, 0.75)) / TINTED_HUE_DEPTH;
        scatter = hue * 0.12;
        deepest = TINTED_DEEPEST;
    }
    let px = in.clip.xy;
    let eye = frame.camera.xyz;
    let to = in.world - eye;
    let dist = length(to);
    let dir = to / max(dist, 1e-4);
    let v = -dir;
    let cos_v = saturate(dot(n, v));
    let indoor = in.cell != 0u;
    let pre = pre_exposure();

    // The ice crossed: no further than the world behind, less at a glancing edge.
    let reach = deepest * mix(0.6, 1.0, cos_v) * mix(0.75, 1.25, tone);
    let thick = clamp(ice_room(px, dist), 0.05, reach);
    // The world behind, where the bent ray leaves the ice; unbent where that is in front of it.
    // A gem parts the colours a little, red bent least and blue most (its fire).
    let fire = TINTED_FIRE * max(index - 1.31, 0.0);
    var seen = vec3<f32>(0.0);
    for (var k = 0; k < 3; k++) {
        let bent = refract(dir, n, 1.0 / (index + fire * f32(k - 1)));
        let out_at = frame.view_proj * vec4<f32>(in.world + bent * max(thick, 0.6), 1.0);
        var uv = px / frame.params.zw;
        if (out_at.w > 1e-3) {
            let s = out_at.xy / out_at.w;
            let off = clamp(vec2<f32>(s.x * 0.5 + 0.5, 0.5 - s.y * 0.5) - uv, vec2<f32>(-0.06), vec2<f32>(0.06));
            if (ice_room((uv + off) * frame.params.zw, dist) > 0.0) {
                uv += off;
            }
        }
        let got = textureSampleLevel(ice_behind, samp_clamp, uv, 0.0).rgb;
        if (fire <= 0.0) {
            seen = got;
            break;
        }
        seen[k] = got[k];
    }
    let through = exp(-extinction * thick);

    // What lights it: the sun (its shadow), the sky, the lamps.
    let depth = view_depth_at(in.world);
    var sun_vis = 0.0;
    if (!indoor) {
        sun_vis = 1.0;
        if (has(F_SHADOWS)) {
            sun_vis = sun_shadow(in.world, n, depth, px);
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
        let lamp = lamp_light(in.world, n, v, 0.05, vec3<f32>(ICE_F0), select(0u, G_INDOOR, indoor),
                              depth, px, false);
        light_in += lamp.diffuse;
        lamp_spec = lamp.specular;
    }
    var inner = scatter * light_in * (1.0 - exp(-0.9 * thick)) * pre;

    // By night the body glows of itself, fading out with the daylight.
    let night = 1.0 - smoothstep(0.0, 0.35, saturate(frame.sun_dir.w));
    let world = luma(sky + sun * max(frame.sun_dir.z, 0.0));
    let tint = mix(ICE_GLOW_PALE, ICE_GLOW_DEEP, saturate(thick / ICE_DEEPEST + 0.4 * (0.5 - tone)));
    let glow = tint * world * ICE_NIGHT_GLOW * night * pre * select(1.0, 0.0, tinted);
    var body = seen * through + inner + glow;
    if (tinted) {
        // Clouded as retail's half see-through ice is: its picture lit, its cracks and grain kept.
        // Its luminosity glows as a see-through surface's does.
        let own = pic_rgb * light_in * 0.31831 + glow_of(pic_rgb, m.emissive.x);
        body = mix(body, own * pre, milk);
    }
    // What of the world behind comes through, which came through the air already.
    let behind = seen * through * (1.0 - milk);

    // The sky it mirrors, and the sun's glint.
    var r = reflect(dir, n);
    var mirrored = sky;
    if (!indoor && frame.switches.x != 0u) {
        r.z = max(r.z, 0.15);
        if (has(F_ATMOSPHERE)) {
            mirrored = sky_radiance(r);
        } else {
            mirrored = sky_colour(r);
        }
    }
    if (has(F_GI)) {
        let pl = probe_light(in.world + n * 0.3, r);
        mirrored = mix(mirrored, pl.rgb, pl.a * 0.5);
    }
    // Its reflectance by how strongly it bends light: a diamond's faces mirror far more than ice's.
    let f0 = select(ICE_F0, pow((index - 1.0) / (index + 1.0), 2.0), tinted);
    let f90 = select(ICE_F90, max(ICE_F90, 0.6), tinted);
    let fresnel = f0 + (f90 - f0) * pow(1.0 - cos_v, 5.0);
    var c = mix(body, mirrored * pre, fresnel);
    let ndl = dot(n, frame.sun_dir.xyz);
    if (ndl > 0.0) {
        let shine = select(0.06, mix(0.06, 0.025, saturate(index - 1.31)), tinted);
        c += ggx(n, v, frame.sun_dir.xyz, shine, vec3<f32>(f0)) * ndl * sun * sun_vis * pre;
    }
    c += lamp_spec * pre;
    // The world behind came through the air already; the ice's own light is seen through it.
    if (!indoor) {
        let own = c - behind;
        var aired = own;
        if (has(F_ATMOSPHERE)) {
            aired = aerial(own / max(pre, 1e-30), in.world, dir) * pre;
        } else {
            aired = mix(own, sky_colour(dir) * pre, fog_amount(distance(in.world.xy, eye.xy)));
        }
        c = behind + aired;
    }
    if (frame.umbrae[0].w > 0.0) {
        c *= umbra(in.world);
    }
    return vec4<f32>(min(c, vec3<f32>(HDR_MOST)), 1.0);
}
