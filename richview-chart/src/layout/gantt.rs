//! Gantt charts: one bar for each task from its start to its end on a time axis, tasks in the
//! order of the source from top to bottom (FEA-CH-75). Starts and ends are dates or numbers, both
//! the same kind. A task that starts and ends at the same instant is a milestone and is drawn as a
//! diamond. RichView draws the bounds it is given and moves no task; the duration in a readout is
//! computed and says so (FEA-CH-42).

use super::kit::{self, XAxis};
use super::sample::disclose_excluded;
use super::types::Facets;
use super::{BASELINE_WIDTH, Ctx, GRID_WIDTH, LABEL_GAP, MIN_PLOT, Painted, Style};
use crate::axis;
use crate::derived::DataRange;
use crate::diag::Level;
use crate::geometry::{
    Anchor, DropReason, Face, Fill, Item, Observation, Panel, Path, Readout, ReadoutLine, Rect, Role, Shape, Stroke,
    Swatch, Targeting, XScale,
};
use crate::limits;
use crate::numbers;
use crate::rows;
use crate::time::Precision;
use serde_json::json;

const RATIO: f64 = 16.0 / 9.0;
/// Thickest a bar grows.
const MAX_BAR: f64 = 22.0;
/// Half the width of a milestone diamond.
const MILESTONE: f64 = 7.0;
/// Most lines a task name wraps to.
const NAME_LINES: usize = 2;
/// Widest share of a panel the task names may take.
const NAME_SHARE: f64 = 0.38;

struct Task {
    row: usize,
    label: String,
    group: usize,
    start: f64,
    end: f64,
}

fn fail(ctx: &mut Ctx, reason: &str, fix: &str) -> Painted {
    ctx.check(
        "source-error",
        Level::Error,
        reason,
        Some(fix),
        Some("chart.encoding".into()),
        None,
        serde_json::Value::Null,
    );
    kit::failed(ctx, reason, RATIO)
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let (Some(label_field), Some(start_field), Some(end_field)) = (
        block.encoding.label.clone(),
        block.channel("start"),
        block.channel("end"),
    ) else {
        return fail(
            ctx,
            "Cannot draw this Gantt chart · It needs encoding.label, encoding.start and encoding.end",
            "Give encoding.label, encoding.start and encoding.end.",
        );
    };
    let multi = block.encoding.color.is_some();
    let groups: Vec<String> = match &block.encoding.color {
        Some(f) => rows::distinct(ctx.rows, f),
        None => vec!["Tasks".to_owned()],
    };
    ctx.set_series(&groups);
    let locale = ctx.locale;
    let x_readout = block.readout_format(&block.x, locale);
    // Rows with a label, a start and an end; the others are disclosed.
    let mut excluded = 0usize;
    let mut usable: Vec<usize> = Vec::new();
    let mut is_usable = vec![false; ctx.rows.len()];
    for (_, members) in facets {
        for &i in members {
            let row = &ctx.rows[i];
            let complete = [&label_field, &start_field, &end_field]
                .iter()
                .all(|f| rows::text(row, f).is_some())
                && block
                    .encoding
                    .color
                    .as_ref()
                    .is_none_or(|f| rows::text(row, f).is_some());
            if complete {
                usable.push(i);
                is_usable[i] = true;
            } else {
                excluded += 1;
            }
        }
    }
    if usable.is_empty() {
        return fail(
            ctx,
            "Cannot draw this Gantt chart · No row has a label, a start and an end",
            "Give every task a label, a start and an end.",
        );
    }
    let starts = kit::read_x(ctx, &usable, &start_field);
    let ends = kit::read_x(ctx, &usable, &end_field);
    let (xs, xe, x_axis) = match (starts, ends) {
        (Ok((xs, a)), Ok((xe, b))) => match (a, b) {
            (
                XAxis::Calendar {
                    extent: ea,
                    precision: pa,
                    spacing,
                },
                XAxis::Calendar {
                    extent: eb,
                    precision: pb,
                    ..
                },
            ) => (
                xs,
                xe,
                XAxis::Calendar {
                    extent: [ea[0].min(eb[0]), ea[1].max(eb[1])],
                    precision: pa.max(pb),
                    spacing,
                },
            ),
            (XAxis::Numeric { extent: ea }, XAxis::Numeric { extent: eb }) => (
                xs,
                xe,
                XAxis::Numeric {
                    extent: [ea[0].min(eb[0]), ea[1].max(eb[1])],
                },
            ),
            _ => {
                return fail(
                    ctx,
                    "Cannot draw this Gantt chart · Starts and ends must both be dates or both be numbers",
                    "Write every start and end as an ISO date, or every one as a number.",
                );
            }
        },
        (Err(why), _) | (_, Err(why)) => {
            return fail(
                ctx,
                &format!("Cannot draw this Gantt chart · {why}"),
                "Write starts and ends as ISO dates or as plain numbers.",
            );
        }
    };
    // A numeric extent sets the axis and jumps with the data, so the exact rows decide it (TYP-36).
    let numeric: Option<DataRange> = match &x_axis {
        XAxis::Numeric { extent } => Some(ctx.derive_projected(0, "gantt.extent", || {
            DataRange::encode(*extent, ctx.grid_for(&start_field), &x_readout)
        })),
        _ => None,
    };
    let x_axis = match &numeric {
        Some(range) => XAxis::Numeric { extent: range.bounds() },
        None => x_axis,
    };
    let reversed: Vec<usize> = ctx.derive(0, "gantt.invalid-end", || {
        let mut reversed: Vec<_> = usable
            .iter()
            .copied()
            .filter(|&i| matches!((xs[i],xe[i]),(Some(start),Some(end)) if end<start))
            .collect();
        reversed.sort_unstable();
        reversed
    });
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut per_panel: Vec<Vec<Task>> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut tasks: Vec<Task> = Vec::new();
        for &i in members {
            if !is_usable[i] {
                continue;
            }
            let (Some(start), Some(end)) = (xs[i], xe[i]) else {
                excluded += 1;
                continue;
            };
            if reversed.binary_search(&i).is_ok() {
                ctx.check(
                    "source-error",
                    Level::Error,
                    format!("Row {} ends before it starts", i + 1),
                    Some("Swap the start and the end, or correct the date."),
                    Some(Ctx::row_path(i, &end_field)),
                    None,
                    json!({"row": i}),
                );
                failures[fi]
                    .get_or_insert_with(|| "Cannot draw this Gantt chart · A task ends before it starts".into());
                continue;
            }
            let row = &ctx.rows[i];
            let group = match &block.encoding.color {
                Some(f) => rows::text(row, f)
                    .and_then(|g| groups.iter().position(|x| *x == g))
                    .unwrap_or(0),
                None => 0,
            };
            tasks.push(Task {
                row: i,
                label: rows::text(row, &label_field).unwrap_or_default(),
                group,
                start,
                end,
            });
        }
        if tasks.len() > limits::max("gantt", "tasks per panel") {
            failures[fi] = Some(kit::over_limit(
                ctx,
                "gantt",
                "tasks per panel",
                tasks.len(),
                Some("chart.encoding.label"),
            ));
        }
        if tasks.is_empty() && failures[fi].is_none() {
            failures[fi] = Some("Cannot draw this Gantt chart · No row has a label, a start and an end".into());
        }
        per_panel.push(tasks);
    }
    if failures.iter().all(Option::is_some) {
        let reason = failures.iter().flatten().next().cloned().unwrap_or_default();
        return kit::failed(ctx, &reason, RATIO);
    }
    disclose_excluded(ctx, excluded, "a label, a start or an end");
    let lh = ctx.line_height();
    let size = ctx.size();
    let font = ctx.style.font.clone();
    let (ascent, _) = font.vertical();
    let mut plan = kit::Plan::xy(ctx, facets.len(), block.style.ratio.unwrap_or(RATIO));
    let width = ctx.opts.width;
    let pw = (width - 24.0 * (plan.columns as f64 - 1.0)) / plan.columns as f64;
    let live: Vec<&Vec<Task>> = per_panel
        .iter()
        .zip(&failures)
        .filter(|(_, f)| f.is_none())
        .map(|(t, _)| t)
        .collect();
    let widest = live
        .iter()
        .flat_map(|t| t.iter())
        .map(|t| font.width(&t.label, size))
        .fold(0.0, f64::max);
    let gutter = (widest + LABEL_GAP).min(pw * NAME_SHARE).max(LABEL_GAP * 2.0);
    let room = (gutter - LABEL_GAP).max(1.0);
    let name_lines = live
        .iter()
        .flat_map(|t| t.iter())
        .map(|t| font.wrap(&t.label, size, room, NAME_LINES).len())
        .max()
        .unwrap_or(1)
        .max(1);
    let row_h = (lh * 1.9).max(name_lines as f64 * lh + 6.0);
    let n_max = live.iter().map(|t| t.len()).max().unwrap_or(1).max(1);
    let plot_h = (n_max as f64 * row_h).max(MIN_PLOT);
    let pitch = plot_h / n_max as f64;
    let top_pad = 4.0;
    let x_title = block.x.label.clone();
    let bottom_room = LABEL_GAP + lh + if x_title.is_some() { lh } else { 0.0 };
    plan.panel_h = plan.title_room + top_pad + plot_h + bottom_room;
    let frames = plan.frames(facets.len(), width);
    let surface = Style::rgba(ctx.style.surface);
    let rule = Style::rgba(ctx.style.rule_soft);
    let ink = ctx.style.ink;
    let calendar_locale = ctx.calendar_locale();
    let when = |v: f64| match &x_axis {
        XAxis::Calendar { precision, .. } => axis::full_date(v as i64, *precision, calendar_locale),
        XAxis::Numeric { .. } => numbers::value(v, &x_readout),
    };
    // Whether a task has a length (a bar) or none (a milestone) and the duration it reads jump
    // with the data, so the exact rows decide them (TYP-36).
    let duration_of = |t: &Task| match &x_axis {
        XAxis::Calendar { precision, .. } if *precision >= Precision::Day => {
            let plain = numbers::Format {
                kind: numbers::Kind::Number,
                unit: None,
                digits: numbers::MAX_DIGITS,
                locale: x_readout.locale,
            };
            // A milestone has no length to state.
            ((t.end - t.start).abs() > 0.0).then(|| duration_text(t.end - t.start, x_readout.digits, &plain))
        }
        XAxis::Calendar { .. } => None,
        XAxis::Numeric { .. } => {
            let d = t.end - t.start;
            d.is_finite().then(|| numbers::value(d, &x_readout))
        }
    };
    let spans: Vec<Vec<(usize, bool, Option<String>)>> = per_panel
        .iter()
        .enumerate()
        .map(|(fi, tasks)| {
            ctx.derive(fi, "gantt.spans", || {
                tasks.iter().map(|t| (t.row, t.end > t.start, duration_of(t))).collect()
            })
        })
        .collect();
    let mut panels: Vec<Panel> = Vec::new();
    let mut shown = 0usize;
    let mut span = [f64::INFINITY, f64::NEG_INFINITY];
    let mut computed = false;
    for (fi, ((name, _), outer)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *outer, plan.title_room, name, reason));
            continue;
        }
        let tasks: Vec<&Task> = per_panel[fi].iter().filter(|t| !ctx.is_hidden(t.group)).collect();
        let plot = Rect::new(
            outer.x + gutter,
            outer.y + plan.title_room + top_pad,
            (outer.w - gutter - 12.0).max(1.0),
            plot_h,
        );
        let (domain, ticks, calendar) = kit::x_scale(ctx, &x_axis, &plot);
        ctx.fit.x = Some(x_axis.extent());
        let px = |v: f64| {
            let t = crate::scale::position(v, domain[0], domain[1]);
            let t = if t.is_finite() { t } else { 0.0 };
            plot.x + t * plot.w
        };
        let mut items: Vec<Item> = Vec::new();
        for (tx, label) in &ticks {
            if *tx < plot.x - 0.5 || *tx > plot.right() + 0.5 {
                continue;
            }
            items.push(Item::Line {
                path: Path::polyline(&[[*tx, plot.y], [*tx, plot.bottom()]]),
                stroke: Stroke::solid(rule, GRID_WIDTH),
                role: Role::Grid,
                series: None,
                obs: None,
            });
            let half = font.width(label, size) / 2.0;
            let at = tx.clamp(outer.x + half, (outer.right() - half).max(outer.x + half));
            items.push(Item::Text(ctx.text(
                label,
                at,
                plot.bottom() + LABEL_GAP + ascent * size,
                Anchor::Middle,
                Face::Figures,
                Role::AxisLabel,
                ink,
            )));
        }
        items.push(Item::Line {
            path: Path::polyline(&[[plot.x, plot.y], [plot.x, plot.bottom()]]),
            stroke: Stroke::solid(Style::rgba(ink), BASELINE_WIDTH),
            role: Role::Baseline,
            series: None,
            obs: None,
        });
        if let Some(title) = &x_title {
            items.push(Item::Text(ctx.text(
                title,
                plot.right(),
                plot.bottom() + LABEL_GAP + lh + ascent * size,
                Anchor::End,
                Face::Data,
                Role::AxisTitle,
                ink,
            )));
        }
        let th = (pitch * 0.58).clamp(4.0, MAX_BAR);
        let radius = ctx.style.bar_radius.min(th / 2.0);
        let mut marks: Vec<Item> = Vec::new();
        let mut observations: Vec<Observation> = Vec::new();
        for (k, t) in tasks.iter().enumerate() {
            let cy = plot.y + (k as f64 + 0.5) * pitch;
            let name_rows = font.wrap(&t.label, size, room, NAME_LINES);
            if name_rows.iter().any(|l| font.width(l, size) > room + 0.5) {
                ctx.drop_label(&t.label, DropReason::Density, "task name", None);
            } else {
                let count = name_rows.len() as f64;
                for (j, line) in name_rows.iter().enumerate() {
                    items.push(Item::Text(ctx.text_mid(
                        line,
                        plot.x - LABEL_GAP,
                        cy + (j as f64 - (count - 1.0) / 2.0) * lh,
                        Anchor::End,
                        Face::Data,
                        Role::AxisLabel,
                        ink,
                    )));
                }
            }
            let g = t.group;
            let color = Style::rgba(ctx.series_color(g));
            let hatch = ctx.series[g].hatch;
            let fill = Fill {
                color,
                hatch,
                hatch_ink: hatch.map(|_| surface),
                hatch_scale: 1.0,
            };
            let projected = |v| numeric.as_ref().map_or(v, |r| r.project(v, &x_readout));
            let (x0, x1) = (px(projected(t.start)), px(projected(t.end)));
            let index = observations.len();
            let shape = spans[fi]
                .iter()
                .find(|(row, _, _)| *row == t.row)
                .map_or((t.end > t.start, None), |(_, bar, text)| (*bar, text.clone()));
            let rect = if shape.0 {
                if x1 < plot.x || x0 > plot.right() {
                    continue;
                }
                let a = x0.max(plot.x);
                let b = x1.min(plot.right());
                let w = (b - a).max(2.0);
                let a = a.min(plot.right() - w);
                let rect = Rect::new(a, cy - th / 2.0, w, th);
                marks.push(Item::Rect {
                    rect,
                    fill,
                    stroke: None,
                    radius: [radius; 4],
                    role: Role::Mark,
                    series: Some(g),
                    obs: Some(index),
                });
                rect
            } else {
                if x0 < plot.x - 0.5 || x0 > plot.right() + 0.5 {
                    continue;
                }
                let c = x0.clamp(plot.x + MILESTONE, (plot.right() - MILESTONE).max(plot.x + MILESTONE));
                marks.push(Item::Point {
                    center: [c, cy],
                    radius: MILESTONE,
                    shape: Shape::Diamond,
                    fill: Some(fill),
                    stroke: Some(Stroke::solid(surface, 1.0)),
                    role: Role::Mark,
                    series: Some(g),
                    obs: Some(index),
                });
                Rect::new(c - MILESTONE, cy - MILESTONE, 2.0 * MILESTONE, 2.0 * MILESTONE)
            };
            let mut readout_lines = Vec::new();
            if let (true, Some(field)) = (multi && t.label != groups[g], &block.encoding.color) {
                readout_lines.push(ReadoutLine {
                    label: None,
                    name: super::cartesian::readable_name(field),
                    value: groups[g].clone(),
                    series: Some(g),
                    names_series: true,
                    derived: false,
                });
            }
            readout_lines.push(ReadoutLine {
                label: None,
                name: "Start".into(),
                value: when(t.start),
                series: None,
                names_series: false,
                derived: false,
            });
            readout_lines.push(ReadoutLine {
                label: None,
                name: "End".into(),
                value: when(t.end),
                series: None,
                names_series: false,
                derived: false,
            });
            let duration = shape.1.clone();
            if let Some(value) = duration {
                computed = true;
                readout_lines.push(ReadoutLine {
                    label: None,
                    name: "Duration".into(),
                    value,
                    series: None,
                    names_series: false,
                    derived: true,
                });
            }
            let at = rect.center();
            observations.push(Observation {
                row: Some(t.row),
                dots: vec![],
                series: Some(g),
                at,
                wedge: None,
                area: Some(rect),
                radius: 0.0,
                x: at[0],
                readout: Readout {
                    heading: t.label.clone(),
                    lines: readout_lines,
                }
                .into(),
            });
            shown += 1;
            span = [span[0].min(t.start), span[1].max(t.end)];
        }
        items.extend(marks);
        let mut panel = Panel {
            facet: None,
            frame: *outer,
            plot,
            items,
            observations,
            targeting: Targeting::Contain,
            x: Some(XScale {
                domain,
                extent: x_axis.extent(),
                range: [plot.x, plot.right()],
                calendar,
                y: None,
                least: None,
            }),
            failure: None,
        };
        super::panel_title(ctx, &mut panel, name);
        panels.push(panel);
    }
    if computed {
        ctx.note.push("duration is computed from the start and the end".into());
    }
    ctx.summary = if shown > 0 && span[0].is_finite() && span[1].is_finite() {
        format!(
            "Gantt chart of {} from {} to {}.",
            super::count(shown, "task", "tasks"),
            when(span[0]),
            when(span[1])
        )
    } else {
        "Gantt chart with no task in view.".to_owned()
    };
    let legend = kit::legend(ctx, Swatch::Square);
    kit::finish(&plan, facets.len(), panels, legend)
}

/// A duration as days, hours, minutes and seconds ("2 days", "1 day 1 hour", "1 hour 30 minutes", "30 seconds").
/// From 1,000 days on, a length that is not whole rounds to `digits` significant digits.
fn duration_text(ms: f64, digits: u8, plain: &numbers::Format) -> String {
    let unit = |n: i64, one: &str, many: &str| {
        format!(
            "{} {}",
            numbers::value(n as f64, plain),
            if n == 1 { one } else { many }
        )
    };
    let seconds = (ms.abs() / 1000.0).round() as i64;
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    if days >= 1000 && rest != 0 {
        let rounded = numbers::round_sig(seconds as f64 / 86_400.0, digits);
        return format!("{} days", numbers::value(rounded, plain));
    }
    let (hours, minutes, secs) = (rest / 3600, rest % 3600 / 60, rest % 60);
    let mut parts = Vec::new();
    if days > 0 {
        parts.push(unit(days, "day", "days"));
    }
    if hours > 0 {
        parts.push(unit(hours, "hour", "hours"));
    }
    if minutes > 0 {
        parts.push(unit(minutes, "minute", "minutes"));
    }
    if secs > 0 {
        parts.push(unit(secs, "second", "seconds"));
    }
    if parts.is_empty() {
        "less than a second".to_owned()
    } else {
        parts.join(" ")
    }
}
