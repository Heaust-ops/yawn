pub mod app_setup;
pub mod camera;
pub mod command_ring;
pub mod gltf;
pub mod message;
pub mod platform;
pub mod render_data;
pub mod render_graph;
pub mod renderer;
pub mod shared_snapshot;

#[cfg(target_arch = "wasm32")]
thread_local! { static PAYLOADS: std::cell::RefCell<std::collections::HashMap<u32, Vec<u8>>> = Default::default(); }

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn stage_payload(id: u32, bytes: js_sys::Uint8Array) {
    PAYLOADS.with(|payloads| {
        payloads.borrow_mut().insert(id, bytes.to_vec());
    });
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn discard_payload(id: u32) {
    PAYLOADS.with(|payloads| {
        payloads.borrow_mut().remove(&id);
    });
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn clear_payloads() {
    PAYLOADS.with(|payloads| payloads.borrow_mut().clear());
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn take_payload(id: u32) -> Option<Vec<u8>> {
    PAYLOADS.with(|payloads| payloads.borrow_mut().remove(&id))
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn take_payload(_id: u32) -> Option<Vec<u8>> {
    None
}

/// Worker entrypoint helper - executes the closure it is spawned with
/// Applications should export this with #[wasm_bindgen]
pub fn worker_entrypoint_impl(ptr: u32) {
    let work = unsafe { Box::from_raw(ptr as *mut Box<dyn FnOnce()>) };
    (*work)();
}

/// Macro to export the worker_entrypoint function in application crates
///
/// Usage:
/// ```rust
/// use renderer::export_worker_entrypoint;
/// export_worker_entrypoint!();
/// ```
#[macro_export]
macro_rules! export_worker_entrypoint {
    () => {
        #[wasm_bindgen::prelude::wasm_bindgen]
        pub fn worker_entrypoint(ptr: u32) {
            $crate::worker_entrypoint_impl(ptr);
        }
    };
}
