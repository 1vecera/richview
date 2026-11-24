//! Lollipop charts: one stem and a dot per category and group, from the zero baseline to the
//! value. They say what a bar chart says with less ink (FEA-CH-75). Groups sit side by side like
//! grouped bars; negative values hang below the baseline.

use super::kit::{self, ValueSide};
use super::types::Facets;
use super::{Ctx, Painted, Style};
use crate::axis::Zero;
use crate::diag::Level;
use crate::geometry::{
    Anchor, DropReason, Face, Fill, Item, Observation, Path, Readout, ReadoutLine, Rect, Role, Shape, Stroke, Swatch,
};
use crate::numbers;
use crate::rows::{self, Num};
use serde_json::json;

const RATIO: f64 = 16.0 / 9.0;
const RADIUS: f64 = 5.0;
const STEM: f64 = 2.0;

/// One lollipop: where it stands and the row it reads.
struct Pop {
    row: usize,
    category: usize,
    group: usize,
    value: f64,
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let x_field = block.encoding.x.clone().unwrap_or_default();
    let y_field = block.encoding.y.clone().unwrap_or_default();
    let categories = rows::distinct(ctx.rows, &x_field);
    let multi = block.encoding.color.is_some();
    let title = block.y.label.clone().unwrap_or_else(|| y_field.clone());
    let groups: Vec<String> = match &block.encoding.color {
        Some(f) => rows::distinct(ctx.rows, f),
        None => vec![title.clone()],
    };
    ctx.set_series(&groups);
    let label_format = block.label_format(&block.y, ctx.locale);
    let readout = block.readout_format(&block.y, ctx.locale);
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut excluded = 0usize;
    let mut panels_pops: Vec<Vec<Pop>> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut pops: Vec<Pop> = Vec::new();
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
            let value = match rows::num(row, &y_field) {
                Num::Value(v) => v,
                Num::Missing => {
                    excluded += 1;
                    continue;
                }
                Num::Underflow(t) => {
                    ctx.underflow(i, &y_field, &t);
                    failures[fi].get_or_insert_with(|| {
                        format!("Cannot draw this lollipop chart · {y_field} has a value too small to represent")
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
                        format!("Cannot draw this lollipop chart · {y_field} has text that is not a number")
                    });
                    continue;
                }
            };
            if pops.iter().any(|p| p.category == category && p.group == group) {
                ctx.check(
                    "source-error",
                    Level::Error,
                    format!(
                        "Row {} repeats '{}'{}; a lollipop needs one row for each",
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
                failures[fi].get_or_insert_with(|| "Cannot draw this lollipop chart · Repeated categories".into());
                continue;
            }
            pops.push(Pop {
                row: i,
                category,
                group,
                value,
            });
        }
        panels_pops.push(pops);
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
                "{} without a category, group or value {} left out",
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
    let visible: Vec<usize> = (0..groups.len()).filter(|g| !ctx.is_hidden(*g)).collect();
    let extent = |pops: &[&Pop]| {
        pops.iter().fold([f64::INFINITY, f64::NEG_INFINITY], |[lo, hi], p| {
            [lo.min(p.value), hi.max(p.value)]
        })
    };
    let all: Vec<&Pop> = panels_pops
        .iter()
        .zip(&failures)
        .filter(|(_, f)| f.is_none())
        .flat_map(|(p, _)| p)
        .collect();
    let drawn: Vec<(usize, usize)> = all.iter().map(|p| (p.row, p.group)).collect();
    ctx.resolve_emphasis(&drawn);
    // The selected author state's exact rows set the axis even when a group is hidden (TYP-36).
    let shared = ctx.derive_extent(0, "lollipop.shared", || extent(&all));
    let plan = kit::plan(
        ctx,
        facets.len(),
        &categories,
        if multi { groups.len() as f64 * 0.7 } else { 1.0 },
        block.style.ratio.unwrap_or(RATIO),
    );
    let frames = plan.frames(facets.len(), ctx.opts.width);
    let lh = ctx.line_height();
    let mut panels = Vec::new();
    for (fi, ((name, _), outer)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *outer, plan.title_room, name, reason));
            continue;
        }
        let pops = &panels_pops[fi];
        let mine: Vec<&Pop> = pops.iter().collect();
        let mut data = if block.style.free_scales {
            ctx.derive_extent(fi, "lollipop.own", || extent(&mine))
        } else {
            shared
        };
        if !data[0].is_finite() {
            data = ctx.wanted().y.unwrap_or([0.0, 1.0]);
        } else {
            data = super::Fit::widen(data, ctx.wanted().y);
        }
        ctx.fit.y = Some(data);
        let side = ValueSide {
            data,
            zero: Zero::Include,
            title: title.clone(),
            format: &label_format,
            min: block.y.min,
            max: block.y.max,
        };
        let frame = kit::frame(ctx, &plan, outer, &side, fi == 0);
        let t = frame.thickness(visible.len(), 40.0);
        let stem_width = if multi && visible.len() > 1 { STEM.min(t) } else { STEM };
        let mut marks: Vec<Item> = Vec::new();
        let mut observations = Vec::new();
        for p in pops {
            let Some(slot) = visible.iter().position(|g| *g == p.group) else {
                continue;
            };
            let c0 = frame.slot_start(p.category, slot, visible.len(), t);
            let cm = c0 + t / 2.0;
            let r = RADIUS.min(t / 2.0).max(2.5);
            let base = frame.at(0.0);
            let end = frame.at(p.value);
            let color = Style::rgba(ctx.mark_color(p.row, p.group, ctx.series_color(p.group)));
            marks.push(Item::Line {
                path: Path::polyline(&[frame.pt(cm, 0.0), frame.pt(cm, p.value)]),
                stroke: Stroke::solid(color, stem_width),
                role: Role::Mark,
                series: Some(p.group),
                obs: None,
            });
            let center = frame.pt(cm, p.value);
            marks.push(Item::Point {
                center,
                radius: r,
                shape: ctx.series[p.group].shape.unwrap_or(Shape::Circle),
                fill: Some(Fill::solid(color)),
                stroke: None,
                role: Role::Mark,
                series: Some(p.group),
                obs: Some(observations.len()),
            });
            // Stem and dot together, with the dot's reach beyond the value.
            let up = p.value >= 0.0;
            let reach = if frame.horizontal {
                if up { end + r } else { end - r }
            } else if up {
                end - r
            } else {
                end + r
            };
            let area = if frame.horizontal {
                Rect::new(base.min(reach), cm - r, (base - reach).abs().max(1.0), 2.0 * r)
            } else {
                Rect::new(cm - r, base.min(reach), 2.0 * r, (base - reach).abs().max(1.0))
            };
            // The value beyond the dot when one lollipop owns its band; at the plot's edge it
            // moves beside the dot, and it is dropped when neither place fits. The author's
            // `auto_annotations: false` draws none and records no drop.
            if !multi && ctx.block.auto_annotations && !ctx.mutes_mark(p.row, p.group) {
                let text = numbers::value(p.value, &label_format);
                let gap = r + 4.0;
                let beyond = if frame.horizontal {
                    let (x, anchor) = if up {
                        (end + gap, Anchor::Start)
                    } else {
                        (end - gap, Anchor::End)
                    };
                    ctx.text_mid(&text, x, cm, anchor, Face::Figures, Role::Label, ctx.style.ink)
                } else {
                    let y = if up { end - gap } else { end + gap + lh * 0.8 };
                    ctx.text(&text, cm, y, Anchor::Middle, Face::Figures, Role::Label, ctx.style.ink)
                };
                let beside = if frame.horizontal {
                    let y = if up { cm - gap - lh * 0.2 } else { cm + gap + lh * 0.8 };
                    {
                        let (x, anchor) = if up {
                            (end + r, Anchor::End)
                        } else {
                            (end - r, Anchor::Start)
                        };
                        ctx.text(&text, x, y, anchor, Face::Figures, Role::Label, ctx.style.ink)
                    }
                } else {
                    let mut t = ctx.text_mid(
                        &text,
                        cm + gap,
                        end,
                        Anchor::Start,
                        Face::Figures,
                        Role::Label,
                        ctx.style.ink,
                    );
                    // A dot a few pixels from the axis' end would push the value centred on it
                    // past the plot: hold it inside, as long as it still stands level with the dot.
                    let shift = if t.bounds.bottom() > frame.plot.bottom() {
                        frame.plot.bottom() - t.bounds.bottom()
                    } else if t.bounds.y < frame.plot.y {
                        frame.plot.y - t.bounds.y
                    } else {
                        0.0
                    };
                    if shift.abs() <= t.bounds.h / 2.0 + r {
                        t.y += shift;
                        t.bounds.y += shift;
                    }
                    t
                };
                let fits = |t: &crate::geometry::Text| {
                    let in_band = if frame.horizontal {
                        t.bounds.h <= frame.band
                    } else {
                        t.bounds.w <= frame.band
                    };
                    in_band && t.bounds.inside(&frame.plot, 0.5)
                };
                if fits(&beyond) {
                    marks.push(Item::Text(beyond));
                } else if fits(&beside) {
                    marks.push(Item::Text(beside));
                } else {
                    ctx.drop_label(&text, DropReason::OutOfBounds, "value", Some(beyond.bounds));
                }
            }
            observations.push(Observation {
                row: Some(p.row),
                dots: vec![],
                series: Some(p.group),
                at: center,
                wedge: None,
                area: Some(area),
                radius: r,
                x: cm,
                readout: Readout {
                    heading: categories[p.category].clone(),
                    lines: vec![ReadoutLine {
                        label: None,
                        name: if multi { groups[p.group].clone() } else { title.clone() },
                        value: numbers::value(p.value, &readout),
                        series: Some(p.group),
                        names_series: multi,
                        derived: false,
                    }],
                }
                .into(),
            });
        }
        panels.push(kit::panel(ctx, outer, frame, marks, observations, name));
    }
    ctx.summary = format!(
        "{} {} across {} categories.",
        all.len(),
        if all.len() == 1 { "value" } else { "values" },
        categories.len()
    );
    let legend = kit::legend(ctx, Swatch::Dot);
    kit::finish(&plan, facets.len(), panels, legend)
}
