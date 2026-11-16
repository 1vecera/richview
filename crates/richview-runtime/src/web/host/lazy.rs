//! Progressive, memory-bounded mounting (docs/MOTION.md, ARC-18). The runtime mounts one chart
//! at a time, the one nearest the screen first, so a page of many charts never freezes while they
//! lay out. Only a chart within one screen of the viewport holds a drawn canvas, its mesh and its
//! GPU buffers; a chart three screens away gives them back and draws again from its model when
//! it comes near. Until a chart is drawn its frame shows the loading state. `settled()`, print and
//! outputs other than the interactive report mount and draw every chart.

use super::{VIEWS, View, mount_as};
use crate::web::{document, window};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{Element, IntersectionObserver, IntersectionObserverEntry};

/// How far from the viewport a chart is drawn, and how far it may go before it lets go of its
/// drawing. The gap keeps a chart scrolled back and forth from being drawn again and again.
const NEAR: &str = "100% 0px";
const FAR: &str = "300% 0px";
/// A far chart waits this long after the previous mount, leaving frames to the reader's scroll.
const BACKGROUND_MS: i32 = 40;
/// A scheduled mount that no animation frame has run after this long runs from a timer. Many
/// frames long, so a page that is drawing mounts in a frame even when it is slow.
const FRAMELESS_MS: i32 = 250;

thread_local! {
    /// Figures not mounted yet, in document order.
    static QUEUE: RefCell<Vec<Element>> = const { RefCell::new(Vec::new()) };
    static SCHEDULED: Cell<bool> = const { Cell::new(false) };
    /// Names the scheduled mount, so a frame or timer left over from an earlier one does nothing.
    static TICKET: Cell<u32> = const { Cell::new(0) };
    /// Every chart is mounted and drawn from now on: `settled()`, print or a non-interactive output.
    static EVERY: Cell<bool> = const { Cell::new(false) };
    static NEAR_WATCH: RefCell<Option<IntersectionObserver>> = const { RefCell::new(None) };
    static FAR_WATCH: RefCell<Option<IntersectionObserver>> = const { RefCell::new(None) };
}

/// Queue the page's figures for mounting. Returns `false` when this browser cannot watch them,
/// and the caller mounts them all at once.
pub(super) fn start(figures: Vec<Element>) -> bool {
    let near = observer(NEAR, |entry, view| {
        if entry.is_intersecting() {
            wake(view);
        }
    });
    let far = observer(FAR, |entry, view| {
        if !entry.is_intersecting() {
            sleep(view);
        }
    });
    let (Some(near), Some(far)) = (near, far) else {
        return false;
    };
    NEAR_WATCH.with(|slot| *slot.borrow_mut() = Some(near));
    FAR_WATCH.with(|slot| *slot.borrow_mut() = Some(far));
    QUEUE.with(|queue| *queue.borrow_mut() = figures);
    // Scroll events come before the frame is drawn, and before the observers report: a jump to
    // a far chart (a link, a key, a flick) draws it in the frame that first shows it. Capture
    // also hears scrolling inside a scrolled box.
    let listener = Closure::<dyn FnMut()>::new(scrolled);
    let _ = window().add_event_listener_with_callback_and_bool(
        "scroll",
        listener.as_ref().unchecked_ref(),
        true,
    );
    listener.forget();
    schedule(0);
    true
}

/// Draw the sleeping charts and mount the waiting ones that the scroll brought near.
fn scrolled() {
    if every() {
        return;
    }
    let screen = screen();
    let sleeping: Vec<_> = VIEWS.with(|views| {
        views
            .borrow()
            .iter()
            .filter(|v| v.borrow().asleep)
            .cloned()
            .collect()
    });
    for view in sleeping {
        let figure = view.borrow().figure.clone();
        if distance(&figure, screen) <= screen {
            wake(&view);
        }
    }
    let near: Vec<Element> = QUEUE.with(|queue| {
        let mut queue = queue.borrow_mut();
        let mut near = Vec::new();
        queue.retain(|figure| {
            let close = near.len() < 3 && distance(figure, screen) == 0.0;
            if close {
                near.push(figure.clone());
            }
            !close
        });
        near
    });
    for figure in near {
        mount_one(&figure);
    }
}

/// Whether every chart is to be mounted and drawn.
pub(super) fn every() -> bool {
    EVERY.with(Cell::get)
}

/// Mount every waiting chart and draw every sleeping one, now, and keep them drawn: for
/// `settled()`, print and outputs other than the interactive report.
pub(super) fn mount_every() {
    EVERY.with(|every| every.set(true));
    if let Some(far) = FAR_WATCH.with(|slot| slot.borrow_mut().take()) {
        far.disconnect();
    }
    let waiting = QUEUE.with(|queue| std::mem::take(&mut *queue.borrow_mut()));
    for figure in waiting {
        mount_one(&figure);
    }
    let views = VIEWS.with(|views| views.borrow().clone());
    for view in views {
        wake(&view);
    }
}

/// Mount `figure` now if it is still waiting, as a scroll story does before it moves the chart.
pub(super) fn mount_now(figure: &Element) {
    let waiting = QUEUE.with(|queue| {
        let mut queue = queue.borrow_mut();
        let at = queue.iter().position(|f| f == figure)?;
        Some(queue.remove(at))
    });
    if let Some(figure) = waiting {
        mount_one(&figure);
    }
}

/// The figures still waiting to mount, as `pending()` names them (MOTION.md).
pub(super) fn waiting() -> Vec<String> {
    QUEUE.with(|queue| {
        queue
            .borrow()
            .iter()
            .map(|figure| format!("mount {}", figure.id()))
            .collect()
    })
}

fn observer(
    margin: &str,
    act: fn(&IntersectionObserverEntry, &Rc<RefCell<View>>),
) -> Option<IntersectionObserver> {
    let callback = Closure::<dyn FnMut(js_sys::Array)>::new(move |entries: js_sys::Array| {
        for entry in entries.iter() {
            let entry: IntersectionObserverEntry = entry.unchecked_into();
            let target = entry.target();
            let view = VIEWS.with(|views| {
                views
                    .borrow()
                    .iter()
                    .find(|v| {
                        let frame: Element = v.borrow().frame.clone().into();
                        frame == target
                    })
                    .cloned()
            });
            if let Some(view) = view {
                act(&entry, &view);
            }
        }
    });
    let options = web_sys::IntersectionObserverInit::new();
    options.set_root_margin(margin);
    // The page's own viewport, also inside a frame: an implicit root would ignore the margin in
    // a cross-origin frame and measure against the top page instead.
    let _ = js_sys::Reflect::set(&options, &"root".into(), &document());
    let made = IntersectionObserver::new_with_options(callback.as_ref().unchecked_ref(), &options)
        .or_else(|_| {
            let _ = js_sys::Reflect::delete_property(&options, &"root".into());
            IntersectionObserver::new_with_options(callback.as_ref().unchecked_ref(), &options)
        })
        .ok()?;
    callback.forget();
    Some(made)
}

/// The next mount: at the next frame for a chart near the screen, after a pause for one that is
/// not. A hidden document can suspend animation frames, so a timer stands behind the frame and
/// mounts the chart when no frame has come. One mount is scheduled at a time, under one ticket:
/// whichever of the frame and the timer comes first mounts, and the other finds its ticket spent.
fn schedule(delay: i32) {
    if SCHEDULED.with(|s| s.replace(true)) {
        return;
    }
    let ticket = TICKET.with(|t| {
        let next = t.get().wrapping_add(1);
        t.set(next);
        next
    });
    let frame = move || {
        let step = Closure::once_into_js(move || step_once(ticket, false));
        let _ = window().request_animation_frame(step.unchecked_ref());
    };
    if delay > 0 {
        after(delay, frame);
    } else {
        frame();
    }
    after(delay + FRAMELESS_MS, move || step_once(ticket, true));
}

fn after(delay: i32, run: impl FnOnce() + 'static) {
    let run = Closure::once_into_js(run);
    let _ =
        window().set_timeout_with_callback_and_timeout_and_arguments_0(run.unchecked_ref(), delay);
}

/// Run the mount scheduled under `ticket`, unless it has run or a later one took its place.
fn step_once(ticket: u32, frameless: bool) {
    if SCHEDULED.with(Cell::get) && TICKET.with(Cell::get) == ticket {
        step(frameless);
    }
}

/// Mount the waiting chart nearest the screen, then schedule the next. After a mount no frame
/// ran, the next one pauses too: on a page that was only slow, the frame that is still due then
/// passes before the next mount asks for one, and no two charts mount between two paints.
fn step(frameless: bool) {
    SCHEDULED.with(|s| s.set(false));
    let screen = screen();
    let picked = QUEUE.with(|queue| {
        let mut queue = queue.borrow_mut();
        queue.retain(|figure| figure.is_connected());
        let (at, distance) = queue
            .iter()
            .enumerate()
            .map(|(i, f)| (i, distance(f, screen)))
            .fold(None, |best: Option<(usize, f64)>, (i, d)| match best {
                Some((_, b)) if b <= d => best,
                _ => Some((i, d)),
            })?;
        Some((queue.remove(at), distance))
    });
    let Some((figure, _)) = picked else {
        return;
    };
    mount_one(&figure);
    // The one that comes next decides how soon.
    let next = QUEUE.with(|queue| {
        queue
            .borrow()
            .iter()
            .map(|f| distance(f, screen))
            .fold(f64::INFINITY, f64::min)
    });
    if next.is_finite() {
        schedule(if next <= screen && !frameless {
            0
        } else {
            BACKGROUND_MS
        });
    }
}

fn mount_one(figure: &Element) {
    let asleep = !every() && distance(figure, screen()) > screen();
    match mount_as(figure, asleep) {
        Ok(Some(view)) => watch(&view),
        Ok(None) => {}
        Err(e) => web_sys::console::warn_1(&e),
    }
}

fn watch(view: &Rc<RefCell<View>>) {
    let frame = view.borrow().frame.clone();
    NEAR_WATCH.with(|slot| {
        if let Some(near) = slot.borrow().as_ref() {
            near.observe(&frame);
        }
    });
    FAR_WATCH.with(|slot| {
        if let Some(far) = slot.borrow().as_ref() {
            far.observe(&frame);
        }
    });
}

fn screen() -> f64 {
    window()
        .inner_height()
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(800.0)
}

/// How far `figure` lies outside the viewport, in CSS pixels; 0 when it shows, and a chart that
/// is not rendered (a closed tab or section) comes last.
fn distance(figure: &Element, screen: f64) -> f64 {
    if figure.get_client_rects().length() == 0 {
        return f64::MAX;
    }
    let r = figure.get_bounding_client_rect();
    if r.bottom() < 0.0 {
        -r.bottom()
    } else if r.top() > screen {
        r.top() - screen
    } else {
        0.0
    }
}

/// Draw a sleeping chart again from its model.
fn wake(view: &Rc<RefCell<View>>) {
    if !view.borrow().asleep {
        return;
    }
    view.borrow_mut().asleep = false;
    super::restore_surface(view);
}

/// Give back a far chart's canvas, mesh and GPU buffers; its model and reading state stay. A
/// chart the reader is using, playing or expanded keeps its drawing.
fn sleep(view: &Rc<RefCell<View>>) {
    if every() {
        return;
    }
    let busy = {
        let v = view.borrow();
        let focused = document()
            .active_element()
            .is_some_and(|active| v.figure.contains(Some(&active)));
        v.asleep
            || v.expanded.is_some()
            || v.owner.is_some()
            || v.playing.is_some()
            || v.glide.is_some()
            || v.zoom_glide.is_some()
            || v.dialog.is_some()
            || focused
    };
    // The page's own viewport decides, also where an observer measures against another one: in
    // a cross-origin frame offscreen, Chromium reports even a chart the frame shows as far.
    let screen = screen();
    let far = distance(&view.borrow().figure, screen) > 3.0 * screen;
    if busy || !far {
        return;
    }
    let mut v = view.borrow_mut();
    v.asleep = true;
    super::release_surface(&mut v);
}
