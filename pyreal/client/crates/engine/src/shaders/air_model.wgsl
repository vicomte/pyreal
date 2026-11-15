// The atmosphere's model (render-hifi's, after Hillaire): a planet of radius `ozone.w` km under
// air reaching `ground.w` km, holding Rayleigh scatterers (air), Mie scatterers (haze) and an
// absorbing ozone layer, with the lookup tables' parametrisations. Distances in the model are
// kilometres; in the world, metres. The `air` uniform is declared by the including file.

struct Air {
    // rgb: Rayleigh scattering per km at the ground; w: -1 / its scale height (km).
    rayleigh: vec4<f32>,
    // x: Mie scattering, y: Mie extinction per km at the ground; z: -1 / scale height; w: g.
    mie: vec4<f32>,
    // rgb: ozone absorption per km at the layer's peak; w: the planet's radius, km.
    ozone: vec4<f32>,
    // rgb: the ground's albedo; w: the atmosphere's outer radius, km.
    ground: vec4<f32>,
    // xyz: toward the sun as the sky sees it; w: its illuminance over the air, scene units.
    sun: vec4<f32>,
    // x: the eye's height, km; y: how many times thicker the air before the world is drawn;
    // z: daylight, 0 to 1; w: how overcast, 0 to 1.
    eye: vec4<f32>,
    // The haze, calibrated to the world fog: x where it begins, y the fog's end (metres),
    // z its optical depth there, w its height scale (metres).
    haze: vec4<f32>,
    // rgb: the haze's own light (the fog's colour lit by the sky), scene units; w: its glow
    // toward the sun.
    haze_light: vec4<f32>,
    // The moons: toward each, and how much of its face is lit (w); the steps across its picture for a unit
    // of u and of v; its picture's slot, kind (0 none, 1 the dat's card, 2 a pack ball), night
    // lights' and clouds' slots; its light (rgb) and how much of it shows (w).
    moon_dir: array<vec4<f32>, 2>,
    moon_u: array<vec4<f32>, 2>,
    moon_v: array<vec4<f32>, 2>,
    moon_info: array<vec4<u32>, 2>,
    moon_light: array<vec4<f32>, 2>,
    // The cloud deck: its picture laid as u = a + b x, v = c + d y over metres from the eye.
    deck_laying: vec4<f32>,
    // x: its height over the eye (m); y: how opaque; z: how bright; w: its detail (Cloud Detail).
    deck: vec4<f32>,
    // x: its picture's slot (none: 0xFFFFFFFF); yzw unused.
    deck_info: vec4<u32>,
    // xy: the picture's drift, in picture units; zw unused.
    deck_scroll: vec4<f32>,
    // x: how bright the stars are; y: the twinkle's phase; z: the sky's turn (radians); w unused.
    stars: vec4<f32>,
    // rgb: the sun's disc radiance, scene units; w: the cosine of its angular radius.
    sun_disc: vec4<f32>,
    // The volumetric clouds (all zero when the deck is drawn flat): x the steps through them,
    // y their cover against the deck's, z their base over the eye and w their depth (m).
    cloud: vec4<f32>,
    // The deck's picture laid over the clouds' world: u = a + b x, v = c + d y.
    cloud_laying: vec4<f32>,
    // xy: render space's origin in the clouds' world, drifted, wrapped to the picture's period;
    // z: how far the detail has turned over (0 to 1); w: the octaves of fine detail, 1 to 3.
    cloud_offset: vec4<f32>,
    // rgb: the sun's light on them, scene units; w: how dark their bases.
    cloud_sun: vec4<f32>,
    // rgb: the sky's light about them, scene units; w: how much fine detail breaks them up.
    cloud_sky: vec4<f32>,
    // x: radians a texel of their picture spans; y: how far off they fade into the haze (m);
    // zw: the detail's periods a metre along x and y.
    cloud_march: vec4<f32>,
    // x: how far a storm has come in, 0 to 1; y: the lightning; z: how far the rain has closed
    // the sky; w: the frames laid into their history (0: started afresh).
    cloud_storm: vec4<f32>,
    // Their shadow map: xy its centre on their base, render space; z how far it reaches each way
    // (m); w how much of the sun their shadows take from the land.
    cloud_shadow: vec4<f32>,
    // rgb: the clouds' purple glow by night, scene units; w: how far the night has come in.
    cloud_glow: vec4<f32>,
}

const PI: f32 = 3.14159265358979;

fn planet_radius() -> f32 {
    return air.ozone.w;
}

fn top_radius() -> f32 {
    return air.ground.w;
}

// The nearest non-negative distance along `d` (unit) from `o` to a sphere of radius `r` about the
// planet's centre, or -1 when the ray misses it.
fn ray_sphere(o: vec3<f32>, d: vec3<f32>, r: f32) -> f32 {
    let b = dot(o, d);
    let c = dot(o, o) - r * r;
    let disc = b * b - c;
    if (disc < 0.0) {
        return -1.0;
    }
    let s = sqrt(disc);
    let near = -b - s;
    let far = -b + s;
    if (near >= 0.0) {
        return near;
    }
    if (far >= 0.0) {
        return far;
    }
    return -1.0;
}

struct Medium {
    scattering: vec3<f32>,
    extinction: vec3<f32>,
    rayleigh: vec3<f32>,
    mie: f32,
}

// The scattering medium `h` km over the ground.
fn medium(h: f32) -> Medium {
    let hh = max(h, 0.0);
    let rayleigh_density = exp(hh * air.rayleigh.w);
    let mie_density = exp(hh * air.mie.z);
    let ozone_density = max(0.0, 1.0 - abs(hh - 25.0) / 15.0);
    var m: Medium;
    m.rayleigh = air.rayleigh.rgb * rayleigh_density;
    m.mie = air.mie.x * mie_density;
    m.scattering = m.rayleigh + vec3<f32>(m.mie);
    m.extinction = m.rayleigh + vec3<f32>(air.mie.y * mie_density) + air.ozone.rgb * ozone_density;
    return m;
}

fn rayleigh_phase(mu: f32) -> f32 {
    return 3.0 / (16.0 * PI) * (1.0 + mu * mu);
}

// The Cornette-Shanks form of the Mie phase function.
fn mie_phase(mu: f32, g: f32) -> f32 {
    let g2 = g * g;
    let k = 3.0 / (8.0 * PI) * (1.0 - g2) / (2.0 + g2);
    return k * (1.0 + mu * mu) / pow(max(1.0 + g2 - 2.0 * g * mu, 1e-4), 1.5);
}

// The transmittance table maps (height, cosine of the zenith angle) so both the ground and the
// horizon get resolution.
fn transmittance_uv(r: f32, mu: f32) -> vec2<f32> {
    let bottom = planet_radius();
    let top = top_radius();
    let h = sqrt(max(top * top - bottom * bottom, 0.0));
    let rho = sqrt(max(r * r - bottom * bottom, 0.0));
    let disc = r * r * (mu * mu - 1.0) + top * top;
    let d = max(0.0, -r * mu + sqrt(max(disc, 0.0)));
    let d_min = top - r;
    let d_max = rho + h;
    return vec2<f32>((d - d_min) / max(d_max - d_min, 1e-6), rho / h);
}

fn transmittance_from_uv(uv: vec2<f32>) -> vec2<f32> {
    let bottom = planet_radius();
    let top = top_radius();
    let h = sqrt(max(top * top - bottom * bottom, 0.0));
    let rho = h * uv.y;
    let r = sqrt(rho * rho + bottom * bottom);
    let d_min = top - r;
    let d_max = rho + h;
    let d = d_min + uv.x * (d_max - d_min);
    var mu = 1.0;
    if (d > 0.0) {
        mu = (h * h - rho * rho - d * d) / (2.0 * r * d);
    }
    return vec2<f32>(r, clamp(mu, -1.0, 1.0));
}

// The sky-view table maps (cosine of the view's zenith angle, the angle to the sun about the
// vertical) with most rows near the horizon and most columns near the sun.
fn sky_view_uv(r: f32, view_zenith_cos: f32, light_view_cos: f32) -> vec2<f32> {
    let bottom = planet_radius();
    let v_horizon = sqrt(max(r * r - bottom * bottom, 0.0));
    let beta = acos(clamp(v_horizon / r, -1.0, 1.0));
    let zenith_horizon = PI - beta;
    let view_zenith = acos(clamp(view_zenith_cos, -1.0, 1.0));
    var v: f32;
    if (view_zenith < zenith_horizon) {
        var c = view_zenith / zenith_horizon;
        c = 1.0 - sqrt(max(1.0 - c, 0.0));
        v = c * 0.5;
    } else {
        let c = (view_zenith - zenith_horizon) / max(beta, 1e-6);
        v = sqrt(max(c, 0.0)) * 0.5 + 0.5;
    }
    let u = sqrt(max(-light_view_cos * 0.5 + 0.5, 0.0));
    return vec2<f32>(u, v);
}

fn sky_view_from_uv(r: f32, uv: vec2<f32>) -> vec2<f32> {
    let bottom = planet_radius();
    let v_horizon = sqrt(max(r * r - bottom * bottom, 0.0));
    let beta = acos(clamp(v_horizon / r, -1.0, 1.0));
    let zenith_horizon = PI - beta;
    var view_zenith: f32;
    if (uv.y < 0.5) {
        var c = 1.0 - 2.0 * uv.y;
        c = 1.0 - c * c;
        view_zenith = zenith_horizon * c;
    } else {
        let c = uv.y * 2.0 - 1.0;
        view_zenith = zenith_horizon + beta * c * c;
    }
    let c = uv.x * uv.x;
    return vec2<f32>(cos(view_zenith), -(c * 2.0 - 1.0));
}

// The eye's distance from the planet's centre, kept a little over the ground.
fn eye_radius() -> f32 {
    return planet_radius() + clamp(air.eye.x, 0.002, top_radius() - planet_radius() - 1.0);
}

// The cosine of `dir`'s angle to the sun about the vertical (+z).
fn light_view_cos(dir: vec3<f32>) -> f32 {
    let a = dir.xy;
    let b = air.sun.xy;
    let la = length(a);
    let lb = length(b);
    if (la < 1e-5 || lb < 1e-5) {
        return 1.0;
    }
    return clamp(dot(a, b) / (la * lb), -1.0, 1.0);
}
