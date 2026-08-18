use crate::command_ring::CommandRing;
use crate::message::WindowEvent;
use log::info;
use std::sync::mpsc::Receiver;
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{prelude::*, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::MessageEvent;

pub async fn run_render_loop<T: crate::renderer::scene::Scene + 'static>(
    events_chan: Receiver<WindowEvent>,
    ring: &'static CommandRing,
    profile: bool,
) {
    use crate::renderer::Renderer;

    let canvas = wait_for_canvas_transfer().await;

    let renderer = Rc::new(RefCell::new(
        Renderer::<T>::new(canvas, events_chan, profile).await,
    ));
    renderer.borrow_mut().command_ring = Some(ring);
    Renderer::run_render_loop(renderer);
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
    });

    let canvas: web_sys::OffscreenCanvas = JsFuture::from(promise)
        .await
        .expect("promise rejected")
        .unchecked_into();

    info!("received canvas: {:?}", canvas);
    canvas
}
