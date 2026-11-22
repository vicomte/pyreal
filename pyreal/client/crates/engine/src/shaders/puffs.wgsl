// Puffs: a body's billowing particles (an acid elemental's cloud) drawn as one volume in place of
// their cards. Each particle is a soft ball of the volume, as wide as its picture and as thick as
// its strength; one card covers the body's balls, and its whole volume is marched along the view
// once a pixel. The volume is
// churned and carved by noise, thinner at its edge than at its heart, lit by the sun (shadowed by
// the balls between), the sky and the lamps, and glows of itself; blended over what is behind it
// (premultiplied: its light, and how much of the behind it lets through).

// A ball's reach a unit of its particle's scale, and its spread (a Gaussian's) over its reach.
const PUFF_SIZE: f32 = 0.85;
const PUFF_SPREAD: f32 = 0.4;
// The most balls of its body a pixel's march gathers; the march's steps, fewer the more of the
// screen the body covers (past this many pixels), and the fewest.
const PUFF_NEAR: u32 = 32u;
const PUFF_STEPS: f32 = 20.0;
const PUFF_STEPS_UNTIL: f32 = 40000.0;
const PUFF_STEPS_LEAST: f32 = 8.0;
// How long a new ball takes to come in, of its life.
const PUFF_BIRTH: f32 = 0.12;
// A body with a ball wider than this, metres (a pillar), has its noise laid this much larger: a
// step, so it never breathes as balls come and go.
const PUFF_SCALE_FROM: f32 = 3.0;
const PUFF_SCALE_LARGE: f32 = 8.0;
// How late in its life a ball fades out.
const PUFF_DEATH: f32 = 0.88;
// The kind's bits in a run's material word (above its first and count), and the first kind
// `smoke.wgsl` marches (`puffs::SMOKE`).
const PUFF_KINDS: u32 = 31u;
const PUFF_SMOKE: u32 = 3u;
// A frost elemental's snow (`puffs::SNOW`), marched with steam.
const PUFF_SNOW: u32 = 6u;
// A room's haze (`puffs::HAZE`), marched with steam: a glow of its picture's colour.
const PUFF_HAZE: u32 = 8u;
// Haze: how thick a ball is through its heart at full strength, how much a step of it hides of
// what is behind it for the light it gives, and how brightly it glows (a share of a light).
const HAZE_THICKNESS: f32 = 0.6;
const HAZE_HIDES: f32 = 0.15;
const HAZE_GLOW: f32 = 0.008;
// Haze is soft and even: a few steps of it, and one octave of churn.
const HAZE_STEPS: i32 = 6;
// A vent's gas (`puffs::GAS`), marched with steam: a cloud of its picture's colour, lit by the
// room. How much of the light on it it gives back, how far its colour is kept from grey, how
// thick it is beside steam, and how fast it rises through its churn.
const PUFF_GAS: u32 = 9u;
const GAS_ALBEDO: f32 = 0.35;
const GAS_COLOUR: f32 = 0.85;
const GAS_THICKNESS: f32 = 1.0;
const GAS_RISE: f32 = 0.12;
// Its churn is laid at most this much larger with its puffs, so its billows show close by; and
// how hard its billows' edges are, however many of its puffs overlap; how wide (metres) a puff
// grows before it is as thick as its strength; and how deep the gas over a place shades it from
// the room's light.
const GAS_SCALE_MOST: f32 = 1.6;
const GAS_BILLOW: vec2<f32> = vec2<f32>(0.42, 0.62);
const GAS_SWELL: f32 = 0.08;
// How fast a fading puff thins: past its alpha, as a wide clear picture covers little of its disc.
const GAS_FADE: f32 = 1.2;
const GAS_SHADE: f32 = 0.8;
// Heavy gas (a vent's falling jet, marked in its transform's last empty entry) slumps as it ages:
// how flat and how wide it is at its death, how slowly it and the haze over it thin, how much
// light its body gives back and how far its colour is kept from grey (beside its vent's lamp), and
// how fast it sinks through its churn.
const GAS_POOL_FLAT: f32 = 0.45;
const GAS_POOL_WIDE: f32 = 1.6;
const GAS_POOL_FADE: f32 = 0.6;
const GAS_POOL_HAZE_FADE: f32 = 0.8;
const GAS_POOL_ALBEDO: f32 = 0.17;
const GAS_POOL_COLOUR: f32 = 0.45;
const GAS_POOL_THICKNESS: f32 = 0.5;
// How long (metres) a step through heavy gas may be, and its most steps.
const GAS_POOL_STEP: f32 = 0.3;
const GAS_POOL_STEPS: i32 = 24;
const GAS_SINK: f32 = 0.15;
// A grate's fume (`puffs::FUME`), marched with steam: a column of its picture's own colour, self-lit
// as retail draws it, laid over what is behind it. How thick a ball is through its heart at full
// strength (its alpha over its emitter's strongest), how wide (metres) it grows before it is as
// thick, and how much of the light on it it gives back.
const PUFF_FUME: u32 = 10u;
const FUME_THICKNESS: f32 = 0.6;
const FUME_SWELL: f32 = 0.06;
const FUME_ALBEDO: f32 = 0.12;
// How fast it rises through its churn, how fine that is beside steam's, how much a thin edge is
// eaten, and how far its billows stand out of the column, thin and thick.
const FUME_RISE: f32 = 0.7;
const FUME_CELLS: f32 = 1.2;
const FUME_EROSION: f32 = 0.75;
const FUME_BILLOW: vec2<f32> = vec2<f32>(0.3, 1.7);
// Its foot's warm glow: how bright (a share of white on screen, day or night, as retail's luminous
// pictures), how much of its hue it keeps, and how high it reaches, metres.
const FUME_GLOW: f32 = 0.25;
const FUME_GLOW_HUE: f32 = 0.6;
const FUME_GLOW_HEIGHT: f32 = 0.25;

// Acid: how thick a ball is through its heart at full strength (optical depth), the light it
// scatters of what falls on it, and its own glow: the colour at its edge and at its heart, and how
// strongly each glows.
const ACID_THICKNESS: f32 = 3.2;
// The most a metre of it takes, so a newborn ball a few centimetres wide is not a dark speck.
const ACID_DENSEST: f32 = 6.0;
// How strong a ball is at full weight, for its glow and how much of it the churn keeps.
const ACID_STRENGTH: f32 = 2.0;
const ACID_ALBEDO: vec3<f32> = vec3<f32>(0.16, 0.8, 0.12);
const ACID_EDGE: vec3<f32> = vec3<f32>(0.02, 0.45, 0.12);
const ACID_HEART: vec3<f32> = vec3<f32>(0.2, 1.0, 0.1);
const ACID_GLOW: vec2<f32> = vec2<f32>(0.02, 0.15);
// How much of the lamps' light it takes: its own glow lamp stands inside it.
const ACID_LAMPS: f32 = 0.1;
// Its churn: noise cells a metre, how fast it rises and boils.
const ACID_CHURN_SCALE: f32 = 3.8;
const ACID_RISE: f32 = 0.35;
const ACID_BOIL: f32 = 0.25;
// Its bubbles: cells a metre, how many cells hold one, how fast one swells and bursts, and the
// glow of its skin.
const ACID_BUBBLE_SCALE: f32 = 6.5;
const ACID_BUBBLES: f32 = 0.15;
const ACID_BUBBLE_RATE: f32 = 0.45;
const ACID_BUBBLE_GLOW: f32 = 0.25;
// The shimmer at its edge: its cells a metre, how fast it runs, and how bright.
const ACID_SHIMMER_SCALE: f32 = 6.0;
const ACID_SHIMMER_SPEED: f32 = 0.9;
const ACID_SHIMMER: f32 = 2.5;
// Its drips: one ball in this many drips, from this far through its life; how far a drop falls
// over the rest of it, metres; how wide a drop is, how thick, and how brightly it glows a metre.
const ACID_DRIP_EVERY: u32 = 4u;
const ACID_DRIP_START: f32 = 0.3;
const ACID_DRIP_FALL: f32 = 1.1;
const ACID_DROP: f32 = 0.034;
const ACID_DROP_THICKNESS: f32 = 0.4;
const ACID_DROP_GLOW: f32 = 6.0;

// Steam and frost mist (kinds 1 and 2): how thick a ball is through its heart at full strength,
// how wide (metres) it grows before it is as thick as its strength, and the light it scatters.
const STEAM_THICKNESS: f32 = 2.4;
const STEAM_SWELL: f32 = 0.35;
const STEAM_ALBEDO: f32 = 0.98;
// Its noise: cells a metre, how much of a thin edge it eats, how fast it churns, and how fast
// steam rises and frost sinks through it, metres a second.
const STEAM_CELLS: f32 = 2.2;
const STEAM_EROSION: f32 = 0.6;
const STEAM_CHURN: f32 = 0.12;
const STEAM_RISE: f32 = 0.35;
const FROST_SINK: f32 = 0.12;
// The largest puff's reach, metres, past which a steam body's noise is laid larger with it.
const STEAM_SCALE_FROM: f32 = 0.8;
// The share of the balls' depth toward the light the noise leaves (its holes, on the whole).
const STEAM_SHADE_SHARE: f32 = 0.4;
// How far toward the light, noise cells, a lobe's fold is looked for, and how much it shades.
const STEAM_LOBE: f32 = 0.3;
const STEAM_FOLD: f32 = 1.6;
// Frost's faint cold glow, and how much a coloured volume (a pillar's cyan) glows of its colour.
const FROST_GLOW: f32 = 0.02;
const STEAM_MAGIC_GLOW: f32 = 0.35;
// Snow's: its rime at the edge and its ice where it is deep; how dense it must be to be wholly
// ice; its cold glow; its noise's cells a metre across and up (streaks laid flat by the snow
// driving round it), how much a thin edge is eaten, how fast it whirls (radians a second) and
// sinks, and how much its grain shows.
const SNOW_RIME: vec3<f32> = vec3<f32>(0.88, 0.94, 1.0);
const SNOW_ICE: vec3<f32> = vec3<f32>(0.38, 0.76, 0.98);
const SNOW_DEEP: f32 = 4.0;
// How thick it is beside steam: a body, white at its heart, its edge thin.
const SNOW_THICKNESS: f32 = 1.2;
const SNOW_GLOW: f32 = 0.2;
const SNOW_CELLS: vec2<f32> = vec2<f32>(1.1, 1.6);
const SNOW_EROSION: f32 = 0.65;
// How far past its ball snow's march reaches: its spread is thick enough there to cut a hard edge.
const SNOW_CUT: f32 = 1.6;
const SNOW_WHIRL: f32 = 0.9;
const SNOW_SINK: f32 = 0.25;
const SNOW_GRAIN: f32 = 0.35;
// How much of the sky's hue steam takes (a dawn's blue would turn it lilac).
const STEAM_SKY_HUE: f32 = 0.5;
// The heights over its body the lamps' light is found at.
const STEAM_LAMP_POINTS: u32 = 3u;
// How far past its reach a ball off the view still shades it, metres.
const STEAM_SHADE_REACH: f32 = 1.5;

struct PuffOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) @interpolate(flat) index: u32,
    @location(2) @interpolate(flat) steps: u32,
}

// Particle `i` as a ball: its centre and reach.
fn puff_ball(i: u32) -> vec4<f32> {
    let p = particles[i];
    return vec4<f32>(p.m0.w, p.m1.w, p.m2.w, p.m0.x * PUFF_SIZE);
}

// Where particle `i`'s body stands (the particles carry it in their transforms' empty entries).
fn puff_body(i: u32) -> vec3<f32> {
    let p = particles[i];
    return vec3<f32>(p.m0.w + p.m0.y, p.m1.w + p.m0.z, p.m2.w + p.m1.z);
}

fn puff_age(i: u32) -> f32 {
    return f32((particles[i].flags >> 2u) & 1023u) / 1023.0;
}

fn puff_seed(i: u32) -> u32 {
    return particles[i].flags >> 13u;
}

// How strong ball `i` is, 0 to 1: its strength (an acid sprite's is half at most), slow to fade,
// eased in at its birth; steam's swells into being while it is small.
fn puff_weight(i: u32) -> f32 {
    let age = puff_age(i);
    let kind = (particles[i].material >> 23u) & PUFF_KINDS;
    if (kind == PUFF_SNOW) {
        // Snow holds its body as its pictures fade: they fade as the next ones come.
        return pow(clamp(particles[i].alpha, 0.0, 1.0), 0.3) * smoothstep(0.0, STEAM_SWELL, puff_ball(i).w);
    }
    // Gas is as strong as retail draws its puff: its young lumps stand out of its old haze.
    if (kind == PUFF_GAS) {
        let fade = select(select(GAS_FADE, GAS_POOL_HAZE_FADE, gas_sinks), GAS_POOL_FADE, gas_pools(i));
        return pow(clamp(particles[i].alpha, 0.0, 1.0), fade) * smoothstep(0.0, GAS_SWELL, puff_ball(i).w);
    }
    // Fume is as strong as retail draws its faint pictures, there from its birth at the grate.
    if (kind == PUFF_FUME) {
        return clamp(particles[i].alpha, 0.0, 1.0) * smoothstep(0.0, FUME_SWELL, puff_ball(i).w);
    }
    if (kind != 0u) {
        return sqrt(clamp(particles[i].alpha, 0.0, 1.0)) * smoothstep(0.0, STEAM_SWELL, puff_ball(i).w);
    }
    return pow(clamp(particles[i].alpha * 2.0, 0.0, 1.0), 0.75) * smoothstep(0.0, PUFF_BIRTH, age)
        * (1.0 - smoothstep(PUFF_DEATH, 1.0, age));
}

// Whether gas ball `i` is heavy gas that pools.
fn gas_pools(i: u32) -> bool {
    return particles[i].m2.y > 0.0;
}

// How much wider than round heavy gas ball `i` has spread.
fn gas_wide(i: u32) -> f32 {
    return mix(1.0, GAS_POOL_WIDE, puff_age(i));
}

// How thick steam's ball `i` is beside its strength (a geyser's jet is a veil): its transform's
// last empty entry, none for whole.
fn puff_share(i: u32, kind: u32) -> f32 {
    let s = particles[i].m2.y;
    return select(1.0, s, kind == 1u && s > 0.0);
}

// How thick a kind's ball is through its heart at full strength.
fn puff_thickness(kind: u32) -> f32 {
    if (kind == PUFF_SNOW) {
        return STEAM_THICKNESS * SNOW_THICKNESS;
    }
    if (kind == PUFF_HAZE) {
        return HAZE_THICKNESS;
    }
    if (kind == PUFF_GAS) {
        return STEAM_THICKNESS * GAS_THICKNESS;
    }
    if (kind == PUFF_FUME) {
        return FUME_THICKNESS;
    }
    return select(ACID_THICKNESS, STEAM_THICKNESS, kind != 0u);
}

// Steam's and frost's colour: steam white, frost a cold blue toward its picture's own where that
// is a strong colour; and the share of a light its colour glows.
fn steam_tint(kind: u32, material: u32) -> vec4<f32> {
    if (kind == PUFF_SNOW) {
        return vec4<f32>(SNOW_RIME, SNOW_GLOW);
    }
    if (kind != 2u && kind != PUFF_HAZE && kind != PUFF_GAS && kind != PUFF_FUME) {
        return vec4<f32>(0.97, 0.98, 1.0, 0.0);
    }
    let m = materials[material];
    var c = m.base_color.rgb;
    if ((m.flags & M_HAS_BASE) != 0u) {
        c *= textureSampleLevel(textures[m.tex_base], samp_repeat, vec2<f32>(0.5), 16.0).rgb;
    }
    // Fume keeps its picture's own colour, dark as it is: retail draws it unlit.
    if (kind == PUFF_FUME) {
        return vec4<f32>(c, FUME_GLOW);
    }
    let top = max(max(c.r, max(c.g, c.b)), 1e-4);
    if (kind == PUFF_HAZE) {
        return vec4<f32>(c / top, HAZE_GLOW);
    }
    if (kind == PUFF_GAS) {
        let hue = c / top;
        let keep = select(GAS_COLOUR, GAS_POOL_COLOUR, gas_sinks);
        return vec4<f32>(mix(vec3<f32>(luma(hue)), hue, keep) * select(GAS_ALBEDO, GAS_POOL_ALBEDO, gas_sinks), 0.0);
    }
    let strong = smoothstep(0.25, 0.6, 1.0 - min(c.r, min(c.g, c.b)) / top);
    return vec4<f32>(mix(vec3<f32>(0.86, 0.93, 1.0), c / top, 0.8 * strong),
                     FROST_GLOW + STEAM_MAGIC_GLOW * strong);
}

// Steam's churn at `x` with `rho` of strength there: what share of the field is kept (x), how a
// lobe there faces the sun (y) and the sky (z), its folds shaded (1 even).
fn steam_shape(at: vec3<f32>, rho: f32, kind: u32) -> vec3<f32> {
    let time = frame.wind.w;
    if (kind == PUFF_SNOW) {
        return snow_shape(at, rho, time);
    }
    var rise = select(STEAM_RISE, -FROST_SINK, kind == 2u);
    var churn = STEAM_CHURN;
    var cells = STEAM_CELLS;
    var erosion = STEAM_EROSION;
    if (kind == PUFF_FUME) {
        rise = FUME_RISE;
        cells = FUME_CELLS * STEAM_CELLS;
        erosion = FUME_EROSION;
    }
    if (kind == PUFF_GAS) {
        rise = select(GAS_RISE, -GAS_SINK, gas_sinks);
        churn = select(STEAM_CHURN, -STEAM_CHURN, gas_sinks);
    }
    if (kind == PUFF_HAZE) {
        // Haze drifts and churns slowly, lit by nothing but itself: its churn alone.
        let h = (at - puff_from - vec3<f32>(frame.wind.xy * 0.1, 0.0) * time) / puff_scale * STEAM_CELLS;
        let n = clamp((puff_fbm3(h + vec3<f32>(0.0, 0.0, -time * STEAM_CHURN)) - 0.5) * 2.2 + 0.5, 0.0, 1.0);
        return vec3<f32>(mix(0.4, 1.4, n), 1.0, 1.0);
    }
    var q = (at - puff_from - vec3<f32>(frame.wind.xy * 0.25, rise) * time) / puff_scale * cells;
    q.z -= time * churn;
    let n = clamp((puff_fbm3(q) - 0.5) * 2.2 + 0.5, 0.0, 1.0);
    // The noise eats the volume where it is thin: its edge billows, its heart stays whole. Gas
    // billows all through, its edge most.
    let billow = select(vec2<f32>(0.6, 1.4), FUME_BILLOW, kind == PUFF_FUME);
    var keep = max(rho - (1.0 - n) * erosion, 0.0) / max(rho, 1e-4) * mix(billow.x, billow.y, n);
    if (kind == PUFF_GAS) {
        // Lit by the room, not the sun: no lobe toward it to look for.
        let k = smoothstep(GAS_BILLOW.x, GAS_BILLOW.y, n + (min(rho, 1.0) - 0.6) * 0.5) * 2.0;
        return vec3<f32>(k, 1.0, mix(0.8, 1.1, n));
    }
    // The noise a little toward the sun: a lobe facing the light is lit, its folds shaded (from
    // the sky too, half as much).
    let toward = clamp((puff_fbm3(q + frame.sun_dir.xyz * STEAM_LOBE) - 0.5) * 2.2 + 0.5, 0.0, 1.0);
    let fold = clamp(1.0 + (n - toward) * STEAM_FOLD * min(rho, 1.5), 0.55, 1.4);
    return vec3<f32>(keep, fold, mix(1.0, fold, 0.5));
}

// Snow's at `x` with `rho` of strength there, as steam's: whirling round its body and sinking,
// in lumps with a fine grain, its edge eaten hard so it frays into flying snow.
fn snow_shape(at: vec3<f32>, rho: f32, time: f32) -> vec3<f32> {
    let p = (at - puff_from) / puff_scale;
    let turn = time * SNOW_WHIRL;
    let cs = vec2<f32>(cos(turn), sin(turn));
    let w = vec3<f32>(cs.x * p.x - cs.y * p.y, cs.y * p.x + cs.x * p.y, p.z + time * SNOW_SINK);
    let q = vec3<f32>(w.xy * SNOW_CELLS.x, w.z * SNOW_CELLS.y);
    let grain = smoke_value(q * 3.7 + 5.1);
    let n = clamp((puff_fbm3(q) - 0.5) * 3.0 + 0.5 + (grain - 0.5) * SNOW_GRAIN, 0.0, 1.0);
    let keep = max(rho - (1.0 - n) * SNOW_EROSION, 0.0) / max(rho, 1e-4) * mix(0.4, 1.5, n * n);
    let toward = clamp((puff_fbm3(q + frame.sun_dir.xyz * STEAM_LOBE * 2.0) - 0.5) * 2.6 + 0.5, 0.0, 1.0);
    let fold = clamp(1.0 + (n - toward) * STEAM_FOLD * min(rho, 1.5), 0.5, 1.5);
    return vec3<f32>(keep, fold, mix(1.0, fold, 0.5));
}

// Ball `i`'s drop, if it drips: its centre and how far through its fall (w; -1: none).
fn puff_drop(i: u32, kind: u32) -> vec4<f32> {
    let age = puff_age(i);
    if (kind != 0u || puff_seed(i) % ACID_DRIP_EVERY != 0u || age < ACID_DRIP_START) {
        return vec4<f32>(0.0, 0.0, 0.0, -1.0);
    }
    let b = puff_ball(i);
    let u = (age - ACID_DRIP_START) / (1.0 - ACID_DRIP_START);
    let s = f32(puff_seed(i) >> 4u);
    let side = vec2<f32>(cos(s * 0.618), sin(s * 0.618)) * b.w * 0.3;
    let fall = b.w * 0.4 + ACID_DRIP_FALL * u * u;
    return vec4<f32>(b.xy + side, b.z - fall, u);
}

// What ball `i` reaches with its drop's whole fall: a sphere.
fn puff_reach(i: u32, kind: u32) -> vec4<f32> {
    let b = puff_ball(i);
    if (b.w <= 0.0 || puff_weight(i) <= 0.0) {
        return vec4<f32>(b.xyz, 0.0);
    }
    if (kind == PUFF_SNOW) {
        return vec4<f32>(b.xyz, b.w * SNOW_CUT);
    }
    if (kind == PUFF_GAS && gas_pools(i)) {
        return vec4<f32>(b.xyz, b.w * gas_wide(i));
    }
    if (kind != 0u || puff_seed(i) % ACID_DRIP_EVERY != 0u) {
        return b;
    }
    let top = b.z + b.w;
    let bottom = b.z - b.w * 0.4 - ACID_DRIP_FALL - 0.2;
    let mid = (top + bottom) * 0.5;
    let h = (top - bottom) * 0.5;
    return vec4<f32>(b.xy, mid, max(h, b.w + abs(b.z - mid)) + b.w * 0.3);
}

// How far from the middle of its body a ball may stray and still be covered (one left behind).
const PUFF_STRAY: f32 = 30.0;

// Where body `i`'s balls lie on screen: the rectangle round them (xy least, zw most, clip space);
// the whole screen when the eye is in one, none when it has none.
fn puff_bounds(i: u32) -> vec4<f32> {
    let m = particles[i].material;
    let first = m & 0x7FFFu;
    let count = puff_count(first, (m >> 15u) & 0xFFu);
    let kind = (m >> 23u) & PUFF_KINDS;
    var mid = vec3<f32>(0.0);
    var n = 0.0;
    for (var k = 0u; k < count; k++) {
        let b = puff_reach(first + k, kind);
        if (b.w > 0.0) {
            mid += b.xyz;
            n += 1.0;
        }
    }
    if (n == 0.0) {
        return vec4<f32>(0.0);
    }
    mid /= n;
    var lo = vec2<f32>(1e9);
    var hi = vec2<f32>(-1e9);
    for (var k = 0u; k < count; k++) {
        let b = puff_reach(first + k, kind);
        if (b.w <= 0.0 || distance(b.xyz, mid) > PUFF_STRAY) {
            continue;
        }
        for (var c = 0u; c < 8u; c++) {
            let o = vec3<f32>(f32(c & 1u), f32((c >> 1u) & 1u), f32((c >> 2u) & 1u)) * 2.0 - 1.0;
            let h = frame.view_proj * vec4<f32>(b.xyz + o * b.w, 1.0);
            if (h.w <= 0.05) {
                return vec4<f32>(-1.0, -1.0, 1.0, 1.0);
            }
            lo = min(lo, h.xy / h.w);
            hi = max(hi, h.xy / h.w);
        }
    }
    return vec4<f32>(max(lo, vec2<f32>(-1.0)), min(hi, vec2<f32>(1.0)));
}

@vertex
fn vs_puff(@builtin(vertex_index) vi: u32, @builtin(instance_index) i: u32) -> PuffOut {
    // A card a body, by its first ball.
    var b = vec4<f32>(0.0);
    if ((particles[i].material & 0x7FFFu) == i) {
        b = puff_bounds(i);
    }
    let corners = array<vec2<f32>, 6>(vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
                                      vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0));
    let q = mix(b.xy, b.zw, corners[vi]);
    var out: PuffOut;
    out.index = i;
    out.clip = vec4<f32>(q, 0.5, 1.0);
    let h = frame.inv_view_proj * out.clip;
    out.world = h.xyz / h.w;
    let pixels = max((b.z - b.x) * (b.w - b.y) * 0.25 * frame.params.z * frame.params.w, 1.0);
    out.steps = u32(clamp(PUFF_STEPS * sqrt(PUFF_STEPS_UNTIL / pixels), PUFF_STEPS_LEAST, PUFF_STEPS));
    if (b.z <= b.x || b.w <= b.y) {
        out.clip = vec4<f32>(0.0, 0.0, -2.0, 1.0);
    }
    return out;
}

fn puff_erf(x: f32) -> f32 {
    let a = 0.147;
    let x2 = x * x;
    return sign(x) * sqrt(1.0 - exp(-x2 * (1.2732395 + a * x2) / (1.0 + a * x2)));
}

// The gathered balls: centre and spread across (xyz, w), thickness at the centre a metre, spread
// up and down, and where the view enters and leaves them (x, y, z, w); and each one's strength.
var<private> puff_at: array<vec4<f32>, PUFF_NEAR>;
var<private> puff_span: array<vec4<f32>, PUFF_NEAR>;
var<private> puff_w: array<f32, PUFF_NEAR>;
var<private> puff_n: u32;
// How much larger than a metre's the body's noise is laid, and where from.
var<private> puff_scale: f32;
var<private> puff_from: vec3<f32>;
// A steam body's emitters' largest puff's reach, metres.
var<private> steam_reach: f32;
// Whether a gas body is heavy gas, sinking through its churn.
var<private> gas_sinks: bool;

// How much of the light from `l` the gathered balls let reach `x` (their analytic thickness), and
// from `l2`.
fn puff_through(x: vec3<f32>, l: vec3<f32>, l2: vec3<f32>) -> vec2<f32> {
    var depth = vec2<f32>(0.0);
    for (var k = 0u; k < puff_n; k++) {
        let s = puff_at[k].w;
        let thick = puff_span[k].x * s;
        let o = puff_at[k].xyz - x;
        let a1 = dot(o, l);
        let a2 = dot(o, l2);
        // Along the ray past the nearest point, a Gaussian's integral: its thickness over its
        // spread times the share past `x`.
        depth += thick * 1.2533141 * vec2<f32>(
            exp(-(dot(o, o) - a1 * a1) / (2.0 * s * s)) * (1.0 + puff_erf(a1 / (s * 1.4142136))),
            exp(-(dot(o, o) - a2 * a2) / (2.0 * s * s)) * (1.0 + puff_erf(a2 / (s * 1.4142136))));
    }
    return exp(-depth);
}

// How thick ball `c` (spread in w, `thick` at its heart) is along the view, from the eye's offset
// `o` from it and the view's run to its nearest point `bb` (negated).
fn puff_worth(c: vec4<f32>, thick: f32, o: vec3<f32>, bb: f32) -> f32 {
    return thick * c.w * exp(-max(dot(o, o) - bb * bb, 0.0) / (2.0 * c.w * c.w));
}

// The volume at `x`, before its noise: thickness a metre (x), strength (y) and the drops' glow
// (z), 0 for none.
fn puff_field(x: vec3<f32>, t: f32) -> vec3<f32> {
    var f = vec3<f32>(0.0);
    for (var k = 0u; k < puff_n; k++) {
        let sp = puff_span[k];
        if (t < sp.z || t > sp.w) {
            continue;
        }
        let d = x - puff_at[k].xyz;
        let sa = puff_at[k].w;
        let g = exp(-(dot(d.xy, d.xy) / (2.0 * sa * sa) + d.z * d.z / (2.0 * sp.y * sp.y)));
        let w = puff_w[k];
        f += vec3<f32>(sp.x, max(w, 0.0), max(-w, 0.0)) * g;
    }
    return f;
}

// Three octaves of value noise, 0 to 1.
fn puff_fbm3(p: vec3<f32>) -> f32 {
    let a = smoke_value(p);
    let b = smoke_value(vec3<f32>(p.y * 1.6 + p.x * 1.2, p.x * 1.6 - p.y * 1.2, p.z * 1.7) + 11.3);
    let c = smoke_value(p * 3.9 + 23.1);
    return (a * 0.5 + b * 0.25 + c * 0.125) / 0.875;
}

// Acid's churn at `x` with `rho` of strength there: what share of the field is kept (x), the glow
// of a bubble's skin (y), the shimmer at its edge (z) and how hot it glows there (w): its heart's
// veins and folds.
fn acid_shape(at: vec3<f32>, rho: f32) -> vec4<f32> {
    let time = frame.wind.w;
    let x = (at - puff_from) / puff_scale;
    var q = x * ACID_CHURN_SCALE;
    q.z -= time * ACID_RISE;
    let w0 = smoke_value(q * 0.6 + vec3<f32>(0.0, 0.0, time * ACID_BOIL)) - 0.5;
    let w = vec3<f32>(w0, -w0 * 0.7, w0 * 0.5) + vec3<f32>(sin(q.z * 1.3 + time * 0.7), cos(q.x * 1.1 - time * 0.6), 0.0) * 0.25;
    let n = puff_fbm3(q + w * 1.6 + vec3<f32>(0.0, 0.0, time * ACID_BOIL));
    // The fbm's spread is narrow: widened, then eaten away more the thinner the field.
    let billow = clamp((n - 0.5) * 2.8 + 0.5, 0.0, 1.0);
    // Its heart is clumps thick and thin; its thin edge is eaten into wisps.
    let thin = 1.0 - min(rho, 1.0);
    var keep = clamp(billow * 1.9 + 0.25 - thin * 1.7, 0.0, 1.0);
    var hot = 0.3 + 0.6 * billow;
    if (rho > 0.1 && keep > 0.0) {
        let ridge = smoke_value(q * 2.3 + w * 2.5 + vec3<f32>(0.0, 0.0, -time * ACID_BOIL * 2.0));
        hot += 0.9 * pow(1.0 - abs(2.0 * ridge - 1.0), 5.0) * billow;
    }

    // Bubbles rising through it, each swelling and bursting.
    var skin = 0.0;
    // Not on a body many metres wide (a pillar), where they would be rings a metre across.
    if (rho > 0.5 && keep > 0.0 && puff_scale < 2.0) {
        var b = x * ACID_BUBBLE_SCALE;
        b.z -= time * ACID_RISE * ACID_BUBBLE_SCALE * 0.6;
        let cell = floor(b);
        let f = b - cell;
        for (var k = 0u; k < 8u; k++) {
            let o = vec3<f32>(f32(k & 1u), f32((k >> 1u) & 1u), f32((k >> 2u) & 1u)) - vec3<f32>(step(f, vec3<f32>(0.5)));
            let c = cell + o;
            let h = smoke_hash(c);
            if (h > ACID_BUBBLES) {
                continue;
            }
            let at = o + vec3<f32>(smoke_hash(c + 7.1), smoke_hash(c + 13.7), smoke_hash(c + 29.3)) * 0.5 + 0.25;
            let life = fract(time * ACID_BUBBLE_RATE + h * 9.7);
            let r = 0.36 * smoothstep(0.0, 1.0, life);
            let d = length(b - cell - at);
            let pop = 1.0 - smoothstep(0.85, 1.0, life);
            skin = max(skin, exp(-pow((d - r) / 0.05, 2.0)) * pop * smoothstep(0.2, 0.5, life));
        }
        let deep = smoothstep(0.5, 0.9, rho);
        skin *= deep;
    }

    // Caustic threads shimmering over its thin edge.
    let edge = smoothstep(0.02, 0.12, rho * keep) * (1.0 - smoothstep(0.2, 0.55, rho));
    var thread = 0.0;
    if (edge > 0.0) {
        let s = x * ACID_SHIMMER_SCALE;
        let run = time * ACID_SHIMMER_SPEED;
        let c1 = smoke_value(s + vec3<f32>(run, -run * 0.7, run * 0.5));
        let c2 = smoke_value(s * 1.9 + vec3<f32>(-run * 0.8, run * 0.6, run));
        thread = pow(1.0 - abs(c1 + c2 - 1.0), 12.0);
    }
    return vec4<f32>(keep, skin, edge * thread, hot);
}

// A body's light and its share of what is behind (premultiplied), and how much it changes the
// pixel for the upscaler (soft vapour drifts, so its history mostly holds; a flash does not).
struct PuffDrawn {
    @location(0) colour: vec4<f32>,
    @location(1) reactive: vec4<f32>,
}

// A volume's share of the upscaler's reactive mask over its coverage.
const PUFF_REACTIVE: f32 = 0.3;

@fragment
fn fs_puff(in: PuffOut) -> PuffDrawn {
    let c = puff_seen(in);
    var out: PuffDrawn;
    out.colour = c;
    out.reactive = vec4<f32>(clamp((1.0 - c.a) * PUFF_REACTIVE + puff_flashed, 0.0, 1.0));
    return out;
}

// How much a strike lit the pixel's volume this frame (set by `smokes_volume`), for the mask.
var<private> puff_flashed: f32;

fn puff_seen(in: PuffOut) -> vec4<f32> {
    let p = particles[in.index];
    let first = p.material & 0x7FFFu;
    let count = puff_count(first, (p.material >> 15u) & 0xFFu);
    let kind = (p.material >> 23u) & PUFF_KINDS;
    // A portal is its own disc (`gates.wgsl`).
    if (kind == GATE_KIND) {
        return gate_volume(in);
    }
    // Smoke and dust march their own way (`smoke.wgsl`).
    if (kind >= PUFF_SMOKE && kind != PUFF_SNOW && kind != PUFF_HAZE && kind != PUFF_GAS && kind != PUFF_FUME) {
        return smokes_volume(in);
    }
    let eye = frame.camera.xyz;
    let dir = normalize(in.world - eye);
    let px = in.clip.xy;
    let behind = ice_room(px, 0.0);

    // The body's balls the view crosses before what is behind.
    puff_n = 0u;
    var widest = 0.0;
    for (var k = 0u; k < count; k++) {
        widest = max(widest, puff_ball(first + k).w);
    }
    puff_scale = select(1.0, PUFF_SCALE_LARGE, widest > PUFF_SCALE_FROM);
    puff_from = puff_body(first);
    if (kind != 0u) {
        // Steam's noise is sized by its emitters' largest puff (carried in a ball's empty
        // transform entry), so it holds still as its puffs come and go.
        var largest = 0.0;
        for (var k = 0u; k < count; k++) {
            largest = max(largest, particles[first + k].m1.x);
        }
        steam_reach = largest * PUFF_SIZE;
        gas_sinks = false;
        for (var k = 0u; k < count && kind == PUFF_GAS; k++) {
            gas_sinks = gas_sinks || gas_pools(first + k);
        }
        puff_scale = clamp(steam_reach / STEAM_SCALE_FROM, 1.0, select(12.0, GAS_SCALE_MOST, kind == PUFF_GAS));
    }
    var t0 = 1e9;
    var t1 = 0.0;
    for (var k = 0u; k < count; k++) {
        let j = first + k;
        let reach = puff_reach(j, kind);
        if (reach.w <= 0.0) {
            continue;
        }
        let oc = eye - reach.xyz;
        let bq = dot(oc, dir);
        let disc = bq * bq - (dot(oc, oc) - reach.w * reach.w);
        if (disc <= 0.0) {
            continue;
        }
        let sq = sqrt(disc);
        if (-bq + sq <= 0.0 || -bq - sq >= behind) {
            continue;
        }
        let w = puff_weight(j);
        let b = puff_ball(j);
        let s = b.w * PUFF_SPREAD;
        // The ball, and its drop.
        for (var e = 0u; e < 2u; e++) {
            var c = vec4<f32>(b.xyz, s);
            var sz = s;
            var thick = w * puff_thickness(kind) * puff_share(j, kind) / (s * 2.5066283);
            if (kind == 0u) {
                thick = min(thick, ACID_DENSEST);
            }
            var strength = w * select(ACID_STRENGTH, 1.0, kind != 0u);
            var r = select(b.w, b.w * SNOW_CUT, kind == PUFF_SNOW);
            // Heavy gas slumps lower and wider as it ages, its foot where it was, as thick in all.
            if (kind == PUFF_GAS && gas_pools(j)) {
                let flat = mix(1.0, GAS_POOL_FLAT, puff_age(j));
                let wide = gas_wide(j);
                c = vec4<f32>(b.xy, b.z - 2.0 * s * (1.0 - flat), s * wide);
                sz = s * flat;
                thick *= GAS_POOL_THICKNESS / (wide * wide * flat);
                r = b.w * wide;
            }
            if (e == 1u) {
                let d = puff_drop(j, kind);
                if (d.w < 0.0) {
                    break;
                }
                let fade = smoothstep(0.0, 0.1, d.w) * (1.0 - smoothstep(0.85, 1.0, d.w));
                c = vec4<f32>(d.xyz, ACID_DROP);
                sz = ACID_DROP * (1.0 + 2.5 * d.w);
                thick = fade * ACID_DROP_THICKNESS / (ACID_DROP * 2.5066283);
                // A drop's strength is its glow, kept apart (negative).
                strength = -fade * ACID_DROP_GLOW;
                r = sz * 3.0;
            }
            let o = eye - c.xyz;
            let bb = dot(o, dir);
            let dd = bb * bb - (dot(o, o) - r * r);
            if (dd <= 0.0) {
                continue;
            }
            let sd = sqrt(dd);
            let tin = max(-bb - sd, 0.0);
            let tout = min(-bb + sd, behind);
            if (tout <= tin) {
                continue;
            }
            // When full, the ball that would add least along this view gives way.
            var slot = puff_n;
            if (puff_n == PUFF_NEAR) {
                var least = puff_worth(c, thick, o, bb);
                for (var q = 0u; q < PUFF_NEAR; q++) {
                    let w = puff_worth(puff_at[q], puff_span[q].x, eye - puff_at[q].xyz, dot(eye - puff_at[q].xyz, dir));
                    if (w < least) {
                        least = w;
                        slot = q;
                    }
                }
                if (slot == PUFF_NEAR) {
                    continue;
                }
            } else {
                puff_n++;
            }
            puff_at[slot] = c;
            puff_span[slot] = vec4<f32>(thick, sz, tin, tout);
            puff_w[slot] = strength;
            t0 = min(t0, tin);
            t1 = max(t1, tout);
        }
    }
    if (t1 <= t0) {
        discard;
    }
    // Steam is shaded by its body's balls near the view too (those over it, toward the sun), never
    // marched through.
    if (kind != 0u && kind != PUFF_HAZE) {
        for (var k = 0u; k < count && puff_n < PUFF_NEAR; k++) {
            let b = puff_ball(first + k);
            let w = puff_weight(first + k);
            if (b.w <= 0.0 || w <= 0.0) {
                continue;
            }
            let along = dot(b.xyz - eye, dir);
            let off = distance(b.xyz, eye + dir * along);
            if (off < b.w || off > b.w + STEAM_SHADE_REACH || along < t0 - b.w || along > t1 + b.w) {
                continue;
            }
            let sp = b.w * PUFF_SPREAD;
            puff_at[puff_n] = vec4<f32>(b.xyz, sp);
            puff_span[puff_n] = vec4<f32>(w * STEAM_THICKNESS * puff_share(first + k, kind) / (sp * 2.5066283), sp, 1e9, -1e9);
            puff_w[puff_n] = w;
            puff_n++;
        }
    }

    // What lights it, once a pixel: where the view is halfway through it.
    let indoor = p.cell != 0u;
    let pre = pre_exposure();
    let mid = eye + dir * (t0 + t1) * 0.5;
    let depth = view_depth_at(mid);
    var sun = frame.sun_light.rgb;
    var sky = frame.sky_light.rgb;
    if (!has(F_PHYSICAL)) {
        sun = pow(frame.sun.rgb, vec3<f32>(2.2));
        sky = pow(min(frame.ambient.rgb, vec3<f32>(1.0)), vec3<f32>(2.2));
    }
    var sun_vis = 0.0;
    if (indoor) {
        sky = frame.indoor_light.rgb;
    } else {
        sun_vis = 1.0;
        if (has(F_SHADOWS)) {
            sun_vis = sun_shadow(mid, frame.sun_dir.xyz, depth, px);
        }
    }
    var lamps = vec3<f32>(0.0);
    let steam = kind != 0u;
    if (has(F_LAMPS) && !steam) {
        let bits = select(0u, G_INDOOR, indoor);
        lamps = lamp_light(mid, -dir, -dir, 1.0, vec3<f32>(0.0), bits, depth, px, false).diffuse * ACID_LAMPS;
    }
    // Steam takes the lamps at a few heights over where its body stands, alike over all its
    // pixels and from frame to frame, blended by nearness as it is marched.
    var lamp_at: array<vec4<f32>, STEAM_LAMP_POINTS>;
    var lamp_of: array<vec3<f32>, STEAM_LAMP_POINTS>;
    // Gas, low over its vent, takes them at one, halfway up.
    var lamp_balls = select(0u, STEAM_LAMP_POINTS, steam && kind != PUFF_HAZE && has(F_LAMPS));
    if (kind == PUFF_GAS || kind == PUFF_FUME) {
        lamp_balls = min(lamp_balls, 1u);
    }
    // As high as its balls stand over its body, no higher: a vent's steam takes no lamp from the
    // shaft over it.
    var top = 0.0;
    for (var k = 0u; k < count; k++) {
        let b = puff_ball(first + k);
        if (b.w > 0.0) {
            top = max(top, b.z - puff_from.z);
        }
    }
    let lamp_span = clamp(top * 0.5, 0.25, max(steam_reach, 0.5));
    let heights = array<f32, STEAM_LAMP_POINTS>(0.3, 1.0, 2.0);
    for (var k = 0u; k < lamp_balls; k++) {
        let h = select(heights[k], 1.0, kind == PUFF_GAS || kind == PUFF_FUME);
        let b = vec4<f32>(puff_from + vec3<f32>(0.0, 0.0, h * lamp_span), lamp_span);
        lamp_at[k] = b;
        let c = frame.view_proj * vec4<f32>(b.xyz, 1.0);
        var at = px;
        if (c.w > 0.05) {
            at = clamp(vec2<f32>(c.x / c.w * 0.5 + 0.5, 0.5 - c.y / c.w * 0.5) * frame.params.zw,
                       vec2<f32>(0.0), frame.params.zw - 1.0);
        }
        lamp_of[k] = lamp_light(b.xyz, -dir, -dir, 1.0, vec3<f32>(0.0), G_VOLUME | select(0u, G_INDOOR, indoor),
                                max(view_depth_at(b.xyz), 0.1), at, false).diffuse;
    }
    var tint = vec4<f32>(1.0, 1.0, 1.0, 0.0);
    var steam_sun = sun * sun_vis * smoothstep(-0.05, 0.05, frame.sun_dir.z);
    var steam_phase = 1.0;
    var steam_glow = vec3<f32>(0.0);
    if (steam) {
        tint = steam_tint(kind, puff_seed(first));
        // The eye holds steam white: the sun's hue is taken only a little, and the sky's half.
        steam_sun = mix(vec3<f32>(luma(steam_sun)), steam_sun, 0.3);
        if (kind == 1u) {
            sky = mix(vec3<f32>(luma(sky)), sky, STEAM_SKY_HUE);
        }
        // Droplets scatter forward, a little back: relative to an even spread.
        let mu = dot(dir, frame.sun_dir.xyz);
        let g1 = 1.0 + 0.3025 - 1.1 * mu;
        let g2 = 1.0 + 0.0625 + 0.5 * mu;
        steam_phase = mix(0.6975 / (g1 * sqrt(g1)), 0.9375 / (g2 * sqrt(g2)), 0.35);
        steam_glow = tint.rgb * glow_of(vec3<f32>(1.0), tint.w);
    }
    let cos_s = dot(dir, frame.sun_dir.xyz);
    // Half of it scattered on forward, half every way (it glows lit from behind, not dark in front).
    let g = 0.35;
    let phase = mix(1.0, (1.0 - g * g) / pow(1.0 + g * g - 2.0 * g * cos_s, 1.5), 0.5);
    let sun_in = sun * sun_vis * phase * smoothstep(-0.1, 0.1, frame.sun_dir.z);

    // March it front to back.
    var steps = select(i32(in.steps), min(i32(in.steps), HAZE_STEPS), kind == PUFF_HAZE);
    if (kind == PUFF_GAS && gas_sinks) {
        // Heavy gas's tongue is thin: a step no longer than it, to a most.
        steps = clamp(i32(ceil((t1 - t0) / GAS_POOL_STEP)), steps, GAS_POOL_STEPS);
    }
    let dt = (t1 - t0) / f32(steps);
    var t = t0 + dt * fract(ign(px) + frame.wind.w * 7.31);
    var through = 1.0;
    var light = vec3<f32>(0.0);
    var seen_at = t1;
    // The balls' shade from the sun and the sky at the near end, the middle and the far end.
    let up = vec3<f32>(0.0, 0.0, 1.0);
    // Haze is lit by nothing but itself: no shade.
    var shade0 = vec2<f32>(1.0);
    var shade1 = vec2<f32>(1.0);
    var shade2 = vec2<f32>(1.0);
    if (kind != PUFF_HAZE) {
        shade0 = puff_through(eye + dir * t0, frame.sun_dir.xyz, up);
        shade1 = puff_through(mid, frame.sun_dir.xyz, up);
        shade2 = puff_through(eye + dir * t1, frame.sun_dir.xyz, up);
    }
    for (var s = 0; s < steps; s++) {
        let x = eye + dir * t;
        let f = puff_field(x, t);
        if (f.x > 1e-3 && steam) {
            let shape = steam_shape(x, f.y, kind);
            let sigma = f.x * shape.x;
            let u = clamp((t - t0) / (t1 - t0) * 2.0, 0.0, 2.0);
            let shade = mix(mix(shade0, shade1, min(u, 1.0)), shade2, max(u - 1.0, 0.0));
            let sun_depth = -log(max(shade.x, 1e-6)) * STEAM_SHADE_SHARE;
            let over_depth = -log(max(shade.y, 1e-6)) * STEAM_SHADE_SHARE;
            // Light scattered many times gets deeper than once: a few octaves of it.
            let sunlit = (exp(-sun_depth) * steam_phase + 0.6 * exp(-sun_depth * 0.25) * mix(1.0, steam_phase, 0.5))
                * shape.y + 0.3 * exp(-sun_depth * 0.08);
            let skylit = mix(0.2, 1.0, exp(-over_depth * 0.6)) * shape.z;
            var lamp = vec3<f32>(0.0);
            var near = 1e-4;
            for (var k = 0u; k < lamp_balls; k++) {
                let b = lamp_at[k];
                let wk = exp(-dot(x - b.xyz, x - b.xyz) / max(b.w * b.w, 1e-4)) + 1e-3;
                lamp += lamp_of[k] * wk;
                near += wk;
            }
            let lamp_shade = select(mix(0.4, 1.0, exp(-over_depth * 0.3)),
                                    mix(0.2, 1.0, exp(-over_depth * GAS_SHADE)), kind == PUFF_GAS);
            lamp *= lamp_shade / near;
            // Snow is rime where it is thin and ice where it is deep, light inside it bluer.
            var colour = tint.rgb;
            if (kind == PUFF_SNOW) {
                let deep = smoothstep(0.7, SNOW_DEEP, f.y * shape.x + sun_depth * 0.25);
                colour = mix(SNOW_RIME, SNOW_ICE, deep);
            }
            // Its glow, too, shows its lobes and is deeper in its folds.
            var lit = (steam_sun * sunlit + sky * skylit + lamp) * colour * STEAM_ALBEDO
                + steam_glow * skylit * 1.5;
            var hides = 1.0;
            if (kind == PUFF_FUME) {
                // Fume is lit as a picture retail draws unlit is: never darker than its own colour.
                // Its foot, over the grate, glows warm.
                let hue = colour / max(max(colour.r, max(colour.g, colour.b)), 1e-4);
                let world = (steam_sun * sunlit + sky * skylit + lamp) * hue * FUME_ALBEDO;
                let foot = exp(-max(x.z - puff_from.z, 0.0) / FUME_GLOW_HEIGHT);
                lit = max(world, colour / pre)
                    + mix(vec3<f32>(luma(hue)), hue, FUME_GLOW_HUE) * (tint.w / pre) * foot * shape.z;
            }
            if (kind == PUFF_HAZE) {
                // Haze is its own glow, as retail adds its pictures, and hides little of what is
                // behind it.
                lit = steam_glow * shape.z;
                hides = HAZE_HIDES;
            }
            let a = 1.0 - exp(-sigma * dt);
            light += through * a * lit;
            if (through > 0.5 && through * (1.0 - a * hides) <= 0.5) {
                seen_at = t;
            }
            through *= 1.0 - a * hides;
            if (through < 0.01) {
                break;
            }
        } else if (f.x > 1e-3) {
            let shape = acid_shape(x, f.y);
            let sigma = f.x * shape.x;
            let rho = f.y * shape.x;
            let u = clamp((t - t0) / (t1 - t0) * 2.0, 0.0, 2.0);
            let shade = mix(mix(shade0, shade1, min(u, 1.0)), shade2, max(u - 1.0, 0.0));
            // Light scattered on through it keeps some of the shaded side lit.
            // The billows' tops lit, the folds between them dark.
            let fold = mix(0.45, 1.25, clamp((shape.w - 0.3) / 0.6, 0.0, 1.0));
            let lit = ACID_ALBEDO * fold
                * (sun_in * (0.25 + 0.75 * shade.x) + sky * (0.4 + 0.6 * shade.y) + lamps);
            let heat = clamp(smoothstep(0.1, 1.1, f.y) * shape.w, 0.0, 1.0);
            let glow = glow_of(mix(ACID_EDGE, ACID_HEART, heat), mix(ACID_GLOW.x, ACID_GLOW.y, heat)) * fold;
            let a = 1.0 - exp(-sigma * dt);
            let own = glow_of(ACID_HEART, ACID_BUBBLE_GLOW) * shape.y * shape.x
                + glow_of(mix(ACID_EDGE, ACID_HEART, 0.5), ACID_SHIMMER) * shape.z;
            light += through * (a * (lit + glow) + dt * (own * f.y + glow_of(ACID_HEART, f.z)));
            if (through > 0.5 && through * (1.0 - a) <= 0.5) {
                seen_at = t;
            }
            through *= 1.0 - a;
            if (through < 0.01) {
                break;
            }
        }
        t += dt;
    }
    light *= pre;
    if (frame.umbrae[0].w > 0.0) {
        light *= umbra(mid);
    }
    // Seen through the air: its own light dimmed and the air's in front laid over what it hides.
    if (!indoor) {
        let at = eye + dir * min(seen_at, t1);
        if (has(F_ATMOSPHERE)) {
            let air_light = aerial(vec3<f32>(0.0), at, dir);
            let air_through = aerial(vec3<f32>(1.0), at, dir) - air_light;
            light = light * air_through + air_light * pre * (1.0 - through);
        } else {
            let fog = fog_amount(distance(at.xy, eye.xy));
            light = light * (1.0 - fog) + sky_colour(dir) * pre * fog * (1.0 - through);
        }
    }
    return vec4<f32>(min(light, vec3<f32>(HDR_MOST)), through);
}
