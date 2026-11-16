//! Scrollytelling (FEA-RP-48). The report's HTML already shows the first step's figure and every
//! step in order; this moves the figure to the state of the step that crosses the middle of the
//! viewport, or that takes keyboard focus. Only custom properties on the block change, so the
//! stylesheet decides whether the move glides or jumps (reduced motion jumps), and step text never
//! moves.

use super::{on, query_all, window};
use std::cell::Cell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{Element, HtmlElement};

/// Show `step`'s state on its block's figure and mark it as the current step.
fn activate(block: &Element, step: &Element) {
    // The static first-current label does not mean this step's choices have run. Resync its
    // controls on activation; the chart may still be at its own authored default.
    choose_controls(block, step);
    if step.get_attribute("aria-current").as_deref() == Some("step") {
        return;
    }
    for other in query_steps(block) {
        let _ = other.remove_attribute("aria-current");
    }
    let _ = step.set_attribute("aria-current", "step");
    let Some(style) = block.dyn_ref::<HtmlElement>().map(HtmlElement::style) else {
        return;
    };
    let numbers = |attr: &str| -> Vec<String> {
        step.get_attribute(attr)
            .unwrap_or_default()
            .split_whitespace()
            .map(str::to_owned)
            .collect()
    };
    if let [x, y, zoom] = numbers("data-rv-view").as_slice() {
        let _ = style.set_property("--rv-vx", x);
        let _ = style.set_property("--rv-vy", y);
        let _ = style.set_property("--rv-vz", zoom);
    }
    match numbers("data-rv-mark").as_slice() {
        [l, t, w, h] => {
            for (name, value) in [("ml", l), ("mt", t), ("mw", w), ("mh", h)] {
                let _ = style.set_property(&format!("--rv-{name}"), &format!("{value}%"));
            }
            let _ = style.set_property("--rv-mo", "1");
        }
        _ => {
            let _ = style.set_property("--rv-mo", "0");
        }
    }
}

/// A step on a chart names `control:option` pairs (`data-rv-controls`); the figure's own control
/// row takes them as if the reader had chosen, so the chart moves the way its controls move it.
fn choose_controls(block: &Element, step: &Element) {
    let Some(pairs) = step.get_attribute("data-rv-controls") else {
        return;
    };
    // The story's chart takes the step's choices only once it is mounted (MOTION.md).
    if let Ok(figures) = block.query_selector_all("figure[data-rv-chart]") {
        for i in 0..figures.length() {
            if let Some(Ok(figure)) = figures.item(i).map(|n| n.dyn_into::<Element>()) {
                super::host::mount_now(&figure);
            }
        }
    }
    for pair in pairs.split_whitespace() {
        let Some((control, option)) = pair.split_once(':') else {
            continue;
        };
        let group = format!(".rv-chart__controls [data-rv-control=\"{control}\"]");
        let Ok(Some(group)) = block.query_selector(&group) else {
            continue;
        };
        let button = format!(".rv-chart__option[data-rv-option=\"{option}\"]");
        if let Ok(Some(button)) = group.query_selector(&button) {
            if let Ok(button) = button.dyn_into::<HtmlElement>() {
                button.click();
            }
        } else if let Ok(Some(select)) = group.query_selector("select")
            && let Ok(select) = select.dyn_into::<web_sys::HtmlSelectElement>()
        {
            select.set_value(option);
            if let Ok(event) = web_sys::Event::new("change") {
                let _ = select.dispatch_event(&event);
            }
        } else if let Ok(Some(input)) = group.query_selector(".rv-chart__scrubber")
            && let Ok(input) = input.dyn_into::<web_sys::HtmlInputElement>()
        {
            input.set_value(option);
            if let Ok(event) = web_sys::Event::new("input") {
                let _ = input.dispatch_event(&event);
            }
        }
    }
}

fn query_steps(block: &Element) -> Vec<Element> {
    let mut steps = vec![];
    if let Ok(list) = block.query_selector_all("[data-rv-step]") {
        for i in 0..list.length() {
            if let Some(Ok(step)) = list.item(i).map(|n| n.dyn_into::<Element>()) {
                steps.push(step);
            }
        }
    }
    steps
}

pub fn start() {
    for block in query_all("[data-rv-scrolly]") {
        let steps = query_steps(&block);
        let focused = Rc::new(Cell::new(false));
        for step in &steps {
            let (block, step) = (block.clone(), step.clone());
            let target: web_sys::EventTarget = step.clone().into();
            let focused = Rc::clone(&focused);
            // A reader tabbing into a step's link reads that step, so the figure follows.
            on(&target, "focusin", move |_| {
                focused.set(true);
                activate(&block, &step);
            });
        }
        let focus_left = Rc::clone(&focused);
        on(&block.clone().into(), "focusout", move |_| {
            focus_left.set(false)
        });
        // Focus scrolls the page too. Keep that scroll from replacing the keyboard's choice;
        // an explicit scroll gesture hands control back to the viewport.
        for event in ["wheel", "touchmove", "pointerdown"] {
            let focused = Rc::clone(&focused);
            on(&window().into(), event, move |_| focused.set(false));
        }
        let focus_key = Rc::clone(&focused);
        on(&window().into(), "keydown", move |event| {
            if let Ok(event) = event.dyn_into::<web_sys::KeyboardEvent>()
                && matches!(
                    event.key().as_str(),
                    "PageDown" | "PageUp" | "ArrowDown" | "ArrowUp" | "Home" | "End" | " "
                )
            {
                focus_key.set(false);
            }
        });
        let callback = {
            let block = block.clone();
            Closure::<dyn FnMut(js_sys::Array)>::new(move |entries: js_sys::Array| {
                if focused.get() {
                    return;
                }
                for entry in entries.iter() {
                    let Ok(entry) = entry.dyn_into::<web_sys::IntersectionObserverEntry>() else {
                        continue;
                    };
                    if entry.is_intersecting() {
                        activate(&block, &entry.target());
                    }
                }
            })
        };
        let options = web_sys::IntersectionObserverInit::new();
        // A band across the middle of the viewport: the step in it is the one being read.
        options.set_root_margin("-45% 0px -45% 0px");
        let Ok(observer) = web_sys::IntersectionObserver::new_with_options(
            callback.as_ref().unchecked_ref(),
            &options,
        ) else {
            continue;
        };
        callback.forget();
        for step in &steps {
            observer.observe(step);
        }
    }
}
