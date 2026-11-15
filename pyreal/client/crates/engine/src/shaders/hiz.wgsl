// The depth pyramid the cull pass tests against: each texel the farthest depth (the least, in
// reverse Z) of the texels below it. Level 0 halves the ground's depth prepass; each level
// halves the one before, an odd edge folded into the last texel so nothing goes untested.

// The source texels folded into destination texel `p`: two each way, three at an odd last edge.
fn span(p: vec2<u32>, src: vec2<u32>, dst: vec2<u32>) -> vec2<u32> {
    var s = vec2<u32>(2u);
    if (p.x == dst.x - 1u && (src.x & 1u) == 1u) {
        s.x = 3u;
    }
    if (p.y == dst.y - 1u && (src.y & 1u) == 1u) {
        s.y = 3u;
    }
    return s;
}
