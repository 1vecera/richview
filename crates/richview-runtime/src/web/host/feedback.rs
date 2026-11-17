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
    /// The same along y, on a chart that zooms along both axes.
    y: Option<GlideY>,
}

/// A zoom glide's window along y: where it starts, where it goes and the zoom it settles on.
#[derive(Debug, Clone, Copy)]
struct GlideY {
    from: [f64; 2],
    to: [f64; 2],
    end: Option<[f64; 2]>,
}

impl ZoomGlide {
    /// The window along y drawn now, on a chart that zooms along both axes.
    pub(super) fn window_y(&self) -> Option<[f64; 2]> {
        let GlideY { from, to, .. } = self.y?;
        let t = ease((now() - self.start) / ZOOM);
        Some([
            from[0] + (to[0] - from[0]) * t,
            from[1] + (to[1] - from[1]) * t,
        ])
    }

    /// The window along y the glide settles on.
    fn end_y(&self) -> Option<[f64; 2]> {
        self.y.and_then(|y| y.end)
    }

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

/// Names inside the plot would jump from frame to frame of a zoom, since each frame places them
/// afresh. They leave instead: they fade out in 100 ms when a zoom glide starts (at once when the
/// window jumps), stay away while the window moves, and fade in over 150 ms on the settled view
/// once it has rested 120 ms (chart design, 16 Nov 2026).
const NAMES_OUT: f64 = 100.0;
const NAMES_IN: f64 = 150.0;
const NAMES_QUIET: f64 = 120.0;

/// Where the names inside the plot are in their fade: leaving since `out`, last moved at
/// `moved`, coming back since `back`.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct NameFade {
    out: Option<f64>,
    moved: f64,
    back: Option<f64>,
}

impl NameFade {
    /// The names' strength now, 0 to 1.
    pub(super) fn level(&self) -> f64 {
        let at = now();
        match (self.out, self.back) {
            (Some(t), _) => 1.0 - ease_out((at - t) / NAMES_OUT),
            (None, Some(t)) => ease_out((at - t) / NAMES_IN),
            _ => 1.0,
        }
    }

    fn running(&self) -> bool {
        self.out.is_some() || self.back.is_some()
    }

    /// Bring the names back once the window has rested, and end a finished return.
    fn step(&mut self) {
        let at = now();
        if self.out.is_some() && at - self.moved >= NAMES_QUIET {
            self.out = None;
            self.back = Some(at);
        }
        if self.back.is_some_and(|t| at - t >= NAMES_IN) {
            self.back = None;
        }
    }
}

/// A zoom moved the window of a chart that may name its lines inside the plot: the names leave,
/// fading when a glide starts and at once when the window jumps.
pub(super) fn names_moved(v: &mut View, gliding: bool) {
    let named = matches!(v.geometry.kind.as_str(), "line" | "area") && v.geometry.series.len() > 1;
    if reduced_motion() || !named {
        return;
    }
    let at = now();
    if v.names.out.is_none() {
        // A return still under way turns back from where it is.
        let level = v.names.level();
        v.names.out = Some(if gliding {
            began(1.0 - level, NAMES_OUT)
        } else {
            at - NAMES_OUT
        });
    }
    v.names.back = None;
    v.names.moved = at;
}

/// Draw frames until the names are back, after a zoom that set its window at once.
pub(super) fn names_return(view: &Rc<RefCell<View>>) {
    if view.borrow().names.running() {
        run(view);
    }
}

/// The geometry with its names inside the plot drawn at `level` of their strength.
pub(super) fn faded_names(
    g: &richview_chart::geometry::Geometry,
    level: f64,
) -> richview_chart::geometry::Geometry {
    use richview_chart::geometry::{Face, Item, Role};
    let mut g = g.clone();
    let names: Vec<String> = g.series.iter().map(|s| s.name.clone()).collect();
    for panel in &mut g.panels {
        let plot = panel.plot;
        for item in &mut panel.items {
            if let Item::Text(t) = item
                && t.role == Role::Label
                && t.face == Face::Data
                && t.series
                    .and_then(|s| names.get(s))
                    .is_some_and(|n| *n == t.text)
                && t.bounds.inside(&plot, 0.5)
            {
                t.color = t.color.with_alpha(f64::from(t.color.0[3]) * level);
            }
        }
    }
    g
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
        || v.names.running()
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
        v.names = NameFade::default();
        let glide = v.zoom_glide.take();
        if let Some(g) = glide {
            v.chart.zoom_y = g.end_y();
        }
        let zoom = glide.map(|g| g.end);
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
    zoom_to_window(view, zoom, None);
}

/// [`zoom_to`] with the window along y as well, for a chart that zooms along both axes (a
/// map); elsewhere `y` is ignored.
pub(super) fn zoom_to_window(
    view: &Rc<RefCell<View>>,
    zoom: Option<[f64; 2]>,
    y: Option<[f64; 2]>,
) {
    let span = {
        let v = view.borrow();
        v.geometry
            .panels
            .first()
            .and_then(|p| p.x.as_ref())
            .and_then(|s| s.y)
    };
    // The window along y settles with the zoom along x: it is the chart's from here on, and
    // each frame of the glide replaces it with the window of the moment.
    let y = span.and(zoom).and(y);
    let before = std::mem::replace(&mut view.borrow_mut().chart.zoom_y, y);
    let zoom = {
        let v = view.borrow();
        zoom.and_then(|window| {
            v.chart
                .clamp_zoom(window, v.geometry.panels.first()?.x.as_ref()?)
        })
    };
    let y = zoom.and(y);
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
    let glide_y = span.map(|s| GlideY {
        from: s.domain,
        to: y.unwrap_or(s.extent),
        end: y,
    });
    if reduced_motion() || (from == to && glide_y.is_none_or(|g| g.from == g.to)) {
        view.borrow_mut().chart.zoom_y = y;
        set_zoom(view, zoom);
        return;
    }
    {
        let mut v = view.borrow_mut();
        // The frame drawn now still shows the window the glide leaves.
        v.chart.zoom_y = before;
        v.zoom_glide = Some(ZoomGlide {
            from,
            to,
            start: now(),
            end: zoom,
            y: glide_y,
        });
        v.target = None;
        v.pinned = false;
        v.readout.set_hidden(true);
        names_moved(&mut v, true);
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
            {
                let mut v = view.borrow_mut();
                v.zoom_glide = None;
                if g.y.is_some() {
                    v.chart.zoom_y = g.end_y();
                }
            }
            set_zoom(&view, g.end);
            // The glide has settled: its names come back on this view from the next frame.
            let mut v = view.borrow_mut();
            v.names.moved = now() - NAMES_QUIET;
        }
        Some(_) => layout(&view, false),
        None => {
            view.borrow_mut().released.retain(|r| r.level() > 0.0);
            let names = {
                let mut v = view.borrow_mut();
                let running = v.names.running();
                v.names.step();
                running
            };
            // A settled emphasis needs no new mesh while only the hover feedback moves.
            let fading = names || view.borrow().fade.running(now());
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
