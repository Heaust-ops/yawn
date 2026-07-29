use wasm_bindgen::JsCast;

pub mod worker;

pub fn get_canvas_element(selectors: &str) -> web_sys::HtmlCanvasElement {
    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();
    let element = document.query_selector(selectors).unwrap().unwrap();
    element.dyn_into::<web_sys::HtmlCanvasElement>().unwrap()
}
