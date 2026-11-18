//! Scatterplot matrices: every pair of several numeric fields as a small scatterplot in a square
//! grid (FEA-CH-75). The cell in row `r` and column `c` plots the field of column `c` across and
//! the field of row `r` up, below the diagonal; the diagonal names the field and its range; above
//! the diagonal each cell gives the Pearson correlation of its two fields, which RichView computes
//! and marks as derived (FEA-CH-42). Every cell of one field shares that field's range. A row with
//! a missing value in any field is left out and disclosed.

use super::kit;
use super::sample::{disclose_excluded, read_number};
use super::types::Facets;
use super::{Ctx, GRID_WIDTH, POINT_OPACITY, Painted, Style};
use crate::diag::Level;
use crate::geometry::{
    Anchor, DropReason, Face, Fill, Item, Observation, Panel, Path, Readout, ReadoutLine, Rect, Role, Shape, Stroke,
    Swatch, Targeting, Text,
};
use crate::limits;
use crate::numbers::{self, Format, Kind};
use crate::rows;

const RATIO: f64 = 1.0;
/// Widest a cell grows.
const MAX_CELL: f64 = 160.0;
/// Room kept free inside a cell so points do not sit on its border.
const CELL_PAD: f64 = 6.0;
/// Most lines a field name wraps to.
const NAME_LINES: usize = 3;
const PAD: f64 = 4.0;
/// Rows above which points are drawn smaller.
const MANY: usize = 300;

struct Record {
    row: usize,
    group: usize,
    values: Vec<f64>,
}

fn fail(ctx: &mut Ctx, reason: &str, fix: &str) -> Painted {
    ctx.check(
        "source-error",
        Level::Error,
        reason,
        Some(fix),
        Some("chart.encoding.dimensions".into()),
        None,
        serde_json::Value::Null,
    );
    kit::failed(ctx, reason, RATIO)
}

/// The smallest and largest value of each field over `sets`.
fn ranges(sets: &[&Vec<Record>], fields: usize) -> Vec<[f64; 2]> {
    let mut out = vec![[f64::INFINITY, f64::NEG_INFINITY]; fields];
    for record in sets.iter().flat_map(|s| s.iter()) {
        for (range, &v) in out.iter_mut().zip(&record.values) {
            range[0] = range[0].min(v);
            range[1] = range[1].max(v);
        }
    }
    out
}

/// Where `v` sits between `lo` and `hi`, from 0 to 1; the middle when the field has no spread.
fn unit(v: f64, [lo, hi]: [f64; 2]) -> f64 {
    if hi > lo {
        let t = crate::scale::position(v, lo, hi);
        if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.5 }
    } else {
        0.5
    }
}

/// The Pearson correlation of two samples of equal length, or `None` when there are fewer than
/// two pairs or one sample does not vary. The arithmetic runs on values scaled to at most one in
/// size, so a huge value cannot overflow.
fn pearson(a: &[f64], b: &[f64]) -> Option<f64> {
    if a.len() < 2 {
        return None;
    }
    let scale = |v: &[f64]| v.iter().fold(0.0f64, |m, x| m.max(x.abs()));
    let (sa, sb) = (scale(a), scale(b));
    if sa == 0.0 || sb == 0.0 {
        return None;
    }
    let n = a.len() as f64;
    let ua: Vec<f64> = a.iter().map(|v| v / sa).collect();
    let ub: Vec<f64> = b.iter().map(|v| v / sb).collect();
    let (ma, mb) = (ua.iter().sum::<f64>() / n, ub.iter().sum::<f64>() / n);
    let (mut cov, mut va, mut vb) = (0.0, 0.0, 0.0);
    for (x, y) in ua.iter().zip(&ub) {
        cov += (x - ma) * (y - mb);
        va += (x - ma).powi(2);
        vb += (y - mb).powi(2);
    }
    let r = cov / (va * vb).sqrt();
    // Rounding noise on an exact line (0.9999999999999999) is exactly one.
    let r = if r.abs() >= 1.0 - 1e-12 { 1.0f64.copysign(r) } else { r };
    (va > 0.0 && vb > 0.0 && r.is_finite()).then_some(r.clamp(-1.0, 1.0))
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let mut dims = block.channel_list("dimensions");
    let mut seen = std::collections::HashSet::new();
    dims.retain(|d| seen.insert(d.clone()));
    if dims.len() < 2 {
        return fail(
            ctx,
            "Cannot draw this scatterplot matrix · It needs encoding.dimensions with at least two fields",
            "List two or more numeric fields in encoding.dimensions.",
        );
    }
    if dims.len() > limits::max("splom", "dimensions") {
        let reason = kit::over_limit(
            ctx,
            "splom",
            "dimensions",
            dims.len(),
            Some("chart.encoding.dimensions"),
        );
        return kit::failed(ctx, &reason, RATIO);
    }
    let n = dims.len();
    let groups: Vec<String> = match &block.encoding.color {
        Some(f) => rows::distinct(ctx.rows, f),
        None => vec!["Rows".to_owned()],
    };
    ctx.set_series(&groups);
    let locale = ctx.locale;
    let label_format = block.label_format(&block.y, locale);
    let readout = block.readout_format(&block.y, locale);
    let r_format = Format {
        kind: Kind::Number,
        unit: None,
        digits: 3,
        locale,
    };
    let count_format = Format {
        kind: Kind::Number,
        unit: None,
        digits: numbers::MAX_DIGITS,
        locale,
    };
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut excluded = 0usize;
    let mut per_panel: Vec<Vec<Record>> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut records: Vec<Record> = Vec::new();
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
            let mut values = Vec::with_capacity(n);
            let (mut missing, mut bad) = (false, false);
            for field in &dims {
                match read_number(ctx, "scatterplot matrix", &mut failures[fi], i, field) {
                    Ok(Some(v)) => values.push(v),
                    Ok(None) => missing = true,
                    Err(()) => bad = true,
                }
            }
            if bad {
                continue;
            }
            if missing {
                excluded += 1;
                continue;
            }
            records.push(Record { row: i, group, values });
        }
        if records.is_empty() && failures[fi].is_none() {
            failures[fi] = Some("Cannot draw this scatterplot matrix · No row has a value in every field".into());
        }
        per_panel.push(records);
    }
    if failures.iter().all(Option::is_some) {
        let reason = failures.iter().flatten().next().cloned().unwrap_or_default();
        return kit::failed(ctx, &reason, RATIO);
    }
    disclose_excluded(ctx, excluded, "a value in every field or a group");
    let live: Vec<&Vec<Record>> = per_panel
        .iter()
        .zip(&failures)
        .filter(|(_, f)| f.is_none())
        .map(|(r, _)| r)
        .collect();
    let shared = ranges(&live, n);
    let lh = ctx.line_height();
    let size = ctx.size();
    let font = ctx.style.font.clone();
    let mut plan = kit::Plan::xy(ctx, facets.len(), RATIO);
    let width = ctx.opts.width;
    let pw = (width - 24.0 * (plan.columns as f64 - 1.0)) / plan.columns as f64;
    let side = (pw / n as f64).min(MAX_CELL);
    let grid = side * n as f64;
    plan.panel_h = plan.title_room + 2.0 * PAD + grid;
    let frames = plan.frames(facets.len(), width);
    let rule = Style::rgba(ctx.style.rule_soft);
    let ink = ctx.style.ink;
    let ink_soft = ctx.style.ink_soft;
    let mut panels: Vec<Panel> = Vec::new();
    let mut shown = 0usize;
    for (fi, ((name, _), outer)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *outer, plan.title_room, name, reason));
            continue;
        }
        // The ranges (they scale every cell and are written in the diagonal) jump with the data,
        // so the exact rows decide them and the texts they make (TYP-36); they are widened to short
        // decimals (FEA-CH-37).
        let (range, range_text, extreme_text): (Vec<[f64; 2]>, Vec<String>, Vec<[String; 2]>) =
            ctx.derive(fi, "splom.ranges", || {
                let own: Vec<[f64; 2]> = if block.style.free_scales {
                    ranges(&[&per_panel[fi]], n)
                } else {
                    shared.clone()
                }
                .into_iter()
                .map(crate::derived::coarse)
                .collect();
                let range_text = own
                    .iter()
                    .map(|[lo, hi]| {
                        format!(
                            "{} to {}",
                            numbers::value(*lo, &label_format),
                            numbers::value(*hi, &label_format)
                        )
                    })
                    .collect();
                let extreme_text = own
                    .iter()
                    .map(|[lo, hi]| [numbers::value(*lo, &readout), numbers::value(*hi, &readout)])
                    .collect();
                (own, range_text, extreme_text)
            });
        let visible: Vec<&Record> = per_panel[fi].iter().filter(|r| !ctx.is_hidden(r.group)).collect();
        let plot = Rect::new(
            outer.x + (outer.w - grid) / 2.0,
            outer.y + plan.title_room + PAD,
            grid,
            grid,
        );
        let cell = |r: usize, c: usize| Rect::new(plot.x + c as f64 * side, plot.y + r as f64 * side, side, side);
        let mut items: Vec<Item> = Vec::new();
        for k in 0..=n {
            let offset = k as f64 * side;
            items.push(Item::Line {
                path: Path::polyline(&[[plot.x, plot.y + offset], [plot.right(), plot.y + offset]]),
                stroke: Stroke::solid(rule, GRID_WIDTH),
                role: Role::Grid,
                series: None,
                obs: None,
            });
            items.push(Item::Line {
                path: Path::polyline(&[[plot.x + offset, plot.y], [plot.x + offset, plot.bottom()]]),
                stroke: Stroke::solid(rule, GRID_WIDTH),
                role: Role::Grid,
                series: None,
                obs: None,
            });
        }
        let upper: Vec<(usize, usize)> = (0..n).flat_map(|r| (r + 1..n).map(move |c| (r, c))).collect();
        let lower: Vec<(usize, usize)> = (1..n).flat_map(|r| (0..r).map(move |c| (r, c))).collect();
        let mut marks: Vec<Item> = Vec::new();
        let mut observations: Vec<Observation> = Vec::new();
        let show_range = range_text.iter().all(|t| font.width(t, size) <= side - 8.0);
        for (k, field) in dims.iter().enumerate() {
            let c = cell(k, k);
            let lines = font.wrap(field, size, (side - 8.0).max(1.0), NAME_LINES);
            if lines.iter().any(|l| font.width(l, size) > side - 8.0 + 0.5) {
                ctx.drop_label(field, DropReason::Density, "axis-title", None);
            } else {
                let rows_of_text = lines.len() + usize::from(show_range);
                let top = c.center()[1] - rows_of_text as f64 * lh / 2.0;
                for (j, line) in lines.iter().enumerate() {
                    let t = ctx.text_mid(
                        line,
                        c.center()[0],
                        top + (j as f64 + 0.5) * lh,
                        Anchor::Middle,
                        Face::Data,
                        Role::AxisTitle,
                        ink,
                    );
                    items.push(Item::Text(Text { weight: 600, ..t }));
                }
                if show_range {
                    items.push(Item::Text(ctx.text_mid(
                        &range_text[k],
                        c.center()[0],
                        top + (lines.len() as f64 + 0.5) * lh,
                        Anchor::Middle,
                        Face::Figures,
                        Role::AxisLabel,
                        ink_soft,
                    )));
                }
            }
            let at = c.center();
            observations.push(Observation {
                row: None,
                dots: vec![],
                series: None,
                at,
                wedge: None,
                area: Some(c),
                radius: side / 2.0,
                x: at[0],
                readout: Readout {
                    heading: field.clone(),
                    lines: vec![
                        ReadoutLine {
                            label: None,
                            name: "Smallest".into(),
                            value: extreme_text[k][0].clone(),
                            series: None,
                            names_series: false,
                            derived: false,
                        },
                        ReadoutLine {
                            label: None,
                            name: "Largest".into(),
                            value: extreme_text[k][1].clone(),
                            series: None,
                            names_series: false,
                            derived: false,
                        },
                    ],
                }
                .into(),
            });
        }
        // Above the diagonal: the correlation of the two fields over the rows in view.
        // Each correlation is written by the exact rows (TYP-36); a reader who hid a series asks
        // for new ones.
        let hidden: Vec<usize> = (0..groups.len()).filter(|g| ctx.is_hidden(*g)).collect();
        let correlations: Vec<String> = ctx.derive(fi, &format!("splom.correlations.{hidden:?}"), || {
            upper
                .iter()
                .map(|&(r, c)| {
                    let xs: Vec<f64> = visible.iter().map(|v| v.values[c]).collect();
                    let ys: Vec<f64> = visible.iter().map(|v| v.values[r]).collect();
                    pearson(&xs, &ys).map_or_else(
                        || "n/a".to_owned(),
                        |v| numbers::value(not_one(v, &r_format), &r_format),
                    )
                })
                .collect()
        });
        for (&(r, c), text) in upper.iter().zip(correlations) {
            let cell_rect = cell(r, c);
            let t = ctx.text_mid(
                &text,
                cell_rect.center()[0],
                cell_rect.center()[1],
                Anchor::Middle,
                Face::Figures,
                Role::Label,
                ink,
            );
            if t.bounds.w <= side - 4.0 {
                items.push(Item::Text(t));
            } else {
                ctx.drop_label(&text, DropReason::Density, "correlation", Some(t.bounds));
            }
            let at = cell_rect.center();
            observations.push(Observation {
                row: None,
                dots: vec![],
                series: None,
                at,
                wedge: None,
                area: Some(cell_rect),
                radius: side / 2.0,
                x: at[0],
                readout: Readout {
                    heading: format!("{} and {}", dims[c], dims[r]),
                    lines: vec![
                        ReadoutLine {
                            label: None,
                            name: "Correlation".into(),
                            value: text,
                            series: None,
                            names_series: false,
                            derived: true,
                        },
                        ReadoutLine {
                            label: None,
                            name: "Rows".into(),
                            value: numbers::value(visible.len() as f64, &count_format),
                            series: None,
                            names_series: false,
                            derived: true,
                        },
                    ],
                }
                .into(),
            });
        }
        // Below the diagonal: one point for each row.
        let radius = if visible.len() > MANY { 1.6 } else { 2.2 };
        for &(r, c) in &lower {
            let cell_rect = cell(r, c);
            let inner = (side - 2.0 * CELL_PAD).max(1.0);
            for record in &visible {
                let g = record.group;
                let color = Style::rgba(ctx.series_color(g));
                let shape = ctx.series[g].shape.unwrap_or(Shape::Circle);
                let at = [
                    cell_rect.x + CELL_PAD + unit(record.values[c], range[c]) * inner,
                    cell_rect.bottom() - CELL_PAD - unit(record.values[r], range[r]) * inner,
                ];
                marks.push(Item::Point {
                    center: at,
                    radius,
                    shape,
                    fill: Some(Fill::solid(color.with_alpha(POINT_OPACITY))),
                    stroke: None,
                    role: Role::Mark,
                    series: Some(g),
                    obs: Some(observations.len()),
                });
                let label_dim = block
                    .encoding
                    .label
                    .as_ref()
                    .and_then(|f| dims.iter().position(|d| d == f));
                let heading = label_dim
                    .map(|k| {
                        format!(
                            "{} {}",
                            super::cartesian::readable_name(&dims[k]),
                            numbers::value(record.values[k], &readout)
                        )
                    })
                    .or_else(|| {
                        block
                            .encoding
                            .label
                            .as_ref()
                            .and_then(|f| rows::text(&ctx.rows[record.row], f))
                    })
                    .or_else(|| block.encoding.color.as_ref().map(|_| groups[g].clone()))
                    .unwrap_or_else(|| format!("Row {}", record.row + 1));
                let mut lines: Vec<ReadoutLine> = Vec::new();
                if let (true, Some(field)) = (
                    heading != groups[g] && block.encoding.color != block.encoding.label,
                    &block.encoding.color,
                ) {
                    lines.push(ReadoutLine {
                        label: None,
                        name: super::cartesian::readable_name(field),
                        value: groups[g].clone(),
                        series: Some(g),
                        names_series: true,
                        derived: false,
                    });
                }
                for k in [c, r] {
                    if label_dim == Some(k) {
                        continue;
                    }
                    lines.push(ReadoutLine {
                        label: None,
                        name: super::cartesian::readable_name(&dims[k]),
                        value: numbers::value(record.values[k], &readout),
                        series: None,
                        names_series: false,
                        derived: false,
                    });
                }
                observations.push(Observation {
                    row: Some(record.row),
                    dots: vec![],
                    series: Some(g),
                    at,
                    wedge: None,
                    area: None,
                    radius,
                    x: at[0],
                    readout: Readout { heading, lines }.into(),
                });
            }
        }
        shown += visible.len();
        items.extend(marks);
        let mut panel = Panel {
            facet: None,
            frame: *outer,
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
    ctx.note.push(
        "each field has one range in every cell; the cells above the diagonal give the Pearson correlation of their two fields, computed by RichView"
            .into(),
    );
    ctx.summary = format!(
        "Scatterplot matrix of {} across {}: {}.",
        super::count(shown, "row", "rows"),
        super::count(n, "field", "fields"),
        dims.join(", ")
    );
    let legend = kit::legend(ctx, Swatch::Dot);
    kit::finish(&plan, facets.len(), panels, legend)
}

/// A correlation short of a perfect one never rounds up to one: 0.9996 shows as 0.999.
fn not_one(r: f64, format: &Format) -> f64 {
    if r.abs() < 1.0 && numbers::round_sig(r, format.digits).abs() >= 1.0 {
        0.999_f64.copysign(r)
    } else {
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correlation_of_a_line_is_one_and_of_its_mirror_minus_one() {
        let a = [1.0, 2.0, 3.0, 4.0];
        assert!((pearson(&a, &[2.0, 4.0, 6.0, 8.0]).unwrap() - 1.0).abs() < 1e-12);
        assert!((pearson(&a, &[8.0, 6.0, 4.0, 2.0]).unwrap() + 1.0).abs() < 1e-12);
    }

    #[test]
    fn an_exact_line_reads_one_not_a_hair_short() {
        let x: Vec<f64> = (0..40).map(|i| 0.37 * f64::from(i)).collect();
        let y: Vec<f64> = x.iter().map(|v| 0.1 * v + 0.1).collect();
        let r = pearson(&x, &y).unwrap();
        assert_eq!(r, 1.0);
        let format = Format::default();
        assert_eq!(numbers::value(not_one(r, &format), &format), "1");
        let down: Vec<f64> = y.iter().map(|v| -v).collect();
        let r = pearson(&x, &down).unwrap();
        assert_eq!(numbers::value(not_one(r, &format), &format), "\u{2212}1");
        assert_eq!(not_one(0.9996, &format), 0.999);
    }

    #[test]
    fn correlation_needs_two_pairs_and_some_spread() {
        assert_eq!(pearson(&[1.0], &[2.0]), None);
        assert_eq!(pearson(&[1.0, 1.0, 1.0], &[1.0, 2.0, 3.0]), None);
        assert_eq!(pearson(&[0.0, 0.0], &[1.0, 2.0]), None);
    }

    #[test]
    fn correlation_survives_huge_values() {
        let r = pearson(&[-1e300, 0.0, 1e300], &[-2e300, 1.0, 3e300]).unwrap();
        assert!(r > 0.9 && r <= 1.0);
    }
}
