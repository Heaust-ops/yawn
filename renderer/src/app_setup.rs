use std::{
    cell::Cell,
    rc::Rc,
    sync::mpsc::{self, Sender},
};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

#[cfg(target_arch = "wasm32")]
use web_sys::AddEventListenerOptions;

use crate::message::WindowEvent;
#[cfg(target_arch = "wasm32")]
use crate::platform::web;
#[cfg(target_arch = "wasm32")]
use crate::platform::web::worker::MainWorker;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local;

/// Helper struct to store event listener closures
#[cfg(target_arch = "wasm32")]
pub struct EventListeners {
    window: web_sys::Window,
    canvas: web_sys::HtmlCanvasElement,
    pub resize_listener: Option<Closure<dyn FnMut()>>,
    pub mousemove_listener: Option<Closure<dyn FnMut(web_sys::MouseEvent)>>,
    pub mousedown_listener: Option<Closure<dyn FnMut(web_sys::MouseEvent)>>,
    pub mouseup_listener: Option<Closure<dyn FnMut(web_sys::MouseEvent)>>,
    pub wheel_listener: Option<Closure<dyn FnMut(web_sys::WheelEvent)>>,
    pub keyboard_listener: Option<Closure<dyn FnMut(web_sys::KeyboardEvent)>>,
}

#[cfg(target_arch = "wasm32")]
impl EventListeners {
    pub fn new(window: web_sys::Window, canvas: web_sys::HtmlCanvasElement) -> Self {
        Self {
            window,
            canvas,
            resize_listener: None,
            mousemove_listener: None,
            mousedown_listener: None,
            mouseup_listener: None,
            wheel_listener: None,
            keyboard_listener: None,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for EventListeners {
    fn drop(&mut self) {
        if let Some(listener) = &self.resize_listener {
            let _ = self
                .window
                .remove_event_listener_with_callback("resize", listener.as_ref().unchecked_ref());
        }
        if let Some(listener) = &self.keyboard_listener {
            let _ = self
                .window
                .remove_event_listener_with_callback("keydown", listener.as_ref().unchecked_ref());
        }
        if let Some(listener) = &self.mousemove_listener {
            for kind in ["pointermove", "click"] {
                let _ = self
                    .canvas
                    .remove_event_listener_with_callback(kind, listener.as_ref().unchecked_ref());
            }
        }
        if let Some(listener) = &self.mousedown_listener {
            let _ = self.canvas.remove_event_listener_with_callback(
                "pointerdown",
                listener.as_ref().unchecked_ref(),
            );
        }
        if let Some(listener) = &self.mouseup_listener {
            for kind in ["pointerup", "pointercancel", "lostpointercapture"] {
                let _ = self
                    .canvas
                    .remove_event_listener_with_callback(kind, listener.as_ref().unchecked_ref());
            }
        }
        if let Some(listener) = &self.wheel_listener {
            let _ = self
                .canvas
                .remove_event_listener_with_callback("wheel", listener.as_ref().unchecked_ref());
        }
    }
}

/// Setup default window event listeners that forward events to the worker thread
#[cfg(target_arch = "wasm32")]
pub fn setup_event_listeners(
    worker_chan: &Sender<WindowEvent>,
    canvas: &web_sys::HtmlCanvasElement,
) -> Result<EventListeners, JsValue> {
    let window =
        web_sys::window().ok_or_else(|| JsValue::from_str("browser window is unavailable"))?;
    let mut listeners = EventListeners::new(window.clone(), canvas.clone());
    let resize_worker_chan = worker_chan.clone();

    let resize_listener: Closure<dyn FnMut()> = Closure::new(move || {
        use crate::message::ResizeMessage;

        let Some(window) = web_sys::window() else {
            log::error!("cannot process resize: browser window is unavailable");
            return;
        };
        let (Ok(width), Ok(height)) = (window.inner_width(), window.inner_height()) else {
            log::error!("cannot process resize: browser dimensions are unavailable");
            return;
        };
        let (Some(width), Some(height)) = (width.as_f64(), height.as_f64()) else {
            log::error!("cannot process resize: browser dimensions are not numeric");
            return;
        };

        if resize_worker_chan
            .send(WindowEvent::Resize(ResizeMessage {
                width,
                height,
                scale_factor: window.device_pixel_ratio(),
            }))
            .is_err()
        {
            log::error!("cannot forward resize event: worker channel disconnected");
        }
    });

    listeners.resize_listener = Some(resize_listener);
    window.add_event_listener_with_callback(
        "resize",
        listeners
            .resize_listener
            .as_ref()
            .ok_or_else(|| JsValue::from_str("missing resize listener"))?
            .as_ref()
            .unchecked_ref(),
    )?;

    let mousemove_worker_chan = worker_chan.clone();
    let mousemove_listener: Closure<dyn FnMut(web_sys::MouseEvent)> =
        Closure::new(move |event: web_sys::MouseEvent| {
            use crate::message::MouseMessage;
            if event.buttons() & 0x04 != 0 {
                event.prevent_default();
            }
            let mouse_event_data = MouseMessage::from_evt(event.clone());

            let mut event_data = WindowEvent::PointerMove(mouse_event_data.clone());
            if event.type_() == "click" {
                event_data = WindowEvent::PointerClick(mouse_event_data.clone());
            }

            if mousemove_worker_chan.send(event_data).is_err() {
                log::error!("cannot forward mouse event: worker channel disconnected");
            }
        });

    listeners.mousemove_listener = Some(mousemove_listener);
    canvas.add_event_listener_with_callback(
        "pointermove",
        listeners
            .mousemove_listener
            .as_ref()
            .ok_or_else(|| JsValue::from_str("missing pointer listener"))?
            .as_ref()
            .unchecked_ref(),
    )?;

    canvas.add_event_listener_with_callback(
        "click",
        listeners
            .mousemove_listener
            .as_ref()
            .ok_or_else(|| JsValue::from_str("missing pointer listener"))?
            .as_ref()
            .unchecked_ref(),
    )?;

    let capture_canvas = canvas.clone();
    let active_pointer = Rc::new(Cell::new(None::<i32>));
    let down_pointer = active_pointer.clone();
    let mousedown_listener: Closure<dyn FnMut(web_sys::MouseEvent)> =
        Closure::new(move |event: web_sys::MouseEvent| {
            if event.button() == 1 {
                event.prevent_default();
                if let Some(pointer_id) = js_sys::Reflect::get(&event, &"pointerId".into())
                    .ok()
                    .and_then(|id| id.as_f64())
                {
                    if let Err(error) = capture_canvas.set_pointer_capture(pointer_id as i32) {
                        log::error!("failed to capture drag pointer: {error:?}");
                    } else {
                        down_pointer.set(Some(pointer_id as i32));
                    }
                }
            }
        });

    listeners.mousedown_listener = Some(mousedown_listener);
    canvas.add_event_listener_with_callback(
        "pointerdown",
        listeners
            .mousedown_listener
            .as_ref()
            .ok_or_else(|| JsValue::from_str("missing pointer-down listener"))?
            .as_ref()
            .unchecked_ref(),
    )?;

    let release_canvas = canvas.clone();
    let release_pointer = active_pointer.clone();
    let mouseup_listener: Closure<dyn FnMut(web_sys::MouseEvent)> =
        Closure::new(move |_event: web_sys::MouseEvent| {
            if let Some(pointer_id) = release_pointer.take() {
                if release_canvas.has_pointer_capture(pointer_id) {
                    if let Err(error) = release_canvas.release_pointer_capture(pointer_id) {
                        log::error!("failed to release drag pointer: {error:?}");
                    }
                }
            }
        });
    listeners.mouseup_listener = Some(mouseup_listener);
    canvas.add_event_listener_with_callback(
        "pointerup",
        listeners
            .mouseup_listener
            .as_ref()
            .ok_or_else(|| JsValue::from_str("missing pointer-up listener"))?
            .as_ref()
            .unchecked_ref(),
    )?;
    canvas.add_event_listener_with_callback(
        "pointercancel",
        listeners
            .mouseup_listener
            .as_ref()
            .ok_or_else(|| JsValue::from_str("missing pointer-up listener"))?
            .as_ref()
            .unchecked_ref(),
    )?;
    canvas.add_event_listener_with_callback(
        "lostpointercapture",
        listeners
            .mouseup_listener
            .as_ref()
            .ok_or_else(|| JsValue::from_str("missing pointer-up listener"))?
            .as_ref()
            .unchecked_ref(),
    )?;

    let wheel_worker_chan = worker_chan.clone();
    let wheel_listener: Closure<dyn FnMut(web_sys::WheelEvent)> =
        Closure::new(move |event: web_sys::WheelEvent| {
            use crate::message::WheelMessage;

            event.prevent_default();
            let wheel_event_data = WheelMessage::from_evt(event);

            if wheel_worker_chan
                .send(WindowEvent::PointerWheel(wheel_event_data))
                .is_err()
            {
                log::error!("cannot forward wheel event: worker channel disconnected");
            }
        });

    let wheel_options = {
        let options = AddEventListenerOptions::new();
        options.set_passive(false);
        options
    };

    listeners.wheel_listener = Some(wheel_listener);
    canvas.add_event_listener_with_callback_and_add_event_listener_options(
        "wheel",
        listeners
            .wheel_listener
            .as_ref()
            .ok_or_else(|| JsValue::from_str("missing wheel listener"))?
            .as_ref()
            .unchecked_ref(),
        &wheel_options,
    )?;

    let keyboard_worker_chan = worker_chan.clone();
    let keyboard_listener: Closure<dyn FnMut(web_sys::KeyboardEvent)> =
        Closure::new(move |event: web_sys::KeyboardEvent| {
            use crate::message::KeyboardMessage;

            let keyboard_event_data = KeyboardMessage::from_evt(event);

            if keyboard_worker_chan
                .send(WindowEvent::Keyboard(keyboard_event_data))
                .is_err()
            {
                log::error!("cannot forward keyboard event: worker channel disconnected");
            }
        });

    listeners.keyboard_listener = Some(keyboard_listener);
    window.add_event_listener_with_callback(
        "keydown",
        listeners
            .keyboard_listener
            .as_ref()
            .ok_or_else(|| JsValue::from_str("missing keyboard listener"))?
            .as_ref()
            .unchecked_ref(),
    )?;

    Ok(listeners)
}

/// Runtime resources required to keep a WASM application running.
#[cfg(target_arch = "wasm32")]
pub struct WebAppRuntime {
    worker: MainWorker,
    worker_chan: Sender<WindowEvent>,
    _event_listeners: EventListeners,
}

#[cfg(target_arch = "wasm32")]
impl WebAppRuntime {
    /// Initialize the web worker, canvas ownership, and event listeners.
    pub fn new<T: crate::renderer::scene::Scene + 'static>(
        worker_name: &str,
        canvas_selector: &str,
    ) -> Result<Self, JsValue> {
        let (sender, receiver) = mpsc::channel::<WindowEvent>();

        let canvas = web::get_canvas_element(canvas_selector)?;
        // Create the worker while the canvas is still recoverable.
        let worker = MainWorker::create(worker_name)?;
        // Listener registration is fallible and must complete before canvas ownership
        // is irreversibly transferred to the worker.
        let event_listeners = setup_event_listeners(&sender, &canvas)?;
        let offscreen_canvas = canvas.transfer_control_to_offscreen()?;
        worker.start(1, offscreen_canvas, move || {
            spawn_local(async move {
                MainWorker::run_render_loop::<T>(receiver).await;
            });
        })?;

        Ok(Self {
            worker,
            worker_chan: sender,
            _event_listeners: event_listeners,
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
    fn setup_runtime() -> Result<WebAppRuntime, JsValue> {
        let mut runtime =
            WebAppRuntime::new::<Self::Scene>(Self::worker_name(), Self::canvas_selector())?;
        Self::on_runtime_initialized(&mut runtime);
        Ok(runtime)
    }
}
