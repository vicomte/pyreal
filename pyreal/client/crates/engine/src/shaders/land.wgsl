// The land field (land.rs): one texel per landscape vertex of the loaded area, east then north:
// x the height, y how far inside the water (metres, negative on land), z the water's level (all
// float bits), w the record word.

@group(0) @binding(6) var land_field: texture_2d<u32>;

// The far terrain's parameters (far.rs): where its field's first texel lies, the texel and the
// field's size, the near square the exact cells draw, the ground's material, Landscape
// Smoothing, and its water records' side (0: none).
struct FarParams {
    origin: vec2<f32>,
    texel: f32,
    size: f32,
    near_min: vec2<f32>,
    near_max: vec2<f32>,
    material: u32,
    smoothed: u32,
    water_side: u32,
    pad: u32,
}

// The far terrain's water: one record per vertex of its field, as the land field's.
@group(0) @binding(7) var far_land: texture_2d<u32>;
@group(0) @binding(8) var<uniform> far_rings: FarParams;

// The land field's shore field (shore.rs), a texel every 3 m from its first vertex: x the signed
// distance to the water's edge (positive in the water), yz the way landward blurred (its length
// how much the shores about agree), w the water's width, metres.
@group(0) @binding(9) var shore_field: texture_2d<f32>;

const LAND_CELL: f32 = 24.0;
const SHORE_TEXEL: f32 = 3.0;
const LW_TYPE: u32 = 0x1Fu;
const LW_ROAD_SHIFT: u32 = 5u;
const LW_SW_TO_NE: u32 = 128u;
const LW_BUILDING: u32 = 256u;
const LW_LOOK_SHIFT: u32 = 9u;
const LW_SIZE_SHIFT: u32 = 13u;
const LW_WATER: u32 = 1048576u;
const LW_SWASH_SHIFT: u32 = 21u;
const LW_SWASH_STEP: f32 = 0.05;
const LW_BANK_SHIFT: u32 = 27u;
const LW_BANK_MOST: f32 = 1.6;
const LW_BANK_CLEAR: f32 = 0.05;
// The most the edge's wander carries the water over the ground up a bank, metres.
const BANK_REACH_MOST: f32 = 0.4;

struct LandVertex {
    height: f32,
    inside: f32,
    level: f32,
    word: u32,
}

fn has_land() -> bool {
    return frame.land.z > 0.0;
}

// Vertex `g` (counted from the field's first), clamped to the field.
fn land_vertex(g: vec2<i32>) -> LandVertex {
    let last = vec2<i32>(frame.land.zw) - 1;
    let t = textureLoad(land_field, clamp(g, vec2<i32>(0), last), 0);
    return LandVertex(bitcast<f32>(t.x), bitcast<f32>(t.y), bitcast<f32>(t.z), t.w);
}

// The far field's vertex `g`, clamped to it.
fn far_vertex(g: vec2<i32>) -> LandVertex {
    let last = i32(far_rings.water_side) - 1;
    let t = textureLoad(far_land, clamp(g, vec2<i32>(0), vec2<i32>(last)), 0);
    return LandVertex(bitcast<f32>(t.x), bitcast<f32>(t.y), bitcast<f32>(t.z), t.w);
}

// The cell of the near (land) field or the far one that `p` lies in, and where in it.
fn water_cell(p: vec2<f32>, far: bool) -> vec4<f32> {
    let o = select(frame.land.xy, far_rings.origin, far);
    let q = (p - o) / LAND_CELL;
    return vec4<f32>(floor(q), clamp(q - floor(q), vec2<f32>(0.0), vec2<f32>(1.0)));
}

// Vertex `g` of the near field or the far one.
fn water_vertex(g: vec2<i32>, far: bool) -> LandVertex {
    if (far) {
        return far_vertex(g);
    }
    return land_vertex(g);
}

// The shore field at render-space `p`, bilinear between its texels.
fn shore_at(p: vec2<f32>) -> vec4<f32> {
    return shore_sloped(p).at;
}

// The shore field at `p` and how its distance changes east and north there.
struct ShoreHere {
    at: vec4<f32>,
    slope: vec2<f32>,
}

fn shore_sloped(p: vec2<f32>) -> ShoreHere {
    let last = vec2<i32>(textureDimensions(shore_field)) - 1;
    let q = clamp((p - frame.land.xy) / SHORE_TEXEL, vec2<f32>(0.0), vec2<f32>(last));
    let i = min(vec2<i32>(floor(q)), last);
    let j = min(i + 1, last);
    let f = q - vec2<f32>(i);
    let a = textureLoad(shore_field, i, 0);
    let b = textureLoad(shore_field, vec2<i32>(j.x, i.y), 0);
    let c = textureLoad(shore_field, vec2<i32>(i.x, j.y), 0);
    let d = textureLoad(shore_field, j, 0);
    let slope = vec2<f32>(mix(b.x - a.x, d.x - c.x, f.y), mix(c.x - a.x, d.x - b.x, f.x)) / SHORE_TEXEL;
    return ShoreHere(mix(mix(a, b, f.x), mix(c, d, f.x), f.y), slope);
}

// A value across a cell's two triangles as retail cuts it: the corners sw, se, nw, ne at `f`.
fn on_triangles(cut: bool, h: vec4<f32>, f: vec2<f32>) -> f32 {
    if (cut) {
        if (f.x >= f.y) {
            return h.x + f.x * (h.y - h.x) + f.y * (h.w - h.y);
        }
        return h.x + f.y * (h.z - h.x) + f.x * (h.w - h.z);
    }
    if (f.x + f.y <= 1.0) {
        return h.x + f.x * (h.y - h.x) + f.y * (h.z - h.x);
    }
    return h.w + (1.0 - f.x) * (h.z - h.w) + (1.0 - f.y) * (h.y - h.w);
}

// A lattice point's hash, 0..1.
fn land_hash(p: vec2<i32>) -> f32 {
    let h0 = (bitcast<u32>(p.x) * 0x8DA6B343u) ^ (bitcast<u32>(p.y) * 0xD8163841u);
    let h1 = (h0 ^ (h0 >> 13u)) * 0x2C1B3C6Du;
    let h2 = h1 ^ (h1 >> 15u);
    return f32(h2 & 0xFFFFu) / 65535.0;
}

// Render space's origin in the world: its block, 192 m a side (`frame.surfaces.zw`). Render
// space's origin moves as the eye travels; what is laid on the world is laid from here.
fn origin_block() -> vec2<i32> {
    return bitcast<vec2<i32>>(frame.surfaces.zw);
}

// Render space's origin in cells of `cell` metres: whole cells, and the rest (kept small, so
// a point in render space keeps its precision).
fn origin_cells(cell: f32) -> vec4<f32> {
    let o = vec2<f32>(origin_block()) * (192.0 / cell);
    let whole = floor(o);
    return vec4<f32>(whole, o - whole);
}

// Smooth value noise over a lattice `cell` metres wide, 0..1, laid on the world: the same at a
// place however render space lies.
fn land_noise(p: vec2<f32>, cell: f32) -> f32 {
    let o = origin_cells(cell);
    let q = p / cell + o.zw;
    let i = vec2<i32>(floor(q)) + vec2<i32>(o.xy);
    let f = q - floor(q);
    let s = f * f * (3.0 - 2.0 * f);
    return mix(mix(land_hash(i), land_hash(i + vec2<i32>(1, 0)), s.x),
               mix(land_hash(i + vec2<i32>(0, 1)), land_hash(i + vec2<i32>(1, 1)), s.x), s.y);
}

// `land_noise` at world metres `w`, not render space's: the same to the bit wherever render space
// lies, as placement needs.
fn world_noise(w: vec2<f32>, cell: f32) -> f32 {
    let q = w / cell;
    let i = vec2<i32>(floor(q));
    let f = q - floor(q);
    let s = f * f * (3.0 - 2.0 * f);
    return mix(mix(land_hash(i), land_hash(i + vec2<i32>(1, 0)), s.x),
               mix(land_hash(i + vec2<i32>(0, 1)), land_hash(i + vec2<i32>(1, 1)), s.x), s.y);
}

// `land_noise` (x) and how it changes east and north, a metre (yz).
fn land_noise_sloped(p: vec2<f32>, cell: f32) -> vec3<f32> {
    let o = origin_cells(cell);
    let q = p / cell + o.zw;
    let i = vec2<i32>(floor(q)) + vec2<i32>(o.xy);
    let f = q - floor(q);
    let s = f * f * (3.0 - 2.0 * f);
    let ds = 6.0 * f * (1.0 - f) / cell;
    let a = land_hash(i);
    let b = land_hash(i + vec2<i32>(1, 0));
    let c = land_hash(i + vec2<i32>(0, 1));
    let d = land_hash(i + vec2<i32>(1, 1));
    return vec3<f32>(mix(mix(a, b, s.x), mix(c, d, s.x), s.y),
                     mix(b - a, d - c, s.y) * ds.x, mix(c - a, d - b, s.x) * ds.y);
}

// How far the water's edge wanders out at `p`, metres, as a pixel covering `foot` metres sees it
// (x), and how that changes east and north (yz).
fn edge_wander(p: vec2<f32>, foot: f32) -> vec3<f32> {
    let resolved9 = 1.0 - smoothstep(2.25, 4.5, foot);
    let resolved3 = 1.0 - smoothstep(0.9, 1.75, foot);
    let a = land_noise_sloped(p, 9.0);
    let b = land_noise_sloped(p + vec2<f32>(31.7, 12.9), 3.5);
    return vec3<f32>(a.x - 0.5, a.yz) * 6.0 * resolved9 + vec3<f32>(b.x - 0.5, b.yz) * 2.0 * resolved3;
}

// How far the sea's shore waves are carried along the shore at `p`, in their periods (x), and how
// that changes east and north (yz): the swash at the edge keeps the same time.
fn shore_wander(p: vec2<f32>) -> vec3<f32> {
    let n = land_noise_sloped(p + vec2<f32>(5.3, 17.1), 40.0);
    return vec3<f32>(n.x - 0.5, n.yz) * 0.6;
}

// What the water is at a point: how far inside its edge (negative on land), its level there, its
// look, its body's reach and how far its swash runs up (metres); how far its edge wanders out
// there, how steeply its bank rises (metres up a metre across) and how far out from halfway to the
// land the still water stops showing against it (retail's triangles lift the ground over the level
// from the last water vertex on), where its edge is drawn.
struct WaterHere {
    inside: f32,
    level: f32,
    look: u32,
    reach: f32,
    swash: f32,
    wander: f32,
    bank: f32,
    shift: f32,
}

// The water at render-space `p`, as a pixel covering `foot` metres sees it: the edge wanders a
// little (finer than the pixel left out), so it neither runs straight along the cells nor in the
// same curve round every lone water vertex.
fn water_here(p: vec2<f32>, foot: f32) -> WaterHere {
    if (!has_land()) {
        return WaterHere(-1000.0, 0.0, 0u, 0.0, 0.0, 0.0, 0.0, 0.0);
    }
    return water_from(p, foot, false);
}

// The same on the far rings, from the far field's records.
fn far_water_here(p: vec2<f32>, foot: f32) -> WaterHere {
    if (far_rings.water_side == 0u) {
        return WaterHere(-1000.0, 0.0, 0u, 0.0, 0.0, 0.0, 0.0, 0.0);
    }
    return water_from(p, foot, true);
}

// `water_here` from the near field or the far one.
fn water_from(p: vec2<f32>, foot: f32, far: bool) -> WaterHere {
    var w = WaterHere(-1000.0, 0.0, 0u, 0.0, 0.0, 0.0, 0.0, 0.0);
    let cell = water_cell(p, far);
    let c = vec2<i32>(cell.xy);
    let f = cell.zw;
    let a = water_vertex(c, far);
    let b = water_vertex(c + vec2<i32>(1, 0), far);
    let d = water_vertex(c + vec2<i32>(0, 1), far);
    let e = water_vertex(c + vec2<i32>(1, 1), far);
    let banks = banks_at(vec4<u32>(a.word, b.word, d.word, e.word), f);
    w.bank = banks.x;
    w.shift = banks.y;
    // Drawn where the still water stops showing, so the ground's facets never clip its wander.
    w.inside = mix(mix(a.inside, b.inside, f.x), mix(d.inside, e.inside, f.x), f.y) - w.shift;
    let resolved9 = 1.0 - smoothstep(2.25, 4.5, foot);
    let resolved3 = 1.0 - smoothstep(0.9, 1.75, foot);
    w.wander = (land_noise(p, 9.0) - 0.5) * 6.0 * resolved9
        + (land_noise(p + vec2<f32>(31.7, 12.9), 3.5) - 0.5) * 2.0 * resolved3;
    // Up a steep bank, no farther than the water reaches over it.
    w.wander *= min(1.0, (BANK_REACH_MOST - LW_BANK_CLEAR) / max(4.0 * w.bank, 1e-4));
    w.inside += w.wander;
    w.level = on_triangles((a.word & LW_SW_TO_NE) != 0u, vec4<f32>(a.level, b.level, d.level, e.level), f);
    // The look and reach of the most watery corner.
    var best = a;
    if (b.inside > best.inside) { best = b; }
    if (d.inside > best.inside) { best = d; }
    if (e.inside > best.inside) { best = e; }
    w.look = (best.word >> LW_LOOK_SHIFT) & 15u;
    w.reach = f32((best.word >> LW_SIZE_SHIFT) & 127u) * 8.0;
    w.swash = f32((best.word >> LW_SWASH_SHIFT) & 63u) * LW_SWASH_STEP;
    return w;
}

// The bank's rise (x) and how far out the still water stops showing against it (y), bilinear at
// `f` between the corners' words.
fn banks_at(words: vec4<u32>, f: vec2<f32>) -> vec2<f32> {
    let codes = vec4<f32>(words >> vec4<u32>(LW_BANK_SHIFT)) / 31.0;
    let banks = codes * codes * LW_BANK_MOST;
    let shifts = select(vec4<f32>(0.0),
        max(vec4<f32>(LAND_CELL * 0.5) - LW_BANK_CLEAR / max(banks, vec4<f32>(1e-4)), vec4<f32>(0.0)),
        banks > vec4<f32>(0.0));
    return vec2<f32>(mix(mix(banks.x, banks.y, f.x), mix(banks.z, banks.w, f.x), f.y),
                     mix(mix(shifts.x, shifts.y, f.x), mix(shifts.z, shifts.w, f.x), f.y));
}

// How far over the level the edge's wander carries the water up a bank rising `bank`.
fn bank_reach(bank: f32) -> f32 {
    return select(0.0, min(LW_BANK_CLEAR + 4.0 * bank, BANK_REACH_MOST), bank > 0.0);
}

// The water at render-space `p` from the near field, without the edge's wander: how far inside
// it, bilinear (x), its level on retail's triangles (y), how far inside its drawn edge (z) and
// its bank's rise (w).
fn water_plain(p: vec2<f32>) -> vec4<f32> {
    let cell = water_cell(p, false);
    let c = vec2<i32>(cell.xy);
    let f = cell.zw;
    let a = land_vertex(c);
    let b = land_vertex(c + vec2<i32>(1, 0));
    let d = land_vertex(c + vec2<i32>(0, 1));
    let e = land_vertex(c + vec2<i32>(1, 1));
    let inside = mix(mix(a.inside, b.inside, f.x), mix(d.inside, e.inside, f.x), f.y);
    let level = on_triangles((a.word & LW_SW_TO_NE) != 0u, vec4<f32>(a.level, b.level, d.level, e.level), f);
    let banks = banks_at(vec4<u32>(a.word, b.word, d.word, e.word), f);
    return vec4<f32>(inside, level, inside - banks.y, banks.x);
}

// The swash at `p` by water `w`: x how far its edge has run up past the still edge, metres
// (negative as it draws back below it), y how much it is running up (0..1), z the farthest it
// runs. Each of the sea's shore waves reaching the shore (`frame.shore.x`, carried along it as
// the waves are) runs it up fast and lets it slide back; farther on a wider body, in more wind.
fn swash_at(p: vec2<f32>, w: WaterHere) -> vec3<f32> {
    if (w.swash <= 0.0) {
        return vec3<f32>(0.0);
    }
    let most = w.swash * mix(0.45, 1.0, smoothstep(24.0, 400.0, w.reach))
        * (0.6 + 0.4 * frame.wind.z) * (0.7 + 0.6 * land_noise(p + vec2<f32>(3.1, 7.7), 17.0));
    // The sea's swash keeps its surf's time; a lake's or a swamp's laps slower, once in ten to
    // twenty seconds (a whole number of times in the clock's ten-minute wrap).
    let slow = fract(frame.shore.y * round(mix(30.0, 60.0, smoothstep(0.05, 0.8, w.swash))));
    var phase = fract(slow + (land_noise(p, 13.0) - 0.5) * 0.8);
    if (w.swash >= 1.5) {
        phase = fract(frame.shore.x - shore_wander(p).x);
    }
    let up = smoothstep(0.0, 0.3, phase) - smoothstep(0.3, 1.0, phase);
    let running = 1.0 - smoothstep(0.12, 0.4, phase);
    return vec3<f32>(most * (up - 0.25), running, most * 0.75);
}

// How many metres of the world a pixel `dist` metres from the eye covers.
fn pixel_foot(dist: f32) -> f32 {
    return dist * 2.0 * frame.cluster_z.w / frame.params.w;
}

// How much of the water is drawn `edge` metres inside its swashing edge, as a pixel covering `foot`
// metres sees it: none at the edge, whole a little in (over a pixel and a half at least).
fn water_cover(edge: f32, foot: f32) -> f32 {
    return smoothstep(0.0, max(0.5, foot * 1.5), edge);
}

// The ground's height at render-space `p` on retail's triangles, and its terrain word (the
// cell's first corner's).
fn land_height(p: vec2<f32>) -> vec2<f32> {
    let q = (p - frame.land.xy) / LAND_CELL;
    let c = vec2<i32>(floor(q));
    let f = clamp(q - floor(q), vec2<f32>(0.0), vec2<f32>(1.0));
    let a = land_vertex(c);
    let b = land_vertex(c + vec2<i32>(1, 0));
    let d = land_vertex(c + vec2<i32>(0, 1));
    let e = land_vertex(c + vec2<i32>(1, 1));
    let h = on_triangles((a.word & LW_SW_TO_NE) != 0u, vec4<f32>(a.height, b.height, d.height, e.height), f);
    return vec2<f32>(h, bitcast<f32>(a.word));
}

// The made-up depth of the bed below the water's surface `inside` metres inside the water: none
// at its edge, falling away steepest there and levelling out at `depth` metres `shelf` in.
fn bed_depth(inside: f32, depth: f32, shelf: f32) -> f32 {
    let t = 1.0 - clamp(inside / max(shelf, 1e-3), 0.0, 1.0);
    return depth * (1.0 - t * t);
}

// How wet a lit point is from the water beside it (x), and how much of it the water covers (y),
// as a pixel covering `foot` metres sees it: the ground the swash reaches is damp and what it has
// just left wetter, a band just over the level along a bank, and anything standing in it below
// the level wholly.
fn waterline_wet(world: vec3<f32>, foot: f32) -> vec2<f32> {
    if (!has(F_WATER) || !has_land()) {
        return vec2<f32>(0.0);
    }
    // Far from any water, one texel says so.
    let g = vec2<i32>(round((world.xy - frame.land.xy) / LAND_CELL));
    if (land_vertex(g).inside < -30.0) {
        return vec2<f32>(0.0);
    }
    let w = water_here(world.xy, foot);
    if (w.inside < -6.0) {
        return vec2<f32>(0.0);
    }
    let above = world.z - w.level;
    let sw = swash_at(world.xy, w);
    let edge = w.inside + sw.x;
    // Out past the swash's reach on the flat, and farther up a bank standing out of the water.
    let reach = min(sw.z + 1.0 + 16.0 * max(above, 0.0), 6.0);
    let near = 1.0 - smoothstep(sw.z, reach, -w.inside);
    let band = 0.12 + 0.25 * land_noise(world.xy, 1.7);
    // The film the water has just drawn back from, thinner ahead of it as it runs up.
    var film = 1.0;
    if (edge < 0.0) {
        film = (1.0 - smoothstep(0.0, 0.25 + 0.3 * sw.z, -edge)) * mix(1.0, 0.6, sw.y);
    }
    let wet = near * (1.0 - smoothstep(band * 0.5, band, above)) * mix(0.6, 1.0, film);
    let up = bank_reach(w.bank);
    let covered = water_cover(edge, foot) * (1.0 - smoothstep(up, up + 0.03, above));
    return vec2<f32>(max(wet, covered), covered);
}
