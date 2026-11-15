// The sky and the air between the eye and the world, from the atmosphere's tables, in scene units:
// the physical sky toward any direction, tinted toward the dats' fog near the horizon, and the
// aerial perspective over a lit point: the air's own blue of distance, then the haze calibrated to
// the world fog's range, lit by the same sky.

@group(0) @binding(2) var sky_view_table: texture_2d<f32>;
@group(0) @binding(3) var transmittance_lut: texture_2d<f32>;
@group(0) @binding(4) var air_sampler: sampler;
@group(0) @binding(5) var<uniform> air: Air;

// The sky's radiance (times pi) toward `dir` from the sky-view table, the horizon's below it;
// gone by full night, so the sun's twilight under the horizon does not glow on it all night.
fn table_sky(dir: vec3<f32>, lowest: f32) -> vec3<f32> {
    let r = eye_radius();
    let bottom = planet_radius();
    let horizon_cos = -sqrt(max(r * r - bottom * bottom, 0.0)) / r;
    let cz = max(dir.z, max(horizon_cos + 0.002, lowest));
    let uv = sky_view_uv(r, cz, light_view_cos(dir));
    let twilight = smoothstep(0.0, 0.2, air.eye.z);
    return textureSampleLevel(sky_view_table, air_sampler, uv, 0.0).rgb * air.sun.w * PI * twilight;
}

// The dats colour the sky through their fog: toward its hue near the horizon, a little overhead.
fn fog_tinted(c: vec3<f32>, dir: vec3<f32>) -> vec3<f32> {
    let fog = frame.fog.rgb;
    let fl = luma(fog);
    if (fl <= 1e-6) {
        return c;
    }
    let tone = clamp(fog / fl, vec3<f32>(0.5), vec3<f32>(1.6));
    let w = 0.2 + 0.5 * (1.0 - smoothstep(0.0, 0.45, dir.z));
    let grey = mix(c, vec3<f32>(luma(c)), air.eye.w);
    return mix(grey, luma(c) * tone, w);
}

// The night's sky (and dusk's share of it): the dats' fog colour at the horizon deepening
// overhead, in scene units.
fn night_sky(dir: vec3<f32>) -> vec3<f32> {
    let up = clamp(dir.z, -0.2, 1.0);
    let horizon = frame.fog.rgb;
    let zenith = horizon * vec3<f32>(0.45, 0.62, 1.0) * 0.7;
    var c = mix(horizon, zenith, smoothstep(0.0, 0.6, up));
    c = mix(c, horizon * 0.7, smoothstep(0.0, -0.2, up));
    return c * frame.sky_light.w;
}

// The sky toward `dir`, without its sun, moons, stars or clouds.
fn sky_radiance(dir: vec3<f32>) -> vec3<f32> {
    return fog_tinted(table_sky(dir, -1.0), dir) + night_sky(dir) * (1.0 - air.eye.z);
}

// The sky just over the horizon toward `dir`'s bearing: what the air before the land scatters.
fn horizon_sky(dir: vec3<f32>) -> vec3<f32> {
    let d = vec3<f32>(dir.xy, max(dir.z, 0.01));
    return fog_tinted(table_sky(d, 0.01), d) + night_sky(d) * (1.0 - air.eye.z);
}

// The haze's own light toward `dir`: the fog's colour lit by the sky, glowing toward the sun,
// mostly the horizon's own colour by day so the distance meets the sky drawn over it.
fn haze_light(dir: vec3<f32>, reach: f32) -> vec3<f32> {
    let g = 0.6;
    let mu = dot(dir, air.sun.xyz);
    let lobe = pow((1.0 + g * g - 2.0 * g) / max(1.0 + g * g - 2.0 * g * mu, 1e-4), 1.5);
    let own = air.haze_light.rgb * (1.0 + air.haze_light.w * lobe * reach * air.eye.z);
    return mix(own, horizon_sky(dir), 0.6 * air.eye.z);
}

// The haze's optical depth to a point `dist` metres away and `rise` metres above the eye: none
// before it begins, growing faster than distance so the near and middle ground stay crisp, and
// thinner over high ground.
fn haze_depth(dist: f32, rise: f32) -> f32 {
    let span = max(air.haze.y - air.haze.x, 1.0);
    let x = max(dist - air.haze.x, 0.0) / span;
    var density = 1.0;
    let h = rise / max(air.haze.w, 1.0);
    if (abs(h) > 1e-3) {
        density = (1.0 - exp(-h)) / h;
    }
    return air.haze.z * pow(x, 1.7) * clamp(density, 0.5, 1.3);
}

// `c`, lit at `world`, seen through the air from the eye along `dir`.
fn aerial(c: vec3<f32>, world: vec3<f32>, dir: vec3<f32>) -> vec3<f32> {
    let dist = distance(world, frame.camera.xyz);
    // The air's own extinction at the eye's height, drawn thicker than the real air so the
    // world's short distances show the blue of distance.
    let ext = medium(air.eye.x).extinction;
    let t_air = exp(-ext * (max(dist - 80.0, 0.0) * 0.001 * air.eye.y));
    let through = c * t_air + horizon_sky(dir) * (vec3<f32>(1.0) - t_air);
    let t = exp(-haze_depth(dist, world.z - frame.camera.z));
    return mix(haze_light(dir, 1.0 - exp(-dist / 300.0)), through, t);
}
