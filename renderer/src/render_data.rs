use std::{f32::consts::PI, marker::PhantomData, ops::Range};

use ultraviolet::{Mat4, Vec3};

/// The fixed shared projection covers every canonical instance slot. The slot
/// table starts small and doubles up to this generous ceiling without ever
/// leaving a live instance invisible to JavaScript.
pub const MAX_INSTANCE_CAPACITY: usize = 1 << 12;
const MIN_AFFINE_DETERMINANT: f32 = 1.0e-8;
const MAX_AFFINE_CONDITION: f32 = 1.0e8;

/// Validates the single canonical transform representation accepted by the renderer.
pub fn canonical_affine_transform(transform: Mat4) -> Option<Mat4> {
    let mut transform = transform;
    {
        let m = transform.as_mut_array();
        let scale = [m[0], m[1], m[2], m[4], m[5], m[6], m[8], m[9], m[10]]
            .into_iter()
            .map(f32::abs)
            .fold(1.0, f32::max);
        let tolerance = 32.0 * f32::EPSILON * scale;
        if !m.iter().all(|value| value.is_finite())
            || m[3].abs() > tolerance
            || m[7].abs() > tolerance
            || m[11].abs() > tolerance
            || (m[15] - 1.0).abs() > tolerance
        {
            return None;
        }
        m[3] = 0.0;
        m[7] = 0.0;
        m[11] = 0.0;
        m[15] = 1.0;
    }
    let m = transform.as_array();
    let determinant = transform.determinant();
    if !determinant.is_finite()
        || determinant <= MIN_AFFINE_DETERMINANT * linear_scale_cubed(&transform)
    {
        return None;
    }
    let linear_max = [m[0], m[1], m[2], m[4], m[5], m[6], m[8], m[9], m[10]]
        .into_iter()
        .map(f32::abs)
        .fold(0.0, f32::max);
    let inverse = transform.inversed();
    let i = inverse.as_array();
    let inverse_max = [i[0], i[1], i[2], i[4], i[5], i[6], i[8], i[9], i[10]]
        .into_iter()
        .map(f32::abs)
        .fold(0.0, f32::max);
    (linear_max * inverse_max <= MAX_AFFINE_CONDITION).then_some(transform)
}

fn linear_scale_cubed(transform: &Mat4) -> f32 {
    let m = transform.as_array();
    [m[0], m[1], m[2], m[4], m[5], m[6], m[8], m[9], m[10]]
        .into_iter()
        .map(f32::abs)
        .fold(0.0, f32::max)
        .powi(3)
        .max(f32::MIN_POSITIVE)
}

/// Validates a canonical affine transform and every transformed local AABB corner.
pub fn canonical_transform_for_bounds(transform: Mat4, bounds: Aabb) -> Option<Mat4> {
    if !(0..3).all(|axis| {
        bounds.min[axis].is_finite()
            && bounds.max[axis].is_finite()
            && bounds.min[axis] <= bounds.max[axis]
    }) {
        return None;
    }
    let transform = canonical_affine_transform(transform)?;
    for x in [bounds.min[0], bounds.max[0]] {
        for y in [bounds.min[1], bounds.max[1]] {
            for z in [bounds.min[2], bounds.max[2]] {
                let point = transform.transform_point3(Vec3::new(x, y, z));
                if !point.as_array().iter().all(|value| value.is_finite()) {
                    return None;
                }
            }
        }
    }
    let transformed = bounds.transformed(transform);
    if !(0..3).all(|axis| {
        transformed.min[axis].is_finite()
            && transformed.max[axis].is_finite()
            && transformed.min[axis] <= transformed.max[axis]
    }) {
        return None;
    }
    Some(transform)
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct Handle<T> {
    pub slot: u32,
    pub generation: u32,
    kind: PhantomData<fn() -> T>,
}

impl<T> Copy for Handle<T> {}

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Handle<T> {
    fn new(slot: usize, generation: u32) -> Self {
        Self {
            slot: slot as u32,
            generation,
            kind: PhantomData,
        }
    }

    pub fn from_parts(slot: u32, generation: u32) -> Self {
        Self {
            slot,
            generation,
            kind: PhantomData,
        }
    }
}

#[derive(Clone, Debug)]
pub struct InstanceKind;
pub type GeometryHandle = Handle<GeometryRanges>;
pub type InstanceHandle = Handle<InstanceKind>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Aabb {
    pub fn transformed(self, transform: Mat4) -> Self {
        let center = Vec3::new(
            self.min[0] * 0.5 + self.max[0] * 0.5,
            self.min[1] * 0.5 + self.max[1] * 0.5,
            self.min[2] * 0.5 + self.max[2] * 0.5,
        );
        let extent = Vec3::new(
            (self.max[0] - self.min[0]) * 0.5,
            (self.max[1] - self.min[1]) * 0.5,
            (self.max[2] - self.min[2]) * 0.5,
        );
        let world_center = transform.transform_point3(center);
        let columns = transform.as_slice();
        let world_extent = Vec3::new(
            columns[0].abs() * extent.x + columns[4].abs() * extent.y + columns[8].abs() * extent.z,
            columns[1].abs() * extent.x + columns[5].abs() * extent.y + columns[9].abs() * extent.z,
            columns[2].abs() * extent.x
                + columns[6].abs() * extent.y
                + columns[10].abs() * extent.z,
        );
        Self {
            min: (world_center - world_extent).into(),
            max: (world_center + world_extent).into(),
        }
    }

    fn include(&mut self, other: Self) {
        for axis in 0..3 {
            self.min[axis] = self.min[axis].min(other.min[axis]);
            self.max[axis] = self.max[axis].max(other.max[axis]);
        }
    }
}

#[derive(Clone, Debug)]
pub struct Geometry {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    pub local_bounds: Aabb,
}

#[derive(Clone, Debug)]
pub struct GeometryRanges {
    vertices: Range<usize>,
    indices: Range<usize>,
    local_bounds: Aabb,
}

/// Borrowed view of immutable geometry stored in the canonical global arrays.
#[derive(Clone, Debug)]
pub struct GeometryRef<'a> {
    pub positions: &'a [[f32; 3]],
    pub normals: &'a [[f32; 3]],
    pub uvs: &'a [[f32; 2]],
    pub indices: &'a [u32],
    pub vertex_range: Range<usize>,
    pub index_range: Range<usize>,
    pub local_bounds: Aabb,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum BoundsState {
    Pending = 0,
    Valid = 1,
    Empty = 2,
    InvalidNonFinite = 3,
}
impl BoundsState {
    pub fn from_u32(value: u32) -> Self {
        match value {
            1 => Self::Valid,
            2 => Self::Empty,
            3 => Self::InvalidNonFinite,
            _ => Self::Pending,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoundsIdentity {
    pub slot: u32,
    pub generation: u32,
    pub content_version: u32,
    pub snapshot_id: u32,
}
#[derive(Clone, Copy, Debug)]
pub struct BoundsResult {
    pub identity: BoundsIdentity,
    pub job_id: u32,
    pub state: BoundsState,
    pub bounds: Aabb,
}
#[derive(Clone, Copy, Debug)]
struct BoundsMetadata {
    content_version: u32,
    snapshot_id: u32,
    job_id: u32,
    state: BoundsState,
    accepted: Option<BoundsIdentity>,
    accepted_bounds: Option<Aabb>,
}

impl Geometry {
    pub fn new(
        positions: Vec<[f32; 3]>,
        mut normals: Vec<[f32; 3]>,
        mut uvs: Vec<[f32; 2]>,
        indices: Vec<u32>,
    ) -> Self {
        normals.resize(positions.len(), [0.0, 1.0, 0.0]);
        uvs.resize(positions.len(), [0.0, 0.0]);
        let mut local_bounds = Aabb {
            min: [f32::INFINITY; 3],
            max: [f32::NEG_INFINITY; 3],
        };
        for point in &positions {
            local_bounds.include(Aabb {
                min: *point,
                max: *point,
            });
        }
        if positions.is_empty() {
            local_bounds = Aabb {
                min: [0.0; 3],
                max: [0.0; 3],
            };
        }
        Self {
            positions,
            normals,
            uvs,
            indices,
            local_bounds,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RenderDataConfig {
    pub initial_geometry_capacity: usize,
    pub initial_instance_capacity: usize,
    pub max_geometry_capacity: usize,
    pub max_instance_capacity: usize,
    pub initial_vertex_capacity: usize,
    pub initial_index_capacity: usize,
    pub max_vertex_capacity: usize,
    pub max_index_capacity: usize,
}

impl Default for RenderDataConfig {
    fn default() -> Self {
        Self {
            initial_geometry_capacity: 16,
            initial_instance_capacity: 64,
            max_geometry_capacity: 1 << 20,
            max_instance_capacity: MAX_INSTANCE_CAPACITY,
            initial_vertex_capacity: 1 << 16,
            initial_index_capacity: 1 << 17,
            max_vertex_capacity: 1 << 26,
            max_index_capacity: 1 << 27,
        }
    }
}

#[derive(Clone)]
struct Slots<T> {
    generations: Vec<u32>,
    values: Vec<Option<T>>,
    free: Vec<u32>,
    max: usize,
}

impl<T> Slots<T> {
    fn new(initial: usize, max: usize) -> Self {
        let capacity = initial.max(1).min(max);
        Self {
            generations: vec![1; capacity],
            values: (0..capacity).map(|_| None).collect(),
            free: (0..capacity as u32).rev().collect(),
            max,
        }
    }

    fn insert(&mut self, value: T) -> Option<Handle<T>> {
        if self.free.is_empty() {
            let old = self.values.len();
            let new = (old * 2).min(self.max);
            if new == old {
                return None;
            }
            self.generations.resize(new, 1);
            self.values.resize_with(new, || None);
            self.free.extend((old as u32..new as u32).rev());
        }
        let slot = self.free.pop()? as usize;
        self.values[slot] = Some(value);
        Some(Handle::new(slot, self.generations[slot]))
    }

    fn can_insert(&self) -> bool {
        !self.free.is_empty() || self.capacity() < self.max
    }

    fn ensure_capacity(&mut self, required: usize) -> bool {
        if required > self.max {
            return false;
        }
        let old = self.capacity();
        if required <= old {
            return true;
        }
        self.generations.resize(required, 1);
        self.values.resize_with(required, || None);
        self.free.extend((old as u32..required as u32).rev());
        true
    }

    fn get(&self, handle: Handle<T>) -> Option<&T> {
        let slot = handle.slot as usize;
        if self.generations.get(slot) != Some(&handle.generation) {
            return None;
        }
        self.values.get(slot)?.as_ref()
    }

    fn remove(&mut self, handle: Handle<T>) -> Option<T> {
        let slot = handle.slot as usize;
        if self.generations.get(slot) != Some(&handle.generation) {
            return None;
        }
        let value = self.values[slot].take()?;
        if self.generations[slot] != u32::MAX {
            self.generations[slot] += 1;
            self.free.push(handle.slot);
        }
        Some(value)
    }

    fn capacity(&self) -> usize {
        self.values.len()
    }
}

pub struct InstanceRef<'a> {
    pub geometry: GeometryHandle,
    pub transform: &'a Mat4,
    pub pipeline_key: u32,
    pub render_flags: u32,
}

#[derive(Clone)]
pub struct RenderData {
    geometry: Slots<GeometryRanges>,
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
    max_vertex_capacity: usize,
    max_index_capacity: usize,
    instances: Slots<InstanceKind>,
    instance_geometry: Vec<Option<GeometryHandle>>,
    transforms: Vec<Mat4>,
    pipeline_keys: Vec<u32>,
    render_flags: Vec<u32>,
    layer_masks: Vec<u32>,
    transform_versions: Vec<u32>,
    state_versions: Vec<u32>,
    bounds: Vec<BoundsMetadata>,
    next_content_version: u32,
    next_snapshot_id: u32,
}

impl RenderData {
    pub fn new(config: RenderDataConfig) -> Self {
        let geometry = Slots::new(
            config.initial_geometry_capacity,
            config.max_geometry_capacity,
        );
        let instances = Slots::new(
            config.initial_instance_capacity,
            config.max_instance_capacity,
        );
        let capacity = instances.capacity();
        let geometry_capacity = geometry.capacity();
        Self {
            geometry,
            positions: Vec::with_capacity(
                config
                    .initial_vertex_capacity
                    .min(config.max_vertex_capacity),
            ),
            normals: Vec::with_capacity(
                config
                    .initial_vertex_capacity
                    .min(config.max_vertex_capacity),
            ),
            uvs: Vec::with_capacity(
                config
                    .initial_vertex_capacity
                    .min(config.max_vertex_capacity),
            ),
            indices: Vec::with_capacity(
                config.initial_index_capacity.min(config.max_index_capacity),
            ),
            max_vertex_capacity: config.max_vertex_capacity,
            max_index_capacity: config.max_index_capacity,
            instances,
            instance_geometry: vec![None; capacity],
            transforms: vec![Mat4::identity(); capacity],
            pipeline_keys: vec![0; capacity],
            render_flags: vec![1; capacity],
            layer_masks: vec![u32::MAX; capacity],
            transform_versions: vec![1; capacity],
            state_versions: vec![1; capacity],
            bounds: vec![
                BoundsMetadata {
                    content_version: 0,
                    snapshot_id: 0,
                    job_id: 0,
                    state: BoundsState::Pending,
                    accepted: None,
                    accepted_bounds: None
                };
                geometry_capacity
            ],
            next_content_version: 1,
            next_snapshot_id: 1,
        }
    }

    fn grow_instance_soa(&mut self) {
        let capacity = self.instances.capacity();
        self.instance_geometry.resize(capacity, None);
        self.transforms.resize(capacity, Mat4::identity());
        self.pipeline_keys.resize(capacity, 0);
        self.render_flags.resize(capacity, 1);
        self.layer_masks.resize(capacity, u32::MAX);
        self.transform_versions.resize(capacity, 1);
        self.state_versions.resize(capacity, 1);
    }

    pub fn add_geometry_only(&mut self, geometry: Geometry) -> Option<GeometryHandle> {
        let next_content_version = self.next_content_version.checked_add(1)?;
        let next_snapshot_id = self.next_snapshot_id.checked_add(1)?;
        let vertex_end = self.positions.len().checked_add(geometry.positions.len())?;
        let index_end = self.indices.len().checked_add(geometry.indices.len())?;
        if vertex_end > self.max_vertex_capacity || index_end > self.max_index_capacity {
            return None;
        }
        // Check slot budget before changing any canonical array or its capacity.
        if self.geometry.free.is_empty() && self.geometry.capacity() >= self.geometry.max {
            return None;
        }
        let vertex_capacity = doubled_capacity(
            self.positions.capacity(),
            vertex_end,
            self.max_vertex_capacity,
        )?;
        let index_capacity =
            doubled_capacity(self.indices.capacity(), index_end, self.max_index_capacity)?;
        if self.positions.capacity() < vertex_capacity {
            self.positions
                .reserve_exact(vertex_capacity - self.positions.len());
            self.normals
                .reserve_exact(vertex_capacity - self.normals.len());
            self.uvs.reserve_exact(vertex_capacity - self.uvs.len());
        }
        if self.indices.capacity() < index_capacity {
            self.indices
                .reserve_exact(index_capacity - self.indices.len());
        }

        let ranges = GeometryRanges {
            vertices: self.positions.len()..vertex_end,
            indices: self.indices.len()..index_end,
            local_bounds: geometry.local_bounds,
        };
        let handle = self.geometry.insert(ranges)?;
        self.positions.extend(geometry.positions);
        self.normals.extend(geometry.normals);
        self.uvs.extend(geometry.uvs);
        self.indices.extend(geometry.indices);
        self.bounds.resize(
            self.geometry.capacity(),
            BoundsMetadata {
                content_version: 0,
                snapshot_id: 0,
                job_id: 0,
                state: BoundsState::Pending,
                accepted: None,
                accepted_bounds: None,
            },
        );
        let metadata = &mut self.bounds[handle.slot as usize];
        metadata.content_version = self.next_content_version;
        metadata.snapshot_id = self.next_snapshot_id;
        self.next_content_version = next_content_version;
        self.next_snapshot_id = next_snapshot_id;
        Some(handle)
    }

    /// Inserts immutable geometry and its required default instance.
    pub fn add_geometry(&mut self, geometry: Geometry) -> Option<(GeometryHandle, InstanceHandle)> {
        // The default instance is part of this operation. Do not append geometry
        // when its instance slot cannot be committed.
        if !self.instances.can_insert() {
            return None;
        }
        let geometry = self.add_geometry_only(geometry)?;
        let instance = self.add_instance(geometry, Mat4::identity(), 0)?;
        Some((geometry, instance))
    }

    pub fn add_instance(
        &mut self,
        geometry: GeometryHandle,
        transform: Mat4,
        pipeline_key: u32,
    ) -> Option<InstanceHandle> {
        let local_bounds = self.geometry.get(geometry)?.local_bounds;
        let transform = canonical_transform_for_bounds(transform, local_bounds)?;
        let handle = self.instances.insert(InstanceKind)?;
        self.grow_instance_soa();
        let slot = handle.slot as usize;
        self.instance_geometry[slot] = Some(geometry);
        self.transforms[slot] = transform;
        self.pipeline_keys[slot] = pipeline_key;
        self.render_flags[slot] = 1 | crate::spatial::SELECTABLE;
        self.layer_masks[slot] = u32::MAX;
        self.transform_versions[slot] = 1;
        self.state_versions[slot] = 1;
        Some(handle)
    }

    pub fn geometry(&self, handle: GeometryHandle) -> Option<GeometryRef<'_>> {
        self.geometry
            .get(handle)
            .map(|ranges| self.geometry_ref(ranges))
    }

    pub fn geometries(&self) -> impl Iterator<Item = (u32, GeometryRef<'_>)> {
        self.geometry
            .values
            .iter()
            .enumerate()
            .filter_map(|(slot, value)| {
                value
                    .as_ref()
                    .map(|value| (slot as u32, self.geometry_ref(value)))
            })
    }

    fn geometry_ref<'a>(&'a self, ranges: &GeometryRanges) -> GeometryRef<'a> {
        GeometryRef {
            positions: &self.positions[ranges.vertices.clone()],
            normals: &self.normals[ranges.vertices.clone()],
            uvs: &self.uvs[ranges.vertices.clone()],
            indices: &self.indices[ranges.indices.clone()],
            vertex_range: ranges.vertices.clone(),
            index_range: ranges.indices.clone(),
            local_bounds: ranges.local_bounds,
        }
    }

    pub fn positions(&self) -> &[[f32; 3]] {
        &self.positions
    }
    pub fn normals(&self) -> &[[f32; 3]] {
        &self.normals
    }
    pub fn uvs(&self) -> &[[f32; 2]] {
        &self.uvs
    }
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    pub fn instances(&self) -> impl Iterator<Item = InstanceRef<'_>> {
        self.instances
            .values
            .iter()
            .enumerate()
            .filter_map(|(slot, occupied)| {
                occupied.as_ref()?;
                Some(InstanceRef {
                    geometry: self.instance_geometry[slot]?,
                    transform: &self.transforms[slot],
                    pipeline_key: self.pipeline_keys[slot],
                    render_flags: self.render_flags[slot],
                })
            })
    }

    pub fn instances_with_handles(
        &self,
    ) -> impl Iterator<Item = (InstanceHandle, InstanceRef<'_>)> {
        self.instances
            .values
            .iter()
            .enumerate()
            .filter_map(|(slot, occupied)| {
                occupied.as_ref()?;
                Some((
                    Handle::new(slot, self.instances.generations[slot]),
                    InstanceRef {
                        geometry: self.instance_geometry[slot]?,
                        transform: &self.transforms[slot],
                        pipeline_key: self.pipeline_keys[slot],
                        render_flags: self.render_flags[slot],
                    },
                ))
            })
    }

    pub fn set_transform(&mut self, handle: InstanceHandle, transform: Mat4) -> bool {
        if self.instances.get(handle).is_none() {
            return false;
        }
        let Some(geometry) = self.instance_geometry[handle.slot as usize] else {
            return false;
        };
        let Some(ranges) = self.geometry.get(geometry) else {
            return false;
        };
        let Some(transform) = canonical_transform_for_bounds(transform, ranges.local_bounds) else {
            return false;
        };
        self.transforms[handle.slot as usize] = transform;
        self.transform_versions[handle.slot as usize] = self.transform_versions
            [handle.slot as usize]
            .wrapping_add(1)
            .max(1);
        true
    }

    pub fn set_visible(&mut self, handle: InstanceHandle, visible: bool) -> bool {
        if self.instances.get(handle).is_none() {
            return false;
        }
        let flags = &mut self.render_flags[handle.slot as usize];
        *flags = (*flags & !1) | u32::from(visible);
        self.state_versions[handle.slot as usize] = self.state_versions[handle.slot as usize]
            .wrapping_add(1)
            .max(1);
        true
    }

    pub fn set_pipeline(&mut self, handle: InstanceHandle, pipeline: u32) -> bool {
        if self.instances.get(handle).is_none() {
            return false;
        }
        self.pipeline_keys[handle.slot as usize] = pipeline;
        true
    }

    pub fn clone_instance(&mut self, handle: InstanceHandle) -> Option<InstanceHandle> {
        let geometry = self.instance_geometry(handle)?;
        let slot = handle.slot as usize;
        self.add_instance(geometry, self.transforms[slot], self.pipeline_keys[slot])
    }

    pub fn instance_geometry(&self, handle: InstanceHandle) -> Option<GeometryHandle> {
        self.instances.get(handle)?;
        self.instance_geometry[handle.slot as usize]
    }

    pub fn remove_instance(&mut self, handle: InstanceHandle) -> bool {
        if self.instances.remove(handle).is_none() {
            return false;
        }
        self.instance_geometry[handle.slot as usize] = None;
        true
    }

    pub fn instance_capacity(&self) -> usize {
        self.instances.capacity()
    }

    pub fn geometry_capacity(&self) -> usize {
        self.geometry.capacity()
    }

    pub fn pending_bounds_jobs(&self) -> impl Iterator<Item = (BoundsIdentity, &[[f32; 3]])> {
        self.geometries().filter_map(|(slot, geometry)| {
            let meta = self.bounds[slot as usize];
            (meta.state == BoundsState::Pending && meta.job_id == 0).then_some((
                BoundsIdentity {
                    slot,
                    generation: self.geometry.generations[slot as usize],
                    content_version: meta.content_version,
                    snapshot_id: meta.snapshot_id,
                },
                geometry.positions,
            ))
        })
    }
    pub fn mark_bounds_dispatched(&mut self, identity: BoundsIdentity, job_id: u32) {
        if self.current_bounds_identity(identity.slot) == Some(identity) {
            self.bounds[identity.slot as usize].job_id = job_id;
        }
    }
    fn current_bounds_identity(&self, slot: u32) -> Option<BoundsIdentity> {
        let meta = *self.bounds.get(slot as usize)?;
        self.geometry.values.get(slot as usize)?.as_ref()?;
        Some(BoundsIdentity {
            slot,
            generation: self.geometry.generations[slot as usize],
            content_version: meta.content_version,
            snapshot_id: meta.snapshot_id,
        })
    }
    pub fn accept_bounds(&mut self, result: BoundsResult) -> bool {
        if self.current_bounds_identity(result.identity.slot) != Some(result.identity)
            || self.bounds[result.identity.slot as usize].job_id != result.job_id
        {
            return false;
        }
        let meta = &mut self.bounds[result.identity.slot as usize];
        if meta.accepted == Some(result.identity) {
            return false;
        }
        meta.state = result.state;
        meta.accepted = Some(result.identity);
        meta.accepted_bounds = (result.state == BoundsState::Valid).then_some(result.bounds);
        true
    }
    pub fn bounds_state(&self, geometry: GeometryHandle) -> Option<BoundsState> {
        self.geometry.get(geometry)?;
        Some(self.bounds[geometry.slot as usize].state)
    }
    pub fn accepted_bounds(&self, slot: u32) -> Option<(BoundsState, Option<Aabb>)> {
        self.geometry.values.get(slot as usize)?.as_ref()?;
        let metadata = self.bounds.get(slot as usize)?;
        Some((metadata.state, metadata.accepted_bounds))
    }

    pub fn accepted_bounds_identity(
        &self,
        geometry: GeometryHandle,
    ) -> Option<(BoundsIdentity, Aabb)> {
        self.geometry.get(geometry)?;
        let metadata = self.bounds.get(geometry.slot as usize)?;
        Some((metadata.accepted?, metadata.accepted_bounds?))
    }
    pub fn transform_version(&self, handle: InstanceHandle) -> Option<u32> {
        self.instances.get(handle)?;
        Some(self.transform_versions[handle.slot as usize])
    }
    pub fn state_version(&self, handle: InstanceHandle) -> Option<u32> {
        self.instances.get(handle)?;
        Some(self.state_versions[handle.slot as usize])
    }
    pub fn layer_mask(&self, handle: InstanceHandle) -> Option<u32> {
        self.instances.get(handle)?;
        Some(self.layer_masks[handle.slot as usize])
    }

    pub fn world_bounds(&self) -> Option<Aabb> {
        let mut bounds: Option<Aabb> = None;
        for instance in self.instances() {
            // Provisional CPU bounds are camera-framing-only. Accepted worker
            // bounds will be authoritative for render culling in Phase 6.
            let transformed = self
                .geometry(instance.geometry)?
                .local_bounds
                .transformed(*instance.transform);
            if let Some(bounds) = &mut bounds {
                bounds.include(transformed);
            } else {
                bounds = Some(transformed);
            }
        }
        bounds
    }

    /// Makes handles in a replacement canonical distinct from every handle in `self`.
    pub fn preserve_generations_for_replacement(&self, replacement: &mut Self) -> bool {
        let geometry_capacity = self
            .geometry
            .capacity()
            .max(replacement.geometry.capacity());
        let instance_capacity = self
            .instances
            .capacity()
            .max(replacement.instances.capacity());
        if !replacement.geometry.ensure_capacity(geometry_capacity)
            || !replacement.instances.ensure_capacity(instance_capacity)
        {
            return false;
        }
        replacement.bounds.resize(
            geometry_capacity,
            BoundsMetadata {
                content_version: 0,
                snapshot_id: 0,
                job_id: 0,
                state: BoundsState::Pending,
                accepted: None,
                accepted_bounds: None,
            },
        );
        replacement.grow_instance_soa();
        for slot in 0..geometry_capacity {
            let previous = self
                .geometry
                .generations
                .get(slot)
                .copied()
                .map_or(0, |value| value);
            let Some(generation) = previous.checked_add(1) else {
                return false;
            };
            replacement.geometry.generations[slot] = generation.max(1);
        }
        for geometry in replacement.instance_geometry.iter_mut().flatten() {
            geometry.generation = replacement.geometry.generations[geometry.slot as usize];
        }
        for slot in 0..instance_capacity {
            let previous = self
                .instances
                .generations
                .get(slot)
                .copied()
                .map_or(0, |value| value);
            let Some(generation) = previous.checked_add(1) else {
                return false;
            };
            replacement.instances.generations[slot] = generation.max(1);
        }
        true
    }
}

fn doubled_capacity(current: usize, required: usize, max: usize) -> Option<usize> {
    if required > max {
        return None;
    }
    let mut capacity = current.max(1);
    while capacity < required {
        capacity = capacity.checked_mul(2).map_or(max, |value| value).min(max);
        if capacity < required && capacity == max {
            return None;
        }
    }
    Some(capacity)
}

pub fn cube_geometry() -> Geometry {
    let mut positions = Vec::with_capacity(24);
    let mut normals = Vec::with_capacity(24);
    let mut uvs = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    let faces = [
        (
            [1.0, 0.0, 0.0],
            [
                [0.5, -0.5, -0.5],
                [0.5, 0.5, -0.5],
                [0.5, 0.5, 0.5],
                [0.5, -0.5, 0.5],
            ],
        ),
        (
            [-1.0, 0.0, 0.0],
            [
                [-0.5, -0.5, 0.5],
                [-0.5, 0.5, 0.5],
                [-0.5, 0.5, -0.5],
                [-0.5, -0.5, -0.5],
            ],
        ),
        (
            [0.0, 1.0, 0.0],
            [
                [-0.5, 0.5, -0.5],
                [-0.5, 0.5, 0.5],
                [0.5, 0.5, 0.5],
                [0.5, 0.5, -0.5],
            ],
        ),
        (
            [0.0, -1.0, 0.0],
            [
                [-0.5, -0.5, 0.5],
                [-0.5, -0.5, -0.5],
                [0.5, -0.5, -0.5],
                [0.5, -0.5, 0.5],
            ],
        ),
        (
            [0.0, 0.0, 1.0],
            [
                [0.5, -0.5, 0.5],
                [0.5, 0.5, 0.5],
                [-0.5, 0.5, 0.5],
                [-0.5, -0.5, 0.5],
            ],
        ),
        (
            [0.0, 0.0, -1.0],
            [
                [-0.5, -0.5, -0.5],
                [-0.5, 0.5, -0.5],
                [0.5, 0.5, -0.5],
                [0.5, -0.5, -0.5],
            ],
        ),
    ];
    for (normal, face) in faces {
        let start = positions.len() as u32;
        positions.extend(face);
        normals.extend([normal; 4]);
        uvs.extend([[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]]);
        indices.extend([start, start + 1, start + 2, start, start + 2, start + 3]);
    }
    Geometry::new(positions, normals, uvs, indices)
}

pub fn uv_sphere_geometry(segments: u32, rings: u32) -> Geometry {
    let segments = segments.max(3);
    let rings = rings.max(2);
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    for ring in 0..=rings {
        let v = ring as f32 / rings as f32;
        let phi = v * PI;
        for segment in 0..=segments {
            let u = segment as f32 / segments as f32;
            let theta = u * 2.0 * PI;
            let normal = [phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin()];
            positions.push([normal[0] * 0.5, normal[1] * 0.5, normal[2] * 0.5]);
            normals.push(normal);
            uvs.push([u, v]);
        }
    }
    let mut indices = Vec::new();
    let stride = segments + 1;
    for ring in 0..rings {
        for segment in 0..segments {
            let a = ring * stride + segment;
            let b = a + stride;
            indices.extend([a, a + 1, b, a + 1, b + 1, b]);
        }
    }
    Geometry::new(positions, normals, uvs, indices)
}

/// Builds a 12x10 gallery with 120 instances sharing exactly two geometries.
pub fn procedural_scene() -> RenderData {
    let mut data = RenderData::new(RenderDataConfig::default());
    let Some(cube) = data.add_geometry_only(cube_geometry()) else {
        log::error!("failed to create procedural cube geometry");
        return data;
    };
    let Some(sphere) = data.add_geometry_only(uv_sphere_geometry(24, 16)) else {
        log::error!("failed to create procedural sphere geometry");
        return data;
    };
    for z in 0..10 {
        for x in 0..12 {
            let geometry = if (x + z) % 2 == 0 { cube } else { sphere };
            let translation = Vec3::new((x as f32 - 5.5) * 2.0, 0.0, (z as f32 - 4.5) * 2.0);
            let scale = 0.7 + ((x * 7 + z * 3) % 5) as f32 * 0.1;
            let transform = Mat4::from_translation(translation) * Mat4::from_scale(scale);
            if data.add_instance(geometry, transform, 0).is_none() {
                log::error!("procedural scene instance capacity exhausted at ({x}, {z})");
                return data;
            }
        }
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    fn some<T>(value: Option<T>) -> T {
        match value {
            Some(value) => value,
            None => panic!("test expected Some"),
        }
    }

    fn triangle() -> Geometry {
        Geometry::new(
            vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
            vec![],
            vec![],
            vec![0, 1, 2],
        )
    }

    fn geometry_budget(vertex_max: usize, index_max: usize) -> RenderDataConfig {
        RenderDataConfig {
            initial_vertex_capacity: 2,
            initial_index_capacity: 2,
            max_vertex_capacity: vertex_max,
            max_index_capacity: index_max,
            ..Default::default()
        }
    }

    #[test]
    fn geometry_arrays_double_and_keep_earlier_ranges_stable() {
        let mut data = RenderData::new(geometry_budget(16, 16));
        let first = some(data.add_geometry_only(triangle()));
        assert_eq!(data.positions.capacity(), 4);
        let before = some(data.geometry(first));
        assert_eq!(before.vertex_range, 0..3);
        assert_eq!(
            before.positions,
            &[[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]]
        );

        let second = some(data.add_geometry_only(triangle()));
        assert_eq!(data.positions.capacity(), 8);
        assert_eq!(data.indices.capacity(), 8);
        assert_eq!(some(data.geometry(first)).vertex_range, 0..3);
        assert_eq!(some(data.geometry(second)).vertex_range, 3..6);
        assert_eq!(some(data.geometry(first)).indices, &[0, 1, 2]);
    }

    #[test]
    fn geometry_budget_failure_is_transactional() {
        let mut data = RenderData::new(geometry_budget(3, 3));
        let first = some(data.add_geometry_only(triangle()));
        let capacities = (data.positions.capacity(), data.indices.capacity());
        assert!(data.add_geometry_only(triangle()).is_none());
        assert_eq!(data.geometries().count(), 1);
        assert_eq!(
            (data.positions.capacity(), data.indices.capacity()),
            capacities
        );
        assert_eq!(data.positions.len(), 3);
        assert_eq!(data.indices.len(), 3);
        assert_eq!(some(data.geometry(first)).vertex_range, 0..3);
    }

    #[test]
    fn capacities_double() {
        let mut data = RenderData::new(RenderDataConfig {
            initial_geometry_capacity: 1,
            initial_instance_capacity: 1,
            max_geometry_capacity: 8,
            max_instance_capacity: 8,
            ..Default::default()
        });
        some(data.add_geometry(triangle()));
        some(data.add_geometry(triangle()));
        assert_eq!(data.geometry.capacity(), 2);
        assert!(data.instance_capacity() >= 2);
    }

    #[test]
    fn stale_handles_are_rejected() {
        let mut data = RenderData::new(Default::default());
        let (_, handle) = some(data.add_geometry(triangle()));
        assert!(data.remove_instance(handle));
        assert!(!data.remove_instance(handle));
    }

    #[test]
    fn scene_replacement_rejects_retained_handle() {
        let mut old = RenderData::new(Default::default());
        let (_, retained) = some(old.add_geometry(triangle()));
        let mut replacement = RenderData::new(Default::default());
        some(replacement.add_geometry(triangle()));
        assert!(old.preserve_generations_for_replacement(&mut replacement));
        assert!(!replacement.remove_instance(retained));
    }

    #[test]
    fn replacement_grows_through_old_retained_slot_without_revival() {
        let config = RenderDataConfig {
            initial_geometry_capacity: 1,
            initial_instance_capacity: 1,
            max_geometry_capacity: 8,
            max_instance_capacity: 8,
            ..Default::default()
        };
        let mut old = RenderData::new(config);
        let (_, first) = some(old.add_geometry(triangle()));
        let retained = (0..4).fold(first, |handle, _| some(old.clone_instance(handle)));
        assert_eq!(retained.slot, 4);
        assert_eq!(old.instance_capacity(), 8);

        let mut replacement = RenderData::new(RenderDataConfig {
            initial_geometry_capacity: 2,
            initial_instance_capacity: 2,
            max_geometry_capacity: 8,
            max_instance_capacity: 8,
            ..Default::default()
        });
        some(replacement.add_geometry(triangle()));
        assert!(old.preserve_generations_for_replacement(&mut replacement));
        assert_eq!(replacement.instance_capacity(), 8);
        assert!(!replacement.remove_instance(retained));
        for _ in 0..4 {
            some(replacement.clone_instance(InstanceHandle::from_parts(0, 2)));
        }
        assert!(!replacement.remove_instance(retained));
    }

    #[test]
    fn add_geometry_is_transactional_when_default_instance_is_exhausted() {
        let mut data = RenderData::new(RenderDataConfig {
            initial_geometry_capacity: 2,
            max_geometry_capacity: 2,
            initial_instance_capacity: 1,
            max_instance_capacity: 1,
            ..Default::default()
        });
        some(data.add_geometry(triangle()));
        let lengths = (data.positions.len(), data.indices.len());
        assert!(data.add_geometry(triangle()).is_none());
        assert_eq!(data.geometries().count(), 1);
        assert_eq!((data.positions.len(), data.indices.len()), lengths);
    }

    #[test]
    fn uv_sphere_triangles_face_outward() {
        let sphere = uv_sphere_geometry(8, 6);
        for triangle in sphere.indices.chunks_exact(3) {
            let [a, b, c] = triangle else {
                panic!("chunks_exact returned a non-triangle")
            };
            let a = Vec3::from(sphere.positions[*a as usize]);
            let b = Vec3::from(sphere.positions[*b as usize]);
            let c = Vec3::from(sphere.positions[*c as usize]);
            let normal = (b - a).cross(c - a);
            assert!(normal.dot(a + b + c) >= -1.0e-6);
        }
    }

    #[test]
    fn procedural_scene_shares_two_geometries() {
        let data = procedural_scene();
        assert_eq!(data.geometries().count(), 2);
        assert_eq!(data.instances().count(), 120);
    }

    #[test]
    fn transformed_bounds_include_translation_and_scale() {
        let bounds = Aabb {
            min: [-1.0; 3],
            max: [1.0; 3],
        }
        .transformed(Mat4::from_translation(Vec3::new(5.0, 0.0, 0.0)) * Mat4::from_scale(2.0));
        assert_eq!(bounds.min, [3.0, -2.0, -2.0]);
        assert_eq!(bounds.max, [7.0, 2.0, 2.0]);
    }

    #[test]
    fn bounds_acceptance_requires_exact_identity_and_job() {
        let mut data = RenderData::new(Default::default());
        let geometry = some(data.add_geometry_only(triangle()));
        let (identity, _) = some(data.pending_bounds_jobs().next());
        data.mark_bounds_dispatched(identity, 7);
        let valid = BoundsResult {
            identity,
            job_id: 7,
            state: BoundsState::Valid,
            bounds: Aabb {
                min: [0.0; 3],
                max: [1.0; 3],
            },
        };
        let mut stale = valid;
        stale.identity.content_version += 1;
        assert!(!data.accept_bounds(stale));
        assert_eq!(data.bounds_state(geometry), Some(BoundsState::Pending));
        assert!(data.accept_bounds(valid));
        assert_eq!(data.bounds_state(geometry), Some(BoundsState::Valid));
    }

    #[test]
    fn empty_and_nonfinite_results_are_accepted_without_aabb() {
        for state in [BoundsState::Empty, BoundsState::InvalidNonFinite] {
            let mut data = RenderData::new(Default::default());
            let geometry = some(data.add_geometry_only(triangle()));
            let (identity, _) = some(data.pending_bounds_jobs().next());
            data.mark_bounds_dispatched(identity, 2);
            assert!(data.accept_bounds(BoundsResult {
                identity,
                job_id: 2,
                state,
                bounds: Aabb {
                    min: [0.0; 3],
                    max: [0.0; 3]
                }
            }));
            assert_eq!(data.bounds_state(geometry), Some(state));
            assert_eq!(data.accepted_bounds(geometry.slot), Some((state, None)));
        }
    }
}
