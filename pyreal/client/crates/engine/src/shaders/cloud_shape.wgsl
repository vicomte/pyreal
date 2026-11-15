// The volumetric clouds' shape (render-hifi's): how much light a metre of them takes anywhere in
// their layer, shared by their march and their shadows on the land. The including file declares
// the fine detail `detail` and the sampler that wraps it, `wrap_sampler`.

// How many times thicker than the deck's own see-through a column of cloud is, straight up,
// where its cover is thin and where it is thickest.
const THIN: f32 = 1.0;
const THICK: f32 = 2.0;
// How much denser than that a cloud's own body is, so its edges read as edges.
const DENSITY: f32 = 3.0;
// The longest stretch of the layer a line is stepped through, metres.
const MOST_SPAN: f32 = 30000.0;
// The fine detail's texels across one period.
const DETAIL_TEXELS: f32 = 64.0;
// The planet's radius in metres: the sky model's, for the clouds' curve to the horizon.
const PLANET_METRES: f32 = 6360000.0;

// Where render-space `p` lies in the clouds' world: wrapped to the picture's period, drifted.
fn cloud_at(p: vec2<f32>) -> vec2<f32> {
    return p + air.cloud_offset.xy;
}

// How far along a ray climbing at `mu` (the sine of its elevation) it meets the sphere `h`
// metres over the eye: the rationalised form, so the planet's size takes no precision.
fn shell(mu: f32, h: f32) -> f32 {
    let r = PLANET_METRES;
    let k = h * (2.0 * r + h);
    return k / max(r * mu + sqrt(max(r * r * mu * mu + k, 0.0)), 1e-3);
}

// How much of the layer's depth a column of cloud reaches, from its cover.
fn column_top(c: f32) -> f32 {
    return mix(0.3, 1.0, sqrt(c));
}

// The fine detail at `q` (in its periods), read at level `fine` and faded toward its mean where
// it is finer than the footprint calls for.
fn billow(q: vec3<f32>, fine: f32) -> f32 {
    let n = textureSampleLevel(detail, wrap_sampler, q, fine).r;
    return mix(0.5, n, 1.0 - smoothstep(3.0, 5.0, fine));
}

// How much light a metre of cloud takes at render-space `p`, `h` of the way up the layer, read
// over a footprint `footprint` metres across; `octaves` of the finest detail eat its edges (none:
// the coarsest billows alone, for the sun's light through it).
fn extinction(p: vec3<f32>, h: f32, footprint: f32, octaves: u32) -> f32 {
    let slot = air.deck_info.x;
    let w = cloud_at(p.xy);
    let l = air.cloud_laying;
    let size = f32(textureDimensions(textures[slot]).x);
    let lod = max(log2(footprint * abs(l.y) * size), 0.0);
    let retail = textureSampleLevel(textures[slot], samp_repeat, vec2<f32>(l.x + l.y * w.x, l.z + l.w * w.y), lod).a;
    // The share of the sky the deck's picture covers there: its thin veils open into gaps. A
    // storm closes it into a low, heavy deck, its own shapes still in it.
    let open = smoothstep(0.3, 1.0, retail);
    // Rain closes the sky nearly over, its gaps few; a storm wholly.
    let closed = mix(open, 0.85 + 0.15 * open, air.cloud_storm.z);
    let cover = clamp(mix(closed, 0.95 + 0.05 * open, air.cloud_storm.x) * air.cloud.y, 0.0, 1.0);
    if (cover < 0.01) {
        return 0.0;
    }
    let per = air.cloud_march.zw;
    let q = vec3<f32>(w * per, h * air.cloud.w * per.x + air.cloud_offset.z);
    let fine = log2(max(footprint * DETAIL_TEXELS * per.x, 1.0));
    let share = air.cloud_sky.w;
    let n1 = billow(q, fine);
    var n2 = 0.5;
    if (octaves > 0u) {
        n2 = billow(q * 4.0 + vec3<f32>(0.37, 0.61, 0.13), fine + 2.0);
    }
    // Billows where the noise stands over what the cover leaves clear; with no detail, the cover
    // alone.
    let n = 0.5 + (0.65 * n1 + 0.35 * n2 - 0.5) * share * 1.6;
    let c = clamp((n - (1.0 - cover)) / max(cover, 0.05), 0.0, 1.0);
    if (c < 0.01) {
        return 0.0;
    }
    let top = column_top(c);
    // A flat base and a rounded top.
    let shape = smoothstep(0.0, 0.08, h) * (1.0 - smoothstep(top * 0.45, top, h));
    if (shape < 0.01) {
        return 0.0;
    }
    // The smallest billows eat into the thin edges, finer with each octave; with none, the edges
    // only draw in.
    var n3 = 0.5;
    if (octaves >= 2u) {
        n3 = billow(q * 16.0 + vec3<f32>(0.71, 0.29, 0.53), fine + 4.0);
    }
    if (octaves >= 3u) {
        n3 = mix(n3, billow(q * 48.0 + vec3<f32>(0.19, 0.83, 0.47), fine + 5.6), 0.3);
    }
    let erosion = share * 0.42 * n3;
    let s = clamp((c * shape - erosion) / max(1.0 - erosion, 0.05), 0.0, 1.0);
    let tau = -log(1.0 - 0.97 * c) * mix(THIN, THICK, c * c);
    let column = air.cloud.w * top * 0.7;
    return s * tau * DENSITY / max(column, 1.0);
}

// The Henyey-Greenstein phase, against an even spread's.
fn hg(mu: f32, g: f32) -> f32 {
    let g2 = g * g;
    return (1.0 - g2) / pow(max(1.0 + g2 - 2.0 * g * mu, 1e-4), 1.5);
}

// How much of the sun comes through the layer to the point `q` (render space) of its base: along
// the sun's line up through its depth in six steps, the cover read over `footprint` metres.
fn sun_through(q: vec2<f32>, footprint: f32) -> f32 {
    let sun = frame.sun_dir.xyz;
    let depth = air.cloud.w;
    let rise = depth / max(sun.z, 0.05);
    var tau = 0.0;
    for (var k = 0.0; k < 6.0; k += 1.0) {
        let h = (k + 0.5) / 6.0;
        let p = vec3<f32>(q + sun.xy * (h * rise), frame.camera.z + air.cloud.z + h * depth);
        tau += extinction(p, h, max(footprint, rise / 24.0), 0u) * rise / 6.0;
    }
    return exp(-tau);
}
