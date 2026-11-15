// The water near the eye, simulated (render-hifi's, ported): a square of the surface 20 metres
// across, its height in each small cell stepped as waves run (Tessendorf's iWave: each cell pulled
// by its neighbours' heights through `KERNEL`, the water's vertical derivative), so what disturbs
// it (a body wading, later the rain) spreads in trains of ripples, crosses, meets the shore and
// dies away. The square is stored wrapped: a world cell is always at the same texel, so it follows
// the eye a cell at a time. Each step reads one picture (r the height now, g a step ago, b how
// sheltered by a body standing in it, a the froth where it meets a body's legs and where a body
// churns it, fading over a second or so, so a wader leaves a trail) and writes the next. Land holds the water's level, so waves turn back from the shore.

struct Step {
    // xy the square's first cell; zw the first cell of the square the last step held.
    origin: vec4<i32>,
    // xy the first cell's south-west corner, render space; zw unused.
    place: vec4<f32>,
    // x gravity times a step squared, in cells; y how much motion the water loses a step;
    // z the chance a drop lands in a patch of the rain's lattice; w how deep a drop dents it.
    wave: vec4<f32>,
    // x the step's count, for the rain; y how many bodies stand in `start` and `end`.
    misc: vec4<u32>,
    // How far the waves lift the water over its level where each body stands.
    lift: array<vec4<f32>, 2>,
    // Each body at the step's start and end: xy where it stands, z its feet, w how much it stirs.
    start: array<vec4<f32>, 8>,
    end: array<vec4<f32>, 8>,
}

@group(1) @binding(0) var<uniform> S: Step;
@group(1) @binding(1) var state: texture_2d<f32>;
@group(1) @binding(2) var next: texture_storage_2d<rgba16float, write>;

const SIDE: i32 = 256;
const CELL: f32 = 0.08;
const GROUP: i32 = 16;
const REACH: i32 = 5;
const TILE: i32 = 26;
const SPONGE: f32 = 14.0;
const LEGS: f32 = 0.35;
const PUSH: f32 = 0.1;
const SWAY: f32 = 0.45;
const SWAYS: f32 = 9.0;
const SWAYS_TOO: f32 = 13.0;
const SHELTER: f32 = 0.9;
const BOW_OFF: f32 = 0.38;
const BOW_WIDTH: f32 = 0.28;
const BOW_RISE: f32 = 0.008;
const RUN: f32 = 4.0;
const STEP_SECONDS: f32 = 1.0 / 60.0;
const RAIN_PATCH: f32 = 0.4;
const DROP: f32 = 0.07;
// What the froth keeps a step, and how hard a body must churn a cell to froth it.
const FROTH_KEEPS: f32 = 0.985;
const CHURN_FROM: f32 = 0.0015;
const CHURN_FULL: f32 = 0.01;

var<workgroup> tile: array<f32, 676>;
// How far each body wades at the step's start (x) and end (y), 0 to 1.
var<workgroup> soak: array<vec2<f32>, 8>;

fn wrap(w: vec2<i32>) -> vec2<i32> {
    return ((w % vec2<i32>(SIDE)) + vec2<i32>(SIDE)) % vec2<i32>(SIDE);
}

fn within(w: vec2<i32>, first: vec2<i32>) -> bool {
    let r = w - first;
    return all(r >= vec2<i32>(0)) && all(r < vec2<i32>(SIDE));
}

// The water at world cell `w` as the last step left it: calm where that square did not reach.
fn held(w: vec2<i32>) -> vec4<f32> {
    if (!within(w, S.origin.xy) || !within(w, S.origin.zw)) {
        return vec4<f32>(0.0);
    }
    return textureLoad(state, wrap(w), 0);
}

// How far a body's legs stand in the water, its surface lifted `lift` by the waves: none with
// its feet out, all from knee deep. Where
// retail lays the water at the ground's own height (a swamp, a pond, a river's flats) the feet
// stand at the level, so they wade as deep as the bed is drawn there, shallower than the sea.
fn wet(at: vec4<f32>, lift: f32) -> f32 {
    let w = water_here(at.xy, 0.0);
    var deep = w.level + lift + 0.05 - at.z;
    if (w.inside > 0.0 && at.z > w.level - 0.03) {
        deep = max(deep, min(0.1 + 0.05 * w.inside, 0.4));
    }
    return clamp(deep / 0.45, 0.0, 1.0);
}

fn sway(k: i32, count: u32) -> f32 {
    let t = f32(count % 600u) / 600.0;
    let one = sin(6.2831853 * (t * SWAYS + f32(k) * 0.37));
    let two = sin(6.2831853 * (t * SWAYS_TOO + f32(k) * 0.71));
    return 1.0 + SWAY * (0.65 * one + 0.35 * two);
}

fn bow(q: vec2<f32>, was: vec4<f32>, at: vec4<f32>, soaked: f32) -> f32 {
    let moved = at.xy - was.xy;
    let len = length(moved);
    if (len < 1e-5) {
        return 0.0;
    }
    let ahead = moved / len;
    let speed = min(len / STEP_SECONDS / RUN, 1.3);
    let front = q - (at.xy + ahead * BOW_OFF);
    let back = q - (at.xy - ahead * BOW_OFF);
    let w2 = BOW_WIDTH * BOW_WIDTH;
    let heap = exp(-dot(front, front) / w2) - exp(-dot(back, back) / w2);
    return BOW_RISE * speed * heap * soaked * at.w;
}

fn pushed(q: vec2<f32>, at: vec4<f32>, soaked: f32) -> f32 {
    let r = length(q - at.xy) / LEGS;
    if (r >= 1.0 || at.w <= 0.0) {
        return 0.0;
    }
    let k = 1.0 - r * r;
    return PUSH * k * k * soaked * at.w;
}

fn rain_hash(c: vec2<i32>, n: u32) -> vec3<f32> {
    var h = bitcast<u32>(c.x) * 0x8DA6B343u ^ bitcast<u32>(c.y) * 0xD8163841u ^ n * 0xCB1AB31Fu;
    h = (h ^ (h >> 15u)) * 0x2C1B3C6Du;
    h = (h ^ (h >> 12u)) * 0x297A2D39u;
    h = h ^ (h >> 15u);
    let h2 = (h ^ 0x9E3779B9u) * 0x85EBCA6Bu;
    let h3 = (h2 ^ (h2 >> 13u)) * 0xC2B2AE35u;
    return vec3<f32>(f32(h & 0xFFFFu), f32(h2 >> 16u), f32(h3 & 0xFFFFu)) / 65535.0;
}

@compute @workgroup_size(16, 16, 1)
fn step(
    @builtin(global_invocation_id) id: vec3<u32>,
    @builtin(workgroup_id) group: vec3<u32>,
    @builtin(local_invocation_index) index: u32,
) {
    let first = S.origin.xy;
    let corner = first + vec2<i32>(group.xy) * GROUP - vec2<i32>(REACH);
    for (var i = i32(index); i < TILE * TILE; i = i + GROUP * GROUP) {
        tile[i] = held(corner + vec2<i32>(i % TILE, i / TILE)).x;
    }
    if (index < min(S.misc.y, 8u)) {
        let lift = S.lift[index / 4u][index % 4u];
        soak[index] = vec2<f32>(wet(S.start[index], lift), wet(S.end[index], lift));
    }
    workgroupBarrier();
    let local = vec2<i32>(id.xy) - vec2<i32>(group.xy) * GROUP + vec2<i32>(REACH);
    let w = first + vec2<i32>(id.xy);
    let texel = wrap(w);
    let rel = vec2<f32>(id.xy);
    let q = S.place.xy + (rel + vec2<f32>(0.5)) * CELL;
    let here = water_here(q, 0.0);
    // On land the water holds its level.
    if (here.inside <= 0.0) {
        textureStore(next, texel, vec4<f32>(0.0));
        return;
    }
    let now = held(w);
    var pull = 0.0;
    for (var dy = -REACH; dy <= REACH; dy = dy + 1) {
        for (var dx = -REACH; dx <= REACH; dx = dx + 1) {
            let k = KERNEL[abs(dy) * (REACH + 1) + abs(dx)];
            pull = pull + k * tile[(local.y + dy) * TILE + local.x + dx];
        }
    }
    // Damped a little, more in the shallows at the very edge, where waves break.
    let a = S.wave.y + (1.0 - smoothstep(0.0, 0.8, here.inside)) * 0.06;
    var h = (now.x * (2.0 - a) - now.y - S.wave.x * pull) / (1.0 + a);
    let count = i32(S.misc.y);
    var shelter = 0.0;
    var waterline = 0.0;
    var churn = 0.0;
    for (var k = 0; k < count; k = k + 1) {
        let now_pushed = pushed(q, S.end[k], soak[k].y) * sway(k, S.misc.x + 1u);
        let shove = bow(q, S.start[k], S.end[k], soak[k].y)
            - (now_pushed - pushed(q, S.start[k], soak[k].x) * sway(k, S.misc.x));
        h = h + shove;
        churn = max(churn, smoothstep(CHURN_FROM, CHURN_FULL, abs(shove)));
        let r = length(q - S.end[k].xy);
        let there = soak[k].y * min(S.end[k].w, 1.0);
        shelter = max(shelter, there * exp(-(r / SHELTER) * (r / SHELTER)));
        let lap = clamp(0.5 + now.x * 30.0, 0.15, 1.5);
        let d = (r - LEGS * 1.1) / (0.04 + 0.04 * lap);
        waterline = max(waterline, there * lap * exp(-d * d));
    }
    if (S.wave.z > 0.0) {
        let home = vec2<i32>(floor(q / RAIN_PATCH));
        for (var j = -1; j <= 1; j = j + 1) {
            for (var i = -1; i <= 1; i = i + 1) {
                let c = home + vec2<i32>(i, j);
                let r = rain_hash(c, S.misc.x);
                if (r.z < S.wave.z) {
                    let at = (vec2<f32>(c) + r.xy) * RAIN_PATCH;
                    let dd = q - at;
                    h = h - S.wave.w * exp(-dot(dd, dd) / (2.0 * DROP * DROP));
                }
            }
        }
    }
    let edge = min(min(rel.x, rel.y), min(f32(SIDE - 1) - rel.x, f32(SIDE - 1) - rel.y));
    let calm = mix(0.85, 1.0, smoothstep(0.0, SPONGE, edge));
    let froth = max(max(waterline, churn), now.w * FROTH_KEEPS);
    textureStore(next, texel, vec4<f32>(clamp(h * calm, -0.3, 0.3), now.x * calm, shelter, froth * calm));
}
