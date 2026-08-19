export const gltfShader = /* wgsl */ `
struct UniformData { resolution: vec2<f32>, time: f32, _padding0: f32, camera_position: vec4<f32> }
struct MaterialData { base_color_factor: vec4<f32>, emissive_factor: vec4<f32>, surface_factors: vec4<f32>, alpha_optics: vec4<f32>, flags: vec4<u32>, uv_sets: vec4<u32>, debug_extras: vec4<u32> }
@group(0) @binding(0) var<uniform> uni: UniformData;
@group(1) @binding(0) var<uniform> view_proj: mat4x4<f32>;
@group(2) @binding(0) var<uniform> material: MaterialData;
@group(2) @binding(1) var base_tex: texture_2d<f32>;
@group(2) @binding(2) var mr_tex: texture_2d<f32>;
@group(2) @binding(3) var normal_tex: texture_2d<f32>;
@group(2) @binding(4) var occlusion_tex: texture_2d<f32>;
@group(2) @binding(5) var emissive_tex: texture_2d<f32>;
@group(2) @binding(6) var base_sampler: sampler;
@group(2) @binding(7) var mr_sampler: sampler;
@group(2) @binding(8) var normal_sampler: sampler;
@group(2) @binding(9) var occlusion_sampler: sampler;
@group(2) @binding(10) var emissive_sampler: sampler;
struct VertexInput { @location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>, @location(3) model_col0: vec4<f32>, @location(4) model_col1: vec4<f32>, @location(5) model_col2: vec4<f32>, @location(6) model_col3: vec4<f32>, @location(7) normal_col0: vec4<f32>, @location(8) normal_col1: vec4<f32>, @location(9) normal_col2: vec4<f32>, @location(10) tangent: vec4<f32> }
struct VertexOutput { @builtin(position) clip_position: vec4<f32>, @location(0) world_pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) tangent: vec3<f32>, @location(3) bitangent: vec3<f32>, @location(4) uv: vec2<f32>, @location(5) @interpolate(flat) determinant_sign: f32 }
fn safe_normalize(v: vec3<f32>, fallback: vec3<f32>) -> vec3<f32> { let l2 = dot(v, v); return select(fallback, v * inverseSqrt(l2), l2 > 1e-12 && l2 < 1e30); }
@vertex fn vs_main(in: VertexInput) -> VertexOutput {
  var out: VertexOutput; let model = mat4x4<f32>(in.model_col0, in.model_col1, in.model_col2, in.model_col3);
  let linear = mat3x3<f32>(in.model_col0.xyz, in.model_col1.xyz, in.model_col2.xyz); let nm = mat3x3<f32>(in.normal_col0.xyz, in.normal_col1.xyz, in.normal_col2.xyz);
  let world = model * vec4<f32>(in.pos, 1.0); let n = safe_normalize(nm * in.normal, vec3<f32>(0,1,0)); let raw_t = linear * in.tangent.xyz;
  var t = raw_t - n * dot(n, raw_t); if dot(t,t) < 1e-8 { t = cross(select(vec3<f32>(0,1,0), vec3<f32>(1,0,0), abs(n.x) < 0.9), n); } t = safe_normalize(t, vec3<f32>(1,0,0));
  out.clip_position = view_proj * world; out.world_pos = world.xyz; out.normal = n; out.tangent = t; out.bitangent = safe_normalize(cross(n,t), vec3<f32>(0,0,1)) * in.tangent.w * in.normal_col0.w; out.uv = in.uv; out.determinant_sign = in.normal_col0.w; return out;
}
struct Closure { base: vec4<f32>, mr: vec2<f32>, normal_map: vec3<f32>, ao: f32, emissive: vec3<f32> }
fn sample_closure(uv: vec2<f32>) -> Closure {
  let bits = material.flags.x; var c: Closure;
  c.base = material.base_color_factor * select(vec4<f32>(1), textureSample(base_tex, base_sampler, uv), (bits & 1u) != 0u);
  let mr = select(vec4<f32>(1), textureSample(mr_tex, mr_sampler, uv), (bits & 2u) != 0u); c.mr = vec2<f32>(clamp(material.surface_factors.x * mr.b,0,1), clamp(material.surface_factors.y * mr.g,0.045,1));
  c.normal_map = select(vec3<f32>(0.5,0.5,1), textureSample(normal_tex, normal_sampler, uv).xyz, (bits & 4u) != 0u);
  let occ = select(1.0, textureSample(occlusion_tex, occlusion_sampler, uv).r, (bits & 8u) != 0u); c.ao = mix(1.0, occ, material.surface_factors.w);
  c.emissive = material.emissive_factor.rgb * select(vec3<f32>(1), textureSample(emissive_tex, emissive_sampler, uv).rgb, (bits & 16u) != 0u); return c;
}
fn schlick(f0: vec3<f32>, v_h: f32) -> vec3<f32> { return f0 + (vec3<f32>(1)-f0) * pow(1.0-clamp(v_h,0,1),5.0); }
fn ggx_d(n_h_input: f32, a: f32) -> f32 { let n_h=clamp(n_h_input,0.0,1.0); let a2=a*a; let nh2=n_h*n_h; let q=(1.0-nh2)+a2*nh2; return a2/(3.14159265*q*q); }
fn smith_v(n_v: f32, n_l: f32, a: f32) -> f32 { let a2=a*a; let gv=n_l*sqrt(max(n_v*n_v*(1.0-a2)+a2,0)); let gl=n_v*sqrt(max(n_l*n_l*(1.0-a2)+a2,0)); return 0.5/max(gv+gl,1e-6); }
@fragment fn fs_main(in: VertexOutput, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
  let c=sample_closure(in.uv); if material.alpha_optics.x == 1.0 && c.base.a < material.alpha_optics.y { discard; }
  let physical_front=front == (in.determinant_sign > 0); let orientation=select(-1.0,1.0,physical_front || material.flags.y == 0u); let map=c.normal_map*2.0-1.0;
  let n=safe_normalize(mat3x3<f32>(in.tangent,in.bitangent,in.normal)*safe_normalize(vec3<f32>(map.xy*material.surface_factors.z,map.z),vec3<f32>(0,0,1)),in.normal)*orientation;
  let v=safe_normalize(uni.camera_position.xyz-in.world_pos,n); let l=safe_normalize(vec3<f32>(0.35,1,0.45),vec3<f32>(0,1,0)); let h=safe_normalize(v+l,n);
  let nv=max(dot(n,v),0); let nl=max(dot(n,l),0); let nh=dot(n,h); let vh=max(dot(v,h),0); let a=c.mr.y*c.mr.y;
  let f0=mix(vec3<f32>(material.alpha_optics.w),c.base.rgb,c.mr.x); let direct_f=schlick(f0,vh); let env_f=schlick(f0,nv); let spec=direct_f*ggx_d(nh,a)*smith_v(nv,nl,a); let diffuse=(vec3<f32>(1)-direct_f)*(1.0-c.mr.x)*c.base.rgb/3.14159265;
  let sun=(diffuse+spec)*nl*vec3<f32>(3.0,2.85,2.65); let up=clamp(n.y*0.5+0.5,0,1); let sky=mix(vec3<f32>(0.055,0.045,0.035),vec3<f32>(0.24,0.36,0.58),up);
  let reflection=reflect(-v,n); let horizon=clamp(reflection.y*0.5+0.5,0,1); let env_spec=env_f*mix(vec3<f32>(0.04,0.035,0.03),vec3<f32>(0.28,0.42,0.7),horizon)*(1.0-0.65*c.mr.y);
  let color=sun+(vec3<f32>(1)-env_f)*(1.0-c.mr.x)*c.base.rgb*sky*c.ao+env_spec*c.ao+c.emissive; return vec4<f32>(color,1.0);
}`;

export const groundShader = /* wgsl */ `
@group(0) @binding(0) var<uniform> application: array<vec4<f32>, 3>;
@group(1) @binding(0) var<uniform> view_proj: mat4x4<f32>;
struct Input { @location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>, @location(3) c0: vec4<f32>, @location(4) c1: vec4<f32>, @location(5) c2: vec4<f32>, @location(6) c3: vec4<f32> }
struct Output { @builtin(position) position: vec4<f32>, @location(0) normal: vec3<f32> }
@vertex fn vs_main(input: Input) -> Output { var output: Output; output.position=view_proj*mat4x4<f32>(input.c0,input.c1,input.c2,input.c3)*vec4(input.position,1); output.normal=input.normal; return output; }
@fragment fn fs_main(input: Output) -> @location(0) vec4<f32> { return vec4(vec3(0.12)+max(input.normal.y,0.0)*vec3(0.16),1); }
`;

export const frameShader = /* wgsl */ `
@group(0) @binding(0) var source_texture: texture_2d<f32>;
@group(0) @binding(1) var second_texture: texture_2d<f32>;
@group(0) @binding(2) var linear_clamp: sampler;
struct Parameters { values: array<vec4<f32>, 8> }
@group(0) @binding(3) var<uniform> parameters: Parameters;
struct VertexOut { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs_main(@builtin(vertex_index) index: u32) -> VertexOut { let positions=array(vec2(-1.0,-1.0),vec2(3.0,-1.0),vec2(-1.0,3.0)); let p=positions[index]; var out:VertexOut; out.position=vec4(p,0,1); out.uv=p*vec2(0.5,-0.5)+vec2(0.5); return out; }
fn aces(x:vec3<f32>)->vec3<f32>{return clamp((x*(2.51*x+vec3(0.03)))/(x*(2.43*x+vec3(0.59))+vec3(0.14)),vec3(0),vec3(1));}
fn linear_to_srgb(x:vec3<f32>)->vec3<f32>{let safe=clamp(x,vec3(0),vec3(1));return select(1.055*pow(safe,vec3(1.0/2.4))-vec3(0.055),safe*12.92,safe<=vec3(0.0031308));}
@fragment fn fs_frame_out(in:VertexOut)->@location(0) vec4<f32>{let sampled=textureSampleLevel(source_texture,linear_clamp,in.uv,0);var rgb=max(sampled.rgb*exp2(parameters.values[0].z),vec3(0));if parameters.values[0].x>0.5{if parameters.values[0].y>1.5{rgb=aces(rgb);}else if parameters.values[0].y>0.5{rgb=rgb/(vec3(1)+rgb);}}if parameters.values[0].w>0.5{rgb=linear_to_srgb(rgb);}return vec4(clamp(rgb,vec3(0),vec3(1)),clamp(sampled.a,0,1));}
`;

export const noopComputeShader = /* wgsl */ `@compute @workgroup_size(1) fn main() {}`;

/** Optional declarations copied into each graph that wants these implementations. */
export const defaultPipelines = Object.freeze({
  render: Object.freeze([
    Object.freeze({ name: "ground_plane", shader: groundShader, vertexEntry: "vs_main", fragmentEntry: "fs_main" }),
    Object.freeze({ name: "gltf_standard", shader: gltfShader, vertexEntry: "vs_main", fragmentEntry: "fs_main", material: true }),
    Object.freeze({ name: "gltf_standard_double_sided", shader: gltfShader, vertexEntry: "vs_main", fragmentEntry: "fs_main", doubleSided: true, material: true }),
    Object.freeze({ name: "frame_out", shader: frameShader, vertexEntry: "vs_main", fragmentEntry: "fs_frame_out" }),
  ]),
  compute: Object.freeze([
    Object.freeze({ name: "initialize_scene", shader: noopComputeShader, entry: "main", dispatch: [1, 1, 1] }),
  ]),
});
