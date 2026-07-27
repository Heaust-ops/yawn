//! Triple-buffered, immutable packed scene snapshot shared with JavaScript.
use std::sync::atomic::{AtomicU32, Ordering};

use crate::{render_data::RenderFlags, renderer::scene_frame::SceneFramePlan};

pub const MAGIC: u32 = u32::from_le_bytes(*b"YSNP");
pub const BLOB_MAGIC: u32 = u32::from_le_bytes(*b"RDS1");
pub const CONTROL_VERSION: u32 = 1;
pub const SCHEMA: u32 = 1;
pub const SLOT_COUNT: usize = 3;
pub const INIT: u32 = 0;
pub const OPEN: u32 = 1;
pub const FAILED: u32 = 2;
pub const CLOSED: u32 = 3;
pub const FREE: u32 = 0;
pub const WRITING: u32 = 1;
pub const READY: u32 = 2;
pub const READING: u32 = 3;
pub const ERROR_NO_SLOT: u32 = 1;
pub const ERROR_OVERFLOW: u32 = 2;
pub const ERROR_INVARIANT: u32 = 3;
pub const ERROR_PUBLICATION: u32 = 4;
const CONTROL_BYTES: u32 = 256;
const SLOT_BYTES: u32 = 64;
const SNAPSHOT_HEADER_BYTES: usize = 64;
const DATA_OFFSET: usize = 512;
const DESCRIPTOR_BYTES: usize = 32;
const STREAMS: usize = 14;
const SCHEMA_FLAGS: u32 = 3; // dense arrays | affine transforms

#[repr(C, align(64))]
pub struct SnapshotDescriptor(pub [AtomicU32; 16]);

#[repr(C, align(64))]
pub struct SnapshotControl {
    pub header: [AtomicU32; 16],
    pub slots: [SnapshotDescriptor; SLOT_COUNT],
}

#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct SnapshotBlock(pub [u8; 16]);

pub struct SharedSnapshot {
    pub control: Box<SnapshotControl>,
    blocks: [Vec<SnapshotBlock>; SLOT_COUNT],
    last_revision: Option<u64>,
    next_epoch: u32,
    layout_epoch: u32,
}

impl SharedSnapshot {
    pub fn new() -> Self {
        let control = Box::new(SnapshotControl {
            header: std::array::from_fn(|_| AtomicU32::new(0)),
            slots: std::array::from_fn(|_| {
                SnapshotDescriptor(std::array::from_fn(|_| AtomicU32::new(0)))
            }),
        });
        let this = Self {
            control,
            blocks: Default::default(),
            last_revision: None,
            next_epoch: 1,
            layout_epoch: 0,
        };
        for (i, value) in [
            MAGIC,
            CONTROL_VERSION,
            CONTROL_BYTES,
            SLOT_COUNT as u32,
            SLOT_BYTES,
            SCHEMA,
            INIT,
        ]
        .into_iter()
        .enumerate()
        {
            this.control.header[i].store(value, Ordering::Relaxed);
        }
        // No publication exists yet; zero is a valid slot number.
        this.control.header[9].store(u32::MAX, Ordering::Relaxed);
        this
    }

    pub fn control_ptr(&self) -> u32 {
        self.control.as_ref() as *const _ as usize as u32
    }

    /// Permanently fails snapshot publication without affecting rendering or mutations.
    pub fn fail(&self, error: u32) {
        self.control.header[14].store(error, Ordering::Relaxed);
        self.control.header[6].store(FAILED, Ordering::Release);
    }

    /// Packs and publishes if `data` changed. Returns the newly published data epoch.
    pub fn publish(&mut self, data: &SceneFramePlan) -> Result<Option<u32>, u32> {
        if self.control.header[6].load(Ordering::Acquire) == FAILED {
            return Err(self.control.header[14].load(Ordering::Relaxed));
        }
        if self.last_revision == Some(data.revision) {
            return Ok(None);
        }
        let slot = match self.claim_slot() {
            Some(slot) => slot,
            None => {
                self.fail(ERROR_NO_SLOT);
                return Err(ERROR_NO_SLOT);
            }
        };
        let result = self.publish_claimed(slot, data);
        if let Err(error) = result {
            self.control.slots[slot].0[0].store(FREE, Ordering::Release);
            self.fail(error);
        }
        result
    }

    fn publish_claimed(&mut self, slot: usize, data: &SceneFramePlan) -> Result<Option<u32>, u32> {
        let epoch = self.next_epoch;
        let next_epoch = epoch.checked_add(1).ok_or(ERROR_OVERFLOW)?;
        let bytes = pack(data, epoch)?;
        let blocks = bytes.len().checked_add(15).ok_or(ERROR_OVERFLOW)? / 16;
        if self.blocks[slot].capacity() < blocks {
            self.layout_epoch = self.layout_epoch.checked_add(1).ok_or(ERROR_OVERFLOW)?;
        }
        self.blocks[slot].resize(blocks, SnapshotBlock([0; 16]));
        let allocation_bytes = blocks.checked_mul(16).ok_or(ERROR_OVERFLOW)?;
        let target = unsafe {
            std::slice::from_raw_parts_mut(
                self.blocks[slot].as_mut_ptr().cast::<u8>(),
                allocation_bytes,
            )
        };
        target[..bytes.len()].copy_from_slice(&bytes);
        let ptr = self.blocks[slot].as_ptr() as usize;
        let ptr32 = u32::try_from(ptr).map_err(|_| ERROR_OVERFLOW)?;
        let length = u32::try_from(bytes.len()).map_err(|_| ERROR_OVERFLOW)?;
        let revision = data.revision;
        let d = &self.control.slots[slot].0;
        let values = [
            epoch,
            self.layout_epoch,
            ptr32,
            length,
            revision as u32,
            (revision >> 32) as u32,
            data.meshes.len() as u32,
            data.occurrences.len() as u32,
            SCHEMA,
            SNAPSHOT_HEADER_BYTES as u32,
            0,
            0,
            0,
            0,
            0,
        ];
        for (i, value) in values.into_iter().enumerate() {
            d[i + 1].store(value, Ordering::Relaxed);
        }
        let end = ptr.checked_add(allocation_bytes).ok_or(ERROR_OVERFLOW)?;
        let pages = wasm_pages(end)?;
        let seq = self.open_sequence()?;
        d[0].store(READY, Ordering::Release);
        for (i, value) in [
            epoch,
            slot as u32,
            revision as u32,
            (revision >> 32) as u32,
            pages,
            self.layout_epoch,
        ]
        .into_iter()
        .enumerate()
        {
            self.control.header[8 + i].store(value, Ordering::Relaxed);
        }
        self.control.header[14].store(0, Ordering::Relaxed);
        self.control.header[15].store(0, Ordering::Relaxed);
        self.control.header[6].store(OPEN, Ordering::Relaxed);
        self.control.header[7].fetch_add(1, Ordering::Release);
        debug_assert_eq!(seq & 1, 0);
        self.next_epoch = next_epoch;
        self.last_revision = Some(revision);
        Ok(Some(epoch))
    }

    fn open_sequence(&self) -> Result<u32, u32> {
        loop {
            let seq = self.control.header[7].load(Ordering::Acquire);
            if seq & 1 != 0 {
                std::hint::spin_loop();
                continue;
            }
            match self.control.header[7].compare_exchange_weak(
                seq,
                seq.checked_add(1).ok_or(ERROR_OVERFLOW)?,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Ok(seq),
                Err(_) => continue,
            }
        }
    }

    fn claim_slot(&self) -> Option<usize> {
        loop {
            for slot in 0..SLOT_COUNT {
                if self.control.slots[slot].0[0]
                    .compare_exchange(FREE, WRITING, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    return Some(slot);
                }
            }
            let oldest = (0..SLOT_COUNT)
                .filter(|&slot| self.control.slots[slot].0[0].load(Ordering::Acquire) == READY)
                .min_by_key(|&slot| self.control.slots[slot].0[1].load(Ordering::Relaxed));
            let slot = oldest?;
            if self.control.slots[slot].0[0]
                .compare_exchange(READY, WRITING, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Some(slot);
            }
            // A reader or another claimant won. Recompute rather than touching READING.
        }
    }
}

fn align16(value: usize) -> Result<usize, u32> {
    Ok(value.checked_add(15).ok_or(ERROR_OVERFLOW)? & !15)
}

#[cfg(target_arch = "wasm32")]
fn wasm_pages(_minimum_end: usize) -> Result<u32, u32> {
    u32::try_from(core::arch::wasm32::memory_size(0)).map_err(|_| ERROR_OVERFLOW)
}

#[cfg(not(target_arch = "wasm32"))]
fn wasm_pages(minimum_end: usize) -> Result<u32, u32> {
    u32::try_from(minimum_end.checked_add(65535).ok_or(ERROR_OVERFLOW)? / 65536)
        .map_err(|_| ERROR_OVERFLOW)
}

fn pack(data: &SceneFramePlan, epoch: u32) -> Result<Vec<u8>, u32> {
    let meshes = &data.meshes;
    let instances = &data.occurrences;
    let strides = [4usize, 4, 4, 12, 12, 4, 4, 4, 4, 4, 64, 12, 12, 4];
    let components = [1u32, 1, 1, 3, 3, 1, 1, 1, 1, 1, 16, 3, 3, 1];
    let scalar = [1u32, 1, 1, 2, 2, 1, 1, 1, 1, 1, 2, 2, 2, 1];
    let counts = [meshes.len(); 5]
        .into_iter()
        .chain([instances.len(); 9])
        .collect::<Vec<_>>();
    let mut offsets = [0usize; STREAMS];
    let mut cursor = DATA_OFFSET;
    for i in 0..STREAMS {
        offsets[i] = cursor;
        let bytes = strides[i].checked_mul(counts[i]).ok_or(ERROR_OVERFLOW)?;
        cursor = align16(cursor.checked_add(bytes).ok_or(ERROR_OVERFLOW)?)?;
    }
    let total = u32::try_from(cursor).map_err(|_| ERROR_OVERFLOW)?;
    let mesh_count = u32::try_from(meshes.len()).map_err(|_| ERROR_OVERFLOW)?;
    let instance_count = u32::try_from(instances.len()).map_err(|_| ERROR_OVERFLOW)?;
    let mut out = vec![0u8; cursor];
    let put32 = |out: &mut [u8], at: usize, value: u32| {
        out[at..at + 4].copy_from_slice(&value.to_le_bytes())
    };
    let revision = data.revision;
    for (i, value) in [
        BLOB_MAGIC,
        SCHEMA,
        SNAPSHOT_HEADER_BYTES as u32,
        total,
        epoch,
        revision as u32,
        (revision >> 32) as u32,
        STREAMS as u32,
        SNAPSHOT_HEADER_BYTES as u32,
        DESCRIPTOR_BYTES as u32,
        mesh_count,
        instance_count,
        0x0102_0304,
        SCHEMA_FLAGS,
        0,
        0,
    ]
    .into_iter()
    .enumerate()
    {
        put32(&mut out, i * 4, value);
    }
    for i in 0..STREAMS {
        let at = SNAPSHOT_HEADER_BYTES + i * DESCRIPTOR_BYTES;
        for (j, value) in [
            i as u32 + 1,
            scalar[i],
            offsets[i] as u32,
            counts[i] as u32,
            components[i],
            strides[i] as u32,
            4,
            0,
        ]
        .into_iter()
        .enumerate()
        {
            put32(&mut out, at + j * 4, value);
        }
    }
    for (dense, mesh) in meshes.iter().enumerate() {
        for (i, value) in [
            mesh.handle.slot(),
            mesh.handle.generation(),
            mesh.flags.bits(),
        ]
        .into_iter()
        .enumerate()
        {
            put32(&mut out, offsets[i] + dense * 4, value);
        }
        for i in 0..3 {
            put32(
                &mut out,
                offsets[3] + dense * 12 + i * 4,
                mesh.aabb.min[i].to_bits(),
            );
            put32(
                &mut out,
                offsets[4] + dense * 12 + i * 4,
                mesh.aabb.max[i].to_bits(),
            );
        }
    }
    for (dense, instance) in instances.iter().enumerate() {
        let mesh = data
            .meshes
            .get(instance.mesh_index)
            .ok_or(ERROR_INVARIANT)?;
        for (i, value) in [
            instance.handle.slot(),
            instance.handle.generation(),
            instance.mesh.slot(),
            instance.mesh.generation(),
            instance.flags.bits(),
        ]
        .into_iter()
        .enumerate()
        {
            put32(&mut out, offsets[5 + i] + dense * 4, value);
        }
        for i in 0..16 {
            put32(
                &mut out,
                offsets[10] + dense * 64 + i * 4,
                instance.model[i / 4][i % 4].to_bits(),
            );
        }
        for i in 0..3 {
            put32(
                &mut out,
                offsets[11] + dense * 12 + i * 4,
                instance.world_aabb.min[i].to_bits(),
            );
            put32(
                &mut out,
                offsets[12] + dense * 12 + i * 4,
                instance.world_aabb.max[i].to_bits(),
            );
        }
        put32(
            &mut out,
            offsets[13] + dense * 4,
            (mesh.flags.contains(RenderFlags::VISIBLE)
                && instance.flags.contains(RenderFlags::VISIBLE)) as u32,
        );
    }
    Ok(out)
}

impl Drop for SharedSnapshot {
    fn drop(&mut self) {
        if self.control.header[6].load(Ordering::Acquire) != FAILED {
            self.control.header[6].store(CLOSED, Ordering::Release);
        }
    }
}

const _: [(); 256] = [(); std::mem::size_of::<SnapshotControl>()];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_data::{
        MeshCreateInfo, PipelineKey, RenderData, RenderDataConfig, IDENTITY_MODEL_TRANSFORM,
    };

    #[test]
    fn exact_control_layout_and_initial_values() {
        assert_eq!(std::mem::size_of::<SnapshotControl>(), 256);
        assert_eq!(std::mem::size_of::<SnapshotDescriptor>(), 64);
        let snapshot = SharedSnapshot::new();
        let values: Vec<_> = snapshot
            .control
            .header
            .iter()
            .map(|v| v.load(Ordering::Relaxed))
            .collect();
        assert_eq!(&values[..7], &[MAGIC, 1, 256, 3, 64, 1, INIT]);
        assert_eq!(values[9], u32::MAX);
    }

    #[test]
    fn claiming_never_overwrites_reading_and_prefers_free() {
        let snapshot = SharedSnapshot::new();
        snapshot.control.slots[0].0[0].store(READING, Ordering::Relaxed);
        assert_eq!(snapshot.claim_slot(), Some(1));
        assert_eq!(
            snapshot.control.slots[0].0[0].load(Ordering::Relaxed),
            READING
        );
        assert_eq!(
            snapshot.control.slots[1].0[0].load(Ordering::Relaxed),
            WRITING
        );
    }

    #[test]
    fn producer_blob_abi_matches_schema_exactly() {
        let mut data = RenderData::new(RenderDataConfig::default()).unwrap();
        let create = |data: &mut RenderData, flags, x: f32| {
            data.create_mesh(MeshCreateInfo {
                positions: &[[x, 0., 0.], [x + 2., 0., 0.], [x, 3., 0.]],
                normals: &[[0., 0., 1.]; 3],
                tangents: &[[1., 0., 0., 1.]; 3],
                uvs: &[[0., 0.]; 3],
                indices: &[0, 1, 2],
                pipeline: PipelineKey::new(7),
                material: crate::render_data::MaterialKey::DEFAULT,
                flags,
                default_instance_flags: RenderFlags::VISIBLE,
                default_transform: IDENTITY_MODEL_TRANSFORM,
            })
            .unwrap()
        };
        let visible = create(&mut data, RenderFlags::VISIBLE, -1.0);
        let hidden = create(&mut data, RenderFlags::NONE, 10.0);
        let mut model = IDENTITY_MODEL_TRANSFORM;
        model[0][0] = 2.0;
        model[1][1] = 0.5;
        model[3][0] = 4.0;
        model[3][1] = -2.0;
        let extra = data
            .create_instance(hidden.mesh, model, RenderFlags::NONE)
            .unwrap();
        let plan = SceneFramePlan::build(&data).unwrap();
        let epoch = 0x1234_5678;
        let blob = pack(&plan, epoch).unwrap();
        let word = |at: usize| u32::from_le_bytes(blob[at..at + 4].try_into().unwrap());

        assert_eq!(word(0), BLOB_MAGIC);
        assert_eq!(word(4), SCHEMA);
        assert_eq!(word(8), SNAPSHOT_HEADER_BYTES as u32);
        assert_eq!(word(12), blob.len() as u32);
        assert_eq!(word(16), epoch);
        assert_eq!(word(20), plan.revision as u32);
        assert_eq!(word(24), (plan.revision >> 32) as u32);
        assert_eq!(word(28), STREAMS as u32);
        assert_eq!(word(32), SNAPSHOT_HEADER_BYTES as u32);
        assert_eq!(word(36), DESCRIPTOR_BYTES as u32);
        assert_eq!(word(40), 2);
        assert_eq!(word(44), 3);
        assert_eq!(word(48), 0x0102_0304);
        assert_eq!(word(52), SCHEMA_FLAGS);

        let strides = [4, 4, 4, 12, 12, 4, 4, 4, 4, 4, 64, 12, 12, 4];
        let scalars = [1, 1, 1, 2, 2, 1, 1, 1, 1, 1, 2, 2, 2, 1];
        let components = [1, 1, 1, 3, 3, 1, 1, 1, 1, 1, 16, 3, 3, 1];
        let counts = [2usize; 5]
            .into_iter()
            .chain([3usize; 9])
            .collect::<Vec<_>>();
        let mut offsets = Vec::new();
        let mut cursor = DATA_OFFSET;
        for i in 0..STREAMS {
            let at = SNAPSHOT_HEADER_BYTES + i * DESCRIPTOR_BYTES;
            offsets.push(cursor);
            assert_eq!(
                [
                    word(at),
                    word(at + 4),
                    word(at + 8),
                    word(at + 12),
                    word(at + 16),
                    word(at + 20),
                    word(at + 24),
                    word(at + 28)
                ],
                [
                    i as u32 + 1,
                    scalars[i],
                    cursor as u32,
                    counts[i] as u32,
                    components[i],
                    strides[i] as u32,
                    4,
                    0
                ]
            );
            assert_eq!(cursor % 16, 0);
            cursor = (cursor + strides[i] as usize * counts[i] + 15) & !15;
        }
        assert_eq!(cursor, blob.len());

        for (dense, mesh) in plan.meshes.iter().enumerate() {
            assert_eq!(word(offsets[0] + dense * 4), mesh.handle.slot());
            assert_eq!(word(offsets[1] + dense * 4), mesh.handle.generation());
            assert_eq!(word(offsets[2] + dense * 4), mesh.flags.bits());
            for axis in 0..3 {
                assert_eq!(
                    word(offsets[3] + dense * 12 + axis * 4),
                    mesh.aabb.min[axis].to_bits()
                );
                assert_eq!(
                    word(offsets[4] + dense * 12 + axis * 4),
                    mesh.aabb.max[axis].to_bits()
                );
            }
        }
        for (dense, occurrence) in plan.occurrences.iter().enumerate() {
            assert_eq!(
                [
                    word(offsets[5] + dense * 4),
                    word(offsets[6] + dense * 4),
                    word(offsets[7] + dense * 4),
                    word(offsets[8] + dense * 4),
                    word(offsets[9] + dense * 4)
                ],
                [
                    occurrence.handle.slot(),
                    occurrence.handle.generation(),
                    occurrence.mesh.slot(),
                    occurrence.mesh.generation(),
                    occurrence.flags.bits()
                ]
            );
            for i in 0..16 {
                assert_eq!(
                    word(offsets[10] + dense * 64 + i * 4),
                    occurrence.model[i / 4][i % 4].to_bits()
                );
            }
            for axis in 0..3 {
                assert_eq!(
                    word(offsets[11] + dense * 12 + axis * 4),
                    occurrence.world_aabb.min[axis].to_bits()
                );
                assert_eq!(
                    word(offsets[12] + dense * 12 + axis * 4),
                    occurrence.world_aabb.max[axis].to_bits()
                );
            }
            let mesh_visible = plan.meshes[occurrence.mesh_index]
                .flags
                .contains(RenderFlags::VISIBLE);
            assert_eq!(
                word(offsets[13] + dense * 4),
                (mesh_visible && occurrence.flags.contains(RenderFlags::VISIBLE)) as u32
            );
        }
        assert!(plan
            .meshes
            .windows(2)
            .all(|w| w[0].handle.slot() < w[1].handle.slot()));
        assert!(plan
            .occurrences
            .windows(2)
            .all(|w| w[0].handle.slot() < w[1].handle.slot()));
        assert_eq!(
            plan.occurrences
                .iter()
                .find(|o| o.handle == extra)
                .unwrap()
                .model,
            model
        );
        assert!(plan.occurrences.iter().any(|o| o.mesh == visible.mesh));
    }
}
