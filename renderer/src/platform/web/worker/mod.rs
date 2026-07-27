use crate::command_ring::CommandRing;
use crate::message::WindowEvent;
use log::info;
use std::sync::mpsc::Receiver;
use std::{cell::RefCell, fmt::Debug, ops::Deref, rc::Rc};
use wasm_bindgen::{prelude::*, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::MessageEvent;

/// Binds JS.
#[wasm_bindgen(module = "/src/platform/web/worker/workerGen.js")]
extern "C" {
    /// Spawn new worker in JS side in order to make bundler know about dependency.
    #[wasm_bindgen(js_name = "createWorker")]
    fn create_worker(kind: &str, name: &str) -> web_sys::Worker;
}

/// Binds JS.
/// This makes wasm-bindgen bring `mainWorker.js` to the `pkg` directory.
/// So that bundler can bundle it together.
#[wasm_bindgen(module = "/src/platform/web/worker/mainWorker.js")]
extern "C" {
    /// Nothing to do.
    #[wasm_bindgen]
    fn attachMain();

    #[wasm_bindgen(js_name = "listenerReady")]
    fn listener_ready();
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
    /// Spawns main worker from the window context.
    pub fn spawn(
        name: &str,
        id: usize,
        ring_ptr: u32,
        f: impl FnOnce() + Send + 'static,
    ) -> Result<Self, JsValue> {
        // Creates a new worker.
        let handle = create_worker("main", name);

        // Double-boxing because `dyn FnOnce` is unsized and so `Box<dyn FnOnce()>` has
        // an undefined layout (although I think in practice its a pointer and a length?).
        let ptr = Box::into_raw(Box::new(Box::new(f) as Box<dyn FnOnce()>));

        // Sets default callback.
        let callback = Closure::new(|_ev| {});
        handle.set_onmessage(Some(callback.as_ref().unchecked_ref()));

        let msg = js_sys::Object::new();
        for (key, value) in [
            ("type", JsValue::from("init")),
            ("wasmModule", wasm_bindgen::module()),
            ("workerId", id.into()),
            ("memory", wasm_bindgen::memory()),
            ("entryPtr", (ptr as u32).into()),
            ("ringPtr", ring_ptr.into()),
        ] {
            js_sys::Reflect::set(&msg, &key.into(), &value)?;
        }

        info!("posting message");
        handle.post_message(&msg)?;

        Ok(Self {
            handle,
            name: name.to_owned(),
            _callback: callback,
        })
    }

    pub fn transfer_ownership(&self, canvas: &web_sys::HtmlCanvasElement) {
        let offscreen_canvas = canvas.transfer_control_to_offscreen().unwrap();
        let transfer_list = js_sys::Array::new();
        transfer_list.push(&offscreen_canvas);
        let msg = js_sys::Object::new();
        js_sys::Reflect::set(&msg, &"type".into(), &"canvas".into()).unwrap();
        js_sys::Reflect::set(&msg, &"canvas".into(), &offscreen_canvas).unwrap();

        info!("posting canvas (is_undefined: {})", canvas.is_undefined());
        self.handle
            .post_message_with_transfer(&msg, &transfer_list)
            .unwrap();
    }

    pub async fn run_render_loop<T: crate::renderer::scene::Scene + 'static>(
        events_chan: Receiver<WindowEvent>,
        ring: &'static CommandRing,
    ) {
        use crate::renderer::Renderer;

        let canvas = wait_for_canvas_transfer().await;

        let renderer = Rc::new(RefCell::new(Renderer::<T>::new(canvas, events_chan).await));
        renderer.borrow_mut().command_ring = Some(ring);
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

pub async fn wait_for_canvas_transfer() -> web_sys::OffscreenCanvas {
    let global = js_sys::global().unchecked_into::<web_sys::DedicatedWorkerGlobalScope>();

    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let handler = Closure::once(move |event: MessageEvent| {
            let data = event.data();

            info!("data received: {:?}", data);

            // Check if the received data is an OffscreenCanvas directly
            if data.is_instance_of::<web_sys::OffscreenCanvas>() {
                resolve
                    .call1(&JsValue::NULL, &data)
                    .expect("resolve failed");
            }
        });

        global
            .add_event_listener_with_callback("renderer-canvas", handler.as_ref().unchecked_ref())
            .unwrap();
        handler.forget();

        listener_ready();
    });

    let canvas: web_sys::OffscreenCanvas = JsFuture::from(promise)
        .await
        .expect("promise rejected")
        .unchecked_into();

    info!("received canvas: {:?}", canvas);
    canvas
}
