//! The docs' preview frames load progressively and stay within a memory bound. Each preview is a
//! sandboxed report with its own runtime and WebGL context, and a phone cannot hold the gallery's
//! dozen at once (Daniel, 15 Nov: the chart gallery crashed his phone). A frame loads when it
//! comes within half a screen of the viewport, one at a time, nearest first: the next starts when
//! the previous has loaded, or after a few seconds. A frame two screens away is unloaded, and so
//! is the farthest one when more than a few are loaded; its box shows the loading state again.
//! The frame's address is in `data-src`; one written as `src` is taken over the same way.

use std::cell::{Cell, RefCell};
use wasm_bindgen::{JsCast, prelude::*};
use web_sys::{Element, HtmlIFrameElement, IntersectionObserver, IntersectionObserverEntry};

const NEAR: &str = "50% 0px";
const FAR: &str = "200% 0px";
/// A frame that never reports its load does not hold up the next one for longer.
const LOAD_WAIT_MS: i32 = 4000;
const BLANK: &str = "about:blank";

struct Frame {
    frame: HtmlIFrameElement,
    src: String,
    near: bool,
    loaded: bool,
}

thread_local! {
    static FRAMES: RefCell<Vec<Frame>> = const { RefCell::new(Vec::new()) };
    /// The frame loading now, if any.
    static LOADING: Cell<Option<usize>> = const { Cell::new(None) };
    /// Each load's number, so a late load event or timer of an earlier one is ignored.
    static TICKET: Cell<u32> = const { Cell::new(0) };
}

fn window() -> web_sys::Window {
    web_sys::window().expect("browser")
}

/// How many frames may be loaded at once: fewer on a phone.
fn live_limit() -> usize {
    let width = window()
        .inner_width()
        .ok()
        .and_then(|w| w.as_f64())
        .unwrap_or(1280.0);
    if width < 600.0 { 4 } else { 9 }
}

pub(crate) fn start() {
    let Some(document) = window().document() else {
        return;
    };
    let Ok(list) = document.query_selector_all("iframe[data-docs-preview]") else {
        return;
    };
    let mut frames = Vec::new();
    for i in 0..list.length() {
        let Some(frame) = list
            .item(i)
            .and_then(|n| n.dyn_into::<HtmlIFrameElement>().ok())
        else {
            continue;
        };
        let src = frame
            .get_attribute("data-src")
            .or_else(|| frame.get_attribute("src"))
            .filter(|s| !s.is_empty() && s != BLANK);
        let Some(src) = src else { continue };
        // A frame the page wrote with `src` may have begun to load. Near the screen and within
        // the limit it keeps loading; any other is stopped and loads in its turn, so every frame
        // follows the same bound.
        let _ = frame.remove_attribute("loading");
        let started = frame.get_attribute("src").is_some_and(|s| s != BLANK);
        let keep = started
            && frames.iter().filter(|f: &&Frame| f.loaded).count() < live_limit()
            && distance(&frame) <= screen() / 2.0;
        if started && !keep {
            frame.set_src(BLANK);
        }
        let _ = frame.set_attribute("data-docs-state", if keep { "loaded" } else { "waiting" });
        frames.push(Frame {
            frame,
            src,
            near: keep,
            loaded: keep,
        });
    }
    if frames.is_empty() {
        return;
    }
    let near = observer(NEAR, |at, entry| {
        FRAMES.with(|f| f.borrow_mut()[at].near = entry.is_intersecting());
    });
    let far = observer(FAR, |at, entry| {
        if !entry.is_intersecting() {
            unload(at);
        }
    });
    let (Some(near), Some(far)) = (near, far) else {
        // Without the observer every frame loads, as the page wrote it.
        for f in &frames {
            f.frame.set_src(&f.src);
            let _ = f.frame.set_attribute("data-docs-state", "loaded");
        }
        return;
    };
    for f in &frames {
        // The frame is scaled with a transform; its box in the page is its parent's.
        let target = f
            .frame
            .parent_element()
            .unwrap_or_else(|| f.frame.clone().into());
        near.observe(&target);
        far.observe(&target);
    }
    FRAMES.with(|slot| *slot.borrow_mut() = frames);
    std::mem::forget(near);
    std::mem::forget(far);
}

fn observer(
    margin: &str,
    act: fn(usize, &IntersectionObserverEntry),
) -> Option<IntersectionObserver> {
    let callback = Closure::<dyn FnMut(js_sys::Array)>::new(move |entries: js_sys::Array| {
        for entry in entries.iter() {
            let entry: IntersectionObserverEntry = entry.unchecked_into();
            let target = entry.target();
            let at = FRAMES.with(|frames| {
                frames.borrow().iter().position(|f| {
                    let frame: &Element = f.frame.as_ref();
                    *frame == target || f.frame.parent_element().as_ref() == Some(&target)
                })
            });
            if let Some(at) = at {
                act(at, &entry);
            }
        }
        next();
    });
    let options = web_sys::IntersectionObserverInit::new();
    options.set_root_margin(margin);
    let made =
        IntersectionObserver::new_with_options(callback.as_ref().unchecked_ref(), &options).ok()?;
    callback.forget();
    Some(made)
}

fn screen() -> f64 {
    window()
        .inner_height()
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(800.0)
}

/// Distance of a frame's box from the viewport, in CSS pixels; 0 when it shows.
fn distance(frame: &HtmlIFrameElement) -> f64 {
    let screen = screen();
    let target = frame
        .parent_element()
        .unwrap_or_else(|| frame.clone().into());
    let r = target.get_bounding_client_rect();
    if r.bottom() < 0.0 {
        -r.bottom()
    } else if r.top() > screen {
        r.top() - screen
    } else {
        0.0
    }
}

/// Start the nearest waiting frame that is near the screen, if none is loading.
fn next() {
    if LOADING.with(Cell::get).is_some() {
        return;
    }
    let pick = FRAMES.with(|frames| {
        frames
            .borrow()
            .iter()
            .enumerate()
            .filter(|(_, f)| f.near && !f.loaded)
            .map(|(i, f)| (i, distance(&f.frame)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    });
    let Some(at) = pick else { return };
    if make_room() {
        load(at);
    }
}

/// Unload the farthest loaded frames that are not near the screen until one more fits under the
/// limit; `false` when every loaded frame is near and the next must wait.
fn make_room() -> bool {
    loop {
        let (full, farthest) = FRAMES.with(|frames| {
            let frames = frames.borrow();
            let full = frames.iter().filter(|f| f.loaded).count() >= live_limit();
            let farthest = frames
                .iter()
                .enumerate()
                .filter(|(_, f)| f.loaded && !f.near)
                .map(|(i, f)| (i, distance(&f.frame)))
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i);
            (full, farthest)
        });
        match (full, farthest) {
            (false, _) => return true,
            (true, Some(at)) => unload(at),
            (true, None) => return false,
        }
    }
}

fn load(at: usize) {
    let ticket = TICKET.with(|t| {
        t.set(t.get().wrapping_add(1));
        t.get()
    });
    LOADING.with(|l| l.set(Some(at)));
    let frame = FRAMES.with(|frames| {
        let mut frames = frames.borrow_mut();
        let f = &mut frames[at];
        f.loaded = true;
        f.frame.clone()
    });
    let src = FRAMES.with(|frames| frames.borrow()[at].src.clone());
    let _ = frame.set_attribute("data-docs-state", "loading");
    let done = move || {
        if TICKET.with(Cell::get) != ticket {
            return;
        }
        LOADING.with(|l| l.set(None));
        let still = FRAMES.with(|frames| frames.borrow().get(at).is_some_and(|f| f.loaded));
        if still {
            let _ = frame.set_attribute("data-docs-state", "loaded");
        }
        next();
    };
    let on_load = Closure::<dyn FnMut()>::new(done.clone());
    let target = FRAMES.with(|frames| frames.borrow()[at].frame.clone());
    target.set_onload(Some(on_load.as_ref().unchecked_ref()));
    on_load.forget();
    let fallback = Closure::once_into_js(done);
    let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(
        fallback.unchecked_ref(),
        LOAD_WAIT_MS,
    );
    target.set_src(&src);
}

fn unload(at: usize) {
    let frame = FRAMES.with(|frames| {
        let mut frames = frames.borrow_mut();
        let f = frames.get_mut(at)?;
        if !f.loaded {
            return None;
        }
        f.loaded = false;
        Some(f.frame.clone())
    });
    let Some(frame) = frame else { return };
    if LOADING.with(Cell::get) == Some(at) {
        LOADING.with(|l| l.set(None));
        TICKET.with(|t| t.set(t.get().wrapping_add(1)));
    }
    frame.set_onload(None);
    frame.set_src(BLANK);
    let _ = frame.set_attribute("data-docs-state", "waiting");
}
