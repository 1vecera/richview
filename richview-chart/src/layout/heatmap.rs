//! Heatmaps (FEA-CH-10, FEA-CH-34): one cell per column and row pair, coloured on the style's
//! ramp by magnitude with a second arm for negative values, values printed where they fit and a
//! compact key under the plot where any does not.

use super::{Ctx, LABEL_GAP, MIN_PLOT, Painted, Style};
use crate::axis::{self, CategoryLayout};
use crate::color::{self, Oklch};
use crate::diag::Level;
use crate::geometry::{
    Anchor, Face, Fill, Hatch, Item, Observation, Panel, Readout, ReadoutLine, Rect, Role, Stroke, Targeting,
};
use crate::numbers;
use crate::rows::{self, Num};
use serde_json::{Value, json};
use std::collections::HashMap;

/// Least cell size that prints its value.
const VALUE_W: f64 = 40.0;
const VALUE_H: f64 = 20.0;
/// Key bar size.
const KEY_W: f64 = 240.0;
const KEY_H: f64 = 8.0;

/// The smallest of 1, 2, 2.5, 5 × 10ⁿ at or above `v`: the high stop's value.
pub fn nice_ceil(v: f64) -> f64 {
    if !(v.is_finite() && v > 0.0) {
        return 1.0;
    }
    let p = 10f64.powf(v.log10().floor());
    for m in [1.0, 2.0, 2.5, 5.0, 10.0] {
        if m * p >= v * (1.0 - 1e-12) {
            return m * p;
        }
    }
    10.0 * p
}

/// Colour scale: zero at the low stop, `top` at the high stop, negative values on their own arm.
pub struct Scale {
    pub top: f64,
    positive: [Oklch; 3],
    negative: [Oklch; 3],
}

impl Scale {
    pub fn new(style: &Style, top: f64) -> Scale {
        let negative = style.negative_ramp.unwrap_or_else(|| {
            style
                .ramp
                .map(|c| Oklch::new(c.l, c.c, (c.h + 150.0).rem_euclid(360.0)))
        });
        Scale {
            top,
            positive: style.ramp,
            negative,
        }
    }
    pub fn color(&self, v: f64) -> Oklch {
        let t = (v.abs() / self.top).clamp(0.0, 1.0);
        color::ramp(if v < 0.0 { &self.negative } else { &self.positive }, t)
    }
}

/// One source row placed in the grid.
struct Cell {
    row: usize,
    column: usize,
    line: usize,
    value: Option<f64>,
}

/// The grid of one panel, or why it cannot draw.
enum Grid {
    Cells(Vec<Cell>),
    Rejected(String),
}

pub(super) fn layout(ctx: &mut Ctx, facets: &[(Option<String>, Vec<usize>)]) -> Painted {
    let block = ctx.block;
    let x_field = block.encoding.x.clone().unwrap_or_default();
    let y_field = block.encoding.y.clone().unwrap_or_default();
    let c_field = block.encoding.color.clone().unwrap_or_default();
    let columns = rows::distinct(ctx.rows, &x_field);
    let lines = rows::distinct(ctx.rows, &y_field);
    let width = ctx.opts.width;
    let lh = ctx.line_height();
    let size = ctx.size();
    let font = ctx.style.font.clone();
    let (ascent, _) = font.vertical();
    let ink = ctx.style.ink;

    // Read every row once: unlocated, invalid and ambiguous rows are errors (FEA-CH-69).
    let mut unlocated = Vec::new();
    let mut invalid = Vec::new();
    let mut underflow: Vec<(usize, String)> = Vec::new();
    let mut grids = Vec::new();
    for (_, members) in facets {
        let mut at: HashMap<(usize, usize), (Vec<usize>, Vec<usize>)> = HashMap::new();
        for &i in members {
            let r = &ctx.rows[i];
            let value = rows::num(r, &c_field);
            let (Some(x), Some(y)) = (rows::text(r, &x_field), rows::text(r, &y_field)) else {
                if matches!(value, Num::Value(_)) {
                    unlocated.push(i);
                }
                continue;
            };
            let key = (
                columns.iter().position(|c| *c == x).unwrap_or(0),
                lines.iter().position(|l| *l == y).unwrap_or(0),
            );
            let entry = at.entry(key).or_default();
            match value {
                Num::Value(_) => entry.0.push(i),
                Num::Missing => entry.1.push(i),
                Num::Invalid(t) => invalid.push((i, t)),
                Num::Underflow(t) => underflow.push((i, t)),
            }
        }
        let mut repeated: Vec<(usize, usize, Vec<usize>)> = at
            .iter()
            .filter(|(_, (n, _))| n.len() > 1)
            .map(|(&(c, l), (n, _))| (c, l, n.clone()))
            .collect();
        repeated.sort_by_key(|(_, _, rows)| rows[0]);
        if !repeated.is_empty() {
            let cells: Vec<Value> = repeated
                .iter()
                .map(|(c, l, rows)| json!({"x": columns[*c], "y": lines[*l], "rows": rows}))
                .collect();
            let first = &repeated[0];
            ctx.check(
                "ambiguous-numeric-cell",
                Level::Error,
                format!(
                    "{} cells have more than one value, such as {} · {} in rows {}",
                    repeated.len(),
                    columns[first.0],
                    lines[first.1],
                    first
                        .2
                        .iter()
                        .map(|r| (r + 1).to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                Some("Aggregate the data so each column and row pair has one value."),
                Some(Ctx::row_path(first.2[1], &c_field)),
                None,
                json!({"cells": cells}),
            );
            grids.push(Grid::Rejected("Cannot draw this heatmap · Repeated cell values".into()));
            continue;
        }
        let mut cells = Vec::new();
        let mut conflicts = Vec::new();
        for (&(column, line), (numeric, missing)) in &at {
            if let Some(&row) = numeric.first() {
                cells.push(Cell {
                    row,
                    column,
                    line,
                    value: match rows::num(&ctx.rows[row], &c_field) {
                        Num::Value(v) => Some(v),
                        _ => None,
                    },
                });
            }
            if !missing.is_empty() {
                if numeric.is_empty() && missing.len() == 1 {
                    cells.push(Cell {
                        row: missing[0],
                        column,
                        line,
                        value: None,
                    });
                } else {
                    conflicts.extend(missing.iter().copied());
                }
            }
        }
        if !conflicts.is_empty() {
            conflicts.sort_unstable();
            ctx.check(
                "ambiguous-missing-cell",
                Level::Error,
                format!(
                    "Rows {} mark a cell missing that another row also fills",
                    conflicts
                        .iter()
                        .map(|r| (r + 1).to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                Some("Remove the missing rows or the duplicates."),
                Some(Ctx::row_path(conflicts[0], &c_field)),
                None,
                json!({"rows": conflicts}),
            );
        }
        cells.sort_by_key(|c| c.row);
        grids.push(Grid::Cells(cells));
    }
    if !unlocated.is_empty() {
        ctx.check(
            "unlocated-color-values",
            Level::Error,
            format!(
                "{} rows have a value but no {x_field} or {y_field}, so they cannot be placed",
                unlocated.len()
            ),
            Some("Fill in both coordinates or remove the rows."),
            Some(Ctx::row_path(unlocated[0], &x_field)),
            None,
            json!({"rows": unlocated}),
        );
    }
    for (row, text) in &underflow {
        ctx.underflow(*row, &c_field, text);
    }
    if !invalid.is_empty() {
        let (row, text) = &invalid[0];
        ctx.check(
            "invalid-color-values",
            Level::Error,
            format!(
                "{} {c_field} values are not numbers, such as '{text}' in row {}",
                invalid.len(),
                row + 1
            ),
            Some("Write plain numbers, or a missing marker such as NA."),
            Some(Ctx::row_path(*row, &c_field)),
            None,
            json!({"rows": invalid.iter().map(|(r, _)| r).collect::<Vec<_>>()}),
        );
    }
    let values: Vec<f64> = grids
        .iter()
        .flat_map(|g| match g {
            Grid::Cells(cells) => cells.iter().filter_map(|c| c.value).collect(),
            Grid::Rejected(_) => Vec::new(),
        })
        .collect();
    if values.is_empty() && grids.iter().any(|g| matches!(g, Grid::Cells(_))) {
        ctx.check(
            "missing-color-values",
            Level::Error,
            format!("No row has a numeric {c_field}"),
            Some("encoding.color names the numeric field to colour by."),
            Some("chart.encoding.color".into()),
            None,
            Value::Null,
        );
    }
    // A playback keeps one colour scale for every frame (FEA-BR-31).
    let wanted = ctx.wanted().color;
    let largest = values
        .iter()
        .chain(wanted.iter().flatten())
        .fold(0.0f64, |m, v| m.max(v.abs()));
    let (lo, hi) = values.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
        (lo.min(*v), hi.max(*v))
    });
    ctx.fit.color = lo.is_finite().then(|| super::Fit::widen([lo, hi], wanted));
    // The colour scale's top and whether it runs both ways jump with the data (a grid step moves a
    // value over a round number or across zero), so the exact rows decide them (TYP-36).
    let (top, diverging): (f64, bool) = ctx.derive(0, "heatmap.scale", || {
        let diverging = values.iter().chain(wanted.iter().flatten()).any(|v| *v < 0.0);
        (crate::derived::coarse([0.0, nice_ceil(largest)])[1], diverging)
    });
    let scale = Scale::new(&ctx.style, top);

    // Orientation: names wrap to two lines; transpose when that names every column (FEA-CH-10).
    // Names wrap at 30% of the panel; a single word longer than that widens the gutter up to
    // 45% rather than running off the chart's left edge.
    let panel_columns = super::facet_columns(facets.len(), width);
    let pw = (width - 24.0 * (panel_columns as f64 - 1.0)) / panel_columns as f64;
    let plan = |cols: &[String], rws: &[String]| {
        let gutter = rws
            .iter()
            .map(|n| font.lines_width(&font.wrap(n, size, pw * 0.3, 2), size))
            .fold(0.0, f64::max)
            .min(pw * 0.45)
            + LABEL_GAP;
        let band = (pw - gutter) / cols.len().max(1) as f64;
        let ordered = axis::ordered(cols, ctx.locale);
        (
            gutter,
            band,
            axis::category_layout(cols, ordered.as_ref(), band, &font, size),
        )
    };
    let names_all = |l: &CategoryLayout| matches!(l, CategoryLayout::OneLine | CategoryLayout::Wrapped(_));
    let mut transposed = false;
    let (mut gutter, mut band, mut fit) = plan(&columns, &lines);
    if !names_all(&fit) {
        let turned = plan(&lines, &columns);
        if names_all(&turned.2) {
            (gutter, band, fit) = turned;
            transposed = true;
        }
    }
    // If neither orientation fits, alternating rows can keep whole names readable in the
    // original columns. Try this after transposition, preserving the heatmap's fitting order.
    if !names_all(&fit) {
        fit = axis::category_layout_with(
            &columns,
            axis::ordered(&columns, ctx.locale).as_ref(),
            band,
            &font,
            size,
            true,
        );
    }
    let (col_names, row_names) = if transposed {
        (&lines, &columns)
    } else {
        (&columns, &lines)
    };
    let (col_title, row_title) = if transposed {
        (&block.y.label, &block.x.label)
    } else {
        (&block.x.label, &block.y.label)
    };
    let label_lines: Vec<Vec<String>> = match &fit {
        CategoryLayout::OneLine => col_names.iter().map(|n| vec![n.clone()]).collect(),
        CategoryLayout::Wrapped(l)
        | CategoryLayout::Thinned { lines: l, .. }
        | CategoryLayout::Unfit { lines: l, .. } => l.clone(),
    };
    match &fit {
        CategoryLayout::Thinned { every, lines } => {
            ctx.check(
                "label-thinned",
                Level::Info,
                format!("Column names show every {}", axis::nth_printed(lines)),
                None,
                Some("chart.encoding.x".into()),
                None,
                json!({"every": every}),
            );
        }
        CategoryLayout::Unfit { unfit, .. } => {
            let names: Vec<&String> = unfit.iter().map(|&i| &col_names[i]).collect();
            ctx.check(
                "label-unfit",
                Level::Warning,
                format!(
                    "{} column names do not fit their columns: {}",
                    names.len(),
                    names.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                ),
                Some("Shorten the names or widen the chart."),
                Some("chart.encoding.x".into()),
                None,
                json!({"names": names}),
            );
        }
        _ => {}
    }

    // Rows are at least 1.6 lines tall; the chart grows with them (FEA-CH-65).
    let row_wraps: Vec<Vec<String>> = row_names
        .iter()
        .map(|n| font.wrap(n, size, gutter - LABEL_GAP, 2))
        .collect();
    let most_lines = row_wraps.iter().map(Vec::len).max().unwrap_or(1) as f64;
    let need = (1.6 * lh).max(most_lines * lh + 6.0);
    let title_room = if block.encoding.facet.is_some() { lh + 6.0 } else { 0.0 };
    let top = title_room + if row_title.is_some() { lh + 6.0 } else { 0.0 };
    let label_rows = label_lines.iter().map(Vec::len).max().unwrap_or(1).max(1) as f64;
    let below = LABEL_GAP + label_rows * lh + if col_title.is_some() { lh + 4.0 } else { 0.0 };
    let ratio = block.style.ratio.unwrap_or_else(|| block.kind.default_ratio(false));
    let nrows = row_names.len().max(1) as f64;
    let from_ratio = (pw / ratio - top - below).max(MIN_PLOT) / nrows;
    let cell_h = need.max(from_ratio);
    let panel_h = top + cell_h * nrows + below;

    let frames = super::grid_frames(facets.len(), width, 0.0, panel_h);
    let mut panels = Vec::new();
    let mut unprinted = 0usize;
    let label_format = block.label_format(&block.color, ctx.locale);
    let readout = block.readout_format(&block.color, ctx.locale);
    let value_name = block
        .color
        .label
        .clone()
        .unwrap_or_else(|| super::cartesian::readable_name(&c_field));
    let surface = ctx.style.surface;
    for (((name, _), frame), grid) in facets.iter().zip(&frames).zip(&grids) {
        let plot = Rect::new(frame.x + gutter, frame.y + top, frame.w - gutter, cell_h * nrows);
        let mut panel = match grid {
            Grid::Rejected(reason) => super::failed_panel(ctx, plot, reason),
            Grid::Cells(cells) => {
                let mut items = Vec::new();
                let mut observations = Vec::new();
                for cell in cells {
                    let (ci, ri) = if transposed {
                        (cell.line, cell.column)
                    } else {
                        (cell.column, cell.line)
                    };
                    let rect = Rect::new(plot.x + ci as f64 * band, plot.y + ri as f64 * cell_h, band, cell_h);
                    // A 1 px surface gap keeps neighbouring cells apart.
                    let inner = Rect::new(
                        rect.x + 0.5,
                        rect.y + 0.5,
                        (rect.w - 1.0).max(0.0),
                        (rect.h - 1.0).max(0.0),
                    );
                    let center = rect.center();
                    let read = match cell.value {
                        Some(v) => {
                            let fill = scale.color(v);
                            // A style whose arms share one hue marks the sign with a hatch.
                            let mut painted = Fill::solid(Style::rgba(fill));
                            if v < 0.0 && ctx.style.sign_hatch {
                                painted.hatch = Some(Hatch::Diagonal);
                                painted.hatch_ink = Some(Style::rgba(
                                    if color::contrast(ink, fill) >= color::contrast(surface, fill) {
                                        ink
                                    } else {
                                        surface
                                    },
                                ));
                            }
                            items.push(Item::Rect {
                                rect: inner,
                                fill: painted,
                                stroke: None,
                                radius: [0.0; 4],
                                role: Role::Mark,
                                series: None,
                                obs: Some(observations.len()),
                            });
                            // The author's `auto_annotations: false` prints no cell value; the
                            // key below then decodes the colour.
                            let text = numbers::value(v, &label_format);
                            if ctx.block.auto_annotations
                                && rect.w >= VALUE_W
                                && rect.h >= VALUE_H
                                && ctx.text_width(&text) <= rect.w - 10.0
                            {
                                let cell_ink = if color::contrast(ink, fill) >= color::contrast(surface, fill) {
                                    ink
                                } else {
                                    surface
                                };
                                items.push(Item::Text(ctx.text_mid(
                                    &text,
                                    center[0],
                                    center[1],
                                    Anchor::Middle,
                                    Face::Figures,
                                    Role::Label,
                                    cell_ink,
                                )));
                            } else {
                                unprinted += 1;
                            }
                            numbers::value(v, &readout)
                        }
                        None => {
                            let stroke = Stroke {
                                dash: vec![2.0, 3.0],
                                ..Stroke::solid(Style::rgba(ctx.style.ink_faint), 1.0)
                            };
                            items.push(Item::Rect {
                                rect: Rect::new(
                                    inner.x + 0.5,
                                    inner.y + 0.5,
                                    (inner.w - 1.0).max(0.0),
                                    (inner.h - 1.0).max(0.0),
                                ),
                                fill: Fill::solid(Style::rgba(surface)),
                                stroke: Some(stroke),
                                radius: [0.0; 4],
                                role: Role::Missing,
                                series: None,
                                obs: Some(observations.len()),
                            });
                            if rect.w >= 24.0 && rect.h >= VALUE_H {
                                items.push(Item::Text(ctx.text_mid(
                                    "n/a",
                                    center[0],
                                    center[1],
                                    Anchor::Middle,
                                    Face::Data,
                                    Role::Missing,
                                    ctx.style.ink_soft,
                                )));
                            }
                            "No observation".to_owned()
                        }
                    };
                    observations.push(Observation {
                        row: Some(cell.row),
                        dots: vec![],
                        series: None,
                        at: center,
                        area: Some(rect),
                        wedge: None,
                        radius: 0.0,
                        x: center[0],
                        readout: Readout {
                            heading: format!("{} · {}", columns[cell.column], lines[cell.line]),
                            lines: vec![ReadoutLine {
                                label: None,
                                name: value_name.clone(),
                                value: read,
                                series: None,
                                names_series: false,
                                derived: false,
                            }],
                        }
                        .into(),
                    });
                }
                Panel {
                    facet: None,
                    frame: *frame,
                    plot,
                    items,
                    observations,
                    targeting: Targeting::Contain,
                    x: None,
                    failure: None,
                }
            }
        };
        // Names, which the panel keeps even when it cannot draw.
        for (i, wrapped) in row_wraps.iter().enumerate() {
            let cy = plot.y + (i as f64 + 0.5) * cell_h;
            let first = cy - (wrapped.len() as f64 - 1.0) * lh / 2.0;
            for (k, line) in wrapped.iter().enumerate() {
                panel.items.push(Item::Text(ctx.text_mid(
                    line,
                    plot.x - LABEL_GAP,
                    first + k as f64 * lh,
                    Anchor::End,
                    Face::Data,
                    Role::AxisLabel,
                    ink,
                )));
            }
        }
        for (i, wrapped) in label_lines.iter().enumerate() {
            let x = plot.x + (i as f64 + 0.5) * band;
            for (k, line) in wrapped.iter().enumerate() {
                let y = plot.bottom() + LABEL_GAP + k as f64 * lh + ascent * size;
                panel.items.push(Item::Text(ctx.text(
                    line,
                    x,
                    y,
                    Anchor::Middle,
                    Face::Data,
                    Role::AxisLabel,
                    ink,
                )));
            }
        }
        if let Some(label) = row_title {
            panel.items.push(Item::Text(ctx.text(
                label,
                frame.x,
                frame.y + title_room + ascent * size,
                Anchor::Start,
                Face::Data,
                Role::AxisTitle,
                ctx.style.ink_soft,
            )));
        }
        if let Some(label) = col_title {
            let y = plot.bottom() + LABEL_GAP + label_rows * lh + 4.0 + ascent * size;
            panel.items.push(Item::Text(ctx.text(
                label,
                plot.right(),
                y,
                Anchor::End,
                Face::Data,
                Role::AxisTitle,
                ctx.style.ink_soft,
            )));
        }
        if let Some(name) = name {
            let t = ctx.text(
                name,
                frame.x,
                frame.y + font.baseline_offset(size),
                Anchor::Start,
                Face::Data,
                Role::PanelTitle,
                ink,
            );
            panel.items.push(Item::Text(crate::geometry::Text { weight: 600, ..t }));
        }
        panels.push(panel);
    }
    let mut height = super::grid_height(facets.len(), panel_columns, panel_h, super::FACET_GAP_Y);

    // Unprinted values: one record, and a key so print readers can decode colour (FEA-CH-34).
    let mut figure_items = Vec::new();
    if unprinted > 0 && !values.is_empty() {
        // An author who turned the values off chose it: that is no dropped label.
        if ctx.block.auto_annotations {
            ctx.drop_label(
                &format!("{unprinted} cell values"),
                crate::geometry::DropReason::Density,
                "cell-value",
                None,
            );
        }
        let key_w = KEY_W.min(width - gutter);
        let x0 = gutter.min(width - key_w);
        let y = height + 14.0;
        let lo = if diverging { -scale.top } else { 0.0 };
        let steps = 48;
        for s in 0..steps {
            let t0 = s as f64 / steps as f64;
            let v = lo + (scale.top - lo) * (t0 + 0.5 / steps as f64);
            let rect = Rect::new(x0 + t0 * key_w, y, key_w / steps as f64 + 0.3, KEY_H);
            let key_color = scale.color(v);
            let mut key_fill = Fill::solid(Style::rgba(key_color));
            if v < 0.0 && ctx.style.sign_hatch {
                key_fill.hatch = Some(Hatch::Diagonal);
                key_fill.hatch_ink = Some(Style::rgba(
                    if color::contrast(ink, key_color) >= color::contrast(surface, key_color) {
                        ink
                    } else {
                        surface
                    },
                ));
            }
            figure_items.push(Item::Rect {
                rect,
                fill: key_fill,
                stroke: None,
                radius: [0.0; 4],
                role: Role::Key,
                series: None,
                obs: None,
            });
        }
        let label_y = y + KEY_H + 4.0 + ascent * size;
        let mut ends = vec![(lo, x0, Anchor::Start), (scale.top, x0 + key_w, Anchor::End)];
        if diverging {
            ends.insert(1, (0.0, x0 + key_w / 2.0, Anchor::Middle));
        }
        for (v, x, anchor) in ends {
            figure_items.push(Item::Text(ctx.text(
                &numbers::value(v, &label_format),
                x,
                label_y,
                anchor,
                Face::Figures,
                Role::Key,
                ctx.style.ink_soft,
            )));
        }
        height = label_y + lh / 2.0;
    }

    let (min, max) = values.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
        (lo.min(*v), hi.max(*v))
    });
    ctx.summary = if values.is_empty() {
        format!(
            "{} rows by {} columns with no values.",
            row_names.len(),
            col_names.len()
        )
    } else {
        format!(
            "{} rows by {} columns; values from {} to {}.",
            row_names.len(),
            col_names.len(),
            numbers::value(min, &readout),
            numbers::value(max, &readout)
        )
    };
    let failure = grids
        .iter()
        .all(|g| matches!(g, Grid::Rejected(_)))
        .then(|| "Cannot draw this heatmap · Repeated cell values".to_owned());
    Painted {
        height,
        panels,
        figure_items,
        legend: None,
        failure,
    }
}

#[cfg(test)]
mod tests {
    use super::nice_ceil;
    use crate::block::ChartBlock;
    use crate::geometry::{Item, Role};
    use crate::layout::{ChartTheme, Options, layout};
    use serde_json::{Value, json};

    fn chart(rows: &[Value], width: f64) -> crate::geometry::Geometry {
        let block = ChartBlock::from_value(
            &json!({"type": "heatmap", "title": "T", "encoding": {"x": "c", "y": "r", "color": "v"}}),
        )
        .unwrap();
        layout(
            &block,
            rows,
            &ChartTheme::default(),
            &Options {
                width,
                ..Options::default()
            },
        )
    }

    #[test]
    fn the_high_stop_rounds_up() {
        assert_eq!(nice_ceil(87.0), 100.0);
        assert_eq!(nice_ceil(0.23), 0.25);
        assert_eq!(nice_ceil(1.0), 1.0);
        assert_eq!(nice_ceil(4100.0), 5000.0);
    }

    #[test]
    fn large_cells_print_values_and_draw_no_key() {
        let rows: Vec<Value> = ["A", "B"]
            .iter()
            .flat_map(|c| ["x", "y"].iter().map(move |r| json!({"c": c, "r": r, "v": 12})))
            .collect();
        let g = chart(&rows, 640.0);
        assert!(g.failure.is_none());
        let labels = g.panels[0]
            .items
            .iter()
            .filter(|i| matches!(i, Item::Text(t) if t.role == Role::Label))
            .count();
        assert_eq!(labels, 4);
        assert!(g.figure_items.is_empty(), "no key when every value prints");
        assert!(g.panels[0].plot.h / 2.0 >= 1.6 * 12.0);
    }

    #[test]
    fn small_cells_get_a_key_and_one_dropped_record() {
        let rows: Vec<Value> = (0..24)
            .flat_map(|h| {
                (0..24).map(move |d| json!({"c": h.to_string(), "r": format!("Day {d}"), "v": (h * d) as f64 - 20.0}))
            })
            .collect();
        let g = chart(&rows, 480.0);
        assert!(g.figure_items.iter().any(|i| i.role() == Role::Key));
        assert_eq!(g.dropped.len(), 1);
        let key_labels: Vec<_> = g
            .figure_items
            .iter()
            .filter_map(|i| {
                if let Item::Text(t) = i {
                    Some(t.text.clone())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(key_labels.len(), 3, "diverging data shows zero: {key_labels:?}");
    }

    #[test]
    fn repeated_cells_reject_the_panel_and_missing_cells_stay_outlined() {
        let g = chart(
            &[json!({"c": "A", "r": "x", "v": 1}), json!({"c": "A", "r": "x", "v": 2})],
            480.0,
        );
        assert!(g.checks.iter().any(|c| c.kind == "ambiguous-numeric-cell"));
        assert!(g.panels[0].failure.is_some());
        let g = chart(
            &[
                json!({"c": "A", "r": "x", "v": 1}),
                json!({"c": "B", "r": "x", "v": "NA"}),
            ],
            480.0,
        );
        assert!(g.panels[0].items.iter().any(|i| i.role() == Role::Missing));
        let readout = &g.panels[0].observations[1].readout;
        assert_eq!(readout.heading, "B · x");
        assert_eq!(readout.lines.len(), 1);
        assert_eq!(readout.lines[0].value, "No observation");
    }

    #[test]
    fn unplaceable_and_non_numeric_values_are_errors() {
        let g = chart(
            &[
                json!({"c": "A", "v": 1}),
                json!({"c": "A", "r": "x", "v": "1,2"}),
                json!({"c": "B", "r": "x", "v": 3}),
            ],
            480.0,
        );
        assert!(g.checks.iter().any(|c| c.kind == "unlocated-color-values"));
        assert!(g.checks.iter().any(|c| c.kind == "invalid-color-values"));
    }

    #[test]
    fn long_column_names_transpose_the_grid() {
        let names = [
            "Northern Europe and the Baltics",
            "Southern Europe and the Balkans",
            "Western Europe and the Benelux",
            "Central and Eastern Europe",
        ];
        let rows: Vec<Value> = names
            .iter()
            .flat_map(|c| ["2024", "2025"].iter().map(move |r| json!({"c": c, "r": r, "v": 5})))
            .collect();
        let g = chart(&rows, 360.0);
        let rows_text: Vec<_> = g.panels[0]
            .items
            .iter()
            .filter_map(|i| {
                if let Item::Text(t) = i {
                    (t.role == Role::AxisLabel && t.anchor == crate::geometry::Anchor::End).then(|| t.text.clone())
                } else {
                    None
                }
            })
            .collect();
        assert!(rows_text.iter().any(|t| t.contains("Europe")), "{rows_text:?}");
    }

    #[test]
    fn flagship_provider_names_fit_the_phone_figure_in_measured_style_fonts() {
        let labs = ["OpenAI", "Anthropic", "Google"];
        let benchmarks = [
            "SWE-bench Verified",
            "Terminal-Bench",
            "METR time horizon",
            "GDPval",
            "OSWorld",
            "GPQA Diamond",
            "FrontierMath",
            "Humanity's Last Exam",
            "ARC-AGI-2",
        ];
        let rows: Vec<Value> = benchmarks
            .iter()
            .flat_map(|r| labs.iter().map(move |c| json!({"c": c, "r": r, "v": 50})))
            .collect();
        let block = ChartBlock::from_value(&json!({
            "type": "heatmap", "title": "T",
            "encoding": {"x": "c", "y": "r", "color": "v"},
            "axes": {"x": {"label": "Lab"}, "y": {"label": "Benchmark"}},
            "style": {"ratio": "4:3"}
        }))
        .unwrap();
        for family in [
            "Geist Mono, JetBrains Mono, ui-monospace, monospace",
            "Space Mono, ui-monospace, monospace",
        ] {
            let font = crate::text::Font::new(family);
            assert!(font.is_measured());
            let theme = ChartTheme {
                data_font: family.into(),
                ..ChartTheme::default()
            };
            // A 320 px viewport leaves a 288 px figure inside the report's phone gutters.
            for width in [288.0, 320.0, 390.0, 640.0] {
                let g = layout(
                    &block,
                    &rows,
                    &theme,
                    &Options {
                        width,
                        ..Options::default()
                    },
                );
                let labels: Vec<_> = g.panels[0]
                    .items
                    .iter()
                    .filter_map(|item| match item {
                        Item::Text(t) if t.role == Role::AxisLabel && labs.contains(&t.text.as_str()) => Some(t),
                        _ => None,
                    })
                    .collect();
                assert_eq!(labels.len(), labs.len(), "{family} at {width}");
                for (i, a) in labels.iter().enumerate() {
                    let bounds = font.bounds(&a.text, a.size, a.x, a.y, a.anchor);
                    assert!(bounds.x >= 0.0 && bounds.right() <= width, "{family} at {width}: {a:?}");
                    for b in &labels[i + 1..] {
                        assert!(
                            !bounds.overlaps(&font.bounds(&b.text, b.size, b.x, b.y, b.anchor), 0.0),
                            "{family} at {width}: {} overlaps {}",
                            a.text,
                            b.text
                        );
                    }
                }
                assert!(!g.checks.iter().any(|c| c.kind == "label-unfit"), "{family} at {width}");
            }
        }
    }
}
