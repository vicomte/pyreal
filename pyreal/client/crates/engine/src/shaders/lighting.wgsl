// Deferred lighting: the sun and the sky's light on every G-buffer pixel, seen through the air,
// and the heavens where there is nothing, into the HDR target (pre-exposed).

@group(1) @binding(0) var g_albedo: texture_2d<f32>;
@group(1) @binding(1) var g_normal: texture_2d<f32>;
@group(1) @binding(2) var g_material: texture_2d<f32>;
@group(1) @binding(3) var g_depth: texture_depth_2d;
@group(1) @binding(4) var cloud_picture: texture_2d<f32>;
@group(1) @binding(5) var cloud_shadows: texture_2d<f32>;
// What stands over the ground round the eye (rain.wgsl).
@group(3) @binding(10) var shelter_map: texture_depth_2d;
// The prints trodden into the snow round the eye (snow.rs): the dent and the heap round it.
@group(3) @binding(14) var trail_map: texture_2d<f32>;

// How deep a whole print goes, and how high the heap round one stands, metres.
const PRINT_DEPTH: f32 = 0.04;
const PRINT_HEAP: f32 = 0.018;
// The fresh snow's colour, linear.
const FRESH_SNOW: vec3<f32> = vec3<f32>(0.80, 0.83, 0.88);

// Where render-space `p` falls on the trail map, 0..1 across (north up), and how far toward its
// edge (0 the middle, 1 the edge).
fn trail_uv(p: vec2<f32>) -> vec3<f32> {
    let s = (p - frame.trail.xy) / frame.trail.z;
    return vec3<f32>(s.x * 0.5 + 0.5, 0.5 - s.y * 0.5, max(abs(s.x), abs(s.y)));
}

// The trodden snow's height at render-space `p` against the untrodden, metres, and its dent.
fn trail_height(p: vec2<f32>) -> vec2<f32> {
    let t = textureSampleLevel(trail_map, air_sampler, trail_uv(p).xy, 0.0).rg;
    return vec2<f32>(t.y * PRINT_HEAP - t.x * PRINT_DEPTH, t.x);
}

// The prints at `p` on snowy ground: how the ground bends there (xy, east and north), how much
// of the floor of a print is shaded from the sky (z), and how much of the sun the print's own
// walls keep off it (w).
fn trail_at(p: vec2<f32>) -> vec4<f32> {
    let e = trail_uv(p);
    if (bitcast<u32>(frame.trail.w) == 0u || e.z >= 1.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let fade = 1.0 - smoothstep(0.8, 1.0, e.z);
    let texel = 2.0 * frame.trail.z / f32(textureDimensions(trail_map).x);
    let h = trail_height(p);
    let hx = trail_height(p + vec2<f32>(texel, 0.0)).x - trail_height(p - vec2<f32>(texel, 0.0)).x;
    let hy = trail_height(p + vec2<f32>(0.0, texel)).x - trail_height(p - vec2<f32>(0.0, texel)).x;
    let slope = vec2<f32>(hx, hy) / (2.0 * texel);
    // Toward the sun across the map: what of the trodden snow stands over the line to it.
    var lit = 1.0;
    let sun = frame.sun_dir.xyz;
    let flat_len = length(sun.xy);
    if (sun.z > 0.03 && flat_len > 1e-3 && h.y > 0.02) {
        let rise = sun.z / flat_len;
        let toward = sun.xy / flat_len;
        var over = 0.0;
        for (var k = 1; k <= 5; k++) {
            let d = f32(k) * texel * 1.5;
            over = max(over, trail_height(p + toward * d).x - (h.x + d * rise));
        }
        lit = 1.0 - smoothstep(0.0, 0.01, over);
    }
    return vec4<f32>(slope * fade, h.y * fade, mix(1.0, lit, fade));
}

// Snow crystals glinting at `p` when they mirror the sun toward `v`, a pixel `foot` metres across:
// how bright.
fn snow_glint(p: vec2<f32>, v: vec3<f32>, foot: f32) -> f32 {
    let cell = 0.02;
    let o = origin_cells(cell);
    let q = p / cell + o.zw;
    let id = vec2<i32>(floor(q)) + vec2<i32>(o.xy);
    if (land_hash(id) < 0.97) {
        return 0.0;
    }
    let facet = normalize(vec3<f32>(land_hash(id + vec2<i32>(91, 37)) - 0.5,
                                    land_hash(id + vec2<i32>(13, 71)) - 0.5, 0.45));
    let half_v = normalize(frame.sun_dir.xyz + v);
    let g = pow(max(dot(facet, half_v), 0.0), 600.0);
    let seen = clamp(cell / max(foot, 1e-4), 0.0, 1.0);
    return g * seen * seen;
}

// How much of the sun reaches `world` past the clouds: their shadow map read where the sun's line
// from it meets their base, none past the map's reach.
fn cloud_shadow(world: vec3<f32>) -> f32 {
    let m = air.cloud_shadow;
    let sun = frame.sun_dir.xyz;
    if (air.cloud.x <= 0.0 || m.w <= 0.0 || sun.z < 0.05) {
        return 1.0;
    }
    let q = world.xy + sun.xy * ((frame.camera.z + air.cloud.z - world.z) / sun.z);
    let uv = (q - m.xy) / (2.0 * m.z) + 0.5;
    let edge = 1.0 - smoothstep(0.4, 0.5, max(abs(uv.x - 0.5), abs(uv.y - 0.5)));
    let through = textureSampleLevel(cloud_shadows, air_sampler, vec2<f32>(uv.x, 1.0 - uv.y), 0.0).r;
    return 1.0 - m.w * edge * (1.0 - through);
}

// The volumetric clouds over pixel `p`, enlarged from their half-size picture and softened over
// a texel each way, which hides their steps' steady offsets: premultiplied light, and cover.
fn cloud_cover(p: vec2<f32>) -> vec4<f32> {
    let uv = p / frame.params.zw;
    let texel = 0.5 / vec2<f32>(textureDimensions(cloud_picture));
    var c = textureSampleLevel(cloud_picture, air_sampler, uv, 0.0);
    c += textureSampleLevel(cloud_picture, air_sampler, uv + texel, 0.0);
    c += textureSampleLevel(cloud_picture, air_sampler, uv - texel, 0.0);
    c += textureSampleLevel(cloud_picture, air_sampler, uv + vec2<f32>(texel.x, -texel.y), 0.0);
    c += textureSampleLevel(cloud_picture, air_sampler, uv + vec2<f32>(-texel.x, texel.y), 0.0);
    return c * 0.2;
}

// The world point under pixel `p` at depth `d`.
fn unproject(p: vec2<f32>, d: f32) -> vec3<f32> {
    let ndc = vec2<f32>(p.x / frame.params.z * 2.0 - 1.0, 1.0 - p.y / frame.params.w * 2.0);
    let h = frame.inv_view_proj * vec4<f32>(ndc, d, 1.0);
    return h.xyz / h.w;
}

@fragment
fn fs_lighting(in: FsIn) -> @location(0) vec4<f32> {
    let px = vec2<i32>(in.pos.xy);
    let d = textureLoad(g_depth, px, 0);
    let dir = normalize(unproject(in.pos.xy, 1.0) - frame.camera.xyz);
    if (d <= 0.0) {
        // Nothing drawn: the sky when the outside is seen, the dark of a dungeon when not.
        if (frame.switches.x == 0u) {
            return vec4<f32>(0.0, 0.0, 0.0, 1.0);
        }
        if (has(F_ATMOSPHERE)) {
            let below = normalize(unproject(in.pos.xy + vec2<f32>(0.0, 1.0), 1.0) - frame.camera.xyz);
            let pixel = length(below - dir);
            var sky: vec3<f32>;
            if (air.cloud.x > 0.0) {
                let cl = cloud_cover(in.pos.xy);
                sky = heavens_behind(dir, pixel) * (1.0 - clamp(cl.a, 0.0, 1.0)) + cl.rgb;
            } else {
                sky = heavens(dir, pixel);
            }
            return vec4<f32>(min(sky * pre_exposure(), vec3<f32>(HDR_MOST)), 1.0);
        }
        return vec4<f32>(sky_colour(dir) * pre_exposure(), 1.0);
    }
    let world = unproject(in.pos.xy, d);
    let surface = textureLoad(g_albedo, px, 0);
    var albedo = surface.rgb;
    var n = oct_decode(textureLoad(g_normal, px, 0).xy);
    var m = textureLoad(g_material, px, 0);
    var bits = u32(m.a * 255.0 + 0.5);
    // Molten rock: its heat glowing whatever the light, the light on it a dark crust's.
    var molten = vec3<f32>(0.0);
    if ((bits & G_GLOWS) != 0u) {
        molten = molten_light(albedo, m.b);
        albedo *= MOLTEN_CRUST;
        m.b = 0.0;
        bits &= ~G_GLOWS;
    }
    let v = -dir;
    let dist = distance(world, frame.camera.xyz);
    // How open the surface is to the rain, and how much of it stands in a puddle.
    var open = 0.0;
    var puddle = 0.0;
    // Wet where the water reaches: darker, and shining; under it, the water does the mirroring,
    // taking it over across the water's thin edge.
    var covered = 0.0;
    // How snowy the land is, and what the prints in it keep of the sun.
    var snowy = 0.0;
    var print_sun = 1.0;
    if ((bits & G_INDOOR) == 0u) {
        snowy = land_snowy(world.xy);
        let wet = waterline_wet(world, pixel_foot(dist));
        albedo *= 1.0 - 0.45 * wet.x;
        m.r = mix(m.r, 0.18, wet.x);
        covered = wet.y;
        if (covered >= 1.0) {
            bits |= G_NO_MIRROR;
        }
        // The rain's, where no roof keeps it off: soaked darker and deeper in colour and a little
        // glossier, and puddles on the open ground whose mirror the puddles' pass lays on.
        if (frame.jitter.z > 0.0 || frame.rain.w > 0.0) {
            // A wall is read a step out from its face, clear of its own top.
            open = 1.0 - shelter_share(world + vec3<f32>(n.xy * 0.35, 0.0), 0.4);
            // Snow takes up the rain: no soak and no puddles on it.
            let soak = rain_soak(n) * open * (1.0 - snowy);
            albedo = max(mix(vec3<f32>(luma(albedo)), albedo, 1.0 + 0.25 * soak), vec3<f32>(0.0))
                * (1.0 - 0.4 * soak);
            m.r = mix(m.r, min(m.r, max(m.r * 0.7, 0.55)), soak);
            if ((bits & G_TERRAIN) != 0u && wet.x < 0.01 && open > 0.0) {
                let pd = puddle_at(world, n) * open * (1.0 - snowy);
                puddle = pd.x;
                // The ground under the water darker, the damp round it a little.
                albedo *= (1.0 - 0.2 * pd.y) * (1.0 - 0.5 * puddle);
                m.r = mix(m.r, 0.06, puddle);
                m.g = m.g * (1.0 - puddle);
                n = normalize(mix(n, puddle_normal(world.xy, puddle_rings(dist)), puddle));
            }
        }
    }
    // Fresh snow over the open snowy ground and the flatter tops of what stands still on it,
    // and the prints trodden into the ground.
    if (snowy > 0.0 && (bits & (G_WATER | G_MOVES)) == 0u) {
        let cover = frame.snow.y * snowy;
        if (cover > 0.0) {
            var sky_open = 1.0;
            if (frame.rain.x > 0.0) {
                sky_open = 1.0 - shelter_share(world + vec3<f32>(n.xy * 0.35, 0.0), 0.4);
            }
            var lay: f32;
            if ((bits & G_TERRAIN) != 0u) {
                lay = smoothstep(0.45, 0.8, n.z) * 0.65;
            } else {
                // Its edge on a slope wanders.
                let wobble = land_noise(world.xy + world.zz * 0.7, 0.45) - 0.5;
                lay = smoothstep(0.6 + 0.2 * wobble, 0.85 + 0.1 * wobble, n.z) * 0.9;
            }
            let k = cover * lay * sky_open;
            let fresh = FRESH_SNOW * (0.94 + 0.06 * land_noise(world.xy, 0.37));
            albedo = mix(albedo, fresh, k);
            m.r = mix(m.r, 0.6, k);
            m.g = m.g * (1.0 - k);
        }
        if ((bits & G_TERRAIN) != 0u && dist < frame.trail.z * 1.5) {
            let t = trail_at(world.xy);
            // Soft snow takes them, and ice under fresh snow.
            let soft = land_typed(world.xy, bitcast<u32>(frame.trail.w));
            let k = max(soft, frame.snow.y) * snowy;
            n = normalize(n - vec3<f32>(t.xy * k, 0.0) * n.z);
            // The floor of a print sees less of the sky: grey-blue in its shade.
            albedo *= mix(vec3<f32>(1.0), vec3<f32>(0.8, 0.84, 0.92), t.z * k);
            print_sun = mix(1.0, t.w, k);
        }
    }
    let depth = view_depth_at(world);
    var sun_vis = print_sun;
    if (has(F_SHADOWS) && (bits & G_INDOOR) == 0u && dot(n, frame.sun_dir.xyz) > -0.2) {
        sun_vis *= sun_shadow(world, n, depth, in.pos.xy) * contact_shadow(world, n, depth, in.pos.xy);
    }
    // The clouds' shadows drift over the land.
    if ((bits & G_INDOOR) == 0u && sun_vis > 0.0) {
        sun_vis *= cloud_shadow(world);
    }
    var lamp = LampLight(vec3<f32>(0.0), vec3<f32>(0.0), vec3<f32>(0.0));
    if (has(F_LAMPS)) {
        lamp = lamp_light(world, n, v, m.r, mix(vec3<f32>(0.04), albedo, m.g), bits, depth, in.pos.xy, true);
    }
    // What a surface's relief keeps from its key light: the lamps', or the sun's.
    if ((bits & G_KEY_LAMP) != 0u) {
        lamp.diffuse *= surface.a;
        lamp.specular *= surface.a;
    } else {
        sun_vis *= surface.a;
    }
    var ao = 1.0;
    if (has(F_AO)) {
        ao = ambient_occlusion(in.pos.xy);
    }
    // Past the shadow distance the sun keeps the occlusion's darkening, not the full sun.
    if (has(F_SHADOWS) && (bits & G_INDOOR) == 0u) {
        sun_vis *= mix(ao, 1.0, shadow_reach(depth));
    }
    var bounce = vec4<f32>(0.0);
    if (has(F_GI)) {
        bounce = probe_light(world, n);
    }
    var c = shade_lit(albedo, n, v, m.r, m.g, m.b, bits, sun_vis, ao, lamp.diffuse, bounce)
        + lamp.specular + albedo * lamp.glow * (LAMP_GLOW / max(pre_exposure(), 1e-6)) + molten;
    // Metal and anything as smooth as standing water mirror the sky itself, its gradient and its
    // sun side, where the rest take one colour.
    if ((m.g > 0.5 || m.r < 0.1) && has(F_ATMOSPHERE) && (bits & (G_INDOOR | G_NO_MIRROR | G_WATER | G_TERRAIN)) == 0u) {
        let r = reflect(dir, n);
        let up = smoothstep(-0.3, 0.5, r.z);
        let flat = frame.sky_light.rgb * mix(0.35, 1.0, up);
        let seen = sky_radiance(normalize(vec3<f32>(r.xy, max(r.z, 0.02))));
        let f0 = mix(vec3<f32>(0.04), albedo, m.g);
        c += env_brdf(f0, m.r, max(dot(n, v), 0.0)) * (mix(flat, seen, up) - flat) * ao
            * (1.0 - bounce.a * 0.5);
    }
    if (covered > 0.0 && covered < 1.0) {
        let under = shade_lit(albedo, n, v, m.r, m.g, m.b, bits | G_NO_MIRROR, sun_vis, ao,
                              lamp.diffuse, bounce) + lamp.specular;
        c = mix(c, under, covered);
    }
    // Snow crystals glinting in the sun near the eye.
    if (snowy > 0.0 && (bits & G_TERRAIN) != 0u && dist < 25.0 && sun_vis > 0.0 && frame.sun_dir.z > 0.0) {
        let foot = dist * 2.0 * frame.cluster_z.w / frame.params.w;
        let g = snow_glint(world.xy, v, foot) * snowy * (1.0 - smoothstep(12.0, 25.0, dist));
        c += frame.sun_light.rgb * g * sun_vis * 4.0;
    }
    // Drops splashing on the open ground near the eye, lit by the overcast.
    if (open > 0.0 && frame.rain.z > 0.0 && dist < 16.0 && n.z > 0.3) {
        let footprint = dist * 2.0 * frame.cluster_z.w / frame.params.w;
        let s = rain_splashes(world.xy, footprint) * smoothstep(0.3, 0.85, n.z)
            * (1.0 - smoothstep(7.0, 16.0, dist)) * open * frame.rain.z;
        var air_light = frame.fog.rgb;
        if (has(F_PHYSICAL)) {
            air_light = frame.sky_light.rgb;
        }
        c += air_light * s * 0.6;
    }
    if (frame.umbrae[0].w > 0.0) {
        c *= umbra(world);
    }
    if ((bits & G_INDOOR) == 0u) {
        if (has(F_ATMOSPHERE)) {
            c = aerial(c, world, dir);
        } else {
            c = mix(c, sky_colour(dir), fog_amount(distance(world.xy, frame.camera.xy)));
        }
    }
    // Alpha: one, and over it how much of the pixel is puddle, which the puddles' pass reads.
    return vec4<f32>(min(c * pre_exposure(), vec3<f32>(HDR_MOST)), 1.0 + puddle);
}
