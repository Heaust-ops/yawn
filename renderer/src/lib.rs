pub mod app_setup;
pub mod command_ring;
pub mod platform;
pub mod render_data;
pub mod render_graph;
pub mod renderer;
pub mod shared_snapshot;
pub mod shared_soa;

/// Start the core renderer inside its owning worker.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn worker_main() -> u32 {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    wasm_logger::init(wasm_logger::Config::default());
    app_setup::worker_entrypoint()
}

/// Return this worker's shared WebAssembly memory to messaging clients.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn worker_memory() -> wasm_bindgen::JsValue {
    wasm_bindgen::memory()
}

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
