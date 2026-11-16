//! Streamgraphs: stacked areas around a moving centre line instead of a flat baseline, so the
//! whole shape swells and thins with the total (FEA-CH-75). Streams stack in the order of the
//! legend, and the stack at each x is centred on zero (the silhouette baseline). The thickness of
//! a stream is its value, so the value axis is replaced by a scale bar of one tick step. A group
//! with no row at an x is drawn at zero width there and disclosed, never joined across. A stream
//! is named inside its widest stretch when the name fits, and the legend names every stream
//! (FEA-CH-42, FEA-CH-70).

use super::kit::{self, ValueSide, XAxis};
use super::types::Facets;
use super::{Ctx, Painted, Style};
use crate::axis::{self, Zero};
use crate::color;
use crate::curve;
use crate::diag::Level;
use crate::geometry::{
    Anchor, Cmd, Dot, DropReason, Face, Fill, Item, Observation, Path, Readout, ReadoutLine, Rect, Role, Stroke, Swatch,
};
use crate::numbers;
use crate::rows::{self, Num};
use serde_json::json;

const RATIO: f64 = 16.0 / 9.0;
/// Opacity of a stream, so the surface still shows through a little.
const OPACITY: f64 = 0.92;
/// Streams tried for a name's place, widest first.
const TRIES: usize = 24;

/// One group's observation at one x.
struct Point {
    row: usize,
    x: f64,
    value: f64,
}

/// `edge` (one move, then lines and curves) walked backwards onto `path`, which continues from the
/// edge's last point.
fn walk_back(path: &mut Path, edge: &Path) {
    let mut nodes: Vec<[f64; 2]> = Vec::new();
    let mut segments: Vec<Option<([f64; 2], [f64; 2])>> = Vec::new();
    for cmd in &edge.cmds {
        match cmd {
            Cmd::M { p } => nodes.push(*p),
            Cmd::L { p } => {
                nodes.push(*p);
                segments.push(None);
            }
            Cmd::C { c1, c2, p } => {
                nodes.push(*p);
                segments.push(Some((*c1, *c2)));
            }
            Cmd::Z => {}
        }
    }
    let Some(&last) = nodes.last() else { return };
    path.line_to(last);
    for i in (0..segments.len()).rev() {
        match segments[i] {
            None => path.line_to(nodes[i]),
            Some((c1, c2)) => path.curve_to(c2, c1, nodes[i]),
        }
    }
}

/// `values` at `x`, linear between the knots at `at` (ascending).
fn lerp_at(at: &[f64], values: &[f64], x: f64) -> f64 {
    if at.is_empty() {
        return 0.0;
    }
    if x <= at[0] {
        return values[0];
    }
    for i in 1..at.len() {
        if x <= at[i] {
            let d = at[i] - at[i - 1];
            let t = if d > 0.0 { (x - at[i - 1]) / d } else { 1.0 };
            return values[i - 1] + t * (values[i] - values[i - 1]);
        }
    }
    values[at.len() - 1]
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let x_field = block.encoding.x.clone().unwrap_or_default();
    let y_field = block.encoding.y.clone().unwrap_or_default();
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
    let mut axes: Vec<Option<XAxis>> = Vec::new();
    let mut data: Vec<Vec<Vec<Point>>> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut per_group: Vec<Vec<Point>> = groups.iter().map(|_| Vec::new()).collect();
        let dated: Vec<usize> = members
            .iter()
            .copied()
            .filter(|&i| rows::text(&ctx.rows[i], &x_field).is_some())
            .collect();
        excluded += members.len() - dated.len();
        if dated.is_empty() {
            failures[fi] = Some("Nothing to draw · No row has an x value".into());
            axes.push(None);
            data.push(per_group);
            continue;
        }
        match kit::read_x(ctx, &dated, &x_field) {
            Err(reason) => {
                failures[fi] = Some(format!("Cannot draw this streamgraph · {reason}"));
                axes.push(None);
            }
            Ok((xs, x_axis)) => {
                for &i in &dated {
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
                    match rows::num(row, &y_field) {
                        Num::Value(v) if v < 0.0 => {
                            ctx.check(
                                "source-error",
                                Level::Error,
                                format!(
                                    "Row {} has the negative value {}; a stream's thickness cannot be negative",
                                    i + 1,
                                    numbers::value(v, &readout)
                                ),
                                Some("Use a line or bar chart for signed values."),
                                Some(Ctx::row_path(i, &y_field)),
                                None,
                                json!({"row": i}),
                            );
                            failures[fi].get_or_insert_with(|| {
                                format!("Cannot draw this streamgraph · {y_field} has a negative value")
                            });
                        }
                        Num::Value(value) => per_group[group].push(Point { row: i, x, value }),
                        Num::Missing => excluded += 1,
                        Num::Underflow(t) => {
                            ctx.underflow(i, &y_field, &t);
                            failures[fi].get_or_insert_with(|| {
                                format!("Cannot draw this streamgraph · {y_field} has a value too small to represent")
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
                                format!("Cannot draw this streamgraph · {y_field} has text that is not a number")
                            });
                        }
                    }
                }
                for points in &mut per_group {
                    points.sort_by(|a, b| a.x.total_cmp(&b.x));
                }
                let repeats: Vec<usize> = per_group
                    .iter()
                    .flat_map(|points| points.windows(2).filter(|w| w[0].x == w[1].x).map(|w| w[1].row))
                    .collect();
                for row in repeats {
                    ctx.check(
                        "source-error",
                        Level::Error,
                        format!("Row {} repeats a group at the same x", row + 1),
                        Some("Give each group one row per x; add the values first if they belong together."),
                        Some(Ctx::row_path(row, &x_field)),
                        None,
                        json!({"row": row}),
                    );
                    failures[fi]
                        .get_or_insert_with(|| "Cannot draw this streamgraph · A group repeats at one x".into());
                }
                axes.push(Some(x_axis));
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
                "{} without an x, group or value {} left out",
                super::count(excluded, "row", "rows"),
                super::verb(excluded, "is", "are")
            ),
            Some("Give every row an x, a group and a value."),
            Some("chart.encoding.y".into()),
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
    let surface = Style::rgba(ctx.style.surface);
    let ink = Style::rgba(ctx.style.ink);
    let lh = ctx.line_height();
    let mut panels = Vec::new();
    let mut gaps = 0usize;
    let mut widest_total = 0.0f64;
    let mut stream_count = 0usize;
    for (fi, ((name, _), outer)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *outer, plan.title_room, name, reason));
            continue;
        }
        let Some(x_axis) = &axes[fi] else { continue };
        let per_group = &data[fi];
        // Streams in legend order; a hidden group leaves the stack.
        let visible: Vec<usize> = (0..groups.len())
            .filter(|&g| !per_group[g].is_empty() && !ctx.is_hidden(g))
            .collect();
        let mut xs: Vec<f64> = per_group.iter().flat_map(|points| points.iter().map(|p| p.x)).collect();
        xs.sort_by(f64::total_cmp);
        xs.dedup();
        let n = xs.len();
        let mut vals: Vec<Vec<Option<(f64, usize)>>> = Vec::new();
        for &g in &visible {
            let points = &per_group[g];
            let mut cursor = 0;
            let mut line = Vec::with_capacity(n);
            for &x in &xs {
                if cursor < points.len() && points[cursor].x == x {
                    line.push(Some((points[cursor].value, points[cursor].row)));
                    cursor += 1;
                } else {
                    line.push(None);
                }
            }
            gaps += line.iter().filter(|v| v.is_none()).count();
            vals.push(line);
        }
        // Each stream's lower and upper edge at every x, the stack centred on zero.
        let mut lowers = vec![vec![0.0f64; n]; visible.len()];
        let mut uppers = vec![vec![0.0f64; n]; visible.len()];
        let mut totals = vec![0.0f64; n];
        for i in 0..n {
            let total: f64 = vals.iter().map(|v| v[i].map_or(0.0, |p| p.0)).sum();
            totals[i] = total;
            let mut level = -total / 2.0;
            for k in 0..visible.len() {
                lowers[k][i] = level;
                level += vals[k][i].map_or(0.0, |p| p.0);
                uppers[k][i] = level;
            }
        }
        if totals.iter().any(|t| !t.is_finite()) {
            ctx.check(
                "value-overflow",
                Level::Error,
                "A total is too large to represent",
                None,
                Some("chart.encoding.y".into()),
                None,
                json!({}),
            );
            panels.push(super::failed_facet(
                ctx,
                *outer,
                plan.title_room,
                name,
                "Cannot draw this streamgraph · A total is too large",
            ));
            continue;
        }
        // The exact full-state total keeps the scale still; visible totals use shown values.
        let tallest = ctx.derive_extent(fi, "streamgraph.extent", || {
            let mut all = vec![0.0; n];
            for p in per_group.iter().flatten() {
                if let Ok(i) = xs.binary_search_by(|x| x.total_cmp(&p.x)) {
                    all[i] += p.value;
                }
            }
            [0.0, all.into_iter().fold(0.0, f64::max)]
        })[1];
        let total_texts: Vec<String> = ctx.derive_visible(fi, "streamgraph.totals", || {
            totals.iter().map(|t| numbers::value(*t, &readout)).collect()
        });
        let name_order: Vec<Vec<usize>> = ctx.derive(fi, "streamgraph.names", || {
            per_group
                .iter()
                .map(|points| {
                    let mut thickness = vec![0.0; n];
                    for p in points {
                        if let Ok(i) = xs.binary_search_by(|x| x.total_cmp(&p.x)) {
                            thickness[i] = p.value;
                        }
                    }
                    let mut order: Vec<usize> = (0..n).collect();
                    order.sort_by(|&a, &b| thickness[b].total_cmp(&thickness[a]));
                    order.truncate(TRIES);
                    order
                })
                .collect()
        });
        widest_total = widest_total.max(tallest);
        stream_count = stream_count.max(visible.len());
        let extent = if tallest > 0.0 {
            super::Fit::widen([-tallest / 2.0, tallest / 2.0], ctx.wanted().y)
        } else {
            ctx.wanted().y.unwrap_or([-1.0, 1.0])
        };
        ctx.fit.y = Some(extent);
        let side = ValueSide {
            data: extent,
            zero: Zero::Include,
            title: title.clone(),
            format: &label_format,
            min: None,
            max: None,
        };
        let x_title = block.x.label.as_deref();
        let mut frame = kit::xy_frame(ctx, &plan, outer, x_axis, x_title, &side);
        let plot = frame.plot;
        // The value axis would read as heights above a baseline; the centre line is not one.
        frame.items.retain(|item| match item {
            Item::Text(t) => {
                !(t.role == Role::AxisLabel && t.y < plot.bottom() + 6.0)
                    && !(t.role == Role::AxisTitle && t.y < plot.y)
            }
            other => other.role() != Role::Grid && other.role() != Role::Baseline,
        });
        let px: Vec<f64> = xs.iter().map(|x| frame.x(*x)).collect();
        let mut marks: Vec<Item> = Vec::new();
        let mut separators: Vec<Item> = Vec::new();
        let mut labels: Vec<Rect> = Vec::new();
        let mut label_items: Vec<Item> = Vec::new();
        let span = (plot.w * 0.04).clamp(4.0, 24.0);
        for (k, &g) in visible.iter().enumerate() {
            let tops: Vec<f64> = uppers[k].iter().map(|v| frame.y(*v)).collect();
            let bottoms: Vec<f64> = lowers[k].iter().map(|v| frame.y(*v)).collect();
            let c = Style::rgba(ctx.series_color(g));
            let hatch = ctx.series[g].hatch;
            let fill = Fill {
                color: c.with_alpha(OPACITY),
                hatch,
                hatch_ink: hatch.map(|_| surface),
                hatch_scale: 1.0,
            };
            if n == 1 {
                marks.push(Item::Rect {
                    rect: Rect::new(px[0] - span / 2.0, tops[0], span, (bottoms[0] - tops[0]).max(0.0)),
                    fill,
                    stroke: None,
                    radius: [0.0; 4],
                    role: Role::Mark,
                    series: Some(g),
                    obs: None,
                });
                continue;
            }
            let up: Vec<[f64; 2]> = px.iter().zip(&tops).map(|(x, y)| [*x, *y]).collect();
            let down: Vec<[f64; 2]> = px.iter().zip(&bottoms).map(|(x, y)| [*x, *y]).collect();
            let mut top_edge = Path::new();
            curve::append_run(&mut top_edge, &up, curve_kind, strength, false);
            let mut bottom_edge = Path::new();
            curve::append_run(&mut bottom_edge, &down, curve_kind, strength, false);
            let mut area = top_edge.clone();
            walk_back(&mut area, &bottom_edge);
            area.close();
            marks.push(Item::Area {
                path: area,
                fill,
                role: Role::Mark,
                series: Some(g),
                obs: None,
            });
            if k + 1 < visible.len() {
                separators.push(Item::Line {
                    path: top_edge,
                    stroke: Stroke::solid(surface, 1.0),
                    role: Role::Band,
                    series: None,
                    obs: None,
                });
            }
            // The name inside the stream's widest stretch, where the whole label fits.
            if visible.len() > 1 {
                let text_w = ctx.text_width(&groups[g]);
                let mids: Vec<f64> = tops.iter().zip(&bottoms).map(|(t, b)| (t + b) / 2.0).collect();
                let mut placed = false;
                for &i in name_order.get(g).map_or(&[][..], Vec::as_slice) {
                    if i >= n {
                        break;
                    }
                    if bottoms[i] - tops[i] < lh * 1.05 || plot.w < text_w + 4.0 {
                        break;
                    }
                    let cx = px[i].clamp(plot.x + text_w / 2.0 + 2.0, plot.right() - text_w / 2.0 - 2.0);
                    let cy = lerp_at(&px, &mids, cx);
                    let fits = (0..=8).all(|s| {
                        let sx = cx - text_w / 2.0 + text_w * f64::from(s) / 8.0;
                        lerp_at(&px, &tops, sx) <= cy - lh / 2.0 && lerp_at(&px, &bottoms, sx) >= cy + lh / 2.0
                    });
                    if !fits {
                        continue;
                    }
                    let fill_color = ctx.series_color(g);
                    let text_color = if color::contrast(ctx.style.ink, fill_color)
                        >= color::contrast(ctx.style.surface, fill_color)
                    {
                        ctx.style.ink
                    } else {
                        ctx.style.surface
                    };
                    let t = ctx.text_mid(&groups[g], cx, cy, Anchor::Middle, Face::Data, Role::Label, text_color);
                    if labels.iter().any(|l| l.overlaps(&t.bounds, 1.0)) {
                        continue;
                    }
                    labels.push(t.bounds);
                    label_items.push(Item::Text(t));
                    placed = true;
                    break;
                }
                if !placed {
                    ctx.drop_label(&groups[g], DropReason::Density, "stream-name", None);
                }
            }
        }
        marks.extend(separators);
        marks.extend(label_items);
        // A bar one tick step thick says how thick a stream of that value is.
        if tallest > 0.0 {
            let step = frame.value.step;
            let half = (frame.y(0.0) - frame.y(step)).abs() / 2.0;
            let cy = frame.y(0.0);
            let bx = outer.x + 4.0;
            if half >= 3.0 && step.is_finite() {
                let rule = Stroke::solid(ink, 1.5);
                for path in [
                    Path::polyline(&[[bx, cy - half], [bx, cy + half]]),
                    Path::polyline(&[[bx - 3.0, cy - half], [bx + 3.0, cy - half]]),
                    Path::polyline(&[[bx - 3.0, cy + half], [bx + 3.0, cy + half]]),
                ] {
                    marks.push(Item::Line {
                        path,
                        stroke: rule.clone(),
                        role: Role::Annotation,
                        series: None,
                        obs: None,
                    });
                }
                let text = numbers::value(step, &label_format);
                let t = ctx.text_mid(
                    &text,
                    outer.x,
                    cy - half - lh / 2.0 - 2.0,
                    Anchor::Start,
                    Face::Figures,
                    Role::Key,
                    ctx.style.ink_soft,
                );
                if t.bounds.right() <= plot.x - 2.0 && t.bounds.y >= outer.y {
                    marks.push(Item::Text(t));
                } else {
                    ctx.drop_label(&text, DropReason::OutOfBounds, "scale-bar", Some(t.bounds));
                }
            }
        }
        // One observation per x, with a dot on each stream's centre.
        let mut observations = Vec::new();
        for (i, &x) in xs.iter().enumerate() {
            let at = px[i];
            if at < plot.x - 0.5 || at > plot.right() + 0.5 {
                continue;
            }
            let mut lines = Vec::new();
            let mut dots = Vec::new();
            let mut row = None;
            for (k, &g) in visible.iter().enumerate() {
                match vals[k][i] {
                    Some((v, r)) => {
                        row.get_or_insert(r);
                        lines.push(ReadoutLine {
                            name: groups[g].clone(),
                            value: numbers::value(v, &readout),
                            series: Some(g),
                            names_series: true,
                            derived: false,
                        });
                        dots.push(Dot {
                            at: [at, frame.y((lowers[k][i] + uppers[k][i]) / 2.0)],
                            series: g,
                        });
                    }
                    None => lines.push(ReadoutLine {
                        name: groups[g].clone(),
                        value: "No observation".into(),
                        series: Some(g),
                        names_series: true,
                        derived: false,
                    }),
                }
            }
            if visible.len() > 1 {
                lines.push(ReadoutLine {
                    name: "Total".into(),
                    value: total_texts.get(i).cloned().unwrap_or_default(),
                    series: None,
                    names_series: false,
                    derived: true,
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
                x: at,
                readout: Readout { heading, lines }.into(),
            });
        }
        panels.push(kit::xy_panel(ctx, outer, frame, marks, observations, name));
    }
    if gaps > 0 {
        ctx.check(
            "missing-stream-values",
            Level::Warning,
            format!(
                "{} missing at an x where another group has one, and {} drawn at zero width",
                super::count(gaps, "group value is", "group values are"),
                super::verb(gaps, "is", "are")
            ),
            Some("Give every group a row at every x, or leave the x out."),
            Some("chart.encoding.y".into()),
            None,
            json!({"count": gaps}),
        );
        ctx.note.push(format!(
            "{} {} drawn at zero width",
            super::count(gaps, "missing group value", "missing group values"),
            super::verb(gaps, "is", "are")
        ));
    }
    ctx.note.push(
        "streams are stacked in the order of the legend around a centre line; thickness is the value, and the bar at the left is one tick step"
            .into(),
    );
    ctx.summary = format!(
        "Streamgraph of {}; the thickest total is {}.",
        super::count(stream_count, "stream", "streams"),
        numbers::value(widest_total, &readout)
    );
    let legend = kit::legend(ctx, Swatch::Square);
    kit::finish(&plan, facets.len(), panels, legend)
}
