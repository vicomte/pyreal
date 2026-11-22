// A player's face and hair pictures' detail (crate::face_detail), shared by the G-buffer and the
// preview spaces.

// What a face's detail gives the rest of the surface: its colour, its roughness and how much of
// it the face's regions cover, how much is skin, and its relief as a tangent-space normal.
struct Face {
    c: vec4<f32>,
    rough: f32,
    region: f32,
    skin: f32,
    t: vec3<f32>,
}

// Roughness of skin (a soft sheen), hair and the eyes (wet).
const FACE_SKIN_ROUGH: f32 = 0.52;
const FACE_HAIR_ROUGH: f32 = 0.62;
const FACE_EYE_ROUGH: f32 = 0.04;
// The largest a picture texel may be on the head, metres, for the detail worked in texels.
const FACE_TEXEL_FINE: f32 = 0.0025;
// The skin's noise sizes, metres: pores, grain and mottle.
const FACE_PORE: f32 = 0.0011;
const FACE_GRAIN: f32 = 0.0005;
const FACE_MOTTLE: f32 = 0.014;

// Smooth value noise in 3D, 0 to 1, a lattice at whole units.
fn value3(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = p - i;
    let u = f * f * (3.0 - 2.0 * f);
    var n = 0.0;
    for (var z = 0u; z < 2u; z++) {
        for (var y = 0u; y < 2u; y++) {
            for (var x = 0u; x < 2u; x++) {
                let o = vec3<f32>(f32(x), f32(y), f32(z));
                let c = vec3<i32>(i + o);
                var k = (bitcast<u32>(c.x) * 0x8DA6B343u) ^ (bitcast<u32>(c.y) * 0xD8163841u) ^ (bitcast<u32>(c.z) * 0xCB1AB31Fu);
                k = (k ^ (k >> 15u)) * 0x2C1B3C6Du;
                k = k ^ (k >> 12u);
                let w = mix(1.0 - u, u, o);
                n += f32(k & 0xFFFFu) / 65535.0 * w.x * w.y * w.z;
            }
        }
    }
    return n;
}

// How strongly the face's height bends its normal, and how much each hair strand is rounded.
const FACE_RELIEF: f32 = 4.0;
const FACE_STRAND_BEND: f32 = 0.35;

// The face tile's strands (red) turned to run along `ang`, drawn out four times along their length.
fn face_strand(tile: u32, tuv: vec2<f32>, tdx: vec2<f32>, tdy: vec2<f32>, ang: f32) -> f32 {
    let d = vec2<f32>(cos(ang), sin(ang));
    let p = vec2<f32>(-d.y, d.x);
    let at = vec2<f32>(dot(tuv, p), dot(tuv, d) * 0.25);
    let gx = vec2<f32>(dot(tdx, p), dot(tdx, d) * 0.25);
    let gy = vec2<f32>(dot(tdy, p), dot(tdy, d) * 0.25);
    return textureSampleGrad(textures[tile], samp_repeat, at, gx, gy).r;
}

// The strands at one size (`tuv` in tiles) about flow turn `q` (eighths of a half turn): how light
// they are (about 0, -2.5 to 2.5) and their slope across.
fn face_strands(tile: u32, tuv: vec2<f32>, tdx: vec2<f32>, tdy: vec2<f32>, q: f32) -> vec2<f32> {
    let i0 = floor(q);
    let a0 = i0 * 3.14159265 / 8.0;
    let a1 = a0 + 3.14159265 / 8.0;
    let s0 = face_strand(tile, tuv, tdx, tdy, a0);
    let s1 = face_strand(tile, tuv, tdx, tdy, a1);
    // Across a strand: its slope in either turn's tile a quarter texel either side.
    let n0 = vec2<f32>(-sin(a0), cos(a0)) * 0.25 / 64.0;
    let n1 = vec2<f32>(-sin(a1), cos(a1)) * 0.25 / 64.0;
    let sl0 = face_strand(tile, tuv + n0, tdx, tdy, a0) - face_strand(tile, tuv - n0, tdx, tdy, a0);
    let sl1 = face_strand(tile, tuv + n1, tdx, tdy, a1) - face_strand(tile, tuv - n1, tdx, tdy, a1);
    return vec2<f32>((mix(s0, s1, q - i0) - 0.5) * 5.0, mix(sl0, sl1, q - i0) * 5.0);
}

// A player's face or hair picture's skin and hair (crate::face_detail): the picture smoothed along
// its own edges and sharpened across them, so an enlarged picture's stair-steps become lines; stray
// hairs past the hair's edge; strands along its flow; pores, grain and mottle on its skin. The face
// and edge maps are the texture slots in the record's views word, the shared tile the metal map's.
fn face_detail(m: Material, c: vec4<f32>, uv: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>, hold: u32,
               local: vec3<f32>, wdx: vec3<f32>, wdy: vec3<f32>, border: f32) -> Face {
    let map = m.views & 0xffffu;
    let edges = m.views >> 16u;
    let tile = m.tex_metal;
    let size = vec2<f32>(textureDimensions(textures[m.tex_base]));
    let texel = 1.0 / size;
    // The face and edge maps are held at their edges: a picture's far side (a brow, the hairline)
    // never bleeds into its near border as the picture shrinks.
    // Where it is skin, hair and eyes holds at every distance.
    let f = pic_grad(map, uv, dx, dy, 3u);
    let ex = pic_grad(edges, uv, dx, dy, 3u);
    let hair = f.b;
    let eye = ex.b;
    // The whites are painted in skin entries: the eye's opening is not skin.
    let skin = min(f.a, 1.0 - hair) * (1.0 - eye);
    var out: Face;
    out.c = c;
    out.skin = skin;
    out.region = clamp(skin + hair + eye, 0.0, 1.0);
    out.rough = (skin * FACE_SKIN_ROUGH + hair * FACE_HAIR_ROUGH + eye * FACE_EYE_ROUGH) / max(skin + hair + eye, 1e-3);
    out.t = vec3<f32>(0.0, 0.0, 1.0);
    // Retail texels a screen pixel spans: what works texel by texel (the edges, stray hairs) fades
    // out as the picture shrinks below its size; the relief and the strands hold at every distance.
    let foot = max(length(dx * size), length(dy * size));
    let near = 1.0 - smoothstep(0.7, 1.5, foot);
    // A face a few pixels across (eight texels to a pixel) shows none of it: its skin, hair and
    // eyes alone, for the light.
    if (foot > 8.0) {
        return out;
    }
    // Where a texel is small on the head (the jaw's underside stretches one over several
    // millimetres) and away from the picture's border, so neighbouring pictures meet as retail
    // draws them.
    let pix_m = max(length(wdx), length(wdy));
    let texel_m = pix_m / max(foot, 1e-6);
    let fit = clamp(border, 0.0, 1.0) * (1.0 - smoothstep(FACE_TEXEL_FINE, 2.0 * FACE_TEXEL_FINE, texel_m));
    let lin = near * fit;
    // The relief: the height's slope, a texel or a pixel each way, whichever is larger.
    let wide = max(foot, 1.0);
    let ox = vec2<f32>(texel.x * wide, 0.0);
    let oy = vec2<f32>(0.0, texel.y * wide);
    let hx = (pic_grad(edges, uv + ox, dx, dy, 3u).a - pic_grad(edges, uv - ox, dx, dy, 3u).a) / wide;
    let hy = (pic_grad(edges, uv + oy, dx, dy, 3u).a - pic_grad(edges, uv - oy, dx, dy, 3u).a) / wide;
    var col = c.rgb;
    // Along the edge: five taps two texels each way; across it: the mean of either side, pushed
    // away from, so the edge is as crisp as the texels were.
    let e = ex.rg * 2.0 - 1.0;
    let clear = smoothstep(0.08, 0.35, length(e));
    if (clear > 0.0) {
        let phi = 0.5 * atan2(e.y, e.x);
        let t = vec2<f32>(cos(phi), sin(phi)) * texel;
        let n = vec2<f32>(-t.y, t.x);
        var along = col * 3.0;
        along += 2.0 * (pic_grad(m.tex_base, uv + t, dx, dy, hold).rgb + pic_grad(m.tex_base, uv - t, dx, dy, hold).rgb);
        along += pic_grad(m.tex_base, uv + 2.0 * t, dx, dy, hold).rgb + pic_grad(m.tex_base, uv - 2.0 * t, dx, dy, hold).rgb;
        along = along * m.base_color.rgb / 9.0;
        let across = m.base_color.rgb * (pic_grad(m.tex_base, uv + 0.8 * n, dx, dy, hold).rgb + pic_grad(m.tex_base, uv - 0.8 * n, dx, dy, hold).rgb);
        let soft = (along + across) / 3.0;
        let sharp = max(along + 0.6 * (along - soft), vec3<f32>(0.0));
        // On hair the strands carry the detail; sharpened, its streaks step.
        col = mix(col, sharp, clear * lin * mix(1.0, 0.2, hair));
    }
    // The shared tile repeats every 64 retail texels, four of its texels to one, and twice that,
    // and twice again, as the picture shrinks, so a strand is never finer than about a pixel.
    let a2 = f.rg * 2.0 - 1.0;
    let q = 0.5 * atan2(a2.y, a2.x) / (3.14159265 / 8.0);
    let octave = max(log2(foot / 0.7), 0.0);
    let o0 = floor(octave);
    let k0 = exp2(-o0) * size / 64.0;
    let h0 = face_strands(tile, uv * k0, dx * k0, dy * k0, q);
    let h1 = face_strands(tile, uv * k0 * 0.5, dx * k0 * 0.5, dy * k0 * 0.5, q);
    let hs = mix(h0, h1, octave - o0);
    let s = hs.x;
    let slope = hs.y;
    let i0 = floor(q);
    let across = mix(vec2<f32>(-sin(i0 * 0.3926991), cos(i0 * 0.3926991)),
                     vec2<f32>(-sin((i0 + 1.0) * 0.3926991), cos((i0 + 1.0) * 0.3926991)), q - i0);
    // Stray hairs: just outside the hair, where a strand runs, the skin takes the hair's colour
    // from a texel further in.
    if (hair > 0.04 && hair < 0.5) {
        let gx = pic_grad(map, uv + vec2<f32>(texel.x, 0.0), dx, dy, 3u).b - pic_grad(map, uv - vec2<f32>(texel.x, 0.0), dx, dy, 3u).b;
        let gy = pic_grad(map, uv + vec2<f32>(0.0, texel.y), dx, dy, 3u).b - pic_grad(map, uv - vec2<f32>(0.0, texel.y), dx, dy, 3u).b;
        let g = vec2<f32>(gx, gy);
        if (length(g) > 0.01) {
            let inside = m.base_color.rgb * pic_grad(m.tex_base, uv + normalize(g) * texel * 1.5, dx, dy, hold).rgb;
            let stray = clamp((s - 0.9) * 0.8, 0.0, 1.0) * smoothstep(0.04, 0.4, hair);
            col = mix(col, inside, stray * lin);
        }
    }
    // Skin: pores, grain and mottle from noise in the head's own frame (metres), so they run on
    // across every picture's border and never stretch; each fades as it shrinks below a pixel.
    let pore = smoothstep(0.7, 0.92, value3(local / FACE_PORE)) * (1.0 - smoothstep(0.35 * FACE_PORE, 0.7 * FACE_PORE, pix_m));
    let grain = (value3(local / FACE_GRAIN + 17.0) - 0.5) * 4.0 * (1.0 - smoothstep(0.35 * FACE_GRAIN, 0.7 * FACE_GRAIN, pix_m));
    let mottle = (value3(local / FACE_MOTTLE + 41.0) - 0.5) * 4.0 * (1.0 - smoothstep(0.15 * FACE_MOTTLE, 0.3 * FACE_MOTTLE, pix_m));
    // Hair: strands lighter and darker, the gaps between them darker still.
    let h_l = 1.0 + 0.09 * s + 0.07 * max(s - 1.4, 0.0) - 0.06 * max(-s - 1.0, 0.0);
    let s_l = 1.0 - 0.035 * pore + 0.012 * grain + 0.022 * mottle;
    // The strengths are in display values, as the pictures were painted; the colour here is linear.
    let ratio = pow(max(mix(1.0, s_l, skin) * mix(1.0, h_l, hair * fit), 0.0), 2.2);
    col *= ratio;
    out.c = vec4<f32>(col, c.a);
    // The bend: the relief's slope, and across a strand its own round, in the tangent space `bend`
    // takes on a head (y along the picture's v, as a catchlight on the eyes' domes shows).
    // The eyes' domes hold up to a picture's border (a cap's brim cuts the eye picture just over
    // the brows): flat, a wet eye would shine all over.
    let k = FACE_RELIEF * max(fit, smoothstep(0.05, 0.4, eye));
    let round = -slope * FACE_STRAND_BEND * hair * fit;
    out.t = normalize(vec3<f32>(-hx * k + round * across.x, -hy * k + round * across.y, 1.0));
    // A strand's top shines a little more than the gap beside it.
    out.rough = mix(out.rough, out.rough - 0.12 * clamp(s * 0.5, -1.0, 1.0), hair * fit);
    return out;
}
