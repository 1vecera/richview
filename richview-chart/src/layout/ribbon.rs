//! Ribbon charts (Power BI): a stacked bar for each category with its groups ranked by value,
//! largest on top, and a ribbon from each group's segment to its segment in the next category, so
//! a change of rank shows as a ribbon that crosses (FEA-CH-75). Categories keep the order of the
//! source and run left to right. A group with no row in a category has no segment there and no
//! ribbon into or out of it. Values cannot be negative. RichView ranks and stacks and adds
//! nothing else; rank and share in a readout are derived (FEA-CH-42, FEA-CH-70).

use super::kit::{self, ValueSide};
use super::pie::share_text;
use super::types::Facets;
use super::{Ctx, Painted, Style};
use crate::axis::Zero;
use crate::color;
use crate::diag::Level;
use crate::geometry::{
    Anchor, DropReason, Face, Fill, Item, Observation, Path, Readout, ReadoutLine, Rect, Role, Stroke, Swatch,
};
use crate::numbers;
use crate::rows::{self, Num};
use serde_json::json;

const RATIO: f64 = 16.0 / 9.0;
const RIBBON_OPACITY: f64 = 0.32;
/// Widest a bar grows.
const MAX_BAR: f64 = 56.0;

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
    let multi = block.encoding.color.is_some();
    let groups: Vec<String> = match &block.encoding.color {
        Some(f) => rows::distinct(ctx.rows, f),
        None => vec![title.clone()],
    };
    ctx.set_series(&groups);
    let label_format = block.label_format(&block.y, ctx.locale);
    let readout = block.readout_format(&block.y, ctx.locale);
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut excluded = 0usize;
    let mut tables: Vec<Vec<Vec<Option<Cell>>>> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut table: Vec<Vec<Option<Cell>>> = vec![vec![None; n]; groups.len()];
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
                Num::Value(v) if v < 0.0 => {
                    ctx.check(
                        "source-error",
                        Level::Error,
                        format!(
                            "Row {} has the negative value {}; a stacked segment cannot be negative",
                            i + 1,
                            numbers::value(v, &readout)
                        ),
                        Some("Use a bar chart for signed values."),
                        Some(Ctx::row_path(i, &y_field)),
                        None,
                        json!({"row": i}),
                    );
                    failures[fi].get_or_insert_with(|| {
                        format!("Cannot draw this ribbon chart · {y_field} has a negative value")
                    });
                    continue;
                }
                Num::Value(v) => v,
                Num::Missing => {
                    excluded += 1;
                    continue;
                }
                Num::Underflow(t) => {
                    ctx.underflow(i, &y_field, &t);
                    failures[fi].get_or_insert_with(|| {
                        format!("Cannot draw this ribbon chart · {y_field} has a value too small to represent")
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
                        format!("Cannot draw this ribbon chart · {y_field} has text that is not a number")
                    });
                    continue;
                }
            };
            if table[group][cat].is_some() {
                ctx.check(
                    "source-error",
                    Level::Error,
                    format!(
                        "Row {} repeats '{}'{}; a ribbon chart needs one row for each",
                        i + 1,
                        categories[cat],
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
                failures[fi].get_or_insert_with(|| "Cannot draw this ribbon chart · Repeated categories".into());
                continue;
            }
            table[group][cat] = Some(Cell { row: i, value });
        }
        let overflow = (0..n).any(|c| {
            !table
                .iter()
                .filter_map(|g| g[c])
                .map(|cell| cell.value)
                .sum::<f64>()
                .is_finite()
        });
        if failures[fi].is_none() && overflow {
            ctx.check(
                "value-overflow",
                Level::Error,
                "A total is too large to represent",
                None,
                Some("chart.encoding.y".into()),
                None,
                json!({}),
            );
            failures[fi] = Some("Cannot draw this ribbon chart · A total is too large".into());
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
    let visible: Vec<usize> = (0..groups.len()).filter(|&g| !ctx.is_hidden(g)).collect();
    let total_at = |table: &[Vec<Option<Cell>>], c: usize| -> f64 {
        visible.iter().filter_map(|&g| table[g][c]).map(|cell| cell.value).sum()
    };
    let tallest = |table: &[Vec<Option<Cell>>]| -> f64 {
        (0..n)
            .map(|c| table.iter().filter_map(|g| g[c]).map(|cell| cell.value).sum())
            .fold(0.0, f64::max)
    };
    // The full-state tallest stack keeps the axis still across reader interactions (TYP-36).
    let shared: f64 = ctx.derive(0, "ribbon.tallest", || {
        let top = tables
            .iter()
            .zip(&failures)
            .filter(|(_, f)| f.is_none())
            .map(|(t, _)| tallest(t))
            .fold(0.0, f64::max);
        crate::derived::coarse([0.0, top])[1]
    });
    let mut plan = kit::plan_with(
        ctx,
        facets.len(),
        &categories,
        1.0,
        block.style.ratio.unwrap_or(RATIO),
        true,
    );
    if plan.horizontal {
        // Categories run left to right so that ribbons can join neighbours.
        plan.horizontal = false;
        ctx.check(
            "style-invalid",
            Level::Info,
            "style.orientation is ignored: a ribbon chart runs left to right",
            None,
            Some("chart.style.orientation".into()),
            None,
            json!({}),
        );
    }
    let frames = plan.frames(facets.len(), ctx.opts.width);
    let surface = Style::rgba(ctx.style.surface);
    let mut panels = Vec::new();
    let mut changes = 0usize;
    for (fi, ((name, _), outer)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *outer, plan.title_room, name, reason));
            continue;
        }
        let table = &tables[fi];
        // Ranks, largest on top, and the edges of every segment in value units.
        let mut lower = vec![vec![0.0f64; n]; groups.len()];
        let mut upper = vec![vec![0.0f64; n]; groups.len()];
        // Exact-row ranks keep the order stable; visible shares are computed from shown values.
        let rank: Vec<Vec<Option<usize>>> = ctx.derive(fi, "ribbon.ranks", || {
            let mut rank = vec![vec![None; n]; groups.len()];
            for c in 0..n {
                let mut present: Vec<(usize, f64)> = table
                    .iter()
                    .enumerate()
                    .filter_map(|(g, cells)| cells[c].map(|cell| (g, cell.value)))
                    .collect();
                present.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
                for (r, (g, _)) in present.iter().enumerate() {
                    rank[*g][c] = Some(r + 1);
                }
            }
            rank
        });
        let share_texts: Vec<Vec<String>> = ctx.derive_visible(fi, "ribbon.shares", || {
            table
                .iter()
                .map(|cells| {
                    cells
                        .iter()
                        .enumerate()
                        .map(|(c, cell)| {
                            let total = total_at(table, c);
                            cell.filter(|_| total > 0.0)
                                .map_or_else(String::new, |cell| share_text(cell.value / total, ctx.locale))
                        })
                        .collect()
                })
                .collect()
        });
        let mut totals = vec![0.0f64; n];
        let mut counts = vec![0usize; n];
        let mut visible_ranks = vec![vec![None; n]; groups.len()];
        for c in 0..n {
            let mut present: Vec<(usize, f64)> = visible
                .iter()
                .filter_map(|&g| table[g][c].map(|cell| (g, cell.value)))
                .collect();
            present.sort_by_key(|p| rank[p.0][c]);
            totals[c] = present.iter().map(|p| p.1).sum();
            counts[c] = present.len();
            let mut top = totals[c];
            for (position, (g, v)) in present.iter().enumerate() {
                visible_ranks[*g][c] = Some(position + 1);
                upper[*g][c] = top;
                top = (top - v).max(0.0);
                lower[*g][c] = top;
            }
        }
        for g in &visible {
            changes += (1..n)
                .filter(|&c| matches!((visible_ranks[*g][c - 1], visible_ranks[*g][c]), (Some(a), Some(b)) if a != b))
                .count();
        }
        let mut data = if block.style.free_scales {
            ctx.derive(fi, "ribbon.own", || crate::derived::coarse([0.0, tallest(table)])[1])
        } else {
            shared
        };
        if data <= 0.0 {
            data = 1.0;
        }
        let data = super::Fit::widen([0.0, data], ctx.wanted().y);
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
        let t = (frame.band * 0.42).clamp(2.0, MAX_BAR);
        let mut ribbons: Vec<Item> = Vec::new();
        for &g in &visible {
            let c0 = Style::rgba(ctx.series_color(g));
            for c in 0..n.saturating_sub(1) {
                let (Some(a), Some(b)) = (table[g][c], table[g][c + 1]) else {
                    continue;
                };
                if a.value <= 0.0 || b.value <= 0.0 {
                    continue;
                }
                let x0 = frame.center(c) + t / 2.0;
                let x1 = frame.center(c + 1) - t / 2.0;
                let mid = (x0 + x1) / 2.0;
                let (top_a, bottom_a) = (frame.at(upper[g][c]), frame.at(lower[g][c]));
                let (top_b, bottom_b) = (frame.at(upper[g][c + 1]), frame.at(lower[g][c + 1]));
                let mut path = Path::new();
                path.move_to([x0, top_a]);
                path.curve_to([mid, top_a], [mid, top_b], [x1, top_b]);
                path.line_to([x1, bottom_b]);
                path.curve_to([mid, bottom_b], [mid, bottom_a], [x0, bottom_a]);
                path.close();
                ribbons.push(Item::Area {
                    path,
                    fill: Fill::solid(c0.with_alpha(RIBBON_OPACITY)),
                    role: Role::Band,
                    series: Some(g),
                    obs: None,
                });
            }
        }
        let mut bars: Vec<Item> = Vec::new();
        let mut observations: Vec<Observation> = Vec::new();
        for c in 0..n {
            let mut present: Vec<usize> = visible.iter().copied().filter(|&g| table[g][c].is_some()).collect();
            present.sort_by_key(|&g| rank[g][c]);
            let centre = frame.center(c);
            for g in present {
                let Some(cell) = table[g][c] else { continue };
                let raw = frame.span(centre - t / 2.0, centre + t / 2.0, lower[g][c], upper[g][c]);
                let rect = Rect::new(raw.x, raw.y, raw.w, raw.h.max(1.0));
                let fill_color = ctx.series_color(g);
                let bar_color = Style::rgba(fill_color);
                let hatch = ctx.series[g].hatch;
                bars.push(Item::Rect {
                    rect,
                    fill: Fill {
                        color: bar_color,
                        hatch,
                        hatch_ink: hatch.map(|_| surface),
                        hatch_scale: 1.0,
                    },
                    stroke: Some(Stroke::solid(surface, 1.0)),
                    radius: [0.0; 4],
                    role: Role::Mark,
                    series: Some(g),
                    obs: Some(observations.len()),
                });
                // The value inside its bar; the author's `auto_annotations: false` draws none and
                // records no drop.
                if cell.value > 0.0 && ctx.block.auto_annotations {
                    let label = numbers::value(cell.value, &label_format);
                    let ink = if color::contrast(ctx.style.ink, fill_color)
                        >= color::contrast(ctx.style.surface, fill_color)
                    {
                        ctx.style.ink
                    } else {
                        ctx.style.surface
                    };
                    let text = ctx.text_mid(
                        &label,
                        centre,
                        rect.center()[1],
                        Anchor::Middle,
                        Face::Figures,
                        Role::Label,
                        ink,
                    );
                    if text.bounds.h <= rect.h && text.bounds.w <= rect.w - 2.0 {
                        bars.push(Item::Text(text));
                    } else {
                        ctx.drop_label(&label, DropReason::Density, "value", Some(text.bounds));
                    }
                }
                let mut lines = vec![ReadoutLine {
                    label: None,
                    name: groups[g].clone(),
                    value: numbers::value(cell.value, &readout),
                    series: Some(g),
                    names_series: true,
                    derived: false,
                }];
                if counts[c] > 1 {
                    lines.push(ReadoutLine {
                        label: None,
                        name: "Rank".into(),
                        value: format!("{} of {}", visible_ranks[g][c].unwrap_or(1), counts[c]),
                        series: None,
                        names_series: false,
                        derived: true,
                    });
                }
                if totals[c] > 0.0 && counts[c] > 1 {
                    lines.push(ReadoutLine {
                        label: None,
                        name: "Share".into(),
                        value: share_texts[g][c].clone(),
                        series: None,
                        names_series: false,
                        derived: true,
                    });
                }
                let at = rect.center();
                observations.push(Observation {
                    row: Some(cell.row),
                    dots: vec![],
                    series: Some(g),
                    at,
                    wedge: None,
                    area: Some(rect),
                    radius: 0.0,
                    x: at[0],
                    readout: Readout {
                        heading: plan.names.get(c).cloned().unwrap_or_else(|| categories[c].clone()),
                        lines,
                    }
                    .into(),
                });
            }
        }
        let mut marks = ribbons;
        marks.extend(bars);
        panels.push(kit::panel(ctx, outer, frame, marks, observations, name));
    }
    ctx.summary = format!(
        "Ribbon chart of {} over {}; ranks change {} between neighbouring categories.",
        super::count(visible.len(), "group", "groups"),
        super::count(n, "category", "categories"),
        super::count(changes, "time", "times")
    );
    ctx.note
        .push("groups are ranked by value in each category, largest on top; ribbons join the same group".into());
    let legend = kit::legend(ctx, Swatch::Square);
    kit::finish(&plan, facets.len(), panels, legend)
}
