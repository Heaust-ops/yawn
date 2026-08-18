use std::mem::size_of;

use bytemuck::{Pod, Zeroable};

use crate::{
    render_data::{MaterialKey, MeshHandle},
    renderer::scene_frame::SceneFramePlan,
};

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
    pub material: MaterialKey,
    pub mesh: MeshHandle,
    pub indices: std::ops::Range<u32>,
    pub base_vertex: i32,
    pub instances: std::ops::Range<u32>,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, PartialEq)]
pub struct GpuLocalAabb {
    pub min: [f32; 4],
    pub max: [f32; 4],
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
    pub instance_types: Vec<[u32; 16]>,
}

impl GpuScenePlan {
    pub fn build(data: &SceneFramePlan) -> Result<Self, &'static str> {
        let mut p = Self::default();
        let mut meshes: Vec<_> = data.meshes.iter().collect();
        meshes.sort_by_key(|m| (m.material.get(), m.handle.slot(), m.handle.generation()));
        for mesh in meshes {
            let occurrences: Vec<_> = data.mesh_occurrence_indices[mesh.occurrence_range.clone()]
                .iter()
                .map(|&i| &data.occurrences[i])
                .collect();
            if occurrences.is_empty() {
                continue;
            }
            let vs = mesh.geometry.vertex_start as usize;
            let ve = vs
                .checked_add(mesh.geometry.vertex_count as usize)
                .ok_or("vertex range overflow")?;
            let vertex_start = p.positions.len();
            p.positions
                .extend_from_slice(data.positions.get(vs..ve).ok_or("invalid vertex range")?);
            p.normals
                .extend_from_slice(data.normals.get(vs..ve).ok_or("invalid normal range")?);
            p.uvs
                .extend_from_slice(data.uvs.get(vs..ve).ok_or("invalid uv range")?);
            p.tangents
                .extend_from_slice(data.tangents.get(vs..ve).ok_or("invalid tangent range")?);
            let first_index =
                u32::try_from(p.indices.len()).map_err(|_| "index start exceeds u32")?;
            let is = mesh.geometry.index_start as usize;
            let ie = is
                .checked_add(mesh.geometry.index_count as usize)
                .ok_or("index range overflow")?;
            p.indices
                .extend_from_slice(data.indices.get(is..ie).ok_or("invalid index range")?);
            for occurrence in occurrences {
                let instance_index =
                    u32::try_from(p.instances.len()).map_err(|_| "instance start exceeds u32")?;
                let m = &occurrence.model;
                let det = m[0][0] * (m[1][1] * m[2][2] - m[2][1] * m[1][2])
                    - m[1][0] * (m[0][1] * m[2][2] - m[2][1] * m[0][2])
                    + m[2][0] * (m[0][1] * m[1][2] - m[1][1] * m[0][2]);
                p.instances.push(GpuInstance {
                    model: occurrence.model,
                    normal_0: [
                        occurrence.normal[0][0],
                        occurrence.normal[0][1],
                        occurrence.normal[0][2],
                        if det < 0. { -1. } else { 1. },
                    ],
                    normal_1: [
                        occurrence.normal[1][0],
                        occurrence.normal[1][1],
                        occurrence.normal[1][2],
                        0.,
                    ],
                    normal_2: [
                        occurrence.normal[2][0],
                        occurrence.normal[2][1],
                        occurrence.normal[2][2],
                        0.,
                    ],
                });
                p.local_aabbs.push(GpuLocalAabb {
                    min: [
                        mesh.local_aabb.min[0],
                        mesh.local_aabb.min[1],
                        mesh.local_aabb.min[2],
                        0.,
                    ],
                    max: [
                        mesh.local_aabb.max[0],
                        mesh.local_aabb.max[1],
                        mesh.local_aabb.max[2],
                        0.,
                    ],
                });
                p.instance_types.push(occurrence.instance_type.words);
                let base_vertex =
                    i32::try_from(vertex_start).map_err(|_| "base vertex exceeds i32")?;
                p.draws.push(DrawItem {
                    material: mesh.material,
                    mesh: mesh.handle,
                    indices: first_index
                        ..first_index
                            .checked_add(mesh.geometry.index_count)
                            .ok_or("draw range overflow")?,
                    base_vertex,
                    instances: instance_index..instance_index + 1,
                });
            }
        }
        Ok(p)
    }
}

#[derive(Default)]
pub struct BufferSlot {
    pub buffer: Option<wgpu::Buffer>,
    capacity: u64,
}

#[derive(Default)]
pub struct GpuSceneCache {
    revision: Option<u64>,
    pub buffer_epoch: u64,
    pub positions: BufferSlot,
    pub normals: BufferSlot,
    pub uvs: BufferSlot,
    pub tangents: BufferSlot,
    pub indices: BufferSlot,
    pub instances: BufferSlot,
    pub instance_records: Vec<GpuInstance>,
    pub local_aabb_records: Vec<GpuLocalAabb>,
    pub instance_type_records: Vec<[u32; 16]>,
    pub draws: Vec<DrawItem>,
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
    Ok(current
        .checked_mul(2)
        .ok_or("buffer capacity overflow")?
        .max(1)
        .max(required)
        .min(maximum))
}

fn logical_or_zero<'a, T: Pod>(values: &'a [T], zero: &'a T) -> &'a [u8] {
    if values.is_empty() {
        bytemuck::bytes_of(zero)
    } else {
        bytemuck::cast_slice(values)
    }
}

impl GpuSceneCache {
    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &SceneFramePlan,
    ) -> Result<(), String> {
        if self.revision == Some(data.revision) {
            return Ok(());
        }
        let p = GpuScenePlan::build(data).map_err(str::to_owned)?;
        let max = device.limits().max_buffer_size;
        fn bytes<T>(v: &[T]) -> Result<u64, String> {
            (v.len() as u64)
                .checked_mul(size_of::<T>() as u64)
                .ok_or("buffer byte size overflow".into())
        }
        let required = [
            bytes(&p.positions)?,
            bytes(&p.normals)?,
            bytes(&p.uvs)?,
            bytes(&p.tangents)?,
            bytes(&p.indices)?,
            bytes(&p.instances)?.max(size_of::<GpuInstance>() as u64),
        ];
        let slots = [
            &mut self.positions,
            &mut self.normals,
            &mut self.uvs,
            &mut self.tangents,
            &mut self.indices,
            &mut self.instances,
        ];
        let usage = [
            wgpu::BufferUsages::VERTEX,
            wgpu::BufferUsages::VERTEX,
            wgpu::BufferUsages::VERTEX,
            wgpu::BufferUsages::VERTEX,
            wgpu::BufferUsages::INDEX,
            wgpu::BufferUsages::VERTEX,
        ];
        let mut replaced = false;
        for ((slot, &need), use_) in slots.into_iter().zip(&required).zip(usage) {
            let cap = required_buffer_capacity(slot.capacity, need, max).map_err(str::to_owned)?;
            if cap != slot.capacity {
                slot.buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("scene buffer"),
                    size: cap,
                    usage: use_ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
                slot.capacity = cap;
                replaced = true
            }
        }
        let zero_instance = GpuInstance::zeroed();
        let contents: [&[u8]; 6] = [
            bytemuck::cast_slice(&p.positions),
            bytemuck::cast_slice(&p.normals),
            bytemuck::cast_slice(&p.uvs),
            bytemuck::cast_slice(&p.tangents),
            bytemuck::cast_slice(&p.indices),
            logical_or_zero(&p.instances, &zero_instance),
        ];
        let slots = [
            &self.positions,
            &self.normals,
            &self.uvs,
            &self.tangents,
            &self.indices,
            &self.instances,
        ];
        for (s, c) in slots.into_iter().zip(contents) {
            if !c.is_empty() {
                queue.write_buffer(s.buffer.as_ref().unwrap(), 0, c)
            }
        }
        if replaced {
            self.buffer_epoch = self.buffer_epoch.wrapping_add(1).max(1)
        }
        self.instance_records = p.instances;
        self.local_aabb_records = p.local_aabbs;
        self.instance_type_records = p.instance_types;
        self.draws = p.draws;
        self.revision = Some(data.revision);
        Ok(())
    }
}

pub fn vertex_layouts() -> [wgpu::VertexBufferLayout<'static>; 5] {
    const IA: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![3=>Float32x4,4=>Float32x4,5=>Float32x4,6=>Float32x4,7=>Float32x4,8=>Float32x4,9=>Float32x4];
    [
        wgpu::VertexBufferLayout {
            array_stride: 12,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0=>Float32x3],
        },
        wgpu::VertexBufferLayout {
            array_stride: 12,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![1=>Float32x3],
        },
        wgpu::VertexBufferLayout {
            array_stride: 8,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![2=>Float32x2],
        },
        wgpu::VertexBufferLayout {
            array_stride: 112,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &IA,
        },
        wgpu::VertexBufferLayout {
            array_stride: 16,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![10=>Float32x4],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn abi() {
        assert_eq!(size_of::<GpuInstance>(), 112);
        assert_eq!(size_of::<GpuLocalAabb>(), 32);
        assert_eq!(size_of::<[u32; 16]>(), 64);
    }

    #[test]
    fn empty_instance_buffer_has_an_exact_zero_floor() {
        assert_eq!(
            logical_or_zero::<GpuInstance>(&[], &GpuInstance::zeroed()),
            [0; 112]
        );
        assert_eq!(required_buffer_capacity(0, 112, 1024), Ok(112));
    }
}
