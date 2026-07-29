//! Graph-owned instance predicate compute support.

use crate::render_graph::{
    BooleanOp, CompareOp, ExpressionOp, InstanceTraversalPlan, SemanticType, TypedLiteral,
};

use super::gpu_scene::{DrawIndexedIndirect, GpuSceneCache};

fn f(value: f32) -> Result<String, String> {
    if !value.is_finite() {
        return Err("non-finite expression literal".into());
    }
    Ok(format!("{value:?}"))
}

/// Generates the dense, single-invocation traversal body. Expressions are emitted in
/// `ExprId` order, so shared IR nodes are evaluated exactly once.
pub fn generate_wgsl(plan: &InstanceTraversalPlan) -> Result<String, String> {
    let mut s = String::from("struct Params { planes: array<vec4<f32>,6>, instance_count:u32, pipeline_count:u32, _pad:vec2<u32> };\nstruct Inst{model:mat4x4<f32>,n0:vec4<f32>,n1:vec4<f32>,n2:vec4<f32>}; struct Aabb{min:vec4<f32>,max:vec4<f32>}; struct Type16{words:array<u32,16>}; struct Meta{index_count:u32,first_index:u32,base_vertex:i32,instance_index:u32}; struct Cmd{index_count:u32,instance_count:u32,first_index:u32,base_vertex:i32,first_instance:u32};\n@group(0) @binding(0)var<uniform>p:Params; @group(0) @binding(1)var<storage,read>instances:array<Inst>; @group(0) @binding(2)var<storage,read>aabbs:array<Aabb>; @group(0) @binding(3)var<storage,read>types:array<Type16>; @group(0) @binding(4)var<storage,read>metadata:array<Meta>; @group(0) @binding(5)var<storage,read_write>commands:array<Cmd>;\nstruct LocalAabb{min:vec3<f32>,max:vec3<f32>}; fn culled(i:u32,a:LocalAabb)->bool{var outside=false;for(var q=0u;q<6u;q++){var all=true;for(var c=0u;c<8u;c++){let v=vec3<f32>(select(a.min.x,a.max.x,(c&1u)!=0u),select(a.min.y,a.max.y,(c&2u)!=0u),select(a.min.z,a.max.z,(c&4u)!=0u));all=all&&(dot(p.planes[q],instances[i].model*vec4<f32>(v,1.0))<0.0);}outside=outside||all;}return outside;} @compute @workgroup_size(64) fn main(@builtin(global_invocation_id)gid:vec3<u32>){let i=gid.x;if(i>=p.instance_count){return;}\n");
    for (i, e) in plan.expressions.expressions.iter().enumerate() {
        let x = |id: crate::render_graph::ExprId| format!("e{}", id.0);
        let rhs = match &e.op {
            ExpressionOp::Literal { literal } => match literal {
                TypedLiteral::Bool(v) => v.to_string(),
                TypedLiteral::F32(v) => f(*v)?,
                TypedLiteral::U32(v) => format!("{v}u"),
                TypedLiteral::Vec2(v) => format!("vec2<f32>({},{})", f(v[0])?, f(v[1])?),
                TypedLiteral::Vec3(v) => {
                    format!("vec3<f32>({},{},{})", f(v[0])?, f(v[1])?, f(v[2])?)
                }
                TypedLiteral::Vec4(v) => format!(
                    "vec4<f32>({},{},{},{})",
                    f(v[0])?,
                    f(v[1])?,
                    f(v[2])?,
                    f(v[3])?
                ),
                TypedLiteral::U32x16(v) => format!(
                    "Type16(array<u32,16>({}))",
                    v.iter()
                        .map(|x| format!("{x}u"))
                        .collect::<Vec<_>>()
                        .join(",")
                ),
                TypedLiteral::LocalAabb { min, max } => format!(
                    "LocalAabb(vec3<f32>({},{},{}),vec3<f32>({},{},{}))",
                    f(min[0])?,
                    f(min[1])?,
                    f(min[2])?,
                    f(max[0])?,
                    f(max[1])?,
                    f(max[2])?
                ),
                TypedLiteral::Mat2(v) => matrix_literal("mat2x2<f32>", v)?,
                TypedLiteral::Mat3(v) => matrix_literal("mat3x3<f32>", v)?,
                TypedLiteral::Mat4(v) => matrix_literal("mat4x4<f32>", v)?,
            },
            ExpressionOp::InstanceType { .. } => "types[i]".into(),
            ExpressionOp::LocalAabb { .. } => "LocalAabb(aabbs[i].min.xyz,aabbs[i].max.xyz)".into(),
            ExpressionOp::Not { value } => format!("!{}", x(*value)),
            ExpressionOp::Boolean {
                operation,
                operands,
            } => {
                let identity = matches!(operation, BooleanOp::And | BooleanOp::Xnor);
                let operator = match operation {
                    BooleanOp::And => "&&",
                    BooleanOp::Or => "||",
                    BooleanOp::Xor | BooleanOp::Xnor => "!=",
                };
                let folded = operands
                    .iter()
                    .map(|operand| x(*operand))
                    .reduce(|left, right| format!("({left} {operator} {right})"))
                    .unwrap_or_else(|| identity.to_string());
                if matches!(operation, BooleanOp::Xnor) && operands.len() > 1 {
                    format!("!{folded}")
                } else if matches!(operation, BooleanOp::Xnor) && !operands.is_empty() {
                    format!("!({folded})")
                } else {
                    folded
                }
            }
            ExpressionOp::CompareF32 {
                operation,
                left,
                right,
            }
            | ExpressionOp::CompareU32 {
                operation,
                left,
                right,
            } => format!(
                "({} {} {})",
                x(*left),
                match operation {
                    CompareOp::GreaterThan => ">",
                    CompareOp::LessThan => "<",
                    CompareOp::Equals => "==",
                },
                x(*right)
            ),
            ExpressionOp::VectorProject { vector, index } => {
                let limit =
                    vector_width(&plan.expressions.expressions[vector.0 as usize].semantic_type)
                        .ok_or("vector projection source is not a vector")?;
                fixed_index(*index, limit, "vector projection")?;
                format!("{}[{}]", x(*vector), index)
            }
            ExpressionOp::VectorConstruct { components } => format!(
                "{}({})",
                wgsl_type(&e.semantic_type)?,
                components
                    .iter()
                    .map(|id| x(*id))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            ExpressionOp::MatrixColumn { matrix, index } => {
                let limit =
                    matrix_width(&plan.expressions.expressions[matrix.0 as usize].semantic_type)
                        .ok_or("matrix projection source is not a matrix")?;
                fixed_index(*index, limit, "matrix projection")?;
                format!("{}[{}]", x(*matrix), index)
            }
            ExpressionOp::MatrixConstruct { columns } => format!(
                "{}({})",
                wgsl_type(&e.semantic_type)?,
                columns
                    .iter()
                    .map(|id| x(*id))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            ExpressionOp::TypeWord { value, index } => {
                fixed_index(*index, 16, "u32x16 projection")?;
                format!("{}.words[{}]", x(*value), index)
            }
            ExpressionOp::TypeConstruct { words } => format!(
                "Type16(array<u32,16>({}))",
                words.iter().map(|id| x(*id)).collect::<Vec<_>>().join(",")
            ),
            ExpressionOp::U32Bit { value, index } => {
                fixed_index(*index, 32, "u32 bit projection")?;
                format!("(({} & (1u<<{}u))!=0u)", x(*value), index)
            }
            ExpressionOp::U32Construct { bits } => format!(
                "({})",
                bits.iter()
                    .enumerate()
                    .map(|(bit, id)| format!("select(0u,{}u,{})", 1u32 << bit, x(*id)))
                    .collect::<Vec<_>>()
                    .join("|")
            ),
            ExpressionOp::AabbMin { aabb } => format!("{}.min", x(*aabb)),
            ExpressionOp::AabbMax { aabb } => format!("{}.max", x(*aabb)),
            ExpressionOp::FrustumCulled { local_aabb, .. } => {
                format!("culled(i,{})", x(*local_aabb))
            }
        };
        s.push_str(&format!("let e{i}={rhs};\n"));
    }
    for entry in &plan.pipelines {
        s.push_str(&format!("{{let m=metadata[i];commands[{}u*p.instance_count+i]=Cmd(m.index_count,select(0u,1u,e{}),m.first_index,m.base_vertex,m.instance_index);}}\n",entry.ordinal,entry.predicate.0));
    }
    s.push('}');
    if s.len() >= 1024 * 1024 {
        return Err("generated traversal WGSL exceeds 1 MiB".into());
    }
    Ok(s)
}

fn fixed_index(index: u8, limit: u8, kind: &str) -> Result<(), String> {
    (index < limit)
        .then_some(())
        .ok_or_else(|| format!("invalid fixed {kind} index"))
}
fn vector_width(ty: &SemanticType) -> Option<u8> {
    match ty {
        SemanticType::Vec2 => Some(2),
        SemanticType::Vec3 => Some(3),
        SemanticType::Vec4 => Some(4),
        _ => None,
    }
}
fn matrix_width(ty: &SemanticType) -> Option<u8> {
    match ty {
        SemanticType::Mat2 => Some(2),
        SemanticType::Mat3 => Some(3),
        SemanticType::Mat4 => Some(4),
        _ => None,
    }
}
fn wgsl_type(ty: &SemanticType) -> Result<&'static str, String> {
    match ty {
        SemanticType::Vec2 => Ok("vec2<f32>"),
        SemanticType::Vec3 => Ok("vec3<f32>"),
        SemanticType::Vec4 => Ok("vec4<f32>"),
        SemanticType::Mat2 => Ok("mat2x2<f32>"),
        SemanticType::Mat3 => Ok("mat3x3<f32>"),
        SemanticType::Mat4 => Ok("mat4x4<f32>"),
        _ => Err("invalid combine result type".into()),
    }
}
fn matrix_literal<const N: usize>(name: &str, columns: &[[f32; N]; N]) -> Result<String, String> {
    let values = columns
        .iter()
        .flatten()
        .map(|v| f(*v))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(format!("{name}({})", values.join(",")))
}
pub fn dispatch_count(instances: u32, pipelines: u32) -> u32 {
    if pipelines == 0 {
        0
    } else {
        instances.max(1).div_ceil(64)
    }
}

pub fn command_offset(predicate_ordinal: u32, instance_count: usize, draw_index: usize) -> u64 {
    (u64::from(predicate_ordinal) * instance_count as u64 + draw_index as u64)
        * std::mem::size_of::<DrawIndexedIndirect>() as u64
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    planes: [[f32; 4]; 6],
    instance_count: u32,
    pipeline_count: u32,
    pad: [u32; 2],
}

pub struct TraversalGpu {
    graph: crate::render_graph::CompiledGraphId,
    plan: InstanceTraversalPlan,
    scene_epoch: u64,
    draw_count: usize,
    pipeline: wgpu::ComputePipeline,
    params: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    pub commands: wgpu::Buffer,
}

impl TraversalGpu {
    pub fn matches(
        &self,
        graph: crate::render_graph::CompiledGraphId,
        plan: &InstanceTraversalPlan,
        scene_epoch: u64,
        draw_count: usize,
    ) -> bool {
        self.graph == graph
            && self.plan == *plan
            && self.scene_epoch == scene_epoch
            && self.draw_count == draw_count
    }
    pub fn create(
        device: &wgpu::Device,
        graph: crate::render_graph::CompiledGraphId,
        plan: &InstanceTraversalPlan,
        gpu: &GpuSceneCache,
    ) -> Result<Self, String> {
        let source = generate_wgsl(plan)?;
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("instance traversal"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let entries = [
            (0, wgpu::BufferBindingType::Uniform),
            (1, wgpu::BufferBindingType::Storage { read_only: true }),
            (2, wgpu::BufferBindingType::Storage { read_only: true }),
            (3, wgpu::BufferBindingType::Storage { read_only: true }),
            (4, wgpu::BufferBindingType::Storage { read_only: true }),
            (5, wgpu::BufferBindingType::Storage { read_only: false }),
        ]
        .map(|(binding, ty)| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("instance traversal"),
            entries: &entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("instance traversal"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("instance traversal"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instance traversal params"),
            size: std::mem::size_of::<Params>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let count = (gpu.draws.len() as u64)
            .checked_mul(plan.pipelines.len() as u64)
            .and_then(|n| n.checked_mul(std::mem::size_of::<DrawIndexedIndirect>() as u64))
            .ok_or("indirect command size overflow")?
            .max(std::mem::size_of::<DrawIndexedIndirect>() as u64);
        if count > device.limits().max_buffer_size {
            return Err("indirect command buffer exceeds device limit".into());
        }
        let commands = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instance traversal commands"),
            size: count,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::INDIRECT,
            mapped_at_creation: false,
        });
        fn required(slot: &super::gpu_scene::BufferSlot) -> Result<&wgpu::Buffer, String> {
            slot.buffer
                .as_ref()
                .ok_or_else(|| "instance traversal scene buffer missing".to_owned())
        }
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("instance traversal"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: required(&gpu.instances)?.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: required(&gpu.local_aabbs)?.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: required(&gpu.instance_types)?.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: required(&gpu.draw_metadata)?.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: commands.as_entire_binding(),
                },
            ],
        });
        Ok(Self {
            graph,
            plan: plan.clone(),
            scene_epoch: gpu.buffer_epoch,
            draw_count: gpu.draws.len(),
            pipeline,
            params,
            bind_group,
            commands,
        })
    }
    pub(crate) fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        planes: Option<[[f32; 4]; 6]>,
        instances: u32,
        mut profile: Option<&mut super::profiler::ProfileFrame>,
    ) {
        queue.write_buffer(
            &self.params,
            0,
            bytemuck::bytes_of(&Params {
                planes: planes.unwrap_or([[0.; 4]; 6]),
                instance_count: instances,
                pipeline_count: self.plan.pipelines.len() as u32,
                pad: [0; 2],
            }),
        );
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("instance traversal"),
            timestamp_writes: profile
                .as_deref_mut()
                .and_then(|p| p.compute_writes("instance_traversal")),
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(
            dispatch_count(instances, self.plan.pipelines.len() as u32),
            1,
            1,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_graph::{Expression, ExpressionPlan, NodeOutputRef};

    fn expression(semantic_type: SemanticType, op: ExpressionOp) -> Expression {
        Expression {
            semantic_type,
            op,
            origin: NodeOutputRef {
                node: "test".into(),
                socket: "value".into(),
            },
            mesh_provenance: None,
        }
    }

    #[test]
    fn pipeline_major_offsets_and_single_dispatch_are_deterministic() {
        assert_eq!(command_offset(0, 7, 6), 120);
        assert_eq!(command_offset(1, 7, 0), 140);
        assert_eq!(command_offset(3, 7, 2), 460);
        assert_eq!(dispatch_count(1, 4), 1);
        assert_eq!(dispatch_count(64, 4), 1);
        assert_eq!(dispatch_count(65, 4), 2);
        assert_eq!(dispatch_count(0, 4), 1);
    }

    #[test]
    fn lowering_helpers_cover_matrices_and_reject_dynamic_indexes() {
        assert_eq!(
            matrix_literal("mat2x2<f32>", &[[1.0, 2.0], [3.0, 4.0]]).unwrap(),
            "mat2x2<f32>(1.0,2.0,3.0,4.0)"
        );
        assert!(fixed_index(3, 3, "vector projection").is_err());
        assert_eq!(wgsl_type(&SemanticType::Mat4).unwrap(), "mat4x4<f32>");
    }

    #[test]
    fn variadic_boolean_wgsl_uses_identities_and_ordered_parity() {
        let expressions = vec![
            expression(
                SemanticType::Bool,
                ExpressionOp::Boolean {
                    operation: BooleanOp::And,
                    operands: vec![],
                },
            ),
            expression(
                SemanticType::Bool,
                ExpressionOp::Boolean {
                    operation: BooleanOp::Xor,
                    operands: vec![],
                },
            ),
            expression(
                SemanticType::Bool,
                ExpressionOp::Boolean {
                    operation: BooleanOp::Xnor,
                    operands: vec![crate::render_graph::ExprId(0)],
                },
            ),
            expression(
                SemanticType::Bool,
                ExpressionOp::Boolean {
                    operation: BooleanOp::Xnor,
                    operands: vec![
                        crate::render_graph::ExprId(0),
                        crate::render_graph::ExprId(1),
                        crate::render_graph::ExprId(2),
                    ],
                },
            ),
        ];
        let wgsl = generate_wgsl(&InstanceTraversalPlan {
            mesh: 0,
            expressions: ExpressionPlan { expressions },
            pipelines: vec![],
            requires_camera: false,
        })
        .unwrap();
        assert!(wgsl.contains("let e0=true;"));
        assert!(wgsl.contains("let e1=false;"));
        assert!(wgsl.contains("let e2=!(e0);"));
        assert!(wgsl.contains("let e3=!((e0 != e1) != e2);"));
    }

    #[test]
    fn u32_construct_wgsl_is_parenthesized() {
        let plan = InstanceTraversalPlan {
            mesh: 0,
            expressions: ExpressionPlan {
                expressions: vec![
                    expression(
                        SemanticType::Bool,
                        ExpressionOp::Literal {
                            literal: TypedLiteral::Bool(true),
                        },
                    ),
                    expression(
                        SemanticType::Bool,
                        ExpressionOp::Literal {
                            literal: TypedLiteral::Bool(false),
                        },
                    ),
                    expression(
                        SemanticType::U32,
                        ExpressionOp::U32Construct {
                            bits: vec![
                                crate::render_graph::ExprId(0),
                                crate::render_graph::ExprId(1),
                            ],
                        },
                    ),
                ],
            },
            pipelines: vec![],
            requires_camera: false,
        };

        let wgsl = generate_wgsl(&plan).unwrap();
        assert_eq!(
            wgsl.lines().find(|line| line.starts_with("let e2=")),
            Some("let e2=(select(0u,1u,e0)|select(0u,2u,e1));")
        );
    }
}
