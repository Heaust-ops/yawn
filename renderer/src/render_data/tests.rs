use super::*;
use crate::render_data::handle::SlotState;

const POSITIONS: [[f32; 3]; 3] = [[-1.0, 2.0, 3.0], [4.0, -2.0, 1.0], [0.0, 1.0, -3.0]];
const NORMALS: [[f32; 3]; 3] = [[0.0, 1.0, 0.0]; 3];
const UVS: [[f32; 2]; 3] = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
const INDICES: [u32; 3] = [0, 1, 2];

fn info() -> MeshCreateInfo<'static> {
    MeshCreateInfo {
        positions: &POSITIONS,
        normals: &NORMALS,
        uvs: &UVS,
        indices: &INDICES,
        pipeline: PipelineKey::new(7),
        flags: RenderFlags::from_bits_retain(2),
        default_instance_flags: RenderFlags::VISIBLE,
        default_transform: IDENTITY_MODEL_TRANSFORM,
    }
}

fn data() -> RenderData {
    RenderData::new(RenderDataConfig {
        initial_vertices: 0,
        initial_indices: 0,
        initial_meshes: 0,
        initial_instances: 0,
        ..RenderDataConfig::default()
    })
    .unwrap()
}

#[test]
fn affine_world_bounds_cover_translation_scale_shear_and_planes() {
    let local = Aabb {
        min: [-1.0, -2.0, 0.0],
        max: [1.0, 2.0, 0.0],
    };
    assert_eq!(
        affine_world_aabb(local, IDENTITY_MODEL_TRANSFORM),
        Ok(local)
    );

    let model = [
        [-2.0, 0.0, 0.0, 0.0],
        [0.5, 3.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [10.0, -4.0, 2.0, 1.0],
    ];
    assert_eq!(
        affine_world_aabb(local, model),
        Ok(Aabb {
            min: [7.0, -10.0, 2.0],
            max: [13.0, 2.0, 2.0],
        })
    );
}

#[test]
fn world_bounds_reject_projective_and_overflowing_transforms() {
    let local = Aabb {
        min: [-1.0; 3],
        max: [1.0; 3],
    };
    let mut projective = IDENTITY_MODEL_TRANSFORM;
    projective[0][3] = 0.5;
    assert_eq!(
        affine_world_aabb(local, projective),
        Err(RenderDataError::InvalidTransform)
    );
    let mut overflowing = IDENTITY_MODEL_TRANSFORM;
    overflowing[0][0] = f32::MAX;
    overflowing[1][0] = f32::MAX;
    assert_eq!(
        affine_world_aabb(local, overflowing),
        Err(RenderDataError::InvalidTransform)
    );
}

#[test]
fn default_instance_is_protected_and_flags_are_separate() {
    let mut data = data();
    let created = data.create_mesh(info()).unwrap();
    assert!(data.instance(created.default_instance).unwrap().is_default);
    assert_eq!(data.mesh(created.mesh).unwrap().flags.bits(), 2);
    assert_eq!(
        data.instance(created.default_instance).unwrap().flags,
        RenderFlags::VISIBLE
    );
    assert_eq!(
        data.destroy_instance(created.default_instance),
        Err(RenderDataError::CannotDestroyDefaultInstance)
    );
    data.set_mesh_flags(created.mesh, RenderFlags::NONE)
        .unwrap();
    assert_eq!(data.mesh(created.mesh).unwrap().flags, RenderFlags::NONE);
    assert_eq!(
        data.instance(created.default_instance).unwrap().flags,
        RenderFlags::VISIBLE
    );
}

#[test]
fn stale_mesh_and_instance_handles_are_rejected_after_reuse() {
    let mut data = data();
    let first = data.create_mesh(info()).unwrap();
    let old_instance = data
        .create_instance(first.mesh, IDENTITY_MODEL_TRANSFORM, RenderFlags::NONE)
        .unwrap();
    data.destroy_instance(old_instance).unwrap();
    let replacement = data
        .create_instance(first.mesh, IDENTITY_MODEL_TRANSFORM, RenderFlags::NONE)
        .unwrap();
    assert_eq!(old_instance.slot(), replacement.slot());
    assert_ne!(old_instance.generation(), replacement.generation());
    assert!(data.instance(old_instance).is_none());
    data.destroy_mesh(first.mesh).unwrap();
    let second = data.create_mesh(info()).unwrap();
    assert_eq!(first.mesh.slot(), second.mesh.slot());
    assert_ne!(first.mesh.generation(), second.mesh.generation());
    assert!(data.mesh(first.mesh).is_none());
}

#[test]
fn clear_handles_all_slot_states_retains_capacity_and_never_reuses_retired() {
    let mut data = data();
    let mesh = data.create_mesh(info()).unwrap();
    let vacant = data
        .create_instance(mesh.mesh, IDENTITY_MODEL_TRANSFORM, RenderFlags::NONE)
        .unwrap();
    data.destroy_instance(vacant).unwrap();
    data.instances
        .slots
        .force_generation(mesh.default_instance.slot(), u32::MAX);
    let old_capacity = data.capacities();
    data.clear().unwrap();
    assert_eq!(data.capacities(), old_capacity);
    assert_eq!(data.mesh_count(), 0);
    assert_eq!(data.instance_count(), 0);
    assert!(data.mesh(mesh.mesh).is_none());
    assert!(matches!(
        data.instances.slots.states[mesh.default_instance.slot() as usize],
        SlotState::Retired
    ));
    let new_mesh = data.create_mesh(info()).unwrap();
    assert_ne!(
        new_mesh.default_instance.slot(),
        mesh.default_instance.slot()
    );
}

#[test]
fn capacity_math_has_exact_bounded_and_unbounded_overflow_behavior() {
    assert_eq!(next_capacity(0, 1, None, "x"), Ok(1));
    assert_eq!(next_capacity(1, 2, None, "x"), Ok(2));
    assert_eq!(next_capacity(2, 3, Some(3), "x"), Ok(3));
    assert_eq!(
        next_capacity(u32::MAX - 1, u32::MAX, Some(u32::MAX), "x"),
        Ok(u32::MAX)
    );
    assert!(matches!(
        next_capacity(u32::MAX - 1, u32::MAX, None, "x"),
        Err(RenderDataError::CapacityOverflow { .. })
    ));
    assert!(matches!(
        next_capacity(2, 4, Some(3), "x"),
        Err(RenderDataError::CapacityExceeded { .. })
    ));
}

#[test]
fn all_storage_classes_grow_and_retired_slots_force_max_checked_append() {
    let mut data = data();
    let mesh = data.create_mesh(info()).unwrap();
    assert_eq!(
        data.capacities(),
        RenderDataCapacities {
            vertices: 3,
            indices: 3,
            meshes: 1,
            instances: 1,
        }
    );
    data.create_instance(mesh.mesh, IDENTITY_MODEL_TRANSFORM, RenderFlags::NONE)
        .unwrap();
    assert_eq!(data.capacities().instances, 2);

    let mut slots = SlotTable::new(0, Some(1), "test").unwrap();
    slots.reserve_for_len(1, "test").unwrap();
    let prepared = slots.prepare().unwrap();
    slots.commit(prepared);
    slots.force_generation(0, u32::MAX);
    slots.remove(0, u32::MAX);
    assert!(matches!(
        slots.reserve_for_len(slots.required_len_for_prepare().unwrap(), "test"),
        Err(RenderDataError::CapacityExceeded { .. })
    ));
}

#[test]
fn allocator_checks_errors_splits_first_fit_coalesces_and_trims_tail() {
    let mut allocator = RangeAllocator::default();
    assert_eq!(allocator.allocate(0), Err(RenderDataError::EmptyRange));
    let left = allocator.allocate(2).unwrap();
    let middle = allocator.allocate(4).unwrap();
    let right = allocator.allocate(2).unwrap();
    assert_eq!(allocator.free(middle.clone()), Ok(8));
    assert_eq!(allocator.allocate(2).unwrap(), 2..4);
    assert_eq!(allocator.free(2..4), Ok(8));
    assert_eq!(allocator.free(2..4), Err(RenderDataError::RangeOverlap));
    assert_eq!(allocator.free(8..9), Err(RenderDataError::RangeOutOfBounds));
    assert_eq!(allocator.free(3..3), Err(RenderDataError::EmptyRange));
    assert_eq!(allocator.free(left), Ok(8));
    assert_eq!(allocator.free(right), Ok(0));

    let mut bridge = RangeAllocator::default();
    bridge.allocate(6).unwrap();
    bridge.free(0..2).unwrap();
    bridge.free(4..6).unwrap();
    bridge.free(2..4).unwrap();
    assert_eq!(bridge.high_water(), 0);

    let mut overflow = RangeAllocator::default();
    overflow.high_water = u32::MAX;
    assert_eq!(overflow.allocate(1), Err(RenderDataError::RangeOverflow));
}

#[test]
fn streams_remain_coordinated_across_interior_delete_tail_delete_and_reuse() {
    let mut data = data();
    let first = data.create_mesh(info()).unwrap();
    let second = data.create_mesh(info()).unwrap();
    assert_eq!(data.streams().positions.len(), 6);
    assert_eq!(data.indices().len(), 6);
    data.destroy_mesh(first.mesh).unwrap();
    assert_eq!(data.streams().positions.len(), 6);
    let reused = data.create_mesh(info()).unwrap();
    assert_eq!(data.mesh(reused.mesh).unwrap().geometry.vertex_start, 0);
    data.destroy_mesh(second.mesh).unwrap();
    assert_eq!(data.streams().positions.len(), 3);
    assert_eq!(data.streams().normals.len(), 3);
    assert_eq!(data.streams().uvs.len(), 3);
    assert_eq!(data.indices().len(), 3);
}

#[test]
fn failed_default_instance_preparation_rolls_back_empty_and_existing_geometry() {
    let mut data = RenderData::new(RenderDataConfig {
        initial_vertices: 0,
        initial_indices: 0,
        initial_meshes: 0,
        initial_instances: 0,
        max_instances: Some(0),
        ..RenderDataConfig::default()
    })
    .unwrap();
    for _ in 0..2 {
        let generations = data.meshes.slots.generations.clone();
        assert!(matches!(
            data.create_mesh(info()),
            Err(RenderDataError::CapacityExceeded {
                resource: "instances",
                ..
            })
        ));
        assert_eq!(data.vertices.allocator.high_water(), 0);
        assert_eq!(data.indices.allocator.high_water(), 0);
        assert!(data.streams().positions.is_empty());
        assert!(data.indices().is_empty());
        assert_eq!(data.meshes.slots.generations, generations);
    }

    data.instances.slots.max_capacity = Some(1);
    let existing = data.create_mesh(info()).unwrap();
    data.instances.slots.max_capacity = Some(0);
    assert!(data.create_mesh(info()).is_err());
    assert_eq!(data.vertices.allocator.high_water(), 3);
    assert_eq!(data.mesh_count(), 1);
    assert!(data.mesh(existing.mesh).is_some());
}

#[test]
fn aabb_supports_one_point_and_multiple_points() {
    let point = [[2.0, -3.0, 4.0]];
    let normal = [[0.0, 1.0, 0.0]];
    let uv = [[0.0, 0.0]];
    let index = [0];
    let mut one = info();
    one.positions = &point;
    one.normals = &normal;
    one.uvs = &uv;
    one.indices = &index;
    let mut data = data();
    let mesh = data.create_mesh(one).unwrap();
    assert_eq!(
        data.mesh(mesh.mesh).unwrap().aabb,
        Aabb {
            min: point[0],
            max: point[0]
        }
    );
    let mesh = data.create_mesh(info()).unwrap();
    assert_eq!(
        data.mesh(mesh.mesh).unwrap().aabb,
        Aabb {
            min: [-1.0, -2.0, -3.0],
            max: [4.0, 2.0, 3.0],
        }
    );
}

#[test]
fn malformed_geometry_matrix_is_rejected_without_consumption() {
    let mut data = data();
    let mut candidate = info();
    candidate.positions = &[];
    assert_eq!(
        data.create_mesh(candidate).unwrap_err(),
        RenderDataError::EmptyVertices
    );
    let short_normals = &NORMALS[..2];
    let mut candidate = info();
    candidate.normals = short_normals;
    assert_eq!(
        data.create_mesh(candidate).unwrap_err(),
        RenderDataError::MismatchedVertexStreams
    );
    let short_uvs = &UVS[..2];
    let mut candidate = info();
    candidate.uvs = short_uvs;
    assert_eq!(
        data.create_mesh(candidate).unwrap_err(),
        RenderDataError::MismatchedVertexStreams
    );
    let mut candidate = info();
    candidate.indices = &[];
    assert_eq!(
        data.create_mesh(candidate).unwrap_err(),
        RenderDataError::EmptyIndices
    );

    for stream in 0..3 {
        for bad in [f32::NAN, f32::INFINITY] {
            let mut positions = POSITIONS;
            let mut normals = NORMALS;
            let mut uvs = UVS;
            match stream {
                0 => positions[0][0] = bad,
                1 => normals[0][0] = bad,
                _ => uvs[0][0] = bad,
            }
            let mut candidate = info();
            candidate.positions = &positions;
            candidate.normals = &normals;
            candidate.uvs = &uvs;
            assert_eq!(
                data.create_mesh(candidate).unwrap_err(),
                RenderDataError::NonFiniteGeometry
            );
        }
    }
    let invalid = [3];
    let mut candidate = info();
    candidate.indices = &invalid;
    assert_eq!(
        data.create_mesh(candidate).unwrap_err(),
        RenderDataError::IndexOutOfBounds
    );
    let valid_last = [2];
    let mut candidate = info();
    candidate.indices = &valid_last;
    assert!(data.create_mesh(candidate).is_ok());
}

#[test]
fn normal_matrices_and_failed_transform_operations_are_transactional() {
    let mut data = data();
    let mesh = data.create_mesh(info()).unwrap();
    assert_eq!(
        data.instance(mesh.default_instance).unwrap().normal,
        IDENTITY_NORMAL_MATRIX
    );
    let translation = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [4.0, 5.0, 6.0, 1.0],
    ];
    data.set_instance_transform(mesh.default_instance, translation)
        .unwrap();
    assert_eq!(
        data.instance(mesh.default_instance).unwrap().normal,
        IDENTITY_NORMAL_MATRIX
    );
    let scale = [
        [2.0, 0.0, 0.0, 0.0],
        [0.0, 4.0, 0.0, 0.0],
        [0.0, 0.0, 0.5, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    data.set_instance_transform(mesh.default_instance, scale)
        .unwrap();
    assert_eq!(
        data.instance(mesh.default_instance).unwrap().normal,
        [[0.5, 0.0, 0.0], [0.0, 0.25, 0.0], [0.0, 0.0, 2.0]]
    );
    let rotation = [
        [0.0, 1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    data.set_instance_transform(mesh.default_instance, rotation)
        .unwrap();
    let old = data.instance(mesh.default_instance).unwrap();
    for invalid in [[[0.0; 4]; 4], {
        let mut value = IDENTITY_MODEL_TRANSFORM;
        value[0][0] = f32::INFINITY;
        value
    }] {
        assert_eq!(
            data.set_instance_transform(mesh.default_instance, invalid),
            Err(RenderDataError::InvalidTransform)
        );
        assert_eq!(data.instance(mesh.default_instance).unwrap(), old);
        let count = data.instance_count();
        assert_eq!(
            data.create_instance(mesh.mesh, invalid, RenderFlags::NONE),
            Err(RenderDataError::InvalidTransform)
        );
        assert_eq!(data.instance_count(), count);
        let mut candidate = info();
        candidate.default_transform = invalid;
        assert_eq!(
            data.create_mesh(candidate).unwrap_err(),
            RenderDataError::InvalidTransform
        );
    }
}

#[test]
fn destroying_mesh_invalidates_exact_owner_instances_with_reused_generations() {
    let mut data = data();
    let first = data.create_mesh(info()).unwrap();
    let second = data.create_mesh(info()).unwrap();
    let first_extra = data
        .create_instance(first.mesh, IDENTITY_MODEL_TRANSFORM, RenderFlags::NONE)
        .unwrap();
    let second_extra = data
        .create_instance(second.mesh, IDENTITY_MODEL_TRANSFORM, RenderFlags::NONE)
        .unwrap();
    data.destroy_instance(first_extra).unwrap();
    let reused = data
        .create_instance(second.mesh, IDENTITY_MODEL_TRANSFORM, RenderFlags::NONE)
        .unwrap();
    assert_eq!(first_extra.slot(), reused.slot());
    data.destroy_mesh(first.mesh).unwrap();
    assert!(data.instance(first.default_instance).is_none());
    assert!(data.instance(second.default_instance).is_some());
    assert!(data.instance(second_extra).is_some());
    assert!(data.instance(reused).is_some());
    assert_eq!(data.instances().count(), 3);
}

#[test]
fn revision_changes_only_after_success_and_replacement_rejects_old_handles() {
    let mut data = data();
    assert_eq!(data.revision(), 0);
    assert!(data.destroy_mesh(MeshHandle::from_parts(9, 9)).is_err());
    assert_eq!(data.revision(), 0);
    let old = data.create_mesh(info()).unwrap();
    assert_eq!(data.revision(), 1);
    let mut stage = data.replacement_stage().unwrap();
    let new = stage.create_mesh(info()).unwrap();
    assert_ne!(old.mesh, new.mesh);
    data.replace_with(stage).unwrap();
    assert_eq!(data.revision(), 2);
    assert!(data.mesh(old.mesh).is_none());
    assert!(data.mesh(new.mesh).is_some());
}

#[test]
fn replacement_stage_is_rejected_after_source_mutation() {
    let mut data = data();
    let original = data.create_mesh(info()).unwrap();
    let mut stage = data.replacement_stage().unwrap();
    stage.create_mesh(info()).unwrap();

    data.destroy_mesh(original.mesh).unwrap();
    let current = data.create_mesh(info()).unwrap();
    assert_eq!(current.mesh.slot(), original.mesh.slot());

    assert_eq!(
        data.replace_with(stage),
        Err(RenderDataError::StaleReplacementStage)
    );
    assert!(data.mesh(current.mesh).is_some());
}

#[test]
fn replacement_stage_is_rejected_by_a_different_render_data() {
    let source = data();
    let stage = source.replacement_stage().unwrap();
    let mut other = data();

    assert_eq!(
        other.replace_with(stage),
        Err(RenderDataError::StaleReplacementStage)
    );
}

#[test]
fn revision_overflow_rejects_mutation_without_committing() {
    let mut data = data();
    data.revision = u64::MAX;
    assert_eq!(
        data.create_mesh(info()),
        Err(RenderDataError::RevisionOverflow)
    );
    assert_eq!(data.mesh_count(), 0);
    assert_eq!(data.revision(), u64::MAX);
}
