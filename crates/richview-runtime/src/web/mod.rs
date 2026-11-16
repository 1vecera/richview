//! Browser glue for the report runtime: mounting figures, drawing through the shared WebGL
//! renderer, and the reader's interactions (FEA-CH-39 to FEA-CH-56).

mod atlas;
mod embeds;
mod entrance;
mod gl;
mod host;
mod preview;
mod reading;
mod scrolly;
mod tables;
mod tabs;

use std::cell::RefCell;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{Document, Element, HtmlElement, Window};

pub(crate) fn window() -> Window {
    web_sys::window().expect("window")
}

pub(crate) fn document() -> Document {
    window().document().expect("document")
}

pub(crate) fn now() -> f64 {
    window().performance().map_or(0.0, |p| p.now())
}

pub(crate) fn el(tag: &str, class: &str) -> HtmlElement {
    let e: HtmlElement = document()
        .create_element(tag)
        .expect("element")
        .unchecked_into();
    if !class.is_empty() {
        e.set_class_name(class);
    }
    e
}

/// Listen for `event` on `target` for the page's lifetime.
pub(crate) fn on(
    target: &web_sys::EventTarget,
    event: &str,
    f: impl FnMut(web_sys::Event) + 'static,
) {
    let closure = Closure::<dyn FnMut(web_sys::Event)>::new(f);
    let _ = target.add_event_listener_with_callback(event, closure.as_ref().unchecked_ref());
    closure.forget();
}

pub(crate) fn query_all(selector: &str) -> Vec<Element> {
    let mut out = Vec::new();
    if let Ok(list) = document().query_selector_all(selector) {
        for i in 0..list.length() {
            if let Some(node) = list.item(i)
                && let Ok(e) = node.dyn_into::<Element>()
            {
                out.push(e);
            }
        }
    }
    out
}

thread_local! {
    static WAITERS: RefCell<Vec<js_sys::Function>> = const { RefCell::new(Vec::new()) };
}

/// `window.__richview` (FEA-RP-37): `output`, `settled()`, `pending()` and `charts()`, each
/// mounted chart's geometry model and reading state (DEL-72).
fn install_hook() -> Result<(), JsValue> {
    let hook = js_sys::Object::new();
    let output = document()
        .document_element()
        .and_then(|e| e.get_attribute("data-rv-output"))
        .unwrap_or_else(|| "report".into());
    js_sys::Reflect::set(&hook, &"output".into(), &output.into())?;
    let settled = Closure::<dyn FnMut() -> js_sys::Promise>::new(|| {
        host::settle_all();
        js_sys::Promise::new(&mut |resolve, reject| {
            WAITERS.with(|w| w.borrow_mut().push(resolve));
            let timeout = Closure::once_into_js(move || {
                let pending = host::pending();
                if !pending.is_empty() {
                    let _ = reject.call1(
                        &JsValue::NULL,
                        &js_sys::Error::new(&format!(
                            "RichView did not settle: {}",
                            pending.join(", ")
                        )),
                    );
                }
            });
            let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(
                timeout.unchecked_ref(),
                30_000,
            );
            if let Ok(ready) = document().fonts().ready() {
                let check = Closure::once(|_: JsValue| check_settled());
                let _ = ready.then(&check);
                check.forget();
            }
        })
    });
    js_sys::Reflect::set(&hook, &"settled".into(), settled.as_ref())?;
    settled.forget();
    let pending = Closure::<dyn FnMut() -> js_sys::Array>::new(|| {
        host::pending().into_iter().map(JsValue::from).collect()
    });
    js_sys::Reflect::set(&hook, &"pending".into(), pending.as_ref())?;
    pending.forget();
    let charts = Closure::<dyn FnMut() -> js_sys::Array>::new(|| {
        host::models()
            .iter()
            .filter_map(|model| js_sys::JSON::parse(model).ok())
            .collect()
    });
    js_sys::Reflect::set(&hook, &"charts".into(), charts.as_ref())?;
    charts.forget();
    js_sys::Reflect::set(&window(), &"__richview".into(), &hook)?;
    Ok(())
}

/// Resolve every waiting `settled()` once nothing is pending and fonts are loaded.
pub(crate) fn check_settled() {
    let fonts_loaded = document().fonts().status() == web_sys::FontFaceSetLoadStatus::Loaded;
    if fonts_loaded && host::pending().is_empty() {
        let waiters = WAITERS.with(|w| std::mem::take(&mut *w.borrow_mut()));
        for resolve in waiters {
            let _ = resolve.call0(&JsValue::NULL);
        }
    } else if !fonts_loaded
        && WAITERS.with(|w| !w.borrow().is_empty())
        && let Ok(ready) = document().fonts().ready()
    {
        // Drawing after shader compilation can start a new font load after the promise that
        // settled() originally observed. Follow the current cycle as well, including on WebKit
        // where a CSS font load need not dispatch `loadingdone`.
        let check = Closure::once(|_: JsValue| check_settled());
        let _ = ready.then(&check);
        check.forget();
    }
}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    if web_sys::window().is_none() || web_sys::window().and_then(|w| w.document()).is_none() {
        return Ok(());
    }
    let root = document().document_element();
    if root
        .as_ref()
        .and_then(|e| e.get_attribute("data-rv-runtime"))
        .is_some()
    {
        return Ok(());
    }
    if let Some(e) = &root {
        e.set_attribute("data-rv-runtime", "started")?;
    }
    install_hook()?;
    tables::start();
    embeds::start();
    tabs::start()?;
    scrolly::start();
    reading::start();
    entrance::start();
    host::start();
    preview::start();
    let target: web_sys::EventTarget = window().into();
    on(&target, "beforeprint", |_| host::print());
    on(&target, "afterprint", |_| host::after_print());
    if let Ok(Some(print)) = window().match_media("print") {
        let media = print.clone();
        let target: web_sys::EventTarget = print.into();
        on(&target, "change", move |_| {
            if media.matches() {
                host::print();
            } else {
                host::after_print();
            }
        });
    }
    if let Ok(Some(motion)) = window().match_media("(prefers-reduced-motion: reduce)") {
        let target: web_sys::EventTarget = motion.into();
        on(&target, "change", |_| host::motion_changed());
    }
    Ok(())
}
