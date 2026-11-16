// The G-buffer: every instance the cull pass kept, drawn by bucket. The instance index is the
// draw's identity: it names the entry of the visible list, which names the instance and its
// material.

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec2<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) extra: u32,
    @location(4) extra2: u32,
}

struct VsOut {
    @builtin(position) @invariant clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) cur: vec4<f32>,
    @location(4) prev: vec4<f32>,
    @location(5) @interpolate(flat) material: u32,
    @location(6) @interpolate(flat) extra: u32,
    @location(7) @interpolate(flat) cell: u32,
    // An impostor's: the eye's bearing round its upright axis, and its own x and y axes.
    @location(8) @interpolate(flat) turn: f32,
    @location(9) @interpolate(flat) ax: vec3<f32>,
    @location(10) @interpolate(flat) ay: vec3<f32>,
    // The object it is part of, plus one (0: none), for picking and highlights.
    @location(11) @interpolate(flat) object: u32,
    // How far the object is faded out, 0 to 255; above it its level's crossfade code and, from
    // bit 16, a hash of the instance ([`screened`]).
    @location(12) @interpolate(flat) fade: u32,
    // The cover's vertex colour (its brightness); unused elsewhere.
    @location(13) shade: f32,
    // 1 for an instance that moves (the pool's dynamic slots).
    @location(14) @interpolate(flat) moves: u32,
    // Where on its model, in the model's own frame.
    @location(15) local: vec3<f32>,
}

// Whether pixel `p` of an object faded `fade & 255` (of 255) is left out this frame: a screen
// door that moves with the jitter, so the temporal resolve sees through it. Two levels crossfaded
// (the code in bits 8 to 15, its top bit the coarser level's) keep complementary pixels, the
// door shifted by the instance's hash so neighbours do not dissolve alike.
fn screened(p: vec2<f32>, fade: u32) -> bool {
    if (fade == 0u) {
        return false;
    }
    let n = door(p);
    if (n < f32(fade & 0xFFu) / 255.0) {
        return true;
    }
    let code = (fade >> 8u) & 0xFFu;
    if (code == 0u) {
        return false;
    }
    let m = fract(n + f32(fade >> 16u) / 65536.0);
    let t = f32(code & 0x7Fu) / 127.0;
    let coarse = (code & 0x80u) != 0u;
    return (coarse && m >= t) || (!coarse && m < t);
}

// The screen door's noise at pixel `p`, moved with the jitter.
fn door(p: vec2<f32>) -> f32 {
    let q = p + frame.jitter.xy * 4096.0;
    return fract(52.9829189 * fract(dot(q, vec2<f32>(0.06711056, 0.00583715))));
}

// An object's fade word: its own fade and its level's crossfade code, with the instance's hash.
fn fade_word(own: u32, code: u32, instance: u32) -> u32 {
    if (code == 0u) {
        return own;
    }
    var h = instance * 0x9E3779B9u;
    h = (h ^ (h >> 15u)) * 0x2C1B3C6Du;
    h = h ^ (h >> 13u);
    return own | (code << 8u) | (h & 0xFFFF0000u);
}

struct GOut {
    @location(0) albedo: vec4<f32>,
    @location(1) normal: vec2<f32>,
    @location(2) material: vec4<f32>,
    @location(3) motion: vec2<f32>,
    @location(4) id: u32,
    @location(5) object: u32,
}

// A transform's rows turned to the eye as degrade mode `mode` turns a part (the client's
// `calc_draw_frame`): 2 aims its y axis at the eye, 3 to 5 keep its x, y or z axis and turn the
// next one toward the eye. Each axis keeps its scale.
fn billboard(rows: array<vec4<f32>, 3>, mode: u32) -> array<vec4<f32>, 3> {
    if (mode < 2u || mode > 5u) {
        return rows;
    }
    var c = array<vec3<f32>, 3>(
        vec3<f32>(rows[0].x, rows[1].x, rows[2].x),
        vec3<f32>(rows[0].y, rows[1].y, rows[2].y),
        vec3<f32>(rows[0].z, rows[1].z, rows[2].z),
    );
    let s = vec3<f32>(length(c[0]), length(c[1]), length(c[2]));
    var a = array<vec3<f32>, 3>(c[0] / max(s.x, 1e-6), c[1] / max(s.y, 1e-6), c[2] / max(s.z, 1e-6));
    let origin = vec3<f32>(rows[0].w, rows[1].w, rows[2].w);
    var v = origin - frame.camera.xyz;
    if (length(v) <= 0.0002) {
        v = vec3<f32>(0.0, 0.0, 1.0);
    } else {
        v = normalize(v);
    }
    if (mode == 2u) {
        var x = cross(v, vec3<f32>(0.0, 0.0, 1.0));
        x = select(vec3<f32>(1.0, 0.0, 0.0), normalize(x), length(x) > 1e-4);
        a[0] = x;
        a[1] = v;
        a[2] = cross(x, v);
    } else {
        let nxt = array<u32, 3>(2u, 0u, 1u);
        let k = mode - 3u;
        let axis = a[k];
        var p = v - axis * dot(axis, v);
        if (length(p) < 1e-4) {
            p = a[nxt[k]];
        } else {
            p = normalize(p);
        }
        if (abs(dot(p, axis)) <= 0.001) {
            a[nxt[k]] = p;
            a[nxt[nxt[k]]] = cross(p, axis);
        }
    }
    c[0] = a[0] * s.x;
    c[1] = a[1] * s.y;
    c[2] = a[2] * s.z;
    return array<vec4<f32>, 3>(
        vec4<f32>(c[0].x, c[1].x, c[2].x, origin.x),
        vec4<f32>(c[0].y, c[1].y, c[2].y, origin.y),
        vec4<f32>(c[0].z, c[1].z, c[2].z, origin.z),
    );
}

// The eye's bearing round an upright model's axis, in its own frame (0 along its x, toward y).
fn bearing(rows: array<vec4<f32>, 3>) -> f32 {
    let to = frame.camera.xyz - vec3<f32>(rows[0].w, rows[1].w, rows[2].w);
    let ax = vec3<f32>(rows[0].x, rows[1].x, rows[2].x);
    let ay = vec3<f32>(rows[0].y, rows[1].y, rows[2].y);
    return atan2(dot(to, ay) / max(dot(ay, ay), 1e-8), dot(to, ax) / max(dot(ax, ax), 1e-8));
}

// An impostor's quad corner (x across, z up) turned about the axis to face the eye at `turn`.
fn facing(local: vec3<f32>, turn: f32) -> vec3<f32> {
    return vec3<f32>(-sin(turn) * local.x, cos(turn) * local.x, local.z);
}

@vertex
fn vs_main(v: VsIn, @builtin(instance_index) entry: u32) -> VsOut {
    let e = visible[entry];
    let inst = instances[e.x];
    // The material word carries the drawn level's billboard mode in its top byte and its
    // crossfade code below it.
    let mode = e.y >> 24u;
    let m = billboard(array<vec4<f32>, 3>(inst.m0, inst.m1, inst.m2), mode);
    let p = billboard(array<vec4<f32>, 3>(inst.p0, inst.p1, inst.p2), mode);
    var local = v.position;
    var turn = 0.0;
    if (mode == IMPOSTOR_MODE) {
        turn = bearing(m);
        local = facing(local, turn);
    }
    var world = xform(m[0], m[1], m[2], local);
    var prev = xform(p[0], p[1], p[2], local);
    let posed = (inst.sway & SKINNED) != 0u;
    if (posed) {
        // A posed body's vertex carries where it stood last frame.
        let was = v.position + vec3<f32>(unpack2x16float(v.extra), unpack2x16float(v.extra2).x);
        prev = xform(p[0], p[1], p[2], was);
    }
    if (inst.sway != 0u && !posed && mode < 2u) {
        // The pack's growth sways in the wind.
        let s = unpack2x16float(inst.sway);
        let foot = vec3<f32>(m[0].w, m[1].w, m[2].w);
        let rise = v.position.z / max(s.y, 0.01);
        let tall = s.y * length(vec3<f32>(m[0].z, m[1].z, m[2].z));
        world += sway_at(foot, v.position, rise, tall, frame.sun_light.w) * s.x;
        prev += sway_at(foot, v.position, rise, tall, frame.wind.w) * s.x;
    }
    var out: VsOut;
    out.turn = turn;
    out.ax = normalize(vec3<f32>(inst.m0.x, inst.m1.x, inst.m2.x));
    out.ay = normalize(vec3<f32>(inst.m0.y, inst.m1.y, inst.m2.y));
    out.clip = frame.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    out.normal = normalize(xform_dir(m[0], m[1], m[2], oct_decode(v.normal)));
    out.uv = v.uv;
    out.cur = out.clip;
    out.prev = frame.prev_view_proj * vec4<f32>(prev, 1.0);
    out.material = e.y & 0xFFFFu;
    out.extra = v.extra;
    out.cell = inst.cell & 0xFFFFFFu;
    out.fade = fade_word(inst.cell >> 24u, (e.y >> 16u) & 0xFFu, e.x);
    out.object = inst.object + 1u;
    out.moves = select(0u, 1u, e.x >= bitcast<u32>(frame.snow.w));
    out.local = v.position;
    return out;
}

// The ground's depth prepass: the same vertices, no outputs, the rooms cut out.
struct DepthOut {
    @builtin(position) @invariant clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) @interpolate(flat) extra: u32,
}

@vertex
fn vs_depth(v: VsIn, @builtin(instance_index) i: u32) -> DepthOut {
    let inst = instances[i];
    let world = xform(inst.m0, inst.m1, inst.m2, v.position);
    var out: DepthOut;
    out.clip = frame.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    out.extra = v.extra;
    return out;
}

fn motion(cur: vec4<f32>, prev: vec4<f32>) -> vec2<f32> {
    return motion_of(cur, prev);
}

// The sky's motion, drawn over the depth's clear value alone: the camera's turn, as for a point
// infinitely far off.
@vertex
fn vs_sky_motion(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
}

struct SkyMotion {
    @location(3) motion: vec2<f32>,
}

@fragment
fn fs_sky_motion(@builtin(position) pos: vec4<f32>) -> SkyMotion {
    let ndc = vec2<f32>(pos.x / frame.params.z * 2.0 - 1.0, 1.0 - pos.y / frame.params.w * 2.0);
    let h = frame.inv_view_proj * vec4<f32>(ndc, 1.0, 1.0);
    let d = vec4<f32>(h.xyz / h.w - frame.camera.xyz, 0.0);
    var o: SkyMotion;
    o.motion = motion_of(frame.view_proj * d, frame.prev_view_proj * d);
    return o;
}

fn pack(albedo: vec3<f32>, n: vec3<f32>, rough: f32, metal: f32, emissive: f32, bits: u32,
        material: u32, cur: vec4<f32>, prev: vec4<f32>) -> GOut {
    var g: GOut;
    g.albedo = vec4<f32>(albedo, 1.0);
    g.normal = oct_encode(n);
    g.material = vec4<f32>(rough, metal, emissive, f32(bits) / 255.0);
    g.motion = motion(cur, prev);
    g.id = material;
    g.object = 0u;
    return g;
}

// `n` bent by a tangent-space normal `t` (x along the picture's width, y up it), the tangent
// frame taken from the surface's screen derivatives, as no vertex carries one.
fn bend(n: vec3<f32>, t: vec3<f32>, wdx: vec3<f32>, wdy: vec3<f32>, dx: vec2<f32>,
        dy: vec2<f32>) -> vec3<f32> {
    let p2 = cross(wdy, n);
    let p1 = cross(n, wdx);
    let tu = p2 * dx.x + p1 * dy.x;
    let tv = p2 * dx.y + p1 * dy.y;
    let s = inverseSqrt(max(max(dot(tu, tu), dot(tv, tv)), 1e-20));
    // Up the picture is against its rows, so against v.
    return normalize(tu * s * t.x - tv * s * t.y + n * t.z);
}

// A two-channel normal's third part.
fn unpack_normal(rg: vec2<f32>) -> vec3<f32> {
    let xy = rg * 2.0 - 1.0;
    return vec3<f32>(xy, sqrt(max(1.0 - dot(xy, xy), 0.0)));
}

// Where in an impostor's atlas one of the two views either side of bearing `turn` keeps `uv`
// (the farther one where `pick`, 0 to 1, is under the bearing's share past the nearer), and the
// size of a tile in the atlas (for the derivatives).
fn impostor_uv(m: Material, uv: vec2<f32>, turn: f32, pick: f32) -> vec4<f32> {
    let count = max(m.views & 0xFFu, 1u);
    let cols = max((m.views >> 8u) & 0xFFu, 1u);
    let rows = (count + cols - 1u) / cols;
    let f = turn / 6.2831853 * f32(count);
    var k = (i32(floor(f)) + select(0, 1, fract(f) > pick)) % i32(count);
    if (k < 0) {
        k += i32(count);
    }
    let grid = vec2<f32>(f32(cols), f32(rows));
    let tile = vec2<f32>(f32(u32(k) % cols), f32(u32(k) / cols));
    return vec4<f32>((tile + clamp(uv, vec2<f32>(0.002), vec2<f32>(0.998))) / grid, 1.0 / grid);
}

// A level of pictures from round a model: the view nearest the eye's bearing, its colour cut out,
// its normal (the model's own, in its frame) turned by the instance.
fn impostor(in: VsOut, m: Material, dx: vec2<f32>, dy: vec2<f32>) -> GOut {
    let at = impostor_uv(m, in.uv, in.turn, door(in.clip.xy + vec2<f32>(37.0, 91.0)));
    let gx = dx * at.zw;
    let gy = dy * at.zw;
    var c = m.base_color;
    if ((m.flags & M_HAS_BASE) != 0u) {
        c *= textureSampleGrad(textures[m.tex_base], samp_clamp, at.xy, gx, gy);
    }
    if (c.a < m.alpha_cutoff) {
        discard;
    }
    var n = normalize(in.normal);
    if (m.tex_normal != NO_TEXTURE) {
        let t = textureSampleGrad(textures[m.tex_normal], samp_clamp, at.xy, gx, gy).rgb * 2.0 - 1.0;
        n = normalize(t.x * in.ax + t.y * in.ay + t.z * cross(in.ax, in.ay));
    }
    var bits = 0u;
    if ((m.flags & M_FOLIAGE) != 0u) {
        bits |= G_FOLIAGE;
    }
    var g = pack(c.rgb, n, m.roughness, m.metal, 0.0, bits, in.material, in.cur, in.prev);
    g.object = in.object;
    return g;
}


// Relief: where the view meets a material's height, and how much of the key light
// (the sun, or the nearest lamp in a room or by night) reaches that point past the height.
struct Relief {
    uv: vec2<f32>,
    lit: f32,
    // 1 when the key light is a lamp.
    lamp: f32,
}

// Where the relief shows: whole within a share of the level's reach (`frame.relief.z`), gone by
// it, and flattened when a surface is seen nearly along it (its shift would grow without bound
// and tear).
const RELIEF_NEAR: f32 = 0.35;
const RELIEF_FLAT_AT: f32 = 0.1;
const RELIEF_FULL_AT: f32 = 0.45;
// How dark the height shades what stands in its light.
const RELIEF_SHADOW: f32 = 0.8;

// A material's height at `uv` (1 the surface, 0 its deepest), at mip `level`.
fn relief_height(m: Material, uv: vec2<f32>, level: f32) -> f32 {
    let t = textureSampleLevel(textures[m.tex_height], samp_repeat, uv, level);
    return select(t.r, t.b, (m.flags & M_PACKED_SURFACE) != 0u);
}

// The light a surface's relief is shaded toward at `p` (xyz, toward it) and whether it is a lamp
// (w): in a room or by night, the nearest lamp lighting it most; else the sun (the moon by night).
fn key_light(p: vec3<f32>, indoor: bool) -> vec4<f32> {
    var best = 0.0;
    var dir = vec4<f32>(frame.sun_dir.xyz, 0.0);
    if (!has(F_LAMPS) || (!indoor && frame.sun_dir.w > 0.3)) {
        return dir;
    }
    for (var k = 0u; k < 4u; k++) {
        let l = frame.key_lamps[k];
        let d = l.xyz - p;
        let dist = length(d);
        if (l.w <= 0.0 || dist >= l.w || dist < 1e-3) {
            continue;
        }
        let x = dist / l.w;
        let e = frame.key_power[k] / (dist * dist) * (1.0 - x * x) * (1.0 - x * x);
        if (e > best) {
            best = e;
            dir = vec4<f32>(d / dist, 1.0);
        }
    }
    if (!indoor && best < frame.sun_light.r * max(frame.sun_dir.z, 0.0)) {
        return vec4<f32>(frame.sun_dir.xyz, 0.0);
    }
    return dir;
}

fn relief(m: Material, in: VsOut, n: vec3<f32>, wdx: vec3<f32>, wdy: vec3<f32>, dx: vec2<f32>,
          dy: vec2<f32>) -> Relief {
    var r: Relief;
    r.uv = in.uv;
    r.lit = 1.0;
    r.lamp = 0.0;
    if (!has(F_RELIEF) || (m.flags & M_HAS_HEIGHT) == 0u || frame.surfaces.x <= 0.0) {
        return r;
    }
    let perp2 = cross(wdy, n);
    let perp1 = cross(n, wdx);
    let det = dot(wdx, perp2);
    let to_eye = frame.camera.xyz - in.world;
    let dist = length(to_eye);
    let to = to_eye / max(dist, 1e-3);
    let facing = dot(to, n);
    let reach = frame.relief.z;
    let fade = (1.0 - smoothstep(reach * RELIEF_NEAR, reach, dist))
        * smoothstep(RELIEF_FLAT_AT, RELIEF_FULL_AT, facing);
    if (abs(det) < 1e-14 || fade <= 0.0) {
        return r;
    }
    // The picture's coordinates per metre along the surface.
    let per_u = (perp2 * dx.x + perp1 * dy.x) / det;
    let per_v = (perp2 * dx.y + perp1 * dy.y) / det;
    let depth_m = frame.surfaces.x * fade;
    let along = -(to - n * facing) / max(facing, RELIEF_FULL_AT) * depth_m;
    let span = vec2<f32>(dot(along, per_u), dot(along, per_v));
    let side = vec2<f32>(textureDimensions(textures[m.tex_height]));
    let level = max(0.0, 0.5 * log2(max(dot(dx * side, dx * side), dot(dy * side, dy * side))));
    // March down the view until it passes under the height, then meet it between the steps.
    let steps = (8.0 + 24.0 * (1.0 - facing)) * frame.relief.x;
    var was_depth = 0.0;
    var was_under = 0.0;
    var depth = 0.0;
    var hit = 0.0;
    for (var k = 0; k < 32; k++) {
        let under = 1.0 - relief_height(m, in.uv + span * depth, level);
        if (depth >= under || depth >= 1.0) {
            let a = was_under - was_depth;
            let b = depth - under;
            hit = mix(was_depth, depth, clamp(a / max(a + b, 1e-4), 0.0, 1.0));
            break;
        }
        was_depth = depth;
        was_under = under;
        depth += 1.0 / steps;
    }
    r.uv = in.uv + span * hit;
    // Walk from there toward the key light (ten steps, fewer below High): what of the height
    // stands above the ray shades it.
    let key = key_light(in.world, in.cell != 0u);
    r.lamp = key.w;
    let l = key.xyz;
    let fl = dot(l, n);
    if (fl < 0.05 || hit <= 0.0) {
        return r;
    }
    let toward = (l - n * fl) / max(fl, 0.33) * depth_m;
    let lspan = vec2<f32>(dot(toward, per_u), dot(toward, per_v));
    var shade = 0.0;
    let walk = max(round(10.0 * frame.relief.x), 1.0);
    for (var k = 1; k <= 10; k++) {
        if (f32(k) > walk) {
            break;
        }
        let t = f32(k) / walk;
        let ray = hit * (1.0 - t);
        let under = 1.0 - relief_height(m, r.uv + lspan * hit * t, level);
        shade = max(shade, (ray - under) * 7.0 * (1.0 - t * 0.5));
    }
    r.lit = 1.0 - RELIEF_SHADOW * clamp(shade, 0.0, 1.0) * fade;
    return r;
}

@fragment
fn fs_object(in: VsOut, @builtin(front_facing) front: bool) -> GOut {
    return object_surface(in, front);
}

fn object_surface(in: VsOut, front: bool) -> GOut {
    if (screened(in.clip.xy, in.fade)) {
        discard;
    }
    let dx = dpdx(in.uv);
    let dy = dpdy(in.uv);
    let wdx = dpdx(in.world);
    let wdy = dpdy(in.world);
    let m = materials[in.material];
    if (web_card_hidden(m.flags)) {
        discard;
    }
    if ((m.flags & M_IMPOSTOR) != 0u) {
        return impostor(in, m, dx, dy);
    }
    var n = normalize(in.normal);
    if (!front) {
        n = -n;
    }
    // Every picture is read where the view meets the height (the surface's own place without one).
    let at = relief(m, in, n, wdx, wdy, dx, dy);
    let uv = at.uv;
    var c = m.base_color;
    if ((m.flags & M_HAS_BASE) != 0u) {
        c *= textureSampleGrad(textures[m.tex_base], samp_repeat, uv, dx, dy);
    }
    if ((m.flags & M_CUTOUT) != 0u && c.a < m.alpha_cutoff) {
        discard;
    }
    if ((m.flags & M_HAS_NORMAL) != 0u) {
        let t = textureSampleGrad(textures[m.tex_normal], samp_repeat, uv, dx, dy).rg;
        // A pack material's bend on a retail picture is depth mapping's.
        let k = select(1.0, frame.relief.y, (m.flags & M_PACKED_SURFACE) != 0u);
        n = bend(n, unpack_normal(0.5 + (t - 0.5) * k), wdx, wdy, dx, dy);
    }
    var rough = m.roughness;
    var metal = m.metal;
    var occlusion = 1.0;
    if ((m.flags & M_HAS_ROUGH_METAL) != 0u) {
        let rm = textureSampleGrad(textures[m.tex_rough_metal], samp_repeat, uv, dx, dy);
        if ((m.flags & M_PACKED_SURFACE) != 0u) {
            // A pack material's surface: roughness, occlusion, height.
            rough = rm.r;
            occlusion = rm.g;
        } else {
            rough = rm.g;
            metal = rm.b;
        }
    }
    if ((m.flags & (M_HAS_OCCLUSION | M_PACKED_SURFACE)) == M_HAS_OCCLUSION) {
        occlusion = textureSampleGrad(textures[m.tex_occlusion], samp_repeat, uv, dx, dy).r;
    }
    var heat = vec2<f32>(0.0);
    if (m.kind == KIND_MOLTEN) {
        heat = molten_heat(c.rgb);
    }
    // A map's occlusion darkens the colour itself: the G-buffer has no channel for it.
    c = vec4<f32>(c.rgb * mix(occlusion, 1.0, heat.x), c.a);
    var bits = 0u;
    if ((m.flags & M_WATER) != 0u) {
        bits |= G_WATER;
    }
    if ((m.flags & M_FOLIAGE) != 0u) {
        bits |= G_FOLIAGE;
    }
    if (in.cell != 0u) {
        bits |= G_INDOOR;
    }
    if (at.lamp > 0.5) {
        bits |= G_KEY_LAMP;
    }
    if (in.moves != 0u) {
        bits |= G_MOVES;
    }
    var emissive = m.emissive.x;
    if (heat.x > 0.0) {
        let glow = molten_glow(heat, in.world.xy);
        c = vec4<f32>(mix(c.rgb, glow.rgb, heat.x), c.a);
        emissive = max(emissive, glow.w);
        bits |= G_GLOWS;
    }
    var g = pack(c.rgb, n, rough, metal, emissive, bits, in.material, in.cur, in.prev);
    g.albedo.a = at.lit;
    g.object = in.object;
    return g;
}

// An alpha map's walk at `r` quarter turns, as the texture merge walks it.
fn rotate_uv(uv: vec2<f32>, r: u32) -> vec2<f32> {
    switch r {
        case 1u: { return vec2<f32>(1.0 - uv.y, uv.x); }
        case 2u: { return vec2<f32>(1.0 - uv.x, 1.0 - uv.y); }
        case 3u: { return vec2<f32>(uv.y, 1.0 - uv.x); }
        default: { return uv; }
    }
}

fn rotate_d(d: vec2<f32>, r: u32) -> vec2<f32> {
    switch r {
        case 1u: { return vec2<f32>(-d.y, d.x); }
        case 2u: { return -d; }
        case 3u: { return vec2<f32>(d.y, -d.x); }
        default: { return d; }
    }
}

// Whether `p` lies inside a room volume over this terrain cell: the ground there is cut away.
fn inside_a_room(cell: TerrainCell, p: vec3<f32>) -> bool {
    for (var k = 0u; k < cell.cut_count; k++) {
        let at = cut_lists[cell.cut_first + k];
        let lo = cuts[at];
        let hi = cuts[at + 1u];
        if (any(p < lo.xyz - 0.02) || any(p > hi.xyz + 0.02)) {
            continue;
        }
        // Past its building's reach the room is not drawn, so the ground stays.
        if (distance(p, frame.camera.xyz) * bitcast<f32>(frame.switches.z) >= hi.w) {
            continue;
        }
        var inside = true;
        let n = bitcast<u32>(lo.w);
        for (var q = 0u; q < n; q++) {
            let pl = cuts[at + 2u + q];
            if (dot(pl.xyz, p) + pl.w < -0.02) {
                inside = false;
                break;
            }
        }
        if (inside) {
            return true;
        }
    }
    return false;
}

@fragment
fn fs_depth(in: DepthOut) {
    if (frame.switches.y != 0u && inside_a_room(cells[in.extra], in.world)) {
        discard;
    }
}

// The ground's surface, each part where the frame asks for it: layers blended by
// their heights, steep ground's pictures projected along the slope, fine bends near the eye, and
// a tint and a second, turned placement of the pictures across large areas.

// Steep ground takes its pictures stood up along the slope from this up-facing (the normal's
// height) and wholly by the next (`renderer::TRIPLANAR`).
const TRIPLANAR_FROM: f32 = 0.8;
const TRIPLANAR_WHOLE: f32 = 0.6;
// How far a layer's height carries it past its plain share, and how much sharper it makes the
// edge per unit of height between the two.
const HEIGHT_REACH: f32 = 0.6;
// How much of the mud picture's grain the bed's silt takes, and how much wider than a pixel its
// blur reads.
const BED_GRAIN: f32 = 0.8;
const BED_BLUR: f32 = 32.0;
const HEIGHT_SHARPEN: f32 = 3.0;
// The second placement's patches and the tint's two scales, metres.
const MACRO_PATCH: f32 = 31.0;
const MACRO_TINT: vec2<f32> = vec2<f32>(57.0, 197.0);
// The second placement's turn (cosine, sine) and shift.
const MACRO_TURN: vec2<f32> = vec2<f32>(0.6, 0.8);
const MACRO_SHIFT: vec2<f32> = vec2<f32>(0.31, 0.57);
// How strongly the close detail bends the ground, per surface kind (`SurfaceKind::ALL`'s order).
const DETAIL_BEND = array<f32, 10>(0.5, 0.8, 0.5, 0.2, 0.3, 0.6, 0.4, 0.0, 0.25, 0.8);

// One ground picture's colour, roughness, bend (its normal's east and north parts), height
// (`has` 1 where the picture has one) and, molten, its heat (`molten_heat`).
struct GroundSample {
    colour: vec3<f32>,
    rough: f32,
    bend: vec2<f32>,
    height: f32,
    has: f32,
    heat: vec2<f32>,
}

// How the pictures are laid at a pixel: the world point and its changes across the pixel, the
// share taken along the slope (`side`) and of it the share stood on the plane facing east
// (`east`), and the share of the second, turned placement (`turn`).
struct Lay {
    p: vec3<f32>,
    wdx: vec3<f32>,
    wdy: vec3<f32>,
    side: f32,
    east: f32,
    turn: f32,
}

fn turned(v: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(MACRO_TURN.x * v.x - MACRO_TURN.y * v.y, MACRO_TURN.y * v.x + MACRO_TURN.x * v.y);
}

fn unturned(v: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(MACRO_TURN.x * v.x + MACRO_TURN.y * v.y, -MACRO_TURN.y * v.x + MACRO_TURN.x * v.y);
}

// Where render space's origin falls in a picture laid along the world's axes `s` times a metre
// (each axis its own), the whole repeats left out: kept exact, the origin being whole blocks.
fn origin_uv(k: vec2<f32>, s: vec2<f32>) -> vec2<f32> {
    return fract(k * fract(192.0 * s));
}

// [`origin_uv`] for the second, turned placement of a picture laid flat `s` times a metre.
fn origin_turned(s: vec2<f32>) -> vec2<f32> {
    let k = vec2<f32>(origin_block());
    let ex = turned(vec2<f32>(s.x, 0.0));
    let ny = turned(vec2<f32>(0.0, s.y));
    return fract(k.x * fract(192.0 * ex) + k.y * fract(192.0 * ny));
}

// A picture at `uv` (render space's, with the origin's share `o` and, for the second placement,
// `ot`), the second placement mixed in by `turn`.
fn placed(slot: u32, uv: vec2<f32>, o: vec2<f32>, ot: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>, turn: f32) -> vec4<f32> {
    var c = vec4<f32>(0.0);
    if (turn < 1.0) {
        c = textureSampleGrad(textures[slot], samp_repeat, uv + o, dx, dy) * (1.0 - turn);
    }
    if (turn > 0.0) {
        c += textureSampleGrad(textures[slot], samp_repeat, turned(uv) + ot + MACRO_SHIFT, turned(dx),
                               turned(dy)) * turn;
    }
    return c;
}

// A picture repeated `per_m` times a metre, laid on the world: flat (east along its width, north
// up it), and stood up on the upright planes where the ground is steep.
fn ground_read(slot: u32, per_m: f32, lay: Lay) -> vec4<f32> {
    let s = vec2<f32>(1.0, -1.0) * per_m;
    let k = vec2<f32>(origin_block());
    var c = vec4<f32>(0.0);
    if (lay.side < 1.0) {
        c = placed(slot, lay.p.xy * s, origin_uv(k, s), origin_turned(s), lay.wdx.xy * s, lay.wdy.xy * s, lay.turn) * (1.0 - lay.side);
    }
    if (lay.side > 0.0) {
        var q = vec4<f32>(0.0);
        if (lay.east < 1.0) {
            let o = vec2<f32>(origin_uv(k, s).x, 0.0);
            let ot = fract(k.x * fract(192.0 * turned(vec2<f32>(s.x, 0.0))));
            q = placed(slot, lay.p.xz * s, o, ot, lay.wdx.xz * s, lay.wdy.xz * s, lay.turn) * (1.0 - lay.east);
        }
        if (lay.east > 0.0) {
            let o = vec2<f32>(origin_uv(k.yx, s).x, 0.0);
            let ot = fract(k.y * fract(192.0 * turned(vec2<f32>(s.x, 0.0))));
            q += placed(slot, lay.p.yz * s, o, ot, lay.wdx.yz * s, lay.wdy.yz * s, lay.turn) * lay.east;
        }
        c += q * lay.side;
    }
    return c;
}

// A normal picture's east and north parts where it lies flat (none where the ground is stood up).
fn ground_bend(slot: u32, per_m: f32, lay: Lay) -> vec2<f32> {
    if (lay.side >= 1.0) {
        return vec2<f32>(0.0);
    }
    let s = vec2<f32>(1.0, -1.0) * per_m;
    let uv = lay.p.xy * s;
    let o = origin_uv(vec2<f32>(origin_block()), s);
    let dx = lay.wdx.xy * s;
    let dy = lay.wdy.xy * s;
    var b = vec2<f32>(0.0);
    if (lay.turn < 1.0) {
        b = (textureSampleGrad(textures[slot], samp_repeat, uv + o, dx, dy).rg * 2.0 - 1.0) * (1.0 - lay.turn);
    }
    if (lay.turn > 0.0) {
        let t = textureSampleGrad(textures[slot], samp_repeat, turned(uv) + origin_turned(s) + MACRO_SHIFT,
                                  turned(dx), turned(dy)).rg * 2.0 - 1.0;
        b += unturned(t) * lay.turn;
    }
    return b * (1.0 - lay.side);
}

// One picture with its companions: its pack normal and surface (roughness, occlusion and perhaps
// height), and where its height is kept.
fn ground_picture(slot: u32, per_m: f32, lay: Lay, rough: f32, molten: bool) -> GroundSample {
    var g: GroundSample;
    let c = ground_read(slot, per_m, lay);
    g.colour = c.rgb;
    g.rough = rough;
    g.bend = vec2<f32>(0.0);
    g.height = 0.5;
    g.has = 0.0;
    g.heat = vec2<f32>(0.0);
    if (molten) {
        g.heat = molten_heat(c.rgb);
    }
    let comp = companions[slot];
    // With depth mapping off the bend is never drawn.
    if (comp.maps.x != NO_TEXTURE && frame.relief.y > 0.0) {
        g.bend = ground_bend(comp.maps.x, per_m, lay);
    }
    if (comp.maps.y != NO_TEXTURE) {
        let sf = ground_read(comp.maps.y, per_m, lay);
        g.rough = sf.r;
        // A crack's floor is shut in, but its glow is not.
        g.colour *= mix(sf.g, 1.0, g.heat.x);
        if ((comp.more.x & H_IN_SURFACE) != 0u) {
            g.height = sf.b;
            g.has = 1.0;
        }
    }
    if ((comp.more.x & H_IN_ALPHA) != 0u) {
        g.height = c.a;
        g.has = 1.0;
    }
    g.colour *= ground_detail(comp.more, lay);
    return g;
}

// A pack ground's detail grain near the eye: twice its gray in display values, so mid-gray leaves
// the colour as it is, scaled by its strength and gone by DETAIL_REACH metres.
const DETAIL_REACH: vec2<f32> = vec2<f32>(15.0, 40.0);
fn ground_detail(more: vec4<u32>, lay: Lay) -> f32 {
    let size = bitcast<f32>(more.z);
    if (size <= 0.0) {
        return 1.0;
    }
    let near = 1.0 - smoothstep(DETAIL_REACH.x, DETAIL_REACH.y, distance(lay.p, frame.camera.xyz));
    if (near <= 0.0) {
        return 1.0;
    }
    let gray = ground_read(more.y, 1.0 / size, lay).r;
    let display = max(mix(1.0, 2.0 * gray, bitcast<f32>(more.w) * near), 0.0);
    return pow(display, 2.2);
}

// One layer's ground: a dat picture repeated `tiling` times across a cell, or (a negative tiling)
// a pack picture laid at its own metres a repeat, its steep slopes' picture where the ground is
// steep.
fn ground_layer(l: vec4<u32>, lay: Lay, up: f32) -> GroundSample {
    let tiling = bitcast<f32>(l.w);
    let kind = min((l.z >> 8u) & 0xFFu, 9u);
    let rough = KIND_ROUGHNESS[kind];
    let molten = kind == KIND_MOLTEN;
    if (tiling >= 0.0) {
        return ground_picture(l.x, tiling / 24.0, lay, rough, molten);
    }
    var g = ground_picture(l.x, -1.0 / tiling, lay, rough, molten);
    let comp = companions[l.x].maps;
    let steep = 1.0 - smoothstep(0.62, 0.78, up);
    if (comp.z != NO_TEXTURE && steep > 0.0) {
        let s = ground_picture(comp.z, 1.0 / bitcast<f32>(comp.w), lay, rough, molten);
        g.colour = mix(g.colour, s.colour, steep);
        g.rough = mix(g.rough, s.rough, steep);
        g.bend = mix(g.bend, s.bend, steep);
        g.height = mix(g.height, s.height, steep);
        g.has = max(g.has, s.has);
        g.heat = mix(g.heat, s.heat, steep);
    }
    return g;
}

// The silt the bed is drawn with over retail's water picture `water`: its colour, the water
// picture's light and dark, and the mud picture's (`frame.bed`) about its mean, with its bend.
fn bed_layer(water: GroundSample, lay: Lay) -> GroundSample {
    let silt = vec3<f32>(0.15, 0.13, 0.09) * clamp(luma(water.colour) / 0.12, 0.6, 1.5);
    let slot = bitcast<u32>(frame.bed.x);
    if (slot == 0u) {
        var g = water;
        g.colour = silt;
        g.rough = 0.85;
        return g;
    }
    var g = ground_picture(slot, frame.bed.y, lay, 0.85, false);
    // The picture's fine grain about its own blur there, so the silt keeps its brightness and
    // no broad patch of the picture shows.
    var blur = lay;
    blur.wdx *= BED_BLUR;
    blur.wdy *= BED_BLUR;
    let grain = luma(ground_read(slot, frame.bed.y, lay).rgb)
        / max(luma(ground_read(slot, frame.bed.y, blur).rgb), 1e-3);
    g.colour = silt * clamp(mix(1.0, grain, BED_GRAIN), 0.3, 2.0);
    // Its height stays the water picture's, so the shore blends and stands as before.
    g.height = water.height;
    g.has = water.has;
    return g;
}

// The share an overlay of plain share `over` takes over what lies under it, by their heights:
// the higher pokes through, the edge sharper the more they differ; their plain shares where
// neither has a height.
fn height_share(over: f32, h_over: f32, h_under: f32) -> f32 {
    let dh = h_over - h_under;
    let t = over + dh * HEIGHT_REACH * 4.0 * over * (1.0 - over);
    return clamp((t - 0.5) * (1.0 + HEIGHT_SHARPEN * abs(dh)) + 0.5, 0.0, 1.0);
}

// The share of the second, turned placement at `p`: patches tens of metres across, mostly one or
// the other.
fn macro_turn(p: vec2<f32>) -> f32 {
    let n = land_noise(p, MACRO_PATCH) * 0.7 + land_noise(p + vec2<f32>(31.7, 11.3), MACRO_PATCH * 0.37) * 0.3;
    return smoothstep(0.44, 0.56, n);
}

// The large-scale tint at `p`: a little lighter or darker, warmer or cooler, over a few hundred
// metres.
fn macro_tint(p: vec2<f32>) -> vec3<f32> {
    let a = land_noise(p + vec2<f32>(91.3, 12.9), MACRO_TINT.x);
    let b = land_noise(p - vec2<f32>(7.1, 55.3), MACRO_TINT.y);
    let bright = 0.9 + 0.2 * (a * 0.6 + b * 0.4);
    let warm = (b - 0.5) * 0.12;
    return bright * vec3<f32>(1.0 + warm, 1.0, 1.0 - warm);
}

// Value noise's slope at `p` in cells of `cell` metres, per cell (one look-up of its four corners).
fn noise_slope(p: vec2<f32>, cell: f32) -> vec2<f32> {
    let o = origin_cells(cell);
    let q = p / cell + o.zw;
    let i = vec2<i32>(floor(q)) + vec2<i32>(o.xy);
    let f = q - floor(q);
    let u = f * f * (3.0 - 2.0 * f);
    let du = 6.0 * f * (1.0 - f);
    let a = land_hash(i);
    let b = land_hash(i + vec2<i32>(1, 0));
    let c = land_hash(i + vec2<i32>(0, 1));
    let d = land_hash(i + vec2<i32>(1, 1));
    let k = a - b - c + d;
    return du * vec2<f32>(b - a + k * u.y, c - a + k * u.x);
}

// Fine bends near the eye, east and north: three octaves of noise, each left out once a pixel is
// too coarse for it.
fn detail_bend(p: vec2<f32>, foot: f32) -> vec2<f32> {
    let scales = array<f32, 3>(0.9, 0.33, 0.12);
    let amounts = array<f32, 3>(0.5, 0.3, 0.2);
    var b = vec2<f32>(0.0);
    for (var k = 0u; k < 3u; k++) {
        let c = scales[k];
        let keep = 1.0 - smoothstep(c * 0.25, c * 0.75, foot);
        if (keep > 0.0) {
            b += noise_slope(p + vec2<f32>(f32(k) * 13.1, f32(k) * 7.7), c) * amounts[k] * keep;
        }
    }
    return b;
}

// The ground's relief searches fewer steps than an object's: each reads every layer.
const GROUND_RELIEF_STEPS: i32 = 16;
const GROUND_RELIEF_WALK: i32 = 6;

// Whether layer `l`'s terrain type grows the drawn cover: its relief only bends the light, the
// plants hiding the rest.
fn covered(l: vec4<u32>) -> bool {
    let t = (l.z >> 16u) & 0xFFu;
    return t < 32u && ((bitcast<u32>(frame.relief.w) >> t) & 1u) != 0u;
}

// A layer's height at `lay` (1 the top, 0 the deepest) and 1 where its picture has one; a
// covered layer's is level.
fn layer_height(l: vec4<u32>, lay: Lay) -> vec2<f32> {
    if (covered(l)) {
        return vec2<f32>(0.5, 0.0);
    }
    let tiling = bitcast<f32>(l.w);
    let per_m = select(-1.0 / tiling, tiling / 24.0, tiling >= 0.0);
    let comp = companions[l.x];
    if ((comp.more.x & H_IN_ALPHA) != 0u) {
        return vec2<f32>(ground_read(l.x, per_m, lay).a, 1.0);
    }
    if ((comp.more.x & H_IN_SURFACE) != 0u && comp.maps.y != NO_TEXTURE) {
        return vec2<f32>(ground_read(comp.maps.y, per_m, lay).b, 1.0);
    }
    return vec2<f32>(0.5, 0.0);
}

// The ground's height at `lay`, the cell's masks read at `uv`: its layers' heights laid as
// `fs_terrain` lays their colours.
fn ground_height(cell: TerrainCell, lay: Lay, uv: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>) -> f32 {
    var height = 0.5;
    var has_height = 0.0;
    for (var k = 0u; k < min(cell.count, 6u); k++) {
        let l = cell.layers[k];
        let h = layer_height(l, lay);
        if (k == 0u || l.y == NO_TEXTURE) {
            height = h.x;
            has_height = h.y;
        } else if (max(h.y, has_height) > 0.0) {
            let rot = l.z & 3u;
            let a = textureSampleGrad(textures[l.y], samp_clamp, rotate_uv(uv, rot),
                                      rotate_d(dx, rot), rotate_d(dy, rot)).a;
            var w = 1.0 - a;
            if (has(F_HEIGHT_BLEND) && max(h.y, has_height) > 0.0) {
                w = height_share(w, h.x, height);
            }
            height = mix(height, h.x, w);
            has_height = max(has_height, h.y);
        }
    }
    return height;
}

// Where the ground's relief moves its pictures (`shift`, metres; `uv`, the cell's masks') and
// how much of the key light reaches the point past it, as `relief` does for an object.
struct GroundRelief {
    shift: vec3<f32>,
    uv: vec2<f32>,
    lit: f32,
    lamp: f32,
}

fn ground_relief(cell: TerrainCell, in: VsOut, lay: Lay, n: vec3<f32>, dx: vec2<f32>,
                 dy: vec2<f32>) -> GroundRelief {
    var r: GroundRelief;
    r.shift = vec3<f32>(0.0);
    r.uv = in.uv;
    r.lit = 1.0;
    r.lamp = 0.0;
    if (!has(F_RELIEF) || frame.surfaces.x <= 0.0 || lay.side >= 1.0) {
        return r;
    }
    var heights = 0u;
    for (var k = 0u; k < min(cell.count, 6u); k++) {
        let l = cell.layers[k];
        if (!covered(l)) {
            heights |= companions[l.x].more.x & (H_IN_ALPHA | H_IN_SURFACE);
        }
    }
    let to_eye = frame.camera.xyz - in.world;
    let dist = length(to_eye);
    let to = to_eye / max(dist, 1e-3);
    let facing = dot(to, n);
    let reach = frame.relief.z;
    let fade = (1.0 - smoothstep(reach * RELIEF_NEAR, reach, dist))
        * smoothstep(RELIEF_FLAT_AT, RELIEF_FULL_AT, facing) * (1.0 - lay.side);
    // The masks' coordinates per metre east and north.
    let det = lay.wdx.x * lay.wdy.y - lay.wdx.y * lay.wdy.x;
    if (heights == 0u || fade <= 0.0 || abs(det) < 1e-14) {
        return r;
    }
    let uv_e = (dx * lay.wdy.y - dy * lay.wdx.y) / det;
    let uv_n = (dy * lay.wdx.x - dx * lay.wdy.x) / det;
    let depth_m = frame.surfaces.x * fade;
    let along = -(to - n * facing) / max(facing, RELIEF_FULL_AT) * depth_m;
    let uv_along = uv_e * along.x + uv_n * along.y;
    var at = lay;
    let steps = (6.0 + 10.0 * (1.0 - facing)) * frame.relief.x;
    var was_depth = 0.0;
    var was_under = 0.0;
    var depth = 0.0;
    var hit = 0.0;
    for (var k = 0; k < GROUND_RELIEF_STEPS; k++) {
        at.p = lay.p + along * depth;
        let under = 1.0 - ground_height(cell, at, in.uv + uv_along * depth, dx, dy);
        if (depth >= under || depth >= 1.0) {
            let a = was_under - was_depth;
            let b = depth - under;
            hit = mix(was_depth, depth, clamp(a / max(a + b, 1e-4), 0.0, 1.0));
            break;
        }
        was_depth = depth;
        was_under = under;
        depth += 1.0 / steps;
    }
    r.shift = along * hit;
    r.uv = in.uv + uv_along * hit;
    let key = key_light(in.world, false);
    r.lamp = key.w;
    let l = key.xyz;
    let fl = dot(l, n);
    if (fl < 0.05 || hit <= 0.0) {
        return r;
    }
    let toward = (l - n * fl) / max(fl, 0.33) * depth_m;
    let uv_toward = uv_e * toward.x + uv_n * toward.y;
    var shade = 0.0;
    let walk = max(round(f32(GROUND_RELIEF_WALK) * frame.relief.x), 1.0);
    for (var k = 1; k <= GROUND_RELIEF_WALK; k++) {
        if (f32(k) > walk) {
            break;
        }
        let t = f32(k) / walk;
        let ray = hit * (1.0 - t);
        at.p = lay.p + r.shift + toward * hit * t;
        let under = 1.0 - ground_height(cell, at, r.uv + uv_toward * hit * t, dx, dy);
        shade = max(shade, (ray - under) * 7.0 * (1.0 - t * 0.5));
    }
    r.lit = 1.0 - RELIEF_SHADOW * clamp(shade, 0.0, 1.0) * fade;
    return r;
}

@fragment
fn fs_terrain(in: VsOut) -> GOut {
    let cell = cells[in.extra];
    if (frame.switches.y != 0u && inside_a_room(cell, in.world)) {
        discard;
    }
    let dx = dpdx(in.uv);
    let dy = dpdy(in.uv);
    let n0 = normalize(in.normal);
    var lay: Lay;
    lay.p = in.world;
    lay.wdx = dpdx(in.world);
    lay.wdy = dpdy(in.world);
    lay.side = 0.0;
    lay.east = 0.0;
    lay.turn = 0.0;
    if (has(F_TRIPLANAR)) {
        // By the triangle's own facing: the smoothed normal can face across a ridge's or a
        // foot's triangle and smear the picture along it.
        let face = normalize(cross(lay.wdx, lay.wdy)) * sign(dot(cross(lay.wdx, lay.wdy), n0));
        lay.side = 1.0 - smoothstep(TRIPLANAR_WHOLE, TRIPLANAR_FROM, face.z);
        let a = abs(face.xy) * abs(face.xy);
        lay.east = a.x / max(a.x + a.y, 1e-6);
    }
    if (has(F_MACRO)) {
        lay.turn = macro_turn(in.world.xy);
    }
    // Every picture is read where the view meets the ground's height.
    let raised = ground_relief(cell, in, lay, n0, dx, dy);
    lay.p += raised.shift;
    let uv = raised.uv;
    let blend_heights = has(F_HEIGHT_BLEND);
    var c = vec3<f32>(0.5);
    var rough = 0.9;
    var bent = vec2<f32>(0.0);
    var height = 0.5;
    var has_height = 0.0;
    var detail = 0.0;
    var heat = vec2<f32>(0.0);
    let water_drawn = has(F_WATER);
    for (var k = 0u; k < min(cell.count, 6u); k++) {
        let l = cell.layers[k];
        let kind = min((l.z >> 8u) & 0xFFu, 9u);
        var g = ground_layer(l, lay, n0.z);
        // Under water drawn as water, retail's water picture is the bed: silt, the picture's own
        // light and dark kept, and the land's mud picture's grain and bend where it has one.
        if (water_drawn && kind == 7u) {
            g = bed_layer(g, lay);
        }
        if (k == 0u || l.y == NO_TEXTURE) {
            c = g.colour;
            rough = g.rough;
            bent = g.bend;
            height = g.height;
            has_height = g.has;
            detail = DETAIL_BEND[kind];
            heat = g.heat;
        } else {
            let rot = l.z & 3u;
            // Alpha zero is the overlay's, as the integer blend has it.
            let a = textureSampleGrad(textures[l.y], samp_clamp, rotate_uv(uv, rot),
                                      rotate_d(dx, rot), rotate_d(dy, rot)).a;
            var w = 1.0 - a;
            if (blend_heights && max(g.has, has_height) > 0.0) {
                w = height_share(w, g.height, height);
            }
            c = mix(c, g.colour, w);
            rough = mix(rough, g.rough, w);
            bent = mix(bent, g.bend, w);
            height = mix(height, g.height, w);
            has_height = max(has_height, g.has);
            detail = mix(detail, DETAIL_BEND[kind], w);
            heat = mix(heat, g.heat, w);
        }
    }
    var bits = G_TERRAIN;
    if (cell.flags != 0u && !water_drawn) {
        bits |= G_WATER;
        rough = KIND_ROUGHNESS[7];
    }
    if (has(F_MACRO)) {
        c *= macro_tint(in.world.xy);
    }
    var emissive = 0.0;
    if (heat.x > 0.0) {
        let glow = molten_glow(heat, in.world.xy);
        c = mix(c, glow.rgb, heat.x);
        emissive = glow.w;
        bits |= G_GLOWS;
    }
    if (raised.lamp > 0.5) {
        bits |= G_KEY_LAMP;
    }
    // The pictures' own bends are depth mapping's, as a pack material's are on an object.
    bent *= frame.relief.y;
    if (has(F_DETAIL)) {
        let dist = distance(in.world, frame.camera.xyz);
        let fade = 1.0 - smoothstep(frame.surfaces.y * 0.4, frame.surfaces.y, dist);
        if (fade > 0.0 && detail > 0.0) {
            let foot = dist * 2.0 * frame.cluster_z.w / frame.params.w;
            bent += detail_bend(in.world.xy, foot) * detail * fade;
        }
    }
    // A bend leans the retail triangle's normal by its east and north parts.
    let n = normalize(vec3<f32>(n0.xy + bent * n0.z, n0.z));
    var g = pack(c, n, rough, 0.0, emissive, bits, in.material, in.cur, in.prev);
    g.albedo.a = raised.lit;
    return g;
}
