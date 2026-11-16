//! Waffle charts: a grid of 100 squares (or 25 or 50) per panel, each category holding the
//! share of squares its share of the total earns, by the largest remainder method so the squares
//! add up to exactly the grid. Squares fill column by column from the bottom left in source
//! order. The value in a readout is the one given; the share and the square count are derived
//! (FEA-CH-42, FEA-CH-70).

use super::graph;
use super::kit;
use super::pie::share_text;
use super::types::Facets;
use super::{Ctx, Painted, Style};
use crate::diag::Level;
use crate::geometry::{Fill, Item, Observation, Panel, Readout, ReadoutLine, Rect, Role, Stroke, Swatch, Targeting};
use crate::numbers;
use crate::rows::{self, Num};
use serde_json::json;

const RATIO: f64 = 1.0;
/// Largest side of a panel's grid when the author sets no ratio.
const MAX_SIDE: f64 = 440.0;
/// Share of a square's pitch left empty between squares.
const GAP: f64 = 0.14;

struct Part {
    row: usize,
    name: String,
    slot: usize,
    value: f64,
}

/// Columns and rows of a grid of `cells` squares.
fn grid(cells: usize) -> (usize, usize) {
    match cells {
        25 => (5, 5),
        50 => (10, 5),
        _ => (10, 10),
    }
}

/// Squares per part by the largest remainder method: each part gets the whole part of its exact
/// share of `cells`, and the squares left over go to the largest remainders, earlier parts first.
pub(crate) fn squares(values: &[f64], cells: usize) -> Vec<usize> {
    let total: f64 = values.iter().sum();
    if total <= 0.0 || !total.is_finite() {
        return vec![0; values.len()];
    }
    let exact: Vec<f64> = values.iter().map(|v| v / total * cells as f64).collect();
    let mut out: Vec<usize> = exact.iter().map(|e| (e + 1e-9).floor() as usize).collect();
    let mut left = cells.saturating_sub(out.iter().sum());
    let mut order: Vec<usize> = (0..values.len()).collect();
    order.sort_by(|&a, &b| {
        let (ra, rb) = (exact[a] - out[a] as f64, exact[b] - out[b] as f64);
        rb.total_cmp(&ra).then(a.cmp(&b))
    });
    for &k in order.iter().cycle().take(values.len().max(1) * 2) {
        if left == 0 {
            break;
        }
        out[k] += 1;
        left -= 1;
    }
    out
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let x_field = block.encoding.x.clone().unwrap_or_default();
    let y_field = block.encoding.y.clone().unwrap_or_default();
    let readout = block.readout_format(&block.y, ctx.locale);
    let value_name = block.y.label.clone().unwrap_or_else(|| y_field.clone());
    let cells = match block.style_number("cells") {
        Some(c) if c == 25.0 || c == 50.0 || c == 100.0 => c as usize,
        Some(c) => {
            ctx.check(
                "style-invalid",
                Level::Warning,
                format!("style.cells is {c}; a waffle has 25, 50 or 100 squares, so it draws 100"),
                Some("Set style.cells to 25, 50 or 100."),
                Some("chart.style.cells".into()),
                None,
                json!({"cells": c}),
            );
            100
        }
        None => 100,
    };
    let (cols, grid_rows) = grid(cells);
    let names = rows::distinct(ctx.rows, &x_field);
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut excluded = 0usize;
    let mut parts_by_panel: Vec<Vec<Part>> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut parts: Vec<Part> = Vec::new();
        // Which values are zero or negative is the exact rows' call: a small one is zero on the grid.
        let skipped = graph::skipped(ctx, fi, members, Some(&y_field));
        for &i in members {
            let row = &ctx.rows[i];
            let Some(name) = rows::text(row, &x_field) else {
                excluded += 1;
                continue;
            };
            let value = match rows::num(row, &y_field) {
                Num::Value(v) if skipped.negative.contains(&i) => {
                    ctx.check(
                        "source-error",
                        Level::Error,
                        format!(
                            "Row {} has the negative value {}; a waffle holds counts or shares",
                            i + 1,
                            numbers::value(v, &readout)
                        ),
                        Some("Use a bar chart for signed values."),
                        Some(Ctx::row_path(i, &y_field)),
                        None,
                        json!({"row": i}),
                    );
                    failures[fi]
                        .get_or_insert_with(|| format!("Cannot draw this waffle · {y_field} has a negative value"));
                    continue;
                }
                Num::Value(_) if skipped.zero.contains(&i) => 0.0,
                Num::Value(v) => v.max(f64::MIN_POSITIVE),
                Num::Missing => {
                    excluded += 1;
                    continue;
                }
                Num::Underflow(t) => {
                    ctx.underflow(i, &y_field, &t);
                    failures[fi].get_or_insert_with(|| {
                        format!("Cannot draw this waffle · {y_field} has a value too small to represent")
                    });
                    continue;
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
                        format!("Cannot draw this waffle · {y_field} has text that is not a number")
                    });
                    continue;
                }
            };
            if parts.iter().any(|p| p.name == name) {
                ctx.check(
                    "source-error",
                    Level::Error,
                    format!("Row {} repeats the category '{name}'", i + 1),
                    Some("Give each category one row per panel."),
                    Some(Ctx::row_path(i, &x_field)),
                    None,
                    json!({"row": i}),
                );
                failures[fi].get_or_insert_with(|| "Cannot draw this waffle · A category appears twice".into());
                continue;
            }
            let slot = names.iter().position(|n| *n == name).unwrap_or(0);
            parts.push(Part {
                row: i,
                name,
                slot,
                value,
            });
        }
        if failures[fi].is_none() && parts.iter().map(|p| p.value).sum::<f64>() <= 0.0 {
            failures[fi] = Some(if parts.is_empty() {
                "Nothing to draw · No category has a value".into()
            } else {
                "Nothing to draw · Every value is zero".into()
            });
        }
        parts_by_panel.push(parts);
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
                "{} without a category or value {} left out",
                super::count(excluded, "row", "rows"),
                super::verb(excluded, "is", "are")
            ),
            Some("Give every row a category and a value."),
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
    ctx.set_series(&names);
    ctx.note.push(format!(
        "each square is 1/{cells} of the total; squares are shared out by the largest remainder"
    ));
    let width = ctx.opts.width;
    let lh = ctx.line_height();
    let title_room = if block.encoding.facet.is_some() { lh + 6.0 } else { 0.0 };
    let columns = super::facet_columns(facets.len(), width);
    let pw = (width - 24.0 * (columns as f64 - 1.0)) / columns as f64;
    let panel_h = match block.style.ratio {
        Some(r) => pw / r + title_room,
        None => (pw / RATIO).min(MAX_SIDE) * grid_rows as f64 / cols as f64 + title_room,
    };
    let frames = super::grid_frames(facets.len(), width, 0.0, panel_h);
    let surface = Style::rgba(ctx.style.surface);
    let mut panels = Vec::new();
    let mut drawn = 0usize;
    let mut unseen: Vec<String> = Vec::new();
    for (fi, ((name, _), outer)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *outer, title_room, name, reason));
            continue;
        }
        let parts = &parts_by_panel[fi];
        let values: Vec<f64> = parts.iter().map(|p| p.value).collect();
        let total: f64 = values.iter().sum();
        // The squares each part earns jump with the values (the largest remainders), so the exact
        // rows share them out (TYP-36).
        let (counts, shares): (Vec<usize>, Vec<String>) = ctx.derive(fi, "waffle.squares", || {
            (
                squares(&values, cells),
                values.iter().map(|v| share_text(v / total, ctx.locale)).collect(),
            )
        });
        let body = Rect::new(outer.x, outer.y + title_room, outer.w, (outer.h - title_room).max(1.0));
        let pitch = (body.w / cols as f64)
            .min(body.h / grid_rows as f64)
            .min(MAX_SIDE / cols as f64);
        let (gw, gh) = (pitch * cols as f64, pitch * grid_rows as f64);
        let origin = [body.x + (body.w - gw) / 2.0, body.y + (body.h - gh) / 2.0];
        let side = pitch * (1.0 - GAP);
        let inset = (pitch - side) / 2.0;
        let plot = Rect::new(origin[0], origin[1], gw, gh);
        let mut items: Vec<Item> = Vec::new();
        let mut observations: Vec<Observation> = Vec::new();
        let mut next = 0usize;
        for ((p, &n), share) in parts.iter().zip(&counts).zip(&shares) {
            let first = next;
            next += n;
            if n == 0 {
                unseen.push(p.name.clone());
                continue;
            }
            let hidden = ctx.is_hidden(p.slot);
            let color = Style::rgba(ctx.series_color(p.slot));
            let hatch = ctx.series[p.slot].hatch;
            let obs = observations.len();
            let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
            for k in first..next {
                let (col, row) = (k / grid_rows, k % grid_rows);
                let rect = Rect::new(
                    origin[0] + col as f64 * pitch + inset,
                    origin[1] + gh - (row as f64 + 1.0) * pitch + inset,
                    side,
                    side,
                );
                x0 = x0.min(rect.x);
                y0 = y0.min(rect.y);
                x1 = x1.max(rect.right());
                y1 = y1.max(rect.bottom());
                if hidden {
                    continue;
                }
                drawn += 1;
                items.push(Item::Rect {
                    rect,
                    fill: Fill {
                        color,
                        hatch,
                        hatch_ink: hatch.map(|_| surface),
                        hatch_scale: 1.0,
                    },
                    stroke: hatch.map(|_| Stroke::solid(color, 1.0)),
                    radius: [pitch * 0.08; 4],
                    role: Role::Mark,
                    series: Some(p.slot),
                    obs: Some(obs),
                });
            }
            if hidden {
                continue;
            }
            let area = Rect::new(x0, y0, x1 - x0, y1 - y0);
            observations.push(Observation {
                row: Some(p.row),
                dots: vec![],
                series: Some(p.slot),
                at: area.center(),
                wedge: None,
                area: Some(area),
                radius: 0.0,
                x: area.center()[0],
                readout: Readout {
                    heading: p.name.clone(),
                    lines: vec![
                        ReadoutLine {
                            name: value_name.clone(),
                            value: numbers::value(p.value, &readout),
                            series: Some(p.slot),
                            names_series: false,
                            derived: false,
                        },
                        ReadoutLine {
                            name: "Share".into(),
                            value: share.clone(),
                            series: None,
                            names_series: false,
                            derived: true,
                        },
                        ReadoutLine {
                            name: "Squares".into(),
                            value: format!("{n} of {cells}"),
                            series: None,
                            names_series: false,
                            derived: true,
                        },
                    ],
                }
                .into(),
            });
        }
        let mut panel = Panel {
            facet: None,
            frame: *outer,
            plot,
            items,
            observations,
            targeting: Targeting::Contain,
            x: None,
            failure: None,
        };
        super::panel_title(ctx, &mut panel, name);
        panels.push(panel);
    }
    if !unseen.is_empty() {
        unseen.dedup();
        ctx.check(
            "no-square",
            Level::Info,
            format!(
                "{} too small to earn a square: {}",
                if unseen.len() == 1 {
                    "A category is"
                } else {
                    "Categories are"
                },
                unseen.join(", ")
            ),
            Some("Use style.cells: 100 for finer shares, or read the table."),
            Some("chart.style.cells".into()),
            None,
            json!({"categories": unseen}),
        );
        ctx.note.push(format!("{} has no square", unseen.join(", ")));
    }
    ctx.summary = format!(
        "Waffle of {} categories in a grid of {cells} squares; {drawn} squares shown.",
        names.len()
    );
    let legend = kit::legend(ctx, Swatch::Square);
    Painted {
        height: super::grid_height(facets.len(), columns, panel_h, super::FACET_GAP_Y),
        panels,
        figure_items: Vec::new(),
        legend,
        failure: None,
    }
}
