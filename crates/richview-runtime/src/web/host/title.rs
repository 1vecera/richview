//! The chart title's first-glyph ink bearing. Core's CSS pulls the title left by it, so the
//! title's ink starts at the figure's left edge with the axis title and legend (FEA-CH-31).
//! Measured once per font and glyph; fonts arriving trigger a new measurement.

use super::super::{document, window};
use std::cell::RefCell;
use std::collections::BTreeMap;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, CssStyleDeclaration, Element, HtmlCanvasElement};

thread_local! {
    static METRICS: RefCell<(Option<CanvasRenderingContext2d>, BTreeMap<String, f64>)> =
        const { RefCell::new((None, BTreeMap::new())) };
}

/// Drop the measurements, for fonts that finished loading under the same family name.
pub(super) fn forget() {
    METRICS.with(|m| m.borrow_mut().1.clear());
}

/// Set `--rv-title-bearing` on `figure` from its title's first glyph.
pub(super) fn apply(figure: &Element) {
    let Ok(Some(title)) = figure.query_selector(".rv-chart__title") else {
        return;
    };
    let Ok(Some(css)) = window().get_computed_style(&title) else {
        return;
    };
    let family = css.get_property_value("font-family").unwrap_or_default();
    let size = css
        .get_property_value("font-size")
        .ok()
        .and_then(|value| value.trim_end_matches("px").parse::<f64>().ok())
        .unwrap_or(19.0);
    let text = title.text_content().unwrap_or_default();
    let left = bearing(&family, &weight(&css, &family), size, text.trim_start()).unwrap_or(0.0);
    if let Some(figure) = figure.dyn_ref::<web_sys::HtmlElement>() {
        let _ = figure
            .style()
            .set_property("--rv-title-bearing", &format!("{left:.3}px"));
    }
}

/// The bundled handwriting face forbids synthesized bold, and a canvas does not inherit
/// `font-synthesis`, so the real face is selected explicitly.
fn weight(css: &CssStyleDeclaration, family: &str) -> String {
    if family.contains("Patrick Hand")
        && css
            .get_property_value("font-synthesis-weight")
            .is_ok_and(|v| v == "none")
    {
        "400".into()
    } else {
        css.get_property_value("font-weight")
            .unwrap_or_else(|_| "400".into())
    }
}

/// Browsers disagree about antialias padding in `actualBoundingBoxLeft`, so the glyph's ink is
/// found on a small transparent canvas at up to 8× resolution; the metric is the fallback.
fn bearing(family: &str, weight: &str, size: f64, text: &str) -> Option<f64> {
    let glyph = text.chars().next()?;
    let font = format!("{weight} {size}px {family}");
    let key = format!("{font}\n{glyph}");
    METRICS.with(|m| {
        let mut m = m.borrow_mut();
        if let Some(left) = m.1.get(&key) {
            return Some(*left);
        }
        if m.0.is_none() {
            let canvas: HtmlCanvasElement =
                document().create_element("canvas").ok()?.unchecked_into();
            m.0 = canvas.get_context("2d").ok()??.dyn_into().ok();
        }
        let context = m.0.clone()?;
        context.set_font(&font);
        let glyph = glyph.to_string();
        let bounds = context.measure_text(&glyph).ok()?;
        let canvas = context.canvas()?;
        let pad = bounds.actual_bounding_box_left() + 4.0;
        let baseline = bounds.actual_bounding_box_ascent() + 4.0;
        let width = bounds.actual_bounding_box_left() + bounds.actual_bounding_box_right() + 8.0;
        let height =
            bounds.actual_bounding_box_ascent() + bounds.actual_bounding_box_descent() + 8.0;
        let raster = (|| {
            if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
                return None;
            }
            let scale = (512.0 / width.max(height)).min(8.0);
            let (w, h) = (
                (width * scale).ceil() as u32,
                (height * scale).ceil() as u32,
            );
            canvas.set_width(w);
            canvas.set_height(h);
            context.set_font(&format!("{weight} {}px {family}", size * scale));
            context.set_text_align("left");
            context.set_text_baseline("alphabetic");
            context
                .fill_text(&glyph, pad * scale, baseline * scale)
                .ok()?;
            let pixels = context
                .get_image_data(0.0, 0.0, w as f64, h as f64)
                .ok()?
                .data();
            let left = pixels
                .0
                .as_chunks::<4>()
                .0
                .iter()
                .enumerate()
                .filter(|(_, rgba)| rgba[3] >= 64)
                .map(|(index, _)| index % w as usize)
                .min()?;
            Some(left as f64 / scale - pad)
        })();
        canvas.set_width(1);
        canvas.set_height(1);
        let left = raster.unwrap_or(-bounds.actual_bounding_box_left());
        if m.1.len() >= 64 {
            m.1.clear();
        }
        m.1.insert(key, left);
        Some(left)
    })
}
