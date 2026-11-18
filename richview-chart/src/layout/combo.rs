//! Combo charts: bars on the axis at the left and a line with a point per category on a second
//! axis at the right, each with its own scale and labels. The line breaks where a value is
//! missing unless `style.gaps` is `bridge`; nothing is interpolated or invented (FEA-CH-38,
//! FEA-CH-42). Values are read from the axes and the readouts; two sets of value labels on one
//! plot would crowd it.

use super::kit::{self, SecondarySide, ValueSide};
use super::types::Facets;
use super::{Ctx, LINE_WIDTH, POINT_RADIUS, Painted, Style};
use crate::axis::Zero;
use crate::block::Axis;
use crate::diag::Level;
use crate::geometry::{
    Anchor, Face, Fill, Item, Observation, Path, Readout, ReadoutLine, Rect, Role, Shape, Stroke, Swatch, Text,
};
use crate::numbers::{self, Kind, Locale};
use crate::rows::{self, Num};
use serde_json::{Value, json};

const RATIO: f64 = 16.0 / 9.0;

/// One row: where it stands and what it says; either value may be missing, not both.
struct Entry {
    row: usize,
    category: usize,
    group: usize,
    bar: Option<f64>,
    line: Option<f64>,
}

/// The second axis as the author set it under `axes.y2`.
fn second_axis(block: &crate::block::ChartBlock) -> Axis {
    let v = block
        .raw
        .get("axes")
        .and_then(|a| a.get("y2"))
        .cloned()
        .unwrap_or(Value::Null);
    let text = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_owned);
    let number = |k: &str| v.get(k).and_then(Value::as_f64).filter(|n| n.is_finite());
    Axis {
        label: text("label"),
        format: v.get("format").and_then(Value::as_str).and_then(Kind::parse),
        unit: text("unit"),
        digits: v
            .get("digits")
            .and_then(Value::as_u64)
            .map(|d| d.clamp(1, numbers::MAX_DIGITS as u64) as u8),
        locale: text("locale").map(|l| Locale::from_tag(&l)),
        min: number("min"),
        max: number("max"),
    }
}

/// An extent with nothing in it, as `extents` starts.
const NO_EXTENT: [f64; 2] = [f64::INFINITY, f64::NEG_INFINITY];

/// `extent` if it holds a value, for a record that cannot carry infinity.
fn finite_extent(extent: [f64; 2]) -> Option<[f64; 2]> {
    (extent[0].is_finite() && extent[1].is_finite()).then_some(extent)
}

/// The bar and line extents of the selected author state, retained across reader interactions.
fn derived_extents(
    ctx: &Ctx,
    panel: usize,
    name: &str,
    entries: &[&Entry],
    extents: impl Fn(&[&Entry]) -> ([f64; 2], [f64; 2]),
) -> ([f64; 2], [f64; 2]) {
    let (bars, lines) = ctx.derive(panel, &format!("combo.{name}"), || {
        let (bars, lines) = extents(entries);
        (finite_extent(bars), finite_extent(lines))
    });
    (bars.unwrap_or(NO_EXTENT), lines.unwrap_or(NO_EXTENT))
}

enum Read {
    Value(f64),
    Missing,
    Failed,
}

/// One numeric field of one row; a value that is not a number or too small fails the panel.
fn read(ctx: &mut Ctx, i: usize, field: &str, failure: &mut Option<String>) -> Read {
    match rows::num(&ctx.rows[i], field) {
        Num::Value(v) => Read::Value(v),
        Num::Missing => Read::Missing,
        Num::Underflow(t) => {
            ctx.underflow(i, field, &t);
            failure.get_or_insert_with(|| {
                format!("Cannot draw this combo chart · {field} has a value too small to represent")
            });
            Read::Failed
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
            failure
                .get_or_insert_with(|| format!("Cannot draw this combo chart · {field} has text that is not a number"));
            Read::Failed
        }
    }
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let x_field = block.encoding.x.clone().unwrap_or_default();
    let y_field = block.encoding.y.clone().unwrap_or_default();
    let Some(line_field) = block.channel("line") else {
        ctx.check(
            "source-error",
            Level::Error,
            "A combo chart needs a line channel: the field drawn as the line",
            Some("Add encoding.line with the name of a numeric field."),
            Some("chart.encoding.line".into()),
            None,
            json!({}),
        );
        return kit::failed(ctx, "Cannot draw this combo chart · encoding.line is missing", RATIO);
    };
    let y2 = second_axis(block);
    let categories = rows::distinct(ctx.rows, &x_field);
    let multi = block.encoding.color.is_some();
    let bar_title = block.y.label.clone().unwrap_or_else(|| y_field.clone());
    let line_title = y2.label.clone().unwrap_or_else(|| line_field.clone());
    let groups: Vec<String> = match &block.encoding.color {
        Some(f) => rows::distinct(ctx.rows, f),
        None => vec![bar_title.clone()],
    };
    // Without a colour channel the line is a second series with its own colour; with one, each
    // group's line has its group's colour.
    if multi {
        ctx.set_series(&groups);
    } else {
        ctx.set_series(&[bar_title.clone(), line_title.clone()]);
    }
    let line_slot = |g: usize| if multi { g } else { 1 };
    let label_format = block.label_format(&block.y, ctx.locale);
    let readout = block.readout_format(&block.y, ctx.locale);
    let locale2 = y2.locale.unwrap_or(ctx.locale);
    let label_format2 = block.label_format(&y2, locale2);
    let readout2 = block.readout_format(&y2, locale2);
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut excluded = 0usize;
    let mut panels_entries: Vec<Vec<Entry>> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut entries: Vec<Entry> = Vec::new();
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
            let bar = read(ctx, i, &y_field, &mut failures[fi]);
            let line = read(ctx, i, &line_field, &mut failures[fi]);
            let (bar, line) = match (bar, line) {
                (Read::Failed, _) | (_, Read::Failed) => continue,
                (b, l) => (
                    if let Read::Value(v) = b { Some(v) } else { None },
                    if let Read::Value(v) = l { Some(v) } else { None },
                ),
            };
            excluded += usize::from(bar.is_none()) + usize::from(line.is_none());
            if bar.is_none() && line.is_none() {
                continue;
            }
            if entries.iter().any(|e| e.category == category && e.group == group) {
                ctx.check(
                    "source-error",
                    Level::Error,
                    format!(
                        "Row {} repeats '{}'{}; a combo chart needs one row for each",
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
                failures[fi].get_or_insert_with(|| "Cannot draw this combo chart · Repeated categories".into());
                continue;
            }
            entries.push(Entry {
                row: i,
                category,
                group,
                bar,
                line,
            });
        }
        entries.sort_by_key(|e| (e.category, e.group));
        panels_entries.push(entries);
    }
    if failures.iter().all(Option::is_some) {
        let reason = failures.iter().flatten().next().cloned().unwrap_or_default();
        return kit::failed(ctx, &reason, RATIO);
    }
    if panels_entries
        .iter()
        .zip(&failures)
        .all(|(e, f)| f.is_some() || e.is_empty())
    {
        return kit::failed(ctx, "No values to show", RATIO);
    }
    if excluded > 0 {
        ctx.check(
            "excluded-observations",
            Level::Warning,
            format!(
                "{} or {} without a category {} left out",
                super::count(excluded, "missing value", "missing values"),
                super::verb(excluded, "row", "rows"),
                super::verb(excluded, "is", "are")
            ),
            Some("Give every category a bar value and a line value."),
            Some("chart.encoding.y".into()),
            None,
            json!({"count": excluded}),
        );
        ctx.note.push(format!(
            "{} {} not shown",
            super::count(excluded, "missing value", "missing values"),
            super::verb(excluded, "is", "are")
        ));
    }
    let visible: Vec<usize> = (0..groups.len()).filter(|g| !ctx.is_hidden(*g)).collect();
    let drawn: Vec<&Entry> = panels_entries
        .iter()
        .zip(&failures)
        .filter(|(_, f)| f.is_none())
        .flat_map(|(e, _)| e)
        .collect();
    let extents = |entries: &[&Entry]| {
        let mut bars = [f64::INFINITY, f64::NEG_INFINITY];
        let mut lines = [f64::INFINITY, f64::NEG_INFINITY];
        for e in entries {
            if let Some(v) = e.bar {
                bars = [bars[0].min(v), bars[1].max(v)];
            }
            if let Some(v) = e.line {
                lines = [lines[0].min(v), lines[1].max(v)];
            }
        }
        (bars, lines)
    };
    // The extents decide the axes (their ticks, whether a soft zero is kept), so the exact rows
    // decide them (TYP-36).
    let (shared_bars, shared_lines) = derived_extents(ctx, 0, "shared", &drawn, extents);
    let mut plan = kit::plan_secondary(
        ctx,
        facets.len(),
        &categories,
        if multi { groups.len() as f64 * 0.7 } else { 1.0 },
        block.style.ratio.unwrap_or(RATIO),
    );
    if !multi {
        // Each axis title in its mark's colour says which axis reads which mark.
        plan.title_colors = [
            Some(ctx.readable(ctx.series_color(0))),
            Some(ctx.readable(ctx.series_color(line_slot(0)))),
        ];
    }
    let frames = plan.frames(facets.len(), ctx.opts.width);
    let surface = Style::rgba(ctx.style.surface);
    let mut panels = Vec::new();
    for (fi, ((name, _), outer)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *outer, plan.title_room, name, reason));
            continue;
        }
        let entries = &panels_entries[fi];
        if entries.is_empty() {
            panels.push(super::failed_facet(
                ctx,
                *outer,
                plan.title_room,
                name,
                "No values to show",
            ));
            continue;
        }
        let mine: Vec<&Entry> = entries.iter().collect();
        let (own_bars, own_lines) = derived_extents(ctx, fi, "own", &mine, extents);
        let (mut data, mut data2) = if block.style.free_scales {
            (own_bars, own_lines)
        } else {
            (shared_bars, shared_lines)
        };
        if !data[0].is_finite() {
            data = ctx.wanted().y.unwrap_or([0.0, 1.0]);
        } else {
            data = super::Fit::widen(data, ctx.wanted().y);
        }
        ctx.fit.y = Some(data);
        if !data2[0].is_finite() {
            data2 = [0.0, 1.0];
        }
        let side = ValueSide {
            data,
            zero: Zero::Include,
            title: bar_title.clone(),
            format: &label_format,
            min: block.y.min,
            max: block.y.max,
        };
        let second = SecondarySide {
            data: data2,
            zero: Zero::Soft,
            title: line_title.clone(),
            format: &label_format2,
            min: y2.min,
            max: y2.max,
        };
        let frame = kit::frame_with(ctx, &plan, outer, &side, Some(&second), fi == 0);
        let t = frame.thickness(visible.len(), 56.0);
        let mut marks: Vec<Item> = Vec::new();
        let mut points: Vec<Item> = Vec::new();
        let mut observations = Vec::new();
        // The line of each group: its point per category, or none where a value is missing.
        let mut tracks: Vec<Vec<Option<[f64; 2]>>> = vec![vec![None; categories.len()]; groups.len()];
        // Each line's last point: its category, observation, readout text and the far edge of
        // its column along the category axis (FEA-CH-28).
        let mut last: Vec<Option<(usize, usize, String, f64)>> = vec![None; groups.len()];
        for e in entries {
            let Some(slot) = visible.iter().position(|g| *g == e.group) else {
                continue;
            };
            let c0 = frame.slot_start(e.category, slot, visible.len(), t);
            let cm = c0 + t / 2.0;
            let mut lines = Vec::new();
            let mut at = frame.pt(cm, 0.0);
            if let Some(v) = e.bar {
                let color = Style::rgba(ctx.series_color(e.group));
                let hatch = ctx.series[e.group].hatch;
                let rect = frame.span(c0, c0 + t, 0.0, v);
                let rect = if frame.horizontal {
                    Rect::new(rect.x, rect.y, rect.w.max(1.0), rect.h)
                } else {
                    Rect::new(rect.x, rect.y, rect.w, rect.h.max(1.0))
                };
                let r = ctx.style.bar_radius.min(rect.w / 2.0).min(rect.h / 2.0);
                marks.push(Item::Rect {
                    rect,
                    fill: Fill {
                        color,
                        hatch,
                        hatch_ink: hatch.map(|_| surface),
                        hatch_scale: 1.0,
                    },
                    stroke: None,
                    radius: [r; 4],
                    role: Role::Mark,
                    series: Some(e.group),
                    obs: Some(observations.len()),
                });
                at = frame.pt(cm, v);
                lines.push(ReadoutLine {
                    label: numbers::printed(v, &readout, label_format.digits),
                    name: if multi {
                        format!("{} {bar_title}", groups[e.group])
                    } else {
                        bar_title.clone()
                    },
                    value: numbers::value(v, &readout),
                    series: Some(e.group),
                    names_series: true,
                    derived: false,
                });
            }
            if let Some(v) = e.line {
                let p = frame.pt2(cm, v);
                tracks[e.group][e.category] = Some(p);
                if last[e.group]
                    .as_ref()
                    .is_none_or(|(category, ..)| *category <= e.category)
                {
                    // The value prints at the label's digits; the readout may carry more (FEA-CH-37).
                    let printed = numbers::printed(v, &readout2, label_format2.digits)
                        .unwrap_or_else(|| numbers::value(v, &readout2));
                    last[e.group] = Some((e.category, observations.len(), printed, c0 + t));
                }
                if e.bar.is_none() {
                    at = p;
                }
                let slot = line_slot(e.group);
                lines.push(ReadoutLine {
                    label: numbers::printed(v, &readout2, label_format2.digits),
                    name: if multi {
                        format!("{} {line_title}", groups[e.group])
                    } else {
                        line_title.clone()
                    },
                    value: numbers::value(v, &readout2),
                    series: Some(slot),
                    names_series: true,
                    derived: false,
                });
                points.push(Item::Point {
                    center: p,
                    radius: POINT_RADIUS,
                    shape: ctx.series[slot].shape.unwrap_or(Shape::Circle),
                    fill: Some(Fill::solid(Style::rgba(ctx.series_color(slot)))),
                    stroke: Some(Stroke::solid(surface, 1.5)),
                    role: Role::Mark,
                    series: Some(slot),
                    obs: Some(observations.len()),
                });
            }
            // The whole slot's column answers the pointer, so a short bar and its line point
            // are both reachable.
            let column = if frame.horizontal {
                Rect::new(frame.plot.x, c0, frame.plot.w, t)
            } else {
                Rect::new(c0, frame.plot.y, t, frame.plot.h)
            };
            observations.push(Observation {
                row: Some(e.row),
                dots: vec![],
                series: Some(e.group),
                at,
                wedge: None,
                area: Some(column),
                radius: 0.0,
                x: cm,
                readout: Readout {
                    heading: categories[e.category].clone(),
                    lines,
                }
                .into(),
            });
        }
        for &g in &visible {
            let slot = line_slot(g);
            let color = Style::rgba(ctx.series_color(slot));
            let mut run: Vec<[f64; 2]> = Vec::new();
            let mut runs: Vec<Vec<[f64; 2]>> = Vec::new();
            for p in &tracks[g] {
                match p {
                    Some(p) => run.push(*p),
                    None if block.style.bridge_gaps => {}
                    None => runs.push(std::mem::take(&mut run)),
                }
            }
            runs.push(run);
            for run in runs.into_iter().filter(|r| r.len() > 1) {
                let path = Path::polyline(&run);
                // A surface-coloured halo keeps the line apart from bars of its own colour.
                marks.push(Item::Line {
                    path: path.clone(),
                    stroke: Stroke::solid(surface, LINE_WIDTH + 3.0),
                    role: Role::Mark,
                    series: Some(slot),
                    obs: None,
                });
                let mut stroke = Stroke::solid(color, LINE_WIDTH);
                stroke.dash = ctx.series[slot].dash.clone();
                marks.push(Item::Line {
                    path,
                    stroke,
                    role: Role::Mark,
                    series: Some(slot),
                    obs: None,
                });
            }
        }
        marks.extend(points);
        let horizontal = frame.horizontal;
        let mut panel = kit::panel(ctx, outer, frame, marks, observations, name);
        // Automatic values, each in its own unit (chart design, 16 Nov 2026). First the last
        // value of every line, all or none: past its column at the point's height, else right
        // of the point, above it or below it, never on a bar, and moved along the plot's edge
        // where a centred value would cross it. Then the bars' values, all in one place
        // ([`super::place::combo_bar_values`]): a value the line would cross is left out, never
        // moved onto it.
        let lh = ctx.line_height();
        let plot = panel.plot;
        let within = |mut t: Text| {
            if t.anchor == Anchor::Middle {
                let dx = (plot.x - t.bounds.x).max(0.0) - (t.bounds.right() - plot.right()).max(0.0);
                t.x += dx;
                t.bounds.x += dx;
            }
            t
        };
        let reach = POINT_RADIUS + 5.0;
        let wanted: Vec<super::place::SparseValue> = visible
            .iter()
            .filter_map(|&g| {
                let (category, observation, text, edge) = last[g].clone()?;
                let [x, y] = tracks[g][category]?;
                let ink = ctx.style.ink;
                let past = if horizontal {
                    ctx.text_mid(
                        &text,
                        x,
                        edge + 4.0 + lh / 2.0,
                        Anchor::Middle,
                        Face::Figures,
                        Role::Label,
                        ink,
                    )
                } else {
                    ctx.text_mid(&text, edge + 4.0, y, Anchor::Start, Face::Figures, Role::Label, ink)
                };
                let places: Vec<Text> = [
                    past,
                    ctx.text_mid(&text, x + reach, y, Anchor::Start, Face::Figures, Role::Label, ink),
                    ctx.text_mid(
                        &text,
                        x,
                        y - reach - lh / 2.0,
                        Anchor::Middle,
                        Face::Figures,
                        Role::Label,
                        ink,
                    ),
                    ctx.text_mid(
                        &text,
                        x,
                        y + reach + lh / 2.0,
                        Anchor::Middle,
                        Face::Figures,
                        Role::Label,
                        ink,
                    ),
                ]
                .into_iter()
                .map(within)
                .collect();
                Some(super::place::SparseValue {
                    text,
                    series: line_slot(g),
                    observation,
                    places,
                })
            })
            .collect();
        let lines = wanted.len();
        super::place::sparse_values(ctx, fi, &mut panel, lines, wanted, &|_| true);
        super::place::combo_bar_values(ctx, fi, &mut panel, horizontal);
        panels.push(panel);
    }
    ctx.summary = format!(
        "{} with bars for {} and a line for {}.",
        super::count(categories.len(), "category", "categories"),
        bar_title,
        line_title
    );
    if multi {
        ctx.note.push(format!(
            "bars read on the axis at the left, the lines ({line_title}) on the axis at the right"
        ));
    }
    let legend = if multi { kit::legend(ctx, Swatch::Square) } else { None };
    kit::finish(&plan, facets.len(), panels, legend)
}

#[cfg(test)]
mod tests {
    use crate::block::ChartBlock;
    use crate::geometry::{Item, Role};
    use crate::layout::{ChartTheme, Options, layout};
    use serde_json::json;

    #[test]
    fn each_axis_title_takes_the_colour_of_the_marks_it_reads() {
        let block = ChartBlock::from_value(&json!({
            "type": "combo", "title": "T",
            "encoding": {"x": "q", "y": "revenue", "line": "margin"},
            "axes": {"y": {"label": "Revenue"}, "y2": {"label": "Margin"}}
        }))
        .unwrap();
        let rows: Vec<_> = [("Q1", 120, 18), ("Q2", 150, 22), ("Q3", 135, 20), ("Q4", 180, 27)]
            .iter()
            .map(|(q, r, m)| json!({"q": q, "revenue": r, "margin": m}))
            .collect();
        let g = layout(&block, &rows, &ChartTheme::default(), &Options::default());
        assert!(g.failure.is_none());
        let titles: Vec<_> = g
            .items()
            .filter_map(|i| match i {
                Item::Text(t) if t.role == Role::AxisTitle => Some((t.text.clone(), t.color)),
                _ => None,
            })
            .collect();
        assert_eq!(titles.len(), 2, "{titles:?}");
        assert_ne!(titles[0].1, titles[1].1, "the pairing reads by colour");
        assert!(g.note.is_none(), "the colours say which axis is which");
    }
}
