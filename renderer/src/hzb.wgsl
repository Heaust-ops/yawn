// Depth is conventional WebGPU depth (0 near, 1 far).  Each HZB texel stores
// the maximum (farthest) depth in its footprint.  An object's nearest depth
// may therefore be rejected only when it is farther than that maximum: every
// sample represented by the texel is then in front of the object.
@group(0) @binding(0) var source_depth: texture_depth_2d;
@group(0) @binding(1) var mip0: texture_storage_2d<r32float, write>;

@compute @workgroup_size(8, 8)
fn init_main(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(mip0);
    if any(id.xy >= size) { return; }
    textureStore(mip0, id.xy, vec4<f32>(textureLoad(source_depth, id.xy, 0), 0.0, 0.0, 0.0));
}

@group(0) @binding(0) var source_hzb: texture_2d<f32>;
@group(0) @binding(1) var destination_hzb: texture_storage_2d<r32float, write>;

@compute @workgroup_size(8, 8)
fn reduce_main(@builtin(global_invocation_id) id: vec3<u32>) {
    let destination_size = textureDimensions(destination_hzb);
    if any(id.xy >= destination_size) { return; }
    let source_size = textureDimensions(source_hzb);
    let base = id.xy * 2u;
    var farthest = 0.0;
    // WebGPU floors odd mip dimensions. Fold the unpaired final source row or
    // column into the final destination texel rather than silently dropping it.
    let count = vec2<u32>(select(2u, 3u, id.x + 1u == destination_size.x && (source_size.x & 1u) != 0u), select(2u, 3u, id.y + 1u == destination_size.y && (source_size.y & 1u) != 0u));
    for (var y = 0u; y < count.y; y++) {
        for (var x = 0u; x < count.x; x++) {
            farthest = max(farthest, textureLoad(source_hzb, min(base + vec2<u32>(x, y), source_size - 1u), 0).x);
        }
    }
    textureStore(destination_hzb, id.xy, vec4<f32>(farthest, 0.0, 0.0, 0.0));
}
