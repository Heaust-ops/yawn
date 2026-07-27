use core::fmt;
use std::cell::BorrowMutError;
use std::sync::mpsc::TryRecvError;
use wasm_bindgen::JsCast;

pub const RIGHT_BUTTON_MASK: u16 = 0x02;
pub const MIDDLE_BUTTON_MASK: u16 = 0x04;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraDrag {
    Orbit,
    Pan,
}

pub fn camera_drag(buttons: u16) -> Option<CameraDrag> {
    if buttons & MIDDLE_BUTTON_MASK != 0 {
        Some(CameraDrag::Orbit)
    } else if buttons & RIGHT_BUTTON_MASK != 0 {
        Some(CameraDrag::Pan)
    } else {
        None
    }
}

pub fn normalize_wheel_delta(delta_y: f64, delta_mode: u32, viewport_height: f64) -> Option<f32> {
    if !delta_y.is_finite() || !viewport_height.is_finite() {
        return None;
    }
    let delta = match delta_mode {
        0 => delta_y,
        1 => delta_y * 16.0,
        2 => delta_y * viewport_height.max(1.0),
        _ => return None,
    };
    let delta = delta as f32;
    delta.is_finite().then_some(delta)
}

#[derive(Debug)]
pub enum WindowEvent {
    Resize(ResizeMessage),
    PointerMove(MouseMessage),
    PointerClick(MouseMessage),
    PointerWheel(WheelMessage),
    Keyboard(KeyboardMessage),
}

// Display for WindowEvent
impl fmt::Display for WindowEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WindowEvent::Resize(msg) => write!(f, "Resize: {:?}", msg),
            WindowEvent::PointerMove(msg) => write!(f, "PointerMove: {:?}", msg),
            WindowEvent::PointerClick(msg) => write!(f, "PointerClick: {:?}", msg),
            WindowEvent::PointerWheel(msg) => write!(f, "PointerWheel: {:?}", msg),
            WindowEvent::Keyboard(msg) => write!(f, "Keyboard: {:?}", msg),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResizeMessage {
    pub scale_factor: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone)]
pub struct MouseMessage {
    pub scale_factor: f64,
    pub button: f64,
    pub buttons: u16,
    pub client_x: f64,
    pub client_y: f64,
    pub movement_x: f64,
    pub movement_y: f64,
    pub offset_x: f64,
    pub offset_y: f64,
    pub viewport_height: f64,
}

impl MouseMessage {
    pub fn from_evt(event: &web_sys::MouseEvent, viewport_height: f64) -> Self {
        let window = web_sys::window().unwrap();
        Self {
            scale_factor: window.device_pixel_ratio(),
            button: event.button() as f64,
            buttons: event.buttons(),
            client_x: event.client_x() as f64,
            client_y: event.client_y() as f64,
            movement_x: event.movement_x() as f64,
            movement_y: event.movement_y() as f64,
            offset_x: event.offset_x() as f64,
            offset_y: event.offset_y() as f64,
            viewport_height,
        }
    }

    pub fn from_pointer_evt(event: &web_sys::PointerEvent, viewport_height: f64) -> Self {
        Self::from_evt(event.unchecked_ref(), viewport_height)
    }
}

#[derive(Debug, Clone)]
pub struct WheelMessage {
    pub delta_y_pixels: f32,
}

impl WheelMessage {
    pub fn from_evt(event: &web_sys::WheelEvent, viewport_height: f64) -> Option<Self> {
        normalize_wheel_delta(event.delta_y(), event.delta_mode(), viewport_height)
            .map(|delta_y_pixels| Self { delta_y_pixels })
    }
}

#[derive(Debug, Clone)]
pub struct KeyboardMessage {
    pub key: String,
    pub code: String,
    pub alt_key: bool,
    pub ctrl_key: bool,
    pub meta_key: bool,
    pub shift_key: bool,
    pub location: u32,
    pub repeat: bool,
}

impl KeyboardMessage {
    pub fn from_evt(event: web_sys::KeyboardEvent) -> Self {
        Self {
            key: event.key(),
            code: event.code(),
            alt_key: event.alt_key(),
            ctrl_key: event.ctrl_key(),
            meta_key: event.meta_key(),
            shift_key: event.shift_key(),
            location: event.location(),
            repeat: event.repeat(),
        }
    }
}

#[derive(Debug)]
pub enum DrainEventError {
    BorrowError(BorrowMutError),
    ChannelDisconnected,
    ChannelEmpty,
}

impl fmt::Display for DrainEventError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DrainEventError::BorrowError(err) => write!(f, "Failed to borrow renderer: {}", err),
            DrainEventError::ChannelDisconnected => write!(f, "Event channel disconnected"),
            DrainEventError::ChannelEmpty => write!(f, "Event channel empty"),
        }
    }
}

impl std::error::Error for DrainEventError {}

impl From<TryRecvError> for DrainEventError {
    fn from(err: TryRecvError) -> Self {
        match err {
            TryRecvError::Empty => DrainEventError::ChannelEmpty,
            TryRecvError::Disconnected => DrainEventError::ChannelDisconnected,
        }
    }
}

impl From<BorrowMutError> for DrainEventError {
    fn from(err: BorrowMutError) -> Self {
        DrainEventError::BorrowError(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_delta_is_normalized_to_css_pixels() {
        assert_eq!(normalize_wheel_delta(12.5, 0, 640.0), Some(12.5));
        assert_eq!(normalize_wheel_delta(2.0, 1, 640.0), Some(32.0));
        assert_eq!(normalize_wheel_delta(-1.0, 2, 640.0), Some(-640.0));
        assert_eq!(normalize_wheel_delta(2.0, 2, 0.0), Some(2.0));
        assert_eq!(normalize_wheel_delta(1.0, 3, 640.0), None);
        assert_eq!(normalize_wheel_delta(f64::NAN, 0, 640.0), None);
        assert_eq!(normalize_wheel_delta(f64::INFINITY, 0, 640.0), None);
        assert_eq!(normalize_wheel_delta(1.0, 0, f64::NAN), None);
    }

    #[test]
    fn camera_drag_prefers_orbit_when_both_buttons_are_down() {
        assert_eq!(camera_drag(0), None);
        assert_eq!(camera_drag(1), None);
        assert_eq!(camera_drag(RIGHT_BUTTON_MASK), Some(CameraDrag::Pan));
        assert_eq!(camera_drag(MIDDLE_BUTTON_MASK), Some(CameraDrag::Orbit));
        assert_eq!(
            camera_drag(RIGHT_BUTTON_MASK | MIDDLE_BUTTON_MASK),
            Some(CameraDrag::Orbit)
        );
    }
}
