// The sky's lookup tables, in compute (render-hifi's, after Hillaire): the transmittance from any
// height toward any zenith angle, the light scattered more than once, and the sky as seen from the
// eye in every direction, for a sun of unit illuminance.

@group(0) @binding(0) var<uniform> air: Air;
@group(0) @binding(1) var transmittance_table: texture_2d<f32>;
@group(0) @binding(2) var multiple_table: texture_2d<f32>;
@group(0) @binding(3) var table_sampler: sampler;
@group(0) @binding(4) var table_out: texture_storage_2d<rgba16float, write>;

// The sky-view raymarch's steps.
const SKY_STEPS: i32 = 30;

fn transmittance_to_top(r: f32, mu: f32) -> vec3<f32> {
    return textureSampleLevel(transmittance_table, table_sampler, transmittance_uv(r, mu), 0.0).rgb;
}

// Sunlight reaching a point at radius `r` whose sun is at zenith cosine `mu_sun`: none when the
// planet is in the way.
fn sunlight(r: f32, mu_sun: f32) -> vec3<f32> {
    let o = vec3<f32>(0.0, 0.0, r);
    let s = vec3<f32>(sqrt(max(1.0 - mu_sun * mu_sun, 0.0)), 0.0, mu_sun);
    if (ray_sphere(o, s, planet_radius()) > 0.0) {
        return vec3<f32>(0.0);
    }
    return transmittance_to_top(r, mu_sun);
}

fn multiple_scattering(r: f32, mu_sun: f32) -> vec3<f32> {
    let u = clamp(mu_sun * 0.5 + 0.5, 0.0, 1.0);
    let v = clamp((r - planet_radius()) / (top_radius() - planet_radius()), 0.0, 1.0);
    return textureSampleLevel(multiple_table, table_sampler, vec2<f32>(u, v), 0.0).rgb;
}

fn texel_uv(id: vec2<u32>) -> vec2<f32> {
    return (vec2<f32>(id) + 0.5) / vec2<f32>(textureDimensions(table_out));
}

@compute @workgroup_size(8, 8)
fn transmittance(@builtin(global_invocation_id) id: vec3<u32>) {
    if (any(id.xy >= textureDimensions(table_out))) {
        return;
    }
    let rm = transmittance_from_uv(texel_uv(id.xy));
    let o = vec3<f32>(0.0, 0.0, rm.x);
    let d = vec3<f32>(sqrt(max(1.0 - rm.y * rm.y, 0.0)), 0.0, rm.y);
    let t_max = max(ray_sphere(o, d, top_radius()), 0.0);
    let steps = 40;
    var depth = vec3<f32>(0.0);
    let dt = t_max / f32(steps);
    for (var s = 0; s < steps; s++) {
        let p = o + d * ((f32(s) + 0.5) * dt);
        depth += medium(length(p) - planet_radius()).extinction * dt;
    }
    textureStore(table_out, id.xy, vec4<f32>(exp(-depth), 1.0));
}

// The second order of scattering from every direction over a sphere, and the fraction that
// scatters again, give the whole series of higher orders in closed form.
@compute @workgroup_size(8, 8)
fn multiple(@builtin(global_invocation_id) id: vec3<u32>) {
    if (any(id.xy >= textureDimensions(table_out))) {
        return;
    }
    let uv = texel_uv(id.xy);
    let mu_sun = uv.x * 2.0 - 1.0;
    let r = planet_radius() + clamp(uv.y, 0.0, 1.0) * (top_radius() - planet_radius());
    let o = vec3<f32>(0.0, 0.0, max(r, planet_radius() + 0.001));
    let sun = vec3<f32>(sqrt(max(1.0 - mu_sun * mu_sun, 0.0)), 0.0, mu_sun);
    let isotropic = 1.0 / (4.0 * PI);
    let n = 8;
    var second = vec3<f32>(0.0);
    var again = vec3<f32>(0.0);
    for (var a = 0; a < n; a++) {
        for (var b = 0; b < n; b++) {
            let z = 1.0 - 2.0 * (f32(a) + 0.5) / f32(n);
            let phi = 2.0 * PI * (f32(b) + 0.5) / f32(n);
            let sz = sqrt(max(1.0 - z * z, 0.0));
            let d = vec3<f32>(sz * cos(phi), sz * sin(phi), z);
            let t_ground = ray_sphere(o, d, planet_radius());
            var t_max = max(ray_sphere(o, d, top_radius()), 0.0);
            if (t_ground > 0.0) {
                t_max = t_ground;
            }
            let steps = 20;
            let dt = t_max / f32(steps);
            var trans = vec3<f32>(1.0);
            var light = vec3<f32>(0.0);
            var fraction = vec3<f32>(0.0);
            for (var s = 0; s < steps; s++) {
                let p = o + d * ((f32(s) + 0.5) * dt);
                let pr = length(p);
                let m = medium(pr - planet_radius());
                let step_t = exp(-m.extinction * dt);
                let scattered = m.scattering * sunlight(pr, dot(p / pr, sun)) * isotropic;
                let ext = max(m.extinction, vec3<f32>(1e-7));
                light += trans * (scattered - scattered * step_t) / ext;
                fraction += trans * (m.scattering - m.scattering * step_t) / ext;
                trans *= step_t;
            }
            if (t_ground > 0.0) {
                let p = o + d * t_ground;
                let mu_g = dot(normalize(p), sun);
                light += trans * sunlight(planet_radius(), mu_g) * max(mu_g, 0.0) * air.ground.rgb / PI;
            }
            second += light;
            again += fraction;
        }
    }
    let count = f32(n * n);
    let total = (second / count) / max(vec3<f32>(1.0) - again / count, vec3<f32>(1e-4));
    textureStore(table_out, id.xy, vec4<f32>(total, 1.0));
}

// The sky from the eye: single scattering of the sun by air and haze, with the higher orders
// from the table above.
@compute @workgroup_size(8, 8)
fn sky_view(@builtin(global_invocation_id) id: vec3<u32>) {
    if (any(id.xy >= textureDimensions(table_out))) {
        return;
    }
    let r = eye_radius();
    let angles = sky_view_from_uv(r, texel_uv(id.xy));
    let cos_zenith = angles.x;
    let cos_light = angles.y;
    let sin_zenith = sqrt(max(1.0 - cos_zenith * cos_zenith, 0.0));
    let sin_light = sqrt(max(1.0 - cos_light * cos_light, 0.0));
    let d = vec3<f32>(sin_zenith * cos_light, sin_zenith * sin_light, cos_zenith);
    let sun_z = clamp(air.sun.z, -1.0, 1.0);
    let sun = vec3<f32>(sqrt(max(1.0 - sun_z * sun_z, 0.0)), 0.0, sun_z);
    let o = vec3<f32>(0.0, 0.0, r);
    let t_ground = ray_sphere(o, d, planet_radius());
    var t_max = max(ray_sphere(o, d, top_radius()), 0.0);
    if (t_ground > 0.0) {
        t_max = t_ground;
    }
    t_max = min(t_max, 4000.0);
    let mu = dot(d, sun);
    let phase_r = rayleigh_phase(mu);
    let phase_m = mie_phase(mu, air.mie.w);
    var trans = vec3<f32>(1.0);
    var light = vec3<f32>(0.0);
    for (var s = 0; s < SKY_STEPS; s++) {
        // Steps grow with distance: short near the eye, long through the thin upper air.
        let f0 = f32(s) / f32(SKY_STEPS);
        let f1 = f32(s + 1) / f32(SKY_STEPS);
        let t0 = t_max * f0 * f0;
        let t1 = t_max * f1 * f1;
        let dt = t1 - t0;
        let p = o + d * (t0 + dt * 0.3);
        let pr = length(p);
        let m = medium(pr - planet_radius());
        let step_t = exp(-m.extinction * dt);
        let mu_p = dot(p / pr, sun);
        let single = (m.rayleigh * phase_r + vec3<f32>(m.mie * phase_m)) * sunlight(pr, mu_p);
        let scattered = single + multiple_scattering(pr, mu_p) * m.scattering;
        let ext = max(m.extinction, vec3<f32>(1e-7));
        light += trans * (scattered - scattered * step_t) / ext;
        trans *= step_t;
    }
    textureStore(table_out, id.xy, vec4<f32>(light, 1.0));
}
