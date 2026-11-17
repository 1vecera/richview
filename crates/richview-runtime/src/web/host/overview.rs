//! The overview strip (FEA-CH-50). After a zoom, line and area charts with a numeric or
//! calendar x show the whole series in a slim strip under the plot, with the current window
//! marked. Dragging the window pans and a press elsewhere on the strip moves the window there;
//! with focus on the window, ←/→ pan, + and − zoom and Home resets. The strip goes on reset;
//! scatter plots and charts nobody zoomed have none.

use super::{View, along, feedback, set_zoom, zoom_by};
use crate::web::{document, el, on};
use richview_chart::block::ChartType;
use richview_chart::geometry::{Item, Rect, Role, XScale};
use std::cell::{Cell, RefCell};
use std::fmt::Write;
use std::rc::{Rc, Weak};
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlElement, PointerEvent};

/// Where a drag on the window started: the pointer's x and the window it held then.
type Grip = (f64, [f64; 2]);

pub(in crate::web) struct Strip {
    root: HtmlElement,
    window: HtmlElement,
    /// The width and hidden series the drawn series belong to.
    key: String,
    /// The whole series' x scale and plot, which the strip spans.
    scale: XScale,
    plot: Rect,
}

/// The whole series as SVG paths in plot coordinates: lines and areas only.
fn series_svg(items: &[Item], plot: Rect) -> String {
    let mut out = format!(
        "<svg viewBox=\"{} {} {} {}\" preserveAspectRatio=\"none\" aria-hidden=\"true\" focusable=\"false\">",
        plot.x, plot.y, plot.w, plot.h
    );
    for item in items.iter().filter(|i| i.role() == Role::Mark) {
        let _ = match item {
            Item::Area { path, fill, .. } => write!(
                out,
                "<path d=\"{}\" fill=\"{}\" fill-opacity=\"{:.2}\"/>",
                richview_chart::vector::d(path),
                fill.color.hex(),
                fill.color.alpha() * 0.6
            ),
            Item::Line { path, stroke, .. } => write!(
                out,
                "<path d=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"1.25\" vector-effect=\"non-scaling-stroke\"/>",
                richview_chart::vector::d(path),
                stroke.color.hex()
            ),
            _ => Ok(()),
        };
    }
    out.push_str("</svg>");
    out
}

/// A data x for the window's spoken range.
fn spoken(x: f64, calendar: bool, span: f64) -> String {
    if calendar {
        let c = richview_chart::time::civil(x.round() as i64);
        return format!("{}-{:02}-{:02}", c.year, c.month, c.day);
    }
    let digits = (2.0 - span.abs().max(1e-9).log10().floor()).clamp(0.0, 6.0) as usize;
    format!("{x:.digits$}")
}

fn eligible(v: &View) -> bool {
    matches!(v.chart.block.kind, ChartType::Line | ChartType::Area)
        && v.geometry.panels.len() == 1
        // Numbers and dates only (FEA-CH-50): a strip over named positions has no range to say.
        && v.geometry.panels[0].x.as_ref().is_some_and(|x| x.least.is_none())
}

/// Show, move or remove the strip to match the chart's zoom.
pub(super) fn sync(view: &Rc<RefCell<View>>) {
    let zoom = {
        let v = view.borrow();
        v.zoom_glide.map(|g| g.window()).or(v.zoom)
    };
    let Some(window) = zoom.filter(|_| eligible(&view.borrow())) else {
        let strip = view.borrow_mut().overview.take();
        if let Some(strip) = strip {
            // Focus in the strip that goes away moves to the chart, never to the page.
            let root: &Element = strip.root.as_ref();
            let had_focus = document()
                .active_element()
                .is_some_and(|a| root.contains(Some(&a)));
            strip.root.remove();
            if had_focus {
                let frame = view.borrow().frame.clone();
                let _ = frame.focus();
            }
        }
        return;
    };
    // The strip draws the whole series of the state on screen.
    let key = {
        let v = view.borrow();
        format!("{}|{:?}|{:?}", v.width, v.hidden, v.chart.state)
    };
    if view.borrow().overview.as_ref().is_none_or(|s| s.key != key) {
        let built = {
            view.borrow_mut().retired = None;
            let v = view.borrow();
            let whole = v.chart.geometry(v.width, &v.hidden, None);
            whole.panels.first().and_then(|p| {
                let scale = p.x.clone()?;
                Some((series_svg(&p.items, p.plot), scale, p.plot))
            })
        };
        let Some((svg, scale, plot)) = built else {
            return;
        };
        let existing = view
            .borrow()
            .overview
            .as_ref()
            .map(|s| (s.root.clone(), s.window.clone()));
        let (root, handle) = existing.unwrap_or_else(|| create(view));
        // The series sit under the plot, aligned with it.
        let _ = root
            .style()
            .set_property("margin-left", &format!("{}px", plot.x));
        let _ = root.style().set_property("width", &format!("{}px", plot.w));
        if let Ok(Some(old)) = root.query_selector("svg") {
            old.remove();
        }
        let _ = root.insert_adjacent_html("afterbegin", &svg);
        view.borrow_mut().overview = Some(Strip {
            root,
            window: handle,
            key,
            scale,
            plot,
        });
    }
    let v = view.borrow();
    let Some(strip) = &v.overview else { return };
    let [lo, hi] = strip.scale.domain;
    let span = (hi - lo).max(f64::MIN_POSITIVE);
    let left = ((window[0] - lo) / span).clamp(0.0, 1.0);
    let right = ((window[1] - lo) / span).clamp(0.0, 1.0);
    let style = strip.window.style();
    let _ = style.set_property("left", &format!("{:.3}%", left * 100.0));
    let _ = style.set_property("width", &format!("{:.3}%", (right - left) * 100.0));
    let _ = strip
        .window
        .set_attribute("aria-valuenow", &format!("{:.0}", left * 100.0));
    let calendar = strip.scale.calendar;
    let _ = strip.window.set_attribute(
        "aria-valuetext",
        &format!(
            "From {} to {}",
            spoken(window[0], calendar, window[1] - window[0]),
            spoken(window[1], calendar, window[1] - window[0])
        ),
    );
}

/// Move the window by `dx` CSS pixels of the strip, keeping its width inside the data.
fn shift(view: &Rc<RefCell<View>>, from: [f64; 2], dx: f64) {
    let window = {
        let v = view.borrow();
        let Some(strip) = &v.overview else { return };
        let [lo, hi] = strip.scale.extent;
        let per_px = (strip.scale.domain[1] - strip.scale.domain[0]) / strip.plot.w.max(1.0);
        let width = from[1] - from[0];
        let a = (from[0] + dx * per_px).clamp(lo, (hi - width).max(lo));
        [a, a + width]
    };
    set_zoom(view, Some(window));
}

fn create(view: &Rc<RefCell<View>>) -> (HtmlElement, HtmlElement) {
    let root = el("div", "rv-chart__overview");
    let handle = el("div", "rv-chart__overview-window");
    handle.set_tab_index(0);
    let _ = handle.set_attribute("role", "slider");
    let _ = handle.set_attribute("aria-label", "Zoomed window");
    let _ = handle.set_attribute("aria-valuemin", "0");
    let _ = handle.set_attribute("aria-valuemax", "100");
    let _ = handle.set_attribute(
        "aria-description",
        "Arrow keys move the window, plus and minus zoom, Home shows everything",
    );
    let _ = root.append_child(&handle);
    let frame = view.borrow().frame.clone();
    let _ = frame.after_with_node_1(&root);
    // Dragging the window pans; a press elsewhere on the strip centres the window there.
    let drag: Rc<Cell<Option<Grip>>> = Rc::new(Cell::new(None));
    let w: Weak<RefCell<View>> = Rc::downgrade(view);
    let d = drag.clone();
    let handle_el: Element = handle.clone().into();
    let strip_el = root.clone();
    on(&root.clone().into(), "pointerdown", move |e| {
        let Some(view) = w.upgrade() else { return };
        let e: PointerEvent = e.unchecked_into();
        let Some(zoom) = view.borrow().zoom else {
            return;
        };
        let target: Option<Element> = e.target().and_then(|t| t.dyn_into().ok());
        if target.as_ref() != Some(&handle_el) {
            let (centre, width) = {
                let v = view.borrow();
                let Some(strip) = &v.overview else { return };
                let (at, length) = along(&v, &e, &strip_el.get_bounding_client_rect());
                let at = at / length.max(1.0);
                let [lo, hi] = strip.scale.domain;
                (lo + at * (hi - lo), zoom[1] - zoom[0])
            };
            let from = [centre - width / 2.0, centre + width / 2.0];
            shift(&view, from, 0.0);
        }
        let now = view.borrow().zoom;
        if let Some(now) = now {
            let start = along(&view.borrow(), &e, &strip_el.get_bounding_client_rect()).0;
            d.set(Some((start, now)));
        }
        if let Some(t) = e.target().and_then(|t| t.dyn_into::<Element>().ok()) {
            let _ = t.set_pointer_capture(e.pointer_id());
        }
        e.prevent_default();
    });
    let w = Rc::downgrade(view);
    let d = drag.clone();
    let strip_el = root.clone();
    on(&root.clone().into(), "pointermove", move |e| {
        let Some(view) = w.upgrade() else { return };
        let e: PointerEvent = e.unchecked_into();
        if let Some((x0, from)) = d.get() {
            let x = along(&view.borrow(), &e, &strip_el.get_bounding_client_rect()).0;
            shift(&view, from, x - x0);
        }
    });
    for end in ["pointerup", "pointercancel"] {
        let d = drag.clone();
        on(&root.clone().into(), end, move |_| d.set(None));
    }
    let w = Rc::downgrade(view);
    on(&handle.clone().into(), "keydown", move |e| {
        let Some(view) = w.upgrade() else { return };
        let e: web_sys::KeyboardEvent = e.unchecked_into();
        if e.ctrl_key() || e.meta_key() || e.alt_key() {
            return;
        }
        let Some(zoom) = view.borrow().zoom else {
            return;
        };
        let step = |share: f64| {
            let v = view.borrow();
            let Some(strip) = &v.overview else { return 0.0 };
            let per_px = (strip.scale.domain[1] - strip.scale.domain[0]) / strip.plot.w.max(1.0);
            (zoom[1] - zoom[0]) * share / per_px
        };
        match e.key().as_str() {
            "ArrowLeft" | "ArrowDown" => {
                let px = step(if e.shift_key() { 0.5 } else { 0.1 });
                shift(&view, zoom, -px);
            }
            "ArrowRight" | "ArrowUp" => {
                let px = step(if e.shift_key() { 0.5 } else { 0.1 });
                shift(&view, zoom, px);
            }
            "+" | "=" => zoom_by(&view, 0.8, None),
            "-" | "_" => zoom_by(&view, 1.25, None),
            "Home" => {
                // The strip goes with the zoom; the chart keeps focus.
                let frame = view.borrow().frame.clone();
                let _ = frame.focus();
                feedback::zoom_to(&view, None);
            }
            _ => return,
        }
        e.prevent_default();
    });
    (root, handle)
}
