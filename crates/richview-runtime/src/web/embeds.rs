//! Embedded pages load only when the reader asks (FEA-RP-56). The report's HTML carries a card
//! with the host, a link to the page and, once this runtime runs, a "Load from <host>" button.
//! The button swaps the card for the sandboxed frame (ARC-14).

use super::{el, on, query_all};
use wasm_bindgen::JsCast;
use web_sys::Element;

pub fn start() {
    for figure in query_all("figure[data-rv-embed]") {
        let Ok(Some(button)) = figure.query_selector(".rv-embed__load") else {
            continue;
        };
        let _ = button.remove_attribute("hidden");
        let target: web_sys::EventTarget = button.into();
        let figure = figure.clone();
        on(&target, "click", move |_| load(&figure));
    }
}

fn load(figure: &Element) {
    let Some(url) = figure.get_attribute("data-rv-embed") else {
        return;
    };
    // The renderer only writes https addresses of allowed hosts; refuse anything else.
    if !url.starts_with("https://") {
        return;
    }
    let frame = el("iframe", "");
    let _ = frame.set_attribute("src", &url);
    let _ = frame.set_attribute(
        "height",
        &figure
            .get_attribute("data-rv-height")
            .unwrap_or_else(|| "420".into()),
    );
    let title = figure.get_attribute("data-rv-title").unwrap_or_default();
    let _ = frame.set_attribute("title", &title);
    let _ = frame.set_attribute(
        "sandbox",
        &figure.get_attribute("data-rv-sandbox").unwrap_or_default(),
    );
    let _ = frame.set_attribute("referrerpolicy", "strict-origin-when-cross-origin");
    let _ = frame.set_attribute("loading", "lazy");
    figure.set_inner_html("");
    let _ = figure.append_child(&frame);
    let caption = el("figcaption", "");
    caption.set_text_content(Some(&title));
    let _ = figure.append_child(&caption);
    if let Ok(frame) = frame.dyn_into::<web_sys::HtmlElement>() {
        let _ = frame.focus();
    }
}
