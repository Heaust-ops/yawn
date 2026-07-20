//! Single-writer shared-memory mailbox for geometry-local bounds.
//!
//! Position jobs use transferred copies: the copy is the immutable snapshot
//! lease for this phase and never aliases a Rust `Vec` allocation.

use std::sync::atomic::{AtomicU32, Ordering};

use js_sys::{Array, Float32Array, Object, Reflect, Uint32Array};
use wasm_bindgen::JsValue;

use crate::render_data::{BoundsIdentity, BoundsResult, BoundsState, RenderData};

#[wasm_bindgen::prelude::wasm_bindgen(module = "/src/platform/web/worker/mainWorker.js")]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(catch, js_name = routeRendererMessage)]
    fn route_renderer_message(data: &JsValue, transfer: &Array) -> Result<(), JsValue>;
}

pub const MAGIC: u32 = 0x424e_4453; // BNDS
pub const VERSION: u32 = 1;
pub const HEADER_WORDS: usize = 8;
pub const WORDS_PER_SLOT: usize = 12;
pub const DEFAULT_CAPACITY: usize = 1 << 20;
const MAX_JOB_BYTES: usize = 64 * 1024 * 1024;
const MAX_IN_FLIGHT_BYTES: usize = 128 * 1024 * 1024;
// During dispatch the canonical Rust positions, the transferred snapshot, and
// the main-worker retry snapshot can coexist.
// Canonical source, Rust snapshot, transferred typed array, and JS retry copy.
const RETAINED_COPY_MULTIPLIER: usize = 4;

/// SAB layout (u32 words): header `[magic, version, capacity, words_per_slot,
/// descriptor_epoch, writer_count, reserved, reserved]`, followed by SoA
/// columns: sequence, generation, content_version, state, job_id, snapshot_id,
/// min_x/y/z, max_x/y/z. The bounds worker is the sole writer after creation.
pub struct BoundsMailbox {
    words: Box<[AtomicU32]>,
    capacity: usize,
    next_job_id: u32,
    last_sequences: Vec<u32>,
    in_flight: Vec<(u32, usize, usize)>,
}

impl BoundsMailbox {
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1).min(DEFAULT_CAPACITY);
        let words: Box<[AtomicU32]> = (0..HEADER_WORDS + capacity * WORDS_PER_SLOT)
            .map(|_| AtomicU32::new(0))
            .collect();
        words[0].store(MAGIC, Ordering::Relaxed);
        words[1].store(VERSION, Ordering::Relaxed);
        words[2].store(capacity as u32, Ordering::Relaxed);
        words[3].store(WORDS_PER_SLOT as u32, Ordering::Relaxed);
        words[4].store(1, Ordering::Relaxed);
        words[5].store(1, Ordering::Relaxed);
        Self {
            words,
            capacity,
            next_job_id: 1,
            last_sequences: vec![0; capacity],
            in_flight: Vec::with_capacity(1),
        }
    }

    fn column(&self, column: usize, slot: usize) -> usize {
        HEADER_WORDS + column * self.capacity + slot
    }

    pub fn descriptor(&self) -> [u32; 7] {
        [
            MAGIC,
            VERSION,
            self.words.as_ptr() as u32,
            self.capacity as u32,
            HEADER_WORDS as u32,
            WORDS_PER_SLOT as u32,
            1,
        ]
    }

    pub fn announce(&self) {
        let message = Object::new();
        let result = Reflect::set(&message, &"type".into(), &"bounds-init".into())
            .and_then(|_| {
                Reflect::set(
                    &message,
                    &"descriptor".into(),
                    &Uint32Array::from(self.descriptor().as_slice()),
                )
            })
            .and_then(|_| {
                // Pass the shared memory explicitly. Vite may instantiate this routing
                // module separately from the worker entry module, so module-local JS
                // state is not a reliable source of the Wasm memory object.
                Reflect::set(&message, &"memory".into(), &wasm_bindgen::memory())
            })
            .and_then(|_| route_renderer_message(&message, &Array::new()));
        if let Err(error) = result {
            log::error!("failed to announce bounds mailbox: {:?}", error);
        }
    }

    pub fn dispatch_all(&mut self, data: &mut RenderData) {
        // Serialize jobs so completion frees every retained transfer/retry copy
        // before another allocation is admitted.
        if !self.in_flight.is_empty() {
            return;
        }
        let candidate = data.pending_bounds_jobs().find_map(|(identity, positions)| {
            let slot = identity.slot as usize;
            if slot >= self.capacity {
                log::error!("bounds capacity {} exceeded by geometry slot {}; conservative visibility retained", self.capacity, slot);
                return None;
            }
            let float_count = positions.len().checked_mul(3)?;
            let _ = u32::try_from(float_count).ok()?;
            let bytes = positions.len().checked_mul(std::mem::size_of::<[f32; 3]>())?;
            let reserved = bytes.checked_mul(RETAINED_COPY_MULTIPLIER)?;
            if bytes > MAX_JOB_BYTES || reserved > MAX_IN_FLIGHT_BYTES { return None; }
            Some((identity, positions.to_vec(), bytes, float_count as u32, slot))
        });
        if let Some((identity, positions, bytes, float_count, slot)) = candidate {
            let job_id = self.next_job_id;
            self.next_job_id = self.next_job_id.wrapping_add(1).max(1);
            let snapshot = Float32Array::new_with_length(float_count);
            for (index, point) in positions.iter().flatten().enumerate() {
                snapshot.set_index(index as u32, *point);
            }
            let message = Object::new();
            let mut result = Reflect::set(&message, &"type".into(), &"bounds-job".into());
            for (name, value) in [
                ("slot", identity.slot),
                ("generation", identity.generation),
                ("contentVersion", identity.content_version),
                ("snapshotId", identity.snapshot_id),
                ("jobId", job_id),
            ] {
                result = result.and_then(|_| {
                    Reflect::set(&message, &JsValue::from_str(name), &JsValue::from(value))
                });
            }
            result = result.and_then(|_| {
                Reflect::set(&message, &"positions".into(), snapshot.buffer().as_ref())
            });
            let transfer = Array::new();
            transfer.push(snapshot.buffer().as_ref());
            result = result.and_then(|_| route_renderer_message(&message, &transfer).map(|_| true));
            match result {
                Ok(_) => {
                    data.mark_bounds_dispatched(identity, job_id);
                    self.in_flight.push((job_id, slot, bytes));
                }
                Err(error) => {
                    log::error!("failed to dispatch bounds job {}: {:?}", job_id, error)
                }
            }
        }
    }

    pub fn poll(&mut self, data: &mut RenderData) -> bool {
        let mut changed = false;
        let jobs = self.in_flight.clone();
        for (expected_job_id, slot, _) in jobs {
            let seq_index = self.column(0, slot);
            let before = self.words[seq_index].load(Ordering::Acquire);
            if before == 0 || before & 1 != 0 || before == self.last_sequences[slot] {
                continue;
            }
            let read = |column| self.words[self.column(column, slot)].load(Ordering::Relaxed);
            let result = BoundsResult {
                identity: BoundsIdentity {
                    slot: slot as u32,
                    generation: read(1),
                    content_version: read(2),
                    snapshot_id: read(5),
                },
                job_id: read(4),
                state: BoundsState::from_u32(read(3)),
                bounds: crate::render_data::Aabb {
                    min: [
                        f32::from_bits(read(6)),
                        f32::from_bits(read(7)),
                        f32::from_bits(read(8)),
                    ],
                    max: [
                        f32::from_bits(read(9)),
                        f32::from_bits(read(10)),
                        f32::from_bits(read(11)),
                    ],
                },
            };
            let after = self.words[seq_index].load(Ordering::Acquire);
            if before == after && after & 1 == 0 && result.job_id == expected_job_id {
                self.last_sequences[slot] = after;
                self.in_flight
                    .retain(|(job_id, _, _)| *job_id != expected_job_id);
                changed |= data.accept_bounds(result);
            }
        }
        self.dispatch_all(data);
        changed
    }
}
