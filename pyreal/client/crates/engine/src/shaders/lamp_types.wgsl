// One of the frame's lamps (`lamps::lamp_bytes`), shared by the cluster build and the lighting.

struct Lamp {
    position: vec3<f32>,
    range: f32,
    // Colour times luminous intensity, scene units at a metre.
    colour: vec3<f32>,
    // Bits 0-7 the flicker (0 steady, 1 flame, 2 embers); bit 8 indoors; bit 9 shadowed; bit 10
    // the dats' own (its light held within a metre); bit 11 a flame found without a light; bit 12
    // magic, the dats' light of a colour no lamp burns (a portal's, a spell's); bit 13 an acid
    // body's glow; bit 14 a strike of lightning; bit 15 a portal's own light (`gates::lamp`); bit
    // 16 a held thing's light (`glow::HELD_SHARE`).
    flags: u32,
    // The client's light: colour times intensity.
    retail: vec3<f32>,
    // Bits 0-7 its ray-traced shadow's slot (255 none); above, a seed.
    slot: u32,
    // The object it belongs to (its fixture never shadows it), low 24 bits.
    object: u32,
    pad0: u32,
    pad1: u32,
    pad2: u32,
}

// The most lamps a cluster holds.
const CLUSTER_MOST: u32 = 48u;

// A room's whole light (retail's one), scene units.
fn room_whole() -> vec3<f32> {
    return frame.indoor_light.rgb / max(pow(min(frame.indoor.rgb, vec3<f32>(1.0)), vec3<f32>(2.2)), vec3<f32>(1e-6));
}

// A lamp's light in a room at `d2` metres squared, before the face it falls on: an inverse square
// windowed to its reach, held no nearer than half a metre (a flame is not a point; a retail lamp a
// metre, as its own fall-off holds), and no more than its whole light in its own colour, as retail
// clamps each light.
fn room_lamp(l: Lamp, d2: f32) -> vec3<f32> {
    let window = clamp(1.0 - d2 / (l.range * l.range), 0.0, 1.0);
    let least = select(0.25, 1.0, (l.flags & 1024u) != 0u);
    let top = max(max(l.colour.r, max(l.colour.g, l.colour.b)), 1e-8);
    return min(l.colour * (window * window / (max(d2, least) + least)), l.colour / top * room_whole());
}

// Outdoor lamps as the old renderer's Better Lighting lit them (render-hifi's lamps): an authored
// lamp's colour pulled most of the way to lamplight, a found flame's to firelight, embers' own; a windowed inverse square softened inside a core, at a gain over the night's ambient.
const LAMPLIGHT: vec3<f32> = vec3<f32>(1.0, 0.56, 0.26);
const FIRE: vec3<f32> = vec3<f32>(1.0, 0.52, 0.18);
const EMBER: vec3<f32> = vec3<f32>(1.0, 0.2, 0.04);
const OUTDOOR_GAIN: f32 = 12.0;
// How bright an acid body's glow is outdoors, of a full lamp's.
const ACID_LAMP: f32 = 0.45;
// How bright a portal's own light is outdoors.
const PORTAL_LAMP: f32 = 0.5;
// A held thing's light's share of its kind's outdoors (`glow::HELD_SHARE`).
const HELD_SHARE: f32 = 0.25;
const OUTDOOR_CORE: f32 = 0.6;
// A strike's lamp's intensity (the client's units) that lights as a bright lamp does.
const LIGHTNING_UNITS: f32 = 40.0;
// How many times a bright lamp a full strike lights with.
const LIGHTNING_GAIN: f32 = 4.0;

// `c` pulled `k` toward `to`, its brightest channel one.
fn toward(c: vec3<f32>, to: vec3<f32>, k: f32) -> vec3<f32> {
    let m = mix(c, to, k);
    return m / max(max(m.r, max(m.g, m.b)), 1e-4);
}

// The most intensity a magic light lights with (`lamps::MAGIC_INTENSITY`).
const MAGIC_INTENSITY: f32 = 4.0;

// Whether a lamp is magic (`lamps::warm`): a portal's or a spell's light.
fn magic(l: Lamp) -> bool {
    return (l.flags & 4096u) != 0u;
}

// An outdoor lamp's colour, its brightest channel one.
fn outdoor_colour(l: Lamp) -> vec3<f32> {
    let top = max(max(l.retail.r, max(l.retail.g, l.retail.b)), 1e-4);
    if ((l.flags & 16384u) != 0u) {
        return l.retail.rgb / top;
    }
    if ((l.flags & 2048u) != 0u) {
        // A blue fire's (a creature of blue fire) keeps its own colour.
        if (l.retail.b > l.retail.r) {
            return l.retail.rgb / top;
        }
        return toward(l.retail.rgb / top, FIRE, 0.6);
    } else if ((l.flags & 255u) == 2u) {
        return EMBER;
    } else if (magic(l)) {
        // Its own colour as paled, not lamplight's.
        return l.colour / max(max(l.colour.r, max(l.colour.g, l.colour.b)), 1e-8);
    }
    return toward(l.retail.rgb / top, LAMPLIGHT, 0.75);
}

// An outdoor lamp's light `d` metres off, before its gain, flicker and the face it falls on.
fn outdoor_lamp(l: Lamp, d: f32) -> vec3<f32> {
    let top = max(max(l.retail.r, max(l.retail.g, l.retail.b)), 1e-4);
    let falloff = l.range / 1.5;
    let colour = outdoor_colour(l);
    var bright = clamp(top / 100.0, 0.3, 1.0);
    var reach = clamp(falloff * 2.0, 5.0, 16.0);
    if ((l.flags & 2048u) != 0u) {
        bright = 0.9;
        reach = clamp(falloff * 1.5, 4.0, 10.0);
    } else if ((l.flags & 255u) == 2u) {
        bright = 0.18;
        reach = 4.0;
    } else if ((l.flags & 8192u) != 0u) {
        // An acid body's glow washes the ground round it.
        bright = ACID_LAMP;
        reach = clamp(falloff, 4.0, 8.0);
    } else if ((l.flags & 32768u) != 0u) {
        // A portal washes the ground round it in its colour.
        bright = PORTAL_LAMP;
        reach = clamp(falloff, 4.0, 10.0);
    } else if (magic(l)) {
        // A soft glow round it, as neither retail nor the old renderer lit the ground with one.
        bright = 0.25;
        reach = clamp(falloff, 3.0, 6.0);
    }
    if ((l.flags & 16384u) != 0u) {
        // A strike of lightning: as bright as its flash, for a moment.
        bright = top / LIGHTNING_UNITS * LIGHTNING_GAIN;
        reach = clamp(falloff, 4.0, 16.0);
    }
    if ((l.flags & 65536u) != 0u) {
        bright *= HELD_SHARE;
    }
    reach = min(reach, l.range);
    let x = d / reach;
    let window = clamp(1.0 - x * x * x * x, 0.0, 1.0);
    return colour * bright * (window * window / (d * d + OUTDOOR_CORE * OUTDOOR_CORE));
}

// The outdoor lamps' gain: the old renderer's units (the dats' linear ambient) at the sky's scale.
fn outdoor_lamp_scale() -> f32 {
    let lin_base = pow(min(frame.ambient.rgb, vec3<f32>(1.0)), vec3<f32>(2.2));
    return OUTDOOR_GAIN * luma(frame.sky_light.rgb) / max(luma(lin_base), 1e-6);
}
