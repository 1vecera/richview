//! The chart host: one view per figure. It lays the chart out at the figure's width, draws it
//! through the shared renderer, and runs reading, inspection, legend, actions, zoom and the
//! entrance (FEA-CH-39 to FEA-CH-56, FEA-CH-71).

mod contour;
mod controls;
mod feedback;
mod focus;
mod hint_place;
mod lazy;
mod overview;
mod readout_place;
mod title;
mod wedge_readout;

use super::atlas;
use super::gl::{Clock, Renderer, Uploaded};
use super::{check_settled, document, el, now, on, query_all, window};
use crate::data;
use crate::mesh::{self, Focus, Mesh, State};
use crate::payload::Chart;
use richview_chart::block::Wider;
use richview_chart::geometry::{
    Face, Fill, Geometry, Item, Rect, Rgba, Role, Shape, Stroke, Targeting,
};
use richview_chart::hit::{self, Step, Target};
use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::{Rc, Weak};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{CanvasRenderingContext2d, Element, HtmlCanvasElement, HtmlElement};

// Stable web-sys truncates these MouseEvent properties to i32, losing fractional
// PointerEvent positions before the chart can hit-test its exact outlines.
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = web_sys::MouseEvent, js_name = MouseEvent)]
    type PreciseMouseEvent;

    #[wasm_bindgen(method, getter, js_name = clientX)]
    fn precise_x(this: &PreciseMouseEvent) -> f64;

    #[wasm_bindgen(method, getter, js_name = clientY)]
    fn precise_y(this: &PreciseMouseEvent) -> f64;
}

fn client_point(e: &web_sys::MouseEvent) -> [f64; 2] {
    let e: &PreciseMouseEvent = e.unchecked_ref();
    [e.precise_x(), e.precise_y()]
}

const NOTICE: &str = "This chart cannot be displayed here: it needs JavaScript and WebGL.";
const TAP_MS: f64 = 500.0;
const TAP_SLOP: f64 = 8.0;

/// A single touch's outside charts, remembered before a scroll or control can move them.
struct OutsideTap {
    pointer: i32,
    at: [f64; 2],
    began: f64,
    views: Vec<Weak<RefCell<View>>>,
}

struct OutsideTouch {
    fingers: BTreeSet<i32>,
    tap: Option<OutsideTap>,
}

/// One mounted chart.
pub struct View {
    figure: Element,
    frame: HtmlElement,
    canvas: HtmlCanvasElement,
    ctx: CanvasRenderingContext2d,
    readout: HtmlElement,
    live: HtmlElement,
    legend: Option<HtmlElement>,
    actions: HtmlElement,
    table: Option<HtmlElement>,
    reset: Option<HtmlElement>,
    chart: Chart,
    width: f64,
    dpr: f64,
    geometry: Geometry,
    /// One replaced layout, released after the input task; a new layout drains it first.
    retired: Option<Geometry>,
    retirement: Option<i32>,
    mesh: Mesh,
    /// Grid-before-marks op boundaries for the mesh currently cached above.
    panel_splits: Vec<Option<usize>>,
    uploaded: Option<Uploaded>,
    hidden: Vec<String>,
    zoom: Option<[f64; 2]>,
    target: Option<Target>,
    pinned: bool,
    /// What the chart emphasises now, and whether a hovered or selected mark asked for it
    /// rather than a legend entry (FEA-CH-52).
    emphasis: Option<Focus>,
    mark_focus: bool,
    side: Option<readout_place::Side>,
    /// Entrance start time; `None` before it plays, and settled once it has finished.
    entrance: Option<f64>,
    settled: bool,
    drag: Option<(f64, f64)>,
    /// The same drag along y, on a chart that zooms along both axes (a map).
    drag_y: Option<(f64, f64)>,
    /// The "Drag to zoom" line above a zoomable plot, made when a mouse first rests on the
    /// chart, and whether it shows now (FEA-CH-49).
    hint: Option<HtmlElement>,
    hint_on: bool,
    /// A Shift-drag pan in progress: where it started and the window then (FEA-CH-49).
    pan: Option<(f64, [f64; 2])>,
    /// The same pan along y, on a chart that zooms along both axes.
    pan_y: Option<(f64, [f64; 2])>,
    /// Scrolling over names not yet turned into a step, in names (Chart design, 16 November).
    scroll_rest: f64,
    touch: Option<(f64, f64, f64)>,
    /// A short, stationary touch that has lifted, waiting for its click to activate.
    tap: Option<[f64; 2]>,
    /// The expanded view's copy of this chart, if open.
    expanded: Option<Rc<RefCell<View>>>,
    dialog: Option<web_sys::HtmlDialogElement>,
    owner: Option<Weak<RefCell<View>>>,
    /// The reader's control row, wired (FEA-BR-01).
    controls: Option<HtmlElement>,
    /// Playback running: its pending timer (FEA-BR-02).
    playing: Option<i32>,
    /// A glide between two states: the geometry it leaves, when it began, how long it lasts in
    /// milliseconds, and whether it keeps one pace throughout, as playback does from frame to
    /// frame (FEA-CH-52, FEA-BR-02).
    glide: Option<(Geometry, f64, f64, bool)>,
    /// The Table lists every row rather than the state's (FEA-BR-32).
    table_all: bool,
    /// The emphasis as drawn, fading in and out (FEA-CH-52).
    fade: feedback::Fade,
    /// When the current hover target arrived; its mark grows in (FEA-CH-52).
    hover_at: f64,
    /// Where the pointer is over the plot, for charts whose layers it picks between.
    pointer: Option<[f64; 2]>,
    /// When the pointer left: the feedback lets go while the readout holds (FEA-CH-52).
    letting_go: Option<f64>,
    /// Targets the pointer has left whose feedback is still letting go.
    released: Vec<feedback::Released>,
    /// A drag-zoom or reset on its way (FEA-CH-52).
    zoom_glide: Option<feedback::ZoomGlide>,
    /// Names inside the plot leave while a zoom moves and come back when it rests.
    names: feedback::NameFade,
    /// Feedback frames are being drawn.
    animating: bool,
    /// A resize waits for the next frame to lay out.
    relayout: bool,
    /// The expanded view's dialog is gliding to its Table or back to the chart (FEA-CH-45): the
    /// glide's number, so a later press or the reader's own scroll ends it.
    scrolling: Option<u32>,
    /// What the legend last showed.
    legend_key: String,
    /// The overview strip under a zoomed line or area chart (FEA-CH-50).
    overview: Option<overview::Strip>,
    /// The readout's exit hold: its pending timer (FEA-CH-42).
    leaving: Option<i32>,
    /// Touch points on the expanded chart, for pinch and pan (FEA-CH-49).
    fingers: Vec<(i32, [f64; 2])>,
    /// A pinch or one-finger pan in progress: the start's spread or x, and the window then.
    gesture: Option<(f64, [f64; 2])>,
    /// The same gesture along y, on a chart that zooms along both axes: the start's y and the
    /// window then.
    gesture_y: Option<(f64, [f64; 2])>,
    /// The panels' ratio that fills the turned dialog, and the width, room and hidden-series
    /// count it was found for (FEA-CH-68). A rebuild of the legend (data, controls, hidden
    /// series) or of the fonts forgets it, so a ratio that found no fill is tried again.
    fill: Option<([f64; 4], Option<f64>)>,
    /// The width an expanded chart narrowed to so that it fits its dialog, and the width, room and
    /// hidden-series count it was found for (S27-871, S27-874). Kept as [`View::fill`] is, so a
    /// zoom glide or a playback frame keeps it while the chart still fits there.
    narrowed: Option<([f64; 4], f64)>,
    /// Whether this expanded view is turned on a phone held upright, as of its last layout
    /// (FEA-CH-68); input maps through the turn.
    turned: bool,
    /// Whether this expanded view is in a short, wide window, as of its last layout: a phone held
    /// sideways or a laptop zoomed in. Its dialog is already the turned one's shape, so it takes
    /// that layout without the turn (FEA-CH-68, S27-876).
    short_wide: bool,
    /// The window size this expanded view chose its orientation for on a phone held upright,
    /// and whether it turned (FEA-CH-68). Series shown or hidden, zoom and fonts keep the choice;
    /// a new window size makes it again.
    turn_for: Option<([f64; 2], bool)>,
    /// Whether this expanded view's legend, note and key stand beside its plot, as of its last
    /// layout: in a short, wide window, a chart narrowed to fit the dialog's height (S27-875).
    aside: bool,
    /// The window size this expanded view chose that for, and its choice. As [`View::turn_for`],
    /// series shown or hidden, zoom and fonts keep it, so a legend entry the reader presses never
    /// moves the legend; a new window size makes it again.
    aside_for: Option<([f64; 2], bool)>,
    /// Its atlas drew text whose faces had not loaded; it draws again once they have
    /// (FEA-CH-30).
    fonts_due: bool,
    /// The selection was made by key: only a pointer that moves takes it over (FEA-CH-39).
    keyed: bool,
    /// Far from the screen: no canvas pixels, mesh or GPU buffers until it comes near (`lazy`).
    asleep: bool,
    /// The frame holds a drawn chart; until then it shows the loading state.
    painted: bool,
}

thread_local! {
    static RENDERER: RefCell<Option<Renderer>> = const { RefCell::new(None) };
    static VIEWS: RefCell<Vec<Rc<RefCell<View>>>> = const { RefCell::new(Vec::new()) };
    static FAILED: RefCell<bool> = const { RefCell::new(false) };
    /// At most one frame polls the shared renderer at a time.
    static SHADER_POLL: Cell<bool> = const { Cell::new(false) };
    /// A reader may ask for an image before the shared renderer is ready.
    static PNG_WAITING: RefCell<Vec<Weak<RefCell<View>>>> = const { RefCell::new(Vec::new()) };
    /// Font loads the atlases wait for, and the fonts (with their characters) asked for so far.
    static FONT_LOADS: Cell<usize> = const { Cell::new(0) };
    static FONTS_ASKED: RefCell<BTreeSet<String>> = const { RefCell::new(BTreeSet::new()) };
    /// Where the page last saw the pointer, in client pixels. A move to the same place is the
    /// page scrolling under a resting pointer (WebKit sends one), not the reader moving it.
    static POINTER_AT: Cell<Option<[f64; 2]>> = const { Cell::new(None) };
    /// The page prints: from `beforeprint`, which comes while the screen's media still apply,
    /// until the page is back on screen (FEA-CH-67).
    static PRINTING: Cell<bool> = const { Cell::new(false) };
    /// Expanded dialogs' scroll glides started so far, to number each one.
    static GLIDES: Cell<u32> = const { Cell::new(0) };
    static OUTSIDE_TOUCH: RefCell<OutsideTouch> = const { RefCell::new(OutsideTouch {
        fingers: BTreeSet::new(),
        tap: None,
    }) };
}

/// The largest pixel ratio the page's charts draw at. A page that shows itself scaled down, as
/// the docs' preview frames do (1000 px wide, shown at about a third), names it in
/// `<meta name="richview-max-dpr">`, between 1 and 3; reports carry none, and authors cannot add
/// one (INT-28).
fn max_dpr() -> f64 {
    thread_local! {
        static MAX_DPR: Cell<Option<f64>> = const { Cell::new(None) };
    }
    MAX_DPR.with(|cached| {
        if let Some(max) = cached.get() {
            return max;
        }
        let max = document()
            .query_selector("meta[name='richview-max-dpr']")
            .ok()
            .flatten()
            .and_then(|m| m.get_attribute("content"))
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .map_or(3.0, |v| v.clamp(1.0, 3.0));
        cached.set(Some(max));
        max
    })
}

fn reduced_motion() -> bool {
    window()
        .match_media("(prefers-reduced-motion: reduce)")
        .ok()
        .flatten()
        .is_some_and(|m| m.matches())
}

fn with_renderer<T>(f: impl FnOnce(&Renderer) -> T) -> Option<T> {
    let result = RENDERER.with(|r| {
        let mut slot = r.borrow_mut();
        if slot.as_ref().is_some_and(|r| r.lost() || r.invalid()) {
            *slot = None;
        }
        if slot.is_none() && !FAILED.with(|f| *f.borrow()) {
            match Renderer::new(&document()) {
                Ok(renderer) => {
                    let target: web_sys::EventTarget = renderer.canvas.clone().into();
                    on(&target, "webglcontextlost", |e| {
                        e.prevent_default();
                        let lost = e.current_target();
                        let waiting = RENDERER.with(|slot| {
                            if let Some(renderer) = slot.borrow_mut().as_mut()
                                && lost
                                    .as_ref()
                                    .is_some_and(|t| *t == renderer.canvas.clone().into())
                            {
                                let pending = renderer.pending();
                                renderer.invalidate();
                                pending
                            } else {
                                false
                            }
                        });
                        if !waiting {
                            return;
                        }
                        // If the browser never restores this context, replace it instead of
                        // leaving the report and its pending exports waiting forever.
                        let retry = Closure::once_into_js(move || {
                            let replace = RENDERER.with(|slot| {
                                slot.borrow().as_ref().is_some_and(|renderer| {
                                    renderer.invalid()
                                        && renderer.lost()
                                        && lost
                                            .as_ref()
                                            .is_some_and(|t| *t == renderer.canvas.clone().into())
                                })
                            });
                            if replace {
                                RENDERER.with(|slot| *slot.borrow_mut() = None);
                                redraw_after_renderer();
                                if !renderer_pending() {
                                    finish_waiting_pngs();
                                }
                                check_settled();
                            }
                        });
                        let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(
                            retry.unchecked_ref(),
                            1000,
                        );
                    });
                    on(&target, "webglcontextrestored", |e| {
                        let current = RENDERER.with(|slot| {
                            let mut slot = slot.borrow_mut();
                            // A draw during loss may already have replaced the renderer.
                            // Only rebuild the context whose restoration this event names.
                            if let Some(renderer) = slot.as_ref()
                                && e.current_target()
                                    .is_some_and(|t| t == renderer.canvas.clone().into())
                            {
                                *slot = renderer.restored().ok();
                                true
                            } else {
                                false
                            }
                        });
                        if !current {
                            return;
                        }
                        schedule_shader_poll();
                        redraw_after_renderer();
                        if !renderer_pending() {
                            finish_waiting_pngs();
                            check_settled();
                        }
                    });
                    *slot = Some(renderer);
                }
                Err(_) => FAILED.with(|f| *f.borrow_mut() = true),
            }
        }
        slot.as_ref().filter(|renderer| !renderer.pending()).map(f)
    });
    schedule_shader_poll();
    result
}

/// Poll in a later task, outside the renderer borrow and without blocking shader compilation.
fn schedule_shader_poll() {
    let pending = RENDERER.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|r| r.pending() && !r.lost() && !r.invalid())
    });
    if !pending || SHADER_POLL.with(|scheduled| scheduled.replace(true)) {
        return;
    }
    // Hidden documents can suspend animation frames. One timer at a time keeps `settled()` and
    // queued downloads moving without accumulating callbacks; the extension's query never waits.
    let next = Closure::once_into_js(poll_shaders);
    if window()
        .set_timeout_with_callback_and_timeout_and_arguments_0(next.unchecked_ref(), 32)
        .is_err()
    {
        SHADER_POLL.with(|scheduled| scheduled.set(false));
        RENDERER.with(|slot| *slot.borrow_mut() = None);
        FAILED.with(|failed| *failed.borrow_mut() = true);
    }
}

fn poll_shaders() {
    SHADER_POLL.with(|scheduled| scheduled.set(false));
    let result = RENDERER.with(|slot| {
        let mut slot = slot.borrow_mut();
        slot.as_mut()
            .filter(|r| r.pending() && !r.lost() && !r.invalid())
            .map(|r| r.poll())
    });
    match result {
        Some(Ok(false)) => {
            schedule_shader_poll();
            if FAILED.with(|failed| *failed.borrow()) {
                redraw_after_renderer();
                finish_waiting_pngs();
                check_settled();
            }
        }
        Some(Ok(true)) => {
            redraw_after_renderer();
            finish_waiting_pngs();
            check_settled();
        }
        Some(Err(_)) => {
            RENDERER.with(|slot| *slot.borrow_mut() = None);
            FAILED.with(|failed| *failed.borrow_mut() = true);
            redraw_after_renderer();
            finish_waiting_pngs();
            check_settled();
        }
        None => check_settled(),
    }
}

fn renderer_pending() -> bool {
    RENDERER.with(|slot| slot.borrow().as_ref().is_some_and(Renderer::pending))
}

/// Repaint existing meshes after the shared renderer becomes ready or fails.
fn redraw_after_renderer() {
    for view in every_view() {
        draw(&view);
    }
}

fn finish_waiting_pngs() {
    let waiting = PNG_WAITING.with(|views| std::mem::take(&mut *views.borrow_mut()));
    for view in waiting.into_iter().filter_map(|view| view.upgrade()) {
        png_download(&view);
    }
}

fn show_notice(frame: &HtmlElement) {
    // The notice replaces the loading state (ARC-05).
    let _ = frame.set_attribute("data-rv-notice", "");
    let _ = frame.remove_attribute("data-rv-painted");
    if let Ok(Some(notice)) = frame.query_selector(".rv-chart__notice") {
        let _ = notice.remove_attribute("hidden");
    } else {
        let p = el("p", "rv-chart__notice");
        p.set_text_content(Some(NOTICE));
        let _ = frame.append_child(&p);
    }
}

pub fn start() {
    // The interactive report mounts its charts progressively, nearest the screen first; other
    // outputs, and browsers that cannot watch the charts, mount them all now (MOTION.md).
    let figures = query_all("figure[data-rv-chart]");
    let output = document()
        .document_element()
        .and_then(|e| e.get_attribute("data-rv-output"))
        .unwrap_or_else(|| "report".into());
    let progressive = output == "report" && lazy::start(figures.clone());
    if !progressive {
        lazy::mount_every();
        for figure in figures {
            if let Err(e) = mount(&figure) {
                web_sys::console::warn_1(&e);
            }
        }
    }
    let target: web_sys::EventTarget = window().into();
    on(&target, "resize", |_| redraw_all(false));
    // The window hears a move after the canvas under it, so a chart compares with the last place.
    for event in ["pointermove", "pointerdown"] {
        on(&target, event, |e| {
            let e: web_sys::MouseEvent = e.unchecked_into();
            POINTER_AT.with(|at| at.set(Some(client_point(&e))));
        });
    }
    // Capture sees gestures even when a control handles them, before a canvas selects a new mark.
    // These listeners only read touch events; mouse/pen hover and their exit hold keep their order.
    for kind in ["pointerdown", "pointermove", "pointerup", "pointercancel"] {
        let listener = Closure::<dyn FnMut(web_sys::PointerEvent)>::new(move |e| {
            outside_touch(kind, &e);
        });
        let _ = target.add_event_listener_with_callback_and_bool(
            kind,
            listener.as_ref().unchecked_ref(),
            true,
        );
        listener.forget();
    }
    on(&target, "blur", |_| {
        OUTSIDE_TOUCH.with(|slot| {
            let mut touch = slot.borrow_mut();
            touch.tap = None;
            touch.fingers.clear();
        });
    });
    let fonts: web_sys::EventTarget = document().fonts().into();
    on(&fonts, "loadingdone", |_| redraw_all(true));
}

/// Whether a point is inside this chart's active figure or its open dialog and controls.
fn inside_chart(view: &View, [x, y]: [f64; 2]) -> bool {
    let dialog = view.dialog.clone().or_else(|| {
        view.owner
            .as_ref()
            .and_then(Weak::upgrade)
            .and_then(|owner| owner.borrow().dialog.clone())
    });
    let root: &Element = dialog.as_ref().map_or(&view.figure, AsRef::as_ref);
    let rect = root.get_bounding_client_rect();
    // Native dialogs retarget backdrop events to the dialog itself; use its actual bounds.
    rect.width() > 0.0
        && rect.height() > 0.0
        && x >= rect.left()
        && x <= rect.right()
        && y >= rect.top()
        && y <= rect.bottom()
}

/// Clear outside readouts only after a short single-finger touch lifts within the tap slop.
fn outside_touch(kind: &str, e: &web_sys::PointerEvent) {
    if e.pointer_type() != "touch" {
        return;
    }
    let point = client_point(e);
    let dismiss = OUTSIDE_TOUCH.with(|slot| {
        let mut touch = slot.borrow_mut();
        match kind {
            "pointerdown" => {
                let first = touch.fingers.is_empty();
                touch.fingers.insert(e.pointer_id());
                touch.tap = (first && e.is_primary()).then(|| OutsideTap {
                    pointer: e.pointer_id(),
                    at: point,
                    began: now(),
                    views: every_view()
                        .iter()
                        .filter(|view| {
                            let v = view.borrow();
                            v.target.is_some() && !inside_chart(&v, point)
                        })
                        .map(Rc::downgrade)
                        .collect(),
                });
            }
            "pointermove" => {
                if touch.tap.as_ref().is_some_and(|tap| {
                    tap.pointer == e.pointer_id()
                        && (point[0] - tap.at[0]).hypot(point[1] - tap.at[1]) >= TAP_SLOP
                }) {
                    touch.tap = None;
                }
            }
            "pointerup" => {
                if touch.fingers.remove(&e.pointer_id())
                    && let Some(tap) = touch.tap.take()
                    && tap.pointer == e.pointer_id()
                    && touch.fingers.is_empty()
                    && now() - tap.began < TAP_MS
                    && (point[0] - tap.at[0]).hypot(point[1] - tap.at[1]) < TAP_SLOP
                {
                    return tap.views;
                }
            }
            "pointercancel" => {
                touch.fingers.remove(&e.pointer_id());
                touch.tap = None;
            }
            _ => {}
        }
        Vec::new()
    });
    // Release the gesture borrow before drawing or dispatching any readout feedback.
    for view in dismiss.into_iter().filter_map(|v| v.upgrade()) {
        if !inside_chart(&view.borrow(), point) {
            {
                let mut v = view.borrow_mut();
                v.pinned = false;
                v.keyed = false;
            }
            set_target(&view, None, false);
        }
    }
}

/// Mount the chart in `figure`; `None` when this browser cannot draw it and the notice shows.
fn mount(figure: &Element) -> Result<Option<Rc<RefCell<View>>>, JsValue> {
    mount_as(figure, false)
}

/// Mount the chart in `figure`, `asleep` when it is far from the screen: laid out and ready to
/// read, drawn once it comes near (`lazy`).
fn mount_as(figure: &Element, asleep: bool) -> Result<Option<Rc<RefCell<View>>>, JsValue> {
    let frame: HtmlElement = figure
        .query_selector(".rv-chart__frame")?
        .ok_or("figure has no frame")?
        .unchecked_into();
    let payload = figure
        .query_selector("script[data-rv-payload]")?
        .and_then(|s| s.text_content())
        .unwrap_or_default();
    let chart = match Chart::parse(&payload) {
        Ok(c) => c,
        Err(e) => {
            show_notice(&frame);
            return Err(e.into());
        }
    };
    if with_renderer(|_| ()).is_none() && !renderer_pending() {
        show_notice(&frame);
        return Ok(None);
    }
    let canvas: HtmlCanvasElement = el("canvas", "rv-chart__canvas").unchecked_into();
    let ctx: CanvasRenderingContext2d = canvas.get_context("2d")?.ok_or("no 2d")?.unchecked_into();
    let readout = el("div", "rv-chart__readout");
    readout.set_hidden(true);
    // A scrolling readout would otherwise be a tab stop in some browsers; focus stays on the
    // frame, whose Page keys scroll it (FEA-CH-56).
    readout.set_attribute("tabindex", "-1")?;
    let live = el("div", "rv-chart__live");
    live.set_attribute("aria-live", "polite")?;
    live.set_class_name("rv-chart__live rv-visually-hidden");
    if let Ok(Some(notice)) = frame.query_selector(".rv-chart__notice") {
        notice.set_attribute("hidden", "")?;
    }
    frame.append_child(&canvas)?;
    frame.append_child(&readout)?;
    frame.append_child(&live)?;
    // The readout's faces load with the chart, so the first reading does not wait for them and
    // relayout every chart when they arrive (FEA-CH-40).
    if let Ok(Some(style)) = window().get_computed_style(&readout) {
        let family = style.get_property_value("font-family").unwrap_or_default();
        let size = style.get_property_value("font-size").unwrap_or_default();
        if !family.is_empty() {
            for weight in ["400", "600"] {
                let _ = document()
                    .fonts()
                    .load(&format!("{weight} {size} {family}"));
            }
        }
    }
    frame.set_tab_index(0);
    frame.set_attribute("role", "application")?;
    frame.set_attribute("aria-roledescription", "interactive chart")?;
    // Core's label is the title and the chart's summary (FEA-CH-55); the keys join it.
    let label = frame.get_attribute("aria-label").unwrap_or_else(|| {
        figure
            .query_selector(".rv-chart__title")
            .ok()
            .flatten()
            .and_then(|t| t.text_content())
            .unwrap_or_default()
    });
    let label = label.trim().trim_end_matches('.');
    frame.set_attribute("aria-label", &format!("{label}{KEYS_HINT}"))?;
    let actions = el("div", "rv-chart__actions");
    let view = View {
        figure: figure.clone(),
        frame: frame.clone(),
        canvas,
        ctx,
        readout,
        live,
        legend: None,
        actions,
        table: None,
        reset: None,
        geometry: chart.geometry(640.0, &[], None),
        retired: None,
        retirement: None,
        chart,
        width: 0.0,
        dpr: 1.0,
        mesh: Mesh::default(),
        panel_splits: Vec::new(),
        uploaded: None,
        hidden: Vec::new(),
        zoom: None,
        target: None,
        pinned: false,
        emphasis: None,
        mark_focus: false,
        side: None,
        entrance: None,
        settled: false,
        drag: None,
        drag_y: None,
        hint: None,
        hint_on: false,
        pan: None,
        pan_y: None,
        scroll_rest: 0.0,
        touch: None,
        tap: None,
        expanded: None,
        dialog: None,
        owner: None,
        controls: None,
        playing: None,
        glide: None,
        table_all: false,
        fade: feedback::Fade::default(),
        hover_at: f64::NEG_INFINITY,
        pointer: None,
        letting_go: None,
        released: Vec::new(),
        zoom_glide: None,
        names: feedback::NameFade::default(),
        animating: false,
        relayout: false,
        scrolling: None,
        legend_key: String::new(),
        overview: None,
        leaving: None,
        fingers: Vec::new(),
        gesture: None,
        gesture_y: None,
        fill: None,
        narrowed: None,
        turned: false,
        short_wide: false,
        turn_for: None,
        aside: false,
        aside_for: None,
        fonts_due: false,
        keyed: false,
        asleep,
        painted: false,
    };
    let view = Rc::new(RefCell::new(view));
    // Charts on or above the first screen at mount keep their painted state (FEA-CH-51).
    let rect = frame.get_bounding_client_rect();
    let screen = window()
        .inner_height()
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(800.0);
    let below = rect.top() > screen;
    if !below || reduced_motion() || view.borrow().geometry.entrance.is_none() {
        view.borrow_mut().settled = true;
    }
    title::apply(figure);
    layout(&view, true);
    build_actions(&view)?;
    controls::build(&view)?;
    bind(&view);
    if below && !view.borrow().settled {
        watch_entrance(&view);
    }
    let observer_view = Rc::downgrade(&view);
    let observer = Closure::<dyn FnMut(js_sys::Array)>::new(move |_entries: js_sys::Array| {
        // Lay out on the next frame, not inside the observer: a new height, legend or strip
        // must not resize observed boxes while notifications are being delivered.
        let Some(v) = observer_view.upgrade() else {
            return;
        };
        if v.borrow().relayout {
            return;
        }
        v.borrow_mut().relayout = true;
        let w = Rc::downgrade(&v);
        let next = Closure::once_into_js(move || {
            let Some(v) = w.upgrade() else { return };
            v.borrow_mut().relayout = false;
            let width = v.borrow().frame.client_width() as f64;
            if (width - v.borrow().width).abs() >= 0.5 {
                layout(&v, false);
            }
            check_settled();
        });
        let _ = window().request_animation_frame(next.unchecked_ref());
    });
    let ro = web_sys::ResizeObserver::new(observer.as_ref().unchecked_ref())?;
    ro.observe(&frame);
    observer.forget();
    VIEWS.with(|v| v.borrow_mut().push(view.clone()));
    check_settled();
    Ok(Some(view))
}

/// Every mounted chart view, the open expanded views included.
fn every_view() -> Vec<Rc<RefCell<View>>> {
    let views = VIEWS.with(|v| v.borrow().clone());
    let expanded: Vec<_> = views
        .iter()
        .filter_map(|v| v.borrow().expanded.clone())
        .collect();
    views.into_iter().chain(expanded).collect()
}

/// Lay the chart out at the frame's width and draw it; `fresh` also rebuilds the legend.
fn layout(view: &Rc<RefCell<View>>, fresh: bool) {
    layout_inner(view, fresh, false);
}

fn layout_inner(view: &Rc<RefCell<View>>, fresh: bool, instant: bool) {
    // The series control that has focus keeps it when its legend entry or name is rebuilt or
    // moved, including when the legend folds into line-end names at a new width.
    let focused = focused_series();
    {
        let mut v = view.borrow_mut();
        // A synchronous burst never retains a chain of outgoing layouts.
        v.retired = None;
        if fresh {
            v.fill = None;
            v.narrowed = None;
        }
        let mut width = (v.frame.client_width() as f64).max(240.0);
        // Paper gets at least twice the pixels, also when print styles change the width
        // (FEA-CH-67).
        let printing = PRINTING.with(Cell::get)
            || window()
                .match_media("print")
                .ok()
                .flatten()
                .is_some_and(|m| m.matches());
        let dpr = window().device_pixel_ratio().clamp(1.0, 3.0).min(max_dpr());
        let dpr = if printing { dpr.max(2.0) } else { dpr };
        let hidden = v.hidden.clone();
        // A zoom glide draws its window of the moment (FEA-CH-52).
        let zoom = v.zoom_glide.map(|g| Some(g.window())).unwrap_or(v.zoom);
        if let Some(y) = v.zoom_glide.and_then(|g| g.window_y()) {
            v.chart.zoom_y = Some(y);
        }
        // A turn of the phone resizes the window, which lays every expanded view out again.
        orient(&mut v, &hidden, zoom);
        // In a short, wide window the expanded chart keeps what it chose for this window size:
        // its legend, note and key beside the plot or above and under it (S27-875). Undecided,
        // it is laid out without them first.
        let size = window_size();
        let aside_chosen = match (&v.owner, v.short_wide) {
            (Some(_), true) => v.aside_for.filter(|(s, _)| *s == size).map(|(_, a)| a),
            _ => Some(false),
        };
        if v.owner.is_some() {
            set_aside(&mut v, aside_chosen.unwrap_or(false));
        }
        // A zoom window that no longer lies inside the data is dropped (FEA-CH-71).
        let heading = |g: &Geometry, t: Target| {
            g.panels
                .get(t.panel)
                .and_then(|p| p.observations.get(t.observation))
                .map(|o| o.readout.heading.clone())
        };
        let previous_heading = v.target.and_then(|t| heading(&v.geometry, t));
        let reuse = instant
            && !printing
            && hidden.is_empty()
            && v.owner.is_none()
            && v.expanded.is_none()
            && v.glide.is_none()
            && v.zoom_glide.is_none()
            && v.geometry.kind == "line"
            && !v.chart.block.stack
            && v.geometry
                .checks
                .iter()
                .any(|c| c.kind == "decimation" && c.details["automatic"] == true);
        let mut g = if reuse {
            let observations = v
                .geometry
                .panels
                .iter_mut()
                .map(|p| std::mem::take(&mut p.observations))
                .collect();
            v.chart.geometry_reusing(width, &hidden, zoom, observations)
        } else {
            v.chart.geometry(width, &hidden, zoom)
        };
        let mut applied_ratio = None;
        // The expanded chart redraws at the modal's size (FEA-CH-44): as wide as the window
        // allows, and narrower when that would make it taller than the dialog has room for.
        if v.owner.is_some() {
            match size_expanded(&mut v, width, g, &hidden, zoom) {
                Ok(mut sized) => {
                    // Narrowed in a short, wide window, it tries its legend, note and key beside
                    // the plot, once for each window size, and keeps them there when the chart
                    // fits and shows no less of its plot at once (S27-875). A chart whose height
                    // does not follow its width (a gantt's rows) would only give width up for
                    // them.
                    if aside_chosen.is_none() && sized.narrowed && has_aside(&v, &sized.geometry) {
                        set_aside(&mut v, true);
                        let own = v.chart.geometry(sized.width, &hidden, zoom);
                        match size_expanded(&mut v, sized.width, own, &hidden, zoom) {
                            Ok(beside)
                                if beside.fits
                                    && plot_area(&beside.geometry)
                                        >= plot_in_view(&sized) - 0.5 =>
                            {
                                sized = beside
                            }
                            _ => set_aside(&mut v, false),
                        }
                    }
                    if aside_chosen.is_none() {
                        v.aside_for = Some((size, v.aside));
                    }
                    if let Some(across) = across_at_flattest(&v, &sized, &hidden, zoom) {
                        sized = across;
                    }
                    width = sized.width;
                    g = sized.geometry;
                    applied_ratio = sized.ratio;
                    fit_dialog(&v, width);
                }
                Err(own) => g = *own,
            }
        }
        // Expanding or resizing changes how many pixels one grid step occupies. Apply the
        // floor at the width actually drawn, including a turned phone's fitted panels.
        if let Some(window) = zoom
            && let Some(scale) = g.panels.first().and_then(|p| p.x.as_ref())
        {
            let clamped = v.chart.clamp_zoom(window, scale);
            // A glide out of the whole chart starts on the full extent, which is a valid
            // animation frame even though a settled full-extent window means reset.
            let full_glide = v.zoom_glide.is_some()
                && window[0] <= scale.extent[0]
                && window[1] >= scale.extent[1];
            if clamped != Some(window) && !full_glide {
                v.zoom = clamped;
                if clamped.is_none() {
                    v.chart.zoom_y = None;
                }
                v.zoom_glide = None;
                g = v
                    .chart
                    .geometry_at_ratio(width, applied_ratio, &hidden, clamped);
                if let Some(reset) = &v.reset {
                    reset.set_hidden(clamped.is_none());
                }
            }
        }
        v.width = width;
        v.dpr = dpr;
        // The selection survives a redraw while its observation still reads the same, as when
        // fonts arrive or the width changes (FEA-CH-44); otherwise it clears.
        let keep = v
            .target
            .filter(|t| previous_heading.is_some() && heading(&g, *t) == previous_heading);
        if instant
            && v.glide.is_none()
            && v.geometry.panels.iter().any(|p| !p.observations.is_empty())
            && g.checks
                .iter()
                .any(|c| c.kind == "decimation" && c.details["automatic"] == true)
        {
            let mut old = std::mem::replace(&mut v.geometry, g);
            // Reused line buffers have already left the outgoing geometry. Release paint
            // storage now; only remaining observation cleanup needs a later task.
            for panel in &mut old.panels {
                panel.items = Vec::new();
            }
            old.figure_items = Vec::new();
            v.retired = Some(old);
        } else {
            v.geometry = g;
        }
        // Whether the chart zooms follows its geometry: a control can change it.
        keep_reset_place(&v);
        let label = v.chart.accessible_name(&v.geometry);
        let _ = v
            .frame
            .set_attribute("aria-label", &format!("{label}{KEYS_HINT}"));
        v.target = keep;
        if keep.is_none() {
            v.pinned = false;
        }
        // A new layout numbers its observations afresh: nothing released survives it, and a
        // mark's emphasis follows the kept target.
        v.released.clear();
        if v.mark_focus {
            let focus = focus::of(&v, keep);
            v.mark_focus = focus.is_some();
            v.emphasis = focus.as_ref().map(|(f, _)| f.clone());
            v.fade.refocus(focus);
        }
        // A series the reader just hid stands out from nothing: the shown ones keep full ink
        // although the pointer still rests on its entry (FEA-CH-52).
        if let Some(Focus::Series(s)) = v.emphasis
            && v.geometry.series.get(s).is_none_or(|s| s.hidden)
        {
            v.emphasis = None;
            v.fade.refocus(None);
        }
        show_readout(&mut v, false);
        let height = v.geometry.height;
        let _ = v
            .frame
            .style()
            .set_property("height", &format!("{height}px"));
        let _ = v.frame.style().set_property("aspect-ratio", "auto");
        // Setting even an unchanged canvas dimension resets its bitmap. A zoom redraws
        // into the same-sized surface; only a real size/DPR change needs a new bitmap. A
        // sleeping chart keeps none (`lazy`).
        let (pixel_width, pixel_height) = if v.asleep {
            (0, 0)
        } else {
            ((width * dpr).round() as u32, (height * dpr).round() as u32)
        };
        if v.canvas.width() != pixel_width {
            v.canvas.set_width(pixel_width);
        }
        if v.canvas.height() != pixel_height {
            v.canvas.set_height(pixel_height);
        }
        let _ = v
            .canvas
            .style()
            .set_property("width", &format!("{width}px"));
        let _ = v
            .canvas
            .style()
            .set_property("height", &format!("{height}px"));
        // The drawn size is the model's: page styles never stretch or squeeze the canvas.
        for (property, value) in [
            ("min-width", "0"),
            ("max-width", "none"),
            ("min-height", "0"),
            ("max-height", "none"),
        ] {
            let _ = v.canvas.style().set_property(property, value);
        }
        if let Some(up) = v.uploaded.take() {
            with_renderer(|r| r.release(up));
        }
        if v.asleep {
            release_surface(&mut v);
        } else {
            let state = v.fade.state();
            rebuild_mesh(&mut v, &state, dpr);
        }
        if let Some(note) = v.geometry.note.clone() {
            set_note(&v.figure, &note);
        }
    }
    // The legend is rebuilt when what it shows changes, not on every redraw (a zoom glide
    // lays out every frame).
    let legend_changed = {
        let mut v = view.borrow_mut();
        let key =
            serde_json::to_string(&(&v.geometry.legend, &v.geometry.series)).unwrap_or_default();
        let changed = key != v.legend_key;
        v.legend_key = key;
        changed
    };
    if fresh || legend_changed {
        build_legend(view, &focused);
    }
    build_names(view, &focused);
    overview::sync(view);
    if focused.is_some() && focused_series().is_none() {
        // A legend that moved beside the plot took its focus along; a series that lost its
        // control at this width leaves focus on the chart, for inspection.
        let (figure, frame) = {
            let v = view.borrow();
            (v.figure.clone(), v.frame.clone())
        };
        refocus(&figure, &focused);
        if focused_series().is_none() {
            let _ = frame.focus();
        }
    }
    draw(view);
    retire(view);
}

/// Release the replaced layout after drawing, with one weak-owner timer per view.
fn retire(view: &Rc<RefCell<View>>) {
    if view.borrow().retired.is_none() || view.borrow().retirement.is_some() {
        return;
    }
    let weak = Rc::downgrade(view);
    let release = Closure::once(move || {
        if let Some(view) = weak.upgrade() {
            let retired = {
                let mut v = view.borrow_mut();
                v.retirement = None;
                v.retired.take()
            };
            drop(retired);
        }
    });
    match window()
        .set_timeout_with_callback_and_timeout_and_arguments_0(release.as_ref().unchecked_ref(), 0)
    {
        Ok(timer) => {
            view.borrow_mut().retirement = Some(timer);
            release.forget();
        }
        Err(_) => view.borrow_mut().retired = None,
    }
}

/// An expanded chart laid out for its dialog.
struct Sized {
    width: f64,
    /// The widest width the dialog gives the chart, and the height it leaves for it.
    full: f64,
    room: f64,
    geometry: Geometry,
    /// The panels' ratio that fills the dialog, when it does.
    ratio: Option<f64>,
    /// It narrowed to fit the dialog's height.
    narrowed: bool,
    /// It fits the dialog's height, and so does the column beside its plot.
    fits: bool,
}

/// Lay the expanded chart out for its dialog (FEA-CH-44): as wide as the window allows, and
/// narrower when that would make it taller than the dialog has room for. Turned on a phone held
/// upright, the chart keeps the screen's height as its width and takes the height the dialog
/// leaves (FEA-CH-68); in a short, wide window, so does a chart whose x scale shows more with
/// width. The ratio found for this width, room and number of hidden series is reused, as on every
/// frame of a zoom glide, while the chart it gives still fits. `own` is the layout at `width`.
/// Outside a dialog, `own` comes back.
fn size_expanded(
    v: &mut View,
    width: f64,
    own: Geometry,
    hidden: &[String],
    zoom: Option<[f64; 2]>,
) -> Result<Sized, Box<Geometry>> {
    let Some((full, room)) = expanded_room(v) else {
        return Err(Box::new(own));
    };
    let (mut width, mut g) = (width, own);
    if (full - width).abs() >= 0.5 {
        width = full;
        g = v.chart.geometry(width, hidden, zoom);
    }
    let mut ratio_used = None;
    let mut filled = false;
    let flattens = v.turned || (v.short_wide && v.chart.block.kind.wider() == Wider::ShowsMore);
    let flattest = if v.turned {
        TURN_FLATTEST
    } else {
        SHORT_WIDE_FLATTEST
    };
    if g.height > room && room >= 120.0 && flattens {
        let key = [
            width.round(),
            room.round(),
            hidden.len() as f64,
            aside_key(v),
        ];
        let cached = v.fill.filter(|(k, _)| *k == key).map(|(_, ratio)| ratio);
        let mut ratio =
            cached.unwrap_or_else(|| fill_ratio(v, width, room, flattest, hidden, zoom));
        let mut fitted = ratio.map(|r| v.chart.geometry_at_ratio(width, Some(r), hidden, zoom));
        if cached.is_some() && fitted.as_ref().is_some_and(|f| f.height > room + 0.5) {
            ratio = fill_ratio(v, width, room, flattest, hidden, zoom);
            fitted = ratio.map(|r| v.chart.geometry_at_ratio(width, Some(r), hidden, zoom));
        }
        v.fill = Some((key, ratio));
        if let Some(f) = fitted.filter(|f| f.height <= room + 0.5) {
            g = f;
            ratio_used = ratio;
            filled = true;
        }
    }
    // A filled chart fits within half a pixel, which this test would read as too tall.
    let narrowed = !filled && g.height > room && room >= 120.0;
    if narrowed {
        (width, g) = narrow_to_fit(v, width, g, room, hidden, zoom);
    }
    let column = aside_column(&v.figure).map_or(0.0, |c| c.get_bounding_client_rect().height());
    let fits = g.height <= room + 0.5 && column <= room + 0.5;
    Ok(Sized {
        width,
        full,
        room,
        geometry: g,
        ratio: ratio_used,
        narrowed,
        fits,
    })
}

/// In a short, wide window, a chart whose x scale shows more with width and that fits the dialog
/// at no width, with or without its legend beside the plot, is drawn across the dialog at its
/// flattest rather than narrowed: both scroll, and the wide one reads better (S27-875). A chart
/// that would be taller so keeps its narrowed layout: a splom's square cells grow with the width.
fn across_at_flattest(
    v: &View,
    sized: &Sized,
    hidden: &[String],
    zoom: Option<[f64; 2]>,
) -> Option<Sized> {
    if sized.fits
        || !sized.narrowed
        || v.aside
        || !v.short_wide
        || v.turned
        || v.chart.block.kind.wider() != Wider::ShowsMore
    {
        return None;
    }
    let ratio = Some(SHORT_WIDE_FLATTEST);
    let geometry = v.chart.geometry_at_ratio(sized.full, ratio, hidden, zoom);
    (geometry.height <= sized.geometry.height + 0.5).then_some(Sized {
        width: sized.full,
        full: sized.full,
        room: sized.room,
        geometry,
        ratio,
        narrowed: false,
        fits: false,
    })
}

/// Whether the expanded chart has a legend, a note or a key of its own to stand beside its plot.
fn has_aside(v: &View, g: &Geometry) -> bool {
    v.legend.is_some()
        || v.figure
            .query_selector(".rv-chart__method")
            .ok()
            .flatten()
            .is_some()
        || g.figure_items.iter().any(|i| i.role() == Role::Key)
}

/// The layouts a chart with its legend beside its plot finds are its own: keys of the fill and
/// narrowing found keep the two apart (S27-875).
fn aside_key(v: &View) -> f64 {
    if v.aside { 1.0 } else { 0.0 }
}

/// The share of its upright height that a turned chart must keep (FEA-CH-68).
const TURN_KEEPS: f64 = 0.9;

/// The flattest panels, width to height, that a turned chart fills its room with; flatter, it
/// stays upright (FEA-CH-68).
const TURN_FLATTEST: f64 = 5.0;

/// The flattest panels in a short, wide window, whose dialog has no other orientation to fall
/// back to, so a time series flattens further before it narrows (S27-871).
const SHORT_WIDE_FLATTEST: f64 = 8.0;

/// The narrowest width the chart engine lays a chart out at; below it the drawing would only be
/// squeezed.
const LEAST_WIDTH: f64 = 240.0;

/// The width and layout of an expanded chart too tall for its dialog (FEA-CH-44): the widest
/// width, down to [`LEAST_WIDTH`], at which the chart fits. Its height need not shrink in step
/// with its width (a legend and axes keep theirs, a gauge stops growing at a size of its own), so
/// one step scaled by the room would leave it too tall; the width is bisected (S27-871, S27-874).
/// Each width is measured in the dialog it gives, its title and legend wrapped to it, and with
/// its own note, which can lengthen as the chart narrows (the words a cloud leaves out). The width
/// found for this window, room and number of hidden series is kept while the chart still fits
/// there, so a zoom glide or a playback frame does not change it. A chart that fits at no width
/// takes the least width, or keeps its own where that does not lower it (a gantt's rows), and
/// the dialog scrolls.
fn narrow_to_fit(
    v: &mut View,
    width: f64,
    own: Geometry,
    room: f64,
    hidden: &[String],
    zoom: Option<[f64; 2]>,
) -> (f64, Geometry) {
    let measure = |v: &View, w: f64, g: &Geometry| {
        fit_dialog(v, w);
        if let Some(note) = &g.note {
            set_note(&v.figure, note);
        }
        g.height <= expanded_room(v).map_or(room, |(_, room)| room) + 0.5
    };
    let fits = |v: &View, w: f64| {
        let g = v.chart.geometry(w, hidden, zoom);
        (measure(v, w, &g), g)
    };
    let key = [
        width.round(),
        room.round(),
        hidden.len() as f64,
        aside_key(v),
    ];
    if measure(v, width, &own) || width <= LEAST_WIDTH {
        return (width, own);
    }
    if let Some((_, kept)) = v.narrowed.filter(|(k, w)| *k == key && *w < width)
        && let (true, g) = fits(v, kept)
    {
        return (kept, g);
    }
    let (least_fits, least) = fits(v, LEAST_WIDTH);
    let (lo, g) = if !least_fits {
        if least.height < own.height - 0.5 {
            (LEAST_WIDTH, least)
        } else {
            (width, own)
        }
    } else {
        // The chart fits at `lo` and not at `hi`.
        let (mut lo, mut hi, mut found) = (LEAST_WIDTH, width, least);
        while hi - lo > 2.0 {
            let mid = ((lo + hi) / 2.0).floor();
            match fits(v, mid) {
                (true, g) => (lo, found) = (mid, g),
                (false, _) => hi = mid,
            }
        }
        (lo, found)
    };
    v.narrowed = Some((key, lo));
    (lo, g)
}

/// Whether the expanded view turns a quarter on a phone held upright (FEA-CH-68), chosen once for
/// each window size, and its dialog marked so that the figure styles turn it. Only a type whose
/// x scale shows more with width may turn; a flow, hierarchy, radial or area-filling chart, map,
/// network, word cloud, funnel or 3D scene stays upright, where it reads at once.
fn orient(v: &mut View, hidden: &[String], zoom: Option<[f64; 2]>) {
    let dialog = v
        .owner
        .as_ref()
        .and_then(|_| v.figure.closest("dialog").ok().flatten());
    let Some(dialog) = dialog else {
        v.turned = false;
        v.short_wide = false;
        return;
    };
    let matches = |query: &str| {
        window()
            .match_media(query)
            .ok()
            .flatten()
            .is_some_and(|m| m.matches())
    };
    let upright_phone = matches(richview_chart::interaction::TURN_QUERY);
    v.short_wide = matches(richview_chart::interaction::SHORT_WIDE_QUERY);
    let _ = dialog.toggle_attribute_with_force("data-rv-short-wide", v.short_wide);
    let size = window_size();
    v.turned = upright_phone
        && v.chart.block.kind.wider() == Wider::ShowsMore
        && match v.turn_for {
            Some((s, turn)) if s == size => turn,
            _ => {
                let turn = choose_turn(v, &dialog, hidden, zoom);
                v.turn_for = Some((size, turn));
                turn
            }
        };
    let _ = dialog.toggle_attribute_with_force("data-rv-turned", v.turned);
}

/// Whether the turned layout draws the chart larger than the upright one: wider, keeping at
/// least [`TURN_KEEPS`] of the upright height, and fitting the dialog without scrolling. A chart
/// that cannot flatten to the turned dialog's height (a pie, a waffle, a gantt's rows) stays
/// upright. Each layout is measured in the dialog as it would stand, within one task, so the
/// screen never paints the one not taken.
fn choose_turn(v: &mut View, dialog: &Element, hidden: &[String], zoom: Option<[f64; 2]>) -> bool {
    // Each orientation's own dialog width, not one fitted to the other's chart.
    if let Some(d) = dialog.dyn_ref::<HtmlElement>() {
        let _ = d.style().remove_property("width");
    }
    let turned = drawn_in(v, dialog, true, hidden, zoom);
    let upright = drawn_in(v, dialog, false, hidden, zoom);
    match (turned, upright) {
        (Some(([tw, th], true)), Some(([uw, uh], _))) => tw > uw && th >= TURN_KEEPS * uh,
        _ => false,
    }
}

/// The size of what the expanded chart draws in one orientation, laid out as [`layout`] would,
/// and whether it fits the dialog.
fn drawn_in(
    v: &mut View,
    dialog: &Element,
    turn: bool,
    hidden: &[String],
    zoom: Option<[f64; 2]>,
) -> Option<([f64; 2], bool)> {
    let _ = dialog.toggle_attribute_with_force("data-rv-turned", turn);
    v.turned = turn;
    let (width, room) = expanded_room(v)?;
    let g = v.chart.geometry(width, hidden, zoom);
    if g.height <= room + 0.5 {
        return Some((drawn(&g), true));
    }
    if room < 120.0 {
        return Some((drawn(&g), false));
    }
    if turn {
        // Turned, the panels flatten to the room or the chart does not fit.
        let ratio = fill_ratio(v, width, room, TURN_FLATTEST, hidden, zoom);
        v.fill = Some((
            [
                width.round(),
                room.round(),
                hidden.len() as f64,
                aside_key(v),
            ],
            ratio,
        ));
        let fitted = ratio
            .map(|r| v.chart.geometry_at_ratio(width, Some(r), hidden, zoom))
            .filter(|f| f.height <= room + 0.5);
        return Some(fitted.map_or(([0.0, 0.0], false), |f| (drawn(&f), true)));
    }
    // Upright, a chart taller than the room narrows until it fits.
    let narrow = (width * room / g.height).floor().max(240.0);
    let g = v.chart.geometry(narrow, hidden, zoom);
    Some((drawn(&g), g.height <= room + 0.5))
}

/// The width and height of everything a chart draws, its axes and labels included.
fn drawn(g: &Geometry) -> [f64; 2] {
    let mut extent: Option<[f64; 4]> = None;
    let items = g
        .panels
        .iter()
        .flat_map(|p| &p.items)
        .chain(&g.figure_items);
    for r in items.filter_map(Item::bounds) {
        let [x0, y0, x1, y1] = extent.unwrap_or([r.x, r.y, r.x + r.w, r.y + r.h]);
        extent = Some([
            x0.min(r.x),
            y0.min(r.y),
            x1.max(r.x + r.w),
            y1.max(r.y + r.h),
        ]);
    }
    extent.map_or([0.0, 0.0], |[x0, y0, x1, y1]| [x1 - x0, y1 - y0])
}

/// The expanded chart's widest width and the height the dialog leaves for its plot, from the
/// window: the dialog is at most 96% of the window wide, 1400 px, and 92% of it high, or 96% in a
/// short, wide window (S27-875).
fn expanded_room(v: &View) -> Option<(f64, f64)> {
    let figure = &v.figure;
    let dialog = figure.closest("dialog").ok().flatten()?;
    let style = window().get_computed_style(figure).ok().flatten()?;
    let px = |p: &str| {
        style
            .get_property_value(p)
            .ok()
            .and_then(|s| s.trim_end_matches("px").parse::<f64>().ok())
            .unwrap_or(0.0)
    };
    let pad = px("padding-left") + px("padding-right");
    let screen = (
        window().inner_width().ok()?.as_f64()?,
        window().inner_height().ok()?.as_f64()?,
    );
    // A turned dialog (FEA-CH-68) takes the screen's height as its width, and its boxes' heights
    // show on the screen as widths.
    let turn = v.turned;
    let (inner_w, inner_h) = if turn { (screen.1, screen.0) } else { screen };
    let tall = |r: web_sys::DomRect| if turn { r.width() } else { r.height() };
    let full = (inner_w * 0.96).min(1400.0) - 2.0 - pad;
    let head = dialog
        .query_selector(".rv-chart-dialog__head")
        .ok()
        .flatten()
        .map_or(0.0, |h| tall(h.get_bounding_client_rect()));
    // What the figure holds above and below the plot. An open Table sits under the chart and
    // scrolls with the dialog (FEA-CH-45), so the chart's room leaves it out: otherwise the next
    // layout with the Table open (a series hidden, a control, a turn of the phone) shrinks the
    // chart by the Table's height.
    let table = v.table.as_ref().map_or(0.0, |t| {
        let margin = window()
            .get_computed_style(t)
            .ok()
            .flatten()
            .and_then(|s| s.get_property_value("margin-top").ok())
            .and_then(|m| m.trim_end_matches("px").parse::<f64>().ok())
            .unwrap_or(0.0);
        tall(t.get_bounding_client_rect()) + margin
    });
    // Beside the plot, the column of its legend and note takes its width from the plot's and
    // shares the plot's row, as tall as the taller of the two (S27-875).
    let column = aside_column(figure).map(|c| c.get_bounding_client_rect());
    let full = full
        - column
            .as_ref()
            .map_or(0.0, |c| if turn { c.height() } else { c.width() });
    let row = tall(v.frame.get_bounding_client_rect()).max(column.map_or(0.0, tall));
    let around = tall(figure.get_bounding_client_rect()) - row - table;
    // A chart that opens zoomed gets its overview strip after this layout; leave its room (2.25rem,
    // or 44 px on a touch screen as the stylesheet sets it, and a .5rem margin) so the dialog does
    // not scroll.
    let strip = if v.zoom.is_some()
        && figure
            .query_selector(".rv-chart__overview")
            .ok()
            .flatten()
            .is_none()
    {
        let coarse = window()
            .match_media("(pointer: coarse)")
            .ok()
            .flatten()
            .is_some_and(|m| m.matches());
        let height = if coarse {
            44.0
        } else {
            2.25 * root_font_size()
        };
        height + 0.5 * root_font_size()
    } else {
        0.0
    };
    let high = if v.short_wide { 0.96 } else { 0.92 };
    Some((
        full.max(240.0),
        inner_h * high - 2.0 - head - around - strip,
    ))
}

/// The panels' width-to-height ratio that lays the chart out `room` pixels tall at `width`, for
/// the turned dialog (FEA-CH-68). Panel heights follow the ratio and the rest of the chart (axes,
/// titles, legend) does not, so the height is `rest + k / ratio`; two layouts give `rest` and `k`.
/// A flatter layout can add height of its own (a legend in place of line-end names, a panel's
/// least height); one more layout counts it into `rest`. `None` when the height does not follow
/// the ratio (rows of horizontal bars), when filling the room would flatten the panels past
/// `flattest` (width to height), or when the chart still does not fit.
fn fill_ratio(
    v: &View,
    width: f64,
    room: f64,
    flattest: f64,
    hidden: &[String],
    zoom: Option<[f64; 2]>,
) -> Option<f64> {
    let (a, b) = (1.5, 2.5);
    let height = |r: f64| {
        v.chart
            .geometry_at_ratio(width, Some(r), hidden, zoom)
            .height
    };
    let (ha, hb) = (height(a), height(b));
    let k = (ha - hb) / (1.0 / a - 1.0 / b);
    let mut rest = ha - k / a;
    for _ in 0..2 {
        if k <= 1.0 || room <= rest {
            return None;
        }
        let ratio = k / (room - rest);
        if ratio > flattest {
            return None;
        }
        let h = height(ratio);
        if h <= room + 0.5 {
            return Some(ratio);
        }
        rest = h - k / ratio;
    }
    None
}

/// The window's size in CSS pixels.
fn window_size() -> [f64; 2] {
    let px = |v: Result<JsValue, JsValue>| v.ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
    [px(window().inner_width()), px(window().inner_height())]
}

fn root_font_size() -> f64 {
    document()
        .document_element()
        .and_then(|root| window().get_computed_style(&root).ok().flatten())
        .and_then(|s| s.get_property_value("font-size").ok())
        .and_then(|s| s.trim_end_matches("px").parse::<f64>().ok())
        .unwrap_or(16.0)
}

/// Size the dialog to the expanded chart, so its title, actions and legend line up with it. On a
/// short, wide window the dialog keeps its full width, so the title stays on its line and the
/// room measured for the chart holds, and a chart narrower than that is centred in it.
fn fit_dialog(v: &View, width: f64) {
    let Some(dialog) = v
        .figure
        .closest("dialog")
        .ok()
        .flatten()
        .and_then(|d| d.dyn_into::<HtmlElement>().ok())
    else {
        return;
    };
    let style = window().get_computed_style(&v.figure).ok().flatten();
    let pad: f64 = ["padding-left", "padding-right"]
        .iter()
        .filter_map(|p| {
            style
                .as_ref()?
                .get_property_value(p)
                .ok()?
                .trim_end_matches("px")
                .parse::<f64>()
                .ok()
        })
        .sum();
    let frame = v.frame.style();
    if v.short_wide {
        let _ = dialog.style().remove_property("width");
        let _ = frame.set_property("width", &format!("{}px", width.ceil()));
        let _ = frame.set_property("margin-inline", "auto");
    } else {
        let _ = frame.remove_property("width");
        let _ = frame.remove_property("margin-inline");
        let _ = dialog
            .style()
            .set_property("width", &format!("{}px", (width + pad + 2.0).ceil()));
    }
    // The actions stay in reach under the title row while the dialog scrolls (FEA-CH-45): the
    // stylesheet places them by the title row's height, read at the dialog's new width.
    if let Some(head) = dialog
        .query_selector(".rv-chart-dialog__head")
        .ok()
        .flatten()
        .and_then(|h| window().get_computed_style(&h).ok().flatten())
        .and_then(|s| s.get_property_value("height").ok())
    {
        let _ = dialog.style().set_property("--rv-head-h", &head);
    }
}

fn set_note(figure: &Element, note: &str) {
    let existing = figure.query_selector(".rv-chart__method").ok().flatten();
    let p = existing.unwrap_or_else(|| {
        let p = el("p", "rv-chart__method");
        if let Some(column) = aside_column(figure) {
            let _ = column.append_child(&p);
        } else if let Ok(Some(frame)) = figure.query_selector(".rv-chart__frame") {
            let _ = frame.after_with_node_1(&p);
        }
        p.into()
    });
    p.set_text_content(Some(note));
}

/// The column beside an expanded chart's plot that holds its legend and note (S27-875).
fn aside_column(figure: &Element) -> Option<Element> {
    figure
        .query_selector(":scope > .rv-chart__aside")
        .ok()
        .flatten()
}

/// Stand the expanded chart's legend and note in a column beside its plot, and have the engine
/// draw a key of its own (a pie's) beside the plot too; or put them back, the legend on its side
/// of the plot and the note under it (S27-875). The column follows the plot, legend first; the
/// figure styles set the two side by side. Nothing moves when they already stand where asked,
/// so a focused legend entry keeps its focus.
fn set_aside(v: &mut View, on: bool) {
    v.aside = on;
    v.chart.aside = on;
    let figure = v.figure.clone();
    let frame: &Element = v.frame.as_ref();
    let column = aside_column(&figure);
    let note = figure.query_selector(".rv-chart__method").ok().flatten();
    let legend: Option<Element> = v.legend.clone().map(Into::into);
    if on && (legend.is_some() || note.is_some()) {
        let column = column.unwrap_or_else(|| {
            let c: Element = el("div", "rv-chart__aside").into();
            let _ = frame.after_with_node_1(&c);
            c
        });
        if let Some(l) = &legend
            && l.parent_element().as_ref() != Some(&column)
        {
            let _ = column.prepend_with_node_1(l);
        }
        if let Some(n) = &note
            && n.parent_element().as_ref() != Some(&column)
        {
            let _ = column.append_child(n);
        }
        let _ = figure.set_attribute("data-rv-aside", "");
        return;
    }
    let Some(column) = column else { return };
    if let Some(n) = &note {
        let _ = frame.after_with_node_1(n);
    }
    if let Some(l) = &legend {
        if v.geometry.legend.as_ref().is_some_and(|l| l.bottom) {
            let _ = frame.after_with_node_1(l);
        } else {
            let _ = frame.before_with_node_1(l);
        }
    }
    column.remove();
    let _ = figure.remove_attribute("data-rv-aside");
}

/// The area of a chart's plots, its panels' plot boxes.
fn plot_area(g: &Geometry) -> f64 {
    g.panels.iter().map(|p| p.plot.w * p.plot.h).sum()
}

/// The plot area an expanded layout shows at once: all of it when the chart fits the dialog,
/// else the share of its height in view (S27-875). A key beside the plot is kept when its layout
/// fits and shows no less: a gantt narrowed to 58 % of its width for 11 px of scroll is not, a
/// pie whose key falls 119 px under the fold is.
fn plot_in_view(sized: &Sized) -> f64 {
    let share = if sized.fits {
        1.0
    } else {
        (sized.room / sized.geometry.height.max(1.0)).clamp(0.0, 1.0)
    };
    plot_area(&sized.geometry) * share
}

/// Dim automatic value labels near a selected point and hide those under its readout. Restore
/// their original opacity when neither applies. `true` means the cached upload needs replacing.
fn hover_label_opacity(
    g: &Geometry,
    target: Option<Target>,
    readout: Option<Rect>,
    mesh: &mut Mesh,
) -> bool {
    let selected = target.and_then(|t| {
        let panel = g.panels.get(t.panel)?;
        if !matches!(g.kind.as_str(), "line" | "area" | "scatter")
            || !matches!(panel.targeting, Targeting::NearestX | Targeting::Radius)
        {
            return None;
        }
        Some((t.panel, panel.observations.get(t.observation)?))
    });
    let texts = g
        .panels
        .iter()
        .enumerate()
        .flat_map(|(panel, p)| p.items.iter().map(move |item| (Some(panel), item)))
        .chain(g.figure_items.iter().map(|item| (None, item)))
        .filter_map(|(panel, item)| match item {
            Item::Text(text) => Some((panel, text)),
            _ => None,
        });
    let mut changed = false;
    for (quad, (panel, text)) in mesh.texts.iter_mut().zip(texts) {
        // The cartesian automatic-value painter uses a marked Figures-face label. Authored
        // point names and series names use the Data face; axis labels have their own role.
        let value_label =
            text.role == Role::Label && text.face == Face::Figures && text.mark.is_some();
        let covered = value_label
            && selected.is_some()
            && readout.is_some_and(|bounds| text.bounds.overlaps(&bounds, 0.0));
        let near = value_label
            && selected.is_some_and(|(selected_panel, observation)| {
                panel == Some(selected_panel)
                    && if observation.dots.is_empty() {
                        point_near_box(observation.at, text.bounds, 24.0)
                    } else {
                        observation
                            .dots
                            .iter()
                            .any(|dot| point_near_box(dot.at, text.bounds, 24.0))
                    }
            });
        let alpha = quad.base_alpha
            * if covered {
                0.0
            } else if near {
                0.2
            } else {
                1.0
            };
        if quad.text.color.0[3] != alpha {
            quad.text.color.0[3] = alpha;
            changed = true;
        }
    }
    changed
}

/// Map the visible readout into the canvas's chart coordinates, including the turned phone view.
fn readout_bounds(v: &View) -> Option<Rect> {
    if v.target.is_none() || v.readout.hidden() {
        return None;
    }
    let readout = v.readout.get_bounding_client_rect();
    let canvas = v.canvas.get_bounding_client_rect();
    Some(if turned(v) {
        Rect::new(
            readout.top() - canvas.top(),
            canvas.right() - readout.right(),
            readout.height(),
            readout.width(),
        )
    } else {
        Rect::new(
            readout.left() - canvas.left(),
            readout.top() - canvas.top(),
            readout.width(),
            readout.height(),
        )
    })
}

/// Distance to the painted label box, including its edges and corners.
fn point_near_box(point: [f64; 2], rect: Rect, reach: f64) -> bool {
    let dx = (rect.x - point[0]).max(0.0).max(point[0] - rect.right());
    let dy = (rect.y - point[1]).max(0.0).max(point[1] - rect.bottom());
    dx.hypot(dy) <= reach
}

/// Keep the interaction boundaries paired with the exact mesh that owns the operations.
fn rebuild_mesh(v: &mut View, state: &State, dpr: f64) {
    if v.asleep {
        return;
    }
    let level = v.names.level();
    let (mesh, splits) = if level < 1.0 {
        mesh::build_for_host(&feedback::faded_names(&v.geometry, level), state, dpr)
    } else {
        mesh::build_for_host(&v.geometry, state, dpr)
    };
    v.mesh = mesh;
    v.panel_splits = splits;
}

/// Give back a sleeping chart's canvas pixels, mesh and GPU buffers; the frame shows the
/// loading state until it draws again (`lazy`).
fn release_surface(v: &mut View) {
    if let Some(up) = v.uploaded.take() {
        with_renderer(|r| r.release(up));
    }
    v.mesh = Mesh::default();
    v.panel_splits = Vec::new();
    v.retired = None;
    if v.canvas.width() != 0 {
        v.canvas.set_width(0);
    }
    if v.canvas.height() != 0 {
        v.canvas.set_height(0);
    }
    if v.painted {
        v.painted = false;
        let _ = v.frame.remove_attribute("data-rv-painted");
    }
}

/// Draw a woken chart again from the model it kept, or lay it out afresh when its width or the
/// pixel ratio changed while it slept (`lazy`).
fn restore_surface(view: &Rc<RefCell<View>>) {
    let fresh = {
        let v = view.borrow();
        let width = (v.frame.client_width() as f64).max(240.0);
        let dpr = window().device_pixel_ratio().clamp(1.0, 3.0).min(max_dpr());
        (width - v.width).abs() >= 0.5 || (dpr - v.dpr).abs() > 1e-9
    };
    if fresh {
        layout(view, false);
        return;
    }
    {
        let mut v = view.borrow_mut();
        let pixel_width = (v.width * v.dpr).round() as u32;
        let pixel_height = (v.geometry.height * v.dpr).round() as u32;
        v.canvas.set_width(pixel_width);
        v.canvas.set_height(pixel_height);
        let (state, dpr) = (v.fade.state(), v.dpr);
        rebuild_mesh(&mut v, &state, dpr);
    }
    draw(view);
}

/// Upload if needed, draw the chart with its overlay and copy it to the view's canvas.
fn draw(view: &Rc<RefCell<View>>) {
    let mut v = view.borrow_mut();
    let v = &mut *v;
    if v.asleep {
        return;
    }
    let readout = readout_bounds(v);
    if hover_label_opacity(&v.geometry, v.target, readout, &mut v.mesh)
        && let Some(up) = v.uploaded.take()
    {
        with_renderer(|r| r.release(up));
    }
    let (w, h) = (
        (v.width * v.dpr).round() as u32,
        (v.geometry.height * v.dpr).round() as u32,
    );
    let size = [v.width, v.geometry.height];
    let clock = clock_of(v);
    let overlay = overlay_items(v);
    // The keyboard's mark ring goes over the target's contour (STY-38).
    let (ring, ring_reach) = focus::ring(v);
    let crosshair = crosshair(v);
    // Between two states the chart draws the glide's frame, built afresh (FEA-CH-52).
    let gliding = v.glide.as_ref().map(|(from, start, length, steady)| {
        let g = glide_frame(from, &v.geometry, *start, *length, *steady);
        mesh::build(&g, &v.fade.state(), v.dpr)
    });
    let drawn = with_renderer(|r| {
        if let Some(m) = &gliding {
            let atlas = atlas::build(&document(), m, &v.chart.payload.theme, v.dpr)
                .ok()
                .flatten();
            let up = match &atlas {
                Some(a) => r.upload(m, &a.vertices, a.ranges.clone(), Some(&a.canvas)),
                None => r.upload(m, &[], Vec::new(), None),
            };
            let Ok(up) = up else { return false };
            r.begin(w, h, v.geometry.background.0);
            r.draw(m, &up, size, v.dpr, Clock::settled());
            r.present(&v.ctx, w, h);
            r.release(up);
            return true;
        }
        // Objects from a context that was lost and replaced cannot draw here.
        if v.uploaded.as_ref().is_some_and(|up| !up.belongs_to(r)) {
            v.uploaded = None;
        }
        if v.uploaded.is_none() {
            let atlas = atlas::build(&document(), &v.mesh, &v.chart.payload.theme, v.dpr)
                .ok()
                .flatten();
            let unloaded = atlas.as_ref().map_or(&[][..], |a| &a.unloaded[..]);
            v.fonts_due = !unloaded.is_empty();
            await_faces(unloaded);
            let up = match &atlas {
                Some(a) => r.upload(&v.mesh, &a.vertices, a.ranges.clone(), Some(&a.canvas)),
                None => r.upload(&v.mesh, &[], Vec::new(), None),
            };
            v.uploaded = up.ok();
        }
        let Some(up) = &v.uploaded else { return false };
        let bg = v.geometry.background.0;
        r.begin(w, h, bg);
        if let Some((split, line, plot)) = crosshair {
            let line_mesh = mesh::overlay(&[line], Some(plot), v.dpr);
            if let Ok(line_up) = r.upload(&line_mesh, &[], Vec::new(), None) {
                r.draw_range(&v.mesh, up, size, v.dpr, clock, 0..split);
                r.draw(&line_mesh, &line_up, size, v.dpr, Clock::settled());
                r.draw_range(&v.mesh, up, size, v.dpr, clock, split..v.mesh.ops.len());
                r.release(line_up);
            } else {
                r.draw(&v.mesh, up, size, v.dpr, clock);
            }
        } else {
            r.draw(&v.mesh, up, size, v.dpr, clock);
        }
        if !overlay.is_empty() {
            let plot = v
                .target
                .or(v.released.last().map(|r| r.target))
                .and_then(|t| v.geometry.panels.get(t.panel))
                .map(|p| {
                    // Nearest-x halo clipping gets one device pixel for fractional scissor
                    // rounding; other feedback keeps its existing clip.
                    let margin = if p.targeting == Targeting::NearestX {
                        8.0 + 1.0 / v.dpr
                    } else {
                        8.0
                    };
                    p.plot.inflate(margin)
                });
            let extra = mesh::overlay(&overlay, plot, v.dpr);
            if let Ok(up2) = r.upload(&extra, &[], Vec::new(), None) {
                r.draw(&extra, &up2, size, v.dpr, Clock::settled());
                r.release(up2);
            }
        }
        // The ring stays whole on a mark at the plot's edge: it may reach over the axis labels
        // as far as it reaches past its mark, where the hover feedback stops 8 px out.
        if !ring.is_empty()
            && let Some(p) = v.target.and_then(|t| v.geometry.panels.get(t.panel))
        {
            let clip = p.plot.inflate(ring_reach.max(8.0) + 1.0 / v.dpr);
            let ring_mesh = mesh::overlay(&ring, Some(clip), v.dpr);
            if let Ok(ring_up) = r.upload(&ring_mesh, &[], Vec::new(), None) {
                r.draw(&ring_mesh, &ring_up, size, v.dpr, Clock::settled());
                r.release(ring_up);
            }
        }
        r.present(&v.ctx, w, h);
        true
    });
    if drawn == Some(true) {
        let _ = v.canvas.style().remove_property("display");
        if let Ok(Some(notice)) = v.frame.query_selector(".rv-chart__notice") {
            let _ = notice.set_attribute("hidden", "");
        }
        if !v.painted {
            v.painted = true;
            let _ = v.frame.remove_attribute("data-rv-notice");
            let _ = v.frame.set_attribute("data-rv-painted", "");
        }
    } else if !renderer_pending() {
        show_notice(&v.frame);
        let _ = v.canvas.style().set_property("display", "none");
    }
}

fn clock_of(v: &View) -> Clock {
    let entrance = v.geometry.entrance;
    let marks = mesh::stagger_count(&v.geometry);
    let profile = &v.geometry.timing;
    let [grow, sweep, settle, labels] = profile.durations(entrance, marks).map(|d| d as f32);
    let time = if v.settled {
        1.0e9
    } else {
        match v.entrance {
            Some(start) => (now() - start) as f32,
            None => 0.0,
        }
    };
    Clock {
        time,
        durations: [grow, sweep, settle, labels],
        bezier: profile.entrance_curve().map(|c| c as f32),
        label: profile.labels(entrance, marks)[1] as f32,
        sweep: [0.0, v.width as f32],
    }
}

fn entrance_length(v: &View) -> f64 {
    let clock = clock_of(v);
    clock.durations[3] as f64 + clock.label as f64
}

/// Play the entrance once half the chart is in view (FEA-CH-51).
fn watch_entrance(view: &Rc<RefCell<View>>) {
    let weak = Rc::downgrade(view);
    let callback = Closure::<dyn FnMut(js_sys::Array, web_sys::IntersectionObserver)>::new(
        move |entries: js_sys::Array, observer: web_sys::IntersectionObserver| {
            let visible = entries.iter().any(|e| {
                e.unchecked_into::<web_sys::IntersectionObserverEntry>()
                    .is_intersecting()
            });
            if !visible {
                return;
            }
            observer.disconnect();
            if let Some(v) = weak.upgrade() {
                play(&v);
            }
        },
    );
    let options = web_sys::IntersectionObserverInit::new();
    let height = view.borrow().geometry.height;
    let screen = window()
        .inner_height()
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(800.0);
    // Half the chart, or half the screen for a chart taller than that.
    let threshold = if height > screen {
        (screen / 2.0 / height).clamp(0.05, 0.5)
    } else {
        0.5
    };
    options.set_threshold(&JsValue::from(threshold));
    if let Ok(observer) =
        web_sys::IntersectionObserver::new_with_options(callback.as_ref().unchecked_ref(), &options)
    {
        observer.observe(&view.borrow().frame);
    }
    callback.forget();
}

fn play(view: &Rc<RefCell<View>>) {
    if view.borrow().settled {
        return;
    }
    view.borrow_mut().entrance = Some(now());
    frame_loop(Rc::downgrade(view));
}

fn frame_loop(weak: Weak<RefCell<View>>) {
    let Some(view) = weak.upgrade() else { return };
    let done = {
        let v = view.borrow();
        v.settled || v.entrance.is_some_and(|s| now() - s >= entrance_length(&v))
    };
    if done {
        view.borrow_mut().settled = true;
        draw(&view);
        check_settled();
        return;
    }
    draw(&view);
    let next = Closure::once_into_js(move || frame_loop(weak));
    let _ = window().request_animation_frame(next.unchecked_ref());
}

/// Settle every chart now: entrances finish and charts draw at rest (FEA-CH-54).
pub fn settle_all() {
    lazy::mount_every();
    let views = every_view();
    let keep_painted = RENDERER.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|renderer| !renderer.pending() && (renderer.invalid() || renderer.lost()))
    });
    for view in views {
        let already_settled = view.borrow().settled;
        view.borrow_mut().settled = true;
        if !keep_painted || !already_settled {
            draw(&view);
        }
    }
    check_settled();
}

/// A preference change also stops already running playback, state glides and feedback.
pub fn motion_changed() {
    if !reduced_motion() {
        return;
    }
    for view in every_view() {
        controls::pause(&view);
        {
            let mut v = view.borrow_mut();
            v.settled = true;
            v.glide = None;
        }
        feedback::settle(&view);
    }
    check_settled();
}

/// Print: every chart settled, in the paper's theme and drawn sharp for paper (FEA-CH-67).
pub fn print() {
    PRINTING.with(|p| p.set(true));
    // Paper shows every chart: those still waiting mount, and sleeping ones draw (MOTION.md).
    lazy::mount_every();
    let views = every_view();
    for view in views {
        {
            let mut v = view.borrow_mut();
            v.settled = true;
            v.target = None;
            v.readout.set_hidden(true);
        }
        // Paper shows every part of the page, also those the screen hides, such as the panels of
        // tabs not chosen, where a chart was laid out at the minimum width. Each such part shows
        // while its chart is laid out at the width the part takes, and hides again before the
        // screen can paint (FEA-CH-67).
        let figure = view.borrow().figure.clone();
        let shown = show_hidden_parts(&figure);
        // A dark style prints in its light counterpart's theme, as the page around it does
        // (STY-21): its colours, and the sizes they come with, need a layout of their own.
        let repaper = view.borrow_mut().chart.set_paper(true);
        if repaper || !shown.is_empty() {
            layout(&view, false);
        } else {
            let dpr = view.borrow().dpr;
            if dpr < 2.0 {
                let mut v = view.borrow_mut();
                v.dpr = 2.0;
                let (w, h) = (v.width, v.geometry.height);
                v.canvas.set_width((w * 2.0).round() as u32);
                v.canvas.set_height((h * 2.0).round() as u32);
                if let Some(up) = v.uploaded.take() {
                    with_renderer(|r| r.release(up));
                }
                let state = State::default();
                rebuild_mesh(&mut v, &state, 2.0);
            }
            draw(&view);
        }
        for part in shown {
            let _ = part.set_attribute("hidden", "");
        }
    }
}

/// The hidden ancestors of `figure`, each shown by taking its `hidden` attribute away; the caller
/// puts it back.
fn show_hidden_parts(figure: &Element) -> Vec<Element> {
    let mut shown = Vec::new();
    let mut node = figure.parent_element();
    while let Some(part) = node {
        if part.has_attribute("hidden") && part.remove_attribute("hidden").is_ok() {
            shown.push(part.clone());
        }
        node = part.parent_element();
    }
    shown
}

/// Each mounted chart's current geometry model and reading state as JSON, read-only, for
/// `window.__richview.charts()` (DEL-72, FEA-RP-37). Values are those of the rounded payload
/// (FEA-CH-37), so the model reveals nothing the page does not show. The charts come in
/// document order, whichever mounted first: a chart near the screen mounts before one above it
/// (`lazy`), and a check pairs `charts()[k]` with the page's k-th figure.
pub fn models() -> Vec<String> {
    let shaders = renderer_pending();
    let mut views = VIEWS.with(|views| views.borrow().clone());
    views.sort_by(|a, b| {
        let (a, b) = (a.borrow().figure.clone(), b.borrow().figure.clone());
        if a == b {
            std::cmp::Ordering::Equal
        } else if a.compare_document_position(&b) & web_sys::Node::DOCUMENT_POSITION_FOLLOWING != 0
        {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        }
    });
    views
        .iter()
        .map(|view| {
                let v = view.borrow();
                serde_json::json!({
                    "id": v.chart.payload.id,
                    "figure": v.figure.id(),
                    "width": v.width,
                    "canvas": {"width": v.canvas.width(), "height": v.canvas.height(), "dpr": v.dpr},
                    "hidden": v.hidden,
                    "zoom": v.zoom,
                    "target": v.target.map(|t| serde_json::json!({"panel": t.panel, "observation": t.observation})),
                    "pinned": v.pinned,
                    "settled": !shaders && v.settled && v.glide.is_none(),
                    "state": controls::state_name(&v),
                    "playing": v.playing.is_some(),
                    "expanded": v.expanded.is_some(),
                    "geometry": v.geometry,
                })
                .to_string()
            })
        .collect()
}

pub fn pending() -> Vec<String> {
    let shaders = renderer_pending();
    every_view()
        .iter()
        .filter(|view| {
            let v = view.borrow();
            shaders
                || !v.settled
                || (v.fonts_due && FONT_LOADS.with(Cell::get) > 0)
                || v.glide.is_some()
                || v.zoom_glide.is_some()
                || v.animating
                || v.relayout
                || v.scrolling.is_some()
                || v.leaving.is_some()
                || readout_fading(&v)
        })
        .map(|view| view.borrow().chart.payload.id.clone())
        .chain(lazy::waiting())
        .collect()
}

/// Mount `figure` now if it is still waiting, as a scroll story does before it moves the chart.
pub fn mount_now(figure: &Element) {
    lazy::mount_now(figure);
}

/// Lay out every chart whose width changed, such as those in a tab panel that just opened, so
/// the model and the canvas agree as soon as the panel shows.
pub fn refresh() {
    redraw_all(false);
}

/// Back on screen after printing: every chart in the screen's theme and at its pixel ratio
/// again. `afterprint` and the end of print media both say so; the first one acts.
pub fn after_print() {
    if !PRINTING.with(|p| p.replace(false)) {
        return;
    }
    for view in every_view() {
        view.borrow_mut().chart.set_paper(false);
    }
    redraw_all(true);
}

/// Load the faces an atlas drew without, then draw every chart again in them. WebKit fires no
/// `loadingdone` for faces that CSS loads, so that event alone left a chart that mounted before
/// its fonts in a fallback face (FEA-CH-30). Each font and its characters are asked for once;
/// while a load is in flight, the charts that wait for it are pending (FEA-RP-37).
fn await_faces(unloaded: &[(String, String)]) {
    for (font, text) in unloaded {
        if !FONTS_ASKED.with(|asked| asked.borrow_mut().insert(format!("{font}\u{1f}{text}"))) {
            continue;
        }
        let load = document().fonts().load_with_text(font, text);
        FONT_LOADS.with(|n| n.set(n.get() + 1));
        let done = Closure::once(move || {
            FONT_LOADS.with(|n| n.set(n.get() - 1));
            if FONT_LOADS.with(Cell::get) == 0 {
                if every_view().iter().any(|v| v.borrow().fonts_due) {
                    redraw_all(true);
                }
                check_settled();
            }
        });
        let _ = load.finally(&done);
        done.forget();
    }
}

fn redraw_all(relayout: bool) {
    if relayout {
        // Fonts arrived: the title's glyph is measured again in its real face.
        title::forget();
    }
    for view in every_view() {
        if relayout {
            let figure = view.borrow().figure.clone();
            title::apply(&figure);
            if let Some(up) = view.borrow_mut().uploaded.take() {
                with_renderer(|r| r.release(up));
            }
            view.borrow_mut().fill = None;
            view.borrow_mut().narrowed = None;
            layout(&view, false);
        } else if view.borrow().owner.is_some()
            || (view.borrow().frame.client_width() as f64 - view.borrow().width).abs() >= 0.5
        {
            // The expanded view fits the window, which its frame alone does not follow.
            layout(&view, false);
        }
    }
}

/// The hover dot in its two-pixel surface ring, inside a soft halo of its own colour that
/// reaches four pixels past the ring (FEA-CH-53).
fn hover_dot(items: &mut Vec<Item>, at: [f64; 2], grown: f64, color: Rgba, surface: Rgba) {
    // The dot, its ring and its halo grow in together.
    let scale = 0.4 + 0.6 * grown;
    for (radius, fill) in [
        (
            (8.0 + contour::HALO_REACH) * scale,
            contour::halo(color, grown),
        ),
        (
            8.0 * scale,
            surface.with_alpha(surface.0[3] as f64 * grown.min(1.0)),
        ),
        (
            6.0 * scale,
            color.with_alpha(color.0[3] as f64 * grown.min(1.0)),
        ),
    ] {
        items.push(Item::Point {
            center: at,
            radius,
            shape: Shape::Circle,
            fill: Some(Fill::solid(fill)),
            stroke: None,
            role: Role::Annotation,
            series: None,
            obs: None,
        });
    }
}

/// Keep the full one-CSS-pixel stroke inside both the plot and its GL scissor.
fn crosshair_x(plot: Rect, x: f64, dpr: f64) -> Option<f64> {
    if !plot.x.is_finite()
        || !plot.w.is_finite()
        || !x.is_finite()
        || !dpr.is_finite()
        || dpr <= 0.0
    {
        return None;
    }
    // Match Renderer::scissor: its origin is floored and its width is ceiled separately.
    let left = plot.x * dpr;
    let right = plot.right() * dpr;
    let scissor_left = left.floor() as i32 as f64;
    let scissor_width = (plot.w * dpr).ceil().max(0.0) as i32 as f64;
    let scissor_right = scissor_left + scissor_width;
    let first = left.max(scissor_left).ceil();
    let last = (right.min(scissor_right) - dpr).floor();
    if !first.is_finite() || !last.is_finite() || first > last {
        return None;
    }
    let leading = (x * dpr - dpr / 2.0).round().clamp(first, last);
    Some((leading + dpr / 2.0) / dpr)
}

fn crosshair_line(plot: Rect, x: f64, dpr: f64, ink: Rgba) -> Option<Item> {
    let x = crosshair_x(plot, x, dpr)?;
    Some(Item::Line {
        path: mesh::polyline(&[[x, plot.y], [x, plot.bottom()]]),
        stroke: Stroke {
            dash: vec![3.0, 3.0],
            ..Stroke::solid(ink.with_alpha(0.7), 1.0)
        },
        role: Role::Annotation,
        series: None,
        obs: None,
    })
}

/// The selected panel's crosshair and its safe position between grid and marks.
fn crosshair(v: &View) -> Option<(usize, Item, Rect)> {
    let target = v.target?;
    // It appears at once and fades only as the feedback lets go.
    let level = feedback::crosshair_level(v);
    if level <= 0.0 {
        return None;
    }
    let panel = v.geometry.panels.get(target.panel)?;
    if panel.targeting != Targeting::NearestX {
        return None;
    }
    let observation = panel.observations.get(target.observation)?;
    let split = v.panel_splits.get(target.panel).copied().flatten()?;
    if split > v.mesh.ops.len() {
        return None;
    }
    let soft = richview_chart::color::parse(&v.chart.payload.theme.ink_soft)?;
    let line = crosshair_line(
        panel.plot,
        observation.x,
        v.dpr,
        Rgba::from_oklch(soft, level),
    )?;
    Some((split, line, panel.plot))
}

/// The frame of a glide from `from` to `to` that began at `start` and lasts `length`
/// milliseconds: eased in and out, or at one steady pace for playback, whose glides run on into
/// each other (FEA-CH-52, FEA-BR-02).
pub(super) fn glide_frame(
    from: &Geometry,
    to: &Geometry,
    start: f64,
    length: f64,
    steady: bool,
) -> Geometry {
    let t = ((now() - start) / length).clamp(0.0, 1.0);
    let eased = if steady {
        t
    } else if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    };
    crate::tween::between(from, to, eased)
}

/// Hover feedback from the model's own positions: crosshair, hover dots and target outline,
/// with the drag-zoom selection under them.
fn overlay_items(v: &View) -> Vec<Item> {
    let mut items = Vec::new();
    if let Some((x0, x1)) = v.drag
        && let Some(panel) = v.geometry.panels.first()
    {
        let accent = &v.chart.payload.theme.accent;
        items.extend(match v.drag_y {
            Some((y0, y1)) => drag_box(accent, panel.plot, [x0, y0], [x1, y1]),
            None => drag_band(accent, panel.plot, x0, x1),
        });
    }
    // What the pointer left lets go underneath what it reached.
    for r in &v.released {
        overlay_for(v, r.target, r.level(), &mut items);
    }
    if let Some(t) = v.target {
        overlay_for(v, t, feedback::grown(v), &mut items);
    }
    items
}

/// How far a pointer moves along x before a press becomes a drag-zoom, in CSS pixels.
const DRAG_SLOP: f64 = 6.0;

/// The window a drag from `x0` to `x1` selects, on every zoomable chart alike (FEA-CH-49): the
/// plot's full height between the two x positions, inside the plot, tinted in the accent with
/// a crisp edge at each end, so the reader sees exactly what the zoom will show.
fn drag_band(accent: &str, plot: Rect, x0: f64, x1: f64) -> Vec<Item> {
    let (a, b) = (
        x0.min(x1).clamp(plot.x, plot.right()),
        x0.max(x1).clamp(plot.x, plot.right()),
    );
    if b - a <= 0.0 {
        return Vec::new();
    }
    let accent = richview_chart::color::parse(accent)
        .unwrap_or(richview_chart::color::Oklch::new(0.5, 0.2, 260.0));
    let rect = |x: f64, w: f64, alpha: f64| Item::Rect {
        rect: Rect::new(x, plot.y, w, plot.h),
        fill: Fill::solid(Rgba::from_oklch(accent, alpha)),
        stroke: None,
        radius: [0.0; 4],
        role: Role::Band,
        series: None,
        obs: None,
    };
    let mut items = vec![rect(a, b - a, 0.12)];
    if b - a > DRAG_SLOP {
        items.push(rect(a, 1.0, 0.7));
        items.push(rect(b - 1.0, 1.0, 0.7));
    }
    items
}

/// The window a drag from `p0` to `p1` selects on a chart that zooms along both axes (a map):
/// the rectangle between the two points, inside the plot, in the same tint and edges as
/// [`drag_band`] (FEA-CH-49).
fn drag_box(accent: &str, plot: Rect, p0: [f64; 2], p1: [f64; 2]) -> Vec<Item> {
    let (a, b) = (
        p0[0].min(p1[0]).clamp(plot.x, plot.right()),
        p0[0].max(p1[0]).clamp(plot.x, plot.right()),
    );
    let (c, d) = (
        p0[1].min(p1[1]).clamp(plot.y, plot.bottom()),
        p0[1].max(p1[1]).clamp(plot.y, plot.bottom()),
    );
    if b - a <= 0.0 && d - c <= 0.0 {
        return Vec::new();
    }
    let accent = richview_chart::color::parse(accent)
        .unwrap_or(richview_chart::color::Oklch::new(0.5, 0.2, 260.0));
    let rect = |x: f64, y: f64, w: f64, h: f64, alpha: f64| Item::Rect {
        rect: Rect::new(x, y, w, h),
        fill: Fill::solid(Rgba::from_oklch(accent, alpha)),
        stroke: None,
        radius: [0.0; 4],
        role: Role::Band,
        series: None,
        obs: None,
    };
    let (w, h) = ((b - a).max(1.0), (d - c).max(1.0));
    let mut items = vec![rect(a, c, w, h, 0.12)];
    if w.max(h) > DRAG_SLOP {
        items.push(rect(a, c, 1.0, h, 0.7));
        items.push(rect(a + w - 1.0, c, 1.0, h, 0.7));
        items.push(rect(a, c, w, 1.0, 0.7));
        items.push(rect(a, c + h - 1.0, w, 1.0, 0.7));
    }
    items
}

/// Show or hide the quiet line that tells a mouse a chart zooms: "Drag to zoom", and once
/// zoomed "Drag to zoom · Shift-drag to pan", in soft ink (FEA-CH-49). Every chart that zooms
/// says so, in the first of three places that is free: the right end of the row above the plot,
/// where the y axis's header sits at the left; the title row, against the action icons; a
/// corner inside the plot that holds no mark and no text. It stays hidden while a drag or pan
/// runs and on a turned chart.
fn zoom_hint(v: &mut View, on: bool) {
    let on = on && zoomable(v) && !turned(v) && v.drag.is_none() && v.pan.is_none();
    v.hint_on = on;
    let hint = match &v.hint {
        Some(hint) => hint.clone(),
        None if !on => return,
        None => {
            let hint = el("p", "rv-chart__hint");
            let _ = hint.set_attribute("aria-hidden", "true");
            let style = hint.style();
            for (key, value) in [
                ("position", "absolute"),
                ("left", "0"),
                ("top", "0"),
                ("margin", "0"),
                ("z-index", "1"),
                ("pointer-events", "none"),
                ("white-space", "nowrap"),
                ("font", "12px/16px var(--rv-font-ui, system-ui)"),
                ("color", "var(--rv-ink-soft)"),
                ("opacity", "0"),
            ] {
                let _ = style.set_property(key, value);
            }
            if !reduced_motion() {
                let _ = style.set_property("transition", "opacity 150ms ease-out");
            }
            let _ = v.frame.append_child(&hint);
            v.hint = Some(hint.clone());
            hint
        }
    };
    let place = on.then(|| {
        let zoomed = v.zoom_glide.as_ref().map_or(v.zoom, |g| g.end()).is_some();
        let text = if zoomed {
            "Drag to zoom · Shift-drag to pan"
        } else {
            "Drag to zoom"
        };
        // The place is found for the longer line and kept for both, so the hint does not move
        // when a zoom starts (Chart design, 16 November); the shorter line stands at the end
        // of that place nearer the plot's edge.
        hint.set_text_content(Some("Drag to zoom · Shift-drag to pan"));
        let long = f64::from(hint.offset_width());
        hint.set_text_content(Some(text));
        let short = f64::from(hint.offset_width());
        let plot = v.geometry.panels.first()?.plot;
        let size = [long, 16.0];
        // Level with the y axis's header: the text nearest above the plot, at its left.
        let header = v
            .geometry
            .items()
            .filter_map(|item| match item {
                Item::Text(t)
                    if t.bounds.bottom() <= plot.y + 0.5 && t.bounds.x < plot.x + plot.w / 2.0 =>
                {
                    Some(t.bounds)
                }
                _ => None,
            })
            .max_by(|a, b| a.bottom().total_cmp(&b.bottom()));
        let y = header.map_or(plot.y - size[1] - 2.0, |h| h.center()[1] - size[1] / 2.0);
        let rect = Rect::new(plot.right() - size[0], y, size[0], size[1]);
        let clear = y >= 0.0
            && !v.geometry.items().any(
                |item| matches!(item, Item::Text(t) if t.bounds.overlaps(&rect.inflate(4.0), 0.0)),
            );
        let (place, plate) = clear
            .then_some((rect, false))
            .or_else(|| hint_by_actions(v, size).map(|rect| (rect, false)))
            .or_else(|| hint_place::corner(&v.geometry, plot, size).map(|rect| (rect, true)))?;
        // Every place but a left corner ends at the right: the shorter line ends there too.
        let x = if plate && place.center()[0] < plot.center()[0] {
            place.x
        } else {
            place.right() - short
        };
        Some((Rect::new(x, place.y, short, place.h), plate))
    });
    let style = hint.style();
    match place.flatten() {
        Some((rect, plate)) => {
            let _ = style.set_property(
                "transform",
                &format!("translate({}px, {}px)", rect.x, rect.y),
            );
            // Inside the plot the line stands on the chart's own paper, over the grid.
            let paper = if plate {
                "var(--rv-bg-chart, var(--rv-bg, transparent))"
            } else {
                "transparent"
            };
            let _ = style.set_property("background", paper);
            let _ = style.set_property(
                "box-shadow",
                &format!("0 0 0 {}px {paper}", hint_place::PLATE),
            );
            let _ = style.set_property("opacity", "1");
        }
        None => {
            let _ = style.set_property("opacity", "0");
        }
    }
}

/// The hint's place in the title row: against the action icons, on their centre line, when the
/// title's text ends at least 12 px before it. In the frame's coordinates, like the plot's.
fn hint_by_actions(v: &View, size: [f64; 2]) -> Option<Rect> {
    let actions = v
        .figure
        .query_selector(".rv-chart__titlerow > .rv-chart__actions")
        .ok()??;
    if actions.get_client_rects().length() == 0 {
        return None;
    }
    let (a, f) = (
        actions.get_bounding_client_rect(),
        v.frame.get_bounding_client_rect(),
    );
    let left = a.left() - 12.0 - size[0];
    // Where the title's text ends on the icons' row: a title that wraps fills its box.
    let title_end = v
        .figure
        .query_selector(".rv-chart__titlerow > .rv-chart__title")
        .ok()
        .flatten()
        .and_then(|title| {
            let t = title.get_bounding_client_rect();
            if t.width() <= 0.0 || t.bottom() <= a.top() || t.top() >= a.bottom() {
                return None;
            }
            Some(match one_line_width(&title) {
                Some(w) if w <= t.width() + 0.5 => t.left() + w,
                _ => t.right(),
            })
        })
        .unwrap_or(f.left() - 12.0);
    (left >= title_end + 12.0 && left >= f.left()).then(|| {
        Rect::new(
            left - f.left(),
            a.top() + a.height() / 2.0 - size[1] / 2.0 - f.top(),
            size[0],
            size[1],
        )
    })
}

/// The width `element`'s text takes on one line, measured on a hidden copy beside it.
fn one_line_width(element: &Element) -> Option<f64> {
    let copy: HtmlElement = element.clone_node_with_deep(true).ok()?.dyn_into().ok()?;
    let _ = copy.remove_attribute("id");
    let _ = copy.set_attribute("aria-hidden", "true");
    let style = copy.style();
    for (key, value) in [
        ("position", "absolute"),
        ("left", "0"),
        ("top", "0"),
        ("visibility", "hidden"),
        ("pointer-events", "none"),
        ("width", "max-content"),
        ("white-space", "nowrap"),
        ("flex", "none"),
    ] {
        let _ = style.set_property(key, value);
    }
    element.parent_node()?.append_child(&copy).ok()?;
    let width = copy.get_bounding_client_rect().width();
    copy.remove();
    Some(width)
}

/// The feedback of one target at `grown`, 0 to 1.
fn overlay_for(v: &View, t: Target, grown: f64, items: &mut Vec<Item>) {
    if grown <= 0.0 {
        return;
    }
    let Some(panel) = v.geometry.panels.get(t.panel) else {
        return;
    };
    let Some(o) = panel.observations.get(t.observation) else {
        return;
    };
    let theme = &v.chart.payload.theme;
    let ink = richview_chart::color::parse(&theme.ink_faint)
        .map(|c| Rgba::from_oklch(c, 1.0))
        .unwrap_or(Rgba([0.5, 0.5, 0.5, 1.0]));
    let surface = v.geometry.background;
    let contour_ink = richview_chart::color::parse(&theme.ink)
        .map(|c| Rgba::from_oklch(c, 1.0))
        .unwrap_or(ink);
    if let Some(contour) = contour::items(
        v.chart.block.kind.name(),
        panel,
        t.observation,
        o.wedge,
        if matches!(v.chart.block.kind.name(), "hexbin" | "venn") {
            ink
        } else {
            contour_ink
        },
        surface,
        grown,
    ) {
        items.extend(contour);
        return;
    }
    match panel.targeting {
        // Filled stream layers keep their crosshair and readout without point markers.
        Targeting::NearestX if v.chart.block.kind.name() == "streamgraph" => {}
        Targeting::NearestX => {
            for d in &o.dots {
                let color = v.geometry.series.get(d.series).map_or(ink, |s| s.color);
                hover_dot(items, d.at, grown, color, surface);
            }
            if o.dots.is_empty() {
                let color = o
                    .series
                    .and_then(|s| v.geometry.series.get(s))
                    .map_or(ink, |s| s.color);
                hover_dot(items, o.at, grown, color, surface);
            }
        }
        Targeting::Radius => {
            // The hovered point grows by 25% (FEA-CH-53) inside its ring.
            let scale = 1.0 + 0.25 * grown;
            let mark = panel.items.iter().find(|i| {
                matches!(i, Item::Point { obs: Some(k), role: Role::Mark, .. } if *k == t.observation)
            });
            if let Some(Item::Point {
                radius,
                shape,
                fill,
                stroke,
                ..
            }) = mark
            {
                items.push(Item::Point {
                    center: o.at,
                    radius: radius * scale,
                    shape: *shape,
                    fill: *fill,
                    stroke: stroke.clone(),
                    role: Role::Annotation,
                    series: None,
                    obs: None,
                });
            }
            items.push(Item::Point {
                center: o.at,
                radius: o.radius.max(4.0) * scale + 2.0,
                shape: Shape::Circle,
                fill: None,
                stroke: Some(Stroke::solid(ink.with_alpha(grown), 1.5)),
                role: Role::Annotation,
                series: None,
                obs: None,
            });
        }
        // A wedge outside the contoured families keeps its place: nothing moves on hover.
        Targeting::Contain if o.wedge.is_some() => {}
        Targeting::Contain => {
            if let Some(contours) = contour::tiles(
                v.chart.block.kind.name(),
                panel,
                t.observation,
                contour_ink,
                grown,
            ) {
                items.extend(contours);
                return;
            }
            // A grid node has no series: it answers in ink, as the contours of other marks do.
            if v.chart.block.kind.name() == "contour" {
                if let Some(dot) = contour::node(panel, t.observation, grown, contour_ink, surface)
                {
                    items.extend(dot);
                }
                return;
            }
            if panel.observations.len() > 1
                && let Some(a) = o.area
            {
                items.push(Item::Rect {
                    rect: a,
                    fill: Fill::solid(Rgba([0.0, 0.0, 0.0, 0.0])),
                    stroke: Some(Stroke::solid(ink.with_alpha(grown), 1.5)),
                    radius: [0.0; 4],
                    role: Role::Annotation,
                    series: None,
                    obs: None,
                });
            }
        }
    }
}

/// Convert a client-space rectangle to the chart's coordinates, including a turned dialog.
fn readout_chart_rect(v: &View, left: f64, top: f64, right: f64, bottom: f64) -> Rect {
    let canvas = v.canvas.get_bounding_client_rect();
    if turned(v) {
        Rect::new(
            top - canvas.top(),
            canvas.right() - right,
            (bottom - top).max(0.0),
            (right - left).max(0.0),
        )
    } else {
        let sx = v.width / canvas.width().max(1.0);
        let sy = v.geometry.height / canvas.height().max(1.0);
        Rect::new(
            (left - canvas.left()) * sx,
            (top - canvas.top()) * sy,
            (right - left).max(0.0) * sx,
            (bottom - top).max(0.0) * sy,
        )
    }
}

/// The width and height of where a readout may sit, 8 px in: the visible figure on a chart
/// narrower than 480 px, the visible chart otherwise (FEA-CH-42, FEA-CH-56).
fn readout_room(v: &View) -> [f64; 2] {
    let visible = readout_visible_bounds(v);
    let [w, h] = if v.width < 480.0 {
        [visible.w, visible.h]
    } else {
        [
            visible.right().min(v.width) - visible.x.max(0.0),
            visible.bottom().min(v.geometry.height) - visible.y.max(0.0),
        ]
    };
    [(w - 16.0).max(1.0), (h - 16.0).max(1.0)]
}

/// The figure and viewport, intersected with every clipping ancestor, in chart coordinates.
fn readout_visible_bounds(v: &View) -> Rect {
    let viewport_width = window()
        .inner_width()
        .ok()
        .and_then(|n| n.as_f64())
        .unwrap_or(v.width);
    let viewport_height = window()
        .inner_height()
        .ok()
        .and_then(|n| n.as_f64())
        .unwrap_or(v.geometry.height);
    let figure = v.figure.get_bounding_client_rect();
    let (mut left, mut top, mut right, mut bottom) = (
        0.0_f64.max(figure.left()),
        0.0_f64.max(figure.top()),
        viewport_width.min(figure.right()),
        viewport_height.min(figure.bottom()),
    );
    let mut parent = v.readout.parent_element();
    while let Some(element) = parent {
        if let Ok(Some(style)) = window().get_computed_style(&element) {
            let clips = |property: &str| {
                style
                    .get_property_value(property)
                    .ok()
                    .is_some_and(|value| {
                        matches!(value.as_str(), "hidden" | "clip" | "auto" | "scroll")
                    })
            };
            if clips("overflow-x") || clips("overflow-y") {
                let rect = element.get_bounding_client_rect();
                let x = rect.left() + element.client_left() as f64;
                let y = rect.top() + element.client_top() as f64;
                if clips("overflow-x") {
                    left = left.max(x);
                    right = right.min(x + element.client_width() as f64);
                }
                if clips("overflow-y") {
                    top = top.max(y);
                    bottom = bottom.min(y + element.client_height() as f64);
                }
            }
        }
        parent = element.parent_element();
    }
    readout_chart_rect(v, left, top, right, bottom)
}

/// Whether the readout is fading out; a readout shown again drops the class and stays.
fn readout_fading(v: &View) -> bool {
    v.readout
        .class_list()
        .contains("rv-chart__readout--leaving")
}

/// The readout's hold is over: it fades for 100 ms, then hides (FEA-CH-42). Reduced motion
/// hides it at once.
fn fade_readout(v: &mut View) {
    if v.readout.hidden() || readout_fading(v) {
        return;
    }
    if reduced_motion() {
        v.readout.set_hidden(true);
        return;
    }
    let _ = v.readout.class_list().add_1("rv-chart__readout--leaving");
    let readout = v.readout.clone();
    let done = Closure::once_into_js(move || {
        if readout.class_list().contains("rv-chart__readout--leaving") {
            readout.set_hidden(true);
            let _ = readout.class_list().remove_1("rv-chart__readout--leaving");
        }
        check_settled();
    });
    let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(
        done.unchecked_ref(),
        feedback::READOUT_FADE,
    );
}

/// The shown boxes of the figure's elements matching `selector`, in chart coordinates.
fn figure_rects(v: &View, selector: &str) -> Vec<Rect> {
    let Ok(elements) = v.figure.query_selector_all(selector) else {
        return Vec::new();
    };
    (0..elements.length())
        .filter_map(|i| elements.item(i)?.dyn_into::<Element>().ok())
        .map(|element| element.get_bounding_client_rect())
        .filter(|rect| rect.width() > 0.0 && rect.height() > 0.0)
        .map(|rect| readout_chart_rect(v, rect.left(), rect.top(), rect.right(), rect.bottom()))
        .collect()
}

/// Show the readout for the current target (FEA-CH-40, FEA-CH-42) and announce it.
/// While the chart emphasises one series, the readout's rows for the other series step back to
/// soft ink and the emphasised series' row stays in ink, as the layers behind them do (FEA-CH-52).
fn readout_rows_follow_emphasis(v: &View) {
    let kept = match &v.emphasis {
        Some(Focus::Series(s)) => Some(s.to_string()),
        _ => None,
    };
    let Ok(rows) = v.readout.query_selector_all("[data-rv-row-series]") else {
        return;
    };
    for i in 0..rows.length() {
        let Some(row) = rows.item(i).and_then(|n| n.dyn_into::<Element>().ok()) else {
            continue;
        };
        let back = kept
            .as_deref()
            .is_some_and(|s| row.get_attribute("data-rv-row-series").as_deref() != Some(s));
        let _ = row
            .class_list()
            .toggle_with_force("rv-chart__readout-line--back", back);
    }
}

fn show_readout(v: &mut View, announce: bool) {
    let Some(t) = v.target else {
        // A dismissal hides the readout at once; one the pointer wandered away from is
        // already fading (FEA-CH-42).
        if !readout_fading(v) {
            v.readout.set_hidden(true);
        }
        return;
    };
    let _ = v
        .readout
        .class_list()
        .remove_1("rv-chart__readout--leaving");
    let Some(o) = v
        .geometry
        .panels
        .get(t.panel)
        .and_then(|p| p.observations.get(t.observation))
    else {
        return;
    };
    let r = &v.readout;
    // A readout that appears lands in place; one already showing glides to the next
    // observation (FEA-CH-42).
    let appearing = r.hidden();
    r.set_inner_html("");
    let _ = r.style().remove_property("pointer-events");
    // One derived value can share a row; multiple values stay visible except on flow links.
    let lines = crate::readout::shown_lines(
        &o.readout.lines,
        v.geometry.panels[t.panel].targeting,
        &v.geometry.kind,
    );
    let _ = r.class_list().toggle_with_force(
        "rv-chart__readout--compact",
        lines.len() == 1 || lines.get(1).is_some_and(|(_, soft)| *soft),
    );
    let nearest_x = v.geometry.panels[t.panel].targeting == Targeting::NearestX;
    let _ = r.style().remove_property("max-width");
    let _ = r.style().remove_property("width");
    // Measure at natural width, wrapping only at the width of its room.
    let room = readout_room(v);
    let _ = r.style().set_property("width", "max-content");
    let _ = r
        .style()
        .set_property("max-width", &format!("{}px", room[0]));
    let head = el("p", "rv-chart__readout-head");
    let _ = head.style().set_property("min-width", "0");
    let _ = head.style().set_property("overflow-wrap", "anywhere");
    head.set_text_content(Some(&o.readout.heading));
    let _ = r.append_child(&head);
    let cue_rows: Vec<_> = lines.iter().map(|(line, _)| *line).collect();
    // A shared readout whose rows name every legend entry beside its swatch stands in for the
    // legend while it shows (P1b).
    let stands_in = nearest_x
        && v.geometry.legend.as_ref().is_some_and(|legend| {
            !legend.entries.is_empty()
                && legend.entries.iter().all(|&i| {
                    lines.iter().any(|(line, _)| {
                        line.series == Some(i)
                            && crate::readout::show_cue(line, &cue_rows, true)
                            && v.geometry
                                .series
                                .get(i)
                                .is_some_and(|s| s.name == line.name)
                    })
                })
        });
    for (line, soft) in lines {
        let row = el("p", "rv-chart__readout-line");
        if soft {
            let _ = row.class_list().add_1("rv-chart__readout-line--soft");
        }
        if let Some(s) = line.series.filter(|_| line.names_series) {
            let _ = row.set_attribute("data-rv-row-series", &s.to_string());
        }
        if let Some((i, s)) = line
            .series
            .and_then(|i| v.geometry.series.get(i).map(|s| (i, s)))
            .filter(|_| crate::readout::show_cue(line, &cue_rows, v.geometry.legend.is_some()))
        {
            let cue = el("span", "rv-chart__cue");
            // The series' hatch, dash or marker, as its legend swatch shows it (FEA-CH-57).
            let swatch = v
                .geometry
                .legend
                .as_ref()
                .map_or(richview_chart::geometry::Swatch::Square, |l| l.swatch);
            match richview_chart::vector::legend_swatch(&v.geometry, i, swatch) {
                Some(svg) => {
                    cue.set_inner_html(&svg);
                    let _ = cue.class_list().add_1("rv-chart__cue--decal");
                }
                None => {
                    let _ = cue.style().set_property("background", &s.color.hex());
                }
            }
            let _ = row.append_child(&cue);
        }
        let name = el("span", "rv-chart__readout-name");
        name.set_text_content(Some(&line.name));
        let value = el("span", "rv-chart__readout-value");
        value.set_text_content(Some(&line.value));
        let _ = row.style().set_property("flex-wrap", "wrap");
        let _ = row.style().set_property("min-width", "0");
        for text in [&name, &value] {
            let _ = text.style().set_property("min-width", "0");
            let _ = text.style().set_property("max-width", "100%");
            let _ = text.style().set_property("overflow-wrap", "anywhere");
        }
        let _ = row.append_child(&name);
        let _ = row.append_child(&value);
        let _ = r.append_child(&row);
    }
    readout_rows_follow_emphasis(v);
    // A long readout scrolls inside the chart instead of leaving it (FEA-CH-42), with Page Up and
    // Page Down (FEA-CH-56); each observation opens at its first line.
    let panel = &v.geometry.panels[t.panel];
    let _ = r
        .style()
        .set_property("max-height", &format!("{}px", room[1]));
    r.set_hidden(false);
    r.set_scroll_top(0);
    let width = window()
        .get_computed_style(r)
        .ok()
        .flatten()
        .and_then(|s| s.get_property_value("width").ok())
        .and_then(|s| s.trim().strip_suffix("px")?.parse::<f64>().ok())
        .filter(|width| width.is_finite())
        .unwrap_or_else(|| r.offset_width() as f64 + 0.5);
    // offsetHeight rounds away fractional pixels, which can push a dock into the plot.
    let size = [width, r.get_bounding_client_rect().height()];
    let labels: Vec<_> = panel
        .items
        .iter()
        .chain(&v.geometry.figure_items)
        .filter_map(|item| match item {
            Item::Text(text) if text.role != Role::Key => Some(text.bounds),
            _ => None,
        })
        .collect();
    let narrow = v.width < 480.0;
    // The figure's text, measured part by part: a style may lay the head out as its contents.
    let mut fixed = figure_rects(v, ".rv-chart__title, .rv-chart__actions");
    // A key drawn in the chart, such as a gauge's bands or a colour scale, counts as a legend no
    // readout stands in for: each swatch and label is kept clear like the title (P1b). A gauge's
    // key repeats the band its readout names, so it gives way after the subtitle and before the
    // gauge's own needle and track.
    let keys = panel
        .items
        .iter()
        .chain(&v.geometry.figure_items)
        .filter(|item| item.role() == Role::Key)
        .filter_map(|item| item.bounds());
    let mut gauge_key = Vec::new();
    if v.geometry.kind == "gauge" {
        gauge_key.extend(keys);
    } else {
        fixed.extend(keys);
    }
    let subtitle = figure_rects(v, ".rv-chart__subtitle");
    let note = figure_rects(v, ".rv-chart__method");
    let mut legend = figure_rects(v, ".rv-chart__legend");
    if !stands_in {
        fixed.append(&mut legend);
    } else if narrow {
        // Either dock may cover the legend the readout stands in for.
        legend.clear();
    }
    // A gauge's bands, needle and hub all belong to its one observation.
    let gauge = v.geometry.kind == "gauge";
    let mut others = Vec::new();
    let mut other_lines = Vec::new();
    for item in panel
        .items
        .iter()
        .filter(|item| item.role() == Role::Mark && !gauge)
    {
        match item {
            Item::Line { path, obs, .. } if *obs != Some(t.observation) => {
                other_lines.extend(path.flatten(0.5));
            }
            Item::Rect { obs, .. } | Item::Point { obs, .. } | Item::Area { obs, .. }
                if *obs != Some(t.observation) =>
            {
                others.extend(item.bounds());
            }
            _ => {}
        }
    }
    let dots: Vec<_> = o.dots.iter().map(|dot| dot.at).collect();
    // Pareto targets a full column and a waffle category spans separated squares; the tagged
    // parts are the visible marks that a readout must leave clear.
    let selected_items: Vec<_> = if matches!(v.geometry.kind.as_str(), "pareto" | "waffle") {
        panel
            .items
            .iter()
            .filter(|item| match item {
                Item::Rect { obs, role, .. } | Item::Point { obs, role, .. } => {
                    *role == Role::Mark && *obs == Some(t.observation)
                }
                _ => false,
            })
            .collect()
    } else {
        Vec::new()
    };
    let pareto_bar = if v.geometry.kind == "pareto" {
        selected_items.iter().find_map(|item| match item {
            Item::Rect { rect, .. } => Some(*rect),
            _ => None,
        })
    } else {
        None
    };
    let selected_parts: Vec<_> = selected_items
        .iter()
        .filter_map(|item| item.bounds())
        .collect();
    let curved_paths: Vec<_> = if matches!(v.geometry.kind.as_str(), "alluvial" | "gauge") {
        panel
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Area {
                    path, obs, role, ..
                } if *role == Role::Mark
                    && (v.geometry.kind == "gauge" || *obs == Some(t.observation)) =>
                {
                    Some(path.flatten(0.05))
                }
                _ => None,
            })
            .flatten()
            .collect()
    } else {
        Vec::new()
    };
    // The gauge needle has a filled path and outline, and its hub is a separate point. The value
    // and its name in the middle read the same observation, so the readout keeps them clear as it
    // keeps its mark (P1b).
    let gauge_parts: Vec<_> = if v.geometry.kind == "gauge" {
        panel
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Point {
                    role: Role::Mark, ..
                } => item.bounds(),
                Item::Text(text) if text.role == Role::Label => Some(text.bounds),
                _ => None,
            })
            .collect()
    } else {
        Vec::new()
    };
    let lines: Vec<_> = panel
        .items
        .iter()
        .filter_map(|item| {
            if nearest_x
                && let Item::Line {
                    path,
                    role: Role::Mark,
                    ..
                } = item
            {
                Some(path.flatten(0.5))
            } else {
                None
            }
        })
        .flatten()
        .collect();
    // The gauge's empty middle, centred on its hub, out to the inner edge of its track.
    let hollow = gauge
        .then(|| {
            let center = panel
                .items
                .iter()
                .find_map(|item| match item {
                    Item::Point {
                        center,
                        role: Role::Mark,
                        ..
                    } => Some(*center),
                    _ => None,
                })
                .or_else(|| o.area.map(|a| [a.x + a.w / 2.0, a.y + a.w / 2.0]))?;
            let radius = panel
                .items
                .iter()
                .filter_map(|item| match item {
                    Item::Area {
                        path,
                        role: Role::Band,
                        ..
                    } => Some(path.flatten(0.5)),
                    _ => None,
                })
                .flatten()
                .flatten()
                .map(|p| (p[0] - center[0]).hypot(p[1] - center[1]))
                .fold(f64::INFINITY, f64::min);
            radius.is_finite().then_some(readout_place::Hollow {
                center,
                radius,
                needle: o.at,
            })
        })
        .flatten();
    use readout_place::{Keep, MarkShape, Side};
    let mark = if nearest_x {
        MarkShape::NearestX {
            x: o.x,
            dots: &dots,
            lines: &lines,
        }
    } else if let Some(wedge) = o.wedge {
        MarkShape::Wedge(wedge)
    } else if !curved_paths.is_empty() {
        MarkShape::Polygon {
            anchor: o.area.unwrap_or_default(),
            paths: &curved_paths,
            parts: &gauge_parts,
            edge_slack: if gauge { 0.8 } else { 0.05 },
            hollow,
        }
    } else if o.area.is_none() {
        MarkShape::Point {
            center: o.at,
            radius: o.radius,
        }
    } else {
        let area = pareto_bar.unwrap_or_else(|| o.area.unwrap_or_default());
        let value_end = matches!(
            v.geometry.kind.as_str(),
            "bar"
                | "combo"
                | "pareto"
                | "waterfall"
                | "histogram"
                | "lollipop"
                | "dumbbell"
                | "bullet"
                | "gantt"
                | "interval"
                | "funnel"
                | "bar3d"
        );
        let end = value_end.then(|| {
            let candidates = [
                ((o.at[1] - area.y).abs(), Side::Up),
                ((o.at[1] - area.bottom()).abs(), Side::Down),
                ((o.at[0] - area.right()).abs(), Side::Right),
                ((o.at[0] - area.x).abs(), Side::Left),
            ];
            candidates
                .into_iter()
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map_or(Side::Up, |(_, side)| side)
        });
        if !selected_parts.is_empty() {
            MarkShape::Composite {
                anchor: area,
                parts: &selected_parts,
                end,
            }
        } else {
            MarkShape::Rect { rect: area, end }
        }
    };
    let chart = [v.width, v.geometry.height];
    let visible = readout_visible_bounds(v);
    let previous = if appearing { None } else { v.side };
    let keep = Keep {
        fixed: &fixed,
        key: &gauge_key,
        subtitle: &subtitle,
        legend: &legend,
        note: &note,
        text: &labels,
        others: &others,
        lines: &other_lines,
    };
    let (position, side) = readout_place::place(
        mark, size, chart, panel.plot, visible, &keep, narrow, previous,
    );
    let [x, y] = [position.x, position.y];
    if narrow {
        let _ = r.set_attribute("data-rv-placement", side.narrow_placement());
    } else {
        let _ = r.remove_attribute("data-rv-placement");
    }
    if side.is_fallback() {
        let _ = r.set_attribute("data-rv-placement-fallback", "");
    } else {
        let _ = r.remove_attribute("data-rv-placement-fallback");
    }
    v.side = Some(side);
    if appearing {
        let _ = r.style().set_property("transition", "none");
    }
    let _ = r
        .style()
        .set_property("transform", &format!("translate({x}px, {y}px)"));
    if appearing {
        // Apply the position before the transition comes back.
        let _ = r.offset_width();
        let _ = r.style().remove_property("transition");
    }
    if announce && let Some(text) = hit::announce(&v.geometry, t) {
        v.live.set_text_content(Some(&text));
    }
}

/// The pointer in the plot's own coordinates. A turned chart (FEA-CH-68) is turned a quarter
/// clockwise: its x axis runs down the screen and its y axis to the left.
fn local(v: &View, e: &web_sys::MouseEvent) -> [f64; 2] {
    let rect = v.canvas.get_bounding_client_rect();
    let [x, y] = client_point(e);
    if turned(v) {
        [y - rect.top(), rect.right() - x]
    } else {
        [x - rect.left(), y - rect.top()]
    }
}

/// Whether this is the expanded view on a phone held upright, laid out in landscape and turned
/// a quarter by the figure styles (FEA-CH-68).
pub(super) fn turned(v: &View) -> bool {
    v.turned
}

/// How far the pointer lies along the plot's x axis from the start of `rect`, and that axis's
/// length in `rect`, in screen pixels; a turned chart's x axis runs down the screen.
pub(super) fn along(v: &View, e: &web_sys::MouseEvent, rect: &web_sys::DomRect) -> (f64, f64) {
    let [x, y] = client_point(e);
    if turned(v) {
        (y - rect.top(), rect.height())
    } else {
        (x - rect.left(), rect.width())
    }
}

fn set_target(view: &Rc<RefCell<View>>, target: Option<Target>, announce: bool) {
    let (old, trail, focus) = {
        let mut v = view.borrow_mut();
        // Explicit inspection takes ownership even when it selects the already-hovered mark.
        // A previous pointer-leave timeout must not clear the new keyboard selection.
        if announce && let Some(timer) = v.leaving.take() {
            window().clear_timeout_with_handle(timer);
        }
        if v.target == target && !announce {
            return;
        }
        let old = v.target;
        // Dots on a line follow the pointer; every other mark lets go where it was left.
        let nearest_x = |t: Target| {
            v.geometry
                .panels
                .get(t.panel)
                .is_some_and(|p| p.targeting == Targeting::NearestX)
        };
        let trail = old != target
            && old.is_some_and(|o| {
                !nearest_x(o) || target.is_none_or(|t| t.panel != o.panel || !nearest_x(t))
            });
        v.target = target;
        show_readout(&mut v, announce);
        (old, trail, focus::of(&v, target))
    };
    if old != target || view.borrow().letting_go.is_some() {
        feedback::hovered(view, old, trail);
    }
    // The rest of the chart steps back from a hovered or selected mark (FEA-CH-52); a legend
    // entry's own emphasis stays until its pointer leaves it.
    if focus.is_some() || view.borrow().mark_focus {
        view.borrow_mut().mark_focus = focus.is_some();
        feedback::emphasise(view, focus);
    }
    draw(view);
}

/// A legend entry or series name emphasises its series, or none; a hidden series emphasises
/// nothing, so the shown ones keep full ink (FEA-CH-52).
fn emphasise_series(view: &Rc<RefCell<View>>, series: Option<usize>) {
    let series = {
        let mut v = view.borrow_mut();
        v.mark_focus = false;
        series.filter(|&s| v.geometry.series.get(s).is_some_and(|s| !s.hidden))
    };
    feedback::emphasise(view, series.map(|s| (Focus::Series(s), mesh::DIM_FLOOR)));
}

/// The pointer returned during the readout's hold: the feedback and the dimming come back.
fn resume_feedback(view: &Rc<RefCell<View>>) {
    feedback::resume(view);
    let focus = {
        let v = view.borrow();
        focus::of(&v, v.target)
    };
    if focus.is_some() {
        view.borrow_mut().mark_focus = true;
        feedback::emphasise(view, focus);
    }
}

/// Whether the chart zooms (FEA-CH-49), by the engine's rule that core reserves Reset zoom's
/// place by.
fn zoomable(v: &View) -> bool {
    richview_chart::interaction::zooms(v.chart.block.kind.name(), &v.geometry)
}

/// Keep Reset zoom's place in the action row of a chart a mouse can zoom, while the button is
/// hidden: a zoom then shows it where room was kept, so the title beside the icons does not
/// wrap anew and the plot does not move under the pointer (Chart design, 16 November). A chart
/// that cannot zoom, and a touch screen, where zoom is in the expanded view, keep no place.
fn keep_reset_place(v: &View) {
    let Some(reset) = &v.reset else { return };
    let fine = window()
        .match_media("(any-pointer: fine)")
        .ok()
        .flatten()
        .is_some_and(|m| m.matches());
    let style = reset.style();
    if reset.hidden() && zoomable(v) && fine {
        let _ = style.set_property("display", "inline-flex");
        let _ = style.set_property("visibility", "hidden");
    } else {
        let _ = style.remove_property("display");
        let _ = style.remove_property("visibility");
    }
}

/// The y axis of a chart that zooms along both axes (a map); `None` where zoom is along x only.
fn y_span(v: &View) -> Option<richview_chart::geometry::YSpan> {
    v.geometry.panels.first()?.x.as_ref()?.y
}

/// Pixel y to data y on a chart that zooms along both axes.
fn data_y(v: &View, py: f64) -> Option<f64> {
    let s = y_span(v)?;
    let t = (py - s.range[0]) / (s.range[1] - s.range[0]);
    Some(s.domain[0] + t * (s.domain[1] - s.domain[0]))
}

/// What the plot shows now along x, for a chart that zooms along both axes: the window a pan
/// or a pinch starts from. The layout fits a window whole, so the plot may show more than the
/// reader asked for.
fn shown_x(v: &View) -> Option<[f64; 2]> {
    y_span(v)?;
    Some(v.geometry.panels.first()?.x.as_ref()?.domain)
}

/// Move the window along y by `py` pixels of the plot from `start`, staying on the chart; the
/// zoom set next draws it.
fn pan_y_to(v: &mut View, start: [f64; 2], py: f64) {
    let Some(s) = y_span(v) else { return };
    let height = start[1] - start[0];
    let per_px = height / (s.range[1] - s.range[0]);
    let [lo, hi] = s.extent;
    let a = (start[0] - py * per_px).clamp(lo.min(hi - height), (hi - height).max(lo));
    v.chart.zoom_y = Some([a, a + height]);
}

/// Pixel x to data x through the first panel's scale.
fn data_x(v: &View, px: f64) -> Option<f64> {
    let s = v.geometry.panels.first()?.x.as_ref()?;
    let t = (px - s.range[0]) / (s.range[1] - s.range[0]);
    Some(s.domain[0] + t * (s.domain[1] - s.domain[0]))
}

fn set_zoom(view: &Rc<RefCell<View>>, zoom: Option<[f64; 2]>) {
    {
        let mut v = view.borrow_mut();
        // A zoom set now wins over a glide still on its way (FEA-CH-52).
        v.zoom_glide = None;
        let scale = v
            .geometry
            .panels
            .first()
            .and_then(|p| p.x.as_ref())
            .cloned();
        let zoom = zoom.and_then(|window| v.chart.clamp_zoom(window, scale.as_ref()?));
        if zoom != v.zoom {
            feedback::names_moved(&mut v, false);
        }
        v.zoom = zoom;
        let zoomed = v.zoom.is_some();
        if !zoomed {
            v.chart.zoom_y = None;
        }
        if v.hint_on {
            zoom_hint(&mut v, true);
        }
        if let Some(reset) = &v.reset {
            // Focus on the button that goes away moves to the chart it reset.
            let button: &Element = reset.as_ref();
            let had_focus = document().active_element().as_ref() == Some(button);
            reset.set_hidden(!zoomed);
            keep_reset_place(&v);
            if had_focus && !zoomed {
                let _ = v.frame.focus();
            }
        }
    }
    layout_inner(view, false, true);
    // A chart that zooms along both axes fits the window whole, so its plot may show more
    // than was asked for: what it shows is the window from here on, and a pan or a zoom along
    // one axis starts from it.
    {
        let mut v = view.borrow_mut();
        if v.zoom.is_some()
            && let Some(s) = v.geometry.panels.first().and_then(|p| p.x.clone())
            && let Some(y) = s.y
        {
            let inside = |d: [f64; 2], e: [f64; 2]| [d[0].max(e[0]), d[1].min(e[1])];
            v.zoom = Some(inside(s.domain, s.extent));
            v.chart.zoom_y = Some(inside(y.domain, y.extent));
        }
    }
    feedback::names_return(view);
}

/// Zoom by `factor` around the pixel `at`, or around the middle of the plot; a chart that
/// zooms along both axes zooms both (FEA-CH-49).
fn zoom_by(view: &Rc<RefCell<View>>, factor: f64, at: Option<[f64; 2]>) {
    let (domain, y) = {
        let v = view.borrow();
        let Some(s) = v.geometry.panels.first().and_then(|p| p.x.clone()) else {
            return;
        };
        // Over names a step moves the window's ends by whole names, one at least, so a step
        // is never lost to the snap to names (Chart design, 16 November).
        if let Some(least) = s.least {
            let (first, last) = (s.extent[0].ceil(), s.extent[1].floor());
            let [a, b] = [s.domain[0].max(first), s.domain[1].min(last)];
            let width = b - a;
            let to = if factor < 1.0 {
                (width * factor).floor().min(width - 1.0).max(least)
            } else {
                (width * factor).ceil().max(width + 1.0)
            };
            // At two names a step in has nowhere to go.
            if to >= width && factor < 1.0 {
                return;
            }
            let centre = at
                .and_then(|p| data_x(&v, p[0]))
                .unwrap_or((a + b) / 2.0)
                .clamp(a, b);
            // A step that cannot stay centred keeps the earlier name.
            let start = (centre - (centre - a) * to / width - 0.5).ceil();
            drop(v);
            set_zoom(view, Some([start, start + to]));
            return;
        }
        let around = |centre: Option<f64>, domain: [f64; 2]| {
            let centre = centre.unwrap_or((domain[0] + domain[1]) / 2.0);
            [
                centre - (centre - domain[0]) * factor,
                centre + (domain[1] - centre) * factor,
            ]
        };
        (
            around(at.and_then(|p| data_x(&v, p[0])), s.domain),
            s.y.map(|y| around(at.and_then(|p| data_y(&v, p[1])), y.domain)),
        )
    };
    if y.is_some() {
        view.borrow_mut().chart.zoom_y = y;
    }
    set_zoom(view, Some(domain));
}

/// Pan a zoomed window by a scroll of `px` pixels. Over names a scroll steps by whole names: it
/// gathers until a quarter of a name, then moves one name or more, so a mouse notch moves the
/// window and a trackpad's small steps add up (Chart design, 16 November).
fn scroll_pan(view: &Rc<RefCell<View>>, window: [f64; 2], px: f64) {
    let per_px = view
        .borrow()
        .geometry
        .panels
        .first()
        .and_then(|p| p.x.as_ref())
        .filter(|s| s.least.is_some())
        .map(|s| (s.domain[1] - s.domain[0]) / (s.range[1] - s.range[0]));
    let Some(per_px) = per_px else {
        pan_to(view, window, px);
        return;
    };
    let rest = {
        let mut v = view.borrow_mut();
        v.scroll_rest -= px * per_px;
        v.scroll_rest
    };
    if rest.abs() < 0.25 {
        return;
    }
    let names = rest.signum() * rest.abs().round().max(1.0);
    view.borrow_mut().scroll_rest = 0.0;
    set_zoom(view, Some([window[0] + names, window[1] + names]));
}

/// Move a zoomed window by `px` pixels of the plot, keeping its width and staying inside the
/// data (FEA-CH-49).
fn pan_to(view: &Rc<RefCell<View>>, start: [f64; 2], px: f64) {
    let window = {
        let v = view.borrow();
        let Some(s) = v.geometry.panels.first().and_then(|p| p.x.clone()) else {
            return;
        };
        let per_px = (start[1] - start[0]) / (s.range[1] - s.range[0]);
        let width = start[1] - start[0];
        let [lo, hi] = s.extent;
        let a = (start[0] - px * per_px).clamp(lo, (hi - width).max(lo));
        [a, a + width]
    };
    set_zoom(view, Some(window));
}

/// A finger lands (`Some`) or lifts (`None`) on the expanded chart; any change restarts the
/// gesture from the fingers now down (FEA-CH-49).
fn finger(view: &Rc<RefCell<View>>, id: i32, at: Option<[f64; 2]>) {
    let mut v = view.borrow_mut();
    if v.owner.is_none() {
        return;
    }
    v.fingers.retain(|(f, _)| *f != id);
    if let Some(p) = at {
        v.fingers.push((id, p));
    }
    v.gesture = None;
    v.gesture_y = None;
}

/// Pinch zooms and a one-finger drag pans, only in the expanded view, where the page does not
/// scroll under the chart (FEA-CH-49). Whether this move was such a gesture.
fn touch_gesture(view: &Rc<RefCell<View>>, e: &web_sys::PointerEvent, p: [f64; 2]) -> bool {
    let (fingers, gesture, window, zoomable) = {
        let mut v = view.borrow_mut();
        if v.owner.is_none() || !zoomable(&v) {
            return false;
        }
        let id = e.pointer_id();
        if let Some(f) = v.fingers.iter_mut().find(|(f, _)| *f == id) {
            f.1 = p;
        }
        let domain = v
            .geometry
            .panels
            .first()
            .and_then(|q| q.x.as_ref())
            .map(|s| s.domain);
        // A chart that zooms along both axes starts from what its plot shows.
        let window = shown_x(&v).or(v.zoom.or(domain));
        (v.fingers.clone(), v.gesture, window, true)
    };
    let Some(window) = window.filter(|_| zoomable) else {
        return false;
    };
    let both = y_span(&view.borrow());
    match fingers.as_slice() {
        [(_, a), (_, b)] => {
            // Along both axes the fingers' distance counts, along x their distance along x.
            let spread = match both {
                Some(_) => (a[0] - b[0]).hypot(a[1] - b[1]).max(24.0),
                None => (a[0] - b[0]).abs().max(24.0),
            };
            let mid = (a[0] + b[0]) / 2.0;
            let Some((spread0, from)) = gesture else {
                let mut v = view.borrow_mut();
                v.gesture = Some((spread, window));
                v.gesture_y = both.map(|y| (0.0, y.domain));
                return true;
            };
            let factor = spread0 / spread;
            let centre = {
                let v = view.borrow();
                let Some(s) = v.geometry.panels.first().and_then(|q| q.x.clone()) else {
                    return true;
                };
                let t = (mid - s.range[0]) / (s.range[1] - s.range[0]);
                from[0] + t * (from[1] - from[0])
            };
            let from_y = view.borrow().gesture_y;
            if let (Some(y), Some((_, from))) = (both, from_y) {
                let t = ((a[1] + b[1]) / 2.0 - y.range[0]) / (y.range[1] - y.range[0]);
                let centre = from[0] + t * (from[1] - from[0]);
                view.borrow_mut().chart.zoom_y = Some([
                    centre - (centre - from[0]) * factor,
                    centre + (from[1] - centre) * factor,
                ]);
            }
            set_zoom(
                view,
                Some([
                    centre - (centre - from[0]) * factor,
                    centre + (from[1] - centre) * factor,
                ]),
            );
            true
        }
        [(_, a)] if view.borrow().zoom.is_some() => {
            let Some((x0, from)) = gesture else {
                let start = view.borrow().touch.map_or(a[0], |(x, _, _)| x);
                if (a[0] - start).abs() < 8.0 {
                    return false;
                }
                let mut v = view.borrow_mut();
                v.gesture = Some((a[0], window));
                v.gesture_y = both.map(|y| (a[1], y.domain));
                return true;
            };
            let from_y = view.borrow().gesture_y;
            if let Some((y0, from)) = from_y {
                pan_y_to(&mut view.borrow_mut(), from, a[1] - y0);
            }
            pan_to(view, from, a[0] - x0);
            true
        }
        _ => false,
    }
}

/// Hold the readout briefly while the pointer leaves its mark or scroll box.
fn leave_feedback(view: &Rc<RefCell<View>>) {
    if view.borrow().pinned {
        return;
    }
    let weak = Rc::downgrade(view);
    let hold = Closure::once_into_js(move || {
        if let Some(view) = weak.upgrade() {
            view.borrow_mut().leaving = None;
            if !view.borrow().pinned {
                fade_readout(&mut view.borrow_mut());
                set_target(&view, None, false);
            }
            check_settled();
        }
    });
    if let Ok(timer) =
        window().set_timeout_with_callback_and_timeout_and_arguments_0(hold.unchecked_ref(), 250)
        && let Some(old) = view.borrow_mut().leaving.replace(timer)
    {
        window().clear_timeout_with_handle(old);
    }
    feedback::let_go(view);
    if view.borrow().mark_focus {
        view.borrow_mut().mark_focus = false;
        feedback::emphasise(view, None);
    }
}

fn bind(view: &Rc<RefCell<View>>) {
    let canvas: web_sys::EventTarget = view.borrow().canvas.clone().into();
    let frame: web_sys::EventTarget = view.borrow().frame.clone().into();
    let w = Rc::downgrade(view);
    on(&canvas, "pointermove", move |e| {
        let Some(view) = w.upgrade() else { return };
        let e: web_sys::PointerEvent = e.unchecked_into();
        let p = local(&view.borrow(), &e);
        let leaving = view.borrow_mut().leaving.take();
        if let Some(timer) = leaving {
            window().clear_timeout_with_handle(timer);
            // Resume needs its own borrow of the view.
            resume_feedback(&view);
        }
        if e.pointer_type() == "touch" && touch_gesture(&view, &e, p) {
            return;
        }
        if e.pointer_type() == "touch" {
            // A finger held on the plot scrubs; a quick small touch is a tap (FEA-CH-41).
            let scrub = view.borrow().touch.is_some_and(|(x, y, t)| {
                now() - t > TAP_MS && (p[0] - x).hypot(p[1] - y) < TAP_SLOP
            });
            if !scrub {
                return;
            }
        }
        let pan = view.borrow().pan;
        if let Some((x0, start)) = pan {
            let pan_y = view.borrow().pan_y;
            if let Some((y0, start)) = pan_y {
                pan_y_to(&mut view.borrow_mut(), start, p[1] - y0);
            }
            pan_to(&view, start, p[0] - x0);
            return;
        }
        let drag = view.borrow().drag;
        if let Some((x0, _)) = drag {
            let down = {
                let mut v = view.borrow_mut();
                v.drag = Some((x0, p[0]));
                v.drag_y = v.drag_y.map(|(y0, _)| (y0, p[1]));
                v.drag_y.map_or(0.0, |(y0, y1)| (y1 - y0).abs())
            };
            // Once the press really drags, the crosshair, dots and readout let go, so the band
            // alone shows what the zoom will take (FEA-CH-49).
            let dragging = (p[0] - x0).abs().max(down) > DRAG_SLOP;
            let targeted = view.borrow().target.is_some();
            if dragging && targeted {
                view.borrow_mut().pinned = false;
                set_target(&view, None, false);
            } else {
                draw(&view);
            }
            return;
        }
        if view.borrow().pinned {
            return;
        }
        // The page scrolling under a resting pointer sends a move to the same place (WebKit);
        // a keyboard selection stays until the pointer really moves.
        if view.borrow().keyed && POINTER_AT.with(Cell::get) == Some(client_point(&e)) {
            return;
        }
        view.borrow_mut().keyed = false;
        view.borrow_mut().pointer = Some(p);
        let t = if e.pointer_type() == "touch" {
            hit::at_touch(&view.borrow().geometry, p)
        } else {
            hit::at(&view.borrow().geometry, p)
        };
        set_target(&view, t, false);
        // Within one observation of stacked layers the pointer still moves between layers.
        if t.is_some() && view.borrow().chart.block.kind.name() == "streamgraph" {
            let focus = focus::of(&view.borrow(), t);
            view.borrow_mut().mark_focus = focus.is_some();
            feedback::emphasise(&view, focus);
        }
    });
    let w = Rc::downgrade(view);
    on(&canvas, "pointerenter", move |e| {
        let Some(view) = w.upgrade() else { return };
        let e: web_sys::PointerEvent = e.unchecked_into();
        if e.pointer_type() != "touch" {
            zoom_hint(&mut view.borrow_mut(), true);
        }
    });
    let w = Rc::downgrade(view);
    on(&canvas, "pointerleave", move |e| {
        let Some(view) = w.upgrade() else { return };
        let e: web_sys::PointerEvent = e.unchecked_into();
        if e.pointer_type() != "touch" {
            zoom_hint(&mut view.borrow_mut(), false);
            leave_feedback(&view);
        }
    });
    let readout: web_sys::EventTarget = view.borrow().readout.clone().into();
    let w = Rc::downgrade(view);
    on(&readout, "pointerenter", move |e| {
        let Some(view) = w.upgrade() else { return };
        let e: web_sys::PointerEvent = e.unchecked_into();
        if e.pointer_type() != "touch" {
            let leaving = view.borrow_mut().leaving.take();
            if let Some(timer) = leaving {
                window().clear_timeout_with_handle(timer);
                resume_feedback(&view);
            }
        }
    });
    let w = Rc::downgrade(view);
    on(&readout, "pointerleave", move |e| {
        let Some(view) = w.upgrade() else { return };
        let e: web_sys::PointerEvent = e.unchecked_into();
        if e.pointer_type() != "touch" {
            leave_feedback(&view);
        }
    });
    let w = Rc::downgrade(view);
    on(&canvas, "pointerdown", move |e| {
        let Some(view) = w.upgrade() else { return };
        let e: web_sys::PointerEvent = e.unchecked_into();
        view.borrow_mut().tap = None;
        let p = local(&view.borrow(), &e);
        if e.pointer_type() == "touch" {
            view.borrow_mut().touch = Some((p[0], p[1], now()));
            finger(&view, e.pointer_id(), Some(p));
            return;
        }
        if e.button() == 0 && zoomable(&view.borrow()) {
            // Shift-drag pans a zoomed chart; a plain drag zooms to the dragged window.
            // A chart that zooms along both axes (a map) pans from what its plot shows and
            // drags a rectangle.
            let (zoom, shown, y) = {
                let v = view.borrow();
                (v.zoom, shown_x(&v), y_span(&v))
            };
            let mut v = view.borrow_mut();
            match zoom {
                Some(window) if e.shift_key() => {
                    v.pan = Some((p[0], shown.unwrap_or(window)));
                    v.pan_y = y.map(|y| (p[1], y.domain));
                }
                _ => {
                    v.drag = Some((p[0], p[0]));
                    v.drag_y = y.map(|_| (p[1], p[1]));
                }
            }
            drop(v);
            zoom_hint(&mut view.borrow_mut(), false);
            // The canvas keeps the pointer until release, also outside the chart.
            if let Some(canvas) = e
                .current_target()
                .and_then(|t| t.dyn_into::<Element>().ok())
            {
                let _ = canvas.set_pointer_capture(e.pointer_id());
            }
        }
    });
    let w = Rc::downgrade(view);
    on(&canvas, "pointerup", move |e| {
        let Some(view) = w.upgrade() else { return };
        let e: web_sys::PointerEvent = e.unchecked_into();
        let p = local(&view.borrow(), &e);
        if e.pointer_type() == "touch" {
            finger(&view, e.pointer_id(), None);
            let tap = view.borrow_mut().touch.take().is_some_and(|(x, y, t)| {
                now() - t < TAP_MS && (p[0] - x).hypot(p[1] - y) < TAP_SLOP
            });
            view.borrow_mut().tap = tap.then_some(p);
            return;
        }
        view.borrow_mut().pan_y = None;
        if view.borrow_mut().pan.take().is_some() {
            zoom_hint(&mut view.borrow_mut(), true);
            return;
        }
        let drag = view.borrow_mut().drag.take();
        let drag_y = view.borrow_mut().drag_y.take();
        if let Some((x0, x1)) = drag
            && (x1 - x0)
                .abs()
                .max(drag_y.map_or(0.0, |(y0, y1)| (y1 - y0).abs()))
                > DRAG_SLOP
        {
            // A rectangle dragged almost straight still has a pixel of the other axis: the
            // layout fits the window whole, so its long side decides the zoom.
            let (a, b, y) = {
                let v = view.borrow();
                let wide = if drag_y.is_some() { 1.0 } else { 0.0 };
                let y = drag_y.and_then(|(y0, y1)| {
                    Some([
                        data_y(&v, y0.max(y1).max(y0.min(y1) + 1.0))?,
                        data_y(&v, y0.min(y1))?,
                    ])
                });
                (
                    data_x(&v, x0.min(x1)),
                    data_x(&v, x0.max(x1).max(x0.min(x1) + wide)),
                    y,
                )
            };
            if let (Some(a), Some(b)) = (a, b) {
                feedback::zoom_to_window(&view, Some([a, b]), y);
                zoom_hint(&mut view.borrow_mut(), true);
                return;
            }
        }
        if drag.is_some() {
            zoom_hint(&mut view.borrow_mut(), true);
        }
        draw(&view);
    });
    // Chromium's touch adjustment prefers click responders to nearby pointer-only surfaces.
    // Activate the validated tap here, so a canvas tap is not redirected to an end-name button.
    let w = Rc::downgrade(view);
    on(&canvas, "click", move |_| {
        let Some(view) = w.upgrade() else { return };
        let p = view.borrow_mut().tap.take();
        let Some(p) = p else { return };
        let t = hit::at_touch(&view.borrow().geometry, p);
        // A hover has not pinned the observation: its first tap must keep the readout.
        let same = {
            let v = view.borrow();
            v.pinned && v.target == t && t.is_some()
        };
        view.borrow_mut().pinned = t.is_some() && !same;
        set_target(&view, if same { None } else { t }, true);
    });
    let w = Rc::downgrade(view);
    on(&canvas, "pointercancel", move |e| {
        let Some(view) = w.upgrade() else { return };
        let e: web_sys::PointerEvent = e.unchecked_into();
        finger(&view, e.pointer_id(), None);
        view.borrow_mut().touch = None;
        view.borrow_mut().tap = None;
    });
    // A drag or pan whose pointer is taken away ends without zooming.
    for end in ["pointercancel", "lostpointercapture"] {
        let w = Rc::downgrade(view);
        on(&canvas, end, move |_| {
            let Some(view) = w.upgrade() else { return };
            let had = {
                let mut v = view.borrow_mut();
                v.drag_y = None;
                v.pan_y = None;
                v.drag.take().is_some() | v.pan.take().is_some()
            };
            if had {
                draw(&view);
            }
        });
    }
    let w = Rc::downgrade(view);
    on(&canvas, "dblclick", move |_| {
        if let Some(view) = w.upgrade() {
            feedback::zoom_to(&view, None);
        }
    });
    let w = Rc::downgrade(view);
    on(&canvas, "wheel", move |e| {
        let Some(view) = w.upgrade() else { return };
        let e: web_sys::WheelEvent = e.unchecked_into();
        if !zoomable(&view.borrow()) {
            return;
        }
        // Scrolling along the x axis pans a zoomed chart; the other direction stays the page's.
        // A turned chart's x axis runs down the screen (FEA-CH-68).
        if !(e.ctrl_key() || e.meta_key()) {
            let zoom = view.borrow().zoom;
            let (along, across) = if turned(&view.borrow()) {
                (e.delta_y(), e.delta_x())
            } else {
                (e.delta_x(), e.delta_y())
            };
            if let Some(window) = zoom
                && along.abs() > across.abs()
            {
                e.prevent_default();
                scroll_pan(&view, window, -along);
            }
            return;
        }
        e.prevent_default();
        let p = local(&view.borrow(), &e);
        zoom_by(&view, if e.delta_y() > 0.0 { 1.25 } else { 0.8 }, Some(p));
    });
    let w = Rc::downgrade(view);
    on(&frame, "keydown", move |e| {
        let Some(view) = w.upgrade() else { return };
        let e: web_sys::KeyboardEvent = e.unchecked_into();
        // Keys on the legend and actions belong to those buttons.
        let frame: Element = view.borrow().frame.clone().unchecked_into();
        if e.target()
            .and_then(|t| t.dyn_into::<Element>().ok())
            .as_ref()
            != Some(&frame)
        {
            return;
        }
        // Browser shortcuts (history, zoom) keep their modifiers.
        if e.ctrl_key() || e.meta_key() || e.alt_key() {
            return;
        }
        let key = e.key();
        let current = view.borrow().target;
        let step = |s: Step, n: usize| {
            let mut t = current;
            for _ in 0..n {
                t = hit::step(&view.borrow().geometry, t, s);
            }
            t
        };
        let stream_layer = view.borrow().chart.block.kind.name() == "streamgraph"
            && view.borrow().keyed
            && current.is_some()
            && matches!(key.as_str(), "ArrowUp" | "ArrowDown");
        let next = match key.as_str() {
            "ArrowUp" | "ArrowDown" if stream_layer => Some(current),
            "ArrowRight" | "ArrowDown" => {
                Some(step(Step::Next, if e.shift_key() { 10 } else { 1 }))
            }
            "ArrowLeft" | "ArrowUp" => Some(step(
                if current.is_none() {
                    Step::Next
                } else {
                    Step::Previous
                },
                if e.shift_key() { 10 } else { 1 },
            )),
            "Home" => Some(step(Step::First, 1)),
            // Page keys scroll a long readout and keep the selection (FEA-CH-56); otherwise the
            // page scrolls as usual.
            "PageDown" | "PageUp" => {
                let r = view.borrow().readout.clone();
                let room = r.scroll_height() - r.client_height();
                if r.hidden() || room <= 0 {
                    return;
                }
                let page = r.client_height() * 17 / 20;
                let delta = if key == "PageDown" { page } else { -page };
                r.set_scroll_top((r.scroll_top() + delta).clamp(0, room));
                None
            }
            "End" => Some(step(Step::Last, 1)),
            "+" | "=" | "-" | "_" if !zoomable(&view.borrow()) => return,
            "+" | "=" => {
                zoom_by(&view, 0.8, None);
                None
            }
            "-" | "_" => {
                zoom_by(&view, 1.25, None);
                None
            }
            "Escape" => {
                // One thing at a time: selection, then zoom, then the expanded view.
                if current.is_some() {
                    view.borrow_mut().pinned = false;
                    Some(None)
                } else if view.borrow().zoom.is_some() {
                    set_zoom(&view, None);
                    None
                } else {
                    let owner = view.borrow().owner.clone();
                    if let Some(owner) = owner.and_then(|o| o.upgrade()) {
                        close_expanded(&owner);
                    }
                    None
                }
            }
            _ => return,
        };
        e.prevent_default();
        if let Some(t) = next {
            let mut v = view.borrow_mut();
            let previous = v.keyed.then_some(v.pointer).flatten();
            let direction = if stream_layer {
                if key == "ArrowUp" { -1 } else { 1 }
            } else {
                0
            };
            v.pointer = if v.chart.block.kind.name() == "streamgraph" {
                t.and_then(|target| focus::keyboard_layer(&v.geometry, target, previous, direction))
            } else {
                None
            };
            v.keyed = t.is_some();
            drop(v);
            set_target(&view, t, true);
        }
    });
    let w = Rc::downgrade(view);
    on(&frame, "blur", move |_| {
        if let Some(view) = w.upgrade() {
            view.borrow_mut().keyed = false;
            if !view.borrow().pinned {
                set_target(&view, None, false);
            }
        }
    });
}

/// The legend as buttons in HTML, above or below the canvas as the style places it (FEA-CH-33).
fn build_legend(view: &Rc<RefCell<View>>, focused: &Option<String>) {
    let (legend, series, frame, cues) = {
        let v = view.borrow();
        // Each series' hatch, dash or marker, where it carries one (FEA-CH-57).
        let cues: Vec<Option<String>> = v.geometry.legend.as_ref().map_or_else(Vec::new, |l| {
            (0..v.geometry.series.len())
                .map(|i| richview_chart::vector::legend_swatch(&v.geometry, i, l.swatch))
                .collect()
        });
        (
            v.geometry.legend.clone(),
            v.geometry.series.clone(),
            v.frame.clone(),
            cues,
        )
    };
    if let Some(old) = view.borrow_mut().legend.take() {
        old.remove();
    }
    // Core's static legends, one per reference width when they differ.
    let figure = view.borrow().figure.clone();
    let statics: Vec<Element> = figure
        .query_selector_all(".rv-chart__legend[data-rv-static]")
        .ok()
        .map(|list| {
            (0..list.length())
                .filter_map(|i| list.item(i).and_then(|n| n.dyn_into::<Element>().ok()))
                .collect()
        })
        .unwrap_or_default();
    let Some(legend) = legend else {
        // This width needs no legend: the static ones go too.
        for s in statics {
            s.remove();
        }
        return;
    };
    let list = el("div", "rv-chart__legend");
    let _ = list.set_attribute("role", "group");
    let _ = list.set_attribute("aria-label", "Series");
    for &i in &legend.entries {
        let Some(s) = series.get(i) else { continue };
        let button = el("button", "rv-chart__legend-entry");
        let _ = button.set_attribute("type", "button");
        let _ = button.set_attribute("aria-pressed", if s.hidden { "false" } else { "true" });
        let swatch = el(
            "span",
            &format!(
                "rv-chart__swatch rv-chart__swatch--{}",
                match legend.swatch {
                    richview_chart::geometry::Swatch::Square => "square",
                    richview_chart::geometry::Swatch::Line => "line",
                    richview_chart::geometry::Swatch::Dot => "dot",
                }
            ),
        );
        let _ = swatch.style().set_property("--rv-swatch", &s.color.hex());
        if let Some(Some(svg)) = cues.get(i) {
            swatch.set_inner_html(svg);
            let _ = swatch.class_list().add_1("rv-chart__swatch--cue");
        }
        let label = el("span", "rv-chart__legend-name");
        label.set_text_content(Some(&s.name));
        let _ = button.append_child(&swatch);
        let _ = button.append_child(&label);
        let key = s.key.clone();
        let _ = button.set_attribute("data-rv-series", &key);
        let w = Rc::downgrade(view);
        let target: web_sys::EventTarget = button.clone().into();
        on(&target, "click", move |_| {
            if let Some(view) = w.upgrade() {
                toggle_series(&view, &key);
            }
        });
        let w = Rc::downgrade(view);
        on(&target, "pointerenter", move |_| {
            if let Some(view) = w.upgrade() {
                emphasise_series(&view, Some(i));
            }
        });
        let w = Rc::downgrade(view);
        on(&target, "pointerleave", move |_| {
            if let Some(view) = w.upgrade() {
                emphasise_series(&view, None);
            }
        });
        let _ = list.append_child(&button);
    }
    // The static legend on this legend's side has the same entries and size; swap it in place so
    // nothing moves, and drop the other width's copy.
    let frame_el: &Element = frame.as_ref();
    let side = |s: &Element| {
        let after =
            frame_el.compare_document_position(s) & web_sys::Node::DOCUMENT_POSITION_FOLLOWING != 0;
        after == legend.bottom
    };
    let shown = |s: &Element| s.get_client_rects().length() > 0;
    let keep = statics
        .iter()
        .position(|s| side(s) && shown(s))
        .or_else(|| statics.iter().position(side));
    for (i, s) in statics.iter().enumerate() {
        if Some(i) != keep {
            s.remove();
        }
    }
    match keep.map(|i| &statics[i]) {
        Some(static_legend) if static_legend.parent_element().as_ref() == Some(&figure) => {
            let _ = static_legend.replace_with_with_node_1(&list);
        }
        _ if legend.bottom => {
            let _ = frame.after_with_node_1(&list);
        }
        _ => {
            let _ = frame.before_with_node_1(&list);
        }
    }
    // Beside the plot of an expanded chart in a short, wide window, the legend leads the column
    // there (S27-875).
    if let Some(column) = aside_column(&figure) {
        let _ = column.prepend_with_node_1(&list);
    }
    refocus(&list, focused);
    view.borrow_mut().legend = Some(list);
}

/// The series key of the legend entry or line-end name that has focus, if any.
fn focused_series() -> Option<String> {
    document()
        .active_element()
        .and_then(|e| e.get_attribute("data-rv-series"))
}

/// Give focus back to the rebuilt button for `key`, so toggling never drops it.
fn refocus(root: &Element, key: &Option<String>) {
    let Some(key) = key else { return };
    let Ok(list) = root.query_selector_all("[data-rv-series]") else {
        return;
    };
    for i in 0..list.length() {
        if let Some(b) = list.item(i).and_then(|n| n.dyn_into::<HtmlElement>().ok())
            && b.get_attribute("data-rv-series").as_deref() == Some(key.as_str())
        {
            let _ = b.focus();
            return;
        }
    }
}

/// Show or hide a series from its legend entry or its name (FEA-CH-27, FEA-CH-33). The last
/// visible series cannot be hidden; the value axis refits.
fn toggle_series(view: &Rc<RefCell<View>>, key: &str) {
    {
        let mut v = view.borrow_mut();
        if let Some(at) = v.hidden.iter().position(|h| h == key) {
            v.hidden.remove(at);
        } else {
            let visible = v
                .geometry
                .series
                .iter()
                .filter(|s| !v.hidden.contains(&s.key))
                .count();
            if visible <= 1 {
                return;
            }
            v.hidden.push(key.to_owned());
        }
    }
    layout(view, true);
}

/// Line-end names as buttons over the canvas: each shows or hides its series and reads as the
/// series' name, pressed while shown (FEA-CH-27). Charts with a legend have none.
fn build_names(view: &Rc<RefCell<View>>, focused: &Option<String>) {
    let (frame, names) = {
        let v = view.borrow();
        let names: Vec<(usize, String, String, bool, Rect)> = if v.geometry.legend.is_some() {
            Vec::new()
        } else {
            v.geometry
                .panels
                .iter()
                .flat_map(|p| p.items.iter())
                // A series' name stands beside the plot or inside it beside its own line
                // (FEA-CH-27), set in the data face. A value label is set in figures, also when
                // its number spells a series' name ("18" on a series named 18).
                .filter_map(|i| match i {
                    Item::Text(t)
                        if t.role == Role::Label
                            && t.face == Face::Data
                            && matches!(v.geometry.kind.as_str(), "line" | "area") =>
                    {
                        let i = t.series?;
                        let s = v.geometry.series.get(i)?;
                        (s.name == t.text)
                            .then(|| (i, s.key.clone(), s.name.clone(), s.hidden, t.bounds))
                    }
                    _ => None,
                })
                .collect()
        };
        (v.frame.clone(), names)
    };
    let old = frame.query_selector(".rv-chart__names").ok().flatten();
    // The same names in the same states only move, as during a zoom glide: the buttons stay,
    // so a pointer resting on one keeps its emphasis.
    if let Some(old) = &old
        && names.len() >= 2
    {
        let buttons = old.children();
        let same = buttons.length() as usize == names.len()
            && names.iter().enumerate().all(|(n, (_, key, _, hidden, _))| {
                buttons.item(n as u32).is_some_and(|b| {
                    b.get_attribute("data-rv-series").as_deref() == Some(key.as_str())
                        && b.get_attribute("aria-pressed").as_deref()
                            == Some(if *hidden { "false" } else { "true" })
                })
            });
        if same {
            for (n, (.., r)) in names.iter().enumerate() {
                if let Some(b) = buttons
                    .item(n as u32)
                    .and_then(|b| b.dyn_into::<HtmlElement>().ok())
                {
                    place_name(&b, *r);
                }
            }
            return;
        }
    }
    if let Some(old) = old {
        old.remove();
    }
    if names.len() < 2 {
        return;
    }
    let group = el("div", "rv-chart__names");
    let _ = group.set_attribute("role", "group");
    let _ = group.set_attribute("aria-label", "Series");
    for (i, key, name, hidden, r) in names {
        let b = el("button", "rv-chart__name");
        let _ = b.set_attribute("type", "button");
        let _ = b.set_attribute("data-rv-series", &key);
        let _ = b.set_attribute("aria-pressed", if hidden { "false" } else { "true" });
        b.set_text_content(Some(&name));
        place_name(&b, r);
        let w = Rc::downgrade(view);
        on(&b.clone().into(), "click", move |_| {
            if let Some(view) = w.upgrade() {
                toggle_series(&view, &key);
            }
        });
        // Pointing at a name emphasises its series, as its legend entry would (FEA-CH-52).
        let w = Rc::downgrade(view);
        on(&b.clone().into(), "pointerenter", move |_| {
            if let Some(view) = w.upgrade() {
                emphasise_series(&view, Some(i));
            }
        });
        let w = Rc::downgrade(view);
        on(&b.clone().into(), "pointerleave", move |_| {
            if let Some(view) = w.upgrade() {
                emphasise_series(&view, None);
            }
        });
        let _ = group.append_child(&b);
    }
    let _ = frame.append_child(&group);
    refocus(&group, focused);
}

/// Lay a name button over the canvas text it stands for, with a small margin.
fn place_name(b: &HtmlElement, r: Rect) {
    // A target is at least 24 by 24 CSS pixels (WCAG 2.5.8), centred on the name it covers; the
    // padding is hit area only, and the stylesheet keeps the tint and ring on the name itself.
    const MIN_TARGET: f64 = 24.0;
    let (name_w, name_h) = (r.w + 4.0, r.h + 4.0);
    let w = name_w.max(MIN_TARGET);
    let h = name_h.max(MIN_TARGET);
    let style = b.style();
    let _ = style.set_property("left", &format!("{}px", r.x + r.w / 2.0 - w / 2.0));
    let _ = style.set_property("top", &format!("{}px", r.y + r.h / 2.0 - h / 2.0));
    let _ = style.set_property("width", &format!("{w}px"));
    let _ = style.set_property("height", &format!("{h}px"));
    let _ = style.set_property("--rv-px", &format!("{}px", (w - name_w) / 2.0));
    let _ = style.set_property("--rv-py", &format!("{}px", (h - name_h) / 2.0));
}

/// A labelled button: chart actions from the shared markup with their icon (FEA-CH-43), other
/// controls as plain text buttons.
fn action_button(label: &str, class: &str) -> HtmlElement {
    let Some(kind) = class.strip_prefix("rv-chart__action--") else {
        let b = el("button", &format!("rv-chart__action {class}"));
        let _ = b.set_attribute("type", "button");
        b.set_text_content(Some(label));
        return b;
    };
    let holder = el("div", "");
    holder.set_inner_html(&richview_chart::interaction::action_html(kind, ""));
    let b: HtmlElement = holder
        .first_element_child()
        .expect("button")
        .unchecked_into();
    if let Ok(Some(text)) = b.query_selector("span") {
        text.set_text_content(Some(label));
    }
    let _ = b.set_attribute("aria-label", label);
    b
}

fn download(name: &str, blob: &web_sys::Blob) {
    let Ok(url) = web_sys::Url::create_object_url_with_blob(blob) else {
        return;
    };
    let a: web_sys::HtmlAnchorElement = el("a", "").unchecked_into();
    a.set_href(&url);
    a.set_download(name);
    let _ = document().body().map(|b| b.append_child(&a));
    a.click();
    a.remove();
    let _ = web_sys::Url::revoke_object_url(&url);
}

/// The action row: Table, Download PNG, CSV and Expand, plus Reset zoom while zoomed (FEA-CH-43).
fn build_actions(view: &Rc<RefCell<View>>) -> Result<(), JsValue> {
    let figure = view.borrow().figure.clone();
    // Core reserves the group, buttons included, at the right of the title row; adopt it so
    // nothing moves (FEA-CH-64). A figure without one gets a fresh group.
    let actions: HtmlElement =
        match figure.query_selector(".rv-chart__titlerow > .rv-chart__actions")? {
            Some(reserved) => reserved.unchecked_into(),
            None => {
                let actions = view.borrow().actions.clone();
                actions.set_attribute("role", "group")?;
                actions.set_attribute("aria-label", "Chart actions")?;
                if let Some(row) = figure.query_selector(".rv-chart__titlerow")? {
                    row.append_child(&actions)?;
                } else {
                    figure.prepend_with_node_1(&actions)?;
                }
                actions
            }
        };
    let button = |kind: &str, label: &str| -> Result<HtmlElement, JsValue> {
        if let Some(b) = actions.query_selector(&format!(".rv-chart__action--{kind}"))? {
            return Ok(b.unchecked_into());
        }
        let b = action_button(label, &format!("rv-chart__action--{kind}"));
        actions.append_child(&b)?;
        Ok(b)
    };
    // The expanded view is already expanded: its group has Table, PNG, CSV and Reset zoom, and
    // the dialog's Close (FEA-CH-44).
    let expanded = figure.class_list().contains("rv-chart--expanded");
    let [table, png, csv, expand] = {
        let [(t, tl), (p, pl), (c, cl), (e, el_)] = richview_chart::interaction::ACTIONS;
        [
            button(t, tl)?,
            button(p, pl)?,
            button(c, cl)?,
            button(e, el_)?,
        ]
    };
    // The actions are icons alone: each one's label is its accessible name (FEA-CH-43).
    for (b, (_, label)) in [&table, &png, &csv, &expand]
        .into_iter()
        .zip(richview_chart::interaction::ACTIONS)
    {
        b.set_attribute("aria-label", label)?;
    }
    table.set_attribute("aria-pressed", "false")?;
    // Core reserved Reset zoom's place on a chart that zooms (FEA-CH-43); the runtime takes
    // that button, or adds one where none was reserved.
    let reset: HtmlElement = match actions.query_selector(".rv-chart__action--reset")? {
        Some(b) => b.unchecked_into(),
        None => {
            let b = action_button("Reset zoom", "rv-chart__action--reset");
            actions.append_child(&b)?;
            b
        }
    };
    reset.set_attribute("aria-label", "Reset zoom")?;
    reset.remove_attribute("data-rv-place")?;
    reset.set_hidden(true);
    actions.remove_attribute("data-rv-pending")?;
    actions.remove_attribute("inert")?;
    view.borrow_mut().actions = actions.clone();
    view.borrow_mut().reset = Some(reset.clone());
    keep_reset_place(&view.borrow());
    let w = Rc::downgrade(view);
    let t: web_sys::EventTarget = table.clone().into();
    on(&t, "click", move |_| {
        if let Some(view) = w.upgrade() {
            toggle_table(&view, &table);
        }
    });
    let w = Rc::downgrade(view);
    let t: web_sys::EventTarget = csv.into();
    on(&t, "click", move |_| {
        let Some(view) = w.upgrade() else { return };
        let v = view.borrow();
        let text = data::csv(&v.chart);
        let parts = js_sys::Array::of1(&JsValue::from_str(&text));
        let options = web_sys::BlobPropertyBag::new();
        options.set_type("text/csv;charset=utf-8");
        if let Ok(blob) = web_sys::Blob::new_with_str_sequence_and_options(&parts, &options) {
            download(&format!("{}.csv", v.chart.payload.file), &blob);
        }
    });
    let w = Rc::downgrade(view);
    let t: web_sys::EventTarget = png.into();
    on(&t, "click", move |_| {
        if let Some(view) = w.upgrade() {
            png_download(&view);
        }
    });
    if expanded {
        expand.remove();
    } else {
        let w = Rc::downgrade(view);
        let t: web_sys::EventTarget = expand.into();
        on(&t, "click", move |e| {
            if let Some(view) = w.upgrade() {
                // A pointer's click counts at least one; a key's, Enter or Space, none (S27-878).
                let pointer = e
                    .dyn_ref::<web_sys::UiEvent>()
                    .is_some_and(|u| u.detail() > 0);
                open_expanded(&view, pointer);
            }
        });
    }
    let w = Rc::downgrade(view);
    let t: web_sys::EventTarget = reset.into();
    on(&t, "click", move |_| {
        if let Some(view) = w.upgrade() {
            feedback::zoom_to(&view, None);
        }
    });
    Ok(())
}

/// The Table view under the chart (FEA-CH-45): source rows, 1,000 at a time.
fn toggle_table(view: &Rc<RefCell<View>>, button: &HtmlElement) {
    let taken = view.borrow_mut().table.take();
    if let Some(table) = taken {
        table.remove();
        let _ = button.set_attribute("aria-pressed", "false");
        reveal(view);
        return;
    }
    view.borrow_mut().table_all = false;
    let wrap = build_table(view);
    // Directly under the chart, after a legend set below it.
    let anchor: Element = {
        let v = view.borrow();
        match (
            &v.legend,
            v.geometry.legend.as_ref().is_some_and(|l| l.bottom),
        ) {
            (Some(legend), true) => legend.clone().unchecked_into(),
            _ => v.frame.clone().unchecked_into(),
        }
    };
    let _ = anchor.after_with_node_1(&wrap);
    let _ = button.set_attribute("aria-pressed", "true");
    // A Table wider or taller than its box is a named region that shows where it scrolls.
    super::tables::mark_table(&wrap);
    view.borrow_mut().table = Some(wrap);
    reveal(view);
}

/// In an expanded view, the dialog scrolls so that the Table it just opened shows, or on closing
/// it, so that the chart is back in view (FEA-CH-45). The chart fills the dialog, so the Table
/// usually opens below the fold: all of it comes into view when it fits under the sticky title
/// and action rows, else its top comes to just under them. A Table whose status line and first
/// row already show leaves the dialog still. The dialog scrolls along its own axis, so a turned
/// one scrolls along the turn. The inline figure and focus stay as they are, and the actions stay
/// in sight.
fn reveal(view: &Rc<RefCell<View>>) {
    let (dialog, to) = {
        let v = view.borrow();
        if v.owner.is_none() {
            return;
        }
        let Some(dialog) = v.figure.closest("dialog").ok().flatten() else {
            return;
        };
        let to = match &v.table {
            None => 0.0,
            Some(table) => {
                // What covers the dialog's top while it scrolls: the title row and, upright, the
                // action row held under it (a turned dialog, or one in a short, wide window,
                // keeps its actions in the title row).
                let tall = |selector: &str| {
                    dialog
                        .query_selector(selector)
                        .ok()
                        .flatten()
                        .and_then(|h| h.dyn_into::<HtmlElement>().ok())
                        .map_or(0.0, |h| f64::from(h.offset_height()))
                };
                let cover = tall(".rv-chart-dialog__head")
                    + if v.turned || v.short_wide {
                        0.0
                    } else {
                        tall(".rv-chart--expanded > .rv-chart__actions")
                    };
                let top = offset_in(table, &dialog);
                let height = f64::from(table.offset_height());
                let seen = f64::from(dialog.client_height());
                let at = f64::from(dialog.scroll_top());
                let shown = |e: &HtmlElement| {
                    let y = offset_in(e, &dialog);
                    y >= at + cover - 1.0 && y + f64::from(e.offset_height()) <= at + seen + 1.0
                };
                let part = |selector: &str| {
                    table
                        .query_selector(selector)
                        .ok()
                        .flatten()
                        .and_then(|e| e.dyn_into::<HtmlElement>().ok())
                };
                // The Table's own top margin keeps it off the rule above.
                let gap = window()
                    .get_computed_style(table)
                    .ok()
                    .flatten()
                    .and_then(|s| s.get_property_value("margin-top").ok())
                    .and_then(|m| m.trim_end_matches("px").parse::<f64>().ok())
                    .unwrap_or(0.0);
                let highest = top - cover - gap;
                if part(".rv-chart__table-status").is_none_or(|e| shown(&e))
                    && part("tbody tr").is_none_or(|e| shown(&e))
                {
                    // Its status line and first row already show: the dialog stays where it is.
                    at
                } else if height + gap <= seen - cover {
                    at.max(top + height - seen).min(highest)
                } else {
                    highest
                }
            }
        };
        (dialog, to)
    };
    let run = GLIDES.with(|n| {
        n.set(n.get().wrapping_add(1));
        n.get()
    });
    // A new press ends the glide before it.
    view.borrow_mut().scrolling = None;
    let from = f64::from(dialog.scroll_top());
    let most = f64::from(dialog.scroll_height() - dialog.client_height()).max(0.0);
    let to = to.min(most).max(0.0);
    match style_motion() {
        Some((length, curve)) if (to - from).abs() >= 1.0 => {
            view.borrow_mut().scrolling = Some(run);
            let glide = Glide {
                run,
                dialog,
                from,
                to,
                start: now(),
                length,
                curve,
            };
            scroll_step(Rc::downgrade(view), glide, from);
        }
        _ => {
            dialog.set_scroll_top(to.round() as i32);
            check_settled();
        }
    }
}

/// An expanded dialog's scroll from one place to another along the style's curve.
struct Glide {
    run: u32,
    dialog: Element,
    from: f64,
    to: f64,
    start: f64,
    length: f64,
    curve: [f64; 4],
}

/// One frame of a dialog's glide; `last` is where the frame before left it. The glide ends at its
/// length, when a newer one starts, when the dialog closes, or when the reader scrolls it
/// themselves, so it never fights them.
fn scroll_step(weak: Weak<RefCell<View>>, glide: Glide, last: f64) {
    let Some(view) = weak.upgrade() else { return };
    if view.borrow().scrolling != Some(glide.run) {
        return;
    }
    let moved = (f64::from(glide.dialog.scroll_top()) - last).abs() > 1.0;
    let t = ((now() - glide.start) / glide.length).clamp(0.0, 1.0);
    let y = (glide.from + (glide.to - glide.from) * richview_chart::motion::bezier(glide.curve, t))
        .round();
    if !moved && glide.dialog.is_connected() {
        glide.dialog.set_scroll_top(y as i32);
    }
    if moved || t >= 1.0 || !glide.dialog.is_connected() {
        view.borrow_mut().scrolling = None;
        check_settled();
        return;
    }
    let next = Closure::once_into_js(move || scroll_step(weak, glide, y));
    let _ = window().request_animation_frame(next.unchecked_ref());
}

/// Where `element` sits in `container`'s scrolled content, along the container's own axis:
/// layout offsets, which a turned dialog's transform does not change.
fn offset_in(element: &HtmlElement, container: &Element) -> f64 {
    let mut top = 0.0;
    let mut at = Some(element.clone());
    let container: &web_sys::Node = container;
    while let Some(e) = at {
        if e.is_same_node(Some(container)) {
            break;
        }
        top += f64::from(e.offset_top());
        at = e
            .offset_parent()
            .and_then(|p| p.dyn_into::<HtmlElement>().ok());
    }
    top
}

/// The style's motion for a movement the runtime makes, as its length in milliseconds and its
/// curve, or `None` when motion is off: reduced motion, or a style that switches it off.
fn style_motion() -> Option<(f64, [f64; 4])> {
    let still = document()
        .query_selector(".rv-report[data-motion=\"off\"]")
        .ok()
        .flatten()
        .is_some();
    if reduced_motion() || still {
        return None;
    }
    let style = document()
        .document_element()
        .and_then(|r| window().get_computed_style(&r).ok().flatten())?;
    let property = |name: &str| style.get_property_value(name).ok().unwrap_or_default();
    let length: f64 = property("--rv-motion-duration")
        .trim()
        .trim_end_matches("ms")
        .parse()
        .ok()?;
    let numbers: Vec<f64> = property("--rv-motion-easing")
        .trim()
        .trim_start_matches("cubic-bezier(")
        .trim_end_matches(')')
        .split(',')
        .filter_map(|n| n.trim().parse().ok())
        .collect();
    let curve = <[f64; 4]>::try_from(numbers)
        .unwrap_or_else(|_| richview_chart::motion::easing("standard"));
    (length > 0.0).then_some((length, curve))
}

/// Rebuild an open Table after the state changed (FEA-BR-32).
fn refresh_table(view: &Rc<RefCell<View>>) {
    let Some(old) = view.borrow().table.clone() else {
        return;
    };
    let wrap = build_table(view);
    let _ = old.replace_with_with_node_1(&wrap);
    super::tables::mark_table(&wrap);
    view.borrow_mut().table = Some(wrap);
}

/// The Table's box: a status line, the rows and "Show 1,000 more observations". With reader
/// controls it lists the state on screen and names it, with a control to list every row
/// (FEA-BR-32).
fn build_table(view: &Rc<RefCell<View>>) -> HtmlElement {
    let wrap = el("div", "rv-chart__table");
    let (mut t, title, state, all) = {
        let v = view.borrow();
        let kept = (!v.table_all)
            .then(|| v.chart.kept())
            .flatten()
            .map(<[usize]>::to_vec);
        // The expanded chart's title is the dialog's (S27-879): its caption read "Data for".
        let title = v
            .figure
            .query_selector(".rv-chart__title")
            .ok()
            .flatten()
            .or_else(|| {
                let dialog = v.figure.closest("dialog").ok().flatten()?;
                dialog
                    .query_selector(".rv-chart-dialog__title")
                    .ok()
                    .flatten()
            })
            .and_then(|e| e.text_content())
            .unwrap_or_default();
        (
            data::table(&v.chart),
            title.trim().to_owned(),
            kept.map(|k| (k, controls::state_name(&v).unwrap_or_default())),
            v.chart.objects.len(),
        )
    };
    if let Some((kept, _)) = &state {
        t.rows = kept
            .iter()
            .filter_map(|&i| t.rows.get(i).cloned())
            .collect();
    }
    let shown = t.rows.len().min(1000);
    let counts = |n: usize, shown: usize| match &state {
        Some((_, name)) => format!("{name}: {n} of {all} observations; {shown} shown"),
        None => format!("{n} observations; {shown} shown"),
    };
    let status = el("p", "rv-chart__table-status");
    status.set_text_content(Some(&counts(t.rows.len(), shown)));
    let table = el("table", "rv-table");
    let caption = el("caption", "rv-visually-hidden");
    caption.set_text_content(Some(&format!("Data for {title}")));
    let _ = table.append_child(&caption);
    let thead = el("thead", "");
    let tr = el("tr", "");
    for (h, numeric) in t.header.iter().zip(&t.numeric) {
        let th = el("th", if *numeric { "num" } else { "" });
        let _ = th.set_attribute("scope", "col");
        th.set_text_content(Some(h));
        let _ = tr.append_child(&th);
    }
    let _ = thead.append_child(&tr);
    let _ = table.append_child(&thead);
    let tbody = el("tbody", "");
    let append = |tbody: &HtmlElement, rows: &[Vec<String>]| {
        for r in rows {
            let tr = el("tr", "");
            for (c, numeric) in r.iter().zip(&t.numeric) {
                let td = el("td", if *numeric { "num" } else { "" });
                td.set_text_content(Some(c));
                let _ = tr.append_child(&td);
            }
            let _ = tbody.append_child(&tr);
        }
    };
    append(&tbody, &t.rows[..shown]);
    let _ = table.append_child(&tbody);
    let _ = wrap.append_child(&status);
    if state.as_ref().is_some_and(|(kept, _)| kept.len() < all) {
        let every = action_button(&format!("Show all {all} observations"), "rv-chart__all");
        let w = Rc::downgrade(view);
        let target: web_sys::EventTarget = every.clone().into();
        on(&target, "click", move |_| {
            if let Some(view) = w.upgrade() {
                view.borrow_mut().table_all = true;
                refresh_table(&view);
            }
        });
        let _ = wrap.append_child(&every);
    }
    let _ = wrap.append_child(&table);
    if t.rows.len() > shown {
        let more = action_button("Show 1,000 more observations", "rv-chart__more");
        let total = t.rows.len();
        let rows = Rc::new(RefCell::new((t.rows, shown, t.numeric)));
        let body = tbody.clone();
        let status = status.clone();
        let more2 = more.clone();
        let target: web_sys::EventTarget = more.clone().into();
        let name = state.as_ref().map(|(_, n)| n.clone());
        on(&target, "click", move |_| {
            let mut st = rows.borrow_mut();
            let (every, shown, numeric) = &mut *st;
            let next = (*shown + 1000).min(every.len());
            for r in &every[*shown..next] {
                let tr = el("tr", "");
                for (c, n) in r.iter().zip(numeric.iter()) {
                    let td = el("td", if *n { "num" } else { "" });
                    td.set_text_content(Some(c));
                    let _ = tr.append_child(&td);
                }
                let _ = body.append_child(&tr);
            }
            *shown = next;
            status.set_text_content(Some(&match &name {
                Some(name) => format!("{name}: {total} of {all} observations; {next} shown"),
                None => format!("{total} observations; {next} shown"),
            }));
            if next >= every.len() {
                more2.remove();
            }
        });
        let _ = wrap.append_child(&more);
    }
    wrap
}

/// PNG at twice the on-screen size, on the chart surface (FEA-CH-47).
fn png_download(view: &Rc<RefCell<View>>) {
    if renderer_pending() {
        PNG_WAITING.with(|waiting| waiting.borrow_mut().push(Rc::downgrade(view)));
        return;
    }
    let v = view.borrow();
    let scale = 2.0;
    let (w, h) = (
        (v.width * scale).round() as u32,
        (v.geometry.height * scale).round() as u32,
    );
    let state = State::default();
    let mesh = mesh::build(&v.geometry, &state, scale);
    let canvas: HtmlCanvasElement = el("canvas", "").unchecked_into();
    canvas.set_width(w);
    canvas.set_height(h);
    let Ok(Some(ctx)) = canvas.get_context("2d") else {
        return;
    };
    let ctx: CanvasRenderingContext2d = ctx.unchecked_into();
    let ok = with_renderer(|r| {
        let atlas = atlas::build(&document(), &mesh, &v.chart.payload.theme, scale)
            .ok()
            .flatten();
        let up = match &atlas {
            Some(a) => r.upload(&mesh, &a.vertices, a.ranges.clone(), Some(&a.canvas)),
            None => r.upload(&mesh, &[], Vec::new(), None),
        };
        let Ok(up) = up else { return false };
        r.begin(w, h, v.geometry.background.0);
        r.draw(
            &mesh,
            &up,
            [v.width, v.geometry.height],
            scale,
            Clock::settled(),
        );
        r.present(&ctx, w, h);
        r.release(up);
        true
    });
    // The file names the state on screen (FEA-BR-32).
    let name = match controls::state_name(&v) {
        Some(state) => format!(
            "{}-{}.png",
            v.chart.payload.file,
            crate::payload::file_name(&state)
        ),
        None => format!("{}.png", v.chart.payload.file),
    };
    drop(v);
    if ok.is_none() && renderer_pending() {
        PNG_WAITING.with(|waiting| waiting.borrow_mut().push(Rc::downgrade(view)));
        return;
    }
    if ok != Some(true) {
        let _ = window().alert_with_message(
            "The image could not be drawn. The CSV download still has the chart's data.",
        );
        return;
    }
    let callback = Closure::once_into_js(move |blob: JsValue| {
        if let Ok(blob) = blob.dyn_into::<web_sys::Blob>() {
            download(&name, &blob);
        }
    });
    let _ = canvas.to_blob(callback.unchecked_ref());
    // The view needs its own frame back in the shared buffer.
    draw(view);
}

/// One expanded view: the same chart in a native dialog, keeping its state (FEA-CH-44).
/// What the frame's label adds to core's: how the keyboard reaches the data.
const KEYS_HINT: &str = ". Arrow keys step through the data; Escape clears.";

fn open_expanded(view: &Rc<RefCell<View>>, pointer: bool) {
    if view.borrow().dialog.is_some() {
        return;
    }
    // Expanding interrupts a playback; the reader resumes it in the expanded view.
    controls::pause(view);
    let dialog: web_sys::HtmlDialogElement = el("dialog", "rv-chart-dialog").unchecked_into();
    // A pointer's open shows the chart's focus without its ring until the reader presses a key
    // (S27-878): the chart takes focus by script, and WebKit rings that focus even after a click.
    if pointer {
        let _ = dialog.set_attribute("data-rv-pointer", "");
    }
    let header = el("div", "rv-chart-dialog__head");
    let title = el("p", "rv-chart-dialog__title");
    title.set_text_content(
        view.borrow()
            .figure
            .query_selector(".rv-chart__title")
            .ok()
            .flatten()
            .and_then(|t| t.text_content())
            .as_deref(),
    );
    let title_id = format!("{}-dialog-title", view.borrow().figure.id());
    title.set_id(&title_id);
    let _ = dialog.set_attribute("aria-labelledby", &title_id);
    let titles = el("div", "rv-chart-dialog__titles");
    let _ = titles.append_child(&title);
    // The subtitle follows its title into the dialog: it often says what the chart measures, and
    // the expanded figure has no head of its own (Daniel, 16 Nov: expanding lost the subtitle).
    let subtitle = view
        .borrow()
        .figure
        .query_selector(".rv-chart__subtitle")
        .ok()
        .flatten()
        .and_then(|s| s.text_content())
        .filter(|s| !s.trim().is_empty());
    if let Some(text) = subtitle {
        let subtitle = el("p", "rv-chart-dialog__subtitle");
        subtitle.set_text_content(Some(&text));
        let subtitle_id = format!("{}-dialog-subtitle", view.borrow().figure.id());
        subtitle.set_id(&subtitle_id);
        let _ = dialog.set_attribute("aria-describedby", &subtitle_id);
        let _ = titles.append_child(&subtitle);
    }
    // An action like the others, so it shows its icon where actions are icons alone (FEA-CH-43).
    let close = action_button("Close", "rv-chart__action--close");
    let _ = close.class_list().add_1("rv-chart-dialog__close");
    let _ = header.append_child(&titles);
    let _ = header.append_child(&close);
    let figure = el("figure", "rv-chart rv-chart--expanded");
    // The same controls in the same state; a change in either view applies to both (FEA-BR-32).
    let id = format!("{}-expanded", view.borrow().figure.id());
    let row = controls::row_html(view, &id);
    if !row.is_empty() {
        figure.set_inner_html(&row);
    }
    let frame = el("div", "rv-chart__frame");
    // The expanded chart reads as the inline one: core's title and summary (FEA-CH-55).
    let label = view
        .borrow()
        .frame
        .get_attribute("aria-label")
        .map(|l| l.trim_end_matches(KEYS_HINT).to_owned());
    if let Some(label) = label {
        let _ = frame.set_attribute("aria-label", &label);
    }
    let payload = el("script", "");
    let _ = payload.set_attribute("type", "application/json");
    let _ = payload.set_attribute("data-rv-payload", "");
    payload.set_text_content(Some(
        &serde_json::to_string(&view.borrow().chart.payload).unwrap_or_default(),
    ));
    let _ = figure.append_child(&frame);
    let _ = figure.append_child(&payload);
    let _ = dialog.append_child(&header);
    let _ = dialog.append_child(&figure);
    let _ = document().body().map(|b| b.append_child(&dialog));
    let _ = dialog.show_modal();
    let _ = document()
        .body()
        .map(|b| b.class_list().add_1("rv-scroll-locked"));
    // The expanded view belongs to its inline chart, not to the page's list of charts.
    let expanded = mount(&figure).ok().flatten();
    if let Some(e) = &expanded {
        VIEWS.with(|v| v.borrow_mut().retain(|x| !Rc::ptr_eq(x, e)));
        {
            let mut ev = e.borrow_mut();
            let v = view.borrow();
            ev.hidden = v.hidden.clone();
            ev.zoom = v.zoom;
            // A zoomed chart opens zoomed, with Reset zoom in its group (FEA-CH-43).
            if let Some(reset) = &ev.reset {
                reset.set_hidden(ev.zoom.is_none());
            }
            keep_reset_place(&ev);
            ev.settled = true;
            ev.owner = Some(Rc::downgrade(view));
            let state = v.chart.state.clone();
            ev.chart.set_state(state);
        }
        layout(e, true);
        controls::sync(e);
        // Focus lands on the chart without scrolling the dialog past its title and actions, in a
        // task of its own: WebKit scrolls a dialog shown in this task to whatever holds focus
        // when the task ends, whatever focus() asked, so a chart taller than the room opened the
        // dialog scrolled (27 px for a gantt on a phone held sideways) and moved the chart under
        // the resting pointer.
        let f = e.borrow().frame.clone();
        let focus = Closure::once_into_js(move || {
            if f.is_connected() {
                let options = web_sys::FocusOptions::new();
                options.set_prevent_scroll(true);
                let _ = f.focus_with_options(&options);
            }
        });
        let _ = window()
            .set_timeout_with_callback_and_timeout_and_arguments_0(focus.unchecked_ref(), 0);
    }
    {
        let mut v = view.borrow_mut();
        v.expanded = expanded;
        v.dialog = Some(dialog.clone());
    }
    let w = Rc::downgrade(view);
    let t: web_sys::EventTarget = close.into();
    on(&t, "click", move |_| {
        if let Some(view) = w.upgrade() {
            close_expanded(&view);
        }
    });
    let w = Rc::downgrade(view);
    let t: web_sys::EventTarget = dialog.into();
    on(&t, "cancel", move |e| {
        e.prevent_default();
        if let Some(view) = w.upgrade() {
            close_expanded(&view);
        }
    });
    // Any key in the dialog brings the ring back, in every engine.
    on(&t, "keydown", |e| {
        if let Some(d) = e
            .current_target()
            .and_then(|t| t.dyn_into::<Element>().ok())
        {
            let _ = d.remove_attribute("data-rv-pointer");
        }
    });
}

fn close_expanded(view: &Rc<RefCell<View>>) {
    let (dialog, expanded) = {
        let mut v = view.borrow_mut();
        (v.dialog.take(), v.expanded.take())
    };
    if let Some(e) = expanded {
        controls::pause(&e);
        let (hidden, zoom) = {
            let ev = e.borrow();
            (ev.hidden.clone(), ev.zoom)
        };
        {
            let mut v = view.borrow_mut();
            v.hidden = hidden;
            v.zoom = zoom;
            if let Some(reset) = &v.reset {
                reset.set_hidden(zoom.is_none());
            }
            keep_reset_place(&v);
        }
        if let Some(up) = e.borrow_mut().uploaded.take() {
            with_renderer(|r| r.release(up));
        }
        layout(view, true);
    }
    if let Some(d) = dialog {
        d.close();
        d.remove();
    }
    let _ = document()
        .body()
        .map(|b| b.class_list().remove_1("rv-scroll-locked"));
    if let Ok(Some(b)) = view
        .borrow()
        .figure
        .query_selector(".rv-chart__action--expand")
    {
        let _ = b.unchecked_into::<HtmlElement>().focus();
    }
}

#[allow(dead_code)]
fn face_family(theme: &richview_chart::layout::ChartTheme, face: Face) -> String {
    atlas::family(theme, face)
}

#[cfg(test)]
mod hover_cue_tests {
    use super::*;
    use richview_chart::block::ChartBlock;
    use richview_chart::geometry::Cmd;
    use richview_chart::layout::{ChartTheme, Options, layout};
    use serde_json::json;

    #[test]
    fn crosshair_snaps_its_leading_edge_without_changing_css_width() {
        let soft = Rgba([0.2, 0.3, 0.4, 1.0]);
        for plot in [
            Rect::new(10.0, 20.0, 100.0, 80.0),
            Rect::new(10.3, 20.0, 100.2, 80.0),
            Rect::new(10.8, 20.0, 100.1, 80.0),
        ] {
            for dpr in [1.0, 1.5, 2.0, 3.0] {
                for target in [plot.x, plot.x + plot.w * 0.41, plot.right()] {
                    let Some(Item::Line { path, stroke, .. }) =
                        crosshair_line(plot, target, dpr, soft)
                    else {
                        panic!("normal plot has a crosshair line");
                    };
                    assert_eq!(stroke.width, 1.0);
                    assert_eq!(stroke.dash, [3.0, 3.0]);
                    assert!((stroke.color.0[3] - 0.7).abs() < 1e-6);
                    assert_eq!(&stroke.color.0[..3], &soft.0[..3]);
                    let [Cmd::M { p: start }, Cmd::L { p: end }] = path.cmds.as_slice() else {
                        panic!("crosshair spans the plot");
                    };
                    assert_eq!(start[1], plot.y);
                    assert_eq!(end[1], plot.bottom());
                    assert_eq!(start[0], end[0]);
                    let leading = (start[0] - stroke.width / 2.0) * dpr;
                    let trailing = (start[0] + stroke.width / 2.0) * dpr;
                    let scissor_left = (plot.x * dpr).floor();
                    let scissor_right = scissor_left + (plot.w * dpr).ceil();
                    assert!((leading - leading.round()).abs() < 1e-9);
                    assert!(leading + 1e-9 >= plot.x * dpr);
                    assert!(trailing <= plot.right() * dpr + 1e-9);
                    assert!(leading + 1e-9 >= scissor_left);
                    assert!(trailing <= scissor_right + 1e-9);
                    let desired = (target * dpr - dpr / 2.0).round();
                    let first = (plot.x * dpr).ceil();
                    let last = (plot.right() * dpr).min(scissor_right) - dpr;
                    let expected = desired.max(first).min(last.floor());
                    assert!((leading - expected).abs() < 1e-9);
                }
            }
        }
        let plot = Rect::new(10.0, 20.0, 100.0, 80.0);
        assert_eq!(crosshair_x(plot, 110.0, 1.0), Some(109.5));
        assert_eq!(crosshair_x(plot, 10.0, 1.0), Some(10.5));
        assert!((crosshair_x(plot, 110.0, 1.5).unwrap() - (163.0 / 1.5 + 0.5)).abs() < 1e-9);
        for dpr in [1.0, 1.5, 2.0, 3.0] {
            assert!(crosshair_line(Rect::new(10.0, 20.0, 0.5, 80.0), 10.0, dpr, soft).is_none());
            assert!(crosshair_line(Rect::new(10.3, 20.0, 1.0, 80.0), 10.3, dpr, soft).is_none());
        }
    }

    #[test]
    fn crosshair_does_not_change_the_selected_observation_or_feedback() {
        let block = ChartBlock::from_value(&json!({
            "type": "line", "title": "T", "encoding": {"x": "x", "y": "y"}
        }))
        .unwrap();
        let rows = [json!({"x": 0, "y": 10}), json!({"x": 1, "y": 25})];
        for width in [390.0, 1440.0] {
            let geometry = layout(
                &block,
                &rows,
                &ChartTheme::default(),
                &Options {
                    width,
                    ..Options::default()
                },
            );
            let original = geometry.clone();
            let observation = &geometry.panels[0].observations[0];
            let at = observation
                .dots
                .first()
                .map_or(observation.at, |dot| dot.at);
            let ink = Rgba([0.2, 0.3, 0.4, 1.0]);
            let mut dots = Vec::new();
            hover_dot(&mut dots, at, 1.0, ink, ink);
            assert!(
                dots.iter()
                    .all(|item| matches!(item, Item::Point { center, .. } if *center == at))
            );
            assert!(crosshair_line(geometry.panels[0].plot, observation.x, 1.5, ink).is_some());
            assert!(
                crosshair_line(Rect::new(10.0, 20.0, 0.5, 80.0), observation.x, 1.5, ink).is_none()
            );
            let mut dots_after = Vec::new();
            hover_dot(&mut dots_after, at, 1.0, ink, ink);
            assert_eq!(dots, dots_after, "the dot keeps its original coordinates");
            assert_eq!(
                geometry, original,
                "observation, readout and geometry remain unchanged"
            );
        }
    }

    #[test]
    fn nearby_automatic_values_dim_and_restore_without_changing_the_model() {
        let block = ChartBlock::from_value(&json!({
            "type": "line", "title": "T", "encoding": {"x": "x", "y": "y"}
        }))
        .unwrap();
        let rows = [json!({"x": 0, "y": 10}), json!({"x": 1, "y": 25})];
        let mut geometry = layout(
            &block,
            &rows,
            &ChartTheme::default(),
            &Options {
                width: 640.0,
                ..Options::default()
            },
        );
        let observation = &geometry.panels[0].observations[0];
        let at = observation
            .dots
            .first()
            .map_or(observation.at, |dot| dot.at);
        let mut near = geometry.panels[0]
            .items
            .iter()
            .find_map(|item| match item {
                Item::Text(text)
                    if text.role == Role::Label
                        && text.face == Face::Figures
                        && text.mark.is_some() =>
                {
                    Some(text.clone())
                }
                _ => None,
            })
            .expect("line has an automatic value label");
        near.bounds = Rect::new(at[0] + 24.0, at[1], 20.0, 10.0);
        near.color.0[3] = 0.8;
        let mut far = near.clone();
        far.bounds.x += 0.01;
        let mut name = near.clone();
        name.face = Face::Data;
        let mut axis = near.clone();
        axis.role = Role::AxisLabel;
        let other_at = [at[0] - 100.0, at[1]];
        geometry.panels[0].observations[0]
            .dots
            .push(richview_chart::geometry::Dot {
                at: other_at,
                series: 0,
            });
        let mut other_series = near.clone();
        other_series.bounds = Rect::new(other_at[0], other_at[1], 20.0, 10.0);
        geometry.panels[0]
            .items
            .retain(|item| !matches!(item, Item::Text(_)));
        geometry.panels[0].items.extend(
            [near, far, name, axis, other_series]
                .into_iter()
                .map(Item::Text),
        );
        geometry.figure_items.clear();
        let original = geometry.clone();
        let mut mesh = mesh::build(&geometry, &State::default(), 1.0);
        let target = Some(Target {
            panel: 0,
            observation: 0,
        });
        assert!(hover_label_opacity(&geometry, target, None, &mut mesh));
        let shown = mesh
            .texts
            .iter()
            .map(|q| q.text.color.0[3])
            .collect::<Vec<_>>();
        assert!((shown[0] - 0.16).abs() < 1e-6);
        assert_eq!(&shown[1..4], &[0.8, 0.8, 0.8]);
        assert!((shown[4] - 0.16).abs() < 1e-6);
        assert_eq!(geometry, original, "readouts and geometry stay unchanged");
        assert!(!hover_label_opacity(&geometry, target, None, &mut mesh));
        let readout = Rect::new(at[0] + 30.0, at[1] + 2.0, 5.0, 5.0);
        assert!(hover_label_opacity(
            &geometry,
            target,
            Some(readout),
            &mut mesh
        ));
        let covered = mesh
            .texts
            .iter()
            .map(|q| q.text.color.0[3])
            .collect::<Vec<_>>();
        assert_eq!(&covered[..4], &[0.0, 0.0, 0.8, 0.8]);
        assert!((covered[4] - 0.16).abs() < 1e-6);
        assert!(hover_label_opacity(&geometry, target, None, &mut mesh));
        assert!(hover_label_opacity(&geometry, None, None, &mut mesh));
        assert!(mesh.texts.iter().all(|q| q.text.color.0[3] == 0.8));
        // A mesh may already have dimmed an inside-mark label; drawing must preserve that
        // emphasis while the separate point-label occlusion comes and goes.
        mesh.texts[0].base_alpha = 0.3;
        mesh.texts[0].text.color.0[3] = 0.3;
        assert!(!hover_label_opacity(&geometry, None, None, &mut mesh));
        assert!(hover_label_opacity(&geometry, target, None, &mut mesh));
        assert!((mesh.texts[0].text.color.0[3] - 0.06).abs() < 1e-6);
        assert!(hover_label_opacity(&geometry, None, None, &mut mesh));
        assert_eq!(mesh.texts[0].text.color.0[3], 0.3);
    }

    #[test]
    fn selected_dot_has_six_pixel_fill_and_surface_coloured_two_pixel_halo() {
        let mut items = Vec::new();
        let ink = Rgba([0.1, 0.4, 0.8, 1.0]);
        let dark_surface = Rgba([0.05, 0.06, 0.08, 1.0]);
        hover_dot(&mut items, [20.0, 30.0], 1.0, ink, dark_surface);
        assert_eq!(items.len(), 3);
        // A quarter of the dot's own colour, four pixels past the ring, under both.
        let halo = Rgba([0.1, 0.4, 0.8, 0.25]);
        for (item, expected_radius, expected_color) in [
            (&items[0], 12.0, halo),
            (&items[1], 8.0, dark_surface),
            (&items[2], 6.0, ink),
        ] {
            match item {
                Item::Point {
                    center,
                    radius,
                    fill: Some(fill),
                    stroke,
                    ..
                } => {
                    assert_eq!(*center, [20.0, 30.0]);
                    assert_eq!(*radius, expected_radius);
                    assert_eq!(fill.color, expected_color);
                    assert!(stroke.is_none());
                }
                _ => panic!("expected a filled circle"),
            }
        }
    }
}
