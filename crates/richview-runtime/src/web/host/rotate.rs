//! Turning a three-dimensional chart (surface, 3-D scatter, 3-D bar): a drag with the mouse, a pen
//! or a finger, Shift with the arrow keys, and the way back to the author's view by double-click,
//! the Reset view action or R. Arrow keys alone keep stepping through the marks (FEA-CH-56).
//!
//! The host feeds angles and lays the chart out again; the projection, the clamping of the
//! elevation and the arithmetic of a turn are the engine's (`richview_chart::layout`), so the
//! hover, readout, highlight and keyboard order of every angle come from the same layout as the
//! author's view.

use super::{View, feedback, keep_reset_place, layout, reduced_motion, set_target, zoom_hint};
use crate::web::{check_settled, document, now, window};
use richview_chart::layout::{KEY_TURN, between_views, turn_by_degrees, turn_by_pixels};
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{Element, KeyboardEvent, PointerEvent};

/// How far a press moves, in pixels, before it is a turn and not a click or a tap.
const SLOP: f64 = 4.0;
/// How long the glide back to the author's view takes.
const GLIDE_MS: f64 = 500.0;

/// What a view holds while its chart turns.
#[derive(Default)]
pub(super) struct Rotation {
    drag: Option<Drag>,
    /// The reader has turned this chart once: they know it turns, so its hint stays away.
    pub(super) taught: bool,
    /// A layout waits for the next frame: pointer moves between two frames make one.
    pub(super) queued: bool,
    glide: Option<Glide>,
}

impl Rotation {
    /// A press that may become a turn is down.
    pub(super) fn pressed(&self) -> bool {
        self.drag.is_some()
    }
}

/// A press on the chart: the pointer, where it came down and where it was last.
struct Drag {
    id: i32,
    start: [f64; 2],
    last: [f64; 2],
    started: bool,
}

/// A glide back to the author's view: the view it leaves and when it began.
#[derive(Clone, Copy)]
struct Glide {
    from: [f64; 2],
    start: f64,
}

/// A chart that turns says so to the hand: it takes the finger for the turn, not for scrolling the
/// page, and shows a grab for the pointer.
pub(super) fn prepare(v: &View) {
    if !v.chart.rotates() {
        return;
    }
    let style = v.canvas.style();
    let _ = style.set_property("touch-action", "none");
    let _ = style.set_property("cursor", "grab");
}

/// A pointer came down on the canvas of a chart that turns; the turn starts once it moves.
pub(super) fn begin(view: &Rc<RefCell<View>>, e: &PointerEvent, p: [f64; 2]) {
    {
        let mut v = view.borrow_mut();
        if !v.chart.rotates() || (e.pointer_type() != "touch" && e.button() != 0) {
            return;
        }
        // A second finger leaves the first turning; the hand takes over from a glide.
        if v.rotation.drag.is_some() {
            return;
        }
        v.rotation.glide = None;
        v.rotation.drag = Some(Drag {
            id: e.pointer_id(),
            start: p,
            last: p,
            started: false,
        });
    }
    // The canvas keeps the pointer until release, also outside the chart.
    if let Some(canvas) = e
        .current_target()
        .and_then(|t| t.dyn_into::<Element>().ok())
    {
        let _ = canvas.set_pointer_capture(e.pointer_id());
    }
}

/// The pointer moved at `p`: turn the chart if a press is turning it. Whether the move belongs to
/// the turn, which then leaves the hover alone.
pub(super) fn drag(view: &Rc<RefCell<View>>, e: &PointerEvent, p: [f64; 2]) -> bool {
    let (seen, delta, first) = {
        let mut v = view.borrow_mut();
        let seen = v.chart.seen_view();
        let Some(d) = v.rotation.drag.as_mut() else {
            return false;
        };
        if d.id != e.pointer_id() {
            return true;
        }
        // A mouse released outside the window with no capture event is not still turning.
        if e.pointer_type() == "mouse" && e.buttons() == 0 {
            v.rotation.drag = None;
            return false;
        }
        let mut first = false;
        if !d.started {
            if (p[0] - d.start[0]).hypot(p[1] - d.start[1]) < SLOP {
                return true;
            }
            d.started = true;
            first = true;
        }
        let delta = [p[0] - d.last[0], p[1] - d.last[1]];
        d.last = p;
        (seen, delta, first)
    };
    if first {
        // The press is a turn: the readout and the selection let go, and the grab closes.
        {
            let mut v = view.borrow_mut();
            v.pinned = false;
            v.rotation.taught = true;
            let _ = v.canvas.style().set_property("cursor", "grabbing");
            zoom_hint(&mut v, false);
        }
        set_target(view, None, false);
    }
    set(view, turn_by_pixels(seen, delta));
    true
}

/// The pointer `id` went up: the turn ends with one layout in full, and whether it was a turn. A
/// finger has no resting pointer to hint to; a mouse and a pen do.
pub(super) fn end(view: &Rc<RefCell<View>>, id: i32, touch: bool) -> bool {
    let started = {
        let mut v = view.borrow_mut();
        match v.rotation.drag.take() {
            Some(d) if d.id == id => d.started,
            other => {
                v.rotation.drag = other;
                return false;
            }
        }
    };
    {
        let mut v = view.borrow_mut();
        let _ = v.canvas.style().set_property("cursor", "grab");
        zoom_hint(&mut v, !touch);
    }
    if started {
        queue(view);
    }
    started
}

/// The pointer was taken away: the press ends where it is.
pub(super) fn cancel(view: &Rc<RefCell<View>>) {
    let Some(d) = view.borrow_mut().rotation.drag.take() else {
        return;
    };
    let _ = view.borrow().canvas.style().set_property("cursor", "grab");
    if d.started {
        queue(view);
    }
}

/// A key on the chart: Shift with an arrow key turns it by [`KEY_TURN`] degrees, as a drag in that
/// direction would, and R returns to the author's view. Whether the key was taken.
pub(super) fn key(view: &Rc<RefCell<View>>, e: &KeyboardEvent) -> bool {
    if !view.borrow().chart.rotates() {
        return false;
    }
    let by = match (e.key().as_str(), e.shift_key()) {
        ("r" | "R", _) => {
            reset(view);
            return true;
        }
        ("ArrowLeft", true) => [-KEY_TURN, 0.0],
        ("ArrowRight", true) => [KEY_TURN, 0.0],
        ("ArrowUp", true) => [0.0, -KEY_TURN],
        ("ArrowDown", true) => [0.0, KEY_TURN],
        _ => return false,
    };
    let seen = view.borrow().chart.seen_view();
    let turned = turn_by_degrees(seen, by);
    set(view, turned);
    announce(
        view,
        &format!(
            "Turned to {:.0} degrees round, {:.0} degrees above.",
            turned[0], turned[1]
        ),
    );
    true
}

/// Back to the author's view: a glide, or at once under reduced motion. Nothing happens to a
/// chart that has not been turned.
pub(super) fn reset(view: &Rc<RefCell<View>>) {
    let (from, to, turned) = {
        let v = view.borrow();
        (
            v.chart.seen_view(),
            v.chart.authored_view(),
            v.chart.view.is_some(),
        )
    };
    if !turned {
        return;
    }
    {
        let mut v = view.borrow_mut();
        show_reset(&mut v, false);
        if reduced_motion() || from == to {
            v.chart.view = None;
            v.rotation.glide = None;
        } else {
            v.rotation.glide = Some(Glide { from, start: now() });
        }
    }
    queue(view);
    announce(view, "View reset.");
}

/// The chart is at `angles` as `[azimuth, elevation]`: its Reset view shows, and the layout
/// follows on the next frame.
fn set(view: &Rc<RefCell<View>>, angles: [f64; 2]) {
    {
        let mut v = view.borrow_mut();
        v.chart.view = Some(angles);
        show_reset(&mut v, true);
    }
    queue(view);
}

/// Show or hide Reset view. Focus on the button that goes away moves to the chart it reset.
fn show_reset(v: &mut View, shown: bool) {
    if let Some(reset) = &v.reset {
        let button: &Element = reset.as_ref();
        let had_focus = document().active_element().as_ref() == Some(button);
        if reset.hidden() != !shown {
            reset.set_hidden(!shown);
        }
        keep_reset_place(v);
        if had_focus && !shown {
            let _ = v.frame.focus();
        }
    }
}

/// Say what changed to a screen reader, which cannot see the chart turn.
fn announce(view: &Rc<RefCell<View>>, text: &str) {
    view.borrow().live.set_text_content(Some(text));
}

/// Lay the chart out on the next frame, once however many moves come before it.
fn queue(view: &Rc<RefCell<View>>) {
    if view.borrow().rotation.queued {
        return;
    }
    view.borrow_mut().rotation.queued = true;
    let weak = Rc::downgrade(view);
    let next = Closure::once_into_js(move || frame(weak));
    let _ = window().request_animation_frame(next.unchecked_ref());
}

fn frame(weak: Weak<RefCell<View>>) {
    let Some(view) = weak.upgrade() else { return };
    let gliding = {
        let mut v = view.borrow_mut();
        v.rotation.queued = false;
        match v.rotation.glide {
            Some(g) => {
                let t = ((now() - g.start) / GLIDE_MS).min(1.0);
                if t >= 1.0 {
                    // The author's view again: the reader's is let go.
                    v.rotation.glide = None;
                    v.chart.view = None;
                } else {
                    let to = v.chart.authored_view();
                    v.chart.view = Some(between_views(g.from, to, feedback::ease(t)));
                }
                v.rotation.glide.is_some()
            }
            None => false,
        }
    };
    // While a pointer or a glide moves the view, frames are drafts; the last layout is whole.
    {
        let mut v = view.borrow_mut();
        let dragging = v.rotation.drag.as_ref().is_some_and(|d| d.started);
        v.chart.draft = gliding || dragging;
    }
    layout(&view, false);
    if gliding {
        queue(&view);
    } else {
        check_settled();
    }
}
