struct CullUniform {
    view_proj: mat4x4<f32>,
    candidate_count: u32,
    occlusion_enabled: u32,
    hzb_mip_count: u32,
    bypass_occlusion: u32,
    viewport: vec2<u32>,
    depth_bias: f32,
    minimum_extent: f32,
}

struct LocalBounds {
    min: vec3<f32>,
    state: u32,
    max: vec3<f32>,
    _padding: u32,
}

struct Candidate {
    bounds_index: u32,
    model_index: u32,
    draw_index: u32,
    flags: u32,
}

struct DrawIndexedIndirect {
    index_count: u32,
    instance_count: atomic<u32>,
    first_index: u32,
    base_vertex: i32,
    first_instance: u32,
}

@group(0) @binding(0) var<uniform> cull: CullUniform;
@group(0) @binding(1) var<storage, read> bounds: array<LocalBounds>;
@group(0) @binding(2) var<storage, read> models: array<mat4x4<f32>>;
@group(0) @binding(3) var<storage, read> candidates: array<Candidate>;
@group(0) @binding(4) var<storage, read_write> draws: array<DrawIndexedIndirect>;
@group(0) @binding(5) var previous_hzb: texture_2d<f32>;

// WebGPU's clip volume is -w <= x,y <= w and 0 <= z <= w. Testing the
// transformed world AABB's eight corners is conservative and exactly matches
// rh_yup::perspective_wgpu_dx; no plane-sign convention is implicit here.
fn intersects_frustum(center: vec3<f32>, extent: vec3<f32>) -> bool {
    var outside_left = true;
    var outside_right = true;
    var outside_bottom = true;
    var outside_top = true;
    var outside_near = true;
    var outside_far = true;
    for (var i = 0u; i < 8u; i++) {
        let sign = vec3<f32>(
            select(-1.0, 1.0, (i & 1u) != 0u),
            select(-1.0, 1.0, (i & 2u) != 0u),
            select(-1.0, 1.0, (i & 4u) != 0u),
        );
        let p = cull.view_proj * vec4<f32>(center + sign * extent, 1.0);
        outside_left = outside_left && p.x < -p.w;
        outside_right = outside_right && p.x > p.w;
        outside_bottom = outside_bottom && p.y < -p.w;
        outside_top = outside_top && p.y > p.w;
        outside_near = outside_near && p.z < 0.0;
        outside_far = outside_far && p.z > p.w;
    }
    return !(outside_left || outside_right || outside_bottom || outside_top || outside_near || outside_far);
}

// Fail-open projected AABB test. All corners must be in front of the near
// plane and fully on-screen. The selected mip makes the rectangle span at
// most two texels per axis; the inclusive 4x4 loop covers boundary rounding.
fn proven_occluded(center: vec3<f32>, extent: vec3<f32>) -> bool {
    var lo = vec2<f32>(1.0);
    var hi = vec2<f32>(0.0);
    var nearest = 1.0;
    for (var i = 0u; i < 8u; i++) {
        let sign = vec3<f32>(select(-1.0, 1.0, (i & 1u) != 0u), select(-1.0, 1.0, (i & 2u) != 0u), select(-1.0, 1.0, (i & 4u) != 0u));
        let p = cull.view_proj * vec4<f32>(center + sign * extent, 1.0);
        if p.w <= 0.00001 || p.z < 0.0 { return false; }
        let uv = vec2<f32>(p.x / p.w * 0.5 + 0.5, 0.5 - p.y / p.w * 0.5);
        if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) { return false; }
        lo = min(lo, uv); hi = max(hi, uv); nearest = min(nearest, p.z / p.w);
    }
    let pixels = (hi - lo) * vec2<f32>(cull.viewport);
    if max(pixels.x, pixels.y) < cull.minimum_extent { return false; }
    let mip = min(u32(max(0.0, floor(log2(max(1.0, max(pixels.x, pixels.y)))) - 1.0)), cull.hzb_mip_count - 1u);
    let size = textureDimensions(previous_hzb, mip);
    // Mips fold odd source edges, so map full-resolution pixels by 2^mip
    // rather than scaling UVs by the rounded-down mip dimensions.
    let divisor = exp2(f32(mip));
    let first = min(vec2<u32>(floor(lo * vec2<f32>(cull.viewport) / divisor)), size - 1u);
    let last = min(vec2<u32>(floor(hi * vec2<f32>(cull.viewport) / divisor)), size - 1u);
    if any(last - first >= vec2<u32>(4u)) { return false; }
    var farthest = 0.0;
    for (var y = 0u; y < 4u; y++) { for (var x = 0u; x < 4u; x++) {
        let p = first + vec2<u32>(x, y);
        if all(p <= last) { farthest = max(farthest, textureLoad(previous_hzb, p, mip).x); }
    }}
    return nearest > farthest + cull.depth_bias;
}

@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= cull.candidate_count {
        return;
    }
    let candidate = candidates[id.x];
    let local = bounds[candidate.bounds_index];
    // State 1 is an accepted finite, non-empty AABB. Pending, empty, invalid,
    // missing (encoded pending by the CPU), and ALWAYS_VISIBLE are fail-open.
    var visible = local.state != 1u || (candidate.flags & 16u) != 0u;
    if !visible {
        let model = models[candidate.model_index];
        let local_center = local.min * 0.5 + local.max * 0.5;
        let local_extent = (local.max - local.min) * 0.5;
        let center = (model * vec4<f32>(local_center, 1.0)).xyz;
        let extent = abs(model[0].xyz) * local_extent.x
            + abs(model[1].xyz) * local_extent.y
            + abs(model[2].xyz) * local_extent.z;
        visible = intersects_frustum(center, extent);
        if visible && cull.occlusion_enabled != 0u && cull.bypass_occlusion == 0u {
            visible = !proven_occluded(center, extent);
        }
    }
    atomicStore(&draws[candidate.draw_index].instance_count, select(0u, 1u, visible));
}
