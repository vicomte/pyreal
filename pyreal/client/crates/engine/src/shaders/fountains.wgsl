// Fountains (`fountains::frame_jets`) and a basin's water (`flags::POOL`), drawn with the ice
// over a copy of the frame behind them. The basin's water is still and clear: the floor under it
// seen through it, bent and a little drunk of its red, the room or the sky mirrored by Fresnel
// (what the frame shows along the reflected ray, else the probes' light), the lamps' and the
// sun's glints, and faint ripples, ringing out where a jet's water lands. A jet is strips of
// water along its path, each shaded as a round stream: what is behind it bent across it, the
// room mirrored at its edges, its glints, and a little white where air is caught in it. The
// column is whole; the crown falling from its top tears into drops as it falls.

struct Jet {
    // xyz where it leaves, w its radius there, metres.
    at: vec4<f32>,
    // xyz its speed, metres a second; w seconds to its end.
    v: vec4<f32>,
    // x its pull, metres a second a second, up positive; y how fast it swells, metres a second;
    // z a seed; w its kind (0 the column, 1 a stream of the crown, 2 a stream off a lip, 3 only a
    // ring, its length nothing) plus its flags (256: in a room), a whole number.
    more: vec4<f32>,
    // xyz where it lands, w how strongly it rings the water there (0: nowhere).
    land: vec4<f32>,
}

struct Jets {
    // x how many pieces, y the clock, seconds wrapped to ten minutes (f32 bits).
    head: vec4<u32>,
    piece: array<Jet>,
}

@group(3) @binding(16) var<storage, read> jets: Jets;

// A piece's rows along its path (`fountains::JET_ROWS`).
const JET_ROWS: u32 = 24u;
const JET_COLUMN: u32 = 0u;
const JET_CROWN: u32 = 1u;
const JET_INDOOR: u32 = 256u;
// How much of a stream is white with the air caught in it: the column, the crown, a lip's.
const JET_WHITE: vec3<f32> = vec3<f32>(0.28, 0.45, 0.12);
const JET_WHITE_ALBEDO: vec3<f32> = vec3<f32>(0.75, 0.82, 0.86);
// How far the water bends what is behind it, metres along the bent ray.
const JET_BEND: f32 = 0.25;
// The crown's drops: how many a second of the water's own time, and how much of the stream
// they leave whole at the top and at the bottom.
const JET_DROPS: f32 = 28.0;
const JET_TORN: vec2<f32> = vec2<f32>(0.2, 0.62);
// How much wider than a stream its drops scatter, and their sizes, a share of its radius.
const JET_SCATTER: f32 = 2.5;
const JET_DROP_SIZE: vec2<f32> = vec2<f32>(0.45, 1.0);

// A basin's water: what a metre takes of each channel (clear water, a little green-blue), how
// much of its picture's hue it takes on, and how deep that hue is reached, metres.
const POOL_CLEAR: vec3<f32> = vec3<f32>(0.9, 0.35, 0.25);
const POOL_TINT: f32 = 0.35;
const POOL_HUE_DEPTH: f32 = 0.6;
// How much of retail's picture of the water shows under its surface, lit as the floor is, in
// clear water and in water seen only from above.
const POOL_PICTURE: f32 = 0.3;
const POOL_OPAQUE_PICTURE: f32 = 0.45;
// The least still water mirrors, so it reads as a mirror seen from above too, and how far the
// frame's edge is believed where the mirrored ray leaves the frame.
const POOL_MIRROR: f32 = 0.3;
const POOL_EDGE_BELIEF: f32 = 0.8;
// Water seen only from above (retail's opaque water, a tub's): how deep it is taken to be, metres.
const POOL_OPAQUE_DEPTH: f32 = 0.5;
// Water running down a slope (a wall fountain's stream): its streaks' size across and along,
// metres, how fast it runs, metres a second, their slope, and how much of it is white.
const POOL_RUN_CELL: vec2<f32> = vec2<f32>(0.12, 0.3);
const POOL_RUN_SPEED: f32 = 1.2;
const POOL_RUN_SLOPE: f32 = 0.35;
const POOL_RUN_WHITE: f32 = 0.45;
// How much of retail's own glow still water keeps, beside its picture's share, and running water.
const POOL_STILL_GLOW: f32 = 1.0;
const POOL_RUN_GLOW: f32 = 0.2;
// How much of retail's picture shows in running water.
const POOL_RUN_PICTURE: f32 = 0.4;
// Its picture runs with it, this many metres a repeat.
const POOL_RUN_REPEAT: f32 = 0.6;
// The deepest the view is taken to run through it, metres.
const POOL_DEEPEST: f32 = 1.5;
// The light it scatters back from inside, a share of what falls on it.
const POOL_SCATTER: vec3<f32> = vec3<f32>(0.01, 0.025, 0.03);
// A still pool's drift: its ripples' slope, and their size, metres.
const POOL_DRIFT: f32 = 0.025;
const POOL_DRIFT_CELL: f32 = 0.35;
// Rings where the water lands: their slope at the heart, metres between crests, how fast they
// run out, metres a second, how far they reach and fade over, metres.
const POOL_RING_SLOPE: f32 = 0.12;
const POOL_RING_LENGTH: f32 = 0.09;
const POOL_RING_SPEED: f32 = 0.35;
const POOL_RING_REACH: f32 = 0.9;
const POOL_RING_FADE: f32 = 0.25;
const POOL_INDEX: f32 = 1.33;
const POOL_F0: f32 = 0.02;

fn jet_clock() -> f32 {
    return bitcast<f32>(jets.head.y);
}

fn jet_hash(n: f32) -> f32 {
    return fract(sin(n * 12.9898 + 4.1414) * 43758.5453);
}

// Value noise along one line.
fn jet_noise(x: f32) -> f32 {
    let i = floor(x);
    let f = fract(x);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(jet_hash(i), jet_hash(i + 1.0), u);
}

// Value noise over a plane.
fn pool_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let h = vec4<f32>(
        jet_hash(i.x + i.y * 57.0),
        jet_hash(i.x + 1.0 + i.y * 57.0),
        jet_hash(i.x + (i.y + 1.0) * 57.0),
        jet_hash(i.x + 1.0 + (i.y + 1.0) * 57.0),
    );
    return mix(mix(h.x, h.y, u.x), mix(h.z, h.w, u.x), u.y);
}

fn jet_at(j: Jet, t: f32) -> vec3<f32> {
    return j.at.xyz + j.v.xyz * t + vec3<f32>(0.0, 0.0, 0.5 * j.more.x * t * t);
}

// Where a world point lands on the target, in pixels, and its view depth (w); none behind.
fn pool_to_screen(p: vec3<f32>) -> vec4<f32> {
    let c = frame.view_proj * vec4<f32>(p, 1.0);
    if (c.w <= 1e-3) {
        return vec4<f32>(-1.0);
    }
    let ndc = c.xyz / c.w;
    return vec4<f32>((ndc.x * 0.5 + 0.5) * frame.params.z, (0.5 - ndc.y * 0.5) * frame.params.w,
                     c.z, c.w);
}

// What lies behind a surface at pixel `px` (`dist` along the view), seen along `bent` from
// `world` `reach` metres on: the frame there, where that is behind the surface, else straight
// behind it.
fn pool_behind(px: vec2<f32>, world: vec3<f32>, bent: vec3<f32>, reach: f32, dist: f32,
               most: f32) -> vec3<f32> {
    var uv = px / frame.params.zw;
    let s = pool_to_screen(world + bent * reach);
    if (s.w > 0.0) {
        let off = clamp(s.xy / frame.params.zw - uv, vec2<f32>(-most), vec2<f32>(most));
        if (ice_room((uv + off) * frame.params.zw, dist) > 0.0) {
            uv += off;
        }
    }
    return textureSampleLevel(ice_behind, samp_clamp, uv, 0.0).rgb;
}

// The frame along the reflected ray `r` from `p`, found by marching through the depth behind
// (rgb, and how far it is to be believed in a). Only what stands above `p` is found.
fn pool_mirror(p: vec3<f32>, r: vec3<f32>) -> vec4<f32> {
    var step = 0.08;
    var t = 0.04;
    var prev_t = 0.0;
    var last = vec4<f32>(0.0);
    for (var k = 0u; k < 20u; k++) {
        let q = p + r * t;
        let s = pool_to_screen(q);
        if (s.w <= 0.0 || s.x < 0.0 || s.y < 0.0 || s.x >= frame.params.z || s.y >= frame.params.w) {
            break;
        }
        // Where the ray leaves the frame, the frame's edge along it stands in.
        let uv_k = s.xy / frame.params.zw;
        last = vec4<f32>(textureSampleLevel(ice_behind, samp_clamp, uv_k, 0.0).rgb,
                         POOL_EDGE_BELIEF * smoothstep(0.0, 0.3, t));
        let d = textureLoad(g_depth, vec2<i32>(s.xy), 0);
        if (d > 0.0) {
            let behind = s.w - s.z / d;
            let at = ice_unproject(floor(s.xy) + 0.5, d);
            if (behind > 0.0 && behind < max(step * 3.0, 0.3) && at.z > p.z + 0.02) {
                var lo = prev_t;
                var hi = t;
                for (var b = 0u; b < 4u; b++) {
                    let mid = (lo + hi) * 0.5;
                    let sm = pool_to_screen(p + r * mid);
                    let dm = textureLoad(g_depth, vec2<i32>(clamp(sm.xy, vec2<f32>(0.0), frame.params.zw - 1.0)), 0);
                    if (dm > 0.0 && sm.w - sm.z / dm > 0.0) {
                        hi = mid;
                    } else {
                        lo = mid;
                    }
                }
                let uv = pool_to_screen(p + r * hi).xy / frame.params.zw;
                let edge = min(min(uv.x, uv.y), min(1.0 - uv.x, 1.0 - uv.y));
                let c = textureSampleLevel(ice_behind, samp_clamp, uv, 0.0).rgb;
                return vec4<f32>(c, smoothstep(0.0, 0.06, edge) * (1.0 - f32(k) / 20.0 * 0.4));
            }
        }
        prev_t = t;
        t += step;
        step *= 1.3;
    }
    return last;
}

// What a surface of water at `world` facing `n` mirrors along `r`, pre-exposed: the sky outdoors,
// the room's light indoors, the probes' light where they are trusted.
fn pool_surround(world: vec3<f32>, n: vec3<f32>, r_in: vec3<f32>, indoor: bool) -> vec3<f32> {
    var r = r_in;
    var mirrored = frame.indoor_light.rgb;
    if (!indoor) {
        mirrored = frame.sky_light.rgb;
        if (!has(F_PHYSICAL)) {
            mirrored = pow(min(frame.ambient.rgb, vec3<f32>(1.0)), vec3<f32>(2.2));
        }
        if (frame.switches.x != 0u) {
            r.z = max(r.z, 0.1);
            if (has(F_ATMOSPHERE)) {
                mirrored = sky_radiance(r);
            } else {
                mirrored = sky_colour(r);
            }
        }
    }
    if (has(F_GI)) {
        let pl = probe_light(world + n * 0.3, r);
        mirrored = mix(mirrored, pl.rgb, pl.a * select(0.5, 0.85, indoor));
    }
    return mirrored * pre_exposure();
}

// The light falling on water at `world` (facing `n`): the sun's and the sky's outdoors, the
// room's indoors, and the lamps'; with the lamps' glints off it (pre-exposed).
struct PoolLight {
    falling: vec3<f32>,
    glints: vec3<f32>,
}

fn pool_light(world: vec3<f32>, n: vec3<f32>, v: vec3<f32>, px: vec2<f32>, indoor: bool,
              shine: f32) -> PoolLight {
    var out: PoolLight;
    let pre = pre_exposure();
    let depth = view_depth_at(world);
    var sun = frame.sun_light.rgb;
    var sky = frame.sky_light.rgb;
    if (!has(F_PHYSICAL)) {
        sun = pow(frame.sun.rgb, vec3<f32>(2.2));
        sky = pow(min(frame.ambient.rgb, vec3<f32>(1.0)), vec3<f32>(2.2));
    }
    var sun_vis = 0.0;
    if (!indoor) {
        sun_vis = 1.0;
        if (has(F_SHADOWS)) {
            sun_vis = sun_shadow(world, n, depth, px);
        }
    } else {
        sky = frame.indoor_light.rgb;
    }
    out.falling = sun * max(frame.sun_dir.z, 0.0) * sun_vis + sky;
    out.glints = vec3<f32>(0.0);
    if (has(F_LAMPS)) {
        let lamp = lamp_light(world, n, v, shine, vec3<f32>(POOL_F0), select(0u, G_INDOOR, indoor),
                              depth, px, false);
        out.falling += select(lamps_held(lamp.diffuse, out.falling), lamp.diffuse, indoor);
        out.glints = lamp.specular * pre;
    }
    if (has(F_GI)) {
        let pl = probe_light(world, n);
        out.falling = mix(out.falling, max(out.falling, pl.rgb), pl.a);
    }
    let ndl = dot(n, frame.sun_dir.xyz);
    if (!indoor && ndl > 0.0) {
        out.glints += ggx(n, v, frame.sun_dir.xyz, shine, vec3<f32>(POOL_F0)) * ndl * sun * sun_vis * pre;
    }
    out.falling *= pre;
    return out;
}

// A lit colour's own light seen through the air outdoors; `behind` came through it already.
fn pool_aired(c: vec3<f32>, behind: vec3<f32>, world: vec3<f32>, dir: vec3<f32>) -> vec3<f32> {
    let pre = pre_exposure();
    let own = c - behind;
    var aired = own;
    if (has(F_ATMOSPHERE)) {
        aired = aerial(own / max(pre, 1e-30), world, dir) * pre;
    } else {
        aired = mix(own, sky_colour(dir) * pre, fog_amount(distance(world.xy, frame.camera.xy)));
    }
    return behind + aired;
}

// The slope of a basin's water at `p`: its faint drift and the rings where jets land.
fn pool_slope(p: vec3<f32>, foot: f32) -> vec2<f32> {
    let clock = jet_clock();
    var slope = vec2<f32>(0.0);
    // Two drifting layers of noise, their slope by differences.
    let e = 0.05;
    for (var k = 0; k < 2; k++) {
        let cell = POOL_DRIFT_CELL * select(1.0, 0.43, k == 1);
        let drift = vec2<f32>(0.11, 0.07) * select(1.0, -1.3, k == 1) * clock;
        let q = (p.xy + drift) / cell;
        let h = pool_noise(q);
        slope += vec2<f32>(pool_noise(q + vec2<f32>(e, 0.0)) - h, pool_noise(q + vec2<f32>(0.0, e)) - h)
            / e * POOL_DRIFT;
    }
    // Rings: a pixel too coarse for them keeps none.
    let resolved = 1.0 - smoothstep(POOL_RING_LENGTH * 0.25, POOL_RING_LENGTH * 0.6, foot);
    if (resolved > 0.0) {
        let count = min(jets.head.x, arrayLength(&jets.piece));
        for (var k = 0u; k < count; k++) {
            let j = jets.piece[k];
            if (j.land.w <= 0.0 || abs(p.z - j.land.z) > 0.25) {
                continue;
            }
            let d = p.xy - j.land.xy;
            let r = length(d);
            if (r > POOL_RING_REACH) {
                continue;
            }
            let phase = (r - clock * POOL_RING_SPEED) / POOL_RING_LENGTH * 6.2831853 + j.more.z * 40.0;
            let amp = POOL_RING_SLOPE * j.land.w * exp(-r / POOL_RING_FADE)
                * smoothstep(0.0, 0.03, r) * (0.7 + 0.3 * sin(clock * 3.1 + j.more.z * 17.0));
            slope += d / max(r, 1e-4) * cos(phase) * amp * resolved;
        }
    }
    return slope;
}

@fragment
fn fs_pool(in: VsOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    let m = materials[in.material];
    if (screened(in.clip.xy, in.fade)) {
        discard;
    }
    let px = in.clip.xy;
    let eye = frame.camera.xyz;
    let to = in.world - eye;
    let dist = length(to);
    let dir = to / max(dist, 1e-4);
    let v = -dir;
    let indoor = in.cell != 0u;
    let pre = pre_exposure();
    var n = normalize(in.normal);
    if (!front) {
        n = -n;
    }
    // A level face ripples.
    // Which way down the face, and across it, for water running down an upright one: by the
    // flat face, so the streaks keep their way where the model's normals bend.
    var flat = normalize(cross(dpdx(in.world), dpdy(in.world)));
    if (dot(flat, n) < 0.0) {
        flat = -flat;
    }
    var down = vec3<f32>(0.0, 0.0, -1.0) - flat * (-flat.z);
    if (length(down) < 1e-3) {
        down = vec3<f32>(0.0, 0.0, -1.0);
    }
    down = normalize(down);
    let across = cross(flat, down);
    let running = flat.z <= 0.7;
    var run_uv = in.uv;
    if (running) {
        run_uv = vec2<f32>(dot(in.world, across), dot(in.world, down) - jet_clock() * POOL_RUN_SPEED)
            / POOL_RUN_REPEAT;
    }
    let run_dx = dpdx(run_uv);
    let run_dy = dpdy(run_uv);
    var froth = 0.0;
    if (!running) {
        let foot = dist * 2.0 * frame.cluster_z.w / frame.params.w;
        let s = pool_slope(in.world, foot);
        n = normalize(vec3<f32>(-s, 1.0));
    } else {
        // Running down the face: streaks drawn out downhill, moving with the water.
        let q = vec2<f32>(dot(in.world, across) / POOL_RUN_CELL.x,
                          (dot(in.world, down) - jet_clock() * POOL_RUN_SPEED) / POOL_RUN_CELL.y);
        let e = 0.1;
        let h = pool_noise(q);
        let g = vec2<f32>(pool_noise(q + vec2<f32>(e, 0.0)) - h, pool_noise(q + vec2<f32>(0.0, e)) - h) / e;
        n = normalize(n + (across * g.x + down * g.y * 0.3) * POOL_RUN_SLOPE);
        froth = POOL_RUN_WHITE * smoothstep(0.35, 0.8, pool_noise(q * vec2<f32>(1.0, 0.5) + 17.0));
    }
    let cos_v = saturate(dot(n, v));

    // The floor under it, seen through it: bent, and drunk by the water the view crosses.
    // Retail's opaque water has nothing modelled under it: it is taken to be deep, its picture
    // its floor.
    let clear = (m.flags & M_TRANSLUCENT) != 0u;
    var thick = POOL_OPAQUE_DEPTH;
    if (clear) {
        thick = clamp(ice_room(px, dist), 0.0, POOL_DEEPEST);
    }
    let bent = refract(dir, n, 1.0 / POOL_INDEX);
    var seen = vec3<f32>(0.0);
    if (clear) {
        seen = pool_behind(px, in.world, bent, max(thick, 0.15), dist, 0.04);
    }
    // Its hue: the picture's, at its coarsest, as much as POOL_TINT.
    var hue = vec3<f32>(0.14, 0.45, 1.0);
    var picture = m.base_color.rgb;
    if ((m.flags & M_HAS_BASE) != 0u) {
        let pic = textureSampleLevel(textures[m.tex_base], samp_repeat, in.uv, 6.0).rgb;
        hue = clamp(pic / max(max(pic.r, max(pic.g, pic.b)), 1e-3), vec3<f32>(0.05), vec3<f32>(1.0));
        picture = textureSampleGrad(textures[m.tex_base], samp_repeat, run_uv + (n.xy - normalize(in.normal).xy) * 0.3, run_dx, run_dy).rgb;
    }
    let extinction = mix(POOL_CLEAR, -log(hue) / POOL_HUE_DEPTH, POOL_TINT);
    let through = exp(-extinction * thick);
    let light = pool_light(in.world, n, v, px, indoor, 0.04);
    if (!clear) {
        seen = picture * light.falling * 0.31831;
    }
    var body = seen * through + POOL_SCATTER * light.falling * (1.0 - exp(-3.0 * thick));
    let shown = select(select(POOL_OPAQUE_PICTURE, POOL_PICTURE, clear), POOL_RUN_PICTURE, running);
    body = mix(body, picture * light.falling * 0.31831, shown);
    body = mix(body, JET_WHITE_ALBEDO * light.falling * 0.31831, froth);
    // Retail's own glow, some of it, so the water shows in a dark room.
    let glow = select(POOL_STILL_GLOW * shown, POOL_RUN_GLOW, running);
    body += glow_of(mix(picture, JET_WHITE_ALBEDO * 0.3, froth), m.emissive.x * glow) * pre_exposure();

    // The room above it, mirrored: what the frame shows along the ray, else what surrounds it.
    let r = reflect(dir, n);
    var mirrored = pool_surround(in.world, n, r, indoor);
    if (dist < 40.0 && !running) {
        let ssr = pool_mirror(in.world + n * 0.01, r);
        mirrored = mix(mirrored, ssr.rgb, ssr.a * (1.0 - smoothstep(25.0, 40.0, dist)));
    }
    var fresnel = POOL_F0 + (1.0 - POOL_F0) * pow(1.0 - cos_v, 5.0);
    if (running) {
        fresnel *= 0.5;
    } else {
        fresnel = max(fresnel, POOL_MIRROR);
    }
    var c = mix(body, mirrored, fresnel) + light.glints;
    if (!indoor) {
        c = pool_aired(c, seen * through, in.world, dir);
    }
    if (frame.umbrae[0].w > 0.0) {
        c *= umbra(in.world);
    }
    return vec4<f32>(min(c, vec3<f32>(HDR_MOST)), 1.0);
}

struct JetOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    // Across the strip, -1 to 1; seconds along its path.
    @location(1) across: f32,
    @location(2) t: f32,
    // Its sideways axis and which way the water runs there.
    @location(3) side: vec3<f32>,
    @location(4) along: vec3<f32>,
    // How much of the strip's width is water (a strip widened to a pixel is thinner).
    @location(5) cover: f32,
    @location(6) @interpolate(flat) index: u32,
}

@vertex
fn vs_jet(@builtin(vertex_index) vi: u32, @builtin(instance_index) n: u32) -> JetOut {
    let j = jets.piece[n];
    let row = vi / 6u;
    let corner = vi % 6u;
    // Two triangles a row: (along, across).
    let c = array<vec2<u32>, 6>(vec2<u32>(0u, 0u), vec2<u32>(0u, 1u), vec2<u32>(1u, 0u),
                                vec2<u32>(1u, 0u), vec2<u32>(0u, 1u), vec2<u32>(1u, 1u));
    let k = c[corner];
    let t = f32(row + k.x) / f32(JET_ROWS) * j.v.w;
    let p = jet_at(j, t);
    var along = j.v.xyz + vec3<f32>(0.0, 0.0, j.more.x * t);
    if (length(along) < 1e-3) {
        along = vec3<f32>(0.0, 0.0, 1.0);
    }
    along = normalize(along);
    let to_eye = frame.camera.xyz - p;
    let dist = length(to_eye);
    var side = cross(along, to_eye / max(dist, 1e-4));
    if (length(side) < 1e-4) {
        side = vec3<f32>(1.0, 0.0, 0.0);
    }
    side = normalize(side);
    let radius = j.at.w + j.more.y * t;
    // At least a pixel and a half across; the crown's wide enough for its drops to scatter.
    let foot = dist * 2.0 * frame.cluster_z.w / frame.params.w;
    let spread = select(1.0, JET_SCATTER, u32(j.more.w + 0.5) % 256u == JET_CROWN);
    let drawn = max(radius * spread, foot * 0.75);
    let u = f32(k.y) * 2.0 - 1.0;
    let w = p + side * u * drawn;
    var out: JetOut;
    out.clip = frame.view_proj * vec4<f32>(w, 1.0);
    out.world = w;
    out.across = u;
    out.t = t;
    out.side = side;
    out.along = along;
    out.cover = radius * spread / drawn;
    out.index = n;
    return out;
}

struct JetFrag {
    @location(0) colour: vec4<f32>,
    @location(1) reactive: vec4<f32>,
}

@fragment
fn fs_jet(in: JetOut) -> JetFrag {
    let j = jets.piece[in.index];
    let word = u32(j.more.w + 0.5);
    let kind = word & 0xFFu;
    let indoor = (word & JET_INDOOR) != 0u;
    var u = in.across;
    let share = in.t / max(j.v.w, 1e-4);
    let seed = j.more.z;
    // The water's own time: born this long ago, so its features run along with it.
    let born = jet_clock() - in.t;
    var cover = in.cover;
    var thin = 1.0;
    var white = JET_WHITE.z;
    if (kind == JET_COLUMN) {
        white = JET_WHITE.x;
        cover *= smoothstep(0.0, 0.03, share);
    } else if (kind == JET_CROWN) {
        white = JET_WHITE.y;
        // Torn into drops as it falls, rounder the further.
        let x = born * JET_DROPS + seed * 97.0;
        let id = floor(x);
        let torn = mix(JET_TORN.x, JET_TORN.y, smoothstep(0.0, 0.8, share));
        let loose = smoothstep(0.05, 0.3, share);
        // Each drop a bead of its own size, off to one side of the stream, round at its ends;
        // the stream whole near the top.
        let size = mix(1.0, mix(JET_DROP_SIZE.x, JET_DROP_SIZE.y, jet_hash(id + 7.1)), loose);
        let off = (jet_hash(id + 3.7) * 2.0 - 1.0) * (JET_SCATTER - size) * loose;
        u = (u * JET_SCATTER - off) / size;
        let bead = 1.0 - abs(fract(x) * 2.0 - 1.0);
        let round = 1.0 - (1.0 - bead) * (1.0 - bead) * loose - u * u;
        let kept = mix(1.0, step(torn, jet_hash(id)), loose);
        cover *= kept * smoothstep(0.0, 0.3, round);
        cover *= smoothstep(0.0, 0.06, share);
        thin = size;
    }
    if (abs(u) >= 1.0) {
        discard;
    }
    cover *= 1.0 - smoothstep(0.7, 1.0, abs(u));
    if (j.land.w <= 0.0) {
        cover *= 1.0 - smoothstep(0.7, 1.0, share);
    }
    if (cover < 0.01) {
        discard;
    }
    let eye = frame.camera.xyz;
    let to = in.world - eye;
    let dist = length(to);
    let dir = to / max(dist, 1e-4);
    let v = -dir;
    // A round stream: its face turns from the eye to the side across the strip.
    let facing = normalize(v - in.along * dot(v, in.along));
    let n = normalize(in.side * u + facing * sqrt(max(1.0 - u * u, 0.0)));
    let cos_v = saturate(dot(n, v));
    let px = in.clip.xy;
    let pre = pre_exposure();

    // Behind it, bent across it.
    let bent = refract(dir, n, 1.0 / POOL_INDEX);
    let seen = pool_behind(px, in.world, bent, JET_BEND, dist, 0.03);
    // Air caught in it, more in places, running with the water.
    let froth = white * (0.6 + 0.8 * jet_noise(born * 6.0 + u * 2.0 + seed * 31.0));
    let light = pool_light(in.world, n, v, px, indoor, 0.03);
    let thick = 2.0 * j.at.w * thin * sqrt(max(1.0 - u * u, 0.0));
    let tint = exp(-POOL_CLEAR * thick * 4.0);
    let body = mix(seen * tint, JET_WHITE_ALBEDO * light.falling * 0.31831, froth);
    let r = reflect(dir, n);
    let mirrored = pool_surround(in.world, n, r, indoor);
    let fresnel = POOL_F0 + (1.0 - POOL_F0) * pow(1.0 - cos_v, 5.0);
    var c = mix(body, mirrored, fresnel) + light.glints;
    if (!indoor) {
        c = pool_aired(c, seen * tint * (1.0 - froth), in.world, dir);
    }
    var out: JetFrag;
    out.colour = vec4<f32>(min(c, vec3<f32>(HDR_MOST)) * cover, cover);
    out.reactive = vec4<f32>(cover * 0.8);
    return out;
}
