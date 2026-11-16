//! Soft block entrances (FEA-RP-35). Blocks below the first screen fade up once, about 300 ms,
//! as they come into view. The report's HTML shows every block at rest; the runtime hides only
//! the blocks that start off screen, and reveals each one once, so nothing waits for a script
//! that never runs. Reduced motion, print and a block already in view change nothing.

use super::{document, query_all, window};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{Element, IntersectionObserver, IntersectionObserverEntry};

/// Blocks that arrive together (an anchor jump) stagger over at most this many steps, so the last
/// one never waits long behind the first.
const STAGGER_STEPS: f64 = 4.0;

pub fn start() {
    let reduced = window()
        .match_media("(prefers-reduced-motion: reduce)")
        .ok()
        .flatten()
        .is_some_and(|m| m.matches());
    // A style that switches motion off renders everything settled.
    let motion_off = document()
        .query_selector(".rv-report[data-motion=\"off\"]")
        .ok()
        .flatten()
        .is_some();
    // The style's motion interface (INT-12): `none` leaves every block at rest, and the stagger
    // spaces blocks that come into view together.
    let root = document().document_element();
    let property = |name: &str| {
        root.as_ref()
            .and_then(|r| window().get_computed_style(r).ok().flatten())
            .and_then(|s| s.get_property_value(name).ok())
            .map(|v| v.trim().to_owned())
            .unwrap_or_default()
    };
    let block = property("--rv-motion-block");
    let stagger: f64 = property("--rv-motion-stagger")
        .trim_end_matches("ms")
        .parse()
        .unwrap_or(0.0);
    if reduced || motion_off || block == "none" {
        return;
    }
    let height = window()
        .inner_height()
        .ok()
        .and_then(|h| h.as_f64())
        .unwrap_or(0.0);
    let callback = Closure::<dyn FnMut(js_sys::Array, IntersectionObserver)>::new(
        move |entries: js_sys::Array, observer: IntersectionObserver| {
            let mut order: f64 = 0.0;
            for entry in entries.iter() {
                let entry: IntersectionObserverEntry = entry.unchecked_into();
                if entry.is_intersecting() {
                    let target = entry.target();
                    if stagger > 0.0
                        && let Some(html) = target.dyn_ref::<web_sys::HtmlElement>()
                    {
                        let _ = html.style().set_property(
                            "transition-delay",
                            &format!("{}ms", order.min(STAGGER_STEPS) * stagger),
                        );
                        order += 1.0;
                    }
                    let _ = target.class_list().add_1("rv-entered");
                    observer.unobserve(&target);
                }
            }
        },
    );
    let Ok(observer) = IntersectionObserver::new(callback.as_ref().unchecked_ref()) else {
        return;
    };
    callback.forget();
    for block in query_all(".rv-report__body > :not(.rv-keep), .rv-keep > *") {
        // Only blocks wholly below the first screen enter; the rest are already at rest.
        if block.get_bounding_client_rect().top() < height {
            continue;
        }
        mark(&block);
        observer.observe(&block);
    }
}

fn mark(block: &Element) {
    let _ = block.class_list().add_1("rv-enter");
    // Keep the document element's marker for stylesheets that scope the motion.
    let _ = document()
        .document_element()
        .map(|root| root.set_attribute("data-rv-entrances", "on"));
}
