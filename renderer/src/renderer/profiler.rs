use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};
use wasm_bindgen::JsValue;

// A frame can contain every logical execution as a singleton physical pass and
// one instance-traversal compute pass.
pub const MAX_PROFILE_PASSES: usize = crate::render_graph::MAX_EXECUTIONS + 1;
#[cfg(any(target_arch = "wasm32", test))]
const SLOT_COUNT: usize = 4;
#[cfg(any(target_arch = "wasm32", test))]
const QUERY_COUNT: u32 = (MAX_PROFILE_PASSES * 2) as u32;
#[cfg(any(target_arch = "wasm32", test))]
const USED_RESOLVE_SIZE: u64 = QUERY_COUNT as u64 * 8;
const RESOLVE_SIZE: u64 = USED_RESOLVE_SIZE.next_multiple_of(wgpu::QUERY_RESOLVE_BUFFER_ALIGNMENT);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SlotState {
    Free,
    Encoding,
    Mapping,
}

struct Slot {
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    read: wgpu::Buffer,
    state: SlotState,
}
struct Completion {
    slot: usize,
    epoch: u64,
    ids: Vec<String>,
    values: Option<Vec<u64>>,
}

pub(crate) struct ProfileFrame {
    pub query_set: wgpu::QuerySet,
    slot: usize,
    identity: String,
    ids: Vec<String>,
    invalid: bool,
}
pub(crate) struct ProfileMap {
    slot: usize,
    epoch: u64,
    ids: Vec<String>,
    count: u32,
}
impl ProfileFrame {
    fn allocate(&mut self, id: &str) -> Option<u32> {
        allocate_id(&mut self.ids, &mut self.invalid, id)
    }
    pub fn render_writes(&mut self, id: &str) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        let first = self.allocate(id)?;
        Some(wgpu::RenderPassTimestampWrites {
            query_set: &self.query_set,
            beginning_of_pass_write_index: Some(first),
            end_of_pass_write_index: Some(first + 1),
        })
    }
    pub fn compute_writes(&mut self, id: &str) -> Option<wgpu::ComputePassTimestampWrites<'_>> {
        let first = self.allocate(id)?;
        Some(wgpu::ComputePassTimestampWrites {
            query_set: &self.query_set,
            beginning_of_pass_write_index: Some(first),
            end_of_pass_write_index: Some(first + 1),
        })
    }
}

pub(crate) struct Profiler {
    enabled: bool,
    available: bool,
    slots: Vec<Slot>,
    completions: Arc<Mutex<Vec<Completion>>>,
    epoch: u64,
    identity: String,
    period_ns: f64,
    samples: HashMap<String, VecDeque<(f64, f64)>>,
    last_snapshot_ms: f64,
    dropped: u64,
}

impl Profiler {
    #[cfg(any(target_arch = "wasm32", test))]
    pub fn requested_features(requested: bool, supported: wgpu::Features) -> wgpu::Features {
        if requested && supported.contains(wgpu::Features::TIMESTAMP_QUERY) {
            wgpu::Features::TIMESTAMP_QUERY
        } else {
            wgpu::Features::empty()
        }
    }
    #[cfg(target_arch = "wasm32")]
    pub async fn new(requested: bool, device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let available = requested && device.features().contains(wgpu::Features::TIMESTAMP_QUERY);
        let mut slots = Vec::new();
        if available {
            device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
            device.push_error_scope(wgpu::ErrorFilter::Validation);
            for _ in 0..SLOT_COUNT {
                let queries = device.create_query_set(&wgpu::QuerySetDescriptor {
                    label: Some("profile timestamps"),
                    ty: wgpu::QueryType::Timestamp,
                    count: QUERY_COUNT,
                });
                let resolve = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("profile resolve"),
                    size: RESOLVE_SIZE,
                    usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                });
                let read = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("profile readback"),
                    size: RESOLVE_SIZE,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                slots.push(Slot {
                    queries,
                    resolve,
                    read,
                    state: SlotState::Free,
                });
            }
        }
        let allocation_failed = if available {
            // Pop both scopes before yielding: WebGPU's scope stack must be unwound
            // synchronously, even though completion of each pop is asynchronous.
            let validation = device.pop_error_scope();
            let oom = device.pop_error_scope();
            let (validation, oom) = futures::join!(validation, oom);
            validation.is_some() || oom.is_some()
        } else {
            false
        };
        if allocation_failed {
            slots.clear();
        }
        Self {
            enabled: requested,
            available: available && !allocation_failed,
            slots,
            completions: Default::default(),
            epoch: 0,
            identity: String::new(),
            period_ns: queue.get_timestamp_period() as f64,
            samples: Default::default(),
            last_snapshot_ms: 0.0,
            dropped: 0,
        }
    }
    pub fn begin(&mut self, identity: impl FnOnce() -> String) -> Option<ProfileFrame> {
        self.drain();
        let Some((slot, identity)) = profile_gate(self.enabled, self.available, || {
            begin_transition(self.slots.iter_mut().map(|slot| &mut slot.state))
                .map(|slot| (slot, identity()))
        }) else {
            if self.enabled && self.available {
                self.dropped += 1;
            }
            return None;
        };
        Some(ProfileFrame {
            query_set: self.slots[slot].queries.clone(),
            slot,
            identity,
            ids: Vec::new(),
            invalid: false,
        })
    }
    pub fn cancel(&mut self, frame: ProfileFrame) {
        cancel_state(&mut self.slots[frame.slot].state);
    }
    pub fn finish(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        frame: ProfileFrame,
    ) -> Option<ProfileMap> {
        let count = match finish_transition(
            &mut self.slots[frame.slot].state,
            frame.invalid,
            frame.ids.len(),
        ) {
            FinishAction::Cancel => return None,
            FinishAction::Resolve(count) => count,
        };
        if frame.identity != self.identity {
            self.identity = frame.identity;
            self.epoch = self.epoch.wrapping_add(1);
            self.samples.clear();
        }
        let slot = frame.slot;
        let epoch = self.epoch;
        let ids = frame.ids;
        encoder.resolve_query_set(
            &self.slots[slot].queries,
            0..count,
            &self.slots[slot].resolve,
            0,
        );
        encoder.copy_buffer_to_buffer(
            &self.slots[slot].resolve,
            0,
            &self.slots[slot].read,
            0,
            count as u64 * 8,
        );
        Some(ProfileMap {
            slot,
            epoch,
            ids,
            count,
        })
    }
    pub fn map(&mut self, request: ProfileMap) {
        let ProfileMap {
            slot,
            epoch,
            ids,
            count,
        } = request;
        self.slots[slot].state = SlotState::Mapping;
        let buffer = self.slots[slot].read.clone();
        let completions = self.completions.clone();
        buffer
            .clone()
            .slice(..count as u64 * 8)
            .map_async(wgpu::MapMode::Read, move |result| {
                let values = result.ok().map(|_| {
                    let bytes = buffer.slice(..count as u64 * 8).get_mapped_range();
                    let values = bytes
                        .chunks_exact(8)
                        .map(|x| u64::from_le_bytes(x.try_into().unwrap()))
                        .collect();
                    drop(bytes);
                    buffer.unmap();
                    values
                });
                if let Ok(mut completions) = completions.lock() {
                    completions.push(Completion {
                        slot,
                        epoch,
                        ids,
                        values,
                    });
                }
            });
    }
    fn drain(&mut self) {
        let completions = if let Ok(mut queue) = self.completions.lock() {
            queue.drain(..).collect::<Vec<_>>()
        } else {
            return;
        };
        for c in completions {
            let Some(v) = completion_transition(
                &mut self.slots[c.slot].state,
                c.values,
                c.epoch,
                self.epoch,
                &mut self.available,
                &mut self.samples,
            ) else {
                continue;
            };
            for (id, pair) in c.ids.into_iter().zip(v.chunks_exact(2)) {
                if let Some(ms) = validate(pair[0], pair[1], self.period_ns) {
                    let q = self.samples.entry(id).or_default();
                    q.push_back((js_sys::Date::now(), ms));
                }
            }
        }
    }
    pub fn snapshot_json(&mut self, now: f64) -> Option<JsValue> {
        self.drain();
        profile_gate(self.enabled, self.available, || Some(()))?;
        (now - self.last_snapshot_ms >= 250.0).then_some(())?;
        self.last_snapshot_ms = now;
        let cutoff = now - 1000.0;
        let mut passes = serde_json::Map::new();
        for (id, q) in &mut self.samples {
            while q.front().is_some_and(|x| x.0 < cutoff) {
                q.pop_front();
            }
            if !q.is_empty() {
                passes.insert(
                    id.clone(),
                    serde_json::json!(q.iter().map(|x| x.1).sum::<f64>() / q.len() as f64),
                );
            }
        }
        let value = serde_json::json!({"type":"profile-snapshot","requested":self.enabled,"available":self.available,"epoch":self.epoch,"graph":self.identity,"passes":passes,"dropped":self.dropped});
        js_sys::JSON::parse(&value.to_string()).ok()
    }
}
fn allocate_id(ids: &mut Vec<String>, invalid: &mut bool, id: &str) -> Option<u32> {
    if ids.len() >= MAX_PROFILE_PASSES {
        *invalid = true;
        return None;
    }
    let first = ids.len() as u32 * 2;
    ids.push(id.to_owned());
    Some(first)
}

fn profile_gate<T>(enabled: bool, available: bool, f: impl FnOnce() -> Option<T>) -> Option<T> {
    (enabled && available).then(f).flatten()
}

fn begin_transition<'a>(states: impl IntoIterator<Item = &'a mut SlotState>) -> Option<usize> {
    for (slot, state) in states.into_iter().enumerate() {
        if *state == SlotState::Free {
            *state = SlotState::Encoding;
            return Some(slot);
        }
    }
    None
}

#[derive(Debug, PartialEq, Eq)]
enum FinishAction {
    Cancel,
    Resolve(u32),
}

fn finish_transition(state: &mut SlotState, invalid: bool, id_count: usize) -> FinishAction {
    if *state != SlotState::Encoding || invalid || id_count == 0 {
        cancel_state(state);
        FinishAction::Cancel
    } else {
        FinishAction::Resolve(id_count as u32 * 2)
    }
}

fn completion_transition(
    state: &mut SlotState,
    values: Option<Vec<u64>>,
    completion_epoch: u64,
    current_epoch: u64,
    available: &mut bool,
    samples: &mut HashMap<String, VecDeque<(f64, f64)>>,
) -> Option<Vec<u64>> {
    *state = SlotState::Free;
    match values {
        None => {
            *available = false;
            samples.clear();
            None
        }
        Some(_) if !*available || completion_epoch != current_epoch => None,
        Some(values) => Some(values),
    }
}

fn cancel_state(state: &mut SlotState) {
    if *state == SlotState::Encoding {
        *state = SlotState::Free;
    }
}
fn validate(start: u64, end: u64, period: f64) -> Option<f64> {
    if (start == 0 && end == 0) || end < start || !period.is_finite() || period <= 0.0 {
        return None;
    }
    let ms = (end - start) as f64 * period / 1_000_000.0;
    if ms.is_finite() && ms >= 0.0 && ms <= 1000.0 {
        Some(ms)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn feature_gate() {
        assert!(Profiler::requested_features(false, wgpu::Features::TIMESTAMP_QUERY).is_empty());
        assert!(Profiler::requested_features(true, wgpu::Features::empty()).is_empty());
        assert_eq!(
            Profiler::requested_features(true, wgpu::Features::TIMESTAMP_QUERY),
            wgpu::Features::TIMESTAMP_QUERY
        )
    }
    #[test]
    fn validation() {
        assert_eq!(validate(1, 2, 1_000_000.0), Some(1.0));
        assert_eq!(validate(2, 2, 1.0), Some(0.0));
        assert_eq!(validate(0, 0, 1.0), None);
        assert_eq!(validate(2, 1, 1.0), None);
        assert_eq!(validate(0, 1, 1_000_000_000.0), Some(1000.0));
        assert_eq!(validate(0, 2, 1_000_000_000.0), None);
        assert_eq!(validate(1, 2, f64::NAN), None);
        assert_eq!(validate(1, 2, f64::INFINITY), None);
        assert_eq!(validate(1, 2, 0.0), None);
        assert_eq!(validate(1, 2, -1.0), None);
    }
    #[test]
    fn capacity_is_aligned() {
        assert_eq!(RESOLVE_SIZE % wgpu::QUERY_RESOLVE_BUFFER_ALIGNMENT, 0);
        assert!(RESOLVE_SIZE >= USED_RESOLVE_SIZE);
        assert!(RESOLVE_SIZE - USED_RESOLVE_SIZE < wgpu::QUERY_RESOLVE_BUFFER_ALIGNMENT);
        assert_eq!(QUERY_COUNT as usize, MAX_PROFILE_PASSES * 2)
    }
    #[test]
    fn lazy_identity_when_disabled_unavailable_or_full() {
        let mut calls = 0;
        for (enabled, available) in [(false, true), (true, false)] {
            assert_eq!(
                profile_gate(enabled, available, || {
                    calls += 1;
                    Some(7)
                }),
                None
            );
        }
        assert_eq!(calls, 0);
        let mut full = [SlotState::Mapping; SLOT_COUNT];
        assert_eq!(
            profile_gate(true, true, || {
                begin_transition(&mut full).map(|slot| {
                    calls += 1;
                    slot
                })
            }),
            None
        );
        assert_eq!(calls, 0);
        assert_eq!(full, [SlotState::Mapping; SLOT_COUNT]);
        let mut states = [SlotState::Mapping, SlotState::Free];
        assert_eq!(
            profile_gate(true, true, || {
                begin_transition(&mut states).map(|slot| {
                    calls += 1;
                    slot
                })
            }),
            Some(1)
        );
        assert_eq!(calls, 1);
        assert_eq!(states, [SlotState::Mapping, SlotState::Encoding]);
    }
    #[test]
    fn compact_ids_and_query_pairs_resolve_four() {
        let mut ids = Vec::new();
        let mut invalid = false;
        assert_eq!(allocate_id(&mut ids, &mut invalid, "a"), Some(0));
        assert_eq!(allocate_id(&mut ids, &mut invalid, "b"), Some(2));
        assert_eq!(ids, ["a", "b"]);
        let mut state = SlotState::Encoding;
        assert_eq!(
            finish_transition(&mut state, invalid, ids.len()),
            FinishAction::Resolve(4)
        );
        assert!(!invalid);
    }
    #[test]
    fn capacity_overflow_marks_frame_invalid() {
        let mut ids = (0..MAX_PROFILE_PASSES).map(|i| i.to_string()).collect();
        let mut invalid = false;
        assert_eq!(allocate_id(&mut ids, &mut invalid, "overflow"), None);
        assert!(invalid);
        assert_eq!(ids.len(), MAX_PROFILE_PASSES);
        let mut overflow = SlotState::Encoding;
        assert_eq!(
            finish_transition(&mut overflow, invalid, ids.len()),
            FinishAction::Cancel
        );
        assert_eq!(overflow, SlotState::Free);
        let mut empty = SlotState::Encoding;
        assert_eq!(
            finish_transition(&mut empty, false, 0),
            FinishAction::Cancel
        );
        assert_eq!(empty, SlotState::Free);
    }
    #[test]
    fn repeated_cancel_and_mapping_guard() {
        let mut encoding = SlotState::Encoding;
        for _ in 0..=SLOT_COUNT {
            cancel_state(&mut encoding);
            assert_eq!(encoding, SlotState::Free);
            encoding = SlotState::Encoding;
        }
        let mut mapping = SlotState::Mapping;
        cancel_state(&mut mapping);
        assert_eq!(mapping, SlotState::Mapping);
        let mut free = SlotState::Free;
        cancel_state(&mut free);
        assert_eq!(free, SlotState::Free);
    }
    #[test]
    fn stale_epoch_map_failure_is_terminal_and_prevents_later_aggregation() {
        let mut state = SlotState::Mapping;
        let mut available = true;
        let mut samples = HashMap::from([
            ("a".into(), VecDeque::from([(1.0, 2.0)])),
            ("b".into(), VecDeque::from([(3.0, 4.0)])),
        ]);
        assert_eq!(
            completion_transition(&mut state, None, 1, 2, &mut available, &mut samples),
            None
        );
        assert_eq!(state, SlotState::Free);
        assert!(!available);
        assert!(samples.is_empty());
        for epoch in [2, 1] {
            state = SlotState::Mapping;
            assert_eq!(
                completion_transition(
                    &mut state,
                    Some(vec![1, 2]),
                    epoch,
                    2,
                    &mut available,
                    &mut samples
                ),
                None
            );
            assert_eq!(state, SlotState::Free);
            assert!(samples.is_empty());
        }
    }
    #[test]
    fn snapshot_gate_is_silent_when_disabled_or_unavailable() {
        for enabled in [false, true] {
            for available in [false, true] {
                assert_eq!(
                    profile_gate(enabled, available, || Some("snapshot")),
                    (enabled && available).then_some("snapshot")
                );
            }
        }
    }
}
