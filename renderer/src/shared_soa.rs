//! Extensible, SIMD-aligned SOA columns in shared WebAssembly memory.

use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU32, Ordering},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::render_data::{
    camera::Camera,
    upload::{Material, MaterialState},
    InstanceHandle, MaterialKey, RenderData, RenderDataCapacities,
};

pub const MAGIC: u32 = u32::from_le_bytes(*b"YSOA");
pub const VERSION: u32 = 1;
pub const HEADER_WORDS: usize = 16;
pub const DATA_OFFSET: u32 = 64;

#[repr(C, align(64))]
pub struct SharedBlock([AtomicU32; 16]);

impl SharedBlock {
    fn zeroed() -> Self {
        Self(std::array::from_fn(|_| AtomicU32::new(0)))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScalarType {
    U32,
    I32,
    F32,
}

impl ScalarType {
    fn tag(self) -> u32 {
        match self {
            Self::U32 => 1,
            Self::I32 => 2,
            Self::F32 => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArrayDomain {
    Mesh,
    Instance,
    Fixed,
}

impl ArrayDomain {
    fn tag(self) -> u32 {
        match self {
            Self::Mesh => 1,
            Self::Instance => 2,
            Self::Fixed => 3,
        }
    }

    fn capacity(self, capacities: RenderDataCapacities, fixed: Option<u32>) -> Option<u32> {
        match self {
            Self::Mesh => Some(capacities.meshes),
            Self::Instance => Some(capacities.instances),
            Self::Fixed => fixed,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ArrayRequest {
    pub name: String,
    pub domain: ArrayDomain,
    pub scalar: ScalarType,
    pub lanes: u32,
    pub stride: Option<u32>,
    pub length: Option<u32>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ArrayDescriptor {
    pub id: u32,
    pub name: String,
    pub domain: ArrayDomain,
    pub scalar: ScalarType,
    pub lanes: u32,
    pub stride: u32,
    pub length: u32,
    pub capacity: u32,
    pub control_ptr: u32,
    pub data_offset: u32,
    pub byte_length: u32,
    pub layout_epoch: u32,
    pub writable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_guard: Option<ArrayDomain>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SharedSoaError {
    #[error("shared SOA request is not valid JSON: {0}")]
    InvalidJson(String),
    #[error("shared SOA array name is invalid")]
    InvalidName,
    #[error("shared SOA lanes must be in 1..=64")]
    InvalidLanes,
    #[error("shared SOA stride must fit all lanes and be a multiple of 16 bytes")]
    InvalidStride,
    #[error("fixed shared SOA arrays require a nonzero length")]
    InvalidLength,
    #[error("shared SOA array already exists with a different layout")]
    LayoutConflict,
    #[error("shared SOA allocation size overflow")]
    SizeOverflow,
    #[error("shared SOA allocation failed")]
    AllocationFailed,
    #[error("shared SOA array is unknown")]
    UnknownArray,
    #[error("shared SOA byte uploads require a packed fixed array")]
    NotPackedFixed,
    #[error("shared SOA byte range exceeds the array length")]
    ByteRange,
    #[error("shared SOA array is currently being written")]
    Busy,
}

struct SharedArray {
    id: u32,
    request: ArrayRequest,
    stride_words: u32,
    length: u32,
    capacity: u32,
    layout_epoch: u32,
    storage: Box<[SharedBlock]>,
    retired: Vec<Box<[SharedBlock]>>,
    consumed_sequence: u32,
    consumed_slot_sequences: Vec<u32>,
    generation_guard: Option<ArrayDomain>,
    writable: bool,
}

fn generation_guard(name: &str) -> Option<ArrayDomain> {
    match name {
        "instance.transform" | "instance.type" => Some(ArrayDomain::Instance),
        _ => None,
    }
}

fn writable(name: &str) -> bool {
    !matches!(name, "instance.generation" | "mesh.generation")
}

impl SharedArray {
    fn new(
        id: u32,
        request: ArrayRequest,
        stride_words: u32,
        capacity: u32,
    ) -> Result<Self, SharedSoaError> {
        let generation_guard = generation_guard(&request.name);
        let writable = writable(&request.name);
        let mut array = Self {
            id,
            request,
            stride_words,
            length: capacity,
            capacity,
            layout_epoch: 1,
            storage: allocate(capacity, stride_words)?,
            retired: Vec::new(),
            consumed_sequence: 0,
            consumed_slot_sequences: vec![0; capacity as usize],
            generation_guard,
            writable,
        };
        array.initialize_header();
        Ok(array)
    }

    fn initialize_header(&mut self) {
        for (index, value) in [
            MAGIC,
            VERSION,
            self.id,
            self.request.scalar.tag(),
            self.request.lanes,
            self.stride_words,
            self.length,
            self.capacity,
            self.request.domain.tag(),
            0,
            self.layout_epoch,
            0,
            0,
            0,
            0,
            0,
        ]
        .into_iter()
        .enumerate()
        {
            self.word(index).store(value, Ordering::Relaxed);
        }
        self.consumed_sequence = 0;
    }

    fn word(&self, index: usize) -> &AtomicU32 {
        let block = index / 16;
        let lane = index % 16;
        &self.storage[block].0[lane]
    }

    fn data_word(&self, slot: u32, lane: u32) -> &AtomicU32 {
        let index = HEADER_WORDS + (slot * self.stride_words + lane) as usize;
        self.word(index)
    }

    fn descriptor(&self) -> Result<ArrayDescriptor, SharedSoaError> {
        let control_ptr = pointer(&self.storage)?;
        let byte_length = self
            .capacity
            .checked_mul(self.stride_words)
            .and_then(|words| words.checked_mul(4))
            .ok_or(SharedSoaError::SizeOverflow)?;
        Ok(ArrayDescriptor {
            id: self.id,
            name: self.request.name.clone(),
            domain: self.request.domain,
            scalar: self.request.scalar,
            lanes: self.request.lanes,
            stride: self.stride_words * 4,
            length: self.length,
            capacity: self.capacity,
            control_ptr,
            data_offset: DATA_OFFSET,
            byte_length,
            layout_epoch: self.layout_epoch,
            writable: self.writable,
            generation_guard: self.generation_guard,
        })
    }

    fn resize(&mut self, capacity: u32) -> Result<bool, SharedSoaError> {
        if capacity <= self.capacity {
            self.length = capacity;
            self.word(6).store(capacity, Ordering::Release);
            return Ok(false);
        }
        let replacement = allocate(capacity, self.stride_words)?;
        let old_words = HEADER_WORDS
            + self
                .capacity
                .checked_mul(self.stride_words)
                .ok_or(SharedSoaError::SizeOverflow)? as usize;
        for index in HEADER_WORDS..old_words {
            let block = index / 16;
            let lane = index % 16;
            replacement[block].0[lane]
                .store(self.word(index).load(Ordering::Acquire), Ordering::Relaxed);
        }
        let old = std::mem::replace(&mut self.storage, replacement);
        self.retired.push(old);
        self.capacity = capacity;
        self.length = capacity;
        self.consumed_slot_sequences.resize(capacity as usize, 0);
        self.layout_epoch = self
            .layout_epoch
            .checked_add(1)
            .ok_or(SharedSoaError::SizeOverflow)?;
        self.initialize_header();
        self.word(10).store(self.layout_epoch, Ordering::Relaxed);
        Ok(true)
    }

    fn try_lock(&self) -> Option<u32> {
        let sequence = self.word(9).load(Ordering::Acquire);
        if sequence & 1 != 0 {
            return None;
        }
        self.word(9)
            .compare_exchange(
                sequence,
                sequence.wrapping_add(1),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .ok()
            .map(|_| sequence)
    }

    fn unlock(&mut self, sequence: u32) {
        let next = sequence.wrapping_add(2) & !1;
        self.consumed_sequence = next;
        self.word(9).store(next, Ordering::Release);
    }

    fn changed(&self) -> bool {
        let sequence = self.word(9).load(Ordering::Acquire);
        sequence & 1 == 0 && sequence != self.consumed_sequence
    }
}

fn allocate(capacity: u32, stride_words: u32) -> Result<Box<[SharedBlock]>, SharedSoaError> {
    let words = capacity
        .checked_mul(stride_words)
        .and_then(|value| value.checked_add(HEADER_WORDS as u32))
        .ok_or(SharedSoaError::SizeOverflow)?;
    let blocks = words.checked_add(15).ok_or(SharedSoaError::SizeOverflow)? / 16;
    let blocks = usize::try_from(blocks).map_err(|_| SharedSoaError::SizeOverflow)?;
    let mut storage = Vec::new();
    storage
        .try_reserve_exact(blocks)
        .map_err(|_| SharedSoaError::AllocationFailed)?;
    storage.resize_with(blocks, SharedBlock::zeroed);
    Ok(storage.into_boxed_slice())
}

#[cfg(target_arch = "wasm32")]
fn pointer(storage: &[SharedBlock]) -> Result<u32, SharedSoaError> {
    u32::try_from(storage.as_ptr() as usize).map_err(|_| SharedSoaError::SizeOverflow)
}

#[cfg(not(target_arch = "wasm32"))]
fn pointer(_storage: &[SharedBlock]) -> Result<u32, SharedSoaError> {
    Ok(0)
}

fn valid_name(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic())
        && chars.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '-')
        })
        && value.len() <= 64
}

/// Owns stable shared columns. Replaced allocations are retained so stale external
/// views can never alias newly allocated Rust objects.
pub struct SharedSoaRegistry {
    arrays: BTreeMap<String, SharedArray>,
    next_id: u32,
    layout_changed: bool,
    material_keys: Vec<MaterialKey>,
    published_instance_generations: BTreeMap<u32, u32>,
    published_mesh_generations: BTreeMap<u32, u32>,
}

impl SharedSoaRegistry {
    pub fn new(capacities: RenderDataCapacities) -> Result<Self, SharedSoaError> {
        let mut registry = Self {
            arrays: BTreeMap::new(),
            next_id: 1,
            layout_changed: false,
            material_keys: Vec::new(),
            published_instance_generations: BTreeMap::new(),
            published_mesh_generations: BTreeMap::new(),
        };
        for request in [
            ArrayRequest {
                name: "instance.transform".into(),
                domain: ArrayDomain::Instance,
                scalar: ScalarType::F32,
                lanes: 16,
                stride: Some(80),
                length: None,
            },
            ArrayRequest {
                name: "instance.type".into(),
                domain: ArrayDomain::Instance,
                scalar: ScalarType::U32,
                lanes: 16,
                stride: Some(80),
                length: None,
            },
            ArrayRequest {
                name: "instance.generation".into(),
                domain: ArrayDomain::Instance,
                scalar: ScalarType::U32,
                lanes: 1,
                stride: Some(16),
                length: None,
            },
            ArrayRequest {
                name: "mesh.generation".into(),
                domain: ArrayDomain::Mesh,
                scalar: ScalarType::U32,
                lanes: 1,
                stride: Some(16),
                length: None,
            },
            ArrayRequest {
                name: "camera.state".into(),
                domain: ArrayDomain::Fixed,
                scalar: ScalarType::F32,
                lanes: 16,
                stride: Some(64),
                length: Some(1),
            },
            ArrayRequest {
                name: "material.state".into(),
                domain: ArrayDomain::Fixed,
                scalar: ScalarType::U32,
                lanes: MaterialState::LANES,
                stride: Some(112),
                length: Some(1),
            },
        ] {
            registry.allocate(request, capacities)?;
        }
        registry.publish_materials(&[Material::default()])?;
        registry.material_keys.clear();
        registry.layout_changed = false;
        Ok(registry)
    }

    pub fn allocate_json(
        &mut self,
        bytes: &[u8],
        capacities: RenderDataCapacities,
    ) -> Result<ArrayDescriptor, SharedSoaError> {
        let request = serde_json::from_slice(bytes)
            .map_err(|error| SharedSoaError::InvalidJson(error.to_string()))?;
        self.allocate(request, capacities)
    }

    pub fn allocate(
        &mut self,
        request: ArrayRequest,
        capacities: RenderDataCapacities,
    ) -> Result<ArrayDescriptor, SharedSoaError> {
        if !valid_name(&request.name) {
            return Err(SharedSoaError::InvalidName);
        }
        if !(1..=64).contains(&request.lanes) {
            return Err(SharedSoaError::InvalidLanes);
        }
        let physical_lanes = request
            .lanes
            .checked_add(if generation_guard(&request.name).is_some() {
                2
            } else {
                0
            })
            .ok_or(SharedSoaError::SizeOverflow)?;
        let minimum_stride = physical_lanes
            .checked_mul(4)
            .ok_or(SharedSoaError::SizeOverflow)?;
        let stride = request
            .stride
            .unwrap_or_else(|| (minimum_stride + 15) & !15);
        if stride < minimum_stride || stride % 16 != 0 {
            return Err(SharedSoaError::InvalidStride);
        }
        let capacity = request
            .domain
            .capacity(capacities, request.length)
            .filter(|capacity| *capacity > 0)
            .ok_or(SharedSoaError::InvalidLength)?;
        if let Some(existing) = self.arrays.get_mut(&request.name) {
            if existing.request.domain != request.domain
                || existing.request.scalar != request.scalar
                || existing.request.lanes != request.lanes
                || existing.stride_words != stride / 4
            {
                return Err(SharedSoaError::LayoutConflict);
            }
            if request.domain == ArrayDomain::Fixed {
                self.layout_changed |= existing.resize(capacity)?;
            }
            return existing.descriptor();
        }
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(SharedSoaError::SizeOverflow)?;
        let name = request.name.clone();
        let array = SharedArray::new(id, request, stride / 4, capacity)?;
        let descriptor = array.descriptor()?;
        self.arrays.insert(name, array);
        self.layout_changed = true;
        Ok(descriptor)
    }

    pub fn descriptors(&self) -> Result<Vec<ArrayDescriptor>, SharedSoaError> {
        self.arrays.values().map(SharedArray::descriptor).collect()
    }

    /// Copies a stable byte snapshot from a packed fixed array. Writers publish a
    /// complete upload with the same sequence lock used by all shared SOA columns.
    pub fn read_fixed_bytes(
        &mut self,
        id: u32,
        byte_length: u32,
    ) -> Result<Vec<u8>, SharedSoaError> {
        let array = self
            .arrays
            .values_mut()
            .find(|array| array.id == id)
            .ok_or(SharedSoaError::UnknownArray)?;
        if array.request.domain != ArrayDomain::Fixed
            || array.request.scalar != ScalarType::U32
            || array.stride_words != array.request.lanes
        {
            return Err(SharedSoaError::NotPackedFixed);
        }
        let available = array
            .length
            .checked_mul(array.request.lanes)
            .and_then(|words| words.checked_mul(4))
            .ok_or(SharedSoaError::SizeOverflow)?;
        if byte_length == 0 || byte_length > available {
            return Err(SharedSoaError::ByteRange);
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(byte_length as usize)
            .map_err(|_| SharedSoaError::AllocationFailed)?;
        let sequence = array.try_lock().ok_or(SharedSoaError::Busy)?;
        let word_length = byte_length.div_ceil(4);
        for index in 0..word_length {
            let slot = index / array.request.lanes;
            let lane = index % array.request.lanes;
            bytes.extend_from_slice(
                &array
                    .data_word(slot, lane)
                    .load(Ordering::Acquire)
                    .to_le_bytes(),
            );
        }
        bytes.truncate(byte_length as usize);
        array.unlock(sequence);
        Ok(bytes)
    }

    /// Publish an infrequent worker-owned camera reset into the canonical shared row.
    pub fn publish_camera(&mut self, camera: &Camera) -> Result<(), SharedSoaError> {
        let array = self
            .arrays
            .get_mut("camera.state")
            .ok_or(SharedSoaError::UnknownArray)?;
        let sequence = array.try_lock().ok_or(SharedSoaError::Busy)?;
        for (lane, value) in camera.shared_state().into_iter().enumerate() {
            array
                .data_word(0, lane as u32)
                .store(value.to_bits(), Ordering::Relaxed);
        }
        array.unlock(sequence);
        Ok(())
    }

    /// Apply a newly published shared camera row. Invalid external rows are replaced
    /// with the last valid worker state so all writers can recover on their next read.
    pub fn synchronize_camera(&mut self, camera: &mut Camera) -> Result<(), SharedSoaError> {
        let Some(array) = self.arrays.get_mut("camera.state") else {
            return Err(SharedSoaError::UnknownArray);
        };
        if !array.changed() {
            return Ok(());
        }
        let sequence = array.try_lock().ok_or(SharedSoaError::Busy)?;
        let state = std::array::from_fn(|lane| {
            f32::from_bits(array.data_word(0, lane as u32).load(Ordering::Acquire))
        });
        array.unlock(sequence);
        if !camera.apply_shared_state(state) {
            self.publish_camera(camera)?;
        }
        Ok(())
    }

    /// Replaces the packed material rows after a transactional render-data upload.
    pub fn publish_materials(&mut self, materials: &[Material]) -> Result<(), SharedSoaError> {
        let length = materials
            .iter()
            .map(|material| material.key.get())
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(SharedSoaError::SizeOverflow)?;
        self.allocate(
            ArrayRequest {
                name: "material.state".into(),
                domain: ArrayDomain::Fixed,
                scalar: ScalarType::U32,
                lanes: MaterialState::LANES,
                stride: Some(112),
                length: Some(length),
            },
            RenderDataCapacities {
                vertices: 0,
                indices: 0,
                meshes: 0,
                instances: 0,
            },
        )?;
        let array = self
            .arrays
            .get_mut("material.state")
            .ok_or(SharedSoaError::UnknownArray)?;
        let sequence = (0..1024)
            .find_map(|_| array.try_lock())
            .ok_or(SharedSoaError::Busy)?;
        let fallback = MaterialState::from(&Material::default()).words();
        for slot in 0..length {
            for (lane, word) in fallback.iter().copied().enumerate() {
                array
                    .data_word(slot, lane as u32)
                    .store(word, Ordering::Relaxed);
            }
        }
        for material in materials {
            for (lane, word) in MaterialState::from(material)
                .words()
                .into_iter()
                .enumerate()
            {
                array
                    .data_word(material.key.get(), lane as u32)
                    .store(word, Ordering::Relaxed);
            }
        }
        array.unlock(sequence);
        self.material_keys = materials.iter().map(|material| material.key).collect();
        self.material_keys.sort_by_key(|key| key.get());
        self.material_keys.dedup();
        Ok(())
    }

    /// Takes complete changed material rows for one batched queue write per material.
    pub fn take_material_words(
        &mut self,
    ) -> Option<Vec<(MaterialKey, [u32; MaterialState::LANES as usize])>> {
        let array = self.arrays.get_mut("material.state")?;
        if !array.changed() {
            return None;
        }
        let sequence = array.try_lock()?;
        let rows = self
            .material_keys
            .iter()
            .copied()
            .map(|key| {
                let words = std::array::from_fn(|lane| {
                    array
                        .data_word(key.get(), lane as u32)
                        .load(Ordering::Acquire)
                });
                (key, words)
            })
            .collect();
        array.unlock(sequence);
        Some(rows)
    }

    /// Reallocates matching-domain columns before a frame. Old blocks stay pinned.
    pub fn sync_capacities(
        &mut self,
        capacities: RenderDataCapacities,
    ) -> Result<bool, SharedSoaError> {
        let mut changed = std::mem::take(&mut self.layout_changed);
        for array in self.arrays.values_mut() {
            if array.request.domain == ArrayDomain::Fixed {
                continue;
            }
            let capacity = array
                .request
                .domain
                .capacity(capacities, None)
                .expect("non-fixed domains always resolve");
            changed |= array.resize(capacity)?;
        }
        Ok(changed)
    }

    fn take_instance_words(
        &mut self,
        name: &str,
        handles: &[InstanceHandle],
    ) -> Option<Vec<(InstanceHandle, Vec<u32>)>> {
        let array = self.arrays.get_mut(name)?;
        if !array.changed() {
            return None;
        }
        let sequence = array.try_lock()?;
        let mut values = Vec::new();
        for handle in handles.iter().copied() {
            let slot = handle.slot() as usize;
            let mutation_sequence = array
                .data_word(handle.slot(), array.request.lanes + 1)
                .load(Ordering::Acquire);
            if array.consumed_slot_sequences[slot] == mutation_sequence {
                continue;
            }
            array.consumed_slot_sequences[slot] = mutation_sequence;
            let expected_generation = array
                .data_word(handle.slot(), array.request.lanes)
                .load(Ordering::Acquire);
            if expected_generation == handle.generation() {
                let words = (0..array.request.lanes)
                    .map(|lane| array.data_word(handle.slot(), lane).load(Ordering::Acquire))
                    .collect();
                values.push((handle, words));
            }
        }
        array.unlock(sequence);
        Some(values)
    }

    /// Publishes newly live slots, then applies committed mutable columns. Existing
    /// slots are never republished, so a concurrent shared-memory write cannot be
    /// overwritten by an unrelated render-data revision.
    pub fn synchronize_render_data(&mut self, data: &mut RenderData) {
        self.publish_instance_handles(data);
        self.publish_mesh_handles(data);
        let handles: Vec<_> = data.instances().map(|(handle, _)| handle).collect();
        if let Some(transforms) = self.take_instance_words("instance.transform", &handles) {
            for (handle, words) in transforms {
                let mut transform = [[0.0; 4]; 4];
                for (index, word) in words.into_iter().enumerate() {
                    transform[index / 4][index % 4] = f32::from_bits(word);
                }
                let _ = data.set_instance_transform(handle, transform);
            }
        }
        if let Some(types) = self.take_instance_words("instance.type", &handles) {
            for (handle, words) in types {
                let Ok(words) = <Vec<u32> as TryInto<[u32; 16]>>::try_into(words) else {
                    continue;
                };
                let _ = data.set_instance_type(handle, crate::render_data::InstanceType { words });
            }
        }
    }

    fn publish_instance_handles(&mut self, data: &RenderData) {
        let instances: Vec<_> = data.instances().map(|(_, value)| value).collect();
        let current: BTreeMap<_, _> = instances
            .iter()
            .map(|instance| (instance.handle.slot(), instance.handle.generation()))
            .collect();
        let changed: Vec<_> = instances
            .iter()
            .filter(|instance| {
                self.published_instance_generations
                    .get(&instance.handle.slot())
                    != Some(&instance.handle.generation())
            })
            .collect();
        let removed: Vec<_> = self
            .published_instance_generations
            .keys()
            .filter(|slot| !current.contains_key(slot))
            .copied()
            .collect();
        if changed.is_empty() && removed.is_empty() {
            return;
        }
        for name in ["instance.transform", "instance.type"] {
            let Some(array) = self.arrays.get_mut(name) else {
                return;
            };
            let Some(sequence) = array.try_lock() else {
                return;
            };
            for instance in &changed {
                match name {
                    "instance.transform" => {
                        for (lane, value) in instance.model.iter().flatten().enumerate() {
                            array
                                .data_word(instance.handle.slot(), lane as u32)
                                .store(value.to_bits(), Ordering::Relaxed);
                        }
                    }
                    "instance.type" => {
                        for (lane, value) in instance.instance_type.words.iter().enumerate() {
                            array
                                .data_word(instance.handle.slot(), lane as u32)
                                .store(*value, Ordering::Relaxed);
                        }
                    }
                    _ => unreachable!(),
                }
                array
                    .data_word(instance.handle.slot(), array.request.lanes)
                    .store(instance.handle.generation(), Ordering::Relaxed);
                let mutation_sequence = array
                    .data_word(instance.handle.slot(), array.request.lanes + 1)
                    .load(Ordering::Relaxed);
                array.consumed_slot_sequences[instance.handle.slot() as usize] = mutation_sequence;
            }
            array.unlock(sequence);
        }
        let Some(generations) = self.arrays.get_mut("instance.generation") else {
            return;
        };
        let Some(sequence) = generations.try_lock() else {
            return;
        };
        for slot in removed {
            generations.data_word(slot, 0).store(0, Ordering::Relaxed);
        }
        for instance in changed {
            generations
                .data_word(instance.handle.slot(), 0)
                .store(instance.handle.generation(), Ordering::Relaxed);
        }
        generations.unlock(sequence);
        self.published_instance_generations = current;
    }

    fn publish_mesh_handles(&mut self, data: &RenderData) {
        let meshes: Vec<_> = data.meshes().map(|(_, value)| value).collect();
        let current: BTreeMap<_, _> = meshes
            .iter()
            .map(|mesh| (mesh.handle.slot(), mesh.handle.generation()))
            .collect();
        if current == self.published_mesh_generations {
            return;
        }
        let Some(generations) = self.arrays.get_mut("mesh.generation") else {
            return;
        };
        let Some(sequence) = generations.try_lock() else {
            return;
        };
        for slot in self
            .published_mesh_generations
            .keys()
            .filter(|slot| !current.contains_key(slot))
        {
            generations.data_word(*slot, 0).store(0, Ordering::Relaxed);
        }
        for mesh in meshes.iter().filter(|mesh| {
            self.published_mesh_generations.get(&mesh.handle.slot())
                != Some(&mesh.handle.generation())
        }) {
            generations
                .data_word(mesh.handle.slot(), 0)
                .store(mesh.handle.generation(), Ordering::Relaxed);
        }
        generations.unlock(sequence);
        self.published_mesh_generations = current;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capacities(instances: u32) -> RenderDataCapacities {
        RenderDataCapacities {
            vertices: 0,
            indices: 0,
            meshes: 8,
            instances,
        }
    }

    #[test]
    fn camera_is_one_aligned_row_and_external_updates_are_validated() {
        let mut registry = SharedSoaRegistry::new(capacities(8)).unwrap();
        let descriptor = registry.arrays["camera.state"].descriptor().unwrap();
        assert_eq!(descriptor.domain, ArrayDomain::Fixed);
        assert_eq!(descriptor.scalar, ScalarType::F32);
        assert_eq!(
            (descriptor.lanes, descriptor.stride, descriptor.length),
            (16, 64, 1)
        );

        let mut camera = Camera::new(1.5);
        registry.publish_camera(&camera).unwrap();
        let mut external = camera.shared_state();
        external[0..3].copy_from_slice(&[2.0, 1.0, 4.0]);
        external[13] = 2.0;
        let array = registry.arrays.get_mut("camera.state").unwrap();
        let sequence = array.word(9).load(Ordering::Acquire);
        array.word(9).store(sequence + 1, Ordering::Release);
        for (lane, value) in external.into_iter().enumerate() {
            array
                .data_word(0, lane as u32)
                .store(value.to_bits(), Ordering::Relaxed);
        }
        array.word(9).store(sequence + 2, Ordering::Release);

        registry.synchronize_camera(&mut camera).unwrap();
        assert_eq!(camera.shared_state(), external);

        let valid = camera.shared_state();
        let array = registry.arrays.get_mut("camera.state").unwrap();
        let sequence = array.word(9).load(Ordering::Acquire);
        array.word(9).store(sequence + 1, Ordering::Release);
        array
            .data_word(0, 13)
            .store(f32::NAN.to_bits(), Ordering::Relaxed);
        array.word(9).store(sequence + 2, Ordering::Release);
        registry.synchronize_camera(&mut camera).unwrap();
        let recovered = registry.arrays["camera.state"].data_word(0, 13);
        assert_eq!(f32::from_bits(recovered.load(Ordering::Acquire)), valid[13]);
    }

    #[test]
    fn material_rows_are_packed_resized_and_consumed_after_external_writes() {
        let mut registry = SharedSoaRegistry::new(capacities(8)).unwrap();
        let mut material = Material {
            key: MaterialKey::new(3),
            ..Material::default()
        };
        material.base_color_factor = [0.2, 0.4, 0.8, 1.0];
        material.roughness_factor = 0.75;
        registry.publish_materials(&[material.clone()]).unwrap();
        let descriptor = registry.arrays["material.state"].descriptor().unwrap();
        assert_eq!(
            (
                descriptor.scalar,
                descriptor.lanes,
                descriptor.stride,
                descriptor.length
            ),
            (ScalarType::U32, 28, 112, 4)
        );

        let array = registry.arrays.get_mut("material.state").unwrap();
        let sequence = array.word(9).load(Ordering::Acquire);
        array.word(9).store(sequence + 1, Ordering::Release);
        array
            .data_word(3, 9)
            .store(0.25f32.to_bits(), Ordering::Relaxed);
        array.word(9).store(sequence + 2, Ordering::Release);
        let rows = registry.take_material_words().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, MaterialKey::new(3));
        assert_eq!(f32::from_bits(rows[0].1[9]), 0.25);
        assert!(registry.take_material_words().is_none());
    }

    #[test]
    fn custom_layouts_are_aligned_idempotent_and_conflict_checked() {
        let mut registry = SharedSoaRegistry::new(capacities(8)).unwrap();
        let request = ArrayRequest {
            name: "instance.velocity".into(),
            domain: ArrayDomain::Instance,
            scalar: ScalarType::F32,
            lanes: 3,
            stride: None,
            length: None,
        };
        let first = registry.allocate(request.clone(), capacities(8)).unwrap();
        let second = registry.allocate(request, capacities(8)).unwrap();
        assert_eq!(first, second);
        assert_eq!((first.stride, first.capacity), (16, 8));
        let conflict = ArrayRequest {
            lanes: 4,
            ..serde_json::from_str::<ArrayRequest>(
                r#"{"name":"instance.velocity","domain":"instance","scalar":"f32","lanes":3,"stride":null,"length":null}"#,
            )
            .unwrap()
        };
        assert_eq!(
            registry.allocate(conflict, capacities(8)),
            Err(SharedSoaError::LayoutConflict)
        );
    }

    #[test]
    fn domain_growth_replaces_layout_and_retains_old_storage() {
        let mut registry = SharedSoaRegistry::new(capacities(8)).unwrap();
        let old = registry.arrays["instance.transform"].storage.as_ptr();
        assert!(registry.sync_capacities(capacities(16)).unwrap());
        let array = &registry.arrays["instance.transform"];
        assert_ne!(old, array.storage.as_ptr());
        assert_eq!(array.retired.len(), 1);
        assert_eq!(array.descriptor().unwrap().capacity, 16);
    }

    #[test]
    fn fixed_arrays_grow_and_publish_stable_byte_uploads() {
        let mut registry = SharedSoaRegistry::new(capacities(8)).unwrap();
        let request = |length| ArrayRequest {
            name: "upload.renderData".into(),
            domain: ArrayDomain::Fixed,
            scalar: ScalarType::U32,
            lanes: 4,
            stride: Some(16),
            length: Some(length),
        };
        let first = registry.allocate(request(1), capacities(8)).unwrap();
        let second = registry.allocate(request(2), capacities(8)).unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!((second.length, second.capacity), (2, 2));

        let array = registry.arrays.get_mut("upload.renderData").unwrap();
        let sequence = array.try_lock().unwrap();
        array
            .data_word(0, 0)
            .store(u32::from_le_bytes(*b"YRDP"), Ordering::Relaxed);
        array
            .data_word(0, 1)
            .store(u32::from_le_bytes([2, 0, 0, 0]), Ordering::Relaxed);
        array.unlock(sequence);
        assert_eq!(
            registry.read_fixed_bytes(first.id, 8).unwrap(),
            b"YRDP\x02\0\0\0"
        );
    }

    #[test]
    fn guarded_columns_ignore_stale_slot_writers() {
        let mut registry = SharedSoaRegistry::new(capacities(8)).unwrap();
        let handle = InstanceHandle::from_parts(2, 7);
        let array = registry.arrays.get_mut("instance.transform").unwrap();
        let descriptor = array.descriptor().unwrap();
        assert_eq!(
            (descriptor.stride, descriptor.generation_guard),
            (80, Some(ArrayDomain::Instance))
        );

        array
            .data_word(2, 0)
            .store(1.0f32.to_bits(), Ordering::Relaxed);
        array.data_word(2, 16).store(6, Ordering::Relaxed);
        array.data_word(2, 17).store(1, Ordering::Relaxed);
        array.word(9).store(2, Ordering::Release);
        assert!(registry
            .take_instance_words("instance.transform", &[handle])
            .unwrap()
            .is_empty());

        let array = registry.arrays.get_mut("instance.transform").unwrap();
        array
            .data_word(2, 0)
            .store(2.0f32.to_bits(), Ordering::Relaxed);
        array.data_word(2, 16).store(7, Ordering::Relaxed);
        array.data_word(2, 17).store(2, Ordering::Relaxed);
        array.word(9).store(6, Ordering::Release);
        let values = registry
            .take_instance_words("instance.transform", &[handle])
            .unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(f32::from_bits(values[0].1[0]), 2.0);
    }
}
