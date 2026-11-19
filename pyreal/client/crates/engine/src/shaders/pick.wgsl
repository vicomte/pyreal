// A pick's pixel: the object (plus one) and the depth there, read into four words that hold the
// pixel's x and y on the way in. A depth texture cannot be copied a texel at a time.
@group(0) @binding(0) var objects: texture_2d<u32>;
@group(0) @binding(1) var depth: texture_depth_2d;
@group(0) @binding(2) var<storage, read_write> words: array<u32, 4>;

@compute @workgroup_size(1)
fn pick() {
    let at = vec2<i32>(i32(words[0]), i32(words[1]));
    words[2] = textureLoad(objects, at, 0).r;
    words[3] = bitcast<u32>(textureLoad(depth, at, 0));
}
