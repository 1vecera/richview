//! Gauge charts: one value on a 270° arc that opens at the bottom, from `style.min` to
//! `style.max` (FEA-CH-75). Without bands the arc fills from the minimum to the value; with
//! `style.bands` (a list of `{to, label, color}`, each running from the one before) the arc is
//! split into the bands on the style's ramp and a needle points at the value, and a key under the
//! arc names every band. A value outside the scale is drawn at its edge and counted. A gauge holds
//! one observation per panel; several rows need `encoding.facet`. RichView never grades the value:
//! the bands are the author's (FEA-CH-42, FEA-CH-70).

use super::heatmap::nice_ceil;
use super::kit;
use super::pie::wedge_path;
use super::radar::polar;
use super::types::Facets;
use super::{Ctx, Painted, Style};
use crate::color::{self, Oklch};
use crate::diag::Level;
use crate::geometry::{
    Anchor, DropReason, Face, Fill, Item, Observation, Panel, Path, Readout, ReadoutLine, Rect, Role, Shape, Stroke,
    Targeting, Text,
};
use crate::numbers;
use crate::rows::{self, Num};
use serde_json::{Value, json};
use std::f64::consts::{FRAC_1_SQRT_2, PI};

const RATIO: f64 = 1.2;
/// Widest a gauge grows, whatever the figure's width.
const MAX_WIDTH: f64 = 460.0;
/// The arc starts at 7:30 and ends at 4:30 (clockwise from 12 o'clock).
const START: f64 = -0.75 * PI;
const SWEEP: f64 = 1.5 * PI;
/// Angle left open between two bands.
const BAND_GAP: f64 = 0.012;
const SWATCH: f64 = 10.0;

/// A band as authored: where it ends.
struct RawBand {
    to: f64,
    label: Option<String>,
    slot: Option<usize>,
}

/// A band on the scale.
struct Zone {
    from: f64,
    to: f64,
    label: String,
    color: Oklch,
}

/// What one panel shows.
struct Reading {
    row: usize,
    value: f64,
    name: Option<String>,
}

/// `style.bands` as authored, or the reason it cannot be read.
fn raw_bands(raw: &Value) -> Result<Vec<RawBand>, String> {
    let Some(list) = raw.get("style").and_then(|s| s.get("bands")) else {
        return Ok(Vec::new());
    };
    let Some(items) = list.as_array().filter(|a| !a.is_empty()) else {
        return Err("style.bands must be a list of bands".into());
    };
    let mut out = Vec::new();
    for (k, item) in items.iter().enumerate() {
        let Some(to) = item.get("to").and_then(Value::as_f64).filter(|v| v.is_finite()) else {
            return Err(format!("band {} has no numeric 'to'", k + 1));
        };
        out.push(RawBand {
            to,
            label: item
                .get("label")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
            slot: item
                .get("color")
                .and_then(Value::as_u64)
                .filter(|s| (1..=64).contains(s))
                .map(|s| s as usize),
        });
    }
    Ok(out)
}

/// Text at a size of its own, measured in its face.
fn sized(ctx: &Ctx, t: Text, size: f64, weight: u16) -> Text {
    let bounds = ctx.style.figures.bounds(&t.text, size, t.x, t.y, t.anchor);
    Text {
        size,
        weight,
        bounds,
        ..t
    }
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let y_field = block.encoding.y.clone().unwrap_or_default();
    let x_field = block.encoding.x.clone();
    let title = block.y.label.clone().unwrap_or_else(|| y_field.clone());
    let label_format = block.label_format(&block.y, ctx.locale);
    let readout = block.readout_format(&block.y, ctx.locale);
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut excluded = 0usize;
    let mut reads: Vec<Option<Reading>> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut found: Vec<Reading> = Vec::new();
        for &i in members {
            let row = &ctx.rows[i];
            match rows::num(row, &y_field) {
                Num::Value(value) => found.push(Reading {
                    row: i,
                    value,
                    name: x_field.as_deref().and_then(|f| rows::text(row, f)),
                }),
                Num::Missing => excluded += 1,
                Num::Underflow(t) => {
                    ctx.underflow(i, &y_field, &t);
                    failures[fi].get_or_insert_with(|| {
                        format!("Cannot draw this gauge · {y_field} has a value too small to represent")
                    });
                }
                Num::Invalid(t) => {
                    ctx.check(
                        "non-numeric",
                        Level::Error,
                        format!("Row {} has {y_field} '{t}', which is not a number", i + 1),
                        Some("Write plain numbers; missing values may be empty or NA."),
                        Some(Ctx::row_path(i, &y_field)),
                        None,
                        json!({"row": i}),
                    );
                    failures[fi].get_or_insert_with(|| {
                        format!("Cannot draw this gauge · {y_field} has text that is not a number")
                    });
                }
            }
        }
        if failures[fi].is_none() && found.len() > 1 {
            ctx.check(
                "source-error",
                Level::Error,
                format!(
                    "A gauge shows one value, but this panel has {} rows with a {y_field}",
                    found.len()
                ),
                Some("Give the gauge one row, or use encoding.facet for one gauge per row."),
                Some("chart.encoding.facet".into()),
                None,
                json!({"rows": found.len()}),
            );
            failures[fi] = Some("Cannot draw this gauge · A gauge shows one value; use facets for several".into());
        }
        if failures[fi].is_none() && found.is_empty() {
            failures[fi] = Some("Nothing to draw · No row has a value".into());
        }
        reads.push(if failures[fi].is_none() { found.pop() } else { None });
    }
    if failures.iter().all(Option::is_some) {
        let reason = failures.iter().flatten().next().cloned().unwrap_or_default();
        return kit::failed(ctx, &reason, RATIO);
    }
    if excluded > 0 {
        ctx.check(
            "excluded-observations",
            Level::Warning,
            format!(
                "{} without a value {} left out",
                super::count(excluded, "row", "rows"),
                super::verb(excluded, "is", "are")
            ),
            Some("Give every row a value."),
            Some("chart.encoding.y".into()),
            None,
            json!({"count": excluded}),
        );
        ctx.note.push(format!(
            "{} without a value {} not shown",
            super::count(excluded, "row", "rows"),
            super::verb(excluded, "is", "are")
        ));
    }
    // The scale: authored bounds first, then the end of the last band, then the next round
    // number above the largest value.
    let min = block.style_number("min").or(block.y.min).unwrap_or(0.0);
    let mut bands = match raw_bands(&block.raw) {
        Ok(b) => b,
        Err(why) => {
            ctx.check(
                "style-invalid",
                Level::Warning,
                format!("style.bands is ignored: {why}"),
                Some("Write style.bands as a list such as [{to: 40, label: Low}, {to: 100, label: High}]."),
                Some("chart.style.bands".into()),
                None,
                json!({}),
            );
            Vec::new()
        }
    };
    let mut prev = min;
    if let Some(k) = bands.iter().position(|b| {
        let ok = b.to > prev;
        prev = b.to;
        !ok
    }) {
        ctx.check(
            "style-invalid",
            Level::Warning,
            format!(
                "style.bands is ignored: band {} does not end above the one before it",
                k + 1
            ),
            Some("List the bands from the lowest, each ending above the one before."),
            Some("chart.style.bands".into()),
            None,
            json!({"band": k + 1}),
        );
        bands.clear();
    }
    let largest = reads
        .iter()
        .flatten()
        .map(|r| r.value)
        .fold(f64::NEG_INFINITY, f64::max);
    let max = block
        .style_number("max")
        .or(block.y.max)
        .or_else(|| bands.last().map(|b| b.to))
        .unwrap_or_else(|| nice_ceil((largest - min).max(0.0)) + min);
    let span = max - min;
    if !(span.is_finite() && span > 0.0) {
        let reason = if span.is_finite() {
            "Cannot draw this gauge · style.min must be below style.max"
        } else {
            "Cannot draw this gauge · The scale is too wide to represent"
        };
        ctx.check(
            "style-invalid",
            Level::Error,
            reason,
            Some("Set style.min below style.max, both finite."),
            Some("chart.style.max".into()),
            None,
            json!({"min": min, "max": max}),
        );
        return kit::failed(ctx, reason, RATIO);
    }
    ctx.fit.y = Some([min, max]);
    let names: Vec<String> = vec![title.clone()];
    ctx.set_series(&names);
    // Bands on the style's ramp, light to dark, unless the author picked palette slots.
    let count = bands.len();
    let mut zones: Vec<Zone> = Vec::new();
    let mut from = min;
    for (k, b) in bands.iter().enumerate() {
        let to = b.to.min(max);
        if from >= max {
            break;
        }
        let t = if count > 1 { k as f64 / (count - 1) as f64 } else { 0.5 };
        let color = match b.slot {
            Some(s) => ctx.style.palette[(s - 1) % ctx.style.palette.len()],
            None => color::ramp(&ctx.style.ramp, 0.3 + 0.7 * t),
        };
        let label = b.label.clone().unwrap_or_else(|| {
            format!(
                "{}–{}",
                numbers::value(from, &label_format),
                numbers::value(to, &label_format)
            )
        });
        zones.push(Zone { from, to, label, color });
        from = to;
    }
    let angle = |v: f64| START + (v.clamp(min, max) - min) / span * SWEEP;

    let width = ctx.opts.width;
    let lh = ctx.line_height();
    let size = ctx.size();
    let ascent = ctx.style.font.vertical().0;
    let title_room = if block.encoding.facet.is_some() { lh + 6.0 } else { 0.0 };
    let columns = super::facet_columns(facets.len(), width);
    let pw = (width - 24.0 * (columns as f64 - 1.0)) / columns as f64;
    // The key names every band; entries too wide for a panel are dropped and reported.
    let mut key_rows: Vec<Vec<usize>> = Vec::new();
    let mut row_width = 0.0f64;
    for (k, z) in zones.iter().enumerate() {
        let w = SWATCH + 6.0 + ctx.text_width(&z.label);
        if w > pw - 8.0 {
            ctx.drop_label(&z.label, DropReason::OutOfBounds, "band", None);
            continue;
        }
        let mut gap = if row_width > 0.0 { 16.0 } else { 0.0 };
        if row_width > 0.0 && row_width + gap + w > pw - 8.0 {
            key_rows.push(Vec::new());
            row_width = 0.0;
            gap = 0.0;
        }
        if key_rows.is_empty() {
            key_rows.push(Vec::new());
        }
        row_width += gap + w;
        if let Some(last) = key_rows.last_mut() {
            last.push(k);
        }
    }
    let key_h = if key_rows.is_empty() {
        0.0
    } else {
        key_rows.len() as f64 * (lh + 4.0) + 10.0
    };
    let panel_h = match block.style.ratio {
        Some(r) => pw / r + title_room,
        None => pw.min(MAX_WIDTH) / RATIO + title_room + key_h,
    };
    let frames = super::grid_frames(facets.len(), width, 0.0, panel_h);
    let surface = Style::rgba(ctx.style.surface);
    let ink = Style::rgba(ctx.style.ink);
    let track = Style::rgba(ctx.style.rule_soft);
    let hatch = ctx.series.first().and_then(|s| s.hatch);
    // Boundaries the arc is labelled at: both ends and where a band ends inside the scale.
    let mut marks: Vec<f64> = vec![min, max];
    marks.extend(zones.iter().map(|z| z.to).filter(|t| *t > min && *t < max));
    let widest = marks
        .iter()
        .map(|v| ctx.text_width(&numbers::value(*v, &label_format)))
        .fold(0.0, f64::max);
    let mut panels: Vec<Panel> = Vec::new();
    let mut outside = 0usize;
    for (fi, ((name, _), outer)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *outer, title_room, name, reason));
            continue;
        }
        let Some(read) = &reads[fi] else {
            panels.push(super::failed_facet(
                ctx,
                *outer,
                title_room,
                name,
                "Nothing to draw · No row has a value",
            ));
            continue;
        };
        let body = Rect::new(outer.x, outer.y + title_room, outer.w, (outer.h - title_room).max(1.0));
        let bottom_room = lh * 1.6 + 6.0;
        let by_width = body.w / 2.0 - widest - 10.0;
        // The arc's Bézier controls reach about a tenth of the radius beyond its crown.
        let fit = |top: f64| (body.h - key_h - top - bottom_room) / (1.0 + FRAC_1_SQRT_2);
        let first = by_width.min(fit(lh + 6.0)).max(24.0);
        let top_room = (lh + 6.0).max(first * 0.11);
        let by_height = fit(top_room);
        let radius = by_width.min(by_height).max(24.0);
        let thick = (radius * 0.2).clamp(8.0, 34.0).min(radius * 0.5);
        let inner = radius - thick;
        let mid = (radius + inner) / 2.0;
        let c = [body.x + body.w / 2.0, body.y + top_room + radius];
        let value = read.value;
        let shown = value.clamp(min, max);
        if shown != value {
            outside += 1;
        }
        let a_value = angle(shown);
        let mut items: Vec<Item> = vec![Item::Area {
            path: wedge_path(c, radius, inner, START, START + SWEEP),
            fill: Fill::solid(track),
            role: Role::Band,
            series: None,
            obs: None,
        }];
        if zones.is_empty() {
            if shown > min {
                let color = Style::rgba(ctx.series_color(0));
                items.push(Item::Area {
                    path: wedge_path(c, radius, inner, START, a_value),
                    fill: Fill {
                        color,
                        hatch,
                        hatch_ink: hatch.map(|_| surface),
                        hatch_scale: 1.0,
                    },
                    role: Role::Mark,
                    series: Some(0),
                    obs: None,
                });
            }
        } else {
            let last = zones.len() - 1;
            for (k, z) in zones.iter().enumerate() {
                let (a0, a1) = (angle(z.from), angle(z.to));
                let gap = BAND_GAP.min((a1 - a0) * 0.2);
                let a0 = if k > 0 { a0 + gap / 2.0 } else { a0 };
                let a1 = if k < last { a1 - gap / 2.0 } else { a1 };
                if a1 - a0 <= 1e-6 {
                    continue;
                }
                items.push(Item::Area {
                    path: wedge_path(c, radius, inner, a0, a1),
                    fill: Fill::solid(Style::rgba(z.color)),
                    role: Role::Mark,
                    series: None,
                    obs: None,
                });
            }
            // The needle, with a halo so it reads on any band.
            let half = (thick * 0.18).max(2.0);
            let tip = polar(c, radius - thick * 0.1, a_value);
            let outline = [
                tip,
                polar(c, half, a_value + PI / 2.0),
                polar(c, half, a_value - PI / 2.0),
            ];
            let mut halo = Path::polyline(&outline);
            halo.close();
            let needle = halo.clone();
            items.push(Item::Line {
                path: halo,
                stroke: Stroke::solid(surface, 3.0),
                role: Role::Mark,
                series: None,
                obs: None,
            });
            items.push(Item::Area {
                path: needle,
                fill: Fill::solid(ink),
                role: Role::Mark,
                series: None,
                obs: None,
            });
            items.push(Item::Point {
                center: c,
                radius: half * 1.8,
                shape: Shape::Circle,
                fill: Some(Fill::solid(ink)),
                stroke: Some(Stroke::solid(surface, 1.5)),
                role: Role::Mark,
                series: None,
                obs: None,
            });
        }
        let bottom = c[1] + radius * FRAC_1_SQRT_2;
        let bbox = Rect::new(c[0] - radius, c[1] - radius, 2.0 * radius, bottom - (c[1] - radius));
        // Text boxes already placed, which later labels keep clear of.
        let mut kept: Vec<Rect> = Vec::new();
        // The value in the middle, as large as the opening allows, with the name under it.
        let value_text = numbers::value(value, &readout);
        let centre = c[1] + 0.42 * radius;
        let mut big = (size * 2.2).min(radius * 0.4).max(size);
        let chord = |s: f64| 2.0 * (inner * inner - (0.42 * radius + s * 0.5).powi(2)).max(1.0).sqrt();
        while big > size && ctx.style.figures.width(&value_text, big) > chord(big) * 0.95 {
            big -= 1.0;
        }
        let base = centre + (ascent - ctx.style.font.vertical().1) * big / 2.0;
        let t = ctx.text(
            &value_text,
            c[0],
            base,
            Anchor::Middle,
            Face::Figures,
            Role::Label,
            ctx.style.ink,
        );
        let t = sized(ctx, t, big, 600);
        kept.push(t.bounds);
        let value_bottom = centre + big * 0.5;
        items.push(Item::Text(t));
        if let Some(n) = &read.name {
            let t = ctx.text_mid(
                n,
                c[0],
                value_bottom + lh * 0.8,
                Anchor::Middle,
                Face::Data,
                Role::Label,
                ctx.style.ink_soft,
            );
            let dy = 0.42 * radius + big * 0.5 + lh * 1.3;
            let room = 2.0 * (inner * inner - dy * dy).max(0.0).sqrt();
            if t.bounds.w <= room && t.bounds.inside(&body, 0.5) && !kept.iter().any(|k| k.overlaps(&t.bounds, 1.0)) {
                kept.push(t.bounds);
                items.push(Item::Text(t));
            } else {
                ctx.drop_label(n, DropReason::Collision, "gauge-name", Some(t.bounds));
            }
        }
        // Labels at both ends, under the arc's ends, then at the band boundaries outside it.
        for (v, a) in [(min, START), (max, START + SWEEP)] {
            let label = numbers::value(v, &label_format);
            let t = ctx.text_mid(
                &label,
                polar(c, mid, a)[0],
                bottom + lh * 0.8,
                Anchor::Middle,
                Face::Figures,
                Role::AxisLabel,
                ctx.style.ink,
            );
            if t.bounds.inside(&body, 0.5) && !kept.iter().any(|k| k.overlaps(&t.bounds, 1.0)) {
                kept.push(t.bounds);
                items.push(Item::Text(t));
            } else {
                ctx.drop_label(&label, DropReason::Collision, "tick", Some(t.bounds));
            }
        }
        for &v in marks.iter().skip(2) {
            let a = angle(v);
            let label = numbers::value(v, &label_format);
            let at = polar(c, radius + 7.0, a);
            let anchor = if a.sin().abs() < 0.3 {
                Anchor::Middle
            } else if a.sin() > 0.0 {
                Anchor::Start
            } else {
                Anchor::End
            };
            let t = ctx.text_mid(
                &label,
                at[0],
                at[1] - a.cos() * lh / 2.0,
                anchor,
                Face::Figures,
                Role::AxisLabel,
                ctx.style.ink,
            );
            if t.bounds.inside(&body, 0.5) && !kept.iter().any(|k| k.overlaps(&t.bounds, 1.0)) {
                kept.push(t.bounds);
                items.push(Item::Text(t));
            } else {
                ctx.drop_label(&label, DropReason::Collision, "tick", Some(t.bounds));
            }
        }
        // The key under the arc.
        let key_top = body.bottom() - key_h + 6.0;
        for (r, row) in key_rows.iter().enumerate() {
            let widths: Vec<f64> = row
                .iter()
                .map(|&k| SWATCH + 6.0 + ctx.text_width(&zones[k].label))
                .collect();
            let total = widths.iter().sum::<f64>() + 16.0 * (row.len() as f64 - 1.0);
            let mut x = body.x + (body.w - total) / 2.0;
            let cy = key_top + (r as f64 + 0.5) * (lh + 4.0);
            for (&k, w) in row.iter().zip(&widths) {
                items.push(Item::Rect {
                    rect: Rect::new(x, cy - SWATCH / 2.0, SWATCH, SWATCH),
                    fill: Fill::solid(Style::rgba(zones[k].color)),
                    stroke: None,
                    radius: [2.0; 4],
                    role: Role::Key,
                    series: None,
                    obs: None,
                });
                items.push(Item::Text(ctx.text_mid(
                    &zones[k].label,
                    x + SWATCH + 6.0,
                    cy,
                    Anchor::Start,
                    Face::Data,
                    Role::Key,
                    ctx.style.ink_soft,
                )));
                x += w + 16.0;
            }
        }
        let mut lines = vec![ReadoutLine {
            name: title.clone(),
            value: numbers::value(value, &readout),
            series: Some(0),
            names_series: false,
            derived: false,
        }];
        // The band comes from the true value: one beyond every band names none of them.
        let zone = match (zones.first(), zones.last()) {
            (Some(_), Some(last)) => {
                if value < min {
                    Some("Below the first band".to_owned())
                } else if let Some(z) = zones.iter().find(|z| value < z.to) {
                    Some(z.label.clone())
                } else if value <= last.to {
                    Some(last.label.clone())
                } else {
                    Some("Above the last band".to_owned())
                }
            }
            _ => None,
        };
        if let Some(label) = zone {
            lines.push(ReadoutLine {
                name: "Band".into(),
                value: label,
                series: None,
                names_series: false,
                derived: true,
            });
        }
        lines.push(ReadoutLine {
            name: "Scale".into(),
            value: format!("{} to {}", numbers::value(min, &readout), numbers::value(max, &readout)),
            series: None,
            names_series: false,
            derived: true,
        });
        let at = polar(c, mid, a_value);
        let observation = Observation {
            row: Some(read.row),
            dots: vec![],
            series: Some(0),
            at,
            wedge: None,
            area: Some(bbox),
            radius: 0.0,
            x: at[0],
            readout: Readout {
                heading: read.name.clone().unwrap_or_else(|| title.clone()),
                lines,
            }
            .into(),
        };
        let mut panel = Panel {
            facet: None,
            frame: *outer,
            plot: body,
            items,
            observations: vec![observation],
            targeting: Targeting::Contain,
            x: None,
            failure: None,
        };
        super::panel_title(ctx, &mut panel, name);
        panels.push(panel);
    }
    if outside > 0 {
        ctx.check(
            "value-outside-scale",
            Level::Warning,
            format!(
                "{outside} {} beyond the scale and drawn at its edge",
                if outside == 1 { "value lies" } else { "values lie" }
            ),
            Some("Widen style.min and style.max."),
            Some("chart.style.max".into()),
            None,
            json!({"count": outside}),
        );
    }
    let drawn: Vec<&Reading> = reads.iter().flatten().collect();
    ctx.summary = match drawn.as_slice() {
        [one] => format!(
            "Gauge at {} on a scale from {} to {}.",
            numbers::value(one.value, &readout),
            numbers::value(min, &readout),
            numbers::value(max, &readout)
        ),
        many => format!(
            "{} on a scale from {} to {}.",
            super::count(many.len(), "gauge", "gauges"),
            numbers::value(min, &readout),
            numbers::value(max, &readout)
        ),
    };
    Painted {
        height: super::grid_height(facets.len(), columns, panel_h, super::FACET_GAP_Y),
        panels,
        figure_items: Vec::new(),
        legend: None,
        failure: None,
    }
}
