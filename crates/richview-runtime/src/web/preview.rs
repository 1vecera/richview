//! The `richview serve` preview reload (FEA-SF-08). The trusted preview server marks its page
//! with `<meta name="richview-preview">`, naming its revision route, and the revision it
//! rendered. The page asks for the current revision every 500 ms and reloads when it changes or
//! when the source can no longer render. A report without the mark never fetches anything.

use super::{document, window};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::Response;

const POLL_MS: i32 = 500;

fn meta(name: &str) -> Option<String> {
    document()
        .query_selector(&format!("meta[name='{name}']"))
        .ok()
        .flatten()
        .and_then(|m| m.get_attribute("content"))
}

/// What the revision route says now: `Some(Ok(revision))`, `Some(Err(()))` when the server
/// answers but cannot render the source, and `None` when it cannot be reached.
async fn current(path: &str) -> Option<Result<String, ()>> {
    let response: Response = JsFuture::from(window().fetch_with_str(path))
        .await
        .ok()?
        .dyn_into()
        .ok()?;
    if !response.ok() {
        return Some(Err(()));
    }
    let text = JsFuture::from(response.text().ok()?).await.ok()?;
    let value: serde_json::Value = serde_json::from_str(&text.as_string()?).ok()?;
    Some(value["revision"].as_str().map(str::to_owned).ok_or(()))
}

pub(super) fn start() {
    let Some(path) = meta("richview-preview") else {
        return;
    };
    // Only a same-origin route: the mark names where the preview server answers.
    if !path.starts_with('/') || path.starts_with("//") {
        return;
    }
    let shown = Rc::new(RefCell::new(meta("richview-revision")));
    let busy = Rc::new(RefCell::new(false));
    let tick = Closure::<dyn FnMut()>::new(move || {
        if *busy.borrow() {
            return;
        }
        *busy.borrow_mut() = true;
        let (path, shown, busy) = (path.clone(), shown.clone(), busy.clone());
        spawn_local(async move {
            let reload = match current(&path).await {
                Some(Ok(revision)) => {
                    let changed = shown.borrow().as_deref().is_some_and(|old| old != revision);
                    *shown.borrow_mut() = Some(revision);
                    changed
                }
                // The source broke: the reload shows the server's error (FEA-SF-08).
                Some(Err(())) => true,
                // The server stopped; the last render stays.
                None => false,
            };
            *busy.borrow_mut() = false;
            if reload {
                let _ = window().location().reload();
            }
        });
    });
    let _ = window().set_interval_with_callback_and_timeout_and_arguments_0(
        tick.as_ref().unchecked_ref(),
        POLL_MS,
    );
    tick.forget();
}
