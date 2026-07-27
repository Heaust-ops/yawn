struct Params { planes: array<vec4<f32>, 6>, count: u32, visible_predicate: u32, frustum_predicate: u32, _pad: u32 }
struct Instance { model: mat4x4<f32>, n0: vec4<f32>, n1: vec4<f32>, n2: vec4<f32> }
struct Aabb { min: vec4<f32>, max: vec4<f32> }
struct Meta { index_count: u32, first_index: u32, base_vertex: i32, instance_index: u32 }
struct Command { index_count: u32, instance_count: u32, first_index: u32, base_vertex: i32, first_instance: u32 }
@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> instances: array<Instance>;
@group(0) @binding(2) var<storage, read> bounds: array<Aabb>;
@group(0) @binding(3) var<storage, read> authored_visible: array<u32>;
@group(0) @binding(4) var<storage, read> metadata: array<Meta>;
@group(0) @binding(5) var<storage, read_write> frustum_flags: array<u32>;
@group(0) @binding(6) var<storage, read_write> commands: array<Command>;

@compute @workgroup_size(64)
fn frustum_cull(@builtin(global_invocation_id) id: vec3<u32>) {
  let i = id.x;
  if (i >= params.count) { return; }
  let b = bounds[i];
  let center = (b.min.xyz + b.max.xyz) * 0.5;
  let extent = (b.max.xyz - b.min.xyz) * 0.5;
  let m = instances[i].model;
  let wc = (m * vec4<f32>(center, 1.0)).xyz;
  let ax = m[0].xyz * extent.x;
  let ay = m[1].xyz * extent.y;
  let az = m[2].xyz * extent.z;
  var inside = 1u;
  for (var p = 0u; p < 6u; p++) {
    let plane = params.planes[p];
    let radius = abs(dot(plane.xyz, ax)) + abs(dot(plane.xyz, ay)) + abs(dot(plane.xyz, az));
    if (dot(plane.xyz, wc) + plane.w + radius < 0.0) { inside = 0u; }
  }
  frustum_flags[i] = 1u - inside;
}

fn matches(value: u32, predicate: u32) -> bool {
  return predicate == 0u || (predicate == 1u && value != 0u) || (predicate == 2u && value == 0u);
}

@compute @workgroup_size(64)
fn mesh_query(@builtin(global_invocation_id) id: vec3<u32>) {
  let i = id.x;
  if (i >= params.count) { return; }
  let draw_meta = metadata[i];
  var selected = true;
  if (params.visible_predicate != 0u) {
    selected = matches(authored_visible[i], params.visible_predicate);
  }
  if (params.frustum_predicate != 0u) {
    selected = selected && matches(frustum_flags[i], params.frustum_predicate);
  }
  commands[i] = Command(draw_meta.index_count, select(0u, 1u, selected), draw_meta.first_index, draw_meta.base_vertex, 0u);
}
