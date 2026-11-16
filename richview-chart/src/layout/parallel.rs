//! Parallel coordinates: one vertical axis for each of several numeric fields, each scaled to its
//! own smallest and largest value, and one polyline for each row through its value on every axis
//! (FEA-CH-75). A row with a missing value on any axis is left out and disclosed; RichView never
//! joins across a gap. The ends of an axis are labelled with the values they stand for, so the
//! scale of each axis is readable and nothing in the picture is computed (FEA-CH-42).

use super::kit;
use super::sample::{disclose_excluded, read_number};
use super::types::Facets;
use super::{Ctx, GRID_WIDTH, MIN_PLOT, Painted, Style};
use crate::diag::Level;
use crate::geometry::{
    Anchor, DropReason, Face, Fill, Item, Observation, Panel, Path, Readout, ReadoutLine, Rect, Role, Shape, Stroke,
    Swatch, Targeting,
};
use crate::limits;
use crate::numbers;
use crate::rows;

const RATIO: f64 = 16.0 / 9.0;
const LINE_WIDTH: f64 = 1.5;
/// Radius of the dot on the first axis that stands for a row when pointing at it.
const HANDLE: f64 = 2.5;
/// Most lines an axis name wraps to.
const NAME_LINES: usize = 2;
/// Space between the rows of text around the plot.
const PAD: f64 = 4.0;

struct Polyline {
    row: usize,
    group: usize,
    values: Vec<f64>,
}

fn fail(ctx: &mut Ctx, reason: &str, fix: &str) -> Painted {
    ctx.check(
        "source-error",
        Level::Error,
        reason,
        Some(fix),
        Some("chart.encoding.dimensions".into()),
        None,
        serde_json::Value::Null,
    );
    kit::failed(ctx, reason, RATIO)
}

/// The smallest and largest value on each axis over the polylines in `sets`.
fn ranges(sets: &[&Vec<Polyline>], axes: usize) -> Vec<[f64; 2]> {
    let mut out = vec![[f64::INFINITY, f64::NEG_INFINITY]; axes];
    for line in sets.iter().flat_map(|s| s.iter()) {
        for (range, &v) in out.iter_mut().zip(&line.values) {
            range[0] = range[0].min(v);
            range[1] = range[1].max(v);
        }
    }
    out
}

/// Where `v` sits between `lo` and `hi`, from 0 to 1; the middle when the axis has no spread.
fn unit(v: f64, [lo, hi]: [f64; 2]) -> f64 {
    if hi > lo {
        let t = crate::scale::position(v, lo, hi);
        if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.5 }
    } else {
        0.5
    }
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let mut dims = block.channel_list("dimensions");
    let mut seen = std::collections::HashSet::new();
    dims.retain(|d| seen.insert(d.clone()));
    if dims.len() < 2 {
        return fail(
            ctx,
            "Cannot draw this parallel coordinates chart · It needs encoding.dimensions with at least two fields",
            "List two or more numeric fields in encoding.dimensions.",
        );
    }
    if dims.len() > limits::max("parallel", "axes") {
        let reason = kit::over_limit(ctx, "parallel", "axes", dims.len(), Some("chart.encoding.dimensions"));
        return kit::failed(ctx, &reason, RATIO);
    }
    let n = dims.len();
    let groups: Vec<String> = match &block.encoding.color {
        Some(f) => rows::distinct(ctx.rows, f),
        None => vec!["Rows".to_owned()],
    };
    ctx.set_series(&groups);
    let locale = ctx.locale;
    let label_format = block.label_format(&block.y, locale);
    let readout = block.readout_format(&block.y, locale);
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut excluded = 0usize;
    let mut per_panel: Vec<Vec<Polyline>> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut lines: Vec<Polyline> = Vec::new();
        for &i in members {
            let row = &ctx.rows[i];
            let group = match &block.encoding.color {
                Some(f) => match rows::text(row, f).and_then(|g| groups.iter().position(|x| *x == g)) {
                    Some(g) => g,
                    None => {
                        excluded += 1;
                        continue;
                    }
                },
                None => 0,
            };
            let mut values = Vec::with_capacity(n);
            let (mut missing, mut bad) = (false, false);
            for field in &dims {
                match read_number(ctx, "parallel coordinates chart", &mut failures[fi], i, field) {
                    Ok(Some(v)) => values.push(v),
                    Ok(None) => missing = true,
                    Err(()) => bad = true,
                }
            }
            if bad {
                continue;
            }
            if missing {
                excluded += 1;
                continue;
            }
            lines.push(Polyline { row: i, group, values });
        }
        if lines.is_empty() && failures[fi].is_none() {
            failures[fi] =
                Some("Cannot draw this parallel coordinates chart · No row has a value on every axis".into());
        }
        per_panel.push(lines);
    }
    if failures.iter().all(Option::is_some) {
        let reason = failures.iter().flatten().next().cloned().unwrap_or_default();
        return kit::failed(ctx, &reason, RATIO);
    }
    disclose_excluded(ctx, excluded, "a value on every axis or a group");
    let live: Vec<&Vec<Polyline>> = per_panel
        .iter()
        .zip(&failures)
        .filter(|(_, f)| f.is_none())
        .map(|(l, _)| l)
        .collect();
    let shared = ranges(&live, n);
    let label_text = |v: f64| numbers::value(v, &label_format);
    // The end values of each axis set its scale and are written at its ends. They jump with the
    // data (a grid step moves them, and their texts with them), so the exact rows decide both
    // (TYP-36); the ends are widened to short decimals (FEA-CH-37) before the texts are formatted.
    type Ends = (Vec<Vec<[f64; 2]>>, Vec<Vec<[String; 2]>>);
    let (panel_ranges, end_texts): Ends = per_panel
        .iter()
        .zip(&failures)
        .enumerate()
        .map(|(fi, (l, f))| {
            ctx.derive(fi, "parallel.ranges", || {
                if f.is_some() {
                    return (Vec::new(), Vec::new());
                }
                let own: Vec<[f64; 2]> = if block.style.free_scales {
                    ranges(&[l], n)
                } else {
                    shared.clone()
                }
                .into_iter()
                .map(crate::derived::coarse)
                .collect();
                let texts = own.iter().map(|r| [label_text(r[0]), label_text(r[1])]).collect();
                (own, texts)
            })
        })
        .unzip();
    let lh = ctx.line_height();
    let size = ctx.size();
    let font = ctx.style.font.clone();
    let mut plan = kit::Plan::xy(ctx, facets.len(), block.style.ratio.unwrap_or(RATIO));
    let width = ctx.opts.width;
    let pw = (width - 24.0 * (plan.columns as f64 - 1.0)) / plan.columns as f64;
    // Room at both sides for the end labels and the names of the first and last axis.
    let edge_width = |k: usize| -> f64 {
        end_texts
            .iter()
            .filter(|t| !t.is_empty())
            .flat_map(|t| t[k].iter())
            .map(|text| font.width(text, size))
            .fold(0.0, f64::max)
    };
    let wrapped = |spacing: f64| -> Vec<Vec<String>> {
        dims.iter()
            .map(|d| font.wrap(d, size, (spacing - 8.0).max(1.0), NAME_LINES))
            .collect()
    };
    let reach = |names: &[Vec<String>], k: usize| -> f64 {
        names[k].iter().map(|l| font.width(l, size)).fold(0.0, f64::max) / 2.0
    };
    let spacing_at = |inset: f64| ((pw - 2.0 * inset) / (n - 1) as f64).max(1.0);
    let mut inset = (edge_width(0).max(edge_width(n - 1)) / 2.0).max(8.0);
    let first_pass = wrapped(spacing_at(inset));
    inset = inset.max(reach(&first_pass, 0)).max(reach(&first_pass, n - 1));
    inset = inset.min(pw * 0.2);
    let spacing = spacing_at(inset);
    let names = wrapped(spacing);
    let name_fits: Vec<bool> = names
        .iter()
        .map(|lines| lines.iter().all(|l| font.width(l, size) <= spacing - 8.0 + 0.5))
        .collect();
    let name_h = names
        .iter()
        .zip(&name_fits)
        .filter(|(_, fits)| **fits)
        .map(|(lines, _)| lines.len())
        .max()
        .unwrap_or(1)
        .max(1) as f64
        * lh;
    let chrome = plan.title_room + PAD + name_h + 2.0 + lh + 2.0 + 2.0 + lh + PAD;
    let wanted = pw / block.style.ratio.unwrap_or(RATIO) + plan.title_room;
    let plot_h = (wanted - chrome).max(MIN_PLOT);
    plan.panel_h = chrome + plot_h;
    let frames = plan.frames(facets.len(), width);
    let rule = Style::rgba(ctx.style.ink_faint);
    let ink = ctx.style.ink;
    let ink_soft = ctx.style.ink_soft;
    let mut panels: Vec<Panel> = Vec::new();
    let mut shown = 0usize;
    for (fi, ((name, _), outer)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *outer, plan.title_room, name, reason));
            continue;
        }
        let range = &panel_ranges[fi];
        let ends = &end_texts[fi];
        let name_bottom = outer.y + plan.title_room + PAD + name_h;
        let plot = Rect::new(
            outer.x + inset,
            name_bottom + 2.0 + lh + 2.0,
            outer.w - 2.0 * inset,
            plot_h,
        );
        let step = plot.w / (n - 1) as f64;
        let axis_x = |k: usize| plot.x + k as f64 * step;
        let mut items: Vec<Item> = Vec::new();
        for (k, ((title, fits), field)) in names.iter().zip(&name_fits).zip(&dims).enumerate() {
            let x = axis_x(k);
            items.push(Item::Line {
                path: Path::polyline(&[[x, plot.y], [x, plot.bottom()]]),
                stroke: Stroke::solid(rule, GRID_WIDTH),
                role: Role::Grid,
                series: None,
                obs: None,
            });
            if *fits {
                let count = title.len();
                for (j, line) in title.iter().enumerate() {
                    let cy = name_bottom - lh / 2.0 - (count - 1 - j) as f64 * lh;
                    items.push(Item::Text(ctx.text_mid(
                        line,
                        x,
                        cy,
                        Anchor::Middle,
                        Face::Data,
                        Role::AxisTitle,
                        ink,
                    )));
                }
            } else {
                ctx.drop_label(field, DropReason::Density, "axis-title", None);
            }
            for (end, cy) in [(1, name_bottom + 2.0 + lh / 2.0), (0, plot.bottom() + 2.0 + lh / 2.0)] {
                let text = ends[k][end].clone();
                let label = ctx.text_mid(&text, x, cy, Anchor::Middle, Face::Figures, Role::AxisLabel, ink_soft);
                if label.bounds.w <= spacing - 6.0 {
                    items.push(Item::Text(label));
                } else {
                    ctx.drop_label(&text, DropReason::Density, "axis-label", Some(label.bounds));
                }
            }
        }
        let drawn = per_panel[fi].iter().filter(|l| !ctx.is_hidden(l.group)).count();
        let alpha = (3.0 / (drawn.max(1) as f64).sqrt()).clamp(0.2, 0.9);
        let mut marks: Vec<Item> = Vec::new();
        let mut observations: Vec<Observation> = Vec::new();
        for line in per_panel[fi].iter().filter(|l| !ctx.is_hidden(l.group)) {
            let g = line.group;
            let color = Style::rgba(ctx.series_color(g));
            let dash = ctx.series[g].dash.clone();
            let points: Vec<[f64; 2]> = line
                .values
                .iter()
                .enumerate()
                .map(|(k, &v)| [axis_x(k), plot.bottom() - unit(v, range[k]) * plot.h])
                .collect();
            let index = observations.len();
            marks.push(Item::Line {
                path: Path::polyline(&points),
                stroke: Stroke {
                    dash,
                    ..Stroke::solid(color.with_alpha(alpha), LINE_WIDTH)
                },
                role: Role::Mark,
                series: Some(g),
                obs: Some(index),
            });
            marks.push(Item::Point {
                center: points[0],
                radius: HANDLE,
                shape: Shape::Circle,
                fill: Some(Fill::solid(color.with_alpha(alpha.max(0.6)))),
                stroke: None,
                role: Role::Mark,
                series: Some(g),
                obs: Some(index),
            });
            let label_dim = block
                .encoding
                .label
                .as_ref()
                .and_then(|f| dims.iter().position(|d| d == f));
            let heading = label_dim
                .map(|k| {
                    format!(
                        "{} {}",
                        super::cartesian::readable_name(&dims[k]),
                        numbers::value(line.values[k], &readout)
                    )
                })
                .or_else(|| {
                    block
                        .encoding
                        .label
                        .as_ref()
                        .and_then(|f| rows::text(&ctx.rows[line.row], f))
                })
                .or_else(|| block.encoding.color.as_ref().map(|_| groups[g].clone()))
                .unwrap_or_else(|| format!("Row {}", line.row + 1));
            let mut readout_lines: Vec<ReadoutLine> = Vec::new();
            if let (true, Some(field)) = (
                heading != groups[g] && block.encoding.color != block.encoding.label,
                &block.encoding.color,
            ) {
                readout_lines.push(ReadoutLine {
                    name: super::cartesian::readable_name(field),
                    value: groups[g].clone(),
                    series: Some(g),
                    names_series: true,
                    derived: false,
                });
            }
            readout_lines.extend(
                dims.iter()
                    .zip(&line.values)
                    .filter(|(d, _)| block.encoding.label.as_ref() != Some(*d))
                    .map(|(d, &v)| ReadoutLine {
                        name: super::cartesian::readable_name(d),
                        value: numbers::value(v, &readout),
                        series: None,
                        names_series: false,
                        derived: false,
                    }),
            );
            observations.push(Observation {
                row: Some(line.row),
                dots: vec![],
                series: Some(g),
                at: points[0],
                wedge: None,
                area: None,
                radius: HANDLE + 2.0,
                x: points[0][0],
                readout: Readout {
                    heading,
                    lines: readout_lines,
                }
                .into(),
            });
            shown += 1;
        }
        items.extend(marks);
        let mut panel = Panel {
            facet: None,
            frame: *outer,
            plot,
            items,
            observations,
            targeting: Targeting::Radius,
            x: None,
            failure: None,
        };
        super::panel_title(ctx, &mut panel, name);
        panels.push(panel);
    }
    ctx.note
        .push("each axis runs from its own smallest to its own largest value".into());
    ctx.summary = format!(
        "Parallel coordinates of {} across {}: {}.",
        super::count(shown, "row", "rows"),
        super::count(n, "axis", "axes"),
        dims.join(", ")
    );
    let legend = kit::legend(ctx, Swatch::Line);
    kit::finish(&plan, facets.len(), panels, legend)
}
