//! Documentation controls use the same small module as the standalone docs host.
pub(crate) fn start() -> Result<(), wasm_bindgen::JsValue> {
    crate::docs_ui::start()
}
