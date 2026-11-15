// The water: retail's water drawn as water over the lit world. Its surface lies at
// retail's level; its waves and ripples only bend the light, and its edge runs up the shore and
// back with the swash. For each pixel of it: what lies under it (the bed, which retail draws at
// the level and which is read a made-up depth down, or anything standing in it, read at its own
// depth) is seen through it, bent and faded into the water's own colour with the depth passed
// through; what it mirrors is found along the reflected ray in the frame, else in the probes and
// the sky; the sun glints off it; foam gathers at its edge, round what stands in it and where the
// simulation laps a wading body's legs. Past the near square the far rings' water is shaded the
// same way over the lit far ground, from the far field's records (`fs_far_water`).

struct WaterLook {
    // rgb: what a metre of the water takes of each colour; w: the bed's deepest, metres.
    absorb: vec4<f32>,
    // rgb: the deep water's own colour (what it scatters back); w: metres in to the deepest.
    scatter: vec4<f32>,
    // x: the waves' size, y: the ripples', z: how fast it flows (metres a second), w: its foam.
    motion: vec4<f32>,
    // x: the surface's roughness; y: how much of the sea's swell it carries; z: how much surf
    // rolls in to its shore; w: how much of its chop is a pond's.
    surface: vec4<f32>,
    // rgb: the open water's own colour, far from the shore; w: how rough it reads far off.
    open: vec4<f32>,
    // Its shore wave: x how much of a lake's chop runs in to the shore, y which (0 none, 1 the
    // sea's swell, 2 a lake's chop), z its wavenumber in deep water, w its height a unit of waves.
    shore: vec4<f32>,
}

// The frame's waves (`water::frame_constants`): each wave's wave vector, steepness and phase at
// render space's origin (the chop, six of swell toward each of eight headings, then a pond's
// slow chop); the surf's share of its period and the clock wrapped to ten minutes; each ripple
// octave's drift, the foam's, then each octave's slower drift on a pond (the rest within a cell,
// metres, then whole cells as i32 bits); how far from the eye the waves have height (x, 0 none);
// the bodies standing near the eye (xyz their feet, w 1); the shore waves' phases at the edge, the
// sea's (x) and a lake's (y).
struct WaterWaves {
    swells: array<vec4<f32>, 56>,
    surf: vec4<f32>,
    drift: array<vec4<f32>, 7>,
    heights: vec4<f32>,
    bodies: array<vec4<f32>, 8>,
    shore: vec4<f32>,
}

@group(1) @binding(0) var scene_colour: texture_2d<f32>;
@group(1) @binding(1) var lin_clamp: sampler;
@group(1) @binding(2) var g_material: texture_2d<f32>;
@group(1) @binding(3) var g_depth: texture_depth_2d;
// The simulated water near the eye, wrapped: r its height now, g a step ago, b how sheltered it
// is by a body standing in it, a its froth round a wading body and in its wake.
@group(1) @binding(4) var ripples_tex: texture_2d<f32>;
@group(1) @binding(5) var lin_repeat: sampler;
@group(1) @binding(6) var<storage, read> looks: array<WaterLook>;
// Each pixel's material: the ground's is the bed.
@group(1) @binding(7) var g_ids: texture_2d<u32>;
@group(1) @binding(8) var<uniform> waves: WaterWaves;
// This frame's patches (water.rs `WaterLayout::select`), and the depth they were drawn into.
@group(1) @binding(9) var<storage, read> water_nodes: array<WaterNode>;
@group(1) @binding(10) var water_depth: texture_depth_2d;
// Each look's shore wave's phase gained running in from each half metre out (`water::shore_table`).
@group(1) @binding(11) var<storage, read> shore_gain: array<f32>;
// What stands over the ground round the eye (rain.wgsl).
@group(3) @binding(10) var shelter_map: texture_depth_2d;

const RIPPLE_CELL: f32 = 0.08;
const RIPPLE_SIDE: f32 = 256.0;

// One patch: its south-west vertex in finest steps from the land field's first vertex, its level
// (its vertices `2^level` steps apart), and where its vertices start and end morphing.
struct WaterNode {
    origin: vec2<i32>,
    level: u32,
    pad: u32,
    morph: vec2<f32>,
    pad2: vec2<f32>,
}

const WATER_PATCH: u32 = 16u;
const WATER_STEP: f32 = 0.75;

struct WaterOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) cur: vec4<f32>,
    @location(2) prev: vec4<f32>,
}

// Where vertex `v` of patch `node` rests on the water for an eye at `eye`: its grid point moved
// toward the next coarser grid's as it nears the end of its level's reach (CDLOD), counted in
// whole finest steps so patches meeting at an edge put its vertices on the same bits (xy); and
// the metres between vertices there, morphing to twice as it goes (z).
fn water_rest(node: WaterNode, v: u32, eye: vec3<f32>) -> vec3<f32> {
    let grid = vec2<f32>(f32(v % (WATER_PATCH + 1u)), f32(v / (WATER_PATCH + 1u)));
    let step = f32(1u << node.level);
    let base = vec2<f32>(node.origin);
    let at = frame.land.xy + (base + grid * step) * WATER_STEP;
    let d = distance(eye, vec3<f32>(at, water_plain(at).y));
    let k = clamp((d - node.morph.x) / (node.morph.y - node.morph.x), 0.0, 1.0);
    let g = grid - fract(grid * 0.5) * 2.0 * k;
    return vec3<f32>(frame.land.xy + (base + g * step) * WATER_STEP, step * WATER_STEP * (1.0 + k));
}

// How much the troughs near a body standing in the water are held at the level, 0..1: below the
// level the bed hides its legs, so a trough there would show water where a shin stands in air.
fn trough_held(p: vec2<f32>) -> f32 {
    var held = 0.0;
    for (var i = 0u; i < 8u; i++) {
        let b = waves.bodies[i];
        if (b.w > 0.0) {
            held = max(held, 1.0 - smoothstep(1.0, 2.0, distance(p, b.xy)));
        }
    }
    return held;
}

// The surface over rest point `xy`, its vertices `band` metres apart: the water's level there and
// the waves' height, those long enough for the band. The heights fade out toward the end of their
// reach from the eye; none with Water Detail at Low.
fn water_surface(xy: vec2<f32>, band: f32) -> vec3<f32> {
    let level = water_plain(xy).y;
    let reach = waves.heights.x;
    if (reach <= 0.0) {
        return vec3<f32>(xy, level);
    }
    let fade = 1.0 - smoothstep(reach * 0.75, reach, distance(frame.camera.xy, xy));
    if (fade <= 0.0) {
        return vec3<f32>(xy, level);
    }
    let w = water_here(xy, band);
    let look = looks[min(w.look, arrayLength(&looks) - 1u)];
    var h = wave_field(xy, look, w, Seen(true, Foot(band, 1.0, vec2<f32>(1.0, 0.0))), false).height
        * fade;
    h -= min(h, 0.0) * trough_held(xy);
    return vec3<f32>(xy, level + h);
}

// The water's patches, drawn into the water's own depth and motion; `fs_water` shades what they
// left, once a pixel. The motion is the surface's as it stands, seen by the last frame's camera:
// the waves' own rise and fall, followed, made the upscaler's history shimmer four times as much
// (their light changes as they pass, which no motion can say).
@vertex
fn vs_water(@builtin(vertex_index) v: u32, @builtin(instance_index) n: u32) -> WaterOut {
    let rest = water_rest(water_nodes[n], v, frame.camera.xyz);
    let p = water_surface(rest.xy, rest.z);
    var out: WaterOut;
    out.clip = frame.view_proj * vec4<f32>(p, 1.0);
    out.world = p;
    out.cur = out.clip;
    out.prev = frame.prev_view_proj * vec4<f32>(p, 1.0);
    return out;
}

// Whether the frame's pixel `ip`, at `q`, is the bed: the near ground within a little of the
// water's level, which the waves' troughs dip under (it neither hides nor is mirrored in them).
// Only in a cell with a water corner: a dry vertex's level is its own height. Up a bank, as high
// as the edge's wander reaches out over it.
fn is_bed(ip: vec2<i32>, q: vec3<f32>) -> bool {
    let bits = u32(textureLoad(g_material, ip, 0).a * 255.0 + 0.5);
    let plain = water_plain(q.xy);
    let up = select(0.0, bank_reach(plain.w), plain.z > -4.0);
    return (bits & G_TERRAIN) != 0u && textureLoad(g_ids, ip, 0).r == frame.lists.z
        && plain.x > -LAND_CELL * 0.5
        && q.z <= plain.y + 0.02 + up + distance(q, frame.camera.xyz) * 1e-4;
}

// Whether the world drawn before the water hides it at its fragment `clip`: anything nearer, but
// for the bed the water lies on (retail draws its water cells as ground at the level).
fn water_hidden(clip: vec4<f32>) -> bool {
    let ip = vec2<i32>(clip.xy);
    let d = textureLoad(g_depth, ip, 0);
    if (d <= clip.z) {
        return false;
    }
    return !is_bed(ip, water_unproject(clip.xy, d));
}

// The water's depth and motion: where it is drawn and not hidden by the world.
@fragment
fn fs_water_depth(in: WaterOut) -> @location(0) vec2<f32> {
    // Well inside the water the edge's wander and the swash cannot reach.
    let plain = water_plain(in.world.xy);
    if (plain.z < 12.0) {
        let foot = pixel_foot(distance(in.world, frame.camera.xyz));
        let w = water_here(in.world.xy, foot);
        if (w.inside + swash_at(in.world.xy, w).x <= 0.0) {
            discard;
        }
    }
    if (water_hidden(in.clip)) {
        discard;
    }
    return motion_of(in.cur, in.prev);
}

fn water_unproject(px: vec2<f32>, d: f32) -> vec3<f32> {
    let ndc = vec2<f32>(px.x / frame.params.z * 2.0 - 1.0, 1.0 - px.y / frame.params.w * 2.0);
    let h = frame.inv_view_proj * vec4<f32>(ndc, d, 1.0);
    return h.xyz / h.w;
}

// Value noise over a lattice `cell` metres wide, laid on the world as `land_noise` is, carried
// along by `drift` (the rest within a cell, metres, then whole cells as i32 bits).
fn drifting_noise(p: vec2<f32>, cell: f32, drift: vec4<f32>) -> f32 {
    let o = origin_cells(cell);
    let q = (p - drift.xy) / cell + o.zw;
    let i = vec2<i32>(floor(q)) + vec2<i32>(o.xy) - bitcast<vec2<i32>>(drift.zw);
    let f = q - floor(q);
    let s = f * f * (3.0 - 2.0 * f);
    return mix(mix(land_hash(i), land_hash(i + vec2<i32>(1, 0)), s.x),
               mix(land_hash(i + vec2<i32>(0, 1)), land_hash(i + vec2<i32>(1, 1)), s.x), s.y);
}

// How much of a pattern repeating every `repeat` metres a pixel covering `foot` metres holds.
fn ripple_resolved(repeat: f32, foot: f32) -> f32 {
    return 1.0 - smoothstep(repeat / 24.0, repeat / 6.0, foot);
}

// What a pixel covers of the water: `across` metres across the view, `stretch` times that along
// it as the view glances off the surface, `along` the view's way over the water.
struct Foot {
    across: f32,
    stretch: f32,
    along: vec2<f32>,
}

// The pixel's foot seen from `dir`, `across` metres across.
fn foot_of(dir: vec3<f32>, across: f32) -> Foot {
    return Foot(across, 1.0 / max(abs(dir.z), 0.02), dir.xy / max(length(dir.xy), 1e-4));
}

// How many metres foot `f` covers along a wave whose wave vector is `k`.
fn foot_along(f: Foot, k: vec2<f32>) -> f32 {
    let a = dot(k / max(length(k), 1e-4), f.along);
    return f.across * sqrt(1.0 + a * a * (f.stretch * f.stretch - 1.0));
}

// How much of wave `w` a pixel covering `f` holds.
fn wave_resolved(w: vec4<f32>, f: Foot) -> f32 {
    return ripple_resolved(6.2831853 / max(length(w.xy), 1e-4), foot_along(f, w.xy));
}

// What a sample of the waves holds: a pixel's foot (`geometry` false), or the patches' vertices
// `foot.across` metres apart, which hold a wave six of them long and none under three.
struct Seen {
    geometry: bool,
    foot: Foot,
}

fn wave_seen(w: vec4<f32>, s: Seen) -> f32 {
    if (s.geometry) {
        let lambda = 6.2831853 / max(length(w.xy), 1e-4);
        return smoothstep(3.0 * s.foot.across, 6.0 * s.foot.across, lambda);
    }
    return wave_resolved(w, s.foot);
}

// The mean of `exp(sin θ − 1)` over a period (I0(1) / e), taken off each wave's height so the
// surface keeps retail's level on the whole.
const WAVE_MEAN: f32 = 0.4657596;

// Wave `w` at `p`, `amount` strong, as `seen` holds it: its slope east and north, how near its
// crest `p` lies (0..1), and its height.
fn one_wave(w: vec4<f32>, p: vec2<f32>, seen: f32, amount: f32) -> vec4<f32> {
    if (seen <= 0.0) {
        return vec4<f32>(0.0);
    }
    let kk = max(length(w.xy), 1e-4);
    let th = dot(w.xy, p) + w.w;
    // Sharper crests than a sine.
    let e = exp(sin(th) - 1.0);
    let a = w.z / kk * amount * seen;
    return vec4<f32>(w.xy * (a * e * cos(th)), e * seen, a * (e - WAVE_MEAN));
}

const HEADINGS: i32 = 8;
// The swell's waves a heading, and where the pond's chop starts.
const PER_HEADING: i32 = 6;
const CALM_FIRST: i32 = 52;

struct Swell {
    slope: vec2<f32>,
    crest: f32,
    lost: f32,
    height: f32,
    // The most the waves held could rise and fall, their amplitudes summed.
    reach: f32,
}

// The waves at `p`, as `seen` holds them: a lake's chop `chop` strong, a pond's `pond` strong,
// and the sea's swell `sea` strong running toward `toward` (the two headings either side of it,
// the nearer the more), its long two `long` of that (the rest run in as shore waves).
fn swell(p: vec2<f32>, seen: Seen, chop: f32, pond: f32, sea: f32, toward: vec2<f32>,
         long: f32) -> Swell {
    var out = Swell(vec2<f32>(0.0), 0.0, 0.0, 0.0, 0.0);
    for (var k = 0; k < 8; k++) {
        let amount = select(chop, pond, k >= 4);
        if (amount <= 0.0) {
            continue;
        }
        let i = select(k, CALM_FIRST - 4 + k, k >= 4);
        let w = waves.swells[i];
        let held = wave_seen(w, seen);
        let one = one_wave(w, p, held, amount);
        out.slope += one.xy;
        out.height += one.w;
        out.reach += w.z / max(length(w.xy), 1e-4) * amount * held;
        out.lost += w.z * amount * (1.0 - held);
    }
    if (sea <= 0.0) {
        return out;
    }
    let turn = atan2(toward.y, toward.x) / (6.2831853 / f32(HEADINGS));
    let f = turn - floor(turn);
    let first = (i32(floor(turn)) % HEADINGS + HEADINGS) % HEADINGS;
    var crests = 0.0;
    var weight = 0.0;
    for (var side = 0; side < 2; side++) {
        // Of equal power across the turn, so the swell is as large between two headings as on one.
        let share = select(cos(f * 1.5707963), sin(f * 1.5707963), side == 1);
        if (share < 0.01) {
            continue;
        }
        let h = (first + side) % HEADINGS;
        for (var j = 0; j < PER_HEADING; j++) {
            let i = 4 + h * PER_HEADING + j;
            let w = waves.swells[i];
            let amount = sea * share * select(1.0, long, j >= 4);
            let held = wave_seen(w, seen);
            let one = one_wave(w, p, held, amount);
            out.slope += one.xy;
            out.height += one.w;
            out.reach += w.z / max(length(w.xy), 1e-4) * amount * held;
            out.lost += w.z * amount * (1.0 - held);
            crests += one.z * w.z * share;
            weight += w.z * share;
        }
    }
    if (weight > 0.0) {
        out.crest = crests / weight;
    }
    return out;
}

// How far inside the water `p` lies, metres, straight between the near field's vertices or the
// far field's.
fn inside_roughly(p: vec2<f32>, far: bool) -> f32 {
    let cell = water_cell(p, far);
    let c = vec2<i32>(cell.xy);
    let f = cell.zw;
    return mix(mix(water_vertex(c, far).inside, water_vertex(c + vec2<i32>(1, 0), far).inside, f.x),
               mix(water_vertex(c + vec2<i32>(0, 1), far).inside,
                   water_vertex(c + vec2<i32>(1, 1), far).inside, f.x), f.y);
}

// Which way the shore lies from `p` on the far rings: down the water's distance from its edge,
// taken a cell either way so it turns smoothly; past that distance's reach, the way the wind blows.
fn toward_shore(p: vec2<f32>, far: bool) -> vec2<f32> {
    let e = LAND_CELL;
    let g = vec2<f32>(
        inside_roughly(p + vec2<f32>(e, 0.0), far) - inside_roughly(p - vec2<f32>(e, 0.0), far),
        inside_roughly(p + vec2<f32>(0.0, e), far) - inside_roughly(p - vec2<f32>(0.0, e), far),
    ) / (2.0 * e);
    return normalize(-g + normalize(waves.swells[0].xy) * 0.03);
}

// Which way the sea's swell runs at `p` (xy) and how much of it is kept (z): toward the shore,
// as the shore field has it, turning to the wind out at sea and where the shores about disagree (a
// strait's middle, an islet's lee), which keep a little less; on the far rings, `toward_shore`.
fn swell_way(p: vec2<f32>, far: bool) -> vec3<f32> {
    let wind = normalize(waves.swells[0].xy);
    if (far) {
        return vec3<f32>(toward_shore(p, true), 1.0);
    }
    let s = shore_at(p);
    let open = smoothstep(200.0, 480.0, s.x);
    let way = s.yz * (1.0 - open) + wind * (0.15 + open);
    let agree = max(smoothstep(0.2, 0.7, length(s.yz)), open);
    return vec3<f32>(normalize(way), mix(0.6, 1.0, agree));
}

// How much a steep bank brings the made-up bed's depth in toward its foot: one on a beach, more
// as the bank rises steeper than the bed falls at the edge, so waves keep their height to a few
// metres from a cliff.
fn bank_scale(look: WaterLook, w: WaterHere) -> f32 {
    return clamp(w.bank * look.scatter.w / max(2.0 * look.absorb.w, 1e-3), 1.0, 10.0);
}

// The mean of `exp(κ (sin θ − 1))` over a period: the modified Bessel function of order nought
// at κ, times `e^−κ`.
fn crest_mean(kappa: f32) -> f32 {
    let q = kappa * kappa * 0.25;
    var term = 1.0;
    var sum = 1.0;
    for (var m = 1; m <= 6; m++) {
        term *= q / f32(m * m);
        sum += term;
    }
    return sum * exp(-kappa);
}

// The phase a look's shore wave gains running in to the edge from `x` metres out over its
// made-up bed (`row` its table).
fn shore_phase_in(row: u32, x: f32) -> f32 {
    let last = f32(SHORE_STEPS - 1u);
    let t = x / SHORE_STEP;
    if (t >= last) {
        return shore_gain[row + SHORE_STEPS - 1u] + shore_gain[row + SHORE_STEPS] * (x - last * SHORE_STEP);
    }
    let j = u32(t);
    return mix(shore_gain[row + j], shore_gain[row + j + 1u], t - f32(j));
}

const SHORE_STEP: f32 = 0.5;
const SHORE_STEPS: u32 = 257u;
// How much of the shore waves' slope the light takes: their moving crests near the eye shimmered
// under the upscaler at full strength; the heights keep it all.
const SHORE_LIGHT: f32 = 0.4;
// Where on a shore wave its breaking foam's front lies, radians (`water::SHORE_FRONT`).
const SHORE_FRONT: f32 = 1.8707963;

// The waves at `p` by water `w`, as `seen` holds them: the surface's slope
// east and north, the swell's crests, the roughness what is not held leaves, the height about the
// level, and the foam on the shore waves' breaking crests. A lake's chop grows with how far the
// water reaches, a pond's short, slow chop in its stead on calm water too narrow for it; the sea's
// swell turns to run in to the shore (`far`: the far field's). Near the shore its long swell, and
// a share of a lake's chop, become shore waves: crests that follow the shore's outline, closing up
// and slowing over the made-up bed, and breaking. Before the shore they all shrink: whole where
// the made-up bed is 3 m down, a few centimetres at half a metre, and never more than 0.39 of the
// depth, so no crest meets the waterline. All run in real seconds: their phases and drifts are
// worked out on the CPU (`water::frame_constants`). The ripples are the pixels' alone.
struct WaveField {
    slope: vec2<f32>,
    crest: f32,
    lost: f32,
    height: f32,
    breaking: f32,
}

fn wave_field(p: vec2<f32>, look: WaterLook, w: WaterHere, seen: Seen, far: bool) -> WaveField {
    let gust = 0.6 + 0.4 * frame.wind.z;
    // How far the wind runs over the water: by the distance from the shore, and near by the
    // water's width (a lake's chop is the size of the lake across).
    var fetch = smoothstep(8.0, 120.0, w.inside);
    var sh = ShoreHere(vec4<f32>(0.0), vec2<f32>(0.0));
    if (!far) {
        sh = shore_sloped(p);
        fetch = max(fetch, smoothstep(30.0, 150.0, sh.at.w) * smoothstep(0.0, 6.0, w.inside));
    }
    // Measured from the drawn edge, and a steep bank's bed falling away nearer its foot.
    let bank = bank_scale(look, w);
    let near = max(w.inside, 0.0) * bank;
    let pond = look.surface.w * (1.0 - fetch);
    let waves_here = look.motion.x * gust * smoothstep(0.3, 4.0, near);
    var chop = waves_here * max(mix(0.3, 1.0, fetch), look.surface.y) * (1.0 - pond);
    var sea = look.motion.x * look.surface.y * gust;
    var toward = normalize(waves.swells[0].xy);
    if (sea > 0.0) {
        let way = swell_way(p, far);
        toward = way.xy;
        sea *= way.z;
    }
    let deep = bed_depth(near, look.absorb.w, look.scatter.w);
    // The shore wave: its height before the shore shrinks it, how far in it is, and the share of
    // the sea's long swell left running straight.
    var a0 = 0.0;
    var d = 0.0;
    var long = 1.0;
    if (!far && look.shore.y > 0.0) {
        // Near the edge, the distance to the edge as drawn, wandering.
        d = max(sh.at.x + w.wander * (1.0 - smoothstep(4.0, 16.0, sh.at.x)) - w.shift, 0.0);
        let zone = min(look.scatter.w + 1.5 * 6.2831853 / look.shore.z, 120.0);
        let t = clamp((d - 0.5 * zone) / (0.5 * zone), 0.0, 1.0);
        // Of equal power across the blend, and only where the shores agree on a way.
        let share = cos(t * 1.5707963) * smoothstep(0.3, 0.7, length(sh.at.yz));
        if (look.shore.y < 1.5) {
            a0 = sea * look.shore.w * share;
            long = sqrt(1.0 - share * share);
        } else {
            let turned = look.shore.x * share;
            a0 = look.motion.x * gust * smoothstep(30.0, 150.0, sh.at.w) * look.shore.w * turned;
            chop *= sqrt(1.0 - turned * turned);
        }
        // A channel narrower than a couple of its waves keeps them small.
        a0 = min(a0, 0.05 * sh.at.w);
    }
    let sw = swell(p, seen, chop, waves_here * pond, sea * smoothstep(1.0, 10.0, near), toward, long);
    var out = WaveField(sw.slope, sw.crest, sw.lost, sw.height, 0.0);
    var reach = sw.reach;
    if (a0 > 0.0) {
        let row = min(w.look, arrayLength(&looks) - 1u) * (SHORE_STEPS + 1u);
        let k0 = look.shore.z;
        // Its wavenumber over the bed its table was made for (Fenton and McKee's), closing up as
        // the bed rises.
        let under = bed_depth(d, look.absorb.w, look.scatter.w);
        let k = k0 / pow(tanh(pow(k0 * max(under, 0.05), 0.75)), 0.6666667);
        let n = shore_wander(p);
        let clock = select(waves.shore.y, waves.shore.x, look.shore.y < 1.5);
        let th = -shore_phase_in(row, d) + clock + 6.2831853 * n.x;
        // How hard it breaks: its height over the depth, its crest sharper as it steepens.
        let ratio = a0 / max(deep, 0.05);
        let kappa = 1.0 + 0.5 * smoothstep(0.1, 0.35, ratio);
        var grad = vec2<f32>(0.0);
        var held = 1.0;
        if (seen.geometry) {
            held = smoothstep(3.0 * seen.foot.across, 6.0 * seen.foot.across, 6.2831853 / k);
        } else {
            var slope = sh.slope;
            if (sh.at.x < 16.0) {
                slope += edge_wander(p, seen.foot.across).yz * (1.0 - smoothstep(4.0, 16.0, sh.at.x));
            }
            grad = -k * slope + 6.2831853 * n.yz;
            held = ripple_resolved(6.2831853 / k, foot_along(seen.foot, grad));
            out.lost += k * a0 * (1.0 - held);
            let lead = fract((SHORE_FRONT - th) / 6.2831853);
            // A front and the white water trailing behind it.
            out.breaking = smoothstep(0.0, 0.07, lead) * (1.0 - smoothstep(0.07, 0.3, lead))
                * smoothstep(0.25, 0.45, ratio) * smoothstep(0.01, 0.04, a0)
                * shore_resolved(3.1415927 / k, seen.foot.across);
        }
        let e = exp(kappa * (sin(th) - 1.0));
        let a = a0 * held;
        out.height += a * (e - crest_mean(kappa));
        // The light takes a rounder crest than the height's, and less of it.
        out.slope += grad * (a * exp(sin(th) - 1.0) * cos(th) * SHORE_LIGHT);
        reach += a;
    }
    let shrink = min(smoothstep(0.3, 3.0, deep), 0.39 * deep / max(reach, 1e-3));
    out.slope *= shrink;
    out.height *= shrink;
    if (seen.geometry) {
        return out;
    }
    let foot = seen.foot;
    var s = out.slope;
    var lost = out.lost;
    // Ripples: three drifting octaves of noise, the wind's fine chop; a river's carried along by
    // its flow besides, two copies half a cycle apart crossfaded so the flow never jumps.
    let r = look.motion.y * (0.35 + 0.65 * frame.wind.z);
    let scales = array<f32, 3>(1.1, 0.42, 0.17);
    let wind = normalize(waves.swells[0].xy);
    let cycle = fract(waves.surf.y / 4.0);
    let flows = array<f32, 2>(cycle, fract(cycle + 0.5));
    // Every way at once, so between the foot across and along.
    let spot = foot.across * sqrt(foot.stretch);
    for (var k = 0u; k < 3u; k++) {
        let c = scales[k];
        let keep = 1.0 - smoothstep(c * 0.25, c * 0.6, spot);
        if (keep > 0.0) {
            let e = c * 0.25;
            var g = vec2<f32>(0.0);
            let copies = select(1u, 2u, look.motion.z > 0.0);
            for (var j = 0u; j < copies; j++) {
                var q = p + vec2<f32>(f32(k) * 17.3, f32(j) * 7.1);
                var wt = 1.0;
                if (copies == 2u) {
                    q -= wind * look.motion.z * 4.0 * (flows[j] - 0.5);
                    wt = 1.0 - abs(2.0 * flows[j] - 1.0);
                }
                // A pond's drift slower, faded across where a lake's reach brings the wind's.
                for (var m = 0u; m < 2u; m++) {
                    let share = select(1.0 - pond, pond, m == 1u);
                    if (share <= 0.0) {
                        continue;
                    }
                    let d = waves.drift[select(k, 4u + k, m == 1u)];
                    let gx = drifting_noise(q + vec2<f32>(e, 0.0), c, d) - drifting_noise(q - vec2<f32>(e, 0.0), c, d);
                    let gy = drifting_noise(q + vec2<f32>(0.0, e), c, d) - drifting_noise(q - vec2<f32>(0.0, e), c, d);
                    g += vec2<f32>(gx, gy) * wt * share;
                }
            }
            s += g / (2.0 * e) * c * 0.11 * r * keep;
        }
        lost += 0.06 * r * (1.0 - keep);
    }
    out.slope = s;
    out.lost = lost;
    return out;
}

// The simulated water at `p`: its slope and its shelter and waterline, where the square holds it.
fn ripple_at(p: vec2<f32>) -> vec4<f32> {
    if (frame.ripples.w <= 0.0) {
        return vec4<f32>(0.0);
    }
    let rel = (p - frame.ripples.xy) / frame.ripples.z;
    if (any(rel <= vec2<f32>(0.02)) || any(rel >= vec2<f32>(0.98))) {
        return vec4<f32>(0.0);
    }
    let uv = p / (RIPPLE_CELL * RIPPLE_SIDE);
    let e = 1.0 / RIPPLE_SIDE;
    let here = textureSampleLevel(ripples_tex, lin_repeat, uv, 0.0);
    let hx = textureSampleLevel(ripples_tex, lin_repeat, uv + vec2<f32>(e, 0.0), 0.0).r
        - textureSampleLevel(ripples_tex, lin_repeat, uv - vec2<f32>(e, 0.0), 0.0).r;
    let hy = textureSampleLevel(ripples_tex, lin_repeat, uv + vec2<f32>(0.0, e), 0.0).r
        - textureSampleLevel(ripples_tex, lin_repeat, uv - vec2<f32>(0.0, e), 0.0).r;
    let edge = min(min(rel.x, rel.y), min(1.0 - rel.x, 1.0 - rel.y));
    let fade = smoothstep(0.02, 0.12, edge);
    return vec4<f32>(vec2<f32>(hx, hy) / (2.0 * RIPPLE_CELL) * fade, here.b * fade, here.a * fade);
}

// The light falling on the water's level surface: the sun's (where it reaches) and the sky's.
fn water_light(sun_vis: f32) -> vec3<f32> {
    if (has(F_PHYSICAL)) {
        return frame.sun_light.rgb * max(frame.sun_dir.z, 0.0) * sun_vis + frame.sky_light.rgb;
    }
    return pow(min(frame.sun.rgb * max(frame.sun_dir.z, 0.0) * sun_vis + frame.ambient.rgb,
                   vec3<f32>(1.0)), vec3<f32>(2.2));
}

// Where a world point lands on the target, in pixels, and its view depth (w); none behind.
fn to_screen(p: vec3<f32>) -> vec4<f32> {
    let c = frame.view_proj * vec4<f32>(p, 1.0);
    if (c.w <= 1e-3) {
        return vec4<f32>(-1.0);
    }
    let ndc = c.xyz / c.w;
    return vec4<f32>((ndc.x * 0.5 + 0.5) * frame.params.z, (0.5 - ndc.y * 0.5) * frame.params.w,
                     c.z, c.w);
}

// The sun's light reaching the water at `p` (`depth` metres along the view): one compared tap of
// its cascade, enough under the ripples.
fn water_sun(p: vec3<f32>, depth: f32) -> f32 {
    if (depth >= frame.cascade_far.w) {
        return 1.0;
    }
    var c = 0u;
    for (var k = 0u; k < 3u; k++) {
        if (depth >= frame.cascade_far[k]) {
            c = k + 1u;
        }
    }
    let texel = frame.cascade_texel[c];
    let s = in_cascade(c, p + vec3<f32>(0.0, 0.0, texel * 2.0) + frame.sun_dir.xyz * texel);
    let lo = quarter(c);
    if (any(s.xy <= lo) || any(s.xy >= lo + vec2<f32>(0.5)) || s.z >= 1.0) {
        return 1.0;
    }
    return textureSampleCompareLevel(shadow_atlas, shadow_cmp, s.xy, s.z - texel / frame.cascade_depth[c]);
}

// The frame's colour along the reflected ray `r` from `p`, found by marching through the depth
// buffer (rgb, and how far it is to be believed in a). Only what stands above `p` is found: far
// off, a pixel of the bed below spans so much depth that the ray would find it.
fn screen_reflection(p: vec3<f32>, r: vec3<f32>, px: vec2<f32>, rough: f32, water: bool) -> vec4<f32> {
    var step = 0.5;
    var t = 0.25;
    var prev_t = 0.0;
    for (var k = 0u; k < 18u; k++) {
        let q = p + r * t;
        let s = to_screen(q);
        if (s.w <= 0.0 || s.x < 0.0 || s.y < 0.0 || s.x >= frame.params.z || s.y >= frame.params.w) {
            break;
        }
        let d = textureLoad(g_depth, vec2<i32>(s.xy), 0);
        if (d > 0.0) {
            let scene = s.z / d;
            let behind = s.w - scene;
            let at = water_unproject(floor(s.xy) + 0.5, d);
            if (behind > 0.0 && behind < max(step * 2.5, 0.6) && at.z > p.z + 0.05
                && !(water && is_bed(vec2<i32>(s.xy), at))) {
                // Narrow it down between the last step and this one.
                var lo = prev_t;
                var hi = t;
                for (var b = 0u; b < 3u; b++) {
                    let mid = (lo + hi) * 0.5;
                    let sm = to_screen(p + r * mid);
                    let dm = textureLoad(g_depth, vec2<i32>(clamp(sm.xy, vec2<f32>(0.0), frame.params.zw - 1.0)), 0);
                    if (dm > 0.0 && sm.w - sm.z / dm > 0.0) {
                        hi = mid;
                    } else {
                        lo = mid;
                    }
                }
                let hit = to_screen(p + r * hi);
                let uv = hit.xy / frame.params.zw;
                let edge = min(min(uv.x, uv.y), min(1.0 - uv.x, 1.0 - uv.y));
                // Blurred by the roughness, more the farther the ray ran.
                let spread = rough * 0.05 * hi / (hi + 4.0);
                var c = textureSampleLevel(scene_colour, lin_clamp, uv, 0.0).rgb * 0.4;
                c += textureSampleLevel(scene_colour, lin_clamp, uv + vec2<f32>(spread, 0.0), 0.0).rgb * 0.15;
                c += textureSampleLevel(scene_colour, lin_clamp, uv - vec2<f32>(spread, 0.0), 0.0).rgb * 0.15;
                c += textureSampleLevel(scene_colour, lin_clamp, uv + vec2<f32>(0.0, spread * 2.0), 0.0).rgb * 0.15;
                c += textureSampleLevel(scene_colour, lin_clamp, uv - vec2<f32>(0.0, spread * 2.0), 0.0).rgb * 0.15;
                return vec4<f32>(c, smoothstep(0.0, 0.08, edge) * (1.0 - f32(k) / 18.0 * 0.3));
            }
        }
        prev_t = t;
        t += step;
        step *= 1.4;
    }
    return vec4<f32>(0.0);
}

// How far `p` lies from the nearest edge between the cells of a lattice `cell` metres a mesh, in
// meshes: each mesh's point wanders with the clock (wrapped to ten minutes), a whole number of
// times over it, `wander` of a mesh at most.
fn cell_edge(p: vec2<f32>, cell: f32, wander: f32, salt: i32) -> f32 {
    let o = origin_cells(cell);
    let q = p / cell + o.zw;
    let i = vec2<i32>(floor(q)) + vec2<i32>(o.xy);
    let f = q - floor(q);
    let clock = waves.surf.y;
    var f1 = 9.0;
    var f2 = 9.0;
    for (var j = -1; j <= 1; j++) {
        for (var k = -1; k <= 1; k++) {
            let c = i + vec2<i32>(k, j);
            let h = vec2<f32>(land_hash(c + vec2<i32>(salt, 0)), land_hash(c + vec2<i32>(7, salt)));
            let cycles = floor(vec2<f32>(90.0, 70.0) + h * vec2<f32>(60.0, 70.0));
            let at = vec2<f32>(0.5) + wander * sin(6.2831853 * (clock / 600.0 * cycles + h));
            let d = length(vec2<f32>(f32(k), f32(j)) + at - f);
            if (d < f1) {
                f2 = f1;
                f1 = d;
            } else if (d < f2) {
                f2 = d;
            }
        }
    }
    return f2 - f1;
}

// How much of a lattice `cell` metres wide a pixel covering `foot` metres holds.
fn shore_resolved(cell: f32, foot: f32) -> f32 {
    return 1.0 - smoothstep(0.25 * cell, 0.5 * cell, foot);
}

// The foam's look at `p`, as a pixel covering `foot` metres sees it, sloshing a little with the
// clock: 0..1, highest in its patches and along the walls between its bubbles (render-hifi's).
fn foam_texture(p: vec2<f32>, foot: f32) -> f32 {
    let clock = waves.surf.y;
    let slosh = vec2<f32>(sin(clock * 6.2831853 / 30.0), cos(clock * 6.2831853 / 20.0)) * 0.25;
    let patches = land_noise(p + slosh, 0.5) * 0.6 + land_noise(p + vec2<f32>(3.7, 41.2), 1.25) * 0.4;
    let large = 1.0 - smoothstep(0.0, 0.4 + foot / 0.1, cell_edge(p - slosh * 0.5, 0.1, 0.4, 41));
    let small = 1.0 - smoothstep(0.0, 0.45 + foot / 0.05, cell_edge(p + slosh * 0.3, 0.05, 0.4, 53));
    let which = land_noise(p + vec2<f32>(11.0, 2.0), 0.4);
    // Each lattice's walls fade to their mean once its cells are under about four pixels.
    let walls = mix(mix(0.25, large, 1.0 - smoothstep(0.025, 0.05, foot)),
                    mix(0.25, small, 1.0 - smoothstep(0.0125, 0.025, foot)),
                    smoothstep(0.35, 0.65, which))
        * smoothstep(0.25, 0.6, land_noise(p - slosh + vec2<f32>(5.0, 9.0), 0.25));
    let fine = land_noise(p - slosh + vec2<f32>(17.3, 5.1), 0.2);
    let seen = shore_resolved(0.2, foot);
    return patches * 0.6 + mix(0.25, walls, seen) * 0.08 + mix(0.5, fine, seen) * 0.32;
}

// How much of a pixel whose foam texture is `v` is foam, `q` of the water foaming there: nought at
// nought, a lace of bubbles' walls as it thickens, nearly solid at one.
fn foam_cover(v: f32, q: f32) -> f32 {
    let at = 0.78 - 0.5 * clamp(q, 0.0, 1.0);
    return smoothstep(at, at + 0.24, v) * min(q * 3.0, 1.0);
}

// How much of the water foams `edge` metres inside its swashing edge (`running` how much the
// swash runs up), as a pixel covering `foot` metres sees it: a rim along the edge, thicker as it
// runs up, and the caps of the sea's highest crests (`crest`).
fn foam_share(p: vec2<f32>, edge: f32, running: f32, foot: f32, look: WaterLook, crest: f32) -> f32 {
    let seaish = look.surface.z;
    let shore = 1.0 - smoothstep(0.12, 0.45, foot);
    let reach = (0.35 + 0.8 * seaish) * (0.6 + 0.8 * land_noise(p + vec2<f32>(9.1, 2.3), 6.0))
        * (0.7 + 0.6 * running);
    let rim = (1.0 - smoothstep(0.0, max(reach, foot * 1.5), edge)) * (0.8 + 0.3 * running);
    let caps = smoothstep(0.8, 0.97, crest) * look.surface.y * 0.6;
    return look.motion.w * (shore * rim * 0.75 + caps * 0.8);
}

// How much of the water a shore wave's breaking crest (`breaking`) whitens at `p`: broad, soft
// patches rather than the lace, whose cells popped in and out under the upscaler as each front
// swept over them.
fn breaking_cover(p: vec2<f32>, breaking: f32, look: WaterLook) -> f32 {
    let q = breaking * look.surface.z * look.motion.w * 0.8;
    if (q <= 0.0) {
        return 0.0;
    }
    let clock = waves.surf.y;
    let slosh = vec2<f32>(sin(clock * 6.2831853 / 30.0), cos(clock * 6.2831853 / 20.0)) * 0.25;
    let v = land_noise(p + slosh, 0.5) * 0.35 + land_noise(p + vec2<f32>(3.7, 41.2), 1.25) * 0.45
        + land_noise(p - slosh + vec2<f32>(17.3, 5.1), 0.3) * 0.2;
    let at = 0.75 - 0.55 * min(q, 1.0);
    // Mottled through: thinner and thicker over a few tenths of a metre, never a thin wall.
    let body = 0.55 + 0.45 * land_noise(p + slosh * 0.5 + vec2<f32>(5.0, 9.0), 0.16);
    return smoothstep(at - 0.1, at + 0.25, v) * min(q * 2.5, 1.0) * body;
}

// The water's colour and how much the upscaler should trust the last frame's picture of it (the
// reactive mask).
struct WaterFrag {
    @location(0) colour: vec4<f32>,
    @location(1) reactive: vec4<f32>,
}

// What `water_colour` added of the sun's glint (pre-exposed) and how much foam it laid.
var<private> water_glint: vec3<f32>;
var<private> water_foam: f32;

// The near water, over the whole target: where the patches left their depth, the surface there,
// shaded once.
@fragment
fn fs_water(in: FsIn) -> WaterFrag {
    let ip = vec2<i32>(in.pos.xy);
    let d = textureLoad(water_depth, ip, 0);
    if (d <= 0.0) {
        discard;
    }
    let world = water_unproject(in.pos.xy, d);
    let foot = pixel_foot(distance(world, frame.camera.xyz));
    let w = water_here(world.xy, foot);
    // The edge as the swash has moved it (the depth pass left nothing outside it).
    let sw = swash_at(world.xy, w);
    let edge = max(w.inside + sw.x, 0.0);
    let c = water_colour(in.pos.xy, world, w, edge, sw.y, false);
    // Thin at the very edge, the bed showing through as the lighting lets its mirror go.
    let cover = water_cover(edge, foot);
    let under = textureLoad(scene_colour, ip, 0).rgb;
    var out: WaterFrag;
    out.colour = vec4<f32>(min(mix(under, c, cover), vec3<f32>(HDR_MOST)), 1.0);
    // What moves over the surface that its motion cannot say: the glint racing along the crests
    // and the foam rolling in. Nothing else, so the ripples keep their history (a little over all
    // the water doubled its shimmer at render scale 50).
    let glint = smoothstep(0.02, 0.3, luma(water_glint));
    out.reactive = vec4<f32>(max(glint, water_foam * 0.3) * cover);
    return out;
}

// The far rings' water: where the lit far ground lies under the far field's water, the water's
// level met along the view and shaded as the near water is, laid over the ground as far as it
// covers it.
@fragment
fn fs_far_water(in: FsIn) -> WaterFrag {
    let px = in.pos.xy;
    let ip = vec2<i32>(px);
    let d = textureLoad(g_depth, ip, 0);
    if (d <= 0.0) {
        discard;
    }
    let bits = u32(textureLoad(g_material, ip, 0).a * 255.0 + 0.5);
    if ((bits & G_TERRAIN) == 0u || textureLoad(g_ids, ip, 0).r != far_rings.material) {
        discard;
    }
    let ground = water_unproject(px, d);
    if (all(ground.xy > far_rings.near_min) && all(ground.xy < far_rings.near_max)) {
        discard;
    }
    let eye = frame.camera.xyz;
    let reach = distance(ground, eye);
    let dir = (ground - eye) / max(reach, 1e-4);
    let w = far_water_here(ground.xy, pixel_foot(reach));
    let sw = swash_at(ground.xy, w);
    let edge = w.inside + sw.x;
    // The level along the view, hidden where the ground stands nearer (as the near water's depth
    // test has it, a hair nearer).
    let t = (w.level - eye.z) / min(dir.z, -1e-6);
    if (edge <= 0.0 || dir.z >= 0.0 || t <= 0.0 || t > reach * 1.0002 + 0.01) {
        discard;
    }
    let p = eye + dir * t;
    let c = water_colour(px, p, w, edge, sw.y, true);
    let cover = water_cover(edge, pixel_foot(t));
    let under = textureLoad(scene_colour, ip, 0).rgb;
    var out: WaterFrag;
    out.colour = vec4<f32>(min(mix(under, c, cover), vec3<f32>(HDR_MOST)), 1.0);
    return out;
}

// The water's colour at `p` on its surface under pixel `px` (`w` the water there, `edge` metres
// inside its swashing edge, `running` how much the swash runs up; `far`: on the far rings), seen
// through the air, before it is laid over what lies under it.
fn water_colour(px: vec2<f32>, p: vec3<f32>, w: WaterHere, edge: f32, running: f32,
                far: bool) -> vec3<f32> {
    let ip = vec2<i32>(px);
    let eye = frame.camera.xyz;
    let to = p - eye;
    let dist = length(to);
    let dir = to / max(dist, 1e-4);
    let foot = pixel_foot(dist);
    // What the frame drew here: something above the water and nearer hides it.
    let d = textureLoad(g_depth, ip, 0);
    var scene_world = p - vec3<f32>(0.0, 0.0, 1.0);
    var has_scene = false;
    if (d > 0.0) {
        scene_world = water_unproject(px, d);
        has_scene = true;
    }
    let look = looks[min(w.look, arrayLength(&looks) - 1u)];
    // The surface's bend: waves, ripples and the simulation near the eye.
    let wave = wave_field(p.xy, look, w, Seen(false, foot_of(dir, foot)), far);
    let rip = ripple_at(p.xy);
    let calm = 1.0 - rip.z * 0.8;
    let slope = wave.slope * calm + rip.xy;
    let n = normalize(vec3<f32>(-slope, 1.0));
    let v = -dir;
    // Far water stays rough rather than a glassy mirror: what its pixels cannot hold scatters.
    let rough = clamp(look.surface.x + wave.lost * 0.6 + smoothstep(15.0, 250.0, dist) * look.open.w,
                      0.02, 0.8);

    let depth_view = view_depth_at(p);
    var sun_vis = 1.0;
    if (has(F_SHADOWS)) {
        sun_vis = water_sun(p, depth_view);
    }
    let pre = pre_exposure();
    let light_in = water_light(sun_vis);

    // What lies under it: the bed read a made-up depth down, or anything standing in the water at
    // its own depth.
    let bits = u32(textureLoad(g_material, ip, 0).a * 255.0 + 0.5);
    let terrain = (bits & G_TERRAIN) != 0u && textureLoad(g_ids, ip, 0).r == frame.lists.z;
    let bed = bed_depth(max(edge, 0.0) * bank_scale(look, w), look.absorb.w, look.scatter.w);
    var below = max(p.z - scene_world.z, 0.0);
    if (terrain || !has_scene) {
        below = max(below, bed);
    }
    let tdir = refract(dir, n, 1.0 / 1.33);
    let down = max(-tdir.z, 0.08);
    let path = below / down;
    // The bent ray's foot, where its picture is read: on the bed (drawn at the level) or on what
    // stands in the water.
    var q = p + tdir * path;
    if (terrain) {
        q.z = scene_world.z;
    }
    var uv = px / frame.params.zw;
    let s = to_screen(q);
    if (s.w > 0.0) {
        let bent = s.xy / frame.params.zw;
        let off = clamp(bent - uv, vec2<f32>(-0.08), vec2<f32>(0.08));
        let bq = vec2<i32>(clamp((uv + off) * frame.params.zw, vec2<f32>(0.0), frame.params.zw - 1.0));
        let bd = textureLoad(g_depth, bq, 0);
        // Not where something above the water stands nearer than the surface.
        if (bd > 0.0 && distance(water_unproject(vec2<f32>(bq) + 0.5, bd), eye) >= dist - 0.05) {
            uv += off;
        }
    }
    let seen = textureSampleLevel(scene_colour, lin_clamp, uv, 0.0).rgb;
    let through = exp(-look.absorb.rgb * (path + below));
    // Offshore, the coast's tint gives way to the open water's.
    let shelf = look.scatter.w;
    let own = mix(look.scatter.rgb, look.open.rgb, smoothstep(2.0 * shelf, 4.0 * shelf, w.inside));
    let deep = own * light_in * pre;
    let refracted = seen * through + deep * (1.0 - through);

    // The spread of the facets' slopes the pixel holds (an equivalent Beckmann's).
    let spread = rough * rough * 0.71;
    // What it mirrors: the frame along the reflected ray, else the probes and the sky; across
    // the spread, the facets tipped toward the eye are the ones seen, mirroring higher.
    var r = reflect(dir, n);
    r.z = abs(r.z);
    r = normalize(vec3<f32>(r.xy, r.z + spread * 1.5));
    var sky = vec3<f32>(0.0);
    if (frame.switches.x != 0u) {
        if (has(F_ATMOSPHERE)) {
            sky = sky_radiance(r);
            // The deck blurred over the spread, so its mirror at a glance does not alias.
            let cl = clouds(r, max(pixel_foot(1.0), spread));
            sky = mix(sky, cl.rgb, cl.a);
        } else {
            sky = sky_colour(r);
        }
    }
    if (has(F_GI)) {
        let pl = probe_light(p + vec3<f32>(0.0, 0.0, 0.5), r);
        sky = mix(sky, pl.rgb, pl.a * 0.5);
    }
    var mirrored = sky * pre;
    let cos_v = max(dot(n, v), 0.0);
    // The Fresnel averaged over those facets (Bruneton et al. 2010): a rough surface's facets hide
    // one another at a glance, so its mirror never goes whole.
    let fresnel = 0.02 + 0.98 * pow(1.0 - cos_v, 5.0 * exp(-2.69 * spread))
        / (1.0 + 22.7 * pow(spread, 1.5));
    // Looking steeply down, the mirror is too faint to march for; far off, what it would find is
    // too small to tell.
    let marched = 1.0 - smoothstep(500.0, 800.0, dist);
    if (fresnel > 0.04 && marched > 0.0) {
        let ssr = screen_reflection(p + n * 0.02, r, px, rough, true);
        mirrored = mix(mirrored, ssr.rgb, ssr.a * marched);
    }
    let mirror = mirrored;
    var c = mix(refracted, mirror, fresnel);

    // The sun's glint, its lobe no wider than at half rough, so its path runs to the horizon.
    let ndl = dot(n, frame.sun_dir.xyz);
    if (ndl > 0.0) {
        var sun = frame.sun_light.rgb;
        if (!has(F_PHYSICAL)) {
            sun = pow(frame.sun.rgb, vec3<f32>(2.2));
        }
        let lobe = ggx(n, v, frame.sun_dir.xyz, min(rough, 0.5), vec3<f32>(0.02));
        water_glint = lobe * ndl * sun * sun_vis * pre;
        c += water_glint;
    }

    // Foam: a lace along the edge and the surf's lines rolling in, the caps of the sea's highest
    // crests, round what stands in the water, and where the simulation laps a body's legs.
    var q_foam = foam_share(p.xy, max(edge, 0.0), running, foot, look, wave.crest);
    if (has_scene && !terrain) {
        let wet = 1.0 - smoothstep(0.0, 0.3, p.z - scene_world.z);
        // A wading body froths even calm water, if less than the sea.
        q_foam = max(q_foam, wet * max(look.motion.w, 0.5 * rip.z) * 0.6);
    }
    // Where the simulation froths round a wading body's legs and in its wake: lace and flecks,
    // never a whole ring, calm water's less than the sea's.
    let wade = min(rip.w, 1.0) * mix(0.6, 0.45, look.surface.w);
    var foam = 0.0;
    if (q_foam > 0.0 || wade > 0.0) {
        let lace = foam_texture(p.xy, foot);
        foam = foam_cover(lace, q_foam) * smoothstep(0.0, 0.2, edge);
        let fd = waves.drift[3];
        let fleck = drifting_noise(p.xy, 0.09, fd) * 0.55 + drifting_noise(p.xy + vec2<f32>(5.3, 1.7), 0.23, fd) * 0.45;
        foam = max(foam, foam_cover(lace, wade) * mix(0.4, 1.0, smoothstep(0.4, 0.7, fleck)));
    }
    foam = max(foam, breaking_cover(p.xy, wave.breaking, look) * smoothstep(0.0, 0.2, edge));
    foam = clamp(foam * (1.0 - smoothstep(200.0, 600.0, dist) * 0.7), 0.0, 1.0);
    let white = light_in * 0.75 * pre;
    var froth = mix(white, vec3<f32>(dot(white, vec3<f32>(0.2126, 0.7152, 0.0722))), 0.6);
    // Murky water's froth is dun and dim, as stained as the water.
    froth = mix(froth, froth * vec3<f32>(0.5, 0.47, 0.33), smoothstep(0.1, 0.35, look.absorb.g));
    c = mix(c, froth, foam * 0.45);
    water_foam = foam;

    if (has(F_ATMOSPHERE)) {
        c = aerial(c / max(pre, 1e-30), p, dir) * pre;
    } else {
        c = mix(c, sky_colour(dir) * pre, fog_amount(distance(p.xy, eye.xy)));
    }
    return c;
}


// The rain's puddles (rain.wgsl), the lighting's dark, level water laid on the ground (how much
// of a pixel it covers rides over one in the lit picture's alpha): over each, what the frame shows
// along its reflected ray (the water's march), else the sky that way as the frame drew it, else the
// heavens and the probes; under an eave, the shade beneath it. Laid over as far as it mirrors.
@fragment
fn fs_puddles(in: FsIn) -> @location(0) vec4<f32> {
    let px = in.pos.xy;
    let ip = vec2<i32>(px);
    let puddle = textureLoad(scene_colour, ip, 0).a - 1.0;
    if (puddle <= 0.002) {
        discard;
    }
    let p = water_unproject(px, textureLoad(g_depth, ip, 0));
    let eye = frame.camera.xyz;
    let dist = distance(p, eye);
    let v = (eye - p) / max(dist, 1e-4);
    let n = puddle_normal(p.xy, puddle_rings(dist));
    var r = reflect(-v, n);
    r.z = abs(r.z);
    let pre = pre_exposure();
    var mirrored = vec3<f32>(0.0);
    if (frame.switches.x != 0u) {
        if (has(F_ATMOSPHERE)) {
            mirrored = sky_radiance(r);
            let cl = clouds(r, 0.003);
            mirrored = mix(mirrored, cl.rgb, cl.a);
        } else {
            mirrored = sky_colour(r);
        }
    }
    if (has(F_GI)) {
        let pl = probe_light(p + vec3<f32>(0.0, 0.0, 0.5), r);
        mirrored = mix(mirrored, pl.rgb, pl.a * 0.5);
    }
    mirrored *= pre;
    // The sky that way as the frame drew it, its clouds and all, where it shows on screen.
    let far = to_screen(eye + r * 1.0e4);
    if (frame.switches.x != 0u && far.w > 0.0 && all(far.xy >= vec2<f32>(0.0))
        && all(far.xy < frame.params.zw)) {
        if (textureLoad(g_depth, vec2<i32>(far.xy), 0) <= 0.0) {
            mirrored = textureSampleLevel(scene_colour, lin_clamp, far.xy / frame.params.zw, 0.0).rgb;
        }
    }
    // What stands near along the ray.
    if (dist < 40.0) {
        let ssr = screen_reflection(p + n * 0.02, r, px, 0.08, false);
        mirrored = mix(mirrored, ssr.rgb, ssr.a * (1.0 - smoothstep(25.0, 40.0, dist)));
    }
    mirrored *= mix(1.0, 0.35, roofed_ray(p, r));
    if (has(F_ATMOSPHERE)) {
        mirrored = aerial(mirrored / max(pre, 1e-30), p, -v) * pre;
    } else {
        mirrored = mix(mirrored, sky_colour(-v) * pre, fog_amount(distance(p.xy, eye.xy)));
    }
    return vec4<f32>(min(mirrored * 0.85, vec3<f32>(HDR_MOST)), puddle * puddle_mirror(n, v, dist));
}
