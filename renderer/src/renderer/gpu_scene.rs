use std::mem::size_of;

use crate::render_data::{MeshHandle, PipelineKey, RenderData, RenderFlags};
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
    pub mesh: MeshHandle,
    pub indices: std::ops::Range<u32>,
    pub base_vertex: i32,
    pub instances: std::ops::Range<u32>,
}

#[derive(Default)]
pub struct GpuScenePlan {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    pub instances: Vec<GpuInstance>,
    pub draws: Vec<DrawItem>,
}

impl GpuScenePlan {
    pub fn build(data: &RenderData) -> Result<Self, &'static str> {
        let mut plan = Self::default();
        let mut meshes: Vec<_> = data
            .meshes()
            .filter(|(_, mesh)| mesh.flags.contains(RenderFlags::VISIBLE))
            .collect();
        meshes.sort_by_key(|(handle, mesh)| {
            (mesh.pipeline.get(), handle.slot(), handle.generation())
        });
        let streams = data.streams();
        for (handle, mesh) in meshes {
            let mut occurrences: Vec<_> = data
                .instances()
                .filter(|(_, instance)| {
                    instance.mesh == handle && instance.flags.contains(RenderFlags::VISIBLE)
                })
                .collect();
            occurrences.sort_by_key(|(handle, _)| (handle.slot(), handle.generation()));
            if occurrences.is_empty() {
                continue;
            }
            let vertex_start = plan.positions.len();
            let source_start = mesh.geometry.vertex_start as usize;
            let source_end = source_start
                .checked_add(mesh.geometry.vertex_count as usize)
                .ok_or("vertex range overflow")?;
            plan.positions.extend_from_slice(
                streams
                    .positions
                    .get(source_start..source_end)
                    .ok_or("invalid vertex range")?,
            );
            plan.normals.extend_from_slice(
                streams
                    .normals
                    .get(source_start..source_end)
                    .ok_or("invalid normal range")?,
            );
            plan.uvs.extend_from_slice(
                streams
                    .uvs
                    .get(source_start..source_end)
                    .ok_or("invalid uv range")?,
            );
            let index_start =
                u32::try_from(plan.indices.len()).map_err(|_| "index start exceeds u32")?;
            let source_index = mesh.geometry.index_start as usize;
            let source_index_end = source_index
                .checked_add(mesh.geometry.index_count as usize)
                .ok_or("index range overflow")?;
            plan.indices.extend_from_slice(
                data.indices()
                    .get(source_index..source_index_end)
                    .ok_or("invalid index range")?,
            );
            let instance_start =
                u32::try_from(plan.instances.len()).map_err(|_| "instance start exceeds u32")?;
            for (_, instance) in occurrences {
                plan.instances.push(GpuInstance {
                    model: instance.model,
                    normal_0: [
                        instance.normal[0][0],
                        instance.normal[0][1],
                        instance.normal[0][2],
                        0.0,
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
            }
            plan.draws.push(DrawItem {
                pipeline: mesh.pipeline,
                mesh: handle,
                indices: index_start
                    ..index_start
                        .checked_add(mesh.geometry.index_count)
                        .ok_or("draw index range overflow")?,
                base_vertex: i32::try_from(vertex_start).map_err(|_| "base vertex exceeds i32")?,
                instances: instance_start
                    ..u32::try_from(plan.instances.len())
                        .map_err(|_| "instance end exceeds u32")?,
            });
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

pub fn vertex_layouts() -> [wgpu::VertexBufferLayout<'static>; 4] {
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
    pub positions: BufferSlot,
    pub normals: BufferSlot,
    pub uvs: BufferSlot,
    pub indices: BufferSlot,
    pub instances: BufferSlot,
    pub draws: Vec<DrawItem>,
}

impl GpuSceneCache {
    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &RenderData,
    ) -> Result<(), String> {
        if self.revision == Some(data.revision()) {
            return Ok(());
        }
        let plan = GpuScenePlan::build(data).map_err(str::to_owned)?;
        if plan.draws.is_empty() {
            self.draws.clear();
            self.revision = Some(data.revision());
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
            bytes(&plan.indices)?,
            bytes(&plan.instances)?,
        ];
        let old = [
            self.positions.capacity,
            self.normals.capacity,
            self.uvs.capacity,
            self.indices.capacity,
            self.instances.capacity,
        ];
        let mut capacities = [0; 5];
        for i in 0..5 {
            capacities[i] =
                required_buffer_capacity(old[i], required[i], maximum).map_err(str::to_owned)?;
        }
        let usages = [
            wgpu::BufferUsages::VERTEX,
            wgpu::BufferUsages::VERTEX,
            wgpu::BufferUsages::VERTEX,
            wgpu::BufferUsages::INDEX,
            wgpu::BufferUsages::VERTEX,
        ];
        let labels = [
            "scene positions",
            "scene normals",
            "scene uvs",
            "scene indices",
            "scene instances",
        ];
        let mut replacements: [Option<wgpu::Buffer>; 5] = Default::default();
        for i in 0..5 {
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
            &mut self.indices,
            &mut self.instances,
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
            bytemuck::cast_slice(&plan.indices),
            bytemuck::cast_slice(&plan.instances),
        ];
        let slots = [
            &self.positions,
            &self.normals,
            &self.uvs,
            &self.indices,
            &self.instances,
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
        self.revision = Some(data.revision());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_data::{MeshCreateInfo, RenderDataConfig, IDENTITY_MODEL_TRANSFORM};
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
    fn capacity_reuses_and_layout_matches_shader_contract() {
        assert_eq!(required_buffer_capacity(16, 12, 32), Ok(16));
        assert_eq!(required_buffer_capacity(0, 1, 32), Ok(1));
        let layouts = vertex_layouts();
        assert_eq!(
            layouts
                .iter()
                .map(|layout| layout.array_stride)
                .collect::<Vec<_>>(),
            [12, 12, 8, 112]
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
    fn plan_orders_pipelines_skips_hidden_and_uses_local_indices() {
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
                uvs: &u,
                indices: &i,
                pipeline: PipelineKey::new(pipeline),
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
        let plan = GpuScenePlan::build(&data).unwrap();
        assert_eq!(
            plan.draws
                .iter()
                .map(|d| d.pipeline.get())
                .collect::<Vec<_>>(),
            vec![2, 9]
        );
        assert_eq!(plan.draws[0].base_vertex, 0);
        assert_eq!(plan.draws[1].base_vertex, 3);
        assert_eq!(plan.draws[0].instances, 0..2);
        assert_eq!(plan.indices, [0, 1, 2, 0, 1, 2]);
        assert_eq!(high.mesh, plan.draws[1].mesh);
    }
}
