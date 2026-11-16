//! Dumbbell charts: a line between two values per category and group, a hollow dot at the start
//! and a filled dot at the end. Which end is larger is meaningful, so the change is signed
//! (FEA-CH-75). The value axis does not need zero.

use super::kit::{self, ValueSide};
use super::types::Facets;
use super::{Ctx, Painted, Style};
use crate::axis::Zero;
use crate::diag::Level;
use crate::geometry::{
    Anchor, Face, Fill, Item, Observation, Path, Readout, ReadoutLine, Rect, Role, Shape, Stroke, Swatch,
};
use crate::numbers;
use crate::rows::{self, Num};
use serde_json::json;

const RATIO: f64 = 16.0 / 9.0;
const RADIUS: f64 = 5.0;
const LINE: f64 = 2.0;

/// One dumbbell: the row it reads and its two values.
struct Bell {
    row: usize,
    category: usize,
    group: usize,
    lower: f64,
    upper: f64,
}

fn signed(v: f64, format: &numbers::Format) -> String {
    let text = numbers::value(v, format);
    if v > 0.0 { format!("+{text}") } else { text }
}

/// `extent` of `bells` from the exact rows; empty if there are none.
fn derived_extent(
    ctx: &Ctx,
    panel: usize,
    name: &str,
    bells: &[&Bell],
    extent: impl Fn(&[&Bell]) -> [f64; 2],
) -> [f64; 2] {
    let found: Option<[f64; 2]> = ctx.derive(panel, &format!("dumbbell.{name}"), || {
        let e = crate::derived::coarse(extent(bells));
        (e[0].is_finite() && e[1].is_finite()).then_some(e)
    });
    found.unwrap_or([f64::INFINITY, f64::NEG_INFINITY])
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let x_field = block.encoding.x.clone().unwrap_or_default();
    let lo_field = block.encoding.lower.clone().unwrap_or_default();
    let hi_field = block.encoding.upper.clone().unwrap_or_default();
    let categories = rows::distinct(ctx.rows, &x_field);
    let multi = block.encoding.color.is_some();
    let title = block
        .y
        .label
        .clone()
        .unwrap_or_else(|| format!("{lo_field} to {hi_field}"));
    let groups: Vec<String> = match &block.encoding.color {
        Some(f) => rows::distinct(ctx.rows, f),
        None => vec![title.clone()],
    };
    ctx.set_series(&groups);
    let label_format = block.label_format(&block.y, ctx.locale);
    let readout = block.readout_format(&block.y, ctx.locale);
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut excluded = 0usize;
    let mut panels_bells: Vec<Vec<Bell>> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut bells: Vec<Bell> = Vec::new();
        for &i in members {
            let row = &ctx.rows[i];
            let Some(category) = rows::text(row, &x_field).and_then(|c| categories.iter().position(|x| *x == c)) else {
                excluded += 1;
                continue;
            };
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
            let mut values = [0.0f64; 2];
            let mut missing = false;
            let mut failed = false;
            for (k, field) in [&lo_field, &hi_field].into_iter().enumerate() {
                match rows::num(row, field) {
                    Num::Value(v) => values[k] = v,
                    Num::Missing => missing = true,
                    Num::Underflow(t) => {
                        ctx.underflow(i, field, &t);
                        failures[fi].get_or_insert_with(|| {
                            format!("Cannot draw this dumbbell chart · {field} has a value too small to represent")
                        });
                        failed = true;
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
                            format!("Cannot draw this dumbbell chart · {field} has text that is not a number")
                        });
                        failed = true;
                    }
                }
            }
            if failed {
                continue;
            }
            if missing {
                excluded += 1;
                continue;
            }
            if bells.iter().any(|b| b.category == category && b.group == group) {
                ctx.check(
                    "source-error",
                    Level::Error,
                    format!(
                        "Row {} repeats '{}'{}; a dumbbell needs one row for each",
                        i + 1,
                        categories[category],
                        if multi {
                            format!(" in '{}'", groups[group])
                        } else {
                            String::new()
                        }
                    ),
                    Some("Give each category and group one row."),
                    Some("chart.encoding.x".into()),
                    None,
                    json!({"row": i}),
                );
                failures[fi].get_or_insert_with(|| "Cannot draw this dumbbell chart · Repeated categories".into());
                continue;
            }
            bells.push(Bell {
                row: i,
                category,
                group,
                lower: values[0],
                upper: values[1],
            });
        }
        panels_bells.push(bells);
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
                "{} without a category, group or both values {} left out",
                super::count(excluded, "row", "rows"),
                super::verb(excluded, "is", "are")
            ),
            Some("Give every row a category and both values."),
            Some("chart.encoding.lower".into()),
            None,
            json!({"count": excluded}),
        );
        ctx.note.push(format!(
            "{} without both values {} not shown",
            super::count(excluded, "row", "rows"),
            super::verb(excluded, "is", "are")
        ));
    }
    let visible: Vec<usize> = (0..groups.len()).filter(|g| !ctx.is_hidden(*g)).collect();
    let extent = |bells: &[&Bell]| {
        bells.iter().fold([f64::INFINITY, f64::NEG_INFINITY], |[lo, hi], b| {
            [lo.min(b.lower).min(b.upper), hi.max(b.lower).max(b.upper)]
        })
    };
    let all: Vec<&Bell> = panels_bells
        .iter()
        .zip(&failures)
        .filter(|(_, f)| f.is_none())
        .flat_map(|(b, _)| b)
        .collect();
    // The value extent sets the axis and the change is written in the readout, so the exact rows
    // decide them (TYP-36); reader interactions retain the embedded extent.
    let shared = derived_extent(ctx, 0, "shared", &all, extent);
    let changes: Vec<Vec<String>> = panels_bells
        .iter()
        .enumerate()
        .map(|(fi, bells)| {
            ctx.derive(fi, "dumbbell.changes", || {
                bells.iter().map(|b| signed(b.upper - b.lower, &readout)).collect()
            })
        })
        .collect();
    let plan = kit::plan(
        ctx,
        facets.len(),
        &categories,
        if multi { groups.len() as f64 * 0.7 } else { 1.0 },
        block.style.ratio.unwrap_or(RATIO),
    );
    // Few enough rows print both end values beyond their dots (FEA-CH-28), so the value axis
    // keeps `room` pixels for one beyond its first and last dot.
    let most = panels_bells
        .iter()
        .map(|bells| bells.iter().filter(|b| visible.contains(&b.group)).count())
        .max()
        .unwrap_or(0);
    let frames = plan.frames(facets.len(), ctx.opts.width);
    let budget = if frames.first().is_some_and(|f| f.w - 30.0 < 480.0) {
        3
    } else {
        5
    };
    let mut room = 0.0;
    if block.auto_annotations && (1..=budget).contains(&most) {
        let text = if plan.horizontal {
            all.iter()
                .flat_map(|b| [b.lower, b.upper])
                .map(|v| ctx.text_width(&numbers::value(v, &readout)))
                .fold(0.0f64, f64::max)
        } else {
            ctx.line_height()
        };
        room = RADIUS + 6.0 + text;
    }
    let mut panels = Vec::new();
    for (fi, ((name, _), outer)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *outer, plan.title_room, name, reason));
            continue;
        }
        let bells = &panels_bells[fi];
        let mine: Vec<&Bell> = bells.iter().collect();
        let mut data = if block.style.free_scales {
            derived_extent(ctx, fi, "own", &mine, extent)
        } else {
            shared
        };
        if !data[0].is_finite() {
            data = ctx.wanted().y.unwrap_or([0.0, 1.0]);
        } else {
            if room > 0.0 && data[1] > data[0] {
                // The extent grows by the room in the axis's own units, judged against a plot of
                // seven tenths of the panel, and never across zero (FEA-CH-19).
                let length = 0.7 * if plan.horizontal { outer.w } else { outer.h };
                let pad = room * (data[1] - data[0]) / (length - 2.0 * room).max(room);
                let low = if data[0] >= 0.0 {
                    (data[0] - pad).max(0.0)
                } else {
                    data[0] - pad
                };
                let high = if data[1] <= 0.0 {
                    (data[1] + pad).min(0.0)
                } else {
                    data[1] + pad
                };
                data = [low, high];
            }
            data = super::Fit::widen(data, ctx.wanted().y);
        }
        ctx.fit.y = Some(data);
        let side = ValueSide {
            data,
            zero: Zero::None,
            title: title.clone(),
            format: &label_format,
            min: block.y.min,
            max: block.y.max,
        };
        let frame = kit::frame(ctx, &plan, outer, &side, fi == 0);
        let t = frame.thickness(visible.len(), 40.0);
        let line_width = if multi && visible.len() > 1 { LINE.min(t) } else { LINE };
        let mut marks: Vec<Item> = Vec::new();
        let mut observations = Vec::new();
        // Both end values of each row, outside its dots (FEA-CH-28, chart design 16 Nov 2026).
        let mut wanted: Vec<super::place::SparseValue> = Vec::new();
        let lh = ctx.line_height();
        for (bi, b) in bells.iter().enumerate() {
            let Some(slot) = visible.iter().position(|g| *g == b.group) else {
                continue;
            };
            let c0 = frame.slot_start(b.category, slot, visible.len(), t);
            let cm = c0 + t / 2.0;
            let r = RADIUS.min(t / 2.0).max(2.5);
            let color = Style::rgba(ctx.series_color(b.group));
            let shape = ctx.series[b.group].shape.unwrap_or(Shape::Circle);
            let from = frame.pt(cm, b.lower);
            let to = frame.pt(cm, b.upper);
            marks.push(Item::Line {
                path: Path::polyline(&[from, to]),
                stroke: Stroke::solid(color, line_width),
                role: Role::Mark,
                series: Some(b.group),
                obs: None,
            });
            // Hollow at the start, filled at the end, so direction reads without colour.
            marks.push(Item::Point {
                center: from,
                radius: r,
                shape,
                fill: Some(Fill::solid(Style::rgba(ctx.style.surface))),
                stroke: Some(Stroke::solid(color, LINE)),
                role: Role::Mark,
                series: Some(b.group),
                obs: Some(observations.len()),
            });
            marks.push(Item::Point {
                center: to,
                radius: r,
                shape,
                fill: Some(Fill::solid(color)),
                stroke: None,
                role: Role::Mark,
                series: Some(b.group),
                obs: Some(observations.len()),
            });
            let (a, z) = (frame.at(b.lower), frame.at(b.upper));
            let reach = (a.min(z) - r, a.max(z) + r);
            let area = if frame.horizontal {
                Rect::new(reach.0, cm - r, (reach.1 - reach.0).max(1.0), 2.0 * r)
            } else {
                Rect::new(cm - r, reach.0, 2.0 * r, (reach.1 - reach.0).max(1.0))
            };
            let prefix = if multi {
                format!("{} · ", groups[b.group])
            } else {
                String::new()
            };
            // The lesser value before its dot and the greater after it; equal values print once.
            let gap = r + 4.0;
            let ends = if b.lower == b.upper {
                vec![(b.upper, true)]
            } else {
                vec![(b.lower.min(b.upper), false), (b.lower.max(b.upper), true)]
            };
            for (v, after) in ends {
                let text = numbers::value(v, &readout);
                let end = frame.at(v);
                let place = if frame.horizontal {
                    let (x, anchor) = if after {
                        (end + gap, Anchor::Start)
                    } else {
                        (end - gap, Anchor::End)
                    };
                    ctx.text_mid(&text, x, cm, anchor, Face::Figures, Role::Label, ctx.style.ink)
                } else {
                    let y = if after {
                        end - gap - lh / 2.0
                    } else {
                        end + gap + lh / 2.0
                    };
                    ctx.text_mid(&text, cm, y, Anchor::Middle, Face::Figures, Role::Label, ctx.style.ink)
                };
                wanted.push(super::place::SparseValue {
                    text,
                    series: b.group,
                    observation: observations.len(),
                    places: vec![place],
                });
            }
            observations.push(Observation {
                row: Some(b.row),
                dots: vec![],
                series: Some(b.group),
                at: to,
                wedge: None,
                area: Some(area),
                radius: r,
                x: cm,
                readout: Readout {
                    heading: categories[b.category].clone(),
                    lines: vec![
                        ReadoutLine {
                            name: format!("{prefix}Lower"),
                            value: numbers::value(b.lower, &readout),
                            series: Some(b.group),
                            names_series: false,
                            derived: false,
                        },
                        ReadoutLine {
                            name: format!("{prefix}Upper"),
                            value: numbers::value(b.upper, &readout),
                            series: Some(b.group),
                            names_series: false,
                            derived: false,
                        },
                        ReadoutLine {
                            name: format!("{prefix}Change"),
                            value: changes[fi][bi].clone(),
                            series: Some(b.group),
                            names_series: false,
                            derived: true,
                        },
                    ],
                }
                .into(),
            });
        }
        let rows = observations.len();
        let mut panel = kit::panel(ctx, outer, frame, marks, observations, name);
        super::place::sparse_values(ctx, fi, &mut panel, rows, wanted, &|_| true);
        panels.push(panel);
    }
    ctx.summary = format!(
        "{} {} from {lo_field} to {hi_field} across {} categories.",
        all.len(),
        if all.len() == 1 { "change" } else { "changes" },
        categories.len()
    );
    ctx.note
        .push("hollow dot = start, filled dot = end; the change is the end minus the start".into());
    let legend = kit::legend(ctx, Swatch::Dot);
    kit::finish(&plan, facets.len(), panels, legend)
}
