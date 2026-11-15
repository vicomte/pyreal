// What every pass shares: the frame's uniforms, the records' layouts and the lighting.

struct Frame {
    view_proj: mat4x4<f32>,
    prev_view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    // xyz: the camera; w: the tone curve (0 neutral, 1 ACES, 2 AgX).
    camera: vec4<f32>,
    // xyz: toward the sun (the moon by night); w: how much of a day it is, 0 to 1.
    sun_dir: vec4<f32>,
    // The dats' sun and ambient, in their own colour space (the retail calibration).
    sun: vec4<f32>,
    // rgb: the sky's ambient light; w: the sun's strength.
    ambient: vec4<f32>,
    // rgb: the fog; w: where it starts.
    fog: vec4<f32>,
    // x: where the fog is whole; y: the bloom's strength; zw: the target's size.
    params: vec4<f32>,
    // rgb: the light rooms take, retail; w: 1 in a room sealed from the sky.
    indoor: vec4<f32>,
    // x: 1 when the outside is seen (the sky is drawn); y: 1 when rooms cut the ground away;
    // z: the degrade scale's bits (a distance times it is in degrade metres); w: the F_ flags.
    switches: vec4<u32>,
    // The physical lights, scene units (1 = 10,000 lux): the sun's (w: the game clock, seconds
    // wrapped), the sky's (w: the sky's radiance per unit of fog colour), the rooms' (w unused).
    sun_light: vec4<f32>,
    sky_light: vec4<f32>,
    indoor_light: vec4<f32>,
    // World to view.
    view: mat4x4<f32>,
    // The sun's cascades: world to each cascade's atlas quarter (x, y in 0..1, z depth).
    cascades: array<mat4x4<f32>, 4>,
    // The view distance each cascade reaches.
    cascade_far: vec4<f32>,
    // x: an atlas texel in uv; y: the share of the shadow distance the shadows fade out over;
    // z: the share of each cascade blended into the next; w: the sun's angular size, for PCSS.
    shadow: vec4<f32>,
    // The light clusters: tiles across and down, depth slices, lights.
    cluster: vec4<u32>,
    // x: the first slice's view distance, y: the last's, z: slices per log2 of distance, w: unused.
    cluster_z: vec4<f32>,
    // Each cascade's texel and depth range, metres.
    cascade_texel: vec4<f32>,
    cascade_depth: vec4<f32>,
    // x: the visible list's length: each cascade's list follows the frame's, as long; y: the
    // clusters' tile, pixels; z: the ground's material; w: the display's headroom over SDR white
    // (f32 bits; 1 with none), which the tone curve's highlights may reach.
    lists: vec4<u32>,
    // The land field: xy where its first vertex lies, zw its vertices across and along (none: no
    // field).
    land: vec4<f32>,
    // The wind: xy its push east and north (1 a fresh breeze), z its gust now (0 to 1), w the game
    // clock last frame (wrapped as `sun_light.w`).
    wind: vec4<f32>,
    // The water's simulated square: xy its south-west corner, z its side, metres; w 1 when it runs.
    ripples: vec4<f32>,
    // x: how deep a material's relief reaches, metres; y: where the ground's close detail ends;
    // zw: render space's origin block in the world (i32 bits), which the world's patterns are laid from.
    surfaces: vec4<f32>,
    // The nearest lamps a surface's relief is shaded toward: xyz where, w how far its light
    // reaches (0: none); and each one's intensity, scene units at a metre.
    key_lamps: array<vec4<f32>, 4>,
    key_power: vec4<f32>,
    // xy: this frame's jitter, normalised device coordinates (`view_proj` holds it,
    // `prev_view_proj` none); z: how wet the rain has left the ground, w: the lightning, 0 to 1.
    jitter: vec4<f32>,
    // x: the selected object + 1 (0 none); y: the hovered object + 1; z: 1 with a target ring;
    // w: the selection's colour, packed RGBA8 (0 for the default).
    select: vec4<u32>,
    // The target ring: xyz its centre (the feet), w its radius.
    ring: vec4<f32>,
    // x: how far the surf has rolled in, a share of its period (`water::surf_phase`); y: the clock
    // within its ten-minute wrap, a share of it; z: the same, seconds; w: last frame's, seconds.
    shore: vec4<f32>,
    // The shelter map (weather.rs): xy its centre, z how far it reaches, w the height its depth
    // starts at, metres.
    shelter: vec4<f32>,
    // x: the shelter map's depth span, metres (0: not drawn); y: the ripples' clock, seconds
    // (wrapped); z: how hard it rains now, w: how full the puddles are, 0 to 1.
    rain: vec4<f32>,
    // Depth mapping's level: x the share of the relief's steps searched; y how strongly a pack
    // material's normal bends a retail picture's surface; z how far off the relief shows, metres;
    // w the terrain types the drawn cover grows on, a bit each (bits).
    relief: vec4<f32>,
    // x: the snowy terrain types (u32 bits, one a type); y: how much fresh snow covers the open
    // snowy ground, 0 to 1; z: how hard it snows now; w: the instance pool's first dynamic slot
    // (u32 bits).
    snow: vec4<f32>,
    // The trail map (snow.rs): xy its centre, z how far it reaches, w the terrain types of soft
    // snow that takes prints (u32 bits; none when it is not drawn).
    trail: vec4<f32>,
    // The shadows' pools of darkness: xyz each one's feet, w its radius (0: no more).
    umbrae: array<vec4<f32>, 8>,
    // The waypoint's ring: xyz its middle (the player's feet), w its radius (0: none).
    waypoint: vec4<f32>,
    // xy: the way its arrow points, east and north; z: the object it rings + 1 (u32 bits).
    waypoint_way: vec4<f32>,
}

const F_PHYSICAL: u32 = 1u;
const F_SHADOWS: u32 = 2u;
const F_LAMPS: u32 = 4u;
const F_AO: u32 = 8u;
const F_GI: u32 = 16u;
const F_ATMOSPHERE: u32 = 32u;
const F_LAMP_RAYS: u32 = 64u;
const F_WATER: u32 = 128u;
const F_COVER: u32 = 256u;
const F_RELIEF: u32 = 512u;
const F_HEIGHT_BLEND: u32 = 1024u;
const F_TRIPLANAR: u32 = 2048u;
const F_DETAIL: u32 = 4096u;
const F_MACRO: u32 = 8192u;
const F_WEBS: u32 = 16384u;

// A surface's motion since last frame in texture coordinates, this frame's place less the last's,
// without the jitter.
fn motion_of(cur: vec4<f32>, prev: vec4<f32>) -> vec2<f32> {
    return (cur.xy / cur.w - frame.jitter.xy - prev.xy / prev.w) * vec2<f32>(0.5, -0.5);
}

fn has(flag: u32) -> bool {
    return (frame.switches.w & flag) != 0u;
}

// How deep a shadow's pool of darkness is at its middle, and how high over its feet it reaches.
const UMBRA_DEPTH: f32 = 0.7;
const UMBRA_HEIGHT: vec2<f32> = vec2<f32>(0.25, 1.0);

// How much of its light a point keeps in the shadows' pools of darkness round their feet.
fn umbra(p: vec3<f32>) -> f32 {
    var kept = 1.0;
    for (var k = 0u; k < 8u; k++) {
        let u = frame.umbrae[k];
        if (u.w <= 0.0) {
            break;
        }
        let r = length(p.xy - u.xy) / u.w;
        let h = abs(p.z - u.z);
        let pool = (1.0 - smoothstep(0.25, 1.0, r)) * (1.0 - smoothstep(UMBRA_HEIGHT.x, UMBRA_HEIGHT.y, h));
        kept *= 1.0 - UMBRA_DEPTH * pool;
    }
    return kept;
}

fn web_card_hidden(flags: u32) -> bool {
    return (flags & M_WEB_CARD) != 0u && has(F_WEBS);
}

// A row-major 3x4 transform, this frame's and last frame's.
struct Instance {
    m0: vec4<f32>,
    m1: vec4<f32>,
    m2: vec4<f32>,
    p0: vec4<f32>,
    p1: vec4<f32>,
    p2: vec4<f32>,
    mesh: u32,
    object: u32,
    // The cell slot it is seen through; zero outdoors.
    cell: u32,
    // How the wind sways it: half floats of how much, and its height; zero for not at all.
    // SKINNED for a posed body.
    sway: u32,
}

// An instance's sway word for a posed body (scene.rs `SKINNED`).
const SKINNED: u32 = 0x80000000u;

struct Material {
    base_color: vec4<f32>,
    emissive: vec4<f32>,
    tex_base: u32,
    tex_normal: u32,
    tex_height: u32,
    tex_rough_metal: u32,
    tex_occlusion: u32,
    tex_emissive: u32,
    flags: u32,
    kind: u32,
    roughness: f32,
    metal: f32,
    alpha_cutoff: f32,
    // An impostor's views: how many (low byte) and how many to a row of its atlas (next byte).
    views: u32,
}

// A cell's ground: up to six layers of (picture, alpha map, rotation | kind << 8 | terrain type << 16, tiling bits).
// Then the range of the cut lists naming the room volumes over it.
struct TerrainCell {
    count: u32,
    flags: u32,
    cut_first: u32,
    cut_count: u32,
    layers: array<vec4<u32>, 6>,
}

const M_CUTOUT: u32 = 1u;
const M_TRANSLUCENT: u32 = 2u;
const M_TWO_SIDED: u32 = 4u;
const M_WATER: u32 = 8u;
const M_FOLIAGE: u32 = 16u;
const M_ADDITIVE: u32 = 32u;
const M_LUMINOUS: u32 = 64u;
const M_TERRAIN: u32 = 128u;
const M_HAS_BASE: u32 = 256u;
const M_HAS_NORMAL: u32 = 512u;
const M_HAS_HEIGHT: u32 = 1024u;
const M_HAS_ROUGH_METAL: u32 = 2048u;
const M_HAS_OCCLUSION: u32 = 4096u;
const M_HAS_EMISSIVE: u32 = 8192u;
// The roughness, occlusion and height maps are one picture: red, green and blue.
const M_PACKED_SURFACE: u32 = 16384u;
const M_IMPOSTOR: u32 = 32768u;
// The degrade mode of an impostor: turned about its upright axis to face the eye.
const IMPOSTOR_MODE: u32 = 6u;
// The surface blends by its alpha; an additive one without it adds its whole picture.
const M_ALPHA: u32 = 65536u;
// A cobweb card, left out while its silk is drawn (webs.rs).
const M_WEB_CARD: u32 = 131072u;
// Glacier ice, a pack material's (`flags::ICE`).
const M_ICE: u32 = 262144u;
// A shadow's see-through body, drawn as smoke (`flags::SMOKE`).
const M_SMOKE: u32 = 524288u;
// Ice in its picture's own colour (`flags::TINTED_ICE`): a creature's body of ice.
const M_TINTED_ICE: u32 = 1048576u;
const NO_TEXTURE: u32 = 0xFFFFFFFFu;

// The G-buffer's shading bits, in the material target's alpha.
const G_WATER: u32 = 1u;
const G_FOLIAGE: u32 = 2u;
const G_TERRAIN: u32 = 4u;
const G_INDOOR: u32 = 8u;
// The albedo target's alpha (how much of the key light a surface's relief lets reach the point)
// shades the lamps, not the sun.
const G_KEY_LAMP: u32 = 16u;
// Set by the lighting, never stored: the surface gives back nothing as a mirror (under water).
const G_NO_MIRROR: u32 = 32u;
// Set by the forward pass: a see-through or additive surface's luminosity glows. Stored, molten
// rock: its emissive is its glow's strength (`molten_light`).
const G_GLOWS: u32 = 64u;
// Something that moves (a body): no snow settles on it.
const G_MOVES: u32 = 128u;
// Set by the volumes, never stored: a point of a lit volume takes a lamp's light from every side.
const G_VOLUME: u32 = 256u;
// Set by the volumes, never stored: as `G_VOLUME`, but taking every lamp, magic light and its own
// creature's too (a Dark Vapor's own glow lights its smoke).
const G_VOLUME_LIT: u32 = 512u;

// Roughness per surface kind, in `SurfaceKind::ALL`'s order.
const KIND_ROUGHNESS = array<f32, 10>(0.8, 0.75, 0.7, 0.35, 0.95, 0.95, 0.7, 0.06, 0.55, 0.8);
// `SurfaceKind::Molten`: its hot texels glow.
const KIND_MOLTEN: u32 = 9u;

@group(0) @binding(0) var<uniform> frame: Frame;
// [0] the exposure the HDR target is drawn at this frame (last frame's adapted exposure).
@group(0) @binding(1) var<storage, read> exposure_in: array<f32, 4>;

// The most any pass writes into the half-float HDR target.
const HDR_MOST: f32 = 60000.0;

fn pre_exposure() -> f32 {
    return exposure_in[0];
}

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// How hot a texel of molten rock looks from its linear colour: x, how much of it is molten (its
// red past its green and blue, which a far mip's mix of crack and rock keeps in proportion); y,
// how hot that is, its brightest yellowest heart 1.
fn molten_heat(c: vec3<f32>) -> vec2<f32> {
    let red = c.r - max(c.g, c.b);
    if (red <= 0.0) {
        return vec2<f32>(0.0);
    }
    let yellow = (c.g - c.b) / max(c.r - c.b, 1e-4);
    return vec2<f32>(min(red / 0.3, 1.0), max(smoothstep(0.2, 0.7, red), smoothstep(0.12, 0.35, yellow)));
}

// Molten rock's glow at `p` (metres) for `heat` (`molten_heat`): rgb deep red where it is
// coolest to orange-yellow at its heart; w its strength, dimmer where cooler, breathing slowly
// (the colour mixed in by how much is molten carries that share).
fn molten_glow(heat: vec2<f32>, p: vec2<f32>) -> vec4<f32> {
    let t = frame.sun_light.w;
    let phase = sin(p.x * 0.21 + p.y * 0.13) * 2.0 + sin(p.y * 0.37 - p.x * 0.29);
    // Whole turns in the clock's day.
    let breathe = 0.82 + 0.1 * sin(6.2831853 * t / 7.5 + phase)
        + 0.08 * sin(6.2831853 * t / 20.0 + phase * 1.7);
    let warm = mix(vec3<f32>(0.4, 0.02, 0.0), vec3<f32>(1.0, 0.2, 0.015), smoothstep(0.0, 0.55, heat.y));
    let colour = mix(warm, vec3<f32>(1.0, 0.58, 0.14), smoothstep(0.55, 1.0, heat.y));
    return vec4<f32>(colour, smoothstep(0.0, 0.15, heat.x) * mix(0.3, 1.0, heat.y) * breathe);
}

// What molten rock gives off per unit of its glow's strength, scene units, and the least and the
// most that shows on screen in multiples of white: past white by day and by night, for the bloom.
const MOLTEN_UNITS: f32 = 0.6;
const MOLTEN_SCREEN: vec2<f32> = vec2<f32>(1.8, 2.2);
// The share of its glow's colour molten rock takes the light by.
const MOLTEN_CRUST: f32 = 0.25;

// Molten rock's light, scene units: `strength` of its glow `colour`.
fn molten_light(colour: vec3<f32>, strength: f32) -> vec3<f32> {
    let e = max(pre_exposure(), 1e-6);
    return colour * strength * clamp(MOLTEN_UNITS * e, MOLTEN_SCREEN.x, MOLTEN_SCREEN.y) / e;
}

fn xform(r0: vec4<f32>, r1: vec4<f32>, r2: vec4<f32>, p: vec3<f32>) -> vec3<f32> {
    let h = vec4<f32>(p, 1.0);
    return vec3<f32>(dot(r0, h), dot(r1, h), dot(r2, h));
}

fn xform_dir(r0: vec4<f32>, r1: vec4<f32>, r2: vec4<f32>, d: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(dot(r0.xyz, d), dot(r1.xyz, d), dot(r2.xyz, d));
}

fn oct_encode(n: vec3<f32>) -> vec2<f32> {
    let p = n.xy / (abs(n.x) + abs(n.y) + abs(n.z));
    if (n.z >= 0.0) {
        return p;
    }
    let s = select(vec2<f32>(-1.0), vec2<f32>(1.0), p >= vec2<f32>(0.0));
    return (1.0 - abs(p.yx)) * s;
}

fn oct_decode(e: vec2<f32>) -> vec3<f32> {
    var n = vec3<f32>(e, 1.0 - abs(e.x) - abs(e.y));
    if (n.z < 0.0) {
        let s = select(vec2<f32>(-1.0), vec2<f32>(1.0), n.xy >= vec2<f32>(0.0));
        n = vec3<f32>((1.0 - abs(n.yx)) * s, n.z);
    }
    return normalize(n);
}

// The sky seen along `dir`: the fog's colour at the horizon, deepening overhead, with the sun.
// In scene units under the physical calibration.
fn sky_colour(dir: vec3<f32>) -> vec3<f32> {
    let up = clamp(dir.z, -0.2, 1.0);
    let horizon = frame.fog.rgb;
    let zenith = horizon * vec3<f32>(0.45, 0.62, 1.0) * 0.9;
    var c = mix(horizon, zenith, smoothstep(0.0, 0.6, up));
    c = mix(c, horizon * 0.7, smoothstep(0.0, -0.2, up));
    let s = max(dot(dir, frame.sun_dir.xyz), 0.0);
    c += frame.sun.rgb * (pow(s, 3000.0) * 40.0 + pow(s, 16.0) * 0.15);
    if (has(F_PHYSICAL)) {
        c *= frame.sky_light.w;
    }
    return c;
}

// How much fog lies over a point `dist` metres away across the ground: the world fog, closed
// before the edge of the loaded area.
fn fog_amount(dist: f32) -> f32 {
    return smoothstep(frame.fog.w, frame.params.x, dist);
}

fn ggx(n: vec3<f32>, v: vec3<f32>, l: vec3<f32>, rough: f32, f0: vec3<f32>) -> vec3<f32> {
    let h = normalize(v + l);
    let a = max(rough * rough, 0.002);
    let a2 = a * a;
    let nh = max(dot(n, h), 0.0);
    let nv = max(dot(n, v), 1e-4);
    let nl = max(dot(n, l), 1e-4);
    let d = a2 / (3.14159265 * pow(nh * nh * (a2 - 1.0) + 1.0, 2.0));
    let k = a * 0.5;
    let g = (nv / (nv * (1.0 - k) + k)) * (nl / (nl * (1.0 - k) + k));
    let f = f0 + (1.0 - f0) * pow(1.0 - max(dot(h, v), 0.0), 5.0);
    return d * g * f / (4.0 * nv * nl);
}

// How much of what surrounds it a surface of `rough` and `f0` gives back as a mirror does, seen
// at `nv` (Karis's fit to the split-sum lookup).
fn env_brdf(f0: vec3<f32>, rough: f32, nv: f32) -> vec3<f32> {
    let r = rough * vec4<f32>(-1.0, -0.0275, -0.572, 0.022) + vec4<f32>(1.0, 0.0425, 1.04, -0.04);
    let a = min(r.x * r.x, exp2(-9.28 * nv)) * r.x + r.y;
    let ab = vec2<f32>(-1.04, 1.04) * a + r.zw;
    return f0 * ab.x + ab.y;
}

// The wind's gusts: patches of stronger wind blown along the breeze, 16 m by 22 m with eddies
// 5 m by 7 m within, at 4 m/s; both repeat in whole cells over a ten-minute lap of the clock.
const GUST_SPEED: f32 = 4.0;
const GUST_CELLS: vec4<f32> = vec4<f32>(16.0, 22.0, 5.0, 7.0);
const GUST_LAP: f32 = 600.0;

fn gust_hash(x: u32, y: u32) -> f32 {
    let h = ((x * 0x8DA6B343u) ^ (y * 0xD8163841u) ^ 0x68E31DA4u) * 0x2C1B3C6Du;
    return f32((h ^ (h >> 15u)) >> 8u) / 16777216.0;
}

// Value noise at `p` cells, 0 to 1, repeating every `wrap` cells along x.
fn gust_noise(p: vec2<f32>, wrap: f32) -> f32 {
    let i = floor(p);
    let f = p - i;
    let u = f * f * (3.0 - 2.0 * f);
    let x0 = i.x - floor(i.x / wrap) * wrap;
    let a = u32(x0);
    let b = select(a + 1u, 0u, x0 + 1.0 >= wrap);
    let y = bitcast<u32>(i32(i.y));
    return mix(mix(gust_hash(a, y), gust_hash(b, y), u.x),
               mix(gust_hash(a, y + 1u), gust_hash(b, y + 1u), u.x), u.y);
}

// The gusts and the eddies at render-space `foot` blown along `along`, at clock `t` seconds,
// 0 to 1. Laid from a 2 km square of the world so a turning breeze barely moves them.
fn wind_gust(foot: vec2<f32>, along: vec2<f32>, t: f32) -> vec2<f32> {
    let w = foot + vec2<f32>(bitcast<vec2<i32>>(frame.surfaces.zw)) * 192.0;
    let p = w - floor(w / 2048.0) * 2048.0;
    let x = dot(p, along) - (t - floor(t / GUST_LAP) * GUST_LAP) * GUST_SPEED;
    let y = dot(p, vec2<f32>(-along.y, along.x));
    let run = GUST_LAP * GUST_SPEED;
    return vec2<f32>(
        gust_noise(vec2<f32>(x, y) / GUST_CELLS.xy, run / GUST_CELLS.x),
        gust_noise(vec2<f32>(x, y) / GUST_CELLS.zw + 37.0, run / GUST_CELLS.z),
    );
}

// How far the wind moves a point `rise` (0 at the foot, 1 at the top) up a plant or tree `tall`
// metres high whose foot stands at `foot`, at clock `t`: a lean with the wind deepened by the
// passing gusts, a small sway at its own rate and a flutter, the taller the less in proportion.
fn sway_at(foot: vec3<f32>, local: vec3<f32>, rise: f32, tall: f32, t: f32) -> vec3<f32> {
    let r = clamp(rise, 0.0, 1.0);
    let reach = min(tall * mix(0.16, 0.028, smoothstep(1.5, 12.0, tall)), 0.85) * r * r;
    let strength = length(frame.wind.xy);
    if (strength == 0.0) {
        return vec3<f32>(0.0);
    }
    let along = select(vec2<f32>(0.82, 0.57), frame.wind.xy / max(strength, 1e-4), strength > 1e-4);
    let side = vec2<f32>(-along.y, along.x);
    let n = wind_gust(foot.xy, along, t);
    let gust = smoothstep(0.25, 0.8, 0.5 * (n.x + n.y)) * mix(0.7, 1.2, frame.wind.z);
    let phase = dot(foot.xy, vec2<f32>(0.13, 0.09));
    let rate = 1.0 + 0.3 * sin(phase * 3.7);
    let bend = (sin(t * 1.1 * rate + phase) + 0.45 * sin(t * 2.3 * rate + phase * 1.6)) * (0.06 + 0.1 * gust);
    let flutter = 0.1 * sin(t * 6.1 * rate + dot(local, vec3<f32>(2.1, 1.7, 2.9)));
    let across = 0.12 * sin(t * 0.7 * rate + phase * 0.7) + 0.4 * (n.y - 0.5);
    let k = reach * strength;
    let xy = along * (0.22 + 0.24 * gust + bend + 0.4 * flutter) * k
        + side * (across - 0.3 * flutter) * k;
    // The length kept: what leans over drops a little.
    let held = min(length(xy), 0.9 * max(tall, 0.01));
    let drop = tall - sqrt(max(tall * tall - held * held, 0.0));
    return vec3<f32>(xy, 0.3 * flutter * k - drop * r);
}

// What a luminous surface gives off per unit of its emissive level, scene units: about a lit
// lamp glass's brightness, dim beside a sunlit wall and bright at night.
const EMISSIVE_UNITS: f32 = 0.3;
// The least and the most a full glow shows on screen, in multiples of white: never dimmer than
// its picture (retail draws it unlit), past white so the bloom takes it.
const GLOW_SCREEN: vec2<f32> = vec2<f32>(1.6, 12.0);

// A luminous surface's light, scene units: `emissive` of its colour.
fn glow_of(albedo: vec3<f32>, emissive: f32) -> vec3<f32> {
    let e = max(pre_exposure(), 1e-6);
    return albedo * emissive * clamp(EMISSIVE_UNITS * e, GLOW_SCREEN.x, GLOW_SCREEN.y) / e;
}

// The share of the sun a leaf lit from behind passes on (physical light).
const FOLIAGE_THROUGH: f32 = 0.25;

// Sun and sky light on one surface point, with how much of the sun reaches it (`sun_vis`), how
// much of the sky (`sky_vis`, the ambient occlusion), the lamps' light (`extra`, scene units) and
// the probes' bounce and sky light (`bounce`: rgb, and how far it is trusted in a).
fn shade_lit(albedo: vec3<f32>, n: vec3<f32>, v: vec3<f32>, rough: f32, metal: f32,
             emissive: f32, bits: u32, sun_vis: f32, sky_vis: f32, extra: vec3<f32>,
             bounce: vec4<f32>) -> vec3<f32> {
    if (!has(F_PHYSICAL)) {
        return shade_retail(albedo, n, v, rough, metal, emissive, bits, sun_vis, sky_vis, extra);
    }
    let diffuse = albedo * (1.0 - metal * 0.7);
    // Only a see-through glow shines past white; an opaque luminous surface is self-lit, as retail
    // draws it: never dimmer than its picture at its luminosity.
    let glows = (bits & G_GLOWS) != 0u;
    let glow = select(vec3<f32>(0.0), glow_of(albedo, emissive), glows);
    let self_lit = vec3<f32>(select(emissive / max(pre_exposure(), 1e-6), 0.0, glows));
    let f0 = mix(vec3<f32>(0.04), albedo, metal);
    // What surrounds a surface comes back off it as off a mirror, by its roughness: metal reads
    // as metal, not as a dark stone.
    var mirror = env_brdf(f0, rough, max(dot(n, v), 0.0)) * sky_vis;
    if ((bits & G_NO_MIRROR) != 0u) {
        mirror = vec3<f32>(0.0);
    }
    if ((bits & G_INDOOR) != 0u) {
        // Rooms take the world ambient (half of it where the probes bring their lamps' bounce)
        // and their lamps, never the sun.
        let fill = frame.indoor_light.rgb;
        let ambient = mix(fill, fill * 0.5 + bounce.rgb, bounce.a);
        return diffuse * max(ambient * sky_vis + extra, self_lit) + mirror * ambient + glow;
    }
    let l = frame.sun_dir.xyz;
    let ndl = dot(n, l);
    var wrap = max(ndl, 0.0);
    var hemi = mix(0.55, 1.0, n.z * 0.5 + 0.5);
    if ((bits & G_FOLIAGE) != 0u) {
        // A leaf lit from behind passes only part of the sun through; a crown shades its own sky.
        wrap += FOLIAGE_THROUGH * max(-ndl, 0.0);
        hemi = 0.8;
    }
    let sun = frame.sun_light.rgb * sun_vis;
    var spec = vec3<f32>(0.0);
    // Under the water, the water's own glint is the sun's.
    if (ndl > 0.0 && (bits & G_NO_MIRROR) == 0u) {
        spec = ggx(n, v, l, rough, f0) * ndl * sun;
    }
    let ambient = mix(frame.sky_light.rgb * hemi, bounce.rgb, bounce.a);
    // The sky overhead, the ground's darker light below the horizon.
    let r = reflect(-v, n);
    let around = mix(frame.sky_light.rgb * mix(0.35, 1.0, smoothstep(-0.3, 0.5, r.z)), bounce.rgb,
                     bounce.a);
    return diffuse * max(sun * wrap + ambient * sky_vis + extra, self_lit) + spec + mirror * around
        + glow;
}

// The retail calibration: the sun and the sky's light summed in the dats' own (gamma) colour
// space and clamped at one per channel, as the client's vertex light is, then made linear;
// specular light is linear and unclamped. Lamps (`extra`) are summed with them.
fn shade_retail(albedo: vec3<f32>, n: vec3<f32>, v: vec3<f32>, rough: f32, metal: f32,
                emissive: f32, bits: u32, sun_vis: f32, sky_vis: f32,
                extra: vec3<f32>) -> vec3<f32> {
    if ((bits & G_INDOOR) != 0u) {
        let light = pow(min(frame.indoor.rgb * sky_vis + extra, vec3<f32>(1.0)), vec3<f32>(2.2));
        return albedo * (1.0 - metal * 0.7) * light + albedo * emissive;
    }
    let l = frame.sun_dir.xyz;
    let ndl = dot(n, l);
    var wrap = max(ndl, 0.0);
    var hemi = mix(0.55, 1.0, n.z * 0.5 + 0.5);
    if ((bits & G_FOLIAGE) != 0u) {
        wrap = mix(wrap, abs(ndl), 0.7);
        hemi = 1.0;
    }
    var ambient = frame.ambient.rgb;
    if ((bits & G_TERRAIN) == 0u) {
        ambient += frame.ambient.w * 0.2;
    }
    let lit = frame.sun.rgb * wrap * sun_vis + ambient * hemi * sky_vis + extra;
    let light = pow(min(lit, vec3<f32>(1.0)), vec3<f32>(2.2));
    let f0 = mix(vec3<f32>(0.04), albedo, metal);
    var spec = vec3<f32>(0.0);
    if (ndl > 0.0) {
        spec = ggx(n, v, l, rough, f0) * ndl * pow(frame.sun.rgb, vec3<f32>(2.2)) * sun_vis;
    }
    return albedo * (1.0 - metal * 0.7) * light + spec + albedo * emissive;
}

// Sun and sky light on one surface point with nothing in the way.
fn shade(albedo: vec3<f32>, n: vec3<f32>, v: vec3<f32>, rough: f32, metal: f32, emissive: f32,
         bits: u32) -> vec3<f32> {
    return shade_lit(albedo, n, v, rough, metal, emissive, bits, 1.0, 1.0, vec3<f32>(0.0),
                     vec4<f32>(0.0));
}
