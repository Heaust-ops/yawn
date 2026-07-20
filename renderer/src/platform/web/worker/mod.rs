use crate::message::WindowEvent;
use log::info;
use std::sync::mpsc::Receiver;
use std::{cell::RefCell, fmt::Debug, ops::Deref, rc::Rc};
use wasm_bindgen::{prelude::*, JsValue};

/// Binds JS.
#[wasm_bindgen(module = "/src/platform/web/worker/workerGen.js")]
extern "C" {
    /// Spawn new worker in JS side in order to make bundler know about dependency.
    #[wasm_bindgen(catch, js_name = "createWorker")]
    fn create_worker(kind: &str, name: &str) -> Result<web_sys::Worker, JsValue>;
}

/// Binds JS.
/// This makes wasm-bindgen bring `mainWorker.js` to the `pkg` directory.
/// So that bundler can bundle it together.
#[wasm_bindgen(module = "/src/platform/web/worker/mainWorker.js")]
extern "C" {
    /// Nothing to do.
    #[wasm_bindgen]
    fn attachMain();

    #[wasm_bindgen(js_name = "takeStartupCanvas")]
    fn take_startup_canvas() -> JsValue;
}

// Ensure wasm-bindgen copies the nested worker entry beside mainWorker.js so
// Vite can resolve `new URL("./boundsWorker.js", import.meta.url)`.
#[wasm_bindgen(module = "/src/platform/web/worker/boundsWorker.js")]
extern "C" {
    #[wasm_bindgen]
    fn attachBounds();
}

#[wasm_bindgen(module = "/src/platform/web/worker/bvhWorker.js")]
extern "C" {
    #[wasm_bindgen]
    fn attachBvh();
}

pub struct MainWorker {
    handle: web_sys::Worker,
    name: String,
    _callback: Closure<dyn FnMut(web_sys::Event)>,
}

impl Drop for MainWorker {
    /// Terminates web worker *immediately*.
    fn drop(&mut self) {
        self.handle.terminate();
        info!("Worker({}) was terminated", &self.name);
    }
}

impl Debug for MainWorker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MainWorker")
            .field("handle", &self.handle)
            .field("name", &self.name)
            .finish()
    }
}

impl MainWorker {
    fn notify_main_startup_error(error: &JsValue) {
        let global = js_sys::global();
        if let Ok(callback) = js_sys::Reflect::get(&global, &"rendererStartupFailed".into()) {
            if callback.is_function() {
                let _ = callback
                    .unchecked_into::<js_sys::Function>()
                    .call1(&global, error);
            }
        }
    }

    pub fn create(name: &str) -> Result<Self, JsValue> {
        let handle = create_worker("main", name).inspect_err(Self::notify_main_startup_error)?;
        let callback = Closure::new(|ev: web_sys::Event| {
            let ev: web_sys::MessageEvent = ev.unchecked_into();
            let data = ev.data();
            if js_sys::Reflect::get(&data, &"type".into())
                .is_ok_and(|kind| kind == "renderer-abi-ready")
            {
                let global = js_sys::global();
                if let Ok(callback) = js_sys::Reflect::get(&global, &"rendererAbiReady".into()) {
                    if callback.is_function() {
                        let _ = callback
                            .unchecked_into::<js_sys::Function>()
                            .call1(&global, &data);
                    }
                }
            }
        });
        handle.set_onmessage(Some(callback.as_ref().unchecked_ref()));
        Ok(Self {
            handle,
            name: name.to_owned(),
            _callback: callback,
        })
    }

    fn report_startup_error(message: &str) {
        let global = js_sys::global().unchecked_into::<web_sys::DedicatedWorkerGlobalScope>();
        let value = js_sys::Object::new();
        let result = js_sys::Reflect::set(&value, &"type".into(), &"renderer-startup-error".into())
            .and_then(|_| js_sys::Reflect::set(&value, &"message".into(), &message.into()))
            .and_then(|_| global.post_message(&value));
        if let Err(error) = result {
            log::error!("failed to report terminal renderer startup error: {error:?}");
        }
    }

    /// Spawns main worker from the window context.
    pub fn start(
        &self,
        id: usize,
        canvas: web_sys::OffscreenCanvas,
        f: impl FnOnce() + Send + 'static,
    ) -> Result<(), JsValue> {
        let handle = &self.handle;

        // Double-boxing because `dyn FnOnce` is unsized and so `Box<dyn FnOnce()>` has
        // an undefined layout (although I think in practice its a pointer and a length?).
        let ptr = Box::into_raw(Box::new(Box::new(f) as Box<dyn FnOnce()>));

        let msg = js_sys::Object::new();
        js_sys::Reflect::set(&msg, &"type".into(), &"renderer-start".into())?;
        js_sys::Reflect::set(&msg, &"protocolVersion".into(), &1.into())?;
        js_sys::Reflect::set(&msg, &"module".into(), &wasm_bindgen::module())?;
        js_sys::Reflect::set(&msg, &"workerId".into(), &id.into())?;
        js_sys::Reflect::set(&msg, &"memory".into(), &wasm_bindgen::memory())?;
        js_sys::Reflect::set(&msg, &"entryPtr".into(), &JsValue::from(ptr as u32))?;
        js_sys::Reflect::set(&msg, &"canvas".into(), &canvas)?;

        info!("posting message");
        let transfer = js_sys::Array::new();
        transfer.push(&canvas);
        if let Err(error) = handle.post_message_with_transfer(&msg, &transfer) {
            // Posting failed synchronously, so ownership never reached the worker.
            unsafe {
                drop(Box::from_raw(ptr));
            }
            Self::notify_main_startup_error(&error);
            return Err(error);
        }
        // Do not expose a worker that has not accepted its typed startup envelope.
        let global = js_sys::global();
        if let Ok(callback) = js_sys::Reflect::get(&global, &"rendererWorkerReady".into()) {
            if callback.is_function() {
                let _ = callback
                    .unchecked_into::<js_sys::Function>()
                    .call1(&global, handle);
            }
        }
        Ok(())
    }

    pub async fn run_render_loop<T: crate::renderer::scene::Scene + 'static>(
        events_chan: Receiver<WindowEvent>,
    ) {
        use crate::renderer::Renderer;

        let canvas = match take_startup_canvas().dyn_into::<web_sys::OffscreenCanvas>() {
            Ok(canvas) => canvas,
            Err(_) => {
                Self::report_startup_error("failed to receive startup canvas");
                return;
            }
        };

        let renderer = match Renderer::<T>::new(canvas, events_chan).await {
            Ok(renderer) => Rc::new(RefCell::new(renderer)),
            Err(error) => {
                log::error!("failed to initialize renderer: {error}");
                Self::report_startup_error(&error);
                return;
            }
        };
        Renderer::run_render_loop(renderer);
    }
}

impl Deref for MainWorker {
    type Target = web_sys::Worker;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.handle
    }
}
