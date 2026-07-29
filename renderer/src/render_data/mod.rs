mod handle;
mod range_allocator;

pub use handle::{InstanceHandle, MeshHandle};

use std::{
    ops::Deref,
    sync::atomic::{AtomicU64, Ordering},
};

use handle::{PreparedSlot, SlotTable};
use range_allocator::RangeAllocator;
use thiserror::Error;

static NEXT_LINEAGE: AtomicU64 = AtomicU64::new(1);

pub type ModelTransform = [[f32; 4]; 4];
pub type NormalMatrix = [[f32; 3]; 3];

pub const IDENTITY_MODEL_TRANSFORM: ModelTransform = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];
pub const IDENTITY_NORMAL_MATRIX: NormalMatrix =
    [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PipelineKey(u32);

impl PipelineKey {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Stable CPU-side identity for a device-independent material.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct MaterialKey(u32);

impl MaterialKey {
    /// The glTF/default material.
    pub const DEFAULT: Self = Self(0);

    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceType {
    pub words: [u32; 16],
}

impl InstanceType {
    pub const ZERO: Self = Self { words: [0; 16] };
}

const _: [(); 64] = [(); std::mem::size_of::<InstanceType>()];
const _: [(); 4] = [(); std::mem::align_of::<InstanceType>()];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GeometryRange {
    pub vertex_start: u32,
    pub vertex_count: u32,
    pub index_start: u32,
    pub index_count: u32,
}

pub struct MeshCreateInfo<'a> {
    pub positions: &'a [[f32; 3]],
    pub normals: &'a [[f32; 3]],
    pub tangents: &'a [[f32; 4]],
    pub uvs: &'a [[f32; 2]],
    pub indices: &'a [u32],
    pub pipeline: PipelineKey,
    pub material: MaterialKey,
    pub default_instance_type: InstanceType,
    pub default_transform: ModelTransform,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreatedMesh {
    pub mesh: MeshHandle,
    pub default_instance: InstanceHandle,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshView {
    pub handle: MeshHandle,
    pub geometry: GeometryRange,
    pub pipeline: PipelineKey,
    pub material: MaterialKey,
    pub default_instance_type: InstanceType,
    pub local_aabb: Aabb,
    pub default_instance: InstanceHandle,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InstanceView {
    pub handle: InstanceHandle,
    pub mesh: MeshHandle,
    pub model: ModelTransform,
    pub normal: NormalMatrix,
    pub instance_type: InstanceType,
    pub is_default: bool,
}

/// Computes the exact axis-aligned bounds of an AABB under a finite affine transform.
pub fn affine_world_aabb(local: Aabb, model: ModelTransform) -> Result<Aabb, RenderDataError> {
    validate_affine(model)?;
    let center = [
        (local.min[0] + local.max[0]) * 0.5,
        (local.min[1] + local.max[1]) * 0.5,
        (local.min[2] + local.max[2]) * 0.5,
    ];
    let extent = [
        (local.max[0] - local.min[0]) * 0.5,
        (local.max[1] - local.min[1]) * 0.5,
        (local.max[2] - local.min[2]) * 0.5,
    ];
    let mut world_center = [0.0; 3];
    let mut world_extent = [0.0; 3];
    for row in 0..3 {
        world_center[row] = model[3][row]
            + model[0][row] * center[0]
            + model[1][row] * center[1]
            + model[2][row] * center[2];
        world_extent[row] = model[0][row].abs() * extent[0]
            + model[1][row].abs() * extent[1]
            + model[2][row].abs() * extent[2];
    }
    let result = Aabb {
        min: std::array::from_fn(|i| world_center[i] - world_extent[i]),
        max: std::array::from_fn(|i| world_center[i] + world_extent[i]),
    };
    if result
        .min
        .iter()
        .chain(result.max.iter())
        .all(|v| v.is_finite())
    {
        Ok(result)
    } else {
        Err(RenderDataError::InvalidTransform)
    }
}

pub struct VertexStreams<'a> {
    pub positions: &'a [[f32; 3]],
    pub normals: &'a [[f32; 3]],
    pub tangents: &'a [[f32; 4]],
    pub uvs: &'a [[f32; 2]],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderDataCapacities {
    pub vertices: u32,
    pub indices: u32,
    pub meshes: u32,
    pub instances: u32,
}

#[derive(Clone, Debug)]
pub struct RenderDataConfig {
    pub initial_vertices: u32,
    pub max_vertices: Option<u32>,
    pub initial_indices: u32,
    pub max_indices: Option<u32>,
    pub initial_meshes: u32,
    pub max_meshes: Option<u32>,
    pub initial_instances: u32,
    pub max_instances: Option<u32>,
}

impl Default for RenderDataConfig {
    fn default() -> Self {
        Self {
            initial_vertices: 262_144,
            max_vertices: None,
            initial_indices: 524_288,
            max_indices: None,
            initial_meshes: 16_384,
            max_meshes: None,
            initial_instances: 65_536,
            max_instances: None,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RenderDataError {
    #[error("initial {resource} capacity exceeds configured maximum")]
    InvalidCapacityConfig { resource: &'static str },
    #[error("capacity arithmetic overflow for {resource}")]
    CapacityOverflow { resource: &'static str },
    #[error("{resource} capacity {required} exceeds maximum {maximum}")]
    CapacityExceeded {
        resource: &'static str,
        required: u32,
        maximum: u32,
    },
    #[error("allocation failed while reserving {resource}")]
    AllocationFailed { resource: &'static str },
    #[error("input length does not fit in u32")]
    InputTooLarge,
    #[error("vertex streams must be nonempty")]
    EmptyVertices,
    #[error("vertex stream lengths do not match")]
    MismatchedVertexStreams,
    #[error("indices must be nonempty")]
    EmptyIndices,
    #[error("an index refers beyond the supplied vertices")]
    IndexOutOfBounds,
    #[error("geometry contains non-finite values")]
    NonFiniteGeometry,
    #[error("invalid mesh handle")]
    InvalidMeshHandle,
    #[error("invalid instance handle")]
    InvalidInstanceHandle,
    #[error("transform must be finite and invertible")]
    InvalidTransform,
    #[error("the default instance cannot be destroyed directly")]
    CannotDestroyDefaultInstance,
    #[error("range is empty")]
    EmptyRange,
    #[error("range arithmetic overflow")]
    RangeOverflow,
    #[error("range is beyond the allocator high-water mark")]
    RangeOutOfBounds,
    #[error("range overlaps an already-free range")]
    RangeOverlap,
    #[error("render data revision overflow")]
    RevisionOverflow,
    #[error("replacement stage no longer matches its source render data")]
    StaleReplacementStage,
}

struct VertexSoa {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    tangents: Vec<[f32; 4]>,
    uvs: Vec<[f32; 2]>,
    logical_capacity: u32,
    max_capacity: Option<u32>,
    allocator: RangeAllocator,
}

struct IndexSoa {
    values: Vec<u32>,
    logical_capacity: u32,
    max_capacity: Option<u32>,
    allocator: RangeAllocator,
}

struct MeshSoa {
    slots: SlotTable,
    vertex_starts: Vec<u32>,
    vertex_counts: Vec<u32>,
    index_starts: Vec<u32>,
    index_counts: Vec<u32>,
    pipeline_keys: Vec<PipelineKey>,
    material_keys: Vec<MaterialKey>,
    default_instance_types: Vec<InstanceType>,
    aabb_mins: Vec<[f32; 3]>,
    aabb_maxs: Vec<[f32; 3]>,
    default_instance_slots: Vec<u32>,
    default_instance_generations: Vec<u32>,
}

struct InstanceSoa {
    slots: SlotTable,
    mesh_slots: Vec<u32>,
    mesh_generations: Vec<u32>,
    model_col_0: Vec<[f32; 4]>,
    model_col_1: Vec<[f32; 4]>,
    model_col_2: Vec<[f32; 4]>,
    model_col_3: Vec<[f32; 4]>,
    normal_col_0: Vec<[f32; 3]>,
    normal_col_1: Vec<[f32; 3]>,
    normal_col_2: Vec<[f32; 3]>,
    instance_types: Vec<InstanceType>,
}

pub struct RenderData {
    vertices: VertexSoa,
    indices: IndexSoa,
    meshes: MeshSoa,
    instances: InstanceSoa,
    revision: u64,
    lineage: u64,
}

/// A transaction prepared specifically as a successor to an existing scene.
pub struct ReplacementStage {
    data: RenderData,
    source_lineage: u64,
    base_revision: u64,
    mesh_generations: Vec<u32>,
    instance_generations: Vec<u32>,
}

impl Deref for ReplacementStage {
    type Target = RenderData;
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl ReplacementStage {
    pub fn create_mesh(
        &mut self,
        info: MeshCreateInfo<'_>,
    ) -> Result<CreatedMesh, RenderDataError> {
        self.data.create_mesh(info)
    }

    pub fn create_instance(
        &mut self,
        mesh: MeshHandle,
        model: ModelTransform,
        instance_type: InstanceType,
    ) -> Result<InstanceHandle, RenderDataError> {
        self.data.create_instance(mesh, model, instance_type)
    }
}

pub(super) fn next_capacity(
    old: u32,
    required: u32,
    maximum: Option<u32>,
    resource: &'static str,
) -> Result<u32, RenderDataError> {
    if let Some(maximum) = maximum {
        if required > maximum {
            return Err(RenderDataError::CapacityExceeded {
                resource,
                required,
                maximum,
            });
        }
    }
    if required <= old {
        return Ok(old);
    }
    let doubled = match old.checked_mul(2) {
        Some(value) => value,
        None => maximum.ok_or(RenderDataError::CapacityOverflow { resource })?,
    };
    let target = required.max(1).max(doubled);
    Ok(maximum.map_or(target, |maximum| target.min(maximum)))
}

pub(super) fn reserve_vec<T>(
    vector: &mut Vec<T>,
    target_capacity: u32,
    resource: &'static str,
) -> Result<(), RenderDataError> {
    let target = usize::try_from(target_capacity)
        .map_err(|_| RenderDataError::CapacityOverflow { resource })?;
    if vector.capacity() < target {
        vector
            .try_reserve_exact(target - vector.len())
            .map_err(|_| RenderDataError::AllocationFailed { resource })?;
    }
    Ok(())
}

impl RenderData {
    pub fn new(config: RenderDataConfig) -> Result<Self, RenderDataError> {
        for (resource, initial, maximum) in [
            ("vertices", config.initial_vertices, config.max_vertices),
            ("indices", config.initial_indices, config.max_indices),
            ("meshes", config.initial_meshes, config.max_meshes),
            ("instances", config.initial_instances, config.max_instances),
        ] {
            if maximum.is_some_and(|maximum| initial > maximum) {
                return Err(RenderDataError::InvalidCapacityConfig { resource });
            }
        }

        let lineage = NEXT_LINEAGE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| RenderDataError::CapacityOverflow {
                resource: "render data lineage",
            })?;
        let mut data = Self {
            vertices: VertexSoa {
                positions: Vec::new(),
                normals: Vec::new(),
                tangents: Vec::new(),
                uvs: Vec::new(),
                logical_capacity: 0,
                max_capacity: config.max_vertices,
                allocator: RangeAllocator::default(),
            },
            indices: IndexSoa {
                values: Vec::new(),
                logical_capacity: 0,
                max_capacity: config.max_indices,
                allocator: RangeAllocator::default(),
            },
            meshes: MeshSoa::new(config.initial_meshes, config.max_meshes)?,
            instances: InstanceSoa::new(config.initial_instances, config.max_instances)?,
            revision: 0,
            lineage,
        };
        data.reserve_vertices(config.initial_vertices)?;
        data.reserve_indices(config.initial_indices)?;
        Ok(data)
    }

    pub fn capacities(&self) -> RenderDataCapacities {
        RenderDataCapacities {
            vertices: self.vertices.logical_capacity,
            indices: self.indices.logical_capacity,
            meshes: self.meshes.slots.logical_capacity(),
            instances: self.instances.slots.logical_capacity(),
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Creates an empty transactional successor whose handles cannot alias this data.
    pub fn replacement_stage(&self) -> Result<ReplacementStage, RenderDataError> {
        let capacities = self.capacities();
        let mut stage = Self::new(RenderDataConfig {
            initial_vertices: capacities.vertices,
            max_vertices: self.vertices.max_capacity,
            initial_indices: capacities.indices,
            max_indices: self.indices.max_capacity,
            initial_meshes: capacities.meshes,
            max_meshes: self.meshes.slots.max_capacity(),
            initial_instances: capacities.instances,
            max_instances: self.instances.slots.max_capacity(),
        })?;
        stage.meshes.slots.seed_successor(&self.meshes.slots);
        stage.instances.slots.seed_successor(&self.instances.slots);
        Ok(ReplacementStage {
            data: stage,
            source_lineage: self.lineage,
            base_revision: self.revision,
            mesh_generations: self.meshes.slots.generations.clone(),
            instance_generations: self.instances.slots.generations.clone(),
        })
    }

    /// Atomically installs a successfully prepared replacement.
    pub fn replace_with(&mut self, mut stage: ReplacementStage) -> Result<(), RenderDataError> {
        if stage.source_lineage != self.lineage
            || stage.base_revision != self.revision
            || stage.mesh_generations != self.meshes.slots.generations
            || stage.instance_generations != self.instances.slots.generations
        {
            return Err(RenderDataError::StaleReplacementStage);
        }
        stage.data.revision = self.next_revision()?;
        stage.data.lineage = self.lineage;
        *self = stage.data;
        Ok(())
    }

    fn next_revision(&self) -> Result<u64, RenderDataError> {
        self.revision
            .checked_add(1)
            .ok_or(RenderDataError::RevisionOverflow)
    }

    pub fn mesh_count(&self) -> u32 {
        self.meshes.slots.live_count()
    }

    pub fn instance_count(&self) -> u32 {
        self.instances.slots.live_count()
    }

    pub fn create_mesh(
        &mut self,
        info: MeshCreateInfo<'_>,
    ) -> Result<CreatedMesh, RenderDataError> {
        let vertex_count = validate_geometry(&info)?;
        let index_count =
            u32::try_from(info.indices.len()).map_err(|_| RenderDataError::InputTooLarge)?;
        let normal = normal_matrix(info.default_transform)?;
        let bounds = aabb(info.positions);
        affine_world_aabb(bounds, info.default_transform)?;
        let next_revision = self.next_revision()?;

        let mesh_required = self.meshes.slots.required_len_for_prepare()?;
        self.meshes.reserve(mesh_required)?;
        let prepared_mesh = self.meshes.slots.prepare()?;
        let mesh = MeshHandle::from_parts(prepared_mesh.slot, prepared_mesh.generation);

        let vertex_range = self.vertices.allocator.allocate(vertex_count)?;
        let index_range = match self.indices.allocator.allocate(index_count) {
            Ok(range) => range,
            Err(error) => {
                self.vertices.allocator.free(vertex_range).unwrap();
                return Err(error);
            }
        };

        let preparation = (|| {
            self.reserve_vertices(vertex_range.end)?;
            self.reserve_indices(index_range.end)?;
            let instance_required = self.instances.slots.required_len_for_prepare()?;
            self.instances.reserve(instance_required)?;
            self.instances.slots.prepare()
        })();
        let prepared_instance = match preparation {
            Ok(prepared) => prepared,
            Err(error) => {
                self.vertices.allocator.free(vertex_range).unwrap();
                self.indices.allocator.free(index_range).unwrap();
                self.trim_streams();
                return Err(error);
            }
        };
        let default_instance =
            InstanceHandle::from_parts(prepared_instance.slot, prepared_instance.generation);

        self.resize_streams();
        self.vertices.positions[as_usize(vertex_range.start)..as_usize(vertex_range.end)]
            .copy_from_slice(info.positions);
        self.vertices.normals[as_usize(vertex_range.start)..as_usize(vertex_range.end)]
            .copy_from_slice(info.normals);
        self.vertices.tangents[as_usize(vertex_range.start)..as_usize(vertex_range.end)]
            .copy_from_slice(info.tangents);
        self.vertices.uvs[as_usize(vertex_range.start)..as_usize(vertex_range.end)]
            .copy_from_slice(info.uvs);
        self.indices.values[as_usize(index_range.start)..as_usize(index_range.end)]
            .copy_from_slice(info.indices);

        self.meshes.commit(
            prepared_mesh,
            GeometryRange {
                vertex_start: vertex_range.start,
                vertex_count,
                index_start: index_range.start,
                index_count,
            },
            info.pipeline,
            info.material,
            info.default_instance_type,
            bounds,
            default_instance,
        );
        self.instances.commit(
            prepared_instance,
            mesh,
            info.default_transform,
            normal,
            info.default_instance_type,
        );
        self.revision = next_revision;
        Ok(CreatedMesh {
            mesh,
            default_instance,
        })
    }

    pub fn create_instance(
        &mut self,
        mesh: MeshHandle,
        model: ModelTransform,
        instance_type: InstanceType,
    ) -> Result<InstanceHandle, RenderDataError> {
        if !self.meshes.slots.contains(mesh.slot(), mesh.generation()) {
            return Err(RenderDataError::InvalidMeshHandle);
        }
        let normal = normal_matrix(model)?;
        affine_world_aabb(self.mesh(mesh).unwrap().local_aabb, model)?;
        let next_revision = self.next_revision()?;
        let required = self.instances.slots.required_len_for_prepare()?;
        self.instances.reserve(required)?;
        let prepared = self.instances.slots.prepare()?;
        let handle = InstanceHandle::from_parts(prepared.slot, prepared.generation);
        self.instances
            .commit(prepared, mesh, model, normal, instance_type);
        self.revision = next_revision;
        Ok(handle)
    }

    pub fn destroy_instance(&mut self, handle: InstanceHandle) -> Result<(), RenderDataError> {
        let view = self
            .instance(handle)
            .ok_or(RenderDataError::InvalidInstanceHandle)?;
        if self
            .mesh(view.mesh)
            .is_some_and(|mesh| mesh.default_instance == handle)
        {
            return Err(RenderDataError::CannotDestroyDefaultInstance);
        }
        let next_revision = self.next_revision()?;
        self.instances
            .slots
            .remove(handle.slot(), handle.generation());
        self.revision = next_revision;
        Ok(())
    }

    pub fn destroy_mesh(&mut self, handle: MeshHandle) -> Result<(), RenderDataError> {
        let view = self
            .mesh(handle)
            .ok_or(RenderDataError::InvalidMeshHandle)?;
        let next_revision = self.next_revision()?;
        let owned: Vec<_> = self
            .instances
            .slots
            .occupied()
            .filter_map(|(slot, generation)| {
                let instance = InstanceHandle::from_parts(slot, generation);
                (self.instances.mesh_handle(slot) == handle).then_some(instance)
            })
            .collect();
        for instance in owned {
            self.instances
                .slots
                .remove(instance.slot(), instance.generation());
        }
        self.vertices
            .allocator
            .free(
                view.geometry.vertex_start
                    ..view
                        .geometry
                        .vertex_start
                        .checked_add(view.geometry.vertex_count)
                        .expect("live vertex range must not overflow"),
            )
            .expect("live geometry range must be allocated");
        self.indices
            .allocator
            .free(
                view.geometry.index_start
                    ..view
                        .geometry
                        .index_start
                        .checked_add(view.geometry.index_count)
                        .expect("live index range must not overflow"),
            )
            .expect("live geometry range must be allocated");
        self.trim_streams();
        self.meshes.slots.remove(handle.slot(), handle.generation());
        self.revision = next_revision;
        Ok(())
    }

    pub fn set_instance_type(
        &mut self,
        handle: InstanceHandle,
        instance_type: InstanceType,
    ) -> Result<(), RenderDataError> {
        if !self
            .instances
            .slots
            .contains(handle.slot(), handle.generation())
        {
            return Err(RenderDataError::InvalidInstanceHandle);
        }
        let next_revision = self.next_revision()?;
        self.instances.instance_types[handle.slot() as usize] = instance_type;
        self.revision = next_revision;
        Ok(())
    }

    pub fn set_instance_transform(
        &mut self,
        handle: InstanceHandle,
        model: ModelTransform,
    ) -> Result<(), RenderDataError> {
        if !self
            .instances
            .slots
            .contains(handle.slot(), handle.generation())
        {
            return Err(RenderDataError::InvalidInstanceHandle);
        }
        let normal = normal_matrix(model)?;
        let mesh = self.instances.mesh_handle(handle.slot());
        affine_world_aabb(self.mesh(mesh).unwrap().local_aabb, model)?;
        let next_revision = self.next_revision()?;
        self.instances.set_transform(handle.slot(), model, normal);
        self.revision = next_revision;
        Ok(())
    }

    pub fn mesh(&self, handle: MeshHandle) -> Option<MeshView> {
        self.meshes
            .slots
            .contains(handle.slot(), handle.generation())
            .then(|| self.meshes.view(handle))
    }

    pub fn instance(&self, handle: InstanceHandle) -> Option<InstanceView> {
        self.instances
            .slots
            .contains(handle.slot(), handle.generation())
            .then(|| {
                let mesh = self.instances.mesh_handle(handle.slot());
                let default = self
                    .mesh(mesh)
                    .is_some_and(|owner| owner.default_instance == handle);
                self.instances.view(handle, default)
            })
    }

    pub fn meshes(&self) -> impl Iterator<Item = (MeshHandle, MeshView)> + '_ {
        self.meshes.slots.occupied().map(|(slot, generation)| {
            let handle = MeshHandle::from_parts(slot, generation);
            (handle, self.meshes.view(handle))
        })
    }

    pub fn instances(&self) -> impl Iterator<Item = (InstanceHandle, InstanceView)> + '_ {
        self.instances.slots.occupied().map(|(slot, generation)| {
            let handle = InstanceHandle::from_parts(slot, generation);
            (handle, self.instance(handle).unwrap())
        })
    }

    pub fn streams(&self) -> VertexStreams<'_> {
        VertexStreams {
            positions: &self.vertices.positions,
            normals: &self.vertices.normals,
            tangents: &self.vertices.tangents,
            uvs: &self.vertices.uvs,
        }
    }

    pub fn indices(&self) -> &[u32] {
        &self.indices.values
    }

    pub fn clear(&mut self) -> Result<(), RenderDataError> {
        if self.mesh_count() == 0 && self.instance_count() == 0 {
            return Ok(());
        }
        let next_revision = self.next_revision()?;
        self.meshes.slots.clear();
        self.instances.slots.clear();
        self.vertices.positions.clear();
        self.vertices.normals.clear();
        self.vertices.tangents.clear();
        self.vertices.uvs.clear();
        self.indices.values.clear();
        self.vertices.allocator.clear();
        self.indices.allocator.clear();
        self.revision = next_revision;
        Ok(())
    }

    fn reserve_vertices(&mut self, required: u32) -> Result<(), RenderDataError> {
        let target = next_capacity(
            self.vertices.logical_capacity,
            required,
            self.vertices.max_capacity,
            "vertices",
        )?;
        reserve_vec(&mut self.vertices.positions, target, "vertices")?;
        reserve_vec(&mut self.vertices.normals, target, "vertices")?;
        reserve_vec(&mut self.vertices.tangents, target, "vertices")?;
        reserve_vec(&mut self.vertices.uvs, target, "vertices")?;
        self.vertices.logical_capacity = target;
        Ok(())
    }

    fn reserve_indices(&mut self, required: u32) -> Result<(), RenderDataError> {
        let target = next_capacity(
            self.indices.logical_capacity,
            required,
            self.indices.max_capacity,
            "indices",
        )?;
        reserve_vec(&mut self.indices.values, target, "indices")?;
        self.indices.logical_capacity = target;
        Ok(())
    }

    fn resize_streams(&mut self) {
        let vertices = as_usize(self.vertices.allocator.high_water());
        self.vertices.positions.resize(vertices, [0.0; 3]);
        self.vertices.normals.resize(vertices, [0.0; 3]);
        self.vertices
            .tangents
            .resize(vertices, [0.0, 0.0, 0.0, 1.0]);
        self.vertices.uvs.resize(vertices, [0.0; 2]);
        self.indices
            .values
            .resize(as_usize(self.indices.allocator.high_water()), 0);
    }

    fn trim_streams(&mut self) {
        self.vertices
            .positions
            .truncate(as_usize(self.vertices.allocator.high_water()));
        self.vertices
            .normals
            .truncate(as_usize(self.vertices.allocator.high_water()));
        self.vertices
            .tangents
            .truncate(as_usize(self.vertices.allocator.high_water()));
        self.vertices
            .uvs
            .truncate(as_usize(self.vertices.allocator.high_water()));
        self.indices
            .values
            .truncate(as_usize(self.indices.allocator.high_water()));
    }
}

impl MeshSoa {
    fn new(initial: u32, maximum: Option<u32>) -> Result<Self, RenderDataError> {
        let mut soa = Self {
            slots: SlotTable::new(0, maximum, "meshes")?,
            vertex_starts: Vec::new(),
            vertex_counts: Vec::new(),
            index_starts: Vec::new(),
            index_counts: Vec::new(),
            pipeline_keys: Vec::new(),
            material_keys: Vec::new(),
            default_instance_types: Vec::new(),
            aabb_mins: Vec::new(),
            aabb_maxs: Vec::new(),
            default_instance_slots: Vec::new(),
            default_instance_generations: Vec::new(),
        };
        soa.reserve(initial)?;
        Ok(soa)
    }

    fn reserve(&mut self, required: u32) -> Result<(), RenderDataError> {
        let old = self.slots.logical_capacity();
        let maximum = self.slots.max_capacity();
        let target = next_capacity(old, required, maximum, "meshes")?;
        reserve_vec(&mut self.vertex_starts, target, "meshes")?;
        reserve_vec(&mut self.vertex_counts, target, "meshes")?;
        reserve_vec(&mut self.index_starts, target, "meshes")?;
        reserve_vec(&mut self.index_counts, target, "meshes")?;
        reserve_vec(&mut self.pipeline_keys, target, "meshes")?;
        reserve_vec(&mut self.material_keys, target, "meshes")?;
        reserve_vec(&mut self.default_instance_types, target, "meshes")?;
        reserve_vec(&mut self.aabb_mins, target, "meshes")?;
        reserve_vec(&mut self.aabb_maxs, target, "meshes")?;
        reserve_vec(&mut self.default_instance_slots, target, "meshes")?;
        reserve_vec(&mut self.default_instance_generations, target, "meshes")?;
        self.slots.reserve_for_len(target, "meshes")?;
        Ok(())
    }

    fn commit(
        &mut self,
        prepared: PreparedSlot,
        geometry: GeometryRange,
        pipeline: PipelineKey,
        material: MaterialKey,
        default_instance_type: InstanceType,
        bounds: Aabb,
        default: InstanceHandle,
    ) {
        let len = prepared.slot as usize + 1;
        resize_column(&mut self.vertex_starts, len, 0);
        resize_column(&mut self.vertex_counts, len, 0);
        resize_column(&mut self.index_starts, len, 0);
        resize_column(&mut self.index_counts, len, 0);
        resize_column(&mut self.pipeline_keys, len, PipelineKey::new(0));
        resize_column(&mut self.material_keys, len, MaterialKey::DEFAULT);
        resize_column(&mut self.default_instance_types, len, InstanceType::ZERO);
        resize_column(&mut self.aabb_mins, len, [0.0; 3]);
        resize_column(&mut self.aabb_maxs, len, [0.0; 3]);
        resize_column(&mut self.default_instance_slots, len, 0);
        resize_column(&mut self.default_instance_generations, len, 0);
        let index = prepared.slot as usize;
        self.vertex_starts[index] = geometry.vertex_start;
        self.vertex_counts[index] = geometry.vertex_count;
        self.index_starts[index] = geometry.index_start;
        self.index_counts[index] = geometry.index_count;
        self.pipeline_keys[index] = pipeline;
        self.material_keys[index] = material;
        self.default_instance_types[index] = default_instance_type;
        self.aabb_mins[index] = bounds.min;
        self.aabb_maxs[index] = bounds.max;
        self.default_instance_slots[index] = default.slot();
        self.default_instance_generations[index] = default.generation();
        self.slots.commit(prepared);
    }

    fn view(&self, handle: MeshHandle) -> MeshView {
        let index = handle.slot() as usize;
        MeshView {
            handle,
            geometry: GeometryRange {
                vertex_start: self.vertex_starts[index],
                vertex_count: self.vertex_counts[index],
                index_start: self.index_starts[index],
                index_count: self.index_counts[index],
            },
            pipeline: self.pipeline_keys[index],
            material: self.material_keys[index],
            default_instance_type: self.default_instance_types[index],
            local_aabb: Aabb {
                min: self.aabb_mins[index],
                max: self.aabb_maxs[index],
            },
            default_instance: InstanceHandle::from_parts(
                self.default_instance_slots[index],
                self.default_instance_generations[index],
            ),
        }
    }
}

impl InstanceSoa {
    fn new(initial: u32, maximum: Option<u32>) -> Result<Self, RenderDataError> {
        let mut soa = Self {
            slots: SlotTable::new(0, maximum, "instances")?,
            mesh_slots: Vec::new(),
            mesh_generations: Vec::new(),
            model_col_0: Vec::new(),
            model_col_1: Vec::new(),
            model_col_2: Vec::new(),
            model_col_3: Vec::new(),
            normal_col_0: Vec::new(),
            normal_col_1: Vec::new(),
            normal_col_2: Vec::new(),
            instance_types: Vec::new(),
        };
        soa.reserve(initial)?;
        Ok(soa)
    }

    fn reserve(&mut self, required: u32) -> Result<(), RenderDataError> {
        let old = self.slots.logical_capacity();
        let maximum = self.slots.max_capacity();
        let target = next_capacity(old, required, maximum, "instances")?;
        reserve_vec(&mut self.mesh_slots, target, "instances")?;
        reserve_vec(&mut self.mesh_generations, target, "instances")?;
        reserve_vec(&mut self.model_col_0, target, "instances")?;
        reserve_vec(&mut self.model_col_1, target, "instances")?;
        reserve_vec(&mut self.model_col_2, target, "instances")?;
        reserve_vec(&mut self.model_col_3, target, "instances")?;
        reserve_vec(&mut self.normal_col_0, target, "instances")?;
        reserve_vec(&mut self.normal_col_1, target, "instances")?;
        reserve_vec(&mut self.normal_col_2, target, "instances")?;
        reserve_vec(&mut self.instance_types, target, "instances")?;
        self.slots.reserve_for_len(target, "instances")?;
        Ok(())
    }

    fn commit(
        &mut self,
        prepared: PreparedSlot,
        mesh: MeshHandle,
        model: ModelTransform,
        normal: NormalMatrix,
        instance_type: InstanceType,
    ) {
        let len = prepared.slot as usize + 1;
        resize_column(&mut self.mesh_slots, len, 0);
        resize_column(&mut self.mesh_generations, len, 0);
        resize_column(&mut self.model_col_0, len, [0.0; 4]);
        resize_column(&mut self.model_col_1, len, [0.0; 4]);
        resize_column(&mut self.model_col_2, len, [0.0; 4]);
        resize_column(&mut self.model_col_3, len, [0.0; 4]);
        resize_column(&mut self.normal_col_0, len, [0.0; 3]);
        resize_column(&mut self.normal_col_1, len, [0.0; 3]);
        resize_column(&mut self.normal_col_2, len, [0.0; 3]);
        resize_column(&mut self.instance_types, len, InstanceType::ZERO);
        let index = prepared.slot as usize;
        self.mesh_slots[index] = mesh.slot();
        self.mesh_generations[index] = mesh.generation();
        self.instance_types[index] = instance_type;
        self.set_transform(prepared.slot, model, normal);
        self.slots.commit(prepared);
    }

    fn mesh_handle(&self, slot: u32) -> MeshHandle {
        let index = slot as usize;
        MeshHandle::from_parts(self.mesh_slots[index], self.mesh_generations[index])
    }

    fn set_transform(&mut self, slot: u32, model: ModelTransform, normal: NormalMatrix) {
        let index = slot as usize;
        self.model_col_0[index] = model[0];
        self.model_col_1[index] = model[1];
        self.model_col_2[index] = model[2];
        self.model_col_3[index] = model[3];
        self.normal_col_0[index] = normal[0];
        self.normal_col_1[index] = normal[1];
        self.normal_col_2[index] = normal[2];
    }

    fn view(&self, handle: InstanceHandle, is_default: bool) -> InstanceView {
        let index = handle.slot() as usize;
        InstanceView {
            handle,
            mesh: self.mesh_handle(handle.slot()),
            model: [
                self.model_col_0[index],
                self.model_col_1[index],
                self.model_col_2[index],
                self.model_col_3[index],
            ],
            normal: [
                self.normal_col_0[index],
                self.normal_col_1[index],
                self.normal_col_2[index],
            ],
            instance_type: self.instance_types[index],
            is_default,
        }
    }
}

fn resize_column<T: Clone>(column: &mut Vec<T>, len: usize, value: T) {
    if column.len() < len {
        column.resize(len, value);
    }
}

fn as_usize(value: u32) -> usize {
    usize::try_from(value).expect("u32 must fit in usize on supported targets")
}

fn validate_geometry(info: &MeshCreateInfo<'_>) -> Result<u32, RenderDataError> {
    if info.positions.is_empty() {
        return Err(RenderDataError::EmptyVertices);
    }
    if info.positions.len() != info.normals.len()
        || info.positions.len() != info.tangents.len()
        || info.positions.len() != info.uvs.len()
    {
        return Err(RenderDataError::MismatchedVertexStreams);
    }
    let vertex_count =
        u32::try_from(info.positions.len()).map_err(|_| RenderDataError::InputTooLarge)?;
    if info.indices.is_empty() {
        return Err(RenderDataError::EmptyIndices);
    }
    u32::try_from(info.indices.len()).map_err(|_| RenderDataError::InputTooLarge)?;
    if info
        .positions
        .iter()
        .flatten()
        .chain(info.normals.iter().flatten())
        .chain(info.tangents.iter().flatten())
        .chain(info.uvs.iter().flatten())
        .any(|value| !value.is_finite())
    {
        return Err(RenderDataError::NonFiniteGeometry);
    }
    if info.indices.iter().any(|index| *index >= vertex_count) {
        return Err(RenderDataError::IndexOutOfBounds);
    }
    Ok(vertex_count)
}

fn aabb(positions: &[[f32; 3]]) -> Aabb {
    let mut min = positions[0];
    let mut max = positions[0];
    for position in &positions[1..] {
        for axis in 0..3 {
            min[axis] = min[axis].min(position[axis]);
            max[axis] = max[axis].max(position[axis]);
        }
    }
    Aabb { min, max }
}

fn normal_matrix(model: ModelTransform) -> Result<NormalMatrix, RenderDataError> {
    validate_affine(model)?;
    let (a, b, c) = (model[0][0], model[0][1], model[0][2]);
    let (d, e, f) = (model[1][0], model[1][1], model[1][2]);
    let (g, h, i) = (model[2][0], model[2][1], model[2][2]);
    let determinant = a * (e * i - f * h) - d * (b * i - c * h) + g * (b * f - c * e);
    if determinant == 0.0 || !determinant.is_finite() {
        return Err(RenderDataError::InvalidTransform);
    }
    let normal = [
        [
            (e * i - f * h) / determinant,
            (f * g - d * i) / determinant,
            (d * h - e * g) / determinant,
        ],
        [
            (c * h - b * i) / determinant,
            (a * i - c * g) / determinant,
            (b * g - a * h) / determinant,
        ],
        [
            (b * f - c * e) / determinant,
            (c * d - a * f) / determinant,
            (a * e - b * d) / determinant,
        ],
    ];
    if normal.iter().flatten().any(|value| !value.is_finite()) {
        Err(RenderDataError::InvalidTransform)
    } else {
        Ok(normal)
    }
}

fn validate_affine(model: ModelTransform) -> Result<(), RenderDataError> {
    if model.iter().flatten().any(|value| !value.is_finite())
        || model[0][3] != 0.0
        || model[1][3] != 0.0
        || model[2][3] != 0.0
        || model[3][3] != 1.0
    {
        Err(RenderDataError::InvalidTransform)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
