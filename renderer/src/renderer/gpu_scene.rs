use std::mem::size_of;

use crate::{
    render_data::{MaterialKey, MeshHandle, PipelineKey, RenderFlags},
    renderer::scene_frame::SceneFramePlan,
};
use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, PartialEq)]
pub struct GpuInstance {
    pub model: [[f32; 4]; 4],
    pub normal_0: [f32; 4],
    pub normal_1: [f32; 4],
    pub normal_2: [f32; 4],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrawItem {
    pub pipeline: PipelineKey,
    pub material: MaterialKey,
    pub mesh: MeshHandle,
    pub indices: std::ops::Range<u32>,
    pub base_vertex: i32,
    pub instances: std::ops::Range<u32>,
    pub effective_visible: bool,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, PartialEq)]
pub struct GpuLocalAabb {
    pub min: [f32; 4],
    pub max: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, PartialEq, Eq)]
pub struct DrawSlotMetadata {
    pub index_count: u32,
    pub first_index: u32,
    pub base_vertex: i32,
    pub instance_index: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, PartialEq, Eq)]
pub struct DrawIndexedIndirect {
    pub index_count: u32,
    pub instance_count: u32,
    pub first_index: u32,
    pub base_vertex: i32,
    pub first_instance: u32,
}

#[derive(Default)]
pub struct GpuScenePlan {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub tangents: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
    pub instances: Vec<GpuInstance>,
    pub draws: Vec<DrawItem>,
    pub local_aabbs: Vec<GpuLocalAabb>,
    pub effective_visibility: Vec<u32>,
    pub draw_metadata: Vec<DrawSlotMetadata>,
    pub commands: Vec<DrawIndexedIndirect>,
}

fn visibility_matches(
    predicate: crate::render_graph::TriStatePredicate,
    mesh: RenderFlags,
    instance: RenderFlags,
) -> bool {
    let effective = mesh.contains(RenderFlags::VISIBLE) && instance.contains(RenderFlags::VISIBLE);
    match predicate {
        crate::render_graph::TriStatePredicate::Any => true,
        crate::render_graph::TriStatePredicate::RequiredTrue => effective,
        crate::render_graph::TriStatePredicate::RequiredFalse => !effective,
    }
}

impl GpuScenePlan {
    pub fn build(data: &SceneFramePlan) -> Result<Self, &'static str> {
        self::GpuScenePlan::build_with_query(
            data,
            crate::render_graph::MeshQueryRuntimeKey {
                visible: crate::render_graph::TriStatePredicate::RequiredTrue,
                frustum_culled: crate::render_graph::TriStatePredicate::Any,
            },
        )
    }

    pub fn build_with_query(
        data: &SceneFramePlan,
        query: crate::render_graph::MeshQueryRuntimeKey,
    ) -> Result<Self, &'static str> {
        let _ = query; // Packing is canonical; predicates are evaluated by the GPU.
        let mut plan = Self::default();
        let mut meshes: Vec<_> = data.meshes.iter().collect();
        meshes.sort_by_key(|mesh| {
            (
                mesh.pipeline.get(),
                mesh.material.get(),
                mesh.handle.slot(),
                mesh.handle.generation(),
            )
        });
        for mesh in meshes {
            let occurrences: Vec<_> = data.mesh_occurrence_indices[mesh.occurrence_range.clone()]
                .iter()
                .map(|&index| &data.occurrences[index])
                .collect();
            if occurrences.is_empty() {
                continue;
            }
            let vertex_start = plan.positions.len();
            let source_start = mesh.geometry.vertex_start as usize;
            let source_end = source_start
                .checked_add(mesh.geometry.vertex_count as usize)
                .ok_or("vertex range overflow")?;
            plan.positions.extend_from_slice(
                data.positions
                    .get(source_start..source_end)
                    .ok_or("invalid vertex range")?,
            );
            plan.normals.extend_from_slice(
                data.normals
                    .get(source_start..source_end)
                    .ok_or("invalid normal range")?,
            );
            plan.uvs.extend_from_slice(
                data.uvs
                    .get(source_start..source_end)
                    .ok_or("invalid uv range")?,
            );
            plan.tangents.extend_from_slice(
                data.tangents
                    .get(source_start..source_end)
                    .ok_or("invalid tangent range")?,
            );
            let index_start =
                u32::try_from(plan.indices.len()).map_err(|_| "index start exceeds u32")?;
            let source_index = mesh.geometry.index_start as usize;
            let source_index_end = source_index
                .checked_add(mesh.geometry.index_count as usize)
                .ok_or("index range overflow")?;
            plan.indices.extend_from_slice(
                data.indices
                    .get(source_index..source_index_end)
                    .ok_or("invalid index range")?,
            );
            for instance in occurrences {
                let instance_start = u32::try_from(plan.instances.len())
                    .map_err(|_| "instance start exceeds u32")?;
                let m = &instance.model;
                let determinant = m[0][0] * (m[1][1] * m[2][2] - m[2][1] * m[1][2])
                    - m[1][0] * (m[0][1] * m[2][2] - m[2][1] * m[0][2])
                    + m[2][0] * (m[0][1] * m[1][2] - m[1][1] * m[0][2]);
                plan.instances.push(GpuInstance {
                    model: instance.model,
                    normal_0: [
                        instance.normal[0][0],
                        instance.normal[0][1],
                        instance.normal[0][2],
                        if determinant < 0.0 { -1.0 } else { 1.0 },
                    ],
                    normal_1: [
                        instance.normal[1][0],
                        instance.normal[1][1],
                        instance.normal[1][2],
                        0.0,
                    ],
                    normal_2: [
                        instance.normal[2][0],
                        instance.normal[2][1],
                        instance.normal[2][2],
                        0.0,
                    ],
                });
                let base_vertex =
                    i32::try_from(vertex_start).map_err(|_| "base vertex exceeds i32")?;
                let end = index_start
                    .checked_add(mesh.geometry.index_count)
                    .ok_or("draw index range overflow")?;
                let effective_visible = mesh.flags.contains(RenderFlags::VISIBLE)
                    && instance.flags.contains(RenderFlags::VISIBLE);
                plan.local_aabbs.push(GpuLocalAabb {
                    min: [mesh.aabb.min[0], mesh.aabb.min[1], mesh.aabb.min[2], 0.],
                    max: [mesh.aabb.max[0], mesh.aabb.max[1], mesh.aabb.max[2], 0.],
                });
                plan.effective_visibility.push(effective_visible as u32);
                plan.draw_metadata.push(DrawSlotMetadata {
                    index_count: mesh.geometry.index_count,
                    first_index: index_start,
                    base_vertex,
                    instance_index: instance_start,
                });
                plan.commands.push(DrawIndexedIndirect {
                    index_count: mesh.geometry.index_count,
                    instance_count: 0,
                    first_index: index_start,
                    base_vertex,
                    first_instance: 0,
                });
                plan.draws.push(DrawItem {
                    pipeline: mesh.pipeline,
                    material: mesh.material,
                    mesh: mesh.handle,
                    indices: index_start..end,
                    base_vertex,
                    instances: instance_start..instance_start + 1,
                    effective_visible,
                });
            }
        }
        Ok(plan)
    }
}

pub fn required_buffer_capacity(
    current: u64,
    required: u64,
    maximum: u64,
) -> Result<u64, &'static str> {
    if required > maximum {
        return Err("buffer exceeds device max_buffer_size");
    }
    if required == 0 || current >= required {
        return Ok(current);
    }
    let grown = current
        .checked_mul(2)
        .ok_or("buffer capacity overflow")?
        .max(1)
        .max(required);
    Ok(grown.min(maximum))
}

pub fn vertex_layouts() -> [wgpu::VertexBufferLayout<'static>; 5] {
    const INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4, 8 => Float32x4, 9 => Float32x4];
    [
        wgpu::VertexBufferLayout {
            array_stride: 12,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3],
        },
        wgpu::VertexBufferLayout {
            array_stride: 12,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![1 => Float32x3],
        },
        wgpu::VertexBufferLayout {
            array_stride: 8,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![2 => Float32x2],
        },
        wgpu::VertexBufferLayout {
            array_stride: size_of::<GpuInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &INSTANCE_ATTRIBUTES,
        },
        wgpu::VertexBufferLayout {
            array_stride: 16,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![10 => Float32x4],
        },
    ]
}

#[derive(Default)]
pub struct BufferSlot {
    pub buffer: Option<wgpu::Buffer>,
    capacity: u64,
}

#[derive(Default)]
pub struct GpuSceneCache {
    revision: Option<u64>,
    query: Option<crate::render_graph::MeshQueryRuntimeKey>,
    pub positions: BufferSlot,
    pub normals: BufferSlot,
    pub uvs: BufferSlot,
    pub tangents: BufferSlot,
    pub indices: BufferSlot,
    pub instances: BufferSlot,
    pub local_aabbs: BufferSlot,
    pub effective_visibility: BufferSlot,
    pub draw_metadata: BufferSlot,
    pub frustum_flags: BufferSlot,
    pub indirect_commands: BufferSlot,
    pub draws: Vec<DrawItem>,
    compute: Option<CullingCompute>,
}

struct CullingCompute {
    params: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    frustum_pipeline: wgpu::ComputePipeline,
    query_pipeline: wgpu::ComputePipeline,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CullingParams {
    planes: [[f32; 4]; 6],
    count: u32,
    visible_predicate: u32,
    frustum_predicate: u32,
    _pad: u32,
}

fn predicate_code(value: crate::render_graph::TriStatePredicate) -> u32 {
    match value {
        crate::render_graph::TriStatePredicate::Any => 0,
        crate::render_graph::TriStatePredicate::RequiredTrue => 1,
        crate::render_graph::TriStatePredicate::RequiredFalse => 2,
    }
}

impl GpuSceneCache {
    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &SceneFramePlan,
    ) -> Result<(), String> {
        self.upload_with_query(
            device,
            queue,
            data,
            crate::render_graph::MeshQueryRuntimeKey {
                visible: crate::render_graph::TriStatePredicate::RequiredTrue,
                frustum_culled: crate::render_graph::TriStatePredicate::Any,
            },
        )
    }

    pub fn upload_with_query(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &SceneFramePlan,
        query: crate::render_graph::MeshQueryRuntimeKey,
    ) -> Result<(), String> {
        if self.revision == Some(data.revision) && self.query == Some(query) {
            return Ok(());
        }
        let plan = GpuScenePlan::build_with_query(data, query).map_err(str::to_owned)?;
        if plan.draws.is_empty() {
            self.draws.clear();
            self.revision = Some(data.revision);
            self.query = Some(query);
            return Ok(());
        }
        let maximum = device.limits().max_buffer_size;
        fn bytes<T>(values: &[T]) -> Result<u64, String> {
            u64::try_from(values.len())
                .map_err(|_| "buffer length overflow".to_owned())?
                .checked_mul(size_of::<T>() as u64)
                .ok_or_else(|| "buffer byte size overflow".to_owned())
        }
        let required = [
            bytes(&plan.positions)?,
            bytes(&plan.normals)?,
            bytes(&plan.uvs)?,
            bytes(&plan.tangents)?,
            bytes(&plan.indices)?,
            bytes(&plan.instances)?,
            bytes(&plan.local_aabbs)?,
            bytes(&plan.effective_visibility)?,
            bytes(&plan.draw_metadata)?,
            bytes(&plan.effective_visibility)?,
            bytes(&plan.commands)?,
        ];
        let old = [
            self.positions.capacity,
            self.normals.capacity,
            self.uvs.capacity,
            self.tangents.capacity,
            self.indices.capacity,
            self.instances.capacity,
            self.local_aabbs.capacity,
            self.effective_visibility.capacity,
            self.draw_metadata.capacity,
            self.frustum_flags.capacity,
            self.indirect_commands.capacity,
        ];
        let mut capacities = [0; 11];
        for i in 0..11 {
            capacities[i] =
                required_buffer_capacity(old[i], required[i], maximum).map_err(str::to_owned)?;
        }
        let usages = [
            wgpu::BufferUsages::VERTEX,
            wgpu::BufferUsages::VERTEX,
            wgpu::BufferUsages::VERTEX,
            wgpu::BufferUsages::VERTEX,
            wgpu::BufferUsages::INDEX,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::STORAGE,
            wgpu::BufferUsages::STORAGE,
            wgpu::BufferUsages::STORAGE,
            wgpu::BufferUsages::STORAGE,
            wgpu::BufferUsages::STORAGE,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::INDIRECT,
        ];
        let labels = [
            "scene positions",
            "scene normals",
            "scene uvs",
            "scene tangents",
            "scene indices",
            "scene instances",
            "scene local aabbs",
            "scene effective visibility",
            "scene draw metadata",
            "scene frustum flags",
            "scene indirect commands",
        ];
        let mut replacements: [Option<wgpu::Buffer>; 11] = Default::default();
        for i in 0..11 {
            if capacities[i] != old[i] {
                replacements[i] = Some(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(labels[i]),
                    size: capacities[i],
                    usage: usages[i] | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
            }
        }
        let slots = [
            &mut self.positions,
            &mut self.normals,
            &mut self.uvs,
            &mut self.tangents,
            &mut self.indices,
            &mut self.instances,
            &mut self.local_aabbs,
            &mut self.effective_visibility,
            &mut self.draw_metadata,
            &mut self.frustum_flags,
            &mut self.indirect_commands,
        ];
        for (i, slot) in slots.into_iter().enumerate() {
            if let Some(buffer) = replacements[i].take() {
                slot.buffer = Some(buffer);
                slot.capacity = capacities[i];
            }
        }
        let contents = [
            bytemuck::cast_slice(&plan.positions),
            bytemuck::cast_slice(&plan.normals),
            bytemuck::cast_slice(&plan.uvs),
            bytemuck::cast_slice(&plan.tangents),
            bytemuck::cast_slice(&plan.indices),
            bytemuck::cast_slice(&plan.instances),
            bytemuck::cast_slice(&plan.local_aabbs),
            bytemuck::cast_slice(&plan.effective_visibility),
            bytemuck::cast_slice(&plan.draw_metadata),
            bytemuck::cast_slice(&plan.effective_visibility),
            bytemuck::cast_slice(&plan.commands),
        ];
        let slots = [
            &self.positions,
            &self.normals,
            &self.uvs,
            &self.tangents,
            &self.indices,
            &self.instances,
            &self.local_aabbs,
            &self.effective_visibility,
            &self.draw_metadata,
            &self.frustum_flags,
            &self.indirect_commands,
        ];
        for (slot, contents) in slots.into_iter().zip(contents) {
            if !contents.is_empty() {
                queue.write_buffer(
                    slot.buffer.as_ref().expect("nonempty slot allocated"),
                    0,
                    contents,
                );
            }
        }
        self.draws = plan.draws;
        self.rebuild_compute(device)?;
        self.revision = Some(data.revision);
        self.query = Some(query);
        Ok(())
    }

    fn rebuild_compute(&mut self, device: &wgpu::Device) -> Result<(), String> {
        if self.draws.is_empty() {
            self.compute = None;
            return Ok(());
        }
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene culling compute"),
            source: wgpu::ShaderSource::Wgsl(include_str!("culling.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene culling layout"),
            entries: &(0..7)
                .map(|binding| wgpu::BindGroupLayoutEntry {
                    binding,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: if binding == 0 {
                            wgpu::BufferBindingType::Uniform
                        } else {
                            wgpu::BufferBindingType::Storage {
                                read_only: binding < 5,
                            }
                        },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                })
                .collect::<Vec<_>>(),
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene culling params"),
            size: size_of::<CullingParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let buffers = [
            &self.instances,
            &self.local_aabbs,
            &self.effective_visibility,
            &self.draw_metadata,
            &self.frustum_flags,
            &self.indirect_commands,
        ];
        let mut entries = vec![wgpu::BindGroupEntry {
            binding: 0,
            resource: params.as_entire_binding(),
        }];
        for (i, slot) in buffers.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry {
                binding: i as u32 + 1,
                resource: slot
                    .buffer
                    .as_ref()
                    .ok_or("culling buffer absent")?
                    .as_entire_binding(),
            });
        }
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene culling group"),
            layout: &layout,
            entries: &entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene culling pipeline layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = |entry| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        self.compute = Some(CullingCompute {
            params,
            bind_group,
            frustum_pipeline: pipeline("frustum_cull"),
            query_pipeline: pipeline("mesh_query"),
        });
        Ok(())
    }

    pub fn write_culling_params(
        &self,
        queue: &wgpu::Queue,
        planes: Option<[[f32; 4]; 6]>,
        query: crate::render_graph::MeshQueryRuntimeKey,
    ) {
        if let Some(compute) = &self.compute {
            if let Some(planes) = planes {
                queue.write_buffer(&compute.params, 0, bytemuck::bytes_of(&planes));
            }
            let tail = CullingParams {
                planes: [[0.0; 4]; 6],
                count: self.draws.len() as u32,
                visible_predicate: predicate_code(query.visible),
                frustum_predicate: predicate_code(query.frustum_culled),
                _pad: 0,
            };
            queue.write_buffer(
                &compute.params,
                std::mem::offset_of!(CullingParams, count) as u64,
                &bytemuck::bytes_of(&tail)[std::mem::offset_of!(CullingParams, count)..],
            );
        }
    }

    pub(crate) fn encode_frustum_cull(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        profile: Option<&mut crate::renderer::profiler::ProfileFrame>,
        profile_id: &str,
    ) {
        let Some(c) = &self.compute else { return };
        let timestamps = profile.and_then(|p| p.compute_writes(profile_id));
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("frustum_cull"),
            timestamp_writes: timestamps,
        });
        pass.set_pipeline(&c.frustum_pipeline);
        pass.set_bind_group(0, &c.bind_group, &[]);
        pass.dispatch_workgroups((self.draws.len() as u32 + 63) / 64, 1, 1);
    }
    pub(crate) fn encode_mesh_query(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        profile: Option<&mut crate::renderer::profiler::ProfileFrame>,
        profile_id: &str,
    ) {
        let Some(c) = &self.compute else { return };
        let timestamps = profile.and_then(|p| p.compute_writes(profile_id));
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("mesh_query"),
            timestamp_writes: timestamps,
        });
        pass.set_pipeline(&c.query_pipeline);
        pass.set_bind_group(0, &c.bind_group, &[]);
        pass.dispatch_workgroups((self.draws.len() as u32 + 63) / 64, 1, 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesh_query_source_guards_optional_flag_buffer_reads() {
        let source = include_str!("culling.wgsl");
        let visible_guard = source.find("if (params.visible_predicate != 0u)").unwrap();
        let visible_load = source.find("matches(authored_visible[i]").unwrap();
        let frustum_guard = source.find("if (params.frustum_predicate != 0u)").unwrap();
        let frustum_load = source.find("matches(frustum_flags[i]").unwrap();
        assert!(visible_guard < visible_load && frustum_guard < frustum_load);
        for binding in 0..=6 {
            assert!(source.contains(&format!("@binding({binding})")));
        }
    }

    #[test]
    fn effective_visibility_handles_every_mesh_instance_combination() {
        use crate::render_graph::TriStatePredicate::{Any, RequiredFalse, RequiredTrue};
        for (mesh, instance, effective) in [
            (false, false, false),
            (false, true, false),
            (true, false, false),
            (true, true, true),
        ] {
            let flags = |visible| {
                if visible {
                    RenderFlags::VISIBLE
                } else {
                    RenderFlags::NONE
                }
            };
            assert!(visibility_matches(Any, flags(mesh), flags(instance)));
            assert_eq!(
                visibility_matches(RequiredTrue, flags(mesh), flags(instance)),
                effective
            );
            assert_eq!(
                visibility_matches(RequiredFalse, flags(mesh), flags(instance)),
                !effective
            );
        }
    }
    use crate::render_data::{
        MeshCreateInfo, RenderData, RenderDataConfig, IDENTITY_MODEL_TRANSFORM,
    };
    #[test]
    fn instance_is_112_bytes_and_padding_is_zero() {
        assert_eq!(size_of::<GpuInstance>(), 112);
        let value = GpuInstance {
            model: [[1.0; 4]; 4],
            normal_0: [1., 2., 3., 0.],
            normal_1: [4., 5., 6., 0.],
            normal_2: [7., 8., 9., 0.],
        };
        assert_eq!(value.normal_2[3], 0.0);
    }
    #[test]
    fn capacity_grows_and_checks_limit() {
        assert_eq!(required_buffer_capacity(8, 9, 32), Ok(16));
        assert!(required_buffer_capacity(0, 33, 32).is_err());
    }
    #[test]
    fn mirrored_model_stores_negative_determinant_sign_in_padding() {
        let mut data = RenderData::new(RenderDataConfig::default()).unwrap();
        let mut mirrored = IDENTITY_MODEL_TRANSFORM;
        mirrored[0][0] = -1.0;
        data.create_mesh(MeshCreateInfo {
            positions: &[[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
            normals: &[[0., 0., 1.]; 3],
            tangents: &[[1., 0., 0., 1.]; 3],
            uvs: &[[0., 0.]; 3],
            indices: &[0, 1, 2],
            pipeline: PipelineKey::new(0),
            material: MaterialKey::DEFAULT,
            flags: RenderFlags::VISIBLE,
            default_instance_flags: RenderFlags::VISIBLE,
            default_transform: mirrored,
        })
        .unwrap();
        let frame = crate::renderer::scene_frame::SceneFramePlan::build(&data).unwrap();
        let plan = GpuScenePlan::build(&frame).unwrap();
        assert_eq!(plan.instances[0].normal_0[3], -1.0);
    }
    #[test]
    fn capacity_reuses_and_layout_matches_shader_contract() {
        assert_eq!(required_buffer_capacity(16, 12, 32), Ok(16));
        assert_eq!(required_buffer_capacity(0, 1, 32), Ok(1));
        let layouts = vertex_layouts();
        assert_eq!(
            layouts
                .iter()
                .map(|layout| layout.array_stride)
                .collect::<Vec<_>>(),
            [12, 12, 8, 112, 16]
        );
        assert_eq!(
            layouts[3]
                .attributes
                .iter()
                .map(|attribute| attribute.shader_location)
                .collect::<Vec<_>>(),
            vec![3, 4, 5, 6, 7, 8, 9]
        );
    }
    #[test]
    fn plan_is_canonical_and_predicate_independent() {
        let mut data = RenderData::new(RenderDataConfig {
            initial_vertices: 0,
            initial_indices: 0,
            initial_meshes: 0,
            initial_instances: 0,
            ..Default::default()
        })
        .unwrap();
        let p = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]];
        let n = [[0., 0., 1.]; 3];
        let u = [[0., 0.]; 3];
        let i = [0, 1, 2];
        let mut add = |pipeline, visible| {
            data.create_mesh(MeshCreateInfo {
                positions: &p,
                normals: &n,
                tangents: &[[1., 0., 0., 1.]; 3],
                uvs: &u,
                indices: &i,
                pipeline: PipelineKey::new(pipeline),
                material: crate::render_data::MaterialKey::DEFAULT,
                flags: RenderFlags::VISIBLE,
                default_instance_flags: if visible {
                    RenderFlags::VISIBLE
                } else {
                    RenderFlags::NONE
                },
                default_transform: IDENTITY_MODEL_TRANSFORM,
            })
            .unwrap()
        };
        let high = add(9, true);
        let _hidden = add(0, false);
        let low = add(2, true);
        data.create_instance(low.mesh, IDENTITY_MODEL_TRANSFORM, RenderFlags::VISIBLE)
            .unwrap();
        let frame = crate::renderer::scene_frame::SceneFramePlan::build(&data).unwrap();
        let plan = GpuScenePlan::build(&frame).unwrap();
        assert_eq!(
            plan.draws
                .iter()
                .map(|d| d.pipeline.get())
                .collect::<Vec<_>>(),
            vec![0, 2, 2, 9]
        );
        assert_eq!(plan.draws[0].base_vertex, 0);
        assert!(!plan.draws[0].effective_visible);
        assert_eq!(plan.draws[1].base_vertex, 3);
        assert_eq!(plan.draws[1].instances, 1..2);
        assert_eq!(plan.draws[2].instances, 2..3);
        assert_eq!(plan.indices, [0, 1, 2, 0, 1, 2, 0, 1, 2]);
        assert_eq!(high.mesh, plan.draws[3].mesh);
        assert_eq!(size_of::<GpuLocalAabb>(), 32);
        assert_eq!(size_of::<DrawSlotMetadata>(), 16);
        assert_eq!(size_of::<DrawIndexedIndirect>(), 20);
        assert!(plan
            .commands
            .iter()
            .all(|command| command.first_instance == 0));
    }
}
