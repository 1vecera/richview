//! A chart on a slide's stage (FEA-RP-65). Anywhere else a chart has a width and takes the height
//! its layout gives. On a stage it has a box: its figure's width and the height the slide leaves
//! it, with lettering that follows the stage. The chart is laid out for that box, flatter or
//! taller panels first and a narrower chart second, and is never a scaled picture. A chart that
//! fits at no ratio and no width (rows of bars) keeps its own layout, and its slide grows.

use super::{LEAST_WIDTH, View, fill_ratio};
use crate::web::window;
use richview_chart::geometry::Geometry;
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlElement};

/// The flattest panels, width to height, that a chart fills its box with: a trend across the
/// whole stage under a row of numbers is about four. Flatter, it narrows instead.
const FLATTEST: f64 = 6.0;

/// The tallest panels a chart fills its box with; a box taller than that leaves room around it.
const TALLEST: f64 = 0.8;

/// The least room a chart is fitted to; a slide that leaves less grows.
const LEAST_ROOM: f64 = 120.0;

/// Marks the ratios and widths found for a slide's box among those an expanded view keeps.
const KEY: f64 = 2.0;

/// The chart laid out for its box.
pub(super) struct Fitted {
    pub width: f64,
    pub geometry: Geometry,
    pub ratio: Option<f64>,
}

/// The slot a slide gave the figure, on a stage or in a stack.
pub(super) fn slot(figure: &Element) -> Option<HtmlElement> {
    figure
        .closest(".rv-slide__figure")
        .ok()
        .flatten()
        .and_then(|slot| slot.dyn_into().ok())
}

fn px(element: &Element, property: &str) -> Option<f64> {
    window()
        .get_computed_style(element)
        .ok()
        .flatten()?
        .get_property_value(property)
        .ok()?
        .trim()
        .strip_suffix("px")?
        .parse()
        .ok()
}

/// What the slot holds besides the plot, in pixels of height: the chart's title, legend, note
/// and source. It does not change with the plot's height. A card stretched to the height its row
/// shares (two figures side by side) is measured at its own height, or the room would be the plot
/// it already has.
fn around(v: &View, slot: &HtmlElement) -> f64 {
    let (Some(first), Some(last)) = (slot.first_element_child(), slot.last_element_child()) else {
        return 0.0;
    };
    let card = v.figure.dyn_ref::<HtmlElement>().map(HtmlElement::style);
    if let Some(card) = &card {
        let _ = card.set_property("flex", "none");
    }
    let extent = last.get_bounding_client_rect().bottom() - first.get_bounding_client_rect().top();
    let plot = v.frame.get_bounding_client_rect().height();
    if let Some(card) = &card {
        let _ = card.remove_property("flex");
    }
    (extent - plot).max(0.0)
}

/// The slot's height as the slide gives it, without the height a chart that did not fit added.
fn given(slot: &HtmlElement) -> f64 {
    let style = slot.style();
    let grown = style.get_property_value("min-height").unwrap_or_default();
    if grown.is_empty() {
        return slot.get_bounding_client_rect().height();
    }
    let _ = style.remove_property("min-height");
    let height = slot.get_bounding_client_rect().height();
    let _ = style.set_property("min-height", &grown);
    height
}

/// The box a slide's stage gives the chart: the width of its figure and the height its slot
/// leaves for the plot. `None` off a stage (a report, a stack on a phone, the expanded view).
pub(super) fn room(v: &View) -> Option<[f64; 2]> {
    if v.owner.is_some() {
        return None;
    }
    let slot = slot(&v.figure)?;
    let fits = window()
        .get_computed_style(&slot)
        .ok()
        .flatten()?
        .get_property_value("--rv-fit")
        .ok()?;
    if fits.trim() != "1" {
        return None;
    }
    let inset = px(&v.figure, "padding-left").unwrap_or(0.0)
        + px(&v.figure, "padding-right").unwrap_or(0.0);
    let full = f64::from(v.figure.client_width()) - inset;
    let room = given(&slot) - around(v, &slot);
    (full > 0.0).then_some([full.max(LEAST_WIDTH), room])
}

/// Whether the box is no longer the one the chart was last laid out for.
pub(super) fn moved(v: &View) -> bool {
    match (room(v), v.boxed) {
        (None, None) => false,
        (Some(now), Some(then)) => (0..2).any(|i| (now[i] - then[i]).abs() >= 0.5),
        _ => true,
    }
}

/// The size of the chart's lettering on a stage, in CSS pixels to the half; `None` anywhere else,
/// where the style's own size stands.
pub(super) fn lettering(figure: &Element) -> Option<f64> {
    slot(figure)?;
    let size = px(figure, "--rv-chart-text")?;
    (size > 0.0).then(|| (size * 2.0).round() / 2.0)
}

/// Lay the chart out for its box: `own` is its layout at the box's width.
pub(super) fn fit(
    v: &mut View,
    full: f64,
    room: f64,
    own: Geometry,
    hidden: &[String],
    zoom: Option<[f64; 2]>,
) -> Fitted {
    let room = room.max(LEAST_ROOM);
    let keep = |geometry| Fitted {
        width: full,
        geometry,
        ratio: None,
    };
    if (own.height - room).abs() <= 0.5 {
        return keep(own);
    }
    let key = [full.round(), room.round(), hidden.len() as f64, KEY];
    // Panels as flat or as tall as the room asks for, at the box's whole width.
    let find = |v: &View| {
        fill_ratio(v, full, room, FLATTEST, hidden, zoom).map(|ratio| ratio.max(TALLEST))
    };
    let at = |v: &View, ratio: Option<f64>| {
        ratio.map(|r| v.chart.geometry_at_ratio(full, Some(r), hidden, zoom))
    };
    let cached = v.fill.filter(|(k, _)| *k == key).map(|(_, ratio)| ratio);
    let mut ratio = cached.unwrap_or_else(|| find(v));
    let mut filled = at(v, ratio);
    if cached.is_some() && filled.as_ref().is_some_and(|g| g.height > room + 0.5) {
        ratio = find(v);
        filled = at(v, ratio);
    }
    v.fill = Some((key, ratio));
    if let Some(geometry) = filled.filter(|g| g.height <= room + 0.5) {
        return Fitted {
            width: full,
            geometry,
            ratio,
        };
    }
    // A height that does not follow the ratio and leaves room stays as it is.
    if own.height <= room + 0.5 {
        return keep(own);
    }
    // The widest width at which the chart's own layout fits, as the expanded view narrows.
    let fits = |v: &View, width: f64| {
        let geometry = v.chart.geometry(width, hidden, zoom);
        (geometry.height <= room + 0.5, geometry)
    };
    if let Some((_, kept)) = v.narrowed.filter(|(k, w)| *k == key && *w < full)
        && let (true, geometry) = fits(v, kept)
    {
        return Fitted {
            width: kept,
            geometry,
            ratio: None,
        };
    }
    if full <= LEAST_WIDTH {
        return keep(own);
    }
    let (least_fits, least) = fits(v, LEAST_WIDTH);
    if !least_fits {
        return keep(own);
    }
    // The chart fits at `lo` and not at `hi`.
    let (mut lo, mut hi, mut found) = (LEAST_WIDTH, full, least);
    while hi - lo > 2.0 {
        let mid = ((lo + hi) / 2.0).floor();
        match fits(v, mid) {
            (true, geometry) => (lo, found) = (mid, geometry),
            (false, _) => hi = mid,
        }
    }
    v.narrowed = Some((key, lo));
    Fitted {
        width: lo,
        geometry: found,
        ratio: None,
    }
}

/// Stand the plot in its box: centred when it narrowed, and with the slot as high as a chart
/// that did not fit needs, so the slide grows rather than cover what follows it.
pub(super) fn place(v: &View, full: f64, room: f64, width: f64, height: f64) {
    if width < full - 0.5 {
        let style = v.frame.style();
        let _ = style.set_property("width", &format!("{width}px"));
        let _ = style.set_property("margin-inline", "auto");
    } else {
        widen(&v.frame);
    }
    let Some(slot) = slot(&v.figure) else {
        return;
    };
    if height > room + 0.5 {
        let needs = (around(v, &slot) + height).ceil();
        let _ = slot
            .style()
            .set_property("min-height", &format!("{needs}px"));
    } else {
        let _ = slot.style().remove_property("min-height");
    }
}

/// Give the plot its figure's width again.
pub(super) fn widen(frame: &HtmlElement) {
    let style = frame.style();
    let _ = style.remove_property("width");
    let _ = style.remove_property("margin-inline");
}

/// Leave the stage: the plot takes its figure's width and the slot its own height again.
pub(super) fn leave(v: &View) {
    widen(&v.frame);
    if let Some(slot) = slot(&v.figure) {
        let _ = slot.style().remove_property("min-height");
    }
}
