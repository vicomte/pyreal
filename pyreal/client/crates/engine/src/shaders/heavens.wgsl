// The heavens over the physical sky: the stars, the sun's disc, the moons where the dats' sky
// objects stand them (a pack's moons as lit balls, the dats' as their own pictures, each at its
// calendar's phase) and the dats' cloud deck, drifting with the game's clock. Scene units.

fn hash2(p: vec2<u32>) -> u32 {
    var h = p.x * 1664525u + p.y * 1013904223u + 374761393u;
    h = (h ^ (h >> 15u)) * 2246822519u;
    h = (h ^ (h >> 13u)) * 3266489917u;
    return h ^ (h >> 16u);
}

fn unit_hash(h: u32) -> f32 {
    return f32(h & 0xffffu) / 65535.0;
}

// Points of light on a cube of cells round the eye, turning with the sky, twinkling. `pixel` is a
// pixel's angular size.
fn stars(dir_in: vec3<f32>, pixel: f32) -> vec3<f32> {
    if (air.stars.x <= 0.0 || dir_in.z < -0.05) {
        return vec3<f32>(0.0);
    }
    let a = air.stars.z;
    let dir = vec3<f32>(dir_in.x * cos(a) - dir_in.y * sin(a), dir_in.x * sin(a) + dir_in.y * cos(a), dir_in.z);
    let ad = abs(dir);
    var face = 0u;
    var uv = dir.yz / ad.x;
    if (ad.y >= ad.x && ad.y >= ad.z) {
        face = 1u;
        uv = dir.xz / ad.y;
    } else if (ad.z >= ad.x) {
        face = 2u;
        uv = dir.xy / ad.z;
    }
    let side = select(0u, 3u, dir[face] < 0.0) + face;
    let cells = 220.0;
    let g = (uv * 0.5 + 0.5) * cells;
    let cell = floor(g);
    var c = vec3<f32>(0.0);
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let k = cell + vec2<f32>(f32(dx), f32(dy));
            let h = hash2(vec2<u32>(vec2<i32>(k) + vec2<i32>(1000)) + vec2<u32>(side * 7919u, 0u));
            if (unit_hash(h) > 0.22) {
                continue;
            }
            let at = k + vec2<f32>(unit_hash(h >> 3u), unit_hash(h >> 9u));
            // A cell's angular size is about two over `cells` radians.
            let d = length(g - at) * (2.0 / cells) / max(pixel, 1e-5);
            let bright = pow(unit_hash(h >> 5u), 6.0);
            let tw = 0.75 + 0.25 * sin(air.stars.y * (3.0 + 4.0 * unit_hash(h >> 11u)) + f32(h & 63u));
            let warm = unit_hash(h >> 13u);
            let tint = mix(vec3<f32>(0.75, 0.85, 1.0), vec3<f32>(1.0, 0.9, 0.75), warm);
            c += tint * bright * tw * exp(-d * d * 1.5);
        }
    }
    // Low stars are lost in the air.
    return c * air.stars.x * smoothstep(-0.02, 0.25, dir_in.z);
}

// The sun's disc, limb-darkened, through the air between it and the eye.
fn sun_disc(dir: vec3<f32>) -> vec3<f32> {
    let mu = dot(dir, air.sun.xyz);
    if (mu < air.sun_disc.w || air.sun.z < -0.02) {
        return vec3<f32>(0.0);
    }
    let r = sqrt(max(1.0 - mu * mu, 0.0)) / sqrt(max(1.0 - air.sun_disc.w * air.sun_disc.w, 1e-8));
    let limb = 1.0 - 0.6 * (1.0 - sqrt(max(1.0 - r * r, 0.0)));
    let t = textureSampleLevel(transmittance_lut, air_sampler, transmittance_uv(eye_radius(), air.sun.z), 0.0).rgb;
    return air.sun_disc.rgb * t * limb * smoothstep(1.0, 0.9, r);
}

// A moon's night side: its light against the day side's.
const MOON_SHINE: f32 = 0.012;
// A pack moon's sunlit face, against the picture's own brightness; its night side is kept.
const MOON_DAY: f32 = 1.0;
// A pack moon: its height picture's tilt of its ground, against its radius.
const MOON_RELIEF: f32 = 0.03;
// The share of its sunlight that falls off as a matte ball's; the rest its dust spreads to the limb.
const MOON_MATTE: f32 = 0.35;
// How far it rocks on its ball each way, radians.
const MOON_SWAY: f32 = 0.12;
// How bright its own lights burn, against its lit face, and how much the air makes them flicker.
const MOON_LAMPS: f32 = 1.25;
const MOON_FLICKER: f32 = 0.55;
// How far over its ground its clouds lie, against its radius, and how much sun they keep off it.
const MOON_CLOUD_HEIGHT: f32 = 0.02;
const MOON_CLOUD_SHADOW: f32 = 0.75;

// Where the point `p` of a moon's unit ball lies on its pictures, longitude by latitude.
fn moon_uv(p: vec3<f32>) -> vec2<f32> {
    return vec2<f32>(atan2(p.x, p.z) / (2.0 * PI) + 0.5, 0.5 - asin(clamp(p.y, -1.0, 1.0)) / PI);
}

// The level of picture `slot` for a footprint of `foot` turns of the ball across a pixel.
fn moon_level(slot: u32, foot: f32) -> f32 {
    return max(log2(max(f32(textureDimensions(textures[slot]).x) * foot, 1e-4)), 0.0);
}

// How bright moon `k`'s light at `uv` reads through the air now: each light's own flicker, two
// quick waves and one slow, the colours a little apart; `low` more of it low in the sky.
fn moon_flicker(k: u32, uv: vec2<f32>, low: f32) -> vec3<f32> {
    let cell = vec2<u32>(floor(fract(uv) * vec2<f32>(512.0, 256.0)));
    let h0 = hash2(cell + vec2<u32>(0u, 0x5bd1u + k * 4096u));
    let h1 = hash2(vec2<u32>(h0, k + 7u));
    let ph = air.stars.y / 600.0;
    let k1 = f32(900u + (h0 >> 8u) % 1500u);
    let k2 = f32(1400u + (h1 >> 8u) % 1700u);
    let k3 = f32(30u + (h1 >> 20u) % 90u);
    let p = vec3<f32>(0.0, 0.07, 0.15);
    let wave = 0.4 * sin(2.0 * PI * (fract(k1 * ph + unit_hash(h0)) + p))
        + 0.3 * sin(2.0 * PI * (fract(k2 * ph + unit_hash(h1)) - p))
        + 0.3 * sin(2.0 * PI * (fract(k3 * ph + unit_hash(h0 >> 4u))));
    return max(vec3<f32>(1.0) + MOON_FLICKER * low * wave, vec3<f32>(0.0));
}

// Light past 0.7 of the picture's white rolled off with its hue kept.
fn moon_roll(x: vec3<f32>) -> vec3<f32> {
    let k = 0.7;
    let peak = max(x.r, max(x.g, x.b));
    if (peak <= k) {
        return x;
    }
    return x * ((k + (1.0 - k) * (1.0 - exp(-(peak - k) / (1.0 - k)))) / peak);
}

// Moon `k`: its picture's colour and how much it covers, toward `dir`; `pixel` a pixel's angle.
fn moon(k: u32, dir: vec3<f32>, pixel: f32) -> vec4<f32> {
    let info = air.moon_info[k];
    if (info.y == 0u) {
        return vec4<f32>(0.0);
    }
    let m = air.moon_dir[k];
    let mu = dot(dir, m.xyz);
    if (mu <= 0.0) {
        return vec4<f32>(0.0);
    }
    // Where the line meets the card's plane, in steps of its picture.
    let p = dir / mu - m.xyz;
    let su = air.moon_u[k].xyz;
    let sv = air.moon_v[k].xyz;
    let q = vec2<f32>(dot(p, su) / max(dot(su, su), 1e-12), dot(p, sv) / max(dot(sv, sv), 1e-12));
    let light = air.moon_light[k];
    // The ball the card's disc stands for: b runs -1..1 across the card.
    let b = q * 2.0;
    let rr = dot(b, b);
    let right = normalize(su);
    let up = normalize(sv) * -1.0;
    let toward = -m.xyz;
    let n = right * b.x - up * b.y + toward * sqrt(max(1.0 - rr, 0.0));
    // The sun's light on it at its own phase (m.w lit, the calendar's or `@moon`'s), from the
    // sun's side of the sky: toward the eye when full, away when new.
    let side = air.sun.xyz - toward * dot(air.sun.xyz, toward);
    let lean = select(up, normalize(side), length(side) > 1e-3);
    let full = 2.0 * m.w - 1.0;
    let sun = normalize(toward * full + lean * sqrt(max(1.0 - full * full, 0.0)));
    let day = smoothstep(-0.04, 0.08, dot(n, sun));
    // The night side keeps a little light from the world it circles, the most when new.
    let shine = MOON_SHINE * (0.3 + 0.7 * (1.0 - m.w));
    if (info.y == 1u) {
        let uv = q + vec2<f32>(air.moon_u[k].w, air.moon_v[k].w);
        if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) {
            return vec4<f32>(0.0);
        }
        let c = textureSampleLevel(textures[info.x], samp_clamp, uv, 0.0);
        // The dats' picture as painted, its phase drawn in; only a pack's ball takes the calendar's.
        return vec4<f32>(c.rgb * light.rgb, c.a * light.w);
    }
    let veiled = info.w != NO_TEXTURE;
    let rc = select(1.0, 1.0 + MOON_CLOUD_HEIGHT, veiled);
    if (rr >= rc * rc) {
        return vec4<f32>(0.0);
    }
    // Its own frame, rocking slowly on its ball as a moon held face-on to its world does.
    let ph = air.stars.y / 600.0;
    let sway = MOON_SWAY * vec2<f32>(
        sin(2.0 * PI * (ph + 0.37 * f32(k))),
        0.7 * sin(2.0 * PI * (2.0 * ph + 0.21 + 0.5 * f32(k))),
    );
    let e1 = right * cos(sway.x) + toward * sin(sway.x);
    let t1 = toward * cos(sway.x) - right * sin(sway.x);
    let frame = mat3x3<f32>(e1, up * cos(sway.y) + t1 * sin(sway.y), t1 * cos(sway.y) - up * sin(sway.y));
    let into = transpose(frame);
    // Turns of the ball across a pixel, more toward the limb.
    let foot = pixel / max(0.5 * length(su), 1e-6) / (2.0 * PI);
    var c = vec3<f32>(0.0);
    var a = 0.0;
    if (rr < 1.0) {
        let z = sqrt(1.0 - rr);
        let pl = into * n;
        let uv = moon_uv(pl);
        let ls = moon_level(info.x, foot / max(z, 0.15));
        let ground = textureSampleLevel(textures[info.x], samp_repeat, uv, ls);
        // Its height picture tilts the ground: a texel of the level drawn each way.
        let step = exp2(floor(ls)) / f32(textureDimensions(textures[info.x]).x);
        let h = vec4<f32>(
            textureSampleLevel(textures[info.x], samp_repeat, uv + vec2<f32>(step, 0.0), ls).a,
            textureSampleLevel(textures[info.x], samp_repeat, uv - vec2<f32>(step, 0.0), ls).a,
            textureSampleLevel(textures[info.x], samp_repeat, uv - vec2<f32>(0.0, step), ls).a,
            textureSampleLevel(textures[info.x], samp_repeat, uv + vec2<f32>(0.0, step), ls).a,
        );
        let lon = (uv.x - 0.5) * 2.0 * PI;
        let lat = asin(clamp(pl.y, -1.0, 1.0));
        let east = frame * vec3<f32>(cos(lon), 0.0, -sin(lon));
        let north = frame * vec3<f32>(-sin(lat) * sin(lon), cos(lat), -sin(lat) * cos(lon));
        let slope = vec2<f32>(
            (h.x - h.y) / (2.0 * step * 2.0 * PI * max(cos(lat), 0.05)),
            (h.z - h.w) / (2.0 * step * PI),
        );
        let normal = normalize(n - MOON_RELIEF * (east * slope.x + north * slope.y));
        // Its dust spreads the light flatter than a matte ball would.
        let matte = max(dot(normal, sun), 0.0);
        let dust = 2.0 * matte / (matte + max(dot(normal, toward), 0.05)) * smoothstep(0.0, 0.15, matte);
        var lit = mix(dust, matte, MOON_MATTE) * day;
        // The shadow of its clouds, where the sun's line from the ground meets their shell.
        if (veiled) {
            let s = into * sun;
            let bs = dot(pl, s);
            let t = -bs + sqrt(max(bs * bs - (1.0 - rc * rc), 0.0));
            let over = moon_uv(normalize(pl + s * t));
            lit *= 1.0 - MOON_CLOUD_SHADOW * textureSampleLevel(textures[info.w], samp_repeat, over, ls).a;
        }
        // Its own lights on its night side, flickering through the air, more low in the sky.
        var lamps = vec3<f32>(0.0);
        if (info.z != NO_TEXTURE) {
            let low = 1.0 + 1.5 * (1.0 - smoothstep(0.0, 0.35, m.z));
            // Each light's own point kept as the picture's levels shrink it into a pixel.
            let ll = moon_level(info.z, foot / max(sqrt(z), 0.4));
            let keep = min(exp2(moon_level(info.z, foot)), 16.0);
            lamps = textureSampleLevel(textures[info.z], samp_repeat, uv, ll).rgb * MOON_LAMPS * keep
                * moon_flicker(k, uv, low) * (1.0 - smoothstep(0.0, 0.25, lit)) * smoothstep(0.0, 0.05, z);
        }
        let night = ground.rgb * shine + lamps;
        c = mix(moon_roll(night), moon_roll(ground.rgb * lit + night), MOON_DAY) * light.rgb;
        a = smoothstep(1.0, 0.97, rr);
    }
    // Its clouds over the ground, lit a little past where the ground's day ends.
    if (veiled) {
        let zc = sqrt(max(rc * rc - rr, 0.0)) / rc;
        let nc = normalize(right * b.x / rc - up * b.y / rc + toward * zc);
        let lc = moon_level(info.w, foot / max(zc, 0.15));
        let cl = textureSampleLevel(textures[info.w], samp_repeat, moon_uv(into * nc), lc);
        let mc = dot(nc, sun);
        let lit = max(mc + 0.05, 0.0);
        let spread = 2.0 * lit / (lit + max(zc, 0.05));
        let cday = mix(spread, lit, 0.5) * smoothstep(-0.05, 0.05, mc);
        let cover = cl.a * smoothstep(rc * rc, rc * rc * 0.97, rr);
        let cn = cl.rgb * shine * 0.5;
        let cc = mix(moon_roll(cn), moon_roll(cl.rgb * cday + cn), MOON_DAY) * light.rgb;
        c = c * a * (1.0 - cover) + cc * cover;
        a = a * (1.0 - cover) + cover;
        c /= max(a, 1e-4);
    }
    return vec4<f32>(c, a * light.w);
}

// Smooth value noise over the deck's picture, a lattice of `f` cells a repeat.
fn deck_noise(uv: vec2<f32>, f: f32) -> f32 {
    let q = uv * f;
    let i = floor(q);
    let t = q - i;
    let s = t * t * (3.0 - 2.0 * t);
    let n = u32(f);
    let c = vec2<u32>(vec2<i32>(i) % vec2<i32>(i32(n)) + vec2<i32>(i32(n))) % vec2<u32>(n);
    let c1 = (c + vec2<u32>(1u)) % vec2<u32>(n);
    let a = unit_hash(hash2(c));
    let b = unit_hash(hash2(vec2<u32>(c1.x, c.y)));
    let d = unit_hash(hash2(vec2<u32>(c.x, c1.y)));
    let e = unit_hash(hash2(c1));
    return mix(mix(a, b, s.x), mix(d, e, s.x), s.y);
}

// The dats' cloud deck over the eye toward `dir`, lit by the sun and the sky: colour and cover.
fn clouds(dir: vec3<f32>, pixel: f32) -> vec4<f32> {
    let slot = air.deck_info.x;
    if (slot == NO_TEXTURE || dir.z <= 0.01 || air.deck.y <= 0.0) {
        return vec4<f32>(0.0);
    }
    let t = air.deck.x / dir.z;
    let p = dir.xy * t;
    let l = air.deck_laying;
    let uv = vec2<f32>(l.x + l.y * p.x, l.z + l.w * p.y) + air.deck_scroll.xy;
    // The level the footprint of a pixel asks for, so the far deck does not shimmer.
    let size = vec2<f32>(textureDimensions(textures[slot]));
    let footprint = t * pixel / max(dir.z, 0.05) * max(abs(l.y), abs(l.w)) * max(size.x, size.y);
    let level = max(log2(max(footprint, 1e-4)), 0.0);
    let c = textureSampleLevel(textures[slot], samp_repeat, uv, level);
    let sun = frame.sun_light.rgb * (0.35 + 0.65 * pow(max(dot(dir, air.sun.xyz), 0.0), 4.0));
    // Cloud Detail: octaves of noise over the picture eat its thin edges and thicken its middles,
    // and its thick parts shade their undersides; each octave left out once a pixel is too
    // coarse for it, so the far deck does not shimmer.
    var cover = c.a;
    var shading = 1.0;
    let octaves = u32(air.deck.w);
    if (octaves > 0u) {
        var n = 0.0;
        var norm = 0.0;
        var amp = 0.5;
        var f = 8.0;
        for (var k = 0u; k <= octaves; k++) {
            let keep = 1.0 - smoothstep(0.25, 0.6, footprint / max(size.x, size.y) * f);
            n += amp * keep * deck_noise(uv + vec2<f32>(f32(k) * 0.37, f32(k) * 0.71), f);
            norm += amp * keep;
            amp *= 0.55;
            f *= 2.0;
        }
        n = select(0.5, n / max(norm, 1e-4), norm > 1e-4);
        cover = clamp(c.a * (0.45 + 1.1 * n) - 0.12 * (1.0 - n), 0.0, 1.0);
        shading = mix(1.0, 0.68, smoothstep(0.35, 0.95, cover * n));
    }
    let lit = c.rgb * ((sun * 0.25 + frame.sky_light.rgb * 1.2) * air.deck.z * shading + air.cloud_glow.rgb);
    // Far off, toward the horizon, the deck takes the haze's light.
    let fade = smoothstep(0.02, 0.25, dir.z);
    let shade = mix(horizon_sky(dir), lit, fade);
    return vec4<f32>(shade, clamp(cover * air.deck.y, 0.0, 1.0) * mix(0.6, 1.0, fade));
}

// The sky toward `dir` with its stars, sun and moons: what the clouds lie over. The moons hide
// the stars behind them, and the air's own light lies over them, so a dark limb shows the sky.
fn heavens_behind(dir: vec3<f32>, pixel: f32) -> vec3<f32> {
    var space = stars(dir, pixel) * (1.0 - air.eye.z) + sun_disc(dir);
    for (var k = 0u; k < 2u; k++) {
        let m = moon(k, dir, pixel);
        space = mix(space, m.rgb, clamp(m.a, 0.0, 1.0));
    }
    return sky_radiance(dir) + space;
}

// Everything in the sky toward `dir`, under the dats' flat deck.
fn heavens(dir: vec3<f32>, pixel: f32) -> vec3<f32> {
    let cl = clouds(dir, pixel);
    return mix(heavens_behind(dir, pixel), cl.rgb, cl.a);
}
