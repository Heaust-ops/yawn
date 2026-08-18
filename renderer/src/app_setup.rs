#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::sync::mpsc;

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::command_ring::CommandRing;
use crate::message::{MouseMessage, ResizeMessage, WheelMessage, WindowEvent};
use crate::platform::web::worker;

thread_local! {
    static WORKER_EVENTS: RefCell<Option<mpsc::Sender<WindowEvent>>> = const { RefCell::new(None) };
}

/// Deliver a low-frequency browser event to the worker-owned renderer channel.
#[wasm_bindgen]
pub fn worker_window_event(kind: u32, values: js_sys::Float64Array) {
    let values = values.to_vec();
    let value = |index: usize| values.get(index).copied().unwrap_or_default();
    let event = match kind {
        0 => WindowEvent::Resize(ResizeMessage {
            width: value(0),
            height: value(1),
            scale_factor: value(2),
        }),
        1 | 2 => {
            let message = MouseMessage {
                scale_factor: value(0),
                buttons: value(1) as u16,
                movement_x: value(2),
                movement_y: value(3),
                offset_x: value(4),
                offset_y: value(5),
                viewport_height: value(6),
            };
            if kind == 1 {
                WindowEvent::PointerMove(message)
            } else {
                WindowEvent::PointerClick(message)
            }
        }
        3 => WindowEvent::PointerWheel(WheelMessage {
            delta_y_pixels: value(0) as f32,
        }),
        _ => return,
    };
    WORKER_EVENTS.with(|sender| {
        if let Some(sender) = sender.borrow().as_ref() {
            let _ = sender.send(event);
        }
    });
}

/// Start the typed renderer and return its SAB command-ring pointer.
pub fn worker_entrypoint<T: crate::renderer::scene::Scene + 'static>(profile: bool) -> u32 {
    let (sender, events) = mpsc::channel();
    // The render worker owns this allocation for its entire lifetime. Publishing a
    // stable address lets every connected thread use the same shared command ring.
    let ring: &'static CommandRing = Box::leak(CommandRing::new());
    let ring_ptr = ring.ptr();
    WORKER_EVENTS.with(|worker_events| *worker_events.borrow_mut() = Some(sender));
    spawn_local(async move {
        worker::run_render_loop::<T>(events, ring, profile).await;
    });
    ring_ptr
}
