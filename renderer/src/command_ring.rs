//! Versioned, fixed-slot SPSC command transport in shared WebAssembly memory.
use std::sync::atomic::{AtomicU32, Ordering};

pub const MAGIC: u32 = u32::from_le_bytes(*b"YAWN");
pub const VERSION: u32 = 2;
pub const CAPACITY: usize = 1024;
pub const SLOT_WORDS: usize = 40;
pub const SLOT_BYTES: usize = 160;
pub const HEADER_BYTES: usize = 64;
pub const SLOT_VERSION: u32 = 2;
const STATE_OPEN: u32 = 0;
const STATE_CORRUPT: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RingError {
    Closed,
    Backlog,
    SlotVersion,
    ZeroRequest,
}

#[repr(C, align(64))]
pub struct CommandRing {
    header: [AtomicU32; 16],
    slots: [[AtomicU32; SLOT_WORDS]; CAPACITY],
}

impl CommandRing {
    pub fn new() -> Box<Self> {
        let ring = Box::new(Self {
            header: std::array::from_fn(|_| AtomicU32::new(0)),
            slots: std::array::from_fn(|_| std::array::from_fn(|_| AtomicU32::new(0))),
        });
        ring.header[0].store(MAGIC, Ordering::Relaxed);
        ring.header[1].store(VERSION, Ordering::Relaxed);
        ring.header[2].store(CAPACITY as u32, Ordering::Relaxed);
        ring.header[3].store(SLOT_WORDS as u32, Ordering::Relaxed);
        ring
    }

    pub fn ptr(&self) -> u32 {
        self as *const Self as usize as u32
    }

    /// Consumer-only. The producer publishes word zero (slot version) last, then write_index.
    pub fn drain(&self, mut visit: impl FnMut([u32; SLOT_WORDS])) -> Result<(), RingError> {
        if self.header[6].load(Ordering::Acquire) != STATE_OPEN {
            return Err(RingError::Closed);
        }
        let mut read = self.header[4].load(Ordering::Relaxed);
        let write = self.header[5].load(Ordering::Acquire);
        if write.wrapping_sub(read) > CAPACITY as u32 {
            self.header[6].store(STATE_CORRUPT, Ordering::Release);
            return Err(RingError::Backlog);
        }
        while read != write {
            let slot = &self.slots[read as usize % CAPACITY];
            let mut words = [0; SLOT_WORDS];
            for (out, word) in words.iter_mut().zip(slot) {
                *out = word.load(Ordering::Relaxed);
            }
            let error = if words[0] != SLOT_VERSION {
                Some(RingError::SlotVersion)
            } else if words[2] == 0 {
                Some(RingError::ZeroRequest)
            } else {
                None
            };
            if let Some(error) = error {
                self.header[6].store(STATE_CORRUPT, Ordering::Release);
                return Err(error);
            }
            visit(words);
            read = read.wrapping_add(1);
            self.header[4].store(read, Ordering::Release);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_layout() {
        assert_eq!(std::mem::size_of::<[AtomicU32; 16]>(), HEADER_BYTES);
        assert_eq!(std::mem::size_of::<[AtomicU32; SLOT_WORDS]>(), SLOT_BYTES);
        assert_eq!(
            std::mem::size_of::<CommandRing>(),
            HEADER_BYTES + CAPACITY * SLOT_BYTES
        );
        assert_eq!(std::mem::align_of::<CommandRing>(), 64);
    }
    #[test]
    fn tagged_header_and_fifo_drain() {
        let ring = CommandRing::new();
        assert_eq!(ring.header[0].load(Ordering::Relaxed), MAGIC);
        assert_eq!(ring.header[1].load(Ordering::Relaxed), VERSION);
        ring.slots[0][0].store(SLOT_VERSION, Ordering::Relaxed);
        ring.slots[0][1].store(7, Ordering::Relaxed);
        ring.slots[0][2].store(99, Ordering::Relaxed);
        ring.header[5].store(1, Ordering::Release);
        let mut seen = vec![];
        ring.drain(|w| seen.push((w[1], w[2]))).unwrap();
        assert_eq!(seen, [(7, 99)]);
        assert_eq!(ring.header[4].load(Ordering::Acquire), 1);
    }
    #[test]
    fn wraps_slots() {
        let ring = CommandRing::new();
        ring.header[4].store(CAPACITY as u32, Ordering::Relaxed);
        ring.slots[0][0].store(SLOT_VERSION, Ordering::Relaxed);
        ring.slots[0][1].store(3, Ordering::Relaxed);
        ring.slots[0][2].store(1, Ordering::Relaxed);
        ring.header[5].store(CAPACITY as u32 + 1, Ordering::Release);
        let mut opcode = 0;
        ring.drain(|w| opcode = w[1]).unwrap();
        assert_eq!(opcode, 3);
    }

    #[test]
    fn malformed_slot_fails_closed() {
        for (version, request, expected) in [
            (SLOT_VERSION + 1, 1, RingError::SlotVersion),
            (SLOT_VERSION, 0, RingError::ZeroRequest),
        ] {
            let ring = CommandRing::new();
            ring.slots[0][0].store(version, Ordering::Relaxed);
            ring.slots[0][2].store(request, Ordering::Relaxed);
            ring.header[5].store(1, Ordering::Release);
            assert_eq!(ring.drain(|_| {}), Err(expected));
            assert_eq!(ring.drain(|_| {}), Err(RingError::Closed));
        }
    }

    #[test]
    fn full_is_valid_but_overfull_is_corrupt() {
        let full = CommandRing::new();
        for slot in &full.slots {
            slot[0].store(SLOT_VERSION, Ordering::Relaxed);
            slot[2].store(1, Ordering::Relaxed);
        }
        full.header[5].store(CAPACITY as u32, Ordering::Release);
        let mut count = 0;
        full.drain(|_| count += 1).unwrap();
        assert_eq!(count, CAPACITY);

        let overfull = CommandRing::new();
        overfull.header[5].store(CAPACITY as u32 + 1, Ordering::Release);
        assert_eq!(overfull.drain(|_| {}), Err(RingError::Backlog));
        assert_eq!(overfull.drain(|_| {}), Err(RingError::Closed));
    }
}
