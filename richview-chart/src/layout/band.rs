//! Band charts: the span between an authored lower and upper value over a calendar or numeric x
//! axis, with an optional centre line. RichView draws the bounds it is given and infers no
//! interval (FEA-CH-42, FEA-CH-70).

use super::kit::{self, ValueSide, XAxis};
use super::types::Facets;
use super::{BAND_OPACITY, Ctx, LINE_WIDTH, Painted, Style};
use crate::axis::{self, Zero};
use crate::curve;
use crate::diag::Level;
use crate::geometry::{Dot, Fill, Item, Observation, Path, Readout, ReadoutLine, Role, Stroke, Swatch};
use crate::numbers;
use crate::rows::{self, Num};
use serde_json::json;

const RATIO: f64 = 16.0 / 9.0;

/// One group's observation at one x.
struct Point {
    row: usize,
    x: f64,
    lower: f64,
    upper: f64,
    centre: Option<f64>,
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let x_field = block.encoding.x.clone().unwrap_or_default();
    let lo_field = block.encoding.lower.clone().unwrap_or_default();
    let hi_field = block.encoding.upper.clone().unwrap_or_default();
    let y_field = block.encoding.y.clone();
    let groups: Vec<String> = match &block.encoding.color {
        Some(f) => rows::distinct(ctx.rows, f),
        None => vec![block.y.label.clone().unwrap_or_else(|| hi_field.clone())],
    };
    ctx.set_series(&groups);
    let label_format = block.label_format(&block.y, ctx.locale);
    let readout = block.readout_format(&block.y, ctx.locale);
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut excluded = 0usize;
    let mut axes: Vec<Option<(Vec<Option<f64>>, XAxis)>> = Vec::new();
    let mut data: Vec<Vec<Vec<Point>>> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut per_group: Vec<Vec<Point>> = groups.iter().map(|_| Vec::new()).collect();
        match kit::read_x(ctx, members, &x_field) {
            Err(reason) => {
                failures[fi] = Some(format!("Cannot draw this band · {reason}"));
                axes.push(None);
                data.push(per_group);
                continue;
            }
            Ok((xs, axis)) => {
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
                    let Some(x) = xs[i] else {
                        excluded += 1;
                        continue;
                    };
                    let mut read = |field: &str| -> Option<Option<f64>> {
                        match rows::num(row, field) {
                            Num::Value(v) => Some(Some(v)),
                            Num::Missing => Some(None),
                            Num::Underflow(t) => {
                                ctx.underflow(i, field, &t);
                                failures[fi].get_or_insert_with(|| {
                                    format!("Cannot draw this band · {field} has a value too small to represent")
                                });
                                None
                            }
                            Num::Invalid(t) => {
                                ctx.check(
                                    "non-numeric",
                                    Level::Error,
                                    format!("Row {} has {field} '{t}', which is not a number", i + 1),
                                    Some("Write plain numbers; missing values may be empty or NA."),
                                    Some(Ctx::row_path(i, field)),
                                    None,
                                    json!({"row": i}),
                                );
                                failures[fi].get_or_insert_with(|| {
                                    format!("Cannot draw this band · {field} has text that is not a number")
                                });
                                None
                            }
                        }
                    };
                    let (Some(lower), Some(upper)) = (read(&lo_field), read(&hi_field)) else {
                        continue;
                    };
                    let centre = match &y_field {
                        Some(f) => match read(f) {
                            Some(c) => c,
                            None => continue,
                        },
                        None => None,
                    };
                    let (Some(lower), Some(upper)) = (lower, upper) else {
                        excluded += 1;
                        continue;
                    };
                    if lower > upper {
                        ctx.check(
                            "source-error",
                            Level::Error,
                            format!("Row {}: the lower bound is above the upper bound", i + 1),
                            Some("Swap the bounds or fix the data."),
                            Some(Ctx::row_path(i, &lo_field)),
                            None,
                            json!({"row": i}),
                        );
                        failures[fi].get_or_insert_with(|| {
                            "Cannot draw this band · A lower bound is above its upper bound".into()
                        });
                        continue;
                    }
                    per_group[group].push(Point {
                        row: i,
                        x,
                        lower,
                        upper,
                        centre,
                    });
                }
                for g in &mut per_group {
                    g.sort_by(|a, b| a.x.total_cmp(&b.x));
                }
                axes.push(Some((xs, axis)));
            }
        }
        data.push(per_group);
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
                "{} without an x, group or bounds {} left out",
                super::count(excluded, "row", "rows"),
                super::verb(excluded, "is", "are")
            ),
            None,
            None,
            None,
            json!({"count": excluded}),
        );
        ctx.note.push(format!(
            "{} {} not shown",
            super::count(excluded, "row", "rows"),
            super::verb(excluded, "is", "are")
        ));
    }
    let plan = kit::Plan::xy(ctx, facets.len(), RATIO);
    let frames = plan.frames(facets.len(), ctx.opts.width);
    let curve_kind = block.style.curve.unwrap_or(ctx.style.curve);
    let strength = block.style.curvature.unwrap_or(ctx.style.curvature);
    let title = block.y.label.clone().unwrap_or_else(|| hi_field.clone());
    let mut panels = Vec::new();
    for (fi, ((name, _), outer)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *outer, plan.title_room, name, reason));
            continue;
        }
        let Some((_, x_axis)) = &axes[fi] else { continue };
        let per_group = &data[fi];
        // The selected author state's exact rows set the axis even when a group is hidden (TYP-36).
        let [lo, hi] = ctx.derive_extent(fi, "band.extent", || {
            let mut lo = f64::INFINITY;
            let mut hi = f64::NEG_INFINITY;
            for points in per_group {
                for p in points {
                    lo = lo.min(p.lower).min(p.centre.unwrap_or(p.lower));
                    hi = hi.max(p.upper).max(p.centre.unwrap_or(p.upper));
                }
            }
            [lo, hi]
        });
        let data_extent = if lo.is_finite() {
            super::Fit::widen([lo, hi], ctx.wanted().y)
        } else {
            ctx.wanted().y.unwrap_or([0.0, 1.0])
        };
        ctx.fit.y = Some(data_extent);
        let side = ValueSide {
            data: data_extent,
            zero: Zero::Soft,
            title: title.clone(),
            format: &label_format,
            min: block.y.min,
            max: block.y.max,
        };
        let x_title = block.x.label.as_deref();
        let frame = kit::xy_frame(ctx, &plan, outer, x_axis, x_title, &side);
        let mut marks: Vec<Item> = Vec::new();
        for (g, points) in per_group.iter().enumerate() {
            if ctx.is_hidden(g) || points.is_empty() {
                continue;
            }
            let color = Style::rgba(ctx.series_color(g));
            let hatch = ctx.series[g].hatch;
            let surface = Style::rgba(ctx.style.surface);
            // Runs end where a spacing of more than one and a half steps leaves a gap.
            let gap = match x_axis {
                XAxis::Calendar { spacing, .. } => Some(*spacing as f64 * 1.5),
                XAxis::Numeric { .. } => None,
            };
            let mut runs: Vec<&[Point]> = Vec::new();
            let mut start = 0;
            for k in 1..=points.len() {
                if k == points.len() || gap.is_some_and(|g| points[k].x - points[k - 1].x > g) {
                    runs.push(&points[start..k]);
                    start = k;
                }
            }
            for run in runs {
                let up: Vec<[f64; 2]> = run.iter().map(|p| [frame.x(p.x), frame.y(p.upper)]).collect();
                let down: Vec<[f64; 2]> = run.iter().map(|p| [frame.x(p.x), frame.y(p.lower)]).collect();
                let mut area = Path::new();
                curve::append_run(&mut area, &up, curve_kind, strength, false);
                curve::append_run(&mut area, &curve::reversed(&down), curve_kind, strength, true);
                area.close();
                marks.push(Item::Area {
                    path: area,
                    fill: Fill {
                        color: color.with_alpha(BAND_OPACITY),
                        hatch,
                        hatch_ink: hatch.map(|_| surface),
                        hatch_scale: 1.0,
                    },
                    role: Role::Band,
                    series: Some(g),
                    obs: None,
                });
                for edge in [&up, &down] {
                    let mut line = Path::new();
                    curve::append_run(&mut line, edge, curve_kind, strength, false);
                    marks.push(Item::Line {
                        path: line,
                        stroke: Stroke::solid(color, 1.0),
                        role: Role::Mark,
                        series: Some(g),
                        obs: None,
                    });
                }
                let centre: Vec<[f64; 2]> = run
                    .iter()
                    .filter_map(|p| p.centre.map(|c| [frame.x(p.x), frame.y(c)]))
                    .collect();
                if centre.len() > 1 {
                    let mut line = Path::new();
                    curve::append_run(&mut line, &centre, curve_kind, strength, false);
                    marks.push(Item::Line {
                        path: line,
                        stroke: Stroke {
                            dash: ctx.series[g].dash.clone(),
                            ..Stroke::solid(color, LINE_WIDTH)
                        },
                        role: Role::Mark,
                        series: Some(g),
                        obs: None,
                    });
                }
            }
        }
        // One observation per x, with a dot per visible group.
        let mut xs: Vec<f64> = per_group
            .iter()
            .enumerate()
            .filter(|(g, _)| !ctx.is_hidden(*g))
            .flat_map(|(_, pts)| pts.iter().map(|p| p.x))
            .collect();
        xs.sort_by(f64::total_cmp);
        xs.dedup();
        let multi = block.encoding.color.is_some();
        let mut observations = Vec::new();
        for x in xs {
            let px = frame.x(x);
            if px < frame.plot.x - 0.5 || px > frame.plot.right() + 0.5 {
                continue;
            }
            let mut lines = Vec::new();
            let mut dots = Vec::new();
            let mut row = None;
            for (g, points) in per_group.iter().enumerate() {
                if ctx.is_hidden(g) {
                    continue;
                }
                let Some(p) = points.iter().find(|p| p.x == x) else {
                    continue;
                };
                row.get_or_insert(p.row);
                let prefix = if multi {
                    format!("{} · ", groups[g])
                } else {
                    String::new()
                };
                lines.push(ReadoutLine {
                    name: format!("{prefix}Upper"),
                    value: numbers::value(p.upper, &readout),
                    series: Some(g),
                    names_series: false,
                    derived: false,
                });
                if let (Some(c), Some(f)) = (p.centre, &y_field) {
                    lines.push(ReadoutLine {
                        name: format!("{prefix}{}", block.y.label.clone().unwrap_or_else(|| f.clone())),
                        value: numbers::value(c, &readout),
                        series: Some(g),
                        names_series: false,
                        derived: false,
                    });
                }
                lines.push(ReadoutLine {
                    name: format!("{prefix}Lower"),
                    value: numbers::value(p.lower, &readout),
                    series: Some(g),
                    names_series: false,
                    derived: false,
                });
                dots.push(Dot {
                    at: [px, frame.y(p.centre.unwrap_or((p.upper + p.lower) / 2.0))],
                    series: g,
                });
            }
            let Some(first) = dots.first().copied() else { continue };
            let heading = match x_axis {
                XAxis::Calendar { precision, .. } => axis::full_date(x as i64, *precision, ctx.calendar_locale()),
                XAxis::Numeric { .. } => numbers::value(x, &block.readout_format(&block.x, ctx.locale)),
            };
            observations.push(Observation {
                row,
                dots,
                series: Some(first.series),
                at: first.at,
                wedge: None,
                area: None,
                radius: 0.0,
                x: px,
                readout: Readout { heading, lines }.into(),
            });
        }
        panels.push(kit::xy_panel(ctx, outer, frame, marks, observations, name));
    }
    let shown: usize = data.iter().flatten().map(Vec::len).sum();
    ctx.summary = format!("Band of {shown} observations between their lower and upper bounds.");
    let legend = kit::legend(ctx, Swatch::Square);
    kit::finish(&plan, facets.len(), panels, legend)
}
