//! Interaction feedback (FEA-CH-52, FEA-CH-53): quick to arrive, slow to let go. Hovering or
//! selecting a mark, or pointing at a legend entry, dims the rest in 150 ms and restores it in
//! 250 ms. Dark pages keep a larger share of each dimmed mark; hover feedback arrives in 120 ms
//! and releases in 250 ms;
//! drag-zoom and resets glide over 600 ms. Wheel, key and pinch zoom apply at once. Nothing
//! moves on hover. Under reduced motion every change lands at once. During a zoom glide no
//! readout shows, so no value between real observations is ever read.

use super::{Target, View, draw, layout, rebuild_mesh, reduced_motion, set_zoom, with_renderer};
use crate::mesh::{self, Focus, State};
use crate::web::{check_settled, now, window};
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

const EMPHASIS_IN: f64 = 150.0;
const EMPHASIS_OUT: f64 = 250.0;
pub(super) const HOVER: f64 = 120.0;
/// How long hover feedback takes to let go.
pub(super) const HOVER_OUT: f64 = 250.0;
/// How long the readout fades once its hold ends (FEA-CH-42).
pub(super) const READOUT_FADE: i32 = 100;
const ZOOM: f64 = 600.0;

/// The cubic ease the state glides use.
pub(super) fn ease(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

fn ease_out(t: f64) -> f64 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

/// When an ease-out that has reached `level` began, for one `length` long.
fn began(level: f64, length: f64) -> f64 {
    now() - length * (1.0 - (1.0 - level.clamp(0.0, 1.0)).cbrt())
}

/// The emphasis as drawn: what stands out and how far the rest has dimmed.
#[derive(Debug, Clone, Default)]
pub(super) struct Fade {
    focus: Option<Focus>,
    floor: f32,
    node_floor: f32,
    from: f32,
    to: f32,
    start: f64,
    length: f64,
    /// When the emphasis moved to `focus` from other marks, which it then returns from.
    arrived: Option<f64>,
    /// The marks emphasised before, and when the emphasis left them.
    before: Option<(Focus, f64)>,
}

impl Fade {
    fn level(&self, at: f64) -> f32 {
        let t = if self.length <= 0.0 {
            1.0
        } else {
            ease_out((at - self.start) / self.length)
        };
        self.from + (self.to - self.from) * t as f32
    }

    fn running(&self, at: f64) -> bool {
        at - self.start < self.length
            || self.arrived.is_some_and(|t| at - t < EMPHASIS_IN)
            || self
                .before
                .as_ref()
                .is_some_and(|(_, t)| at - t < EMPHASIS_OUT)
    }

    /// Keep the emphasis on `focus` after a new layout, without a new fade; none ends it.
    pub(super) fn refocus(&mut self, focus: Option<(Focus, f32)>) {
        self.arrived = None;
        self.before = None;
        match focus {
            Some((focus, floor)) => {
                self.focus = Some(focus);
                self.floor = floor * self.node_floor / mesh::DIM_FLOOR;
            }
            None => *self = Fade::default(),
        }
    }

    /// What the mesh draws now.
    pub(super) fn state(&self) -> State {
        let at = now();
        let dim = self.level(at);
        if dim <= 0.0 {
            return State::default();
        }
        State {
            emphasis: self.focus.clone(),
            dim,
            floor: self.floor,
            node_floor: self.node_floor,
            arrive: self
                .arrived
                .map_or(1.0, |t| ease_out((at - t) / EMPHASIS_IN) as f32),
            leaving: self.before.as_ref().and_then(|(focus, t)| {
                let held = 1.0 - ease_out((at - t) / EMPHASIS_OUT) as f32;
                (held > 0.0).then(|| (focus.clone(), held))
            }),
        }
    }
}

/// A hover target the pointer has left, still letting go.
#[derive(Debug, Clone, Copy)]
pub(super) struct Released {
    pub(super) target: Target,
    /// The feedback's level when it was left, and when.
    from: f64,
    at: f64,
}

impl Released {
    pub(super) fn level(&self) -> f64 {
        self.from * (1.0 - ease_out((now() - self.at) / HOVER_OUT))
    }
}

/// A zoom on its way from one window to another; `end` is the zoom it settles on, `None` for
/// the whole series.
#[derive(Debug, Clone, Copy)]
pub(super) struct ZoomGlide {
    from: [f64; 2],
    to: [f64; 2],
    start: f64,
    end: Option<[f64; 2]>,
}

impl ZoomGlide {
    /// The window drawn now.
    pub(super) fn window(&self) -> [f64; 2] {
        let t = ease((now() - self.start) / ZOOM);
        [
            self.from[0] + (self.to[0] - self.from[0]) * t,
            self.from[1] + (self.to[1] - self.from[1]) * t,
        ]
    }

    fn done(&self) -> bool {
        now() - self.start >= ZOOM
    }

    /// The zoom the glide settles on; `None` for the whole series.
    pub(super) fn end(&self) -> Option<[f64; 2]> {
        self.end
    }
}

/// The hover feedback's eased level, 0 to 1: it arrives in 120 ms and, once the pointer has
/// left, lets go in 250 ms. Reduced motion lands immediately.
pub(super) fn grown(v: &View) -> f64 {
    if reduced_motion() {
        return if v.letting_go.is_some() { 0.0 } else { 1.0 };
    }
    let arrived = ease_out((now() - v.hover_at) / HOVER);
    match v.letting_go {
        Some(at) => arrived * (1.0 - ease_out((now() - at) / HOVER_OUT)),
        None => arrived,
    }
}

/// The crosshair arrives immediately, then releases without an arrival-level jump.
pub(super) fn crosshair_level(v: &View) -> f64 {
    match v.letting_go {
        Some(_) if reduced_motion() => 0.0,
        Some(at) => 1.0 - ease_out((now() - at) / HOVER_OUT),
        None => 1.0,
    }
}

/// Whether any feedback is still moving.
pub(super) fn busy(v: &View) -> bool {
    let at = now();
    v.fade.running(at)
        || v.zoom_glide.is_some()
        || at - v.hover_at < HOVER
        || v.letting_go.is_some_and(|t| at - t < HOVER_OUT)
        || !v.released.is_empty()
}

/// Emphasise `focus`, or nothing, fading the rest to `floor` of its strength (FEA-CH-52).
pub(super) fn emphasise(view: &Rc<RefCell<View>>, focus: Option<(Focus, f32)>) {
    {
        let mut v = view.borrow_mut();
        let key = focus.as_ref().map(|(f, _)| f.clone());
        if v.emphasis == key {
            return;
        }
        v.emphasis = key;
        super::readout_rows_follow_emphasis(&v);
        let at = now();
        let level = v.fade.level(at);
        let still = reduced_motion();
        let old = v.fade.clone();
        let page = &v.chart.payload.theme.page;
        v.fade = match focus {
            Some((focus, floor)) => {
                // Moving between marks while the rest is dimmed: the rest stays back, the
                // marks left ease down and the marks reached ease up.
                let moved =
                    level > 0.0 && !still && old.focus.as_ref().is_some_and(|f| *f != focus);
                Fade {
                    floor: mesh::page_floor(floor, page),
                    node_floor: mesh::page_floor(mesh::DIM_FLOOR, page),
                    from: level,
                    to: 1.0,
                    start: at,
                    length: if still { 0.0 } else { EMPHASIS_IN },
                    arrived: moved.then_some(at),
                    before: if moved {
                        old.focus.map(|f| (f, at))
                    } else {
                        None
                    },
                    focus: Some(focus),
                }
            }
            // Fading back keeps what it was marking until the rest is whole again.
            None => Fade {
                from: level,
                to: 0.0,
                start: at,
                length: if still { 0.0 } else { EMPHASIS_OUT },
                ..old
            },
        };
        remesh(&mut v);
    }
    draw(view);
    run(view);
}

/// The hover target changed from `old`: the new mark's feedback arrives and the one left
/// lets go. Along a line the dots follow the pointer and leave no trail.
pub(super) fn hovered(view: &Rc<RefCell<View>>, old: Option<Target>, trail: bool) {
    {
        let mut v = view.borrow_mut();
        let level = grown(&v);
        let at = now();
        if let Some(target) = old
            && trail
            && level > 0.01
            && !reduced_motion()
        {
            // A few at most: a sweep across many marks keeps only the latest releases.
            if v.released.len() >= 4 {
                v.released.remove(0);
            }
            v.released.push(Released {
                target,
                from: level,
                at,
            });
        }
        let follows = old.is_some() && !trail && v.letting_go.is_none();
        v.letting_go = None;
        if v.target.is_some() && !follows {
            v.hover_at = at;
        }
        if !busy(&v) {
            return;
        }
    }
    run(view);
}

/// The pointer left the chart: the feedback starts to let go while the readout holds.
pub(super) fn let_go(view: &Rc<RefCell<View>>) {
    {
        let mut v = view.borrow_mut();
        if v.target.is_none() || v.letting_go.is_some() {
            return;
        }
        v.letting_go = Some(now());
    }
    run(view);
}

/// The pointer came back before the readout went: the feedback returns from where it was.
pub(super) fn resume(view: &Rc<RefCell<View>>) {
    {
        let mut v = view.borrow_mut();
        if v.letting_go.is_none() {
            return;
        }
        let level = grown(&v);
        v.letting_go = None;
        v.hover_at = began(level, HOVER);
    }
    run(view);
}

/// Finish feedback immediately when the reader turns reduced motion on mid-interaction.
pub(super) fn settle(view: &Rc<RefCell<View>>) {
    let zoom = {
        let mut v = view.borrow_mut();
        v.fade.from = v.fade.to;
        v.fade.length = 0.0;
        v.hover_at = now() - HOVER;
        v.released.clear();
        v.fade.arrived = None;
        v.fade.before = None;
        v.animating = false;
        let zoom = v.zoom_glide.take().map(|g| g.end);
        remesh(&mut v);
        zoom
    };
    match zoom {
        Some(end) => set_zoom(view, end),
        None => draw(view),
    }
}

/// Zoom to `zoom`, or back to the whole series, gliding there (FEA-CH-52).
pub(super) fn zoom_to(view: &Rc<RefCell<View>>, zoom: Option<[f64; 2]>) {
    let zoom = {
        let v = view.borrow();
        zoom.and_then(|window| {
            v.chart
                .clamp_zoom(window, v.geometry.panels.first()?.x.as_ref()?)
        })
    };
    let from = {
        let v = view.borrow();
        v.geometry
            .panels
            .first()
            .and_then(|p| p.x.as_ref())
            .map(|s| (s.domain, s.extent))
    };
    let Some((from, extent)) = from else {
        set_zoom(view, zoom);
        return;
    };
    let to = zoom.unwrap_or(extent);
    if reduced_motion() || from == to {
        set_zoom(view, zoom);
        return;
    }
    {
        let mut v = view.borrow_mut();
        v.zoom_glide = Some(ZoomGlide {
            from,
            to,
            start: now(),
            end: zoom,
        });
        v.target = None;
        v.pinned = false;
        v.readout.set_hidden(true);
    }
    run(view);
}

fn remesh(v: &mut View) {
    let state = v.fade.state();
    let dpr = v.dpr;
    rebuild_mesh(v, &state, dpr);
    if let Some(up) = v.uploaded.take() {
        with_renderer(|r| r.release(up));
    }
}

/// Draw frames while feedback moves; a finished zoom glide settles on its zoom.
fn run(view: &Rc<RefCell<View>>) {
    if view.borrow().animating {
        return;
    }
    view.borrow_mut().animating = true;
    frame(Rc::downgrade(view));
}

fn frame(weak: Weak<RefCell<View>>) {
    let Some(view) = weak.upgrade() else { return };
    let glide = view.borrow().zoom_glide;
    match glide {
        Some(g) if g.done() => {
            view.borrow_mut().zoom_glide = None;
            set_zoom(&view, g.end);
        }
        Some(_) => layout(&view, false),
        None => {
            view.borrow_mut().released.retain(|r| r.level() > 0.0);
            // A settled emphasis needs no new mesh while only the hover feedback moves.
            let fading = view.borrow().fade.running(now());
            if fading {
                remesh(&mut view.borrow_mut());
            }
            draw(&view);
        }
    }
    if !busy(&view.borrow()) {
        // One last frame at the resting state.
        remesh(&mut view.borrow_mut());
        draw(&view);
        view.borrow_mut().animating = false;
        check_settled();
        return;
    }
    let next = Closure::once_into_js(move || frame(weak));
    let _ = window().request_animation_frame(next.unchecked_ref());
}
