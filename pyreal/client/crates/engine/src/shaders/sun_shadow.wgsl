// The sun's shadow on a lit point: its cascade picked by view depth (blended into the next near
// the edge, the last fading out before the shadow distance), a blocker search sizing the
// penumbra (PCSS), sixteen compared taps on a rotated disc, and short rays through the depth
// buffer near the eye for the contact shadows the cascades are too coarse to hold.

@group(3) @binding(0) var shadow_atlas: texture_depth_2d;
@group(3) @binding(1) var shadow_cmp: sampler_comparison;

// Interleaved gradient noise.
fn ign(p: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(p, vec2<f32>(0.06711056, 5.83715e-3))));
}

// The `k`th of `n` points of a Vogel disc of radius one, turned by `phi`.
fn vogel(k: u32, n: u32, phi: f32) -> vec2<f32> {
    let r = sqrt((f32(k) + 0.5) / f32(n));
    let a = f32(k) * 2.39996323 + phi;
    return vec2<f32>(cos(a), sin(a)) * r;
}

// Cascade `c`'s quarter of the atlas: its lowest corner in uv.
fn quarter(c: u32) -> vec2<f32> {
    return vec2<f32>(f32(c & 1u), f32(c >> 1u)) * 0.5;
}

// `world` in cascade `c`: its uv in the atlas and its depth.
fn in_cascade(c: u32, world: vec3<f32>) -> vec3<f32> {
    let clip = frame.cascades[c] * vec4<f32>(world, 1.0);
    let uv = clip.xy * vec2<f32>(0.5, -0.5) + 0.5;
    return vec3<f32>(uv * 0.5 + quarter(c), clip.z);
}

// How much of the sun reaches `world` (normal `n`) through cascade `c`, 0 to 1.
fn cascade_light(c: u32, world: vec3<f32>, n: vec3<f32>, noise: f32) -> f32 {
    let texel = frame.cascade_texel[c];
    let ndl = clamp(dot(n, frame.sun_dir.xyz), 0.0, 1.0);
    // Off the surface by a texel or two, more where the sun grazes it.
    let p = world + n * texel * (1.5 + 2.0 * (1.0 - ndl)) + frame.sun_dir.xyz * texel;
    let s = in_cascade(c, p);
    let lo = quarter(c);
    let hi = lo + vec2<f32>(0.5);
    if (any(s.xy <= lo) || any(s.xy >= hi) || s.z >= 1.0) {
        return 1.0;
    }
    let range = frame.cascade_depth[c];
    let texel_uv = frame.shadow.x;
    // The blocker search: what lies between the point and the sun within a few metres of it.
    let search = clamp(4.0 * frame.shadow.w / max(texel, 1e-4), 2.0, 16.0) * texel_uv;
    var blockers = 0.0;
    var count = 0.0;
    for (var k = 0u; k < 8u; k++) {
        // Kept a texel inside the quarter: `hi` itself is the next cascade's first texel.
        let uv = clamp(s.xy + vogel(k, 8u, noise * 6.2831853) * search, lo, hi - vec2<f32>(texel_uv));
        let d = textureLoad(shadow_atlas, vec2<i32>(uv * vec2<f32>(textureDimensions(shadow_atlas))), 0);
        if (d < s.z) {
            blockers += d;
            count += 1.0;
        }
    }
    if (count == 0.0) {
        return 1.0;
    }
    let gap = (s.z - blockers / count) * range;
    // The penumbra grows with the gap between blocker and receiver, as under a sun of the
    // shadow's angular size; never under a texel and a half.
    let radius = clamp(gap * frame.shadow.w / texel, 1.5, 14.0) * texel_uv;
    let bias = texel * (1.0 + 2.0 * (1.0 - ndl)) / range;
    var lit = 0.0;
    for (var k = 0u; k < 16u; k++) {
        // Half a texel inside, so the filter's 2x2 footprint stays in the quarter.
        let uv = clamp(s.xy + vogel(k, 16u, noise * 6.2831853) * radius, lo + vec2<f32>(texel_uv * 0.5),
                       hi - vec2<f32>(texel_uv * 0.5));
        lit += textureSampleCompareLevel(shadow_atlas, shadow_cmp, uv, s.z - bias);
    }
    return lit / 16.0;
}

// How much of the cascades' shadow holds `depth` metres along the view: 1 near, easing to 0 over
// the last stretch of the shadow distance.
fn shadow_reach(depth: f32) -> f32 {
    let last = frame.cascade_far.w;
    return 1.0 - smoothstep(last * (1.0 - frame.shadow.y), last, depth);
}

// The sun's light reaching `world` (normal `n`), `depth` metres along the view, 0 to 1.
fn sun_shadow(world: vec3<f32>, n: vec3<f32>, depth: f32, px: vec2<f32>) -> f32 {
    let reach = shadow_reach(depth);
    if (reach <= 0.0) {
        return 1.0;
    }
    var c = 0u;
    for (var k = 0u; k < 3u; k++) {
        if (depth >= frame.cascade_far[k]) {
            c = k + 1u;
        }
    }
    let noise = ign(px);
    var lit = cascade_light(c, world, n, noise);
    // The cascade's last stretch blends into the next, so its edge never shows.
    let far = frame.cascade_far[c];
    let start = select(0.0, frame.cascade_far[max(c, 1u) - 1u], c > 0u);
    let blend = (far - start) * frame.shadow.z;
    let f = (depth - (far - blend)) / max(blend, 1e-3);
    if (f > 0.0 && c < 3u) {
        lit = mix(lit, cascade_light(c + 1u, world, n, noise), clamp(f, 0.0, 1.0));
    }
    return mix(1.0, lit, reach);
}

// The view distance of the depth buffer at `uv` (reverse Z): along the view direction.
fn view_depth_at(world: vec3<f32>) -> f32 {
    return -(frame.view * vec4<f32>(world, 1.0)).z;
}

// Contact shadows: a short ray toward the sun through the depth buffer, near the eye only.
// `g_depth_tex` is the frame's depth.
fn contact_shadow(world_in: vec3<f32>, n: vec3<f32>, depth: f32, px: vec2<f32>) -> f32 {
    if (depth > 40.0) {
        return 1.0;
    }
    let steps = 12u;
    let len = 0.06 + depth * 0.015;
    let noise = ign(px + vec2<f32>(17.0, 3.0));
    // Off the surface, and past what the depth buffer's own steps across a slope could hide.
    let world = world_in + n * (0.02 + depth * 0.002);
    let slack = 0.03 + depth * 0.006;
    var shadow = 0.0;
    for (var k = 0u; k < steps; k++) {
        let t = (f32(k) + noise) / f32(steps) * len;
        let p = world + frame.sun_dir.xyz * t;
        let clip = frame.view_proj * vec4<f32>(p, 1.0);
        if (clip.w <= 0.0) {
            break;
        }
        let ndc = clip.xyz / clip.w;
        let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
        if (any(uv <= vec2<f32>(0.0)) || any(uv >= vec2<f32>(1.0))) {
            break;
        }
        let q = vec2<i32>(uv * frame.params.zw);
        let d = textureLoad(g_depth, q, 0);
        if (d <= 0.0) {
            continue;
        }
        // Reverse Z: a larger depth is nearer. The scene is in front of the ray's point by
        // a little, as a thin thing on the surface is, not by a whole wall.
        let scene = clip.w * ndc.z / max(d, 1e-7);
        let ahead = clip.w - scene;
        if (ahead > slack && ahead < 0.25 + t) {
            shadow = 1.0;
            break;
        }
    }
    return 1.0 - shadow * (1.0 - smoothstep(20.0, 40.0, depth));
}
