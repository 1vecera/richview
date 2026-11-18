//! Lightweight Wasm entry for documentation; no chart renderer or playground is linked.
#[cfg(target_arch = "wasm32")]
#[path = "../docs_ui.rs"]
mod docs_ui;
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() -> Result<(), wasm_bindgen::JsValue> {
    if let Some(root) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|doc| doc.document_element())
    {
        root.set_attribute("data-richview-started", "rust")?;
        if !root
            .class_name()
            .split_ascii_whitespace()
            .any(|name| name == "js")
        {
            root.set_class_name(format!("{} js", root.class_name()).trim());
        }
    }
    docs_ui::start()
}
fn main() {}
