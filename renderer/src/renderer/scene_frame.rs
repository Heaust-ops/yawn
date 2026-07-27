use std::collections::HashMap;

use thiserror::Error;

use crate::render_data::{
    affine_world_aabb, Aabb, GeometryRange, InstanceHandle, MaterialKey, MeshHandle,
    ModelTransform, NormalMatrix, PipelineKey, RenderData, RenderFlags,
};

#[derive(Clone, Debug)]
pub struct SceneFrameMesh {
    pub handle: MeshHandle,
    pub geometry: GeometryRange,
    pub pipeline: PipelineKey,
    pub material: MaterialKey,
    pub flags: RenderFlags,
    pub aabb: Aabb,
    pub default_instance: InstanceHandle,
    pub occurrence_range: std::ops::Range<usize>,
}

#[derive(Clone, Debug)]
pub struct SceneFrameOccurrence {
    pub handle: InstanceHandle,
    pub mesh: MeshHandle,
    pub mesh_index: usize,
    pub model: ModelTransform,
    pub normal: NormalMatrix,
    pub flags: RenderFlags,
    pub is_default: bool,
    pub world_aabb: Aabb,
}

#[derive(Clone, Debug)]
pub struct SceneFramePlan {
    pub revision: u64,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub tangents: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
    pub meshes: Vec<SceneFrameMesh>,
    pub occurrences: Vec<SceneFrameOccurrence>,
    /// Occurrence indices grouped by mesh without changing global occurrence order.
    pub mesh_occurrence_indices: Vec<usize>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SceneFrameError {
    #[error("an occurrence references a missing mesh")]
    MissingMesh,
    #[error("world bounds could not be derived")]
    InvalidWorldBounds,
    #[error("scene frame size overflow")]
    SizeOverflow,
}

impl SceneFramePlan {
    pub fn build(data: &RenderData) -> Result<Self, SceneFrameError> {
        let streams = data.streams();
        let mut meshes: Vec<_> = data.meshes().collect();
        meshes.sort_by_key(|(handle, _)| (handle.slot(), handle.generation()));
        let mesh_indices: HashMap<_, _> = meshes
            .iter()
            .enumerate()
            .map(|(dense, (handle, _))| (*handle, dense))
            .collect();

        let mut source_occurrences: Vec<_> = data.instances().collect();
        source_occurrences.sort_by_key(|(handle, _)| (handle.slot(), handle.generation()));
        let mut counts = vec![0usize; meshes.len()];
        let mut occurrences = Vec::with_capacity(source_occurrences.len());
        for (handle, occurrence) in source_occurrences {
            let mesh_index = *mesh_indices
                .get(&occurrence.mesh)
                .ok_or(SceneFrameError::MissingMesh)?;
            counts[mesh_index] = counts[mesh_index]
                .checked_add(1)
                .ok_or(SceneFrameError::SizeOverflow)?;
            let mesh = meshes[mesh_index].1;
            occurrences.push(SceneFrameOccurrence {
                handle,
                mesh: occurrence.mesh,
                mesh_index,
                model: occurrence.model,
                normal: occurrence.normal,
                flags: occurrence.flags,
                is_default: handle == mesh.default_instance,
                world_aabb: affine_world_aabb(mesh.aabb, occurrence.model)
                    .map_err(|_| SceneFrameError::InvalidWorldBounds)?,
            });
        }

        let mut offsets = Vec::with_capacity(meshes.len() + 1);
        offsets.push(0usize);
        for count in counts {
            offsets.push(
                offsets
                    .last()
                    .unwrap()
                    .checked_add(count)
                    .ok_or(SceneFrameError::SizeOverflow)?,
            );
        }
        let mut cursors = offsets[..meshes.len()].to_vec();
        let mut mesh_occurrence_indices = vec![0; occurrences.len()];
        for (occurrence_index, occurrence) in occurrences.iter().enumerate() {
            let cursor = &mut cursors[occurrence.mesh_index];
            mesh_occurrence_indices[*cursor] = occurrence_index;
            *cursor += 1;
        }
        let meshes = meshes
            .into_iter()
            .enumerate()
            .map(|(dense, (handle, mesh))| SceneFrameMesh {
                handle,
                geometry: mesh.geometry,
                pipeline: mesh.pipeline,
                material: mesh.material,
                flags: mesh.flags,
                aabb: mesh.aabb,
                default_instance: mesh.default_instance,
                occurrence_range: offsets[dense]..offsets[dense + 1],
            })
            .collect();
        Ok(Self {
            revision: data.revision(),
            positions: streams.positions.to_vec(),
            normals: streams.normals.to_vec(),
            uvs: streams.uvs.to_vec(),
            tangents: streams.tangents.to_vec(),
            indices: data.indices().to_vec(),
            meshes,
            occurrences,
            mesh_occurrence_indices,
        })
    }
}

#[derive(Default)]
pub struct SceneFrameCache {
    plan: Option<Box<SceneFramePlan>>,
}

impl SceneFrameCache {
    pub fn get_or_build(&mut self, data: &RenderData) -> Result<&SceneFramePlan, SceneFrameError> {
        if self
            .plan
            .as_ref()
            .is_none_or(|plan| plan.revision != data.revision())
        {
            let replacement = Box::new(SceneFramePlan::build(data)?);
            self.plan = Some(replacement);
        }
        Ok(self.plan.as_deref().unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_data::{MeshCreateInfo, RenderDataConfig, IDENTITY_MODEL_TRANSFORM};

    fn mesh(data: &mut RenderData, visible: bool) -> crate::render_data::CreatedMesh {
        data.create_mesh(MeshCreateInfo {
            positions: &[[0., 0., 0.], [2., 0., 0.], [0., 2., 0.]],
            normals: &[[0., 0., 1.]; 3],
            tangents: &[[1., 0., 0., 1.]; 3],
            uvs: &[[0., 0.]; 3],
            indices: &[0, 1, 2],
            pipeline: PipelineKey::new(0),
            material: crate::render_data::MaterialKey::DEFAULT,
            flags: if visible {
                RenderFlags::VISIBLE
            } else {
                RenderFlags::NONE
            },
            default_instance_flags: RenderFlags::VISIBLE,
            default_transform: IDENTITY_MODEL_TRANSFORM,
        })
        .unwrap()
    }

    #[test]
    fn cache_reuses_pointer_and_rebuilds_on_revision() {
        let mut data = RenderData::new(RenderDataConfig::default()).unwrap();
        let mut cache = SceneFrameCache::default();
        let first = cache.get_or_build(&data).unwrap() as *const _;
        assert_eq!(first, cache.get_or_build(&data).unwrap() as *const _);
        let created = mesh(&mut data, true);
        let second = cache.get_or_build(&data).unwrap() as *const _;
        assert_ne!(first, second);
        data.set_mesh_flags(created.mesh, RenderFlags::NONE)
            .unwrap();
        let third = cache.get_or_build(&data).unwrap() as *const _;
        assert_ne!(second, third);
        let mut moved = IDENTITY_MODEL_TRANSFORM;
        moved[3][1] = 9.0;
        data.set_instance_transform(created.default_instance, moved)
            .unwrap();
        assert_ne!(third, cache.get_or_build(&data).unwrap() as *const _);
    }

    #[test]
    fn retains_hidden_entries_builds_adjacency_and_world_bounds() {
        let mut data = RenderData::new(RenderDataConfig::default()).unwrap();
        let hidden = mesh(&mut data, false);
        let shown = mesh(&mut data, true);
        let mut translated = IDENTITY_MODEL_TRANSFORM;
        translated[0][0] = 2.;
        translated[1][1] = 3.;
        translated[2][2] = 4.;
        translated[3][0] = 5.;
        translated[3][1] = -2.;
        let extra = data
            .create_instance(hidden.mesh, translated, RenderFlags::NONE)
            .unwrap();
        let plan = SceneFramePlan::build(&data).unwrap();
        assert_eq!((plan.meshes.len(), plan.occurrences.len()), (2, 3));
        assert!(plan
            .occurrences
            .iter()
            .any(|o| o.handle == hidden.default_instance && o.is_default));
        let occurrence = plan.occurrences.iter().find(|o| o.handle == extra).unwrap();
        assert_eq!(occurrence.world_aabb.min, [5., -2., 0.]);
        assert_eq!(occurrence.world_aabb.max, [9., 4., 0.]);
        for (mesh_index, mesh) in plan.meshes.iter().enumerate() {
            assert!(plan.mesh_occurrence_indices[mesh.occurrence_range.clone()]
                .iter()
                .all(|&i| plan.occurrences[i].mesh_index == mesh_index));
        }
        assert_eq!(
            shown.default_instance.slot(),
            plan.occurrences
                .iter()
                .find(|o| o.mesh == shown.mesh)
                .unwrap()
                .handle
                .slot()
        );
    }

    #[test]
    fn slot_reuse_preserves_dense_order_adjacency_and_ownership() {
        let mut data = RenderData::new(RenderDataConfig::default()).unwrap();
        let a = mesh(&mut data, true);
        let doomed = mesh(&mut data, true);
        let c = mesh(&mut data, true);
        let a_extra = data
            .create_instance(a.mesh, IDENTITY_MODEL_TRANSFORM, RenderFlags::VISIBLE)
            .unwrap();
        let doomed_extra = data
            .create_instance(doomed.mesh, IDENTITY_MODEL_TRANSFORM, RenderFlags::VISIBLE)
            .unwrap();
        let c_extra = data
            .create_instance(c.mesh, IDENTITY_MODEL_TRANSFORM, RenderFlags::VISIBLE)
            .unwrap();
        data.destroy_instance(a_extra).unwrap();
        data.destroy_mesh(doomed.mesh).unwrap();
        let replacement = mesh(&mut data, true);
        let replacement_extra = data
            .create_instance(
                replacement.mesh,
                IDENTITY_MODEL_TRANSFORM,
                RenderFlags::VISIBLE,
            )
            .unwrap();
        assert_eq!(replacement.mesh.slot(), doomed.mesh.slot());
        assert!(replacement.mesh.generation() > doomed.mesh.generation());
        let destroyed = [a_extra, doomed.default_instance, doomed_extra];
        for reused in [replacement.default_instance, replacement_extra] {
            let old = destroyed
                .iter()
                .find(|old| old.slot() == reused.slot())
                .expect("replacement must reuse an interior instance slot");
            assert!(reused.generation() > old.generation());
        }
        assert!(data.instance(doomed_extra).is_none());

        let plan = SceneFramePlan::build(&data).unwrap();
        assert!(plan
            .meshes
            .windows(2)
            .all(|w| (w[0].handle.slot(), w[0].handle.generation())
                < (w[1].handle.slot(), w[1].handle.generation())));
        assert!(plan.occurrences.windows(2).all(|w| (
            w[0].handle.slot(),
            w[0].handle.generation()
        ) < (
            w[1].handle.slot(),
            w[1].handle.generation()
        )));
        let mut seen = vec![0; plan.occurrences.len()];
        for (mesh_index, mesh) in plan.meshes.iter().enumerate() {
            let adjacency = &plan.mesh_occurrence_indices[mesh.occurrence_range.clone()];
            assert!(adjacency.windows(2).all(|w| {
                let a = plan.occurrences[w[0]].handle;
                let b = plan.occurrences[w[1]].handle;
                (a.slot(), a.generation()) < (b.slot(), b.generation())
            }));
            for &index in adjacency {
                seen[index] += 1;
                let occurrence = &plan.occurrences[index];
                assert_eq!(occurrence.mesh, mesh.handle);
                assert_eq!(occurrence.mesh_index, mesh_index);
                assert_eq!(
                    occurrence.is_default,
                    occurrence.handle == mesh.default_instance
                );
            }
            assert_eq!(
                adjacency
                    .iter()
                    .filter(|&&i| plan.occurrences[i].is_default)
                    .count(),
                1
            );
        }
        assert!(seen.into_iter().all(|count| count == 1));
        assert!(plan.occurrences.iter().any(|o| o.handle == c_extra));
    }
}
