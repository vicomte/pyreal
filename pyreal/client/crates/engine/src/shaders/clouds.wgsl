// The volumetric clouds (render-hifi's): a layer of air over the eye, its cover read from the
// dats' deck picture laid over the world and drifting with the clock, broken into billows by a
// tiling noise fixed to the world, lit by the sun through itself and by the sky about it. Worked
// out at two thirds of the frame's size into a picture of their own: premultiplied light in scene units, and how much
// of what lies behind them they hide. Each pixel's steps turn over frame by frame, which their
// own history smooths (cloud_resolve.wgsl).

@group(1) @binding(0) var g_depth: texture_depth_2d;
@group(1) @binding(1) var cloud_out: texture_storage_2d<rgba16float, write>;
@group(1) @binding(2) var detail: texture_3d<f32>;
@group(1) @binding(3) var wrap_sampler: sampler;
@group(1) @binding(4) var shadow_out: texture_storage_2d<rgba16float, write>;

// How much of their colour they lose at full night (clouds.rs's `NIGHT_GREY`).
const NIGHT_GREY: f32 = 0.6;

// The clouds' shadow map: the sun through them to each texel of their base round the eye.
@compute @workgroup_size(8, 8)
fn cloud_shadows(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(shadow_out);
    if (any(id.xy >= size)) {
        return;
    }
    let m = air.cloud_shadow;
    if (m.w <= 0.0) {
        textureStore(shadow_out, id.xy, vec4<f32>(1.0));
        return;
    }
    let uv = (vec2<f32>(id.xy) + 0.5) / vec2<f32>(size);
    let q = m.xy + (vec2<f32>(uv.x, 1.0 - uv.y) * 2.0 - 1.0) * m.z;
    let texel = 2.0 * m.z / f32(size.x);
    textureStore(shadow_out, id.xy, vec4<f32>(sun_through(q, texel)));
}

@compute @workgroup_size(8, 8)
fn clouds(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(cloud_out);
    if (any(id.xy >= size)) {
        return;
    }
    // Only where the sky shows in the full-size pixels this texel's upsampling reaches.
    let full = vec2<i32>(textureDimensions(g_depth));
    let ratio = vec2<f32>(full) / vec2<f32>(size);
    let base_px = vec2<i32>(vec2<f32>(id.xy) * ratio);
    var open = false;
    for (var dy = -1; dy <= 2; dy++) {
        for (var dx = -1; dx <= 2; dx++) {
            let q = clamp(base_px + vec2<i32>(dx, dy), vec2<i32>(0), full - 1);
            if (textureLoad(g_depth, q, 0) <= 0.0) {
                open = true;
            }
        }
    }
    let up = air.cloud.x;
    if (!open || up <= 0.0) {
        textureStore(cloud_out, id.xy, vec4<f32>(0.0));
        return;
    }
    let px = (vec2<f32>(id.xy) + 0.5) * ratio;
    let ndc = vec2<f32>(px.x / frame.params.z * 2.0 - 1.0, 1.0 - px.y / frame.params.w * 2.0);
    let hp = frame.inv_view_proj * vec4<f32>(ndc, 1.0, 1.0);
    let d = normalize(hp.xyz / hp.w - frame.camera.xyz);
    // Rays that never climb into the layer see none; the land covers them.
    if (d.z < -0.01) {
        textureStore(cloud_out, id.xy, vec4<f32>(0.0));
        return;
    }
    let mu = max(d.z, 0.0);
    let depth = air.cloud.w;
    let t0 = shell(mu, air.cloud.z);
    let t1 = shell(mu, air.cloud.z + depth);
    // Low down the line runs through tens of kilometres of the layer; the first stretch shows.
    // Straight up it crosses the layer alone, in fewer steps.
    let span = min(t1 - t0, MOST_SPAN);
    let steps = max(round(up * clamp(sqrt(span / depth) / 3.0, 0.6, 1.0)), 4.0);
    let dt = span / steps;
    let haze = haze_light(d, 1.0);
    // Each pixel's own offset into its first step, so the steps do not band, turning over frame
    // by frame (a golden-ratio walk) so their history smooths the grain.
    let ign = fract(52.9829189 * fract(dot(vec2<f32>(id.xy), vec2<f32>(0.06711056, 5.83715e-3))));
    let jitter = fract(ign + 0.618034 * air.cloud_storm.w);
    let sun = frame.sun_dir.xyz;
    let cos_sun = dot(d, sun);
    let phase = mix(hg(cos_sun, 0.5), hg(cos_sun, -0.2), 0.5);
    let lit = air.cloud_sun.rgb;
    let ambient = air.cloud_sky.rgb;
    let shade = air.cloud_sun.w;
    let octaves = u32(air.cloud_offset.w);
    // The clouds' own hue: the deck picture's, read whole from its smallest level, near white.
    let whole = textureSampleLevel(textures[air.deck_info.x], samp_repeat, vec2<f32>(0.5), 16.0).rgb;
    let tinted = 0.9 * whole / max(max(whole.r, max(whole.g, whole.b)), 1e-3);
    // The night deck's purple greyed: their glow is the night's only purple.
    let albedo = mix(tinted, vec3<f32>(luma(tinted)), NIGHT_GREY * air.cloud_glow.w);
    // A storm's cloud is dark and slate grey, the darker toward its base.
    let storm = air.cloud_storm.x;
    let slate = vec3<f32>(luma(albedo)) * vec3<f32>(0.9, 0.96, 1.06);
    let albedo_drawn = mix(albedo, slate, storm);
    // Lightning lights the cloud from within.
    let struck = ambient * vec3<f32>(0.78, 0.82, 1.0) * 6.0 * air.cloud_storm.y;
    var transmittance = 1.0;
    var light = vec3<f32>(0.0);
    for (var k = 0.0; k < steps; k += 1.0) {
        let t = t0 + (k + jitter) * dt;
        let p = frame.camera.xyz + d * t;
        let h = clamp((k + jitter) / steps * (span / max(t1 - t0, 1.0)), 0.0, 1.0);
        // How wide the pixel's cone is there, and how long the step.
        let footprint = max(t * air.cloud_march.x, dt * 0.25 * (1.0 - mu));
        let sigma = extinction(p, h, footprint, octaves);
        if (sigma <= 0.0) {
            continue;
        }
        // The sun through the cloud toward it, in three widening steps.
        var tau = 0.0;
        var at = p;
        var ah = h;
        for (var j = 0; j < 3; j += 1) {
            let len = 90.0 * pow(3.0, f32(j));
            at = at + sun * len;
            ah = ah + sun.z * len / depth;
            if (ah > 1.0 || ah < 0.0) {
                break;
            }
            tau += extinction(at, ah, max(footprint, len * 0.5), 0u) * len;
        }
        // Light scattered many times keeps more of the sun than the beam alone.
        let beam = max(exp(-tau), 0.6 * exp(-0.2 * tau));
        // The dark rim a cloud's thin edge shows away from the sun, its lit rim toward it.
        let powder = 1.0 - 0.6 * exp(-2.0 * sigma * 60.0);
        let direct = lit * beam * phase * powder;
        // The sky lights a cloud's top more than its base.
        let sky = ambient * mix(1.0 - 0.3 * shade, 1.0 + 0.2 * shade, h);
        let dark = mix(1.0, mix(0.25, 0.6, h), storm);
        // Far off the clouds take the haze's light, as the land does.
        let step_light = mix((direct + sky) * albedo_drawn * dark + struck + air.cloud_glow.rgb, haze, 1.0 - exp(-t / air.cloud_march.y));
        let a = 1.0 - exp(-sigma * dt);
        light += transmittance * a * step_light;
        transmittance *= 1.0 - a;
        if (transmittance < 0.01) {
            break;
        }
    }
    // They reach down to the horizon, and show as much as the dats' hours show their deck.
    let low = smoothstep(-0.01, 0.0, d.z) * clamp(air.deck.y, 0.0, 1.0);
    textureStore(cloud_out, id.xy, vec4<f32>(light * low, (1.0 - transmittance) * low));
}
