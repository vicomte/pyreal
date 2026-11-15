// Auto-exposure: a histogram of the frame's light (the pre-exposure taken back out), its middle
// share averaged in log2, and the exposure that brings it to the key, eased toward as an eye
// adapts. A manual exposure is written as it is.

struct ExposureParams {
    // x: the histogram's lowest log2 light; y: 1 over its log2 span; zw: the target's size.
    range: vec4<f32>,
    // x: the key the mean is brought to; y, z: the least and the most exposure; w: seconds since
    // the last frame.
    key: vec4<f32>,
    // x: how fast the eye adapts toward a brighter exposure (into the dark), y: toward a dimmer
    // one, per second; z: 1 to take the target at once; w: a manual exposure (0 for automatic).
    rate: vec4<f32>,
    // x, y: the shares of the frame, darkest first, between which the mean is taken; z: the
    // key's share kept at night; w: the mean log2 light at and above which the whole key is kept.
    trim: vec4<f32>,
    // x: the share of the frame, darkest first, whose brightness is guarded; y: the most that
    // brightness may be once exposed; z: the mean log2 light over which the key rises; w: the
    // share of each stop past it the key keeps.
    guard: vec4<f32>,
}

// The most stops bright ground raises the key.
const BRIGHT_MOST: f32 = 1.0;

@group(1) @binding(0) var<uniform> ep: ExposureParams;
@group(1) @binding(1) var hdr: texture_2d<f32>;
@group(1) @binding(2) var<storage, read_write> histogram: array<atomic<u32>, 256>;
// [0] the exposure in force, [1] the mean log2 light, [2] the target, [3] unused.
@group(1) @binding(3) var<storage, read_write> state: array<f32, 4>;
// The G-buffer's material target: molten rock's glow is left out, so the rest is seen as without
// it.
@group(1) @binding(4) var g_material: texture_2d<f32>;

var<workgroup> bins: array<atomic<u32>, 256>;

// One texel in four, weighted toward the middle of the frame.
@compute @workgroup_size(16, 16)
fn build_histogram(@builtin(global_invocation_id) gid: vec3<u32>,
                   @builtin(local_invocation_index) li: u32) {
    atomicStore(&bins[li], 0u);
    workgroupBarrier();
    let px = gid.xy * 2u;
    if (f32(px.x) < ep.range.z && f32(px.y) < ep.range.w
        && (u32(textureLoad(g_material, vec2<i32>(px), 0).a * 255.0 + 0.5) & G_GLOWS) == 0u) {
        let l = luma(textureLoad(hdr, vec2<i32>(px), 0).rgb) / max(pre_exposure(), 1e-30);
        var bin = 0u;
        if (l > 1e-12) {
            bin = u32(clamp((log2(l) - ep.range.x) * ep.range.y, 0.0, 1.0) * 254.0) + 1u;
        }
        let uv = (vec2<f32>(px) + 0.5) / ep.range.zw - 0.5;
        let w = u32(mix(4.0, 1.0, clamp(length(uv) * 1.6, 0.0, 1.0)) + 0.5);
        atomicAdd(&bins[bin], w);
    }
    workgroupBarrier();
    atomicAdd(&histogram[li], atomicLoad(&bins[li]));
}

var<workgroup> counts: array<u32, 256>;

@compute @workgroup_size(256)
fn average(@builtin(local_invocation_index) i: u32) {
    counts[i] = atomicLoad(&histogram[i]);
    atomicStore(&histogram[i], 0u);
    workgroupBarrier();
    if (i != 0u) {
        return;
    }
    if (ep.rate.w > 0.0) {
        state[0] = ep.rate.w;
        state[2] = ep.rate.w;
        return;
    }
    // Black (bin 0) is left out: the empty dark of a dungeon is not what the eye looks at.
    var total = 0.0;
    for (var k = 1u; k < 256u; k++) {
        total += f32(counts[k]);
    }
    let lo = total * ep.trim.x;
    let hi = total * ep.trim.y;
    let bright = total * ep.guard.x;
    var at = 0.0;
    var sum = 0.0;
    var n = 0.0;
    var highlight = ep.range.x;
    for (var k = 1u; k < 256u; k++) {
        let c = f32(counts[k]);
        let take = max(min(at + c, hi) - max(at, lo), 0.0);
        let centre = (f32(k - 1u) + 0.5) / 254.0 / ep.range.y + ep.range.x;
        sum += take * centre;
        n += take;
        if (at < bright) {
            highlight = centre;
        }
        at += c;
    }
    let mean = select(log2(ep.key.x), sum / max(n, 1e-6), n > 0.0);
    // The dark is seen as dark: under the dimmest light the key falls, so a night is drawn
    // darker than a day, the eye adapting only so far.
    var key = ep.key.x * mix(ep.trim.z, 1.0, smoothstep(ep.trim.w - 12.0, ep.trim.w, mean));
    // Bright ground is seen as bright: past a meadow's noon the key rises, at most a stop.
    key *= exp2(min(max(mean - ep.guard.z, 0.0) * ep.guard.w, BRIGHT_MOST));
    // In the dark the brightest few hundredths (lamp pools, a brazier's glow) are kept from
    // burning out; by day the sky is let be.
    let dark = 1.0 - smoothstep(ep.trim.w - 6.0, ep.trim.w, mean);
    let guard = log2(ep.guard.y) - highlight + (1.0 - dark) * 30.0;
    let guarded = exp2(min(log2(key) - mean, guard));
    let target_e = clamp(guarded, ep.key.y, ep.key.z);
    var e = target_e;
    let prev = state[0];
    if (ep.rate.z < 0.5 && prev > 0.0) {
        let rate = select(ep.rate.y, ep.rate.x, target_e > prev);
        let k = 1.0 - exp(-ep.key.w * rate);
        e = exp2(mix(log2(prev), log2(target_e), k));
    }
    state[0] = e;
    state[1] = mean;
    state[2] = target_e;
}
