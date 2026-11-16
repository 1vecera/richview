//! Pareto charts: bars sorted from the largest value down, with a line of the running share of
//! the total on a second axis at the right that runs from 0 to 100 %. RichView sorts and adds
//! up and nothing else; a negative value fails the panel because shares of a total need parts
//! that are not negative (FEA-CH-42, FEA-CH-70).

use super::kit::{self, SecondarySide, ValueSide};
use super::pie::share_text;
use super::types::Facets;
use super::{Ctx, POINT_RADIUS, Painted, Style};
use crate::axis::Zero;
use crate::diag::Level;
use crate::geometry::{
    Anchor, DropReason, Face, Fill, Item, Observation, Path, Readout, ReadoutLine, Rect, Role, Shape, Stroke,
};
use crate::numbers::{self, Format, Kind};
use crate::rows::{self, Num};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;

const RATIO: f64 = 16.0 / 9.0;
/// The running share at which the summary says how many categories were needed.
const MOST: f64 = 0.8;

struct Bar {
    row: usize,
    name: String,
    value: f64,
}

/// One panel's ranking, from the exact values: the rows from the largest down and the shares
/// written out. Two values closer than a step of the grid must keep their order (TYP-36).
#[derive(Serialize, Deserialize)]
struct Ranked {
    rows: Vec<usize>,
    /// Each bar's share of the total and the running share after it; empty without a total.
    shares: Vec<String>,
    running: Vec<String>,
    /// How many bars it takes to reach `MOST` of the total.
    reach: Option<usize>,
}

fn rank(ctx: &Ctx, panel: usize, bars: &[Bar]) -> Ranked {
    ctx.derive(panel, "pareto.ranked", || {
        // Largest first; equal values keep their source order.
        let mut order: Vec<usize> = (0..bars.len()).collect();
        order.sort_by(|&a, &b| bars[b].value.total_cmp(&bars[a].value));
        let total: f64 = bars.iter().map(|b| b.value).sum();
        let (mut shares, mut running, mut reach) = (Vec::new(), Vec::new(), None);
        if total > 0.0 {
            let mut sum = 0.0;
            for (c, &k) in order.iter().enumerate() {
                sum += bars[k].value;
                shares.push(share_text(bars[k].value / total, ctx.locale));
                running.push(share_text(sum / total, ctx.locale));
                if reach.is_none() && sum / total >= MOST - 1e-12 {
                    reach = Some(c + 1);
                }
            }
        }
        Ranked {
            rows: order.iter().map(|&k| bars[k].row).collect(),
            shares,
            running,
            reach,
        }
    })
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let x_field = block.encoding.x.clone().unwrap_or_default();
    let y_field = block.encoding.y.clone().unwrap_or_default();
    let title = block.y.label.clone().unwrap_or_else(|| y_field.clone());
    let label_format = block.label_format(&block.y, ctx.locale);
    let readout = block.readout_format(&block.y, ctx.locale);
    // The share axis counts percent points, so its labels read 20%, 40% and not 20.0%.
    let percent = Format {
        kind: Kind::Number,
        unit: Some("%".into()),
        digits: 3,
        locale: ctx.locale,
    };
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut excluded = 0usize;
    let mut panels_bars: Vec<Vec<Bar>> = Vec::new();
    let mut ranked: Vec<Option<Ranked>> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut bars: Vec<Bar> = Vec::new();
        for &i in members {
            let row = &ctx.rows[i];
            let name = rows::text(row, &x_field).unwrap_or_else(|| format!("Row {}", i + 1));
            match rows::num(row, &y_field) {
                Num::Value(v) if v < 0.0 => {
                    ctx.check(
                        "source-error",
                        Level::Error,
                        format!(
                            "Row {} has the negative value {}; a share of a total cannot be negative",
                            i + 1,
                            numbers::value(v, &readout)
                        ),
                        Some("Pareto charts rank positive contributions; use a bar chart for signed values."),
                        Some(Ctx::row_path(i, &y_field)),
                        None,
                        json!({"row": i}),
                    );
                    failures[fi].get_or_insert_with(|| {
                        format!("Cannot draw this Pareto chart · {y_field} has a negative value")
                    });
                }
                Num::Value(value) => bars.push(Bar { row: i, name, value }),
                Num::Missing => excluded += 1,
                Num::Underflow(t) => {
                    ctx.underflow(i, &y_field, &t);
                    failures[fi].get_or_insert_with(|| {
                        format!("Cannot draw this Pareto chart · {y_field} has a value too small to represent")
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
                        format!("Cannot draw this Pareto chart · {y_field} has text that is not a number")
                    });
                }
            }
        }
        if failures[fi].is_none() && !bars.iter().map(|b| b.value).sum::<f64>().is_finite() {
            ctx.check(
                "value-overflow",
                Level::Error,
                "The total is too large to represent",
                None,
                Some("chart.encoding.y".into()),
                None,
                json!({}),
            );
            failures[fi] = Some("Cannot draw this Pareto chart · The total is too large".into());
        }
        let ranking = failures[fi].is_none().then(|| rank(ctx, fi, &bars));
        if let Some(r) = &ranking {
            let mut by_row: HashMap<usize, Bar> = bars.into_iter().map(|b| (b.row, b)).collect();
            bars = r.rows.iter().filter_map(|row| by_row.remove(row)).collect();
        }
        ranked.push(ranking);
        panels_bars.push(bars);
    }
    if failures.iter().all(Option::is_some) {
        let reason = failures.iter().flatten().next().cloned().unwrap_or_default();
        return kit::failed(ctx, &reason, RATIO);
    }
    if panels_bars
        .iter()
        .zip(&failures)
        .all(|(b, f)| f.is_some() || b.is_empty())
    {
        return kit::failed(ctx, "No values to rank", RATIO);
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
            Some("Give every category a value."),
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
    let cumulative = "Cumulative share".to_owned();
    ctx.set_series(&[title.clone(), cumulative.clone()]);
    // Each panel sorts its own categories, so each has its own category plan; the tallest sets
    // the frames.
    let ratio = block.style.ratio.unwrap_or(RATIO);
    let plans: Vec<kit::Plan> = panels_bars
        .iter()
        .map(|bars| {
            let names: Vec<String> = bars.iter().map(|b| b.name.clone()).collect();
            kit::plan_secondary(ctx, facets.len(), &names, 1.0, ratio)
        })
        .collect();
    let main = plans
        .iter()
        .max_by(|a, b| a.panel_h.total_cmp(&b.panel_h))
        .expect("one plan per panel");
    let frames = main.frames(facets.len(), ctx.opts.width);
    let drawn: Vec<usize> = (0..facets.len()).filter(|&i| failures[i].is_none()).collect();
    let extent = |idx: &[usize]| {
        idx.iter()
            .flat_map(|&i| &panels_bars[i])
            .fold([f64::INFINITY, f64::NEG_INFINITY], |[lo, hi], b| {
                [lo.min(b.value), hi.max(b.value)]
            })
    };
    let shared = ctx.derive_extent(0, "pareto.shared", || extent(&drawn));
    let surface = Style::rgba(ctx.style.surface);
    let mut panels = Vec::new();
    let mut summary_bars: Option<usize> = None;
    for (fi, ((name, _), outer)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *outer, main.title_room, name, reason));
            continue;
        }
        let bars = &panels_bars[fi];
        let Some(ranking) = &ranked[fi] else { continue };
        if bars.is_empty() {
            panels.push(super::failed_facet(
                ctx,
                *outer,
                main.title_room,
                name,
                "No values to rank",
            ));
            continue;
        }
        let plan = &plans[fi];
        let mut data = if block.style.free_scales {
            ctx.derive_extent(fi, "pareto.own", || extent(&[fi]))
        } else {
            shared
        };
        data = super::Fit::widen(data, ctx.wanted().y);
        ctx.fit.y = Some(data);
        // Room above the tallest bar for its value, unless the author fixed the top.
        if block.y.max.is_none() && data[1] > 0.0 {
            data[1] *= 1.08;
        }
        let side = ValueSide {
            data,
            zero: Zero::Include,
            title: title.clone(),
            format: &label_format,
            min: block.y.min,
            max: block.y.max,
        };
        let second = SecondarySide {
            data: [0.0, 100.0],
            zero: Zero::Include,
            title: cumulative.clone(),
            format: &percent,
            min: Some(0.0),
            max: Some(100.0),
        };
        let frame = kit::frame_with(ctx, plan, outer, &side, Some(&second), fi == 0);
        let t = frame.thickness(1, 56.0);
        let total: f64 = bars.iter().map(|b| b.value).sum();
        let bar_color = Style::rgba(ctx.series_color(0));
        let hatch = ctx.series[0].hatch;
        let line_color = Style::rgba(ctx.series_color(1));
        let mut marks: Vec<Item> = Vec::new();
        let mut points: Vec<Item> = Vec::new();
        let mut path_points: Vec<[f64; 2]> = Vec::new();
        let mut observations = Vec::new();
        let mut running = 0.0f64;
        summary_bars = summary_bars.or(ranking.reach);
        for (c, b) in bars.iter().enumerate() {
            running += b.value;
            let c0 = frame.slot_start(c, 0, 1, t);
            let cm = c0 + t / 2.0;
            let rect = frame.span(c0, c0 + t, 0.0, b.value);
            let rect = if frame.horizontal {
                Rect::new(rect.x, rect.y, rect.w.max(1.0), rect.h)
            } else {
                Rect::new(rect.x, rect.y, rect.w, rect.h.max(1.0))
            };
            let r = ctx.style.bar_radius.min(rect.w / 2.0).min(rect.h / 2.0);
            marks.push(Item::Rect {
                rect,
                fill: Fill {
                    color: bar_color,
                    hatch,
                    hatch_ink: hatch.map(|_| surface),
                    hatch_scale: 1.0,
                },
                stroke: None,
                radius: [r; 4],
                role: Role::Mark,
                series: Some(0),
                obs: Some(observations.len()),
            });
            // The bar's value beyond its end; the author's `auto_annotations: false` draws none
            // and records no drop.
            if ctx.block.auto_annotations {
                let label = numbers::value(b.value, &label_format);
                let text = if frame.horizontal {
                    ctx.text_mid(
                        &label,
                        rect.right() + 4.0,
                        cm,
                        Anchor::Start,
                        Face::Figures,
                        Role::Label,
                        ctx.style.ink,
                    )
                } else {
                    ctx.text(
                        &label,
                        cm,
                        rect.y - 4.0,
                        Anchor::Middle,
                        Face::Figures,
                        Role::Label,
                        ctx.style.ink,
                    )
                };
                let fits_band = if frame.horizontal {
                    text.bounds.h <= frame.band
                } else {
                    text.bounds.w <= frame.band
                };
                if fits_band && text.bounds.inside(&frame.plot, 0.5) {
                    marks.push(Item::Text(text));
                } else {
                    let reason = if fits_band {
                        DropReason::OutOfBounds
                    } else {
                        DropReason::Density
                    };
                    ctx.drop_label(&label, reason, "value", Some(text.bounds));
                }
            }
            let mut lines = vec![ReadoutLine {
                name: title.clone(),
                value: numbers::value(b.value, &readout),
                series: Some(0),
                names_series: false,
                derived: false,
            }];
            if total > 0.0 {
                let share = running / total;
                let center = frame.pt2(cm, share * 100.0);
                path_points.push(center);
                points.push(Item::Point {
                    center,
                    radius: POINT_RADIUS,
                    shape: ctx.series[1].shape.unwrap_or(Shape::Circle),
                    fill: Some(Fill::solid(line_color)),
                    stroke: Some(Stroke::solid(surface, 1.5)),
                    role: Role::Mark,
                    series: Some(1),
                    obs: Some(observations.len()),
                });
                lines.push(ReadoutLine {
                    name: "Share".into(),
                    value: ranking.shares.get(c).cloned().unwrap_or_default(),
                    series: Some(0),
                    names_series: false,
                    derived: true,
                });
                lines.push(ReadoutLine {
                    name: cumulative.clone(),
                    value: ranking.running.get(c).cloned().unwrap_or_default(),
                    series: Some(1),
                    names_series: false,
                    derived: true,
                });
            }
            // The whole column answers the pointer, so a short bar and its line point are both reachable.
            let column = if frame.horizontal {
                Rect::new(frame.plot.x, cm - frame.band / 2.0, frame.plot.w, frame.band)
            } else {
                Rect::new(cm - frame.band / 2.0, frame.plot.y, frame.band, frame.plot.h)
            };
            observations.push(Observation {
                row: Some(b.row),
                dots: vec![],
                series: Some(0),
                at: frame.pt(cm, b.value),
                wedge: None,
                area: Some(column),
                radius: 0.0,
                x: cm,
                readout: Readout {
                    heading: b.name.clone(),
                    lines,
                }
                .into(),
            });
        }
        if path_points.len() > 1 {
            let mut stroke = Stroke::solid(line_color, super::LINE_WIDTH);
            stroke.dash = ctx.series[1].dash.clone();
            marks.push(Item::Line {
                path: Path::polyline(&path_points),
                stroke,
                role: Role::Mark,
                series: Some(1),
                obs: None,
            });
        }
        marks.extend(points);
        panels.push(kit::panel(ctx, outer, frame, marks, observations, name));
    }
    let first = drawn
        .iter()
        .find_map(|&i| panels_bars[i].first().map(|b| (i, b)))
        .expect("a drawn panel has bars");
    let count: usize = drawn.iter().map(|&i| panels_bars[i].len()).sum();
    let first_share = ranked[first.0].as_ref().and_then(|r| r.shares.first());
    ctx.summary = format!(
        "{}; the largest is {}",
        super::count(count, "category", "categories"),
        first.1.name
    );
    if let Some(share) = first_share {
        ctx.summary.push_str(&format!(" at {share} of the total"));
        if let Some(k) = summary_bars {
            ctx.summary
                .push_str(&format!(", and the first {k} reach {}", share_text(MOST, ctx.locale)));
        }
    }
    ctx.summary.push('.');
    ctx.note.push(
        "bars are sorted from the largest down; the line is the running share of the total, on the axis at the right"
            .into(),
    );
    kit::finish(main, facets.len(), panels, None)
}
