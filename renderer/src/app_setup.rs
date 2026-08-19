#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::sync::mpsc;

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::command_ring::CommandRing;
use crate::platform::web::worker;
use crate::renderer::ResizeMessage;

thread_local! {
    static WORKER_EVENTS: RefCell<Option<mpsc::Sender<ResizeMessage>>> = const { RefCell::new(None) };
}

/// Deliver a low-frequency browser event to the worker-owned renderer channel.
#[wasm_bindgen]
pub fn worker_window_event(kind: u32, values: js_sys::Float64Array) {
    let values = values.to_vec();
    let value = |index: usize| values.get(index).copied().unwrap_or_default();
    let event = match kind {
        0 => ResizeMessage {
            width: value(0),
            height: value(1),
            scale_factor: value(2),
        },
        _ => return,
    };
    WORKER_EVENTS.with(|sender| {
        if let Some(sender) = sender.borrow().as_ref() {
            let _ = sender.send(event);
        }
    });
}

/// Start the typed renderer and return its SAB command-ring pointer.
pub fn worker_entrypoint() -> u32 {
    let (sender, events) = mpsc::channel();
    // The render worker owns this allocation for its entire lifetime. Publishing a
    // stable address lets every connected thread use the same shared command ring.
    let ring: &'static CommandRing = Box::leak(CommandRing::new());
    let ring_ptr = ring.ptr();
    WORKER_EVENTS.with(|worker_events| *worker_events.borrow_mut() = Some(sender));
    spawn_local(async move {
        worker::run_render_loop(events, ring).await;
    });
    ring_ptr
}
