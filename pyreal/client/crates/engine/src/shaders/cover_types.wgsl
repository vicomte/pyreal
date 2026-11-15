// The ground cover's records (cover.rs), for the pass that places it and the one that draws it.

struct CoverKind {
    // The terrain types it grows on, one bit each; its first counter (kind times levels).
    grounds: u32,
    count_first: u32,
    // Its pack id, which seeds its placement; its near level's mean brightness as drawn (linear).
    seed: u32,
    albedo: f32,
    // Metres between plants on its grid; the least and most scale; the cosine of the steepest
    // ground it grows on.
    step: f32,
    scale_lo: f32,
    scale_hi: f32,
    steepest: f32,
    // Its near level's height and bounding radius, metres; how much the wind sways it; how far
    // from the eye it is drawn at all, metres, at the settings' reach.
    height: f32,
    radius: f32,
    wind: f32,
    reach: f32,
    // Where each level ends, metres a metre of scale, at the settings' reach and detail: three of
    // shape, then its card.
    ends: vec4<f32>,
    // Where each level's instances start in the list, and how many it holds.
    bases: vec4<u32>,
    caps: vec4<u32>,
    // x: its grid's tiles across; y: its first tile of the placement's dispatch; zw unused.
    tiles: vec4<u32>,
}

// One plant: where its foot stands and its scale; its turn; its kind and level; its crossfade
// code (0 none; under 0x80 this finer level's share faded, from it the coarser's shown).
struct CoverPlant {
    at: vec4<f32>,
    turn: f32,
    kind: u32,
    level: u32,
    fade: u32,
}

// A clump of the grass under the cover (blades.wgsl). A flower (scatter.wgsl) keeps its petals'
// colour, its head's radius in `wide` and its head's shape in `look`; a stone keeps its width in
// `at.w`, its colour, and the ground's slope under it in `look` (eight bits a way, 127 level).
struct Clump {
    // xyz: its foot; w: how tall, metres.
    at: vec4<f32>,
    seed: u32,
    // The ground's colour under it (linear), ten bits a channel over `BLADE_RANGE`.
    colour: u32,
    // How much wider its blades stand for the clumps thinned about it.
    wide: f32,
    // How dry its tips are (low byte, of 255); bit 8 for a far clump.
    look: u32,
}

// The scale of a clump's colour word.
const BLADE_RANGE: f32 = 2.0;

// The cover's wind (cover.rs): a steady lean with the breeze, deepened by the gusts blown across
// the field (`wind_gust`); each plant takes them a little late and bends as far as its own
// stiffness lets, flutters at its own two rates and leans its own way. Its rates are whole turns
// in the clock's ten-minute wrap (`frame.shore.zw`).
const COVER_FLUTTER_TURNS: vec2<f32> = vec2<f32>(420.0, 900.0);
const COVER_LAP: f32 = 600.0;
const COVER_TAU: f32 = 6.2831853;

fn cover_mix(x: u32) -> u32 {
    var h = x * 0x9E3779B9u;
    h = (h ^ (h >> 16u)) * 0x7FEB352Du;
    h = (h ^ (h >> 15u)) * 0x846CA68Bu;
    return h ^ (h >> 16u);
}

fn cover_unit(h: u32) -> f32 {
    return f32(h >> 8u) / 16777216.0;
}

// Where render-space `p` lies in the world, metres, for patterns that keep their place as render
// space moves.
fn cover_world(p: vec2<f32>) -> vec2<f32> {
    return p + vec2<f32>(origin_block()) * 192.0;
}

// How far the wind blows the tip of a stem `tall` metres high standing at render-space `foot`,
// seeded `seed`, at `t` seconds into the clock's wrap.
fn cover_tip(foot: vec2<f32>, tall: f32, seed: u32, t: f32) -> vec2<f32> {
    let strength = length(frame.wind.xy);
    let along = select(vec2<f32>(0.82, 0.57), frame.wind.xy / max(strength, 1e-4), strength > 1e-4);
    let side = vec2<f32>(-along.y, along.x);
    let h0 = cover_mix(seed);
    let h1 = cover_mix(h0);
    let h2 = cover_mix(h1);
    let turn = cover_unit(h2) * COVER_TAU;
    let lean = vec2<f32>(cos(turn), sin(turn)) * mix(0.03, 0.12, cover_unit(cover_mix(h2)));
    if (strength == 0.0) {
        return lean * tall;
    }
    let rate = COVER_TAU / COVER_LAP;
    let n = wind_gust(foot, along, t - cover_unit(h0) * 1.5);
    let gust = smoothstep(0.25, 0.8, 0.5 * (n.x + n.y)) * mix(0.7, 1.3, cover_unit(h1));
    let swell = mix(0.8, 1.15, frame.wind.z);
    let turns = COVER_FLUTTER_TURNS + vec2<f32>(vec2<u32>(h1 >> 24u, (h1 >> 16u) & 255u));
    let shake = 0.6 * sin(t * turns.x * rate + cover_unit(h0 ^ h1) * COVER_TAU)
        + 0.4 * sin(t * turns.y * rate + cover_unit(h2 ^ h0) * COVER_TAU);
    let flutter = shake * (0.025 + 0.04 * gust) * min(strength * 1.6, 1.0);
    let eddy = (n.y - 0.5) * 0.12 * strength;
    return (frame.wind.xy * ((0.3 + 0.24 * gust) * swell + 0.5 * flutter) + side * (flutter + eddy)
        + lean) * tall;
}

// How far a point `rise` (0 the foot, 1 the top) up a stem `tall` metres high moves when its tip
// is blown `tip`: bent on a curve stiff at the foot, its length kept.
fn cover_bend(tip: vec2<f32>, rise: f32, tall: f32) -> vec3<f32> {
    let most = 0.8 * tall;
    let l = length(tip);
    let d = select(tip, tip * (most / max(l, 1e-6)), l > most);
    let r = clamp(rise, 0.0, 1.0);
    let up = sqrt(max(tall * tall - dot(d, d), 0.0)) - tall;
    return vec3<f32>(d * (0.3 * r + 0.7 * r * r), up * r * r);
}
