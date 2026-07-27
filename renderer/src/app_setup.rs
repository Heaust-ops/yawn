use std::sync::mpsc::{self, Sender};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::command_ring::CommandRing;
use crate::message::WindowEvent;
#[cfg(target_arch = "wasm32")]
use crate::platform::web;
#[cfg(target_arch = "wasm32")]
use crate::platform::web::worker::MainWorker;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local;
#[cfg(target_arch = "wasm32")]
use web_sys::AddEventListenerOptions;

/// Helper struct to store event listener closures
#[cfg(target_arch = "wasm32")]
pub struct EventListeners {
    pub resize_listener: Option<Closure<dyn FnMut()>>,
    pub pointer_listener: Option<Closure<dyn FnMut(web_sys::PointerEvent)>>,
    pub click_listener: Option<Closure<dyn FnMut(web_sys::MouseEvent)>>,
    pub wheel_listener: Option<Closure<dyn FnMut(web_sys::WheelEvent)>>,
    pub contextmenu_listener: Option<Closure<dyn FnMut(web_sys::MouseEvent)>>,
    pub keyboard_listener: Option<Closure<dyn FnMut(web_sys::KeyboardEvent)>>,
}

#[cfg(target_arch = "wasm32")]
impl EventListeners {
    pub fn new() -> Self {
        Self {
            resize_listener: None,
            pointer_listener: None,
            click_listener: None,
            wheel_listener: None,
            contextmenu_listener: None,
            keyboard_listener: None,
        }
    }
}

/// Setup default window event listeners that forward events to the worker thread
#[cfg(target_arch = "wasm32")]
pub fn setup_event_listeners(
    worker_chan: &Sender<WindowEvent>,
    canvas: &web_sys::HtmlCanvasElement,
) -> Result<EventListeners, JsValue> {
    let window = web_sys::window().unwrap();
    let resize_worker_chan = worker_chan.clone();
    let resize_canvas = canvas.clone();

    let resize_listener: Closure<dyn FnMut()> = Closure::new(move || {
        use crate::message::ResizeMessage;

        let window = web_sys::window().unwrap();
        let width = f64::from(resize_canvas.client_width().max(1));
        let height = f64::from(resize_canvas.client_height().max(1));

        let _ = resize_worker_chan.send(WindowEvent::Resize(ResizeMessage {
            width,
            height,
            scale_factor: window.device_pixel_ratio(),
        }));
    });

    window.add_event_listener_with_callback("resize", resize_listener.as_ref().unchecked_ref())?;

    let pointer_worker_chan = worker_chan.clone();
    let pointer_canvas = canvas.clone();
    let pointer_listener: Closure<dyn FnMut(web_sys::PointerEvent)> =
        Closure::new(move |event: web_sys::PointerEvent| {
            use crate::message::{camera_drag, MouseMessage};

            if event.pointer_type() != "mouse" {
                return;
            }
            match event.type_().as_str() {
                "pointerdown" if matches!(event.button(), 1 | 2) => {
                    event.prevent_default();
                    let _ = pointer_canvas.set_pointer_capture(event.pointer_id());
                }
                "pointermove"
                    if pointer_canvas.has_pointer_capture(event.pointer_id())
                        && camera_drag(event.buttons()).is_some() =>
                {
                    event.prevent_default();
                    let message = MouseMessage::from_pointer_evt(
                        &event,
                        f64::from(pointer_canvas.client_height().max(1)),
                    );
                    let _ = pointer_worker_chan.send(WindowEvent::PointerMove(message));
                }
                "pointerup" | "pointercancel"
                    if pointer_canvas.has_pointer_capture(event.pointer_id()) =>
                {
                    let _ = pointer_canvas.release_pointer_capture(event.pointer_id());
                }
                _ => {}
            }
        });

    for event_name in ["pointerdown", "pointermove", "pointerup", "pointercancel"] {
        canvas.add_event_listener_with_callback(
            event_name,
            pointer_listener.as_ref().unchecked_ref(),
        )?;
    }

    let click_worker_chan = worker_chan.clone();
    let click_canvas = canvas.clone();
    let click_listener: Closure<dyn FnMut(web_sys::MouseEvent)> =
        Closure::new(move |event: web_sys::MouseEvent| {
            use crate::message::MouseMessage;
            if event.button() != 0 {
                return;
            }
            let message =
                MouseMessage::from_evt(&event, f64::from(click_canvas.client_height().max(1)));
            let _ = click_worker_chan.send(WindowEvent::PointerClick(message));
        });
    canvas.add_event_listener_with_callback("click", click_listener.as_ref().unchecked_ref())?;

    let wheel_worker_chan = worker_chan.clone();
    let wheel_canvas = canvas.clone();
    let wheel_listener: Closure<dyn FnMut(web_sys::WheelEvent)> =
        Closure::new(move |event: web_sys::WheelEvent| {
            use crate::message::WheelMessage;

            event.prevent_default();
            if let Some(message) =
                WheelMessage::from_evt(&event, f64::from(wheel_canvas.client_height().max(1)))
            {
                let _ = wheel_worker_chan.send(WindowEvent::PointerWheel(message));
            }
        });

    let wheel_options = {
        let options = AddEventListenerOptions::new();
        options.set_passive(false);
        options
    };

    canvas.add_event_listener_with_callback_and_add_event_listener_options(
        "wheel",
        wheel_listener.as_ref().unchecked_ref(),
        &wheel_options,
    )?;

    let contextmenu_listener: Closure<dyn FnMut(web_sys::MouseEvent)> =
        Closure::new(move |event: web_sys::MouseEvent| event.prevent_default());
    canvas.add_event_listener_with_callback(
        "contextmenu",
        contextmenu_listener.as_ref().unchecked_ref(),
    )?;

    let keyboard_worker_chan = worker_chan.clone();
    let keyboard_listener: Closure<dyn FnMut(web_sys::KeyboardEvent)> =
        Closure::new(move |event: web_sys::KeyboardEvent| {
            use crate::message::KeyboardMessage;

            let keyboard_event_data = KeyboardMessage::from_evt(event);

            let _ = keyboard_worker_chan.send(WindowEvent::Keyboard(keyboard_event_data));
        });

    window
        .add_event_listener_with_callback("keydown", keyboard_listener.as_ref().unchecked_ref())?;

    Ok(EventListeners {
        resize_listener: Some(resize_listener),
        pointer_listener: Some(pointer_listener),
        click_listener: Some(click_listener),
        wheel_listener: Some(wheel_listener),
        contextmenu_listener: Some(contextmenu_listener),
        keyboard_listener: Some(keyboard_listener),
    })
}

/// Runtime resources required to keep a WASM application running.
#[cfg(target_arch = "wasm32")]
pub struct WebAppRuntime {
    worker: MainWorker,
    worker_chan: Sender<WindowEvent>,
    _event_listeners: EventListeners,
    ring: Box<CommandRing>,
}

#[cfg(target_arch = "wasm32")]
impl WebAppRuntime {
    /// Initialize the web worker, canvas ownership, and event listeners.
    pub fn new<T: crate::renderer::scene::Scene + 'static>(
        worker_name: &str,
        canvas_selector: &str,
        profile: bool,
    ) -> Result<Self, JsValue> {
        let (sender, receiver) = mpsc::channel::<WindowEvent>();

        let canvas = web::get_canvas_element(canvas_selector);
        let window = web_sys::window().unwrap();
        let dpr = window.device_pixel_ratio();
        canvas.set_width((canvas.client_width() as f64 * dpr).round() as u32);
        canvas.set_height((canvas.client_height() as f64 * dpr).round() as u32);
        let ring = CommandRing::new();
        let ring_ptr = ring.ptr();
        let worker = MainWorker::spawn(worker_name, 1, ring_ptr, move || {
            spawn_local(async move {
                let ring = unsafe { &*(ring_ptr as *const CommandRing) };
                MainWorker::run_render_loop::<T>(receiver, ring, profile).await;
            });
        })?;

        worker.transfer_ownership(&canvas);

        let event_listeners = setup_event_listeners(&sender, &canvas)?;

        Ok(Self {
            worker,
            worker_chan: sender,
            _event_listeners: event_listeners,
            ring,
        })
    }

    /// Access the worker channel sender for dispatching custom window events.
    pub fn sender(&self) -> &Sender<WindowEvent> {
        &self.worker_chan
    }

    /// Access the spawned worker reference.
    pub fn worker(&self) -> &MainWorker {
        &self.worker
    }
    pub fn ring_ptr(&self) -> u32 {
        self.ring.ptr()
    }
}

/// Trait for applications that rely on the renderer's default WASM setup.
#[cfg(target_arch = "wasm32")]
pub trait WebApp {
    type Scene: crate::renderer::scene::Scene + 'static;

    /// Name used for the spawned `MainWorker`.
    fn worker_name() -> &'static str {
        "main-worker"
    }

    /// CSS selector for the canvas element that will be transferred to the worker.
    fn canvas_selector() -> &'static str {
        "#canvas0"
    }

    /// Hook invoked after the runtime has been created.
    fn on_runtime_initialized(_runtime: &mut WebAppRuntime) {}

    /// Perform the default WASM initialization routine.
    fn setup_runtime(profile: bool) -> Result<WebAppRuntime, JsValue> {
        let mut runtime = WebAppRuntime::new::<Self::Scene>(
            Self::worker_name(),
            Self::canvas_selector(),
            profile,
        )?;
        Self::on_runtime_initialized(&mut runtime);
        Ok(runtime)
    }
}
