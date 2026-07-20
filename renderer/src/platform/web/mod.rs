use wasm_bindgen::JsCast;

pub mod worker;

pub fn get_canvas_element(
    selectors: &str,
) -> Result<web_sys::HtmlCanvasElement, wasm_bindgen::JsValue> {
    let window = web_sys::window()
        .ok_or_else(|| wasm_bindgen::JsValue::from_str("browser window is unavailable"))?;
    let document = window
        .document()
        .ok_or_else(|| wasm_bindgen::JsValue::from_str("browser document is unavailable"))?;
    let element = document.query_selector(selectors)?.ok_or_else(|| {
        wasm_bindgen::JsValue::from_str(&format!("canvas selector did not match: {selectors}"))
    })?;
    let canvas = element
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| {
            wasm_bindgen::JsValue::from_str(&format!("element is not a canvas: {selectors}"))
        })?;
    let scale_factor = window.device_pixel_ratio();
    let width = (canvas.client_width() as f64 * scale_factor) as u32;
    let height = (canvas.client_height() as f64 * scale_factor) as u32;
    canvas.set_width(width);
    canvas.set_height(height);
    Ok(canvas)
}
