//! Stable shared-memory wire contract between browser main and render worker.
//!
//! This is deliberately a projection. Nothing in this module points at a `Vec`.
use std::sync::atomic::{AtomicU32, Ordering};

use crate::render_data::{InstanceHandle, RenderData, MAX_INSTANCE_CAPACITY};

pub const ABI_MAGIC: u32 = 0x5245_4e44;
pub const ABI_VERSION: u32 = 1;
pub const RING_CAPACITY: usize = 256;
pub const RECORD_WORDS: usize = 24;
pub const PROJECTION_CAPACITY: usize = MAX_INSTANCE_CAPACITY;

pub const CMD_BATCH_BEGIN: u32 = 1;
pub const CMD_BATCH_END: u32 = 2;
pub const CMD_CLONE: u32 = 3;
pub const CMD_DESTROY: u32 = 4;
pub const CMD_TRANSFORM: u32 = 5;
pub const CMD_VISIBLE: u32 = 6;
pub const CMD_PIPELINE: u32 = 7;
pub const CMD_LOAD_SCENE: u32 = 8;

pub enum BatchPop {
    EmptyOrIncomplete,
    Malformed,
    Accepted(Vec<[u32; RECORD_WORDS]>),
}

#[repr(C)]
pub struct RingHeader {
    pub head: AtomicU32,
    pub tail: AtomicU32,
    pub capacity: u32,
    pub record_words: u32,
}

impl RingHeader {
    fn new() -> Self {
        Self {
            head: AtomicU32::new(0),
            tail: AtomicU32::new(0),
            capacity: RING_CAPACITY as u32,
            record_words: RECORD_WORDS as u32,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProjectionRecord {
    pub generation: u32,
    pub status: u32,
    pub geometry_slot: u32,
    pub geometry_generation: u32,
    pub pipeline_key: u32,
    pub render_flags: u32,
    pub transform: [f32; 16],
}

impl ProjectionRecord {
    const EMPTY: Self = Self {
        generation: 0,
        status: 0,
        geometry_slot: 0,
        geometry_generation: 0,
        pipeline_key: 0,
        render_flags: 0,
        transform: [0.0; 16],
    };
}

/// Pinned for the renderer lifetime. Offsets and capacities are self-described.
#[repr(C)]
pub struct SharedAbi {
    pub magic: u32,
    pub version: u32,
    pub byte_size: u32,
    pub layout_epoch: AtomicU32,
    pub frame_credit: AtomicU32,
    pub projection_epoch: AtomicU32,
    pub projection_len: u32,
    pub projection_record_words: u32,
    pub command: RingHeader,
    pub completion: RingHeader,
    command_records: [[u32; RECORD_WORDS]; RING_CAPACITY],
    completion_records: [[u32; RECORD_WORDS]; RING_CAPACITY],
    projection: [ProjectionRecord; PROJECTION_CAPACITY],
}

impl SharedAbi {
    pub fn new() -> Box<Self> {
        Box::new(Self {
            magic: ABI_MAGIC,
            version: ABI_VERSION,
            byte_size: std::mem::size_of::<Self>() as u32,
            layout_epoch: AtomicU32::new(1),
            frame_credit: AtomicU32::new(1),
            projection_epoch: AtomicU32::new(0),
            projection_len: PROJECTION_CAPACITY as u32,
            projection_record_words: (std::mem::size_of::<ProjectionRecord>() / 4) as u32,
            command: RingHeader::new(),
            completion: RingHeader::new(),
            command_records: [[0; RECORD_WORDS]; RING_CAPACITY],
            completion_records: [[0; RECORD_WORDS]; RING_CAPACITY],
            projection: [ProjectionRecord::EMPTY; PROJECTION_CAPACITY],
        })
    }

    pub fn descriptor(&self) -> [u32; 10] {
        let base = self as *const Self as usize;
        [
            base as u32,
            ABI_VERSION,
            self.byte_size,
            (&self.command as *const RingHeader as usize - base) as u32,
            (self.command_records.as_ptr() as usize - base) as u32,
            (&self.completion as *const RingHeader as usize - base) as u32,
            (self.completion_records.as_ptr() as usize - base) as u32,
            (self.projection.as_ptr() as usize - base) as u32,
            PROJECTION_CAPACITY as u32,
            std::mem::size_of::<ProjectionRecord>() as u32,
        ]
    }

    /// Takes no data unless an entire begin/count/end frame has been published.
    pub fn pop_batch(&self) -> BatchPop {
        let tail = self.command.tail.load(Ordering::Relaxed);
        let head = self.command.head.load(Ordering::Acquire);
        if tail == head {
            return BatchPop::EmptyOrIncomplete;
        }
        let published = head.wrapping_sub(tail) as usize;
        if published > RING_CAPACITY {
            self.command.tail.store(head, Ordering::Release);
            return BatchPop::Malformed;
        }
        let begin = self.command_records[(tail as usize) % RING_CAPACITY];
        if begin[0] != CMD_BATCH_BEGIN {
            self.command
                .tail
                .store(tail.wrapping_add(1), Ordering::Release);
            return BatchPop::Malformed;
        }
        let body = begin[1] as usize;
        if body > RING_CAPACITY - 2 {
            self.command.tail.store(head, Ordering::Release);
            return BatchPop::Malformed;
        }
        let total = body + 2;
        if published < total {
            // The producer release-publishes `head` only after the complete
            // frame is written. A non-empty truncated publication is corrupt,
            // not an in-progress batch, and must not retain frame credit.
            self.command.tail.store(head, Ordering::Release);
            return BatchPop::Malformed;
        }
        let end = self.command_records[((tail as usize) + total - 1) % RING_CAPACITY];
        if end[0] != CMD_BATCH_END || end[1] != begin[2] {
            self.command
                .tail
                .store(tail.wrapping_add(total as u32), Ordering::Release);
            return BatchPop::Malformed;
        }
        let records = (1..=body)
            .map(|i| self.command_records[((tail as usize) + i) % RING_CAPACITY])
            .collect();
        self.command
            .tail
            .store(tail.wrapping_add(total as u32), Ordering::Release);
        BatchPop::Accepted(records)
    }

    pub fn publish(&mut self, data: &RenderData) {
        // Odd epochs denote an in-progress rewrite; the final release increment
        // publishes every projection word to lock-free JavaScript readers.
        self.projection_epoch.fetch_add(1, Ordering::AcqRel);
        for record in &mut self.projection {
            *record = ProjectionRecord::EMPTY;
        }
        for (handle, instance) in data.instances_with_handles() {
            let Some(record) = self.projection.get_mut(handle.slot as usize) else {
                continue;
            };
            *record = ProjectionRecord {
                generation: handle.generation,
                status: 1,
                geometry_slot: instance.geometry.slot,
                geometry_generation: instance.geometry.generation,
                pipeline_key: instance.pipeline_key,
                render_flags: instance.render_flags,
                transform: *instance.transform.as_array(),
            };
        }
        self.projection_epoch.fetch_add(1, Ordering::Release);
    }

    pub fn complete(&mut self, request: u32, status: u32, handle: Option<InstanceHandle>) -> bool {
        if request == 0 {
            return true;
        }
        let head = self.completion.head.load(Ordering::Relaxed);
        let tail = self.completion.tail.load(Ordering::Acquire);
        if head.wrapping_sub(tail) >= RING_CAPACITY as u32 {
            return false;
        }
        let mut record = [0; RECORD_WORDS];
        record[0] = request;
        record[1] = status;
        if let Some(handle) = handle {
            record[2] = handle.slot;
            record[3] = handle.generation;
        }
        self.completion_records[(head as usize) % RING_CAPACITY] = record;
        self.completion
            .head
            .store(head.wrapping_add(1), Ordering::Release);
        true
    }

    pub fn completion_available(&self) -> usize {
        let head = self.completion.head.load(Ordering::Relaxed);
        let tail = self.completion.tail.load(Ordering::Acquire);
        RING_CAPACITY - head.wrapping_sub(tail) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncated_publication_is_discarded() {
        let mut abi = SharedAbi::new();
        abi.command_records[0][0] = CMD_BATCH_BEGIN;
        abi.command_records[0][1] = 1;
        abi.command.head.store(2, Ordering::Release);
        assert!(matches!(abi.pop_batch(), BatchPop::Malformed));
        assert_eq!(abi.command.tail.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn complete_batch_wraps_and_is_atomic() {
        let mut abi = SharedAbi::new();
        let start = RING_CAPACITY as u32 - 1;
        abi.command.tail.store(start, Ordering::Relaxed);
        abi.command_records[RING_CAPACITY - 1] = [
            CMD_BATCH_BEGIN,
            1,
            7,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ];
        abi.command_records[0][0] = CMD_VISIBLE;
        abi.command_records[1] = [
            CMD_BATCH_END,
            7,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ];
        abi.command.head.store(start + 3, Ordering::Release);
        let BatchPop::Accepted(records) = abi.pop_batch() else {
            panic!("complete batch was not accepted");
        };
        assert_eq!(records[0][0], CMD_VISIBLE);
    }
}
