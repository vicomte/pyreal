// The sun's shadow casters: every instance a cascade's cull kept, drawn into that cascade's
// quarter of the atlas. The visible entry says which cascade: each cascade's list follows the
// frame's, one list long.

struct ShadowOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) material: u32,
    @location(2) @interpolate(flat) turn: f32,
    // Its level's crossfade code and the instance's hash ([`screened`]).
    @location(3) @interpolate(flat) fade: u32,
}

// The sun's bearing round an upright model's axis: a picture turned to it casts the model's
// outline, holding still as the eye moves.
fn sun_bearing(rows: array<vec4<f32>, 3>) -> f32 {
    let to = frame.sun_dir.xyz;
    let ax = vec3<f32>(rows[0].x, rows[1].x, rows[2].x);
    let ay = vec3<f32>(rows[0].y, rows[1].y, rows[2].y);
    return atan2(dot(to, ay) / max(dot(ay, ay), 1e-8), dot(to, ax) / max(dot(ax, ax), 1e-8));
}

@vertex
fn vs_shadow(v: VsIn, @builtin(instance_index) entry: u32) -> ShadowOut {
    let e = visible[entry];
    let inst = instances[e.x];
    let mode = e.y >> 24u;
    let m = billboard(array<vec4<f32>, 3>(inst.m0, inst.m1, inst.m2), mode);
    var local = v.position;
    var turn = 0.0;
    if (mode == IMPOSTOR_MODE) {
        turn = sun_bearing(m);
        local = facing(local, turn);
    }
    var world = xform(m[0], m[1], m[2], local);
    if (inst.sway != 0u && (inst.sway & SKINNED) == 0u && mode < 2u) {
        let s = unpack2x16float(inst.sway);
        let foot = vec3<f32>(m[0].w, m[1].w, m[2].w);
        let tall = s.y * length(vec3<f32>(m[0].z, m[1].z, m[2].z));
        world += sway_at(foot, v.position, v.position.z / max(s.y, 0.01), tall, frame.sun_light.w) * s.x;
    }
    let cascade = min(entry / frame.lists.x, 4u) - 1u;
    var out: ShadowOut;
    out.clip = frame.cascades[min(cascade, 3u)] * vec4<f32>(world, 1.0);
    out.uv = v.uv;
    out.material = e.y & 0xFFFFu;
    out.turn = turn;
    out.fade = fade_word(0u, (e.y >> 16u) & 0xFFu, e.x);
    return out;
}

// The shelter map (weather.rs): what the shelter's cull kept, seen straight down, each point's
// depth its height down from the map's top. Leaves, cut-out cards and turning sprites are left
// out: from above they would print their outlines on the ground as hard-edged dry patches.
@vertex
fn vs_shelter(v: VsIn, @builtin(instance_index) entry: u32) -> @builtin(position) vec4<f32> {
    let e = visible[entry];
    let inst = instances[e.x];
    let flags = materials[e.y & 0xFFFFFFu].flags;
    let mode = e.y >> 24u;
    if ((flags & (M_CUTOUT | M_FOLIAGE | M_IMPOSTOR)) != 0u || (mode >= 2u && mode <= IMPOSTOR_MODE)) {
        return vec4<f32>(2.0, 2.0, 2.0, 1.0);
    }
    let world = xform(inst.m0, inst.m1, inst.m2, v.position);
    let s = frame.shelter;
    return vec4<f32>((world.xy - s.xy) / s.z, clamp((s.w - world.z) / frame.rain.x, 0.0, 1.0), 1.0);
}

// Cut-out surfaces (leaves, fences) cut their shadows too.
@fragment
fn fs_shadow(in: ShadowOut) {
    if (screened(in.clip.xy, in.fade)) {
        discard;
    }
    let m = materials[in.material];
    if (web_card_hidden(m.flags)) {
        discard;
    }
    if ((m.flags & M_IMPOSTOR) != 0u && (m.flags & M_HAS_BASE) != 0u) {
        let at = impostor_uv(m, in.uv, in.turn, 0.5);
        // Cut at half the cutoff: a picture's blurred edges cast as densely as the shape's leaves.
        if (textureSampleLevel(textures[m.tex_base], samp_clamp, at.xy, 1.0).a < m.alpha_cutoff * 0.5) {
            discard;
        }
    } else if ((m.flags & M_CUTOUT) != 0u && (m.flags & M_HAS_BASE) != 0u) {
        let a = textureSampleLevel(textures[m.tex_base], samp_repeat, in.uv, 1.0).a * m.base_color.a;
        if (a < m.alpha_cutoff) {
            discard;
        }
    }
}
