//! Pivot tables, a matrix as in Power BI (FEA-CH-75): the distinct values of `encoding.rows` down the
//! side, those of `encoding.columns` across the top, and in each cell the sum of `encoding.values`
//! over the rows that fall there, with a total for every row and column and a grand total. All
//! of it is text and rectangles inside the geometry model. RichView computes every total and
//! every sum of several rows, and the readout says so (FEA-CH-42); a total covers the rows and
//! columns of the data, all of which are drawn: above 40 rows or 12 columns the table is not
//! drawn. Numbers are right-aligned (FEA-CH-19). `style.shade: true` tints the cells along the style's ramp by value.

use super::kit;
use super::sample::{disclose_excluded, read_number};
use super::types::Facets;
use super::{BASELINE_WIDTH, Ctx, GRID_WIDTH, Painted, Style};
use crate::color;
use crate::diag::Level;
use crate::geometry::{
    Anchor, DropReason, Face, Fill, Item, Observation, Panel, Path, Readout, ReadoutLine, Rect, Role, Stroke,
    Targeting, Text,
};
use crate::limits;
use crate::numbers;
use crate::rows;
use std::collections::HashMap;

const RATIO: f64 = 2.0;
/// Space between a cell's edge and its text.
const PAD: f64 = 8.0;
/// Narrowest the row-header column is meant to be.
const MIN_HEADER: f64 = 64.0;
/// Narrowest room for a row name when the figure is too narrow for the usual header.
const MIN_ROOM: f64 = 24.0;
/// Widest share of the figure the row headers take.
const HEADER_SHARE: f64 = 0.4;
/// Widest a column header grows before it wraps.
const HEADER_WRAP: f64 = 110.0;
/// Most a column grows to fill spare width.
const SPARE: f64 = 40.0;
/// Most lines a row name or column name wraps to.
const NAME_LINES: usize = 2;

/// The sum of the values that fall in one place and how many rows made it.
#[derive(Debug, Clone, Copy, Default)]
struct Tally {
    sum: f64,
    count: usize,
    first: usize,
}

impl Tally {
    fn add(&mut self, row: usize, value: f64) {
        if self.count == 0 {
            self.first = row;
        }
        self.sum += value;
        self.count += 1;
    }
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

fn bold(t: Text) -> Item {
    Item::Text(Text { weight: 600, ..t })
}

/// How a cell looks: its fill, whether its number is bold and the ink of the number.
#[derive(Clone, Copy)]
struct Look {
    fill: Fill,
    strong: bool,
    ink: color::Oklch,
}

/// The items of the table in the order they are drawn: cells, rules, text.
#[derive(Default)]
struct Sheet {
    rects: Vec<Item>,
    rules: Vec<Item>,
    texts: Vec<Item>,
    /// Numbers that do not fit their cell, which are reported and not drawn.
    drops: Vec<(String, Rect)>,
}

impl Sheet {
    /// A cell's rectangle, which readouts and the pointer find, and its right-aligned number.
    fn cell(&mut self, ctx: &Ctx, rect: Rect, text: Option<String>, look: Look, obs: Option<usize>) {
        self.rects.push(Item::Rect {
            rect,
            fill: look.fill,
            stroke: None,
            radius: [0.0; 4],
            role: Role::Mark,
            series: None,
            obs,
        });
        if let Some(text) = text {
            let t = ctx.text_mid(
                &text,
                rect.right() - PAD,
                rect.y + rect.h / 2.0,
                Anchor::End,
                Face::Figures,
                Role::Label,
                look.ink,
            );
            if t.bounds.w > rect.w - PAD + 0.5 {
                self.drops.push((text, t.bounds));
            } else {
                self.texts.push(if look.strong { bold(t) } else { Item::Text(t) });
            }
        }
    }
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let (Some(row_field), Some(col_field), Some(val_field)) =
        (block.channel("rows"), block.channel("columns"), block.channel("values"))
    else {
        return fail(
            ctx,
            "Cannot draw this pivot table · It needs encoding.rows, encoding.columns and encoding.values",
            "Give encoding.rows, encoding.columns and encoding.values.",
        );
    };
    let data = ctx.rows;
    let locale = ctx.locale;
    let label_format = block.label_format(&block.y, locale);
    let readout_format = block.readout_format(&block.y, locale);

    let mut failure: Option<String> = None;
    let mut excluded = 0usize;
    let mut row_names: Vec<String> = Vec::new();
    let mut col_names: Vec<String> = Vec::new();
    let mut row_at: HashMap<String, usize> = HashMap::new();
    let mut col_at: HashMap<String, usize> = HashMap::new();
    let mut cells: HashMap<(usize, usize), Tally> = HashMap::new();
    let mut row_total: Vec<Tally> = Vec::new();
    let mut col_total: Vec<Tally> = Vec::new();
    let mut grand = Tally::default();
    let members: Vec<usize> = facets.iter().flat_map(|(_, m)| m.iter().copied()).collect();
    for i in members {
        let (Some(r), Some(c)) = (rows::text(&data[i], &row_field), rows::text(&data[i], &col_field)) else {
            excluded += 1;
            continue;
        };
        let value = match read_number(ctx, "pivot table", &mut failure, i, &val_field) {
            Ok(Some(v)) => v,
            Ok(None) => {
                excluded += 1;
                continue;
            }
            Err(()) => continue,
        };
        let ri = *row_at.entry(r.clone()).or_insert_with(|| {
            row_names.push(r.clone());
            row_names.len() - 1
        });
        let ci = *col_at.entry(c.clone()).or_insert_with(|| {
            col_names.push(c.clone());
            col_names.len() - 1
        });
        row_total.resize(row_names.len(), Tally::default());
        col_total.resize(col_names.len(), Tally::default());
        cells.entry((ri, ci)).or_default().add(i, value);
        row_total[ri].add(i, value);
        col_total[ci].add(i, value);
        grand.add(i, value);
    }
    if let Some(reason) = failure {
        return kit::failed(ctx, &reason, RATIO);
    }
    if row_names.is_empty() {
        return fail(
            ctx,
            "Cannot draw this pivot table · No row has a row name, a column name and a value",
            "Give every row a value for rows, columns and values.",
        );
    }
    let overflow = cells
        .values()
        .chain(&row_total)
        .chain(&col_total)
        .any(|t| !t.sum.is_finite());
    if overflow {
        return fail(
            ctx,
            "Cannot draw this pivot table · A sum is too large to represent",
            "Scale the values.",
        );
    }
    // Above a limit the table is not drawn, so a total can never cover more than is shown.
    let mut over: Option<String> = None;
    for (what, count, path) in [
        ("rows", row_names.len(), "chart.encoding.rows"),
        ("columns", col_names.len(), "chart.encoding.columns"),
    ] {
        if count > limits::max("pivot", what) {
            let reason = kit::over_limit(ctx, "pivot", what, count, Some(path));
            over.get_or_insert(reason);
        }
    }
    if let Some(reason) = over {
        return kit::failed(ctx, &reason, RATIO);
    }
    disclose_excluded(ctx, excluded, "a row name, a column name or a value");

    let size = ctx.size();
    let lh = ctx.line_height();
    let width = ctx.opts.width;
    let font = ctx.style.font.clone();
    let figures = ctx.style.figures.clone();
    let ink = ctx.style.ink;
    let soft = ctx.style.ink_soft;
    let surface = ctx.style.surface;
    let shade = block
        .raw
        .get("style")
        .and_then(|s| s.get("shade"))
        .and_then(serde_json::Value::as_bool)
        == Some(true);

    let shown_rows = row_names.len();
    let candidate_cols = col_names.len();
    let text_of = |t: &Tally| numbers::value(t.sum, &label_format);

    // Columns: as wide as their widest number or their header, whichever is wider.
    let mut col_w: Vec<f64> = Vec::new();
    let mut col_lines: Vec<Vec<String>> = Vec::new();
    for (j, (name, total)) in col_names.iter().zip(&col_total).take(candidate_cols).enumerate() {
        let mut need = figures.width(&text_of(total), size) + 2.0;
        for ri in 0..shown_rows {
            if let Some(t) = cells.get(&(ri, j)) {
                need = need.max(figures.width(&text_of(t), size));
            }
        }
        let natural = font.width(name, size);
        let target = need.max(natural.min(HEADER_WRAP - 2.0 * PAD));
        let lines = font.wrap(name, size, target, NAME_LINES);
        let room = target.max(font.lines_width(&lines, size));
        col_w.push(room + 2.0 * PAD);
        col_lines.push(lines);
    }
    let mut total_w = figures.width(&text_of(&grand), size) + 2.0;
    for t in &row_total[..shown_rows] {
        total_w = total_w.max(figures.width(&text_of(t), size) + 2.0);
    }
    total_w = total_w.max(font.width("Total", size)) + 2.0 * PAD;
    // Row headers take up to their share of the figure and give way when not even one column fits.
    let widest_name = row_names[..shown_rows]
        .iter()
        .map(|n| font.width(n, size))
        .chain([font.width("Total", size), font.width(&row_field, size)])
        .fold(0.0, f64::max);
    let wanted_header = (widest_name + 2.0 * PAD).max(MIN_HEADER);
    let mut header_w = wanted_header.min(width * HEADER_SHARE);
    let first_col = col_w.first().copied().unwrap_or(0.0);
    if header_w + first_col + total_w > width {
        header_w = (width - first_col - total_w).max(MIN_ROOM + 2.0 * PAD);
    }
    let name_room = (header_w - 2.0 * PAD).max(1.0);
    let row_lines: Vec<Vec<String>> = row_names[..shown_rows]
        .iter()
        .map(|n| font.wrap(n, size, name_room, NAME_LINES))
        .collect();
    let name_lines = row_lines.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let row_h = if name_lines > 1 {
        name_lines as f64 * lh + 8.0
    } else {
        lh + 10.0
    };

    // Every column is drawn. A table wider than its figure narrows every column alike, and a
    // number or header that no longer fits is reported as dropped, never left out quietly.
    let shown_cols = candidate_cols;
    let used: f64 = col_w.iter().sum();
    let spare = ((width - header_w - used - total_w) / (shown_cols as f64 + 1.0)).clamp(0.0, SPARE);
    let fit = ((width - header_w).max(1.0) / (used + total_w)).min(1.0);
    let col_w: Vec<f64> = col_w.iter().map(|w| w * fit + spare).collect();
    let total_w = total_w * fit + spare;

    ctx.note.push("totals are sums RichView computed".into());

    // Geometry.
    let head_lines = col_lines[..shown_cols].iter().map(Vec::len).max().unwrap_or(1).max(1);
    let top = 2.0;
    let head_h = head_lines as f64 * lh + 10.0;
    let height = top + head_h + (shown_rows as f64 + 1.0) * row_h + 2.0;
    let table_w = header_w + col_w.iter().sum::<f64>() + total_w;
    let mut left = Vec::with_capacity(shown_cols);
    let mut x = header_w;
    for w in &col_w {
        left.push(x);
        x += w;
    }
    let total_left = x;

    // Colour of a cell by its value, when shading.
    let (lo, hi) = cells
        .iter()
        .filter(|((r, c), _)| *r < shown_rows && *c < shown_cols)
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), (_, t)| {
            (lo.min(t.sum), hi.max(t.sum))
        });
    let tint = |v: f64| {
        let t = if hi > lo { (v - lo) / (hi - lo) } else { 0.6 };
        color::ramp(&ctx.style.ramp, 0.2 + 0.8 * t.clamp(0.0, 1.0))
    };
    let clear = Fill::solid(Style::rgba(surface).with_alpha(0.0));
    let band = Fill::solid(Style::rgba(ink).with_alpha(0.05));

    let mut sheet = Sheet::default();
    let mut observations: Vec<Observation> = Vec::new();
    let mut drops: Vec<(String, Rect)> = Vec::new();

    // Header row.
    let head_bottom = top + head_h;
    let last_cy = head_bottom - 5.0 - lh / 2.0;
    if font.width(&row_field, size) <= name_room {
        sheet.texts.push(bold(ctx.text_mid(
            &row_field,
            PAD,
            last_cy,
            Anchor::Start,
            Face::Data,
            Role::AxisLabel,
            soft,
        )));
    } else {
        drops.push((
            row_field.clone(),
            Rect::new(PAD, last_cy - lh / 2.0, font.width(&row_field, size), lh),
        ));
    }
    let heads = col_lines[..shown_cols]
        .iter()
        .zip(&left)
        .zip(&col_w)
        .map(|((lines, l), w)| (lines.clone(), l + w, *w))
        .chain([(vec!["Total".to_owned()], total_left + total_w, total_w)]);
    for (lines, right, w) in heads {
        let n = lines.len();
        for (k, line) in lines.iter().enumerate() {
            let t = ctx.text_mid(
                line,
                right - PAD,
                last_cy - (n - 1 - k) as f64 * lh,
                Anchor::End,
                Face::Data,
                Role::AxisLabel,
                ink,
            );
            if t.bounds.w > w - PAD + 0.5 {
                drops.push((line.clone(), t.bounds));
            } else {
                sheet.texts.push(bold(t));
            }
        }
    }
    sheet.rules.push(Item::Line {
        path: Path::polyline(&[[0.0, head_bottom], [table_w, head_bottom]]),
        stroke: Stroke::solid(Style::rgba(ctx.style.ink_faint), 1.0),
        role: Role::Baseline,
        series: None,
        obs: None,
    });

    // One observation for a place that has a sum.
    let mut observe = |rect: Rect, t: &Tally, heading: String, derived: bool| {
        let mut lines = vec![ReadoutLine {
            label: None,
            name: block
                .y
                .label
                .clone()
                .unwrap_or_else(|| super::cartesian::readable_name(&val_field)),
            value: numbers::value(t.sum, &readout_format),
            series: None,
            names_series: false,
            derived,
        }];
        if derived && t.count > 1 {
            lines.push(ReadoutLine {
                label: None,
                name: "Source rows".into(),
                value: t.count.to_string(),
                series: None,
                names_series: false,
                derived: true,
            });
        }
        let at = rect.center();
        observations.push(Observation {
            row: (t.count == 1).then_some(t.first),
            dots: vec![],
            series: None,
            at,
            wedge: None,
            area: Some(rect),
            radius: 0.0,
            x: at[0],
            readout: Readout { heading, lines }.into(),
        });
        observations.len() - 1
    };

    for (ri, name) in row_names[..shown_rows].iter().enumerate() {
        let y0 = top + head_h + ri as f64 * row_h;
        let cy = y0 + row_h / 2.0;
        let lines = &row_lines[ri];
        let n = lines.len();
        if lines.iter().any(|l| font.width(l, size) > name_room + 0.5) {
            drops.push((name.clone(), Rect::new(PAD, y0, name_room, row_h)));
        } else {
            for (k, line) in lines.iter().enumerate() {
                sheet.texts.push(Item::Text(ctx.text_mid(
                    line,
                    PAD,
                    cy + (k as f64 - (n as f64 - 1.0) / 2.0) * lh,
                    Anchor::Start,
                    Face::Data,
                    Role::AxisLabel,
                    ink,
                )));
            }
        }
        for (j, (l, w)) in left.iter().zip(&col_w).enumerate() {
            let rect = Rect::new(*l, y0, *w, row_h);
            match cells.get(&(ri, j)) {
                Some(t) => {
                    let (fill, face_ink) = if shade {
                        let c = tint(t.sum);
                        let face_ink = if color::contrast(ink, c) >= color::contrast(surface, c) {
                            ink
                        } else {
                            surface
                        };
                        (Fill::solid(Style::rgba(c)), face_ink)
                    } else {
                        (clear, ink)
                    };
                    let derived = t.count > 1;
                    let obs = observe(rect, t, format!("{name} · {}", col_names[j]), derived);
                    sheet.cell(
                        ctx,
                        rect,
                        Some(text_of(t)),
                        Look {
                            fill,
                            strong: false,
                            ink: face_ink,
                        },
                        Some(obs),
                    );
                }
                None => sheet.cell(
                    ctx,
                    rect,
                    None,
                    Look {
                        fill: clear,
                        strong: false,
                        ink,
                    },
                    None,
                ),
            }
        }
        let rect = Rect::new(total_left, y0, total_w, row_h);
        let t = &row_total[ri];
        let obs = observe(rect, t, format!("{name} · Total"), true);
        sheet.cell(
            ctx,
            rect,
            Some(text_of(t)),
            Look {
                fill: band,
                strong: true,
                ink,
            },
            Some(obs),
        );
        if ri + 1 < shown_rows {
            sheet.rules.push(Item::Line {
                path: Path::polyline(&[[0.0, y0 + row_h], [table_w, y0 + row_h]]),
                stroke: Stroke::solid(Style::rgba(ctx.style.rule_soft), GRID_WIDTH),
                role: Role::Grid,
                series: None,
                obs: None,
            });
        }
    }

    // The total row.
    let y0 = top + head_h + shown_rows as f64 * row_h;
    let cy = y0 + row_h / 2.0;
    sheet.texts.push(bold(ctx.text_mid(
        "Total",
        PAD,
        cy,
        Anchor::Start,
        Face::Data,
        Role::AxisLabel,
        ink,
    )));
    for (j, (l, w)) in left.iter().zip(&col_w).enumerate() {
        let rect = Rect::new(*l, y0, *w, row_h);
        let t = &col_total[j];
        let obs = observe(rect, t, format!("Total · {}", col_names[j]), true);
        sheet.cell(
            ctx,
            rect,
            Some(text_of(t)),
            Look {
                fill: band,
                strong: true,
                ink,
            },
            Some(obs),
        );
    }
    let rect = Rect::new(total_left, y0, total_w, row_h);
    let obs = observe(rect, &grand, "Total".to_owned(), true);
    sheet.cell(
        ctx,
        rect,
        Some(text_of(&grand)),
        Look {
            fill: band,
            strong: true,
            ink,
        },
        Some(obs),
    );
    sheet.rules.push(Item::Line {
        path: Path::polyline(&[[0.0, y0], [table_w, y0]]),
        stroke: Stroke::solid(Style::rgba(soft), BASELINE_WIDTH),
        role: Role::Baseline,
        series: None,
        obs: None,
    });
    sheet.rules.push(Item::Line {
        path: Path::polyline(&[[total_left, top + head_h], [total_left, y0 + row_h]]),
        stroke: Stroke::solid(Style::rgba(ctx.style.rule_soft), GRID_WIDTH),
        role: Role::Grid,
        series: None,
        obs: None,
    });

    drops.append(&mut sheet.drops);
    for (text, at) in drops {
        ctx.drop_label(&text, DropReason::Density, "pivot header", Some(at));
    }
    ctx.summary = format!(
        "Pivot table of {} by {}: {} rows and {} columns, grand total {}.",
        val_field,
        col_field,
        row_names.len(),
        col_names.len(),
        numbers::value(grand.sum, &readout_format)
    );
    let frame = Rect::new(0.0, 0.0, width, height);
    let mut items = sheet.rects;
    items.extend(sheet.rules);
    items.extend(sheet.texts);
    let panel = Panel {
        facet: None,
        frame,
        plot: frame,
        items,
        observations,
        targeting: Targeting::Contain,
        x: None,
        failure: None,
    };
    Painted {
        height,
        panels: vec![panel],
        figure_items: Vec::new(),
        legend: None,
        failure: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tally_keeps_the_sum_the_count_and_the_first_row() {
        let mut t = Tally::default();
        t.add(3, 2.5);
        t.add(7, 4.0);
        assert_eq!((t.sum, t.count, t.first), (6.5, 2, 3));
    }
}
