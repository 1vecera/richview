//! Rose charts (polar bars, Nightingale): one wedge per category at equal angles from 12 o'clock,
//! clockwise, whose reach is the value (FEA-CH-75). By default the radius grows linearly from a
//! zero centre; `style.scale: area` makes the wedge's area proportional instead, so the radius
//! grows with the square root. The chart says which it drew. Groups sit side by side inside
//! their category's sector.

use super::graph;
use super::pie::wedge_path;
use super::radar::{Disk, place_names, polar};
use super::types::Facets;
use super::{Ctx, Painted, Style, kit};
use crate::axis::{self, Zero};
use crate::diag::Level;
use crate::geometry::{
    Anchor, DropReason, Face, Fill, Item, Observation, Panel, Path, Readout, ReadoutLine, Rect, Role, Stroke, Swatch,
    Targeting, Wedge,
};
use crate::numbers;
use crate::rows::{self, Num};
use serde_json::json;
use std::f64::consts::TAU;

const RATIO: f64 = 1.0;
/// Space either side of a sector left open, as a share of the sector.
const PAD: f64 = 0.1;
/// Space between groups inside a sector, as a share of the sector.
const INNER_PAD: f64 = 0.03;

#[derive(Clone, Copy)]
struct Cell {
    row: usize,
    value: f64,
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let x_field = block.encoding.x.clone().unwrap_or_default();
    let y_field = block.encoding.y.clone().unwrap_or_default();
    let categories = rows::distinct(ctx.rows, &x_field);
    let n = categories.len();
    let title = block.y.label.clone().unwrap_or_else(|| y_field.clone());
    let groups: Vec<String> = match &block.encoding.color {
        Some(f) => rows::distinct(ctx.rows, f),
        None => vec![title.clone()],
    };
    ctx.set_series(&groups);
    let area_scale = block.style_text("scale").as_deref() == Some("area");
    let label_format = block.label_format(&block.y, ctx.locale);
    let readout = block.readout_format(&block.y, ctx.locale);
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut excluded = 0usize;
    let mut tables: Vec<Vec<Vec<Option<Cell>>>> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut table: Vec<Vec<Option<Cell>>> = vec![vec![None; n]; groups.len()];
        // A value that is negative or zero is the exact rows' call: a small one is zero on the grid.
        let skipped = graph::skipped(ctx, fi, members, Some(&y_field));
        for &i in members {
            let row = &ctx.rows[i];
            let Some(cat) = rows::text(row, &x_field).and_then(|c| categories.iter().position(|x| *x == c)) else {
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
                Num::Value(v) if skipped.negative.contains(&i) => {
                    ctx.check(
                        "source-error",
                        Level::Error,
                        format!(
                            "Row {} has a negative {y_field} {}; a rose wedge reaches out from the centre and cannot be negative",
                            i + 1,
                            numbers::value(v, &readout)
                        ),
                        Some("Use a bar chart for signed values."),
                        Some(Ctx::row_path(i, &y_field)),
                        None,
                        json!({"row": i}),
                    );
                    failures[fi]
                        .get_or_insert_with(|| format!("Cannot draw this rose chart · {y_field} has a negative value"));
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
                        format!("Cannot draw this rose chart · {y_field} has a value too small to represent")
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
                        format!("Cannot draw this rose chart · {y_field} has text that is not a number")
                    });
                    continue;
                }
            };
            if table[group][cat].is_some() {
                ctx.check(
                    "source-error",
                    Level::Error,
                    format!(
                        "Row {} repeats '{}'{}; a rose needs one row for each",
                        i + 1,
                        categories[cat],
                        if block.encoding.color.is_some() {
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
                failures[fi].get_or_insert_with(|| "Cannot draw this rose chart · Repeated categories".into());
                continue;
            }
            table[group][cat] = Some(Cell { row: i, value });
        }
        tables.push(table);
    }
    if failures.iter().all(Option::is_some) || n == 0 {
        let reason = failures
            .iter()
            .flatten()
            .next()
            .cloned()
            .unwrap_or_else(|| "Nothing to draw · No row has a category".into());
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
    ctx.note.push(
        if area_scale {
            "Wedge area is proportional to the value, so the radius grows with its square root"
        } else {
            "Wedge radius is proportional to the value, so area grows with its square"
        }
        .into(),
    );
    let visible: Vec<usize> = (0..groups.len()).filter(|g| !ctx.is_hidden(*g)).collect();
    let top = |tables: &[&Vec<Vec<Option<Cell>>>]| {
        tables
            .iter()
            .flat_map(|t| t.iter().flat_map(|g| g.iter().flatten()))
            .fold(0.0f64, |hi, c| hi.max(c.value))
    };
    let live: Vec<&Vec<Vec<Option<Cell>>>> = tables
        .iter()
        .zip(&failures)
        .filter(|(_, f)| f.is_none())
        .map(|(t, _)| t)
        .collect();
    // The radial scale's top sets the ticks, so the exact rows decide it (TYP-36).
    let shared = ctx.derive_extent(0, "rose.shared", || [0.0, top(&live)])[1];
    let width = ctx.opts.width;
    let lh = ctx.line_height();
    let size = ctx.size();
    let title_room = if block.encoding.facet.is_some() { lh + 6.0 } else { 0.0 };
    let columns = super::facet_columns(facets.len(), width);
    let pw = (width - 24.0 * (columns as f64 - 1.0)) / columns as f64;
    let disk = Disk::plan(ctx, &categories, pw, title_room, RATIO);
    let frames = super::grid_frames(facets.len(), width, 0.0, disk.panel_h);
    let sector = TAU / n as f64;
    let centres: Vec<f64> = (0..n).map(|i| (i as f64 + 0.5) * sector).collect();
    let rule = Style::rgba(ctx.style.rule_soft);
    let surface = Style::rgba(ctx.style.surface);
    let mut panels: Vec<Panel> = Vec::new();
    let mut outside = 0usize;
    for (fi, ((name, _), frame)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *frame, title_room, name, reason));
            continue;
        }
        let table = &tables[fi];
        let plot = Rect::new(frame.x, frame.y + title_room, frame.w, frame.h - title_room);
        let mut hi = if block.style.free_scales {
            ctx.derive_extent(fi, "rose.own", || [0.0, top(&[table])])[1]
        } else {
            shared
        };
        hi = super::Fit::widen([0.0, hi], ctx.wanted().y)[1];
        ctx.fit.y = Some([0.0, hi]);
        // A lone category is one wedge of nearly the whole circle, whose Bézier handles reach
        // past the arc, so it keeps a margin inside the plot.
        let radius = disk.radius(&plot, lh) * if n == 1 { 0.9 } else { 1.0 };
        let c = plot.center();
        let value = axis::value_axis(&axis::ValueSpec {
            data: [0.0, hi],
            zero: Zero::Include,
            min: block.y.min,
            max: block.y.max,
            length: radius,
            horizontal: false,
            format: &label_format,
            font: &ctx.style.figures,
            size,
        });
        let span = (value.hi - value.lo).max(f64::MIN_POSITIVE);
        let reach = |v: f64| {
            let f = ((v - value.lo) / span).clamp(0.0, 1.0);
            radius * if area_scale { f.sqrt() } else { f }
        };
        let mut items: Vec<Item> = Vec::new();
        // Rings at the ticks, and the spoke the tick labels sit on, between the last wedge and
        // the first.
        for t in value.ticks.iter().filter(|t| **t > value.lo) {
            items.push(Item::Line {
                path: wedge_path(c, reach(*t), 0.0, 0.0, TAU),
                stroke: Stroke::solid(rule, super::GRID_WIDTH),
                role: Role::Grid,
                series: None,
                obs: None,
            });
        }
        items.push(Item::Line {
            path: Path::polyline(&[c, polar(c, radius, 0.0)]),
            stroke: Stroke::solid(rule, super::GRID_WIDTH),
            role: Role::Grid,
            series: None,
            obs: None,
        });
        let shown = visible.len().max(1) as f64;
        let pad = sector * PAD;
        let inner_gap = sector * INNER_PAD;
        let each = (sector - 2.0 * pad - inner_gap * (shown - 1.0)) / shown;
        let mut observations: Vec<Observation> = Vec::new();
        for (slot, &g) in visible.iter().enumerate() {
            let color = Style::rgba(ctx.series_color(g));
            let hatch = ctx.series[g].hatch;
            for (i, cell) in table[g].iter().enumerate() {
                let Some(cell) = cell else { continue };
                let beyond = value.hi < cell.value;
                if beyond {
                    outside += 1;
                }
                let r = reach(cell.value);
                let a0 = i as f64 * sector + pad + slot as f64 * (each + inner_gap);
                let a1 = a0 + each;
                let mid = (a0 + a1) / 2.0;
                if r > 0.0 {
                    items.push(Item::Area {
                        path: wedge_path(c, r, 0.0, a0, a1),
                        fill: Fill {
                            color: color.with_alpha(0.88),
                            hatch,
                            hatch_ink: hatch.map(|_| surface),
                            hatch_scale: 1.0,
                        },
                        role: Role::Mark,
                        series: Some(g),
                        obs: Some(observations.len()),
                    });
                }
                let at = polar(c, r * 0.6, mid);
                observations.push(Observation {
                    row: Some(cell.row),
                    dots: vec![],
                    series: Some(g),
                    at,
                    area: None,
                    wedge: Some(Wedge {
                        center: c,
                        radius: r,
                        inner: 0.0,
                        start: a0,
                        end: a1,
                    }),
                    radius: 0.0,
                    x: at[0],
                    readout: Readout {
                        heading: categories[i].clone(),
                        lines: vec![ReadoutLine {
                            label: None,
                            name: groups[g].clone(),
                            value: numbers::value(cell.value, &readout),
                            series: Some(g),
                            names_series: true,
                            derived: false,
                        }],
                    }
                    .into(),
                });
            }
        }
        // Tick labels above their rings on the open spoke, outermost first; a label too close
        // to the one outside it is dropped and reported.
        let mut last: Option<f64> = None;
        let mut ticks: Vec<(f64, &String)> = value
            .ticks
            .iter()
            .zip(&value.labels)
            .filter(|(t, _)| **t > value.lo)
            .map(|(t, l)| (reach(*t), l))
            .collect();
        ticks.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut label_boxes: Vec<Rect> = Vec::new();
        for (r, label) in ticks {
            let t = ctx.text_mid(
                label,
                c[0],
                c[1] - r - lh / 2.0 - 1.0,
                Anchor::Middle,
                Face::Figures,
                Role::AxisLabel,
                ctx.style.ink,
            );
            if last.is_some_and(|l| l - r < lh * 1.05) {
                ctx.drop_label(label, DropReason::Density, "tick", Some(t.bounds));
                continue;
            }
            last = Some(r);
            label_boxes.push(t.bounds);
            items.push(Item::Text(t));
        }
        place_names(
            ctx,
            &categories,
            &disk.lines,
            &centres,
            c,
            radius,
            disk.room,
            *frame,
            &label_boxes,
            &mut items,
        );
        let mut panel = Panel {
            facet: None,
            frame: *frame,
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
    if outside > 0 {
        ctx.check(
            "value-outside-scale",
            Level::Warning,
            format!(
                "{} beyond the authored scale and {} drawn at its edge",
                super::count(outside, "value lies", "values lie"),
                super::verb(outside, "is", "are")
            ),
            Some("Widen axes.y.max."),
            Some("chart.axes.y".into()),
            None,
            json!({"count": outside}),
        );
    }
    ctx.summary = format!(
        "Rose of {} in {}: {}.",
        super::count(visible.len(), "group", "groups"),
        super::count(n, "category", "categories"),
        categories.join(", ")
    );
    Painted {
        height: super::grid_height(facets.len(), columns, disk.panel_h, super::FACET_GAP_Y),
        panels,
        figure_items: vec![],
        legend: kit::legend(ctx, Swatch::Square),
        failure: None,
    }
}
