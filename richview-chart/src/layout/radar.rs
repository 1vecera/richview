//! Radar charts: one spoke per named axis at equal angles from 12 o'clock, clockwise, rings at
//! the value axis ticks and one closed polygon per group (FEA-CH-75). A missing value breaks the
//! polygon at its spoke and is disclosed; RichView never joins across it. Polygons are read by
//! the spoke they cross, so the spokes keep one scale (FEA-CH-42, FEA-CH-70).

use super::types::Facets;
use super::{Ctx, LABEL_GAP, Painted, Style, kit};
use crate::axis::{self, Zero};
use crate::diag::Level;
use crate::geometry::{
    Anchor, DropReason, Face, Fill, Item, Observation, Panel, Path, Readout, ReadoutLine, Rect, Role, Shape, Stroke,
    Swatch, Targeting,
};
use crate::numbers;
use crate::rows::{self, Num};
use serde_json::json;
use std::f64::consts::{FRAC_PI_2, TAU};

const RATIO: f64 = 1.0;
/// Points are drawn on every spoke up to this many spokes.
const POINTS_UP_TO: usize = 12;
const POINT_RADIUS: f64 = 3.5;
/// Most lines a spoke name wraps to.
const NAME_LINES: usize = 3;
/// The widest share of a panel a side name may take.
const SIDE_SHARE: f64 = 0.26;
/// The widest share a side name may take when its longest word needs more than [`SIDE_SHARE`]:
/// a word cannot wrap, so the disk gives way before the name is dropped.
const WORD_SHARE: f64 = 0.4;

/// One cell of the table of values: the number and the row it came from.
#[derive(Clone, Copy)]
struct Cell {
    row: usize,
    value: f64,
}

/// The wrapped forms of `names`, each at most `room` wide where the words allow.
pub(super) fn name_lines(ctx: &Ctx, names: &[String], room: f64) -> Vec<Vec<String>> {
    let size = ctx.size();
    names
        .iter()
        .map(|n| ctx.style.font.wrap(n, size, room, NAME_LINES))
        .collect()
}

/// The widest a name may be beside the disk in a panel `width` wide.
pub(super) fn side_room(ctx: &Ctx, names: &[String], width: f64) -> f64 {
    let widest = names.iter().map(|n| ctx.text_width(n)).fold(0.0, f64::max);
    let word = names
        .iter()
        .flat_map(|n| n.split_whitespace())
        .map(|w| ctx.text_width(w))
        .fold(0.0, f64::max);
    widest
        .min((width * SIDE_SHARE).max(word.min(width * WORD_SHARE)))
        .max(1.0)
}

/// How names and the disk share a panel: the wrapped names, the room they take beside the disk
/// and the panel height that fits them.
pub(super) struct Disk {
    pub lines: Vec<Vec<String>>,
    pub room: f64,
    pub tallest: f64,
    pub reach_x: f64,
    pub panel_h: f64,
}

impl Disk {
    /// Plan panels `pw` wide for names set around the disk. Without an authored ratio a panel is
    /// at most `ratio` wide over high and no taller than its disk needs.
    pub fn plan(ctx: &Ctx, names: &[String], pw: f64, title_room: f64, ratio: f64) -> Disk {
        let lh = ctx.line_height();
        let cap = side_room(ctx, names, pw);
        let lines = name_lines(ctx, names, cap);
        // The room names need is the widest wrapped line, so short names leave the disk larger.
        let room = lines
            .iter()
            .flatten()
            .map(|l| ctx.text_width(l))
            .fold(0.0, f64::max)
            .min(cap)
            .max(1.0);
        let tallest = lines.iter().map(Vec::len).max().unwrap_or(1) as f64;
        let reach_x = (pw - 2.0 * (room + LABEL_GAP)) / 2.0;
        let fitted = 2.0 * (reach_x.max(20.0) + LABEL_GAP + tallest * lh) + title_room;
        let authored = ctx.block.style.ratio;
        let tall = pw / authored.unwrap_or(ratio) + title_room;
        Disk {
            lines,
            room,
            tallest,
            reach_x,
            panel_h: if authored.is_some() { tall } else { tall.min(fitted) },
        }
    }
    /// The radius of the disk in `plot`, leaving room for the names.
    pub fn radius(&self, plot: &Rect, lh: f64) -> f64 {
        let reach_y = (plot.h - 2.0 * (LABEL_GAP + self.tallest * lh)) / 2.0;
        self.reach_x.min(reach_y).max(20.0)
    }
}

/// Point at angle `a` (clockwise from 12 o'clock) and radius `r` around `c`.
pub(super) fn polar(c: [f64; 2], r: f64, a: f64) -> [f64; 2] {
    [c[0] + r * (a - FRAC_PI_2).cos(), c[1] + r * (a - FRAC_PI_2).sin()]
}

/// Names outside the disk at `angles`, wrapped, kept inside `bounds` and apart from each other.
/// A name that cannot be placed is dropped and reported, never clipped.
#[allow(clippy::too_many_arguments)]
pub(super) fn place_names(
    ctx: &mut Ctx,
    names: &[String],
    lines: &[Vec<String>],
    angles: &[f64],
    c: [f64; 2],
    radius: f64,
    room: f64,
    bounds: Rect,
    avoid: &[Rect],
    out: &mut Vec<Item>,
) {
    let lh = ctx.line_height();
    let ink = ctx.style.ink;
    let mut kept: Vec<Rect> = avoid.to_vec();
    let mut unfit: Vec<String> = Vec::new();
    for ((name, parts), &a) in names.iter().zip(lines).zip(angles) {
        let (s, k) = (a.sin(), a.cos());
        let at = polar(c, radius + LABEL_GAP, a);
        let n = parts.len() as f64;
        let side = s.abs() >= 0.25;
        let anchor = if !side {
            Anchor::Middle
        } else if s > 0.0 {
            Anchor::Start
        } else {
            Anchor::End
        };
        let mut texts = Vec::new();
        for (i, line) in parts.iter().enumerate() {
            let i = i as f64;
            let cy = if side {
                at[1] + (i - (n - 1.0) / 2.0) * lh
            } else if k > 0.0 {
                at[1] - lh / 2.0 - (n - 1.0 - i) * lh
            } else {
                at[1] + lh / 2.0 + i * lh
            };
            texts.push(ctx.text_mid(line, at[0], cy, anchor, Face::Data, Role::Label, ink));
        }
        let Some(first) = texts.first() else { continue };
        let region = texts.iter().skip(1).fold(first.bounds, |r, t| r.union(&t.bounds));
        let fits = parts.iter().all(|l| ctx.text_width(l) <= room + 0.5)
            && region.inside(&bounds, 0.5)
            && !kept.iter().any(|r| r.overlaps(&region, 1.0));
        if fits {
            kept.push(region);
            out.extend(texts.into_iter().map(Item::Text));
        } else {
            let reason = if region.inside(&bounds, 0.5) {
                DropReason::Density
            } else {
                DropReason::OutOfBounds
            };
            ctx.drop_label(name, reason, "axis name", Some(region));
            unfit.push(name.clone());
        }
    }
    if !unfit.is_empty() {
        ctx.check(
            "label-unfit",
            Level::Warning,
            format!(
                "{} axis names do not fit around the chart: {}",
                unfit.len(),
                unfit.join(", ")
            ),
            Some("Shorten the names, give the chart more width, or use fewer axes."),
            Some("chart.encoding.x".into()),
            None,
            json!({"names": unfit}),
        );
    }
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let x_field = block.encoding.x.clone().unwrap_or_default();
    let y_field = block.encoding.y.clone().unwrap_or_default();
    let spokes = rows::distinct(ctx.rows, &x_field);
    let n = spokes.len();
    let title = block.y.label.clone().unwrap_or_else(|| y_field.clone());
    let groups: Vec<String> = match &block.encoding.color {
        Some(f) => rows::distinct(ctx.rows, f),
        None => vec![title.clone()],
    };
    ctx.set_series(&groups);
    if n < 3 {
        let reason = format!("Cannot draw this radar chart · It needs at least 3 axes, and {x_field} has {n}");
        ctx.check(
            "source-error",
            Level::Error,
            format!("A radar chart needs at least 3 distinct {x_field} values; this has {n}"),
            Some("Add axes, or use a bar chart for fewer than three."),
            Some("chart.encoding.x".into()),
            None,
            json!({"axes": n}),
        );
        return kit::failed(ctx, &reason, RATIO);
    }
    let label_format = block.label_format(&block.y, ctx.locale);
    let readout = block.readout_format(&block.y, ctx.locale);
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut excluded = 0usize;
    let mut tables: Vec<Vec<Vec<Option<Cell>>>> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut table: Vec<Vec<Option<Cell>>> = vec![vec![None; n]; groups.len()];
        for &i in members {
            let row = &ctx.rows[i];
            let Some(spoke) = rows::text(row, &x_field).and_then(|c| spokes.iter().position(|x| *x == c)) else {
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
                        format!("Cannot draw this radar chart · {y_field} has a value too small to represent")
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
                        format!("Cannot draw this radar chart · {y_field} has text that is not a number")
                    });
                    continue;
                }
            };
            if table[group][spoke].is_some() {
                ctx.check(
                    "source-error",
                    Level::Error,
                    format!(
                        "Row {} repeats '{}'{}; a radar needs one row for each",
                        i + 1,
                        spokes[spoke],
                        if block.encoding.color.is_some() {
                            format!(" in '{}'", groups[group])
                        } else {
                            String::new()
                        }
                    ),
                    Some("Give each axis and group one row."),
                    Some("chart.encoding.x".into()),
                    None,
                    json!({"row": i}),
                );
                failures[fi].get_or_insert_with(|| "Cannot draw this radar chart · Repeated axes".into());
                continue;
            }
            table[group][spoke] = Some(Cell { row: i, value });
        }
        tables.push(table);
    }
    if failures.iter().all(Option::is_some) {
        let reason = failures.iter().flatten().next().cloned().unwrap_or_default();
        return kit::failed(ctx, &reason, RATIO);
    }
    // Gaps: rows without a value and (group, axis) pairs no row mentions.
    let absent: usize = tables
        .iter()
        .zip(&failures)
        .filter(|(_, f)| f.is_none())
        .map(|(t, _)| {
            t.iter()
                .map(|g| g.iter().filter(|c| c.is_none()).count())
                .sum::<usize>()
        })
        .sum();
    let gaps = absent.max(excluded);
    if gaps > 0 {
        ctx.check(
            "excluded-observations",
            Level::Warning,
            format!(
                "{} missing, so {}",
                super::count(gaps, "value is", "values are"),
                super::verb(
                    gaps,
                    "the polygon breaks at its axis",
                    "the polygons break at their axes"
                )
            ),
            Some("Give every group a value on every axis."),
            Some("chart.encoding.y".into()),
            None,
            json!({"count": gaps}),
        );
        ctx.note.push(format!(
            "{} {} in the polygons",
            super::count(gaps, "missing value", "missing values"),
            super::verb(gaps, "leaves a gap", "leave gaps")
        ));
    }
    let visible: Vec<usize> = (0..groups.len()).filter(|g| !ctx.is_hidden(*g)).collect();
    let extent = |tables: &[&Vec<Vec<Option<Cell>>>]| {
        tables
            .iter()
            .flat_map(|t| t.iter().flat_map(|g| g.iter().flatten()))
            .fold([f64::INFINITY, f64::NEG_INFINITY], |[lo, hi], c| {
                [lo.min(c.value), hi.max(c.value)]
            })
    };
    let live: Vec<&Vec<Vec<Option<Cell>>>> = tables
        .iter()
        .zip(&failures)
        .filter(|(_, f)| f.is_none())
        .map(|(t, _)| t)
        .collect();
    // The selected author state's exact rows set the axis even when a group is hidden (TYP-36).
    let shared = ctx.derive_extent(0, "radar.shared", || extent(&live));
    let width = ctx.opts.width;
    let lh = ctx.line_height();
    let size = ctx.size();
    let title_room = if block.encoding.facet.is_some() { lh + 6.0 } else { 0.0 };
    let columns = super::facet_columns(facets.len(), width);
    let pw = (width - 24.0 * (columns as f64 - 1.0)) / columns as f64;
    let disk = Disk::plan(ctx, &spokes, pw, title_room, RATIO);
    let (lines, room, panel_h) = (&disk.lines, disk.room, disk.panel_h);
    let frames = super::grid_frames(facets.len(), width, 0.0, panel_h);
    let angles: Vec<f64> = (0..n).map(|i| i as f64 * TAU / n as f64).collect();
    let surface = Style::rgba(ctx.style.surface);
    let rule = Style::rgba(ctx.style.rule_soft);
    let mut panels: Vec<Panel> = Vec::new();
    let mut outside = 0usize;
    let mut negative_centre = None;
    for (fi, ((name, _), frame)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *frame, title_room, name, reason));
            continue;
        }
        let table = &tables[fi];
        let plot = Rect::new(frame.x, frame.y + title_room, frame.w, frame.h - title_room);
        let mut data = if block.style.free_scales {
            ctx.derive_extent(fi, "radar.own", || extent(&[table]))
        } else {
            shared
        };
        if !data[0].is_finite() {
            data = ctx.wanted().y.unwrap_or([0.0, 1.0]);
        } else {
            data = super::Fit::widen(data, ctx.wanted().y);
        }
        ctx.fit.y = Some(data);
        let radius = disk.radius(&plot, lh);
        let c = plot.center();
        let value = axis::value_axis(&axis::ValueSpec {
            data,
            zero: Zero::Include,
            min: block.y.min,
            max: block.y.max,
            length: radius,
            horizontal: false,
            format: &label_format,
            font: &ctx.style.figures,
            size,
        });
        if value.lo < 0.0 {
            negative_centre = Some(numbers::value(value.lo, &readout));
        }
        let at = |spoke: usize, v: f64| polar(c, value.map(v, 0.0, radius).clamp(0.0, radius), angles[spoke]);
        let mut items: Vec<Item> = Vec::new();
        // Rings at the ticks, spokes, and the tick labels up the first spoke.
        for t in value.ticks.iter().filter(|t| **t > value.lo) {
            let r = value.map(*t, 0.0, radius);
            let mut ring: Vec<[f64; 2]> = (0..n).map(|i| polar(c, r, angles[i])).collect();
            ring.push(ring[0]);
            items.push(Item::Line {
                path: Path::polyline(&ring),
                stroke: Stroke::solid(rule, super::GRID_WIDTH),
                role: Role::Grid,
                series: None,
                obs: None,
            });
        }
        for &a in &angles {
            items.push(Item::Line {
                path: Path::polyline(&[c, polar(c, radius, a)]),
                stroke: Stroke::solid(rule, super::GRID_WIDTH),
                role: Role::Grid,
                series: None,
                obs: None,
            });
        }
        let mut marks: Vec<Item> = Vec::new();
        let mut observations: Vec<Observation> = Vec::new();
        let drawn_points = n <= POINTS_UP_TO;
        for &g in &visible {
            let color = Style::rgba(ctx.series_color(g));
            let hatch = ctx.series[g].hatch;
            let dash = ctx.series[g].dash.clone();
            let shape = ctx.series[g].shape.unwrap_or(Shape::Circle);
            let decals = ctx.series[g].shape.is_some();
            let cells = &table[g];
            let outline = |color| Stroke {
                dash: dash.clone(),
                ..Stroke::solid(color, 2.0)
            };
            let fill = Fill {
                color: color.with_alpha(0.18),
                hatch,
                hatch_ink: hatch.map(|_| color),
                hatch_scale: 1.0,
            };
            let present = cells.iter().filter(|c| c.is_some()).count();
            if present == n {
                let pts: Vec<[f64; 2]> = (0..n).map(|i| at(i, cells[i].map_or(0.0, |c| c.value))).collect();
                let mut path = Path::polyline(&pts);
                path.close();
                marks.push(Item::Area {
                    path: path.clone(),
                    fill,
                    role: Role::Mark,
                    series: Some(g),
                    obs: None,
                });
                marks.push(Item::Line {
                    path,
                    stroke: outline(color),
                    role: Role::Mark,
                    series: Some(g),
                    obs: None,
                });
            } else if present > 0 {
                // Runs of consecutive axes, starting just after a gap; each run is filled as a
                // sector from the centre and never joined to the next.
                let start = (0..n)
                    .find(|&i| cells[i].is_some() && cells[(i + n - 1) % n].is_none())
                    .unwrap_or(0);
                let mut run: Vec<usize> = Vec::new();
                let mut runs: Vec<Vec<usize>> = Vec::new();
                for k in 0..n {
                    let i = (start + k) % n;
                    if cells[i].is_some() {
                        run.push(i);
                    } else if !run.is_empty() {
                        runs.push(std::mem::take(&mut run));
                    }
                }
                if !run.is_empty() {
                    runs.push(run);
                }
                for run in runs.iter().filter(|r| r.len() >= 2) {
                    let pts: Vec<[f64; 2]> = run.iter().map(|&i| at(i, cells[i].map_or(0.0, |c| c.value))).collect();
                    let mut sector = vec![c];
                    sector.extend(&pts);
                    let mut path = Path::polyline(&sector);
                    path.close();
                    marks.push(Item::Area {
                        path,
                        fill,
                        role: Role::Mark,
                        series: Some(g),
                        obs: None,
                    });
                    marks.push(Item::Line {
                        path: Path::polyline(&pts),
                        stroke: outline(color),
                        role: Role::Mark,
                        series: Some(g),
                        obs: None,
                    });
                }
            }
            for (i, cell) in cells.iter().enumerate() {
                let Some(cell) = cell else { continue };
                let beyond = value.map(cell.value, 0.0, radius);
                if beyond > radius + 0.5 || beyond < -0.5 {
                    outside += 1;
                }
                let p = at(i, cell.value);
                let isolated = cells[(i + 1) % n].is_none() && cells[(i + n - 1) % n].is_none();
                if drawn_points || decals || isolated {
                    marks.push(Item::Point {
                        center: p,
                        radius: POINT_RADIUS,
                        shape,
                        fill: Some(Fill::solid(color)),
                        stroke: Some(Stroke::solid(surface, 1.0)),
                        role: Role::Mark,
                        series: Some(g),
                        obs: Some(observations.len()),
                    });
                }
                let lines: Vec<ReadoutLine> = visible
                    .iter()
                    .filter_map(|&h| {
                        table[h][i].map(|cell| ReadoutLine {
                            name: groups[h].clone(),
                            value: numbers::value(cell.value, &readout),
                            series: Some(h),
                            names_series: true,
                            derived: false,
                        })
                    })
                    .collect();
                observations.push(Observation {
                    row: Some(cell.row),
                    dots: vec![],
                    series: Some(g),
                    at: p,
                    area: None,
                    wedge: None,
                    radius: POINT_RADIUS + 1.0,
                    x: p[0],
                    readout: Readout {
                        heading: spokes[i].clone(),
                        lines,
                    }
                    .into(),
                });
            }
        }
        items.extend(marks);
        for (t, label) in value.ticks.iter().zip(&value.labels) {
            let r = value.map(*t, 0.0, radius);
            items.push(Item::Text(ctx.text_mid(
                label,
                c[0] - 4.0,
                c[1] - r,
                Anchor::End,
                Face::Figures,
                Role::AxisLabel,
                ctx.style.ink,
            )));
        }
        let avoid: Vec<Rect> = items
            .iter()
            .filter_map(|i| if let Item::Text(t) = i { Some(t.bounds) } else { None })
            .collect();
        place_names(
            ctx, &spokes, lines, &angles, c, radius, room, *frame, &avoid, &mut items,
        );
        let mut panel = Panel {
            facet: None,
            frame: *frame,
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
    if outside > 0 {
        ctx.check(
            "value-outside-scale",
            Level::Warning,
            format!(
                "{} outside the authored scale and {} drawn at its edge",
                super::count(outside, "value lies", "values lie"),
                super::verb(outside, "is", "are")
            ),
            Some("Widen axes.y.min or axes.y.max."),
            Some("chart.axes.y".into()),
            None,
            json!({"count": outside}),
        );
    }
    if let Some(lo) = negative_centre {
        ctx.note.push(format!("The centre of the radar is {lo}"));
    }
    ctx.summary = format!(
        "Radar of {} on {} axes: {}.",
        super::count(visible.len(), "group", "groups"),
        n,
        spokes.join(", ")
    );
    Painted {
        height: super::grid_height(facets.len(), columns, panel_h, super::FACET_GAP_Y),
        panels,
        figure_items: vec![],
        legend: kit::legend(ctx, Swatch::Square),
        failure: None,
    }
}
