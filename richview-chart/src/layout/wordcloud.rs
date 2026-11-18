//! Word clouds: words sized by their values, packed into the plot without overlap. A word's size
//! follows the square root of its value between the smallest size the chart's text allows and a
//! largest that fits the plot, so equal values are equal sizes in every panel. Words go in from
//! the largest to the smallest, each at the first free place along a spiral out of the centre,
//! measured with the real font (horizontal text only). A word that finds no room is reported,
//! never clipped or overlapped (FEA-CH-26), and stays in the readouts and the keyboard order.

use super::graph;
use super::kit;
use super::sample::{disclose_excluded, read_number};
use super::types::Facets;
use super::{Ctx, Painted};
use crate::color::{self, Oklch};
use crate::diag::Level;
use crate::geometry::{
    Anchor, DropReason, Face, Item, Observation, Panel, Readout, ReadoutLine, Rect, Role, Targeting, Text,
};
use crate::limits;
use crate::numbers::{self, Format, Kind};
use crate::rows;
use serde::{Deserialize, Serialize};
use std::f64::consts::TAU;

const RATIO: f64 = 16.0 / 9.0;
/// Space kept around every word and the plot's edge.
const PAD: f64 = 2.0;
/// Most steps along the spiral for one word.
const MAX_STEPS: usize = 40_000;
/// Where on the ramp the smallest and the largest word sit.
const RAMP_FROM: f64 = 0.35;

struct Word {
    row: usize,
    text: String,
    value: f64,
    size: f64,
    color: Oklch,
    /// Its box once placed.
    spot: Option<Rect>,
}

/// The first free place for a box of `w` by `h` along a spiral out of the middle of `area`, as the
/// centre of the box; `None` when the spiral leaves the area without one.
fn find_spot(placed: &[Rect], w: f64, h: f64, area: &Rect) -> Option<[f64; 2]> {
    let [cx, cy] = area.center();
    if w + 2.0 * PAD > area.w || h + 2.0 * PAD > area.h {
        return None;
    }
    let aspect = (area.w / area.h).clamp(1.0, 3.0);
    let pitch = h * 0.5;
    let mut theta = 0.0f64;
    for _ in 0..MAX_STEPS {
        let rho = pitch * theta / TAU;
        let at = [
            cx + rho * aspect * crate::det::cos(theta),
            cy + rho * crate::det::sin(theta),
        ];
        let bx = Rect::new(at[0] - w / 2.0, at[1] - h / 2.0, w, h);
        if bx.inside(area, 0.0) {
            let clear = placed.iter().all(|p| !bx.inflate(PAD).overlaps(p, 0.0));
            if clear {
                return Some(at);
            }
        } else if rho * aspect > area.w && rho > area.h {
            return None;
        }
        theta += (h * 0.5) / (rho * aspect.max(1.0)).max(h * 0.5);
    }
    None
}

/// One word of `size` pixels centred at `at`.
fn set(ctx: &Ctx, text: &str, size: f64, at: [f64; 2], color: Oklch) -> Text {
    let font = &ctx.style.font;
    let (ascent, descent) = font.vertical();
    let baseline = at[1] + (ascent - descent) * size / 2.0;
    let base = ctx.text(text, at[0], baseline, Anchor::Middle, Face::Data, Role::Label, color);
    Text {
        size,
        bounds: font.bounds(text, size, at[0], baseline, Anchor::Middle),
        ..base
    }
}

/// The size for `value` between `lo` and `hi` on a square-root scale between the smallest
/// and largest value.
fn size_of(value: f64, range: [f64; 2], lo: f64, hi: f64) -> f64 {
    let (a, b) = (range[0].sqrt(), range[1].sqrt());
    let t = if b > a {
        ((value.sqrt() - a) / (b - a)).clamp(0.0, 1.0)
    } else {
        0.5
    };
    lo + (hi - lo) * t
}

/// One panel's sizes from exact values on a normalized grid, source-stable shown-value ties,
/// and author-precision shares of the exact values.
/// The sizes in pixels and the packing follow from these, so every layout places the same words
/// in the same order at the same sizes (TYP-36).
#[derive(Serialize, Deserialize)]
struct Plan {
    grid: crate::payload::Quant,
    /// Positions between the smallest and largest size, decoded on `grid`.
    fraction: Vec<u64>,
    order: Vec<usize>,
    shares: Vec<String>,
}

fn plan_of(
    per_panel: &[Vec<Word>],
    failures: &[Option<String>],
    fi: usize,
    sum: f64,
    share: &Format,
    readout: &Format,
) -> Plan {
    let (mut low, mut high) = (f64::INFINITY, 0.0f64);
    for (words, failure) in per_panel.iter().zip(failures) {
        if failure.is_none() {
            for w in words {
                low = low.min(w.value);
                high = high.max(w.value);
            }
        }
    }
    let words = &per_panel[fi];
    let values: Vec<f64> = words.iter().map(|w| w.value).collect();
    let shown: Vec<f64> = words.iter().map(|w| numbers::shown(w.value, readout)).collect();
    // Largest first, ties in source order.
    let mut order: Vec<usize> = (0..words.len()).collect();
    order.sort_by(|&a, &b| shown[b].total_cmp(&shown[a]).then(a.cmp(&b)));
    let fractions: Vec<f64> = values.iter().map(|v| size_of(*v, [low, high], 0.0, 1.0)).collect();
    let fraction_format = Format {
        digits: readout.digits,
        ..share.clone()
    };
    let grid = crate::derived::normalized_grid(&fractions, &fraction_format);
    Plan {
        fraction: fractions
            .iter()
            .map(|v| grid.index_shown(*v, &fraction_format))
            .collect(),
        grid,
        order,
        shares: words.iter().map(|w| numbers::value(w.value / sum, share)).collect(),
    }
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let (Some(name_field), Some(value_field)) = (block.encoding.label.clone(), block.encoding.y.clone()) else {
        let reason = "Cannot draw this word cloud · It needs encoding.label and encoding.y";
        ctx.check(
            "source-error",
            Level::Error,
            "The word cloud has no label or no y",
            Some("Give encoding.label for the words and encoding.y for their values."),
            Some("chart.encoding".into()),
            None,
            serde_json::Value::Null,
        );
        return kit::failed(ctx, reason, RATIO);
    };
    let value_title = block.y.label.clone().unwrap_or_else(|| value_field.clone());
    let readout = block.readout_format(&block.y, ctx.locale);
    let share_format = Format {
        kind: Kind::Percent,
        unit: None,
        digits: 3,
        locale: ctx.locale,
    };
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut excluded = 0usize;
    let mut per_panel: Vec<Vec<Word>> = Vec::new();
    // Each panel's total over every row with a value.
    let mut totals: Vec<f64> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut words: Vec<Word> = Vec::new();
        // Whether a value is positive is the exact rows' call: a small one is zero on the grid.
        let skipped = graph::skipped(ctx, fi, members, Some(&value_field));
        for &i in members {
            let Some(text) = rows::text(&ctx.rows[i], &name_field) else {
                excluded += 1;
                continue;
            };
            match read_number(ctx, "word cloud", &mut failures[fi], i, &value_field) {
                Ok(Some(v)) if !skipped.zero.contains(&i) && !skipped.negative.contains(&i) => words.push(Word {
                    row: i,
                    text,
                    value: v.max(f64::MIN_POSITIVE),
                    size: 0.0,
                    color: ctx.style.ink,
                    spot: None,
                }),
                Ok(_) => excluded += 1,
                Err(()) => {}
            }
        }
        totals.push(words.iter().map(|w| w.value).sum());
        if words.len() > limits::max("wordcloud", "words per panel") {
            failures[fi] = Some(kit::over_limit(
                ctx,
                "wordcloud",
                "words per panel",
                words.len(),
                Some("chart.encoding.label"),
            ));
        }
        if words.is_empty() && failures[fi].is_none() {
            failures[fi] = Some("Cannot draw this word cloud · No row has a word and a positive value".into());
        }
        per_panel.push(words);
    }
    if failures.iter().all(Option::is_some) {
        let reason = failures.iter().flatten().next().cloned().unwrap_or_default();
        return kit::failed(ctx, &reason, RATIO);
    }
    disclose_excluded(ctx, excluded, "a word or a positive value");
    let lh = ctx.line_height();
    let width = ctx.opts.width;
    let title_room = if block.encoding.facet.is_some() { lh + 6.0 } else { 0.0 };
    let columns = super::facet_columns(facets.len(), width);
    let pw = (width - 24.0 * (columns as f64 - 1.0)) / columns as f64;
    let ratio = block.style.ratio.unwrap_or(RATIO);
    let panel_h = pw / ratio + title_room;
    let frames = super::grid_frames(facets.len(), width, 0.0, panel_h);
    let plots: Vec<Rect> = frames
        .iter()
        .map(|f| Rect::new(f.x, f.y + title_room, f.w, (f.h - title_room).max(1.0)))
        .collect();
    // The largest word sets the biggest size its width allows; ties go to the first.
    let top: Option<String> = ctx.derive(0, "wordcloud.top", || {
        let mut top: Option<(f64, String)> = None;
        for (words, failure) in per_panel.iter().zip(&failures) {
            if failure.is_none() {
                for w in words {
                    let value = numbers::shown(w.value, &readout);
                    if top.as_ref().is_none_or(|t| value > t.0) {
                        top = Some((value, w.text.clone()));
                    }
                }
            }
        }
        top.map(|t| t.1)
    });
    let smallest = plots.iter().map(|p| p.w).fold(f64::INFINITY, f64::min);
    let shortest = plots.iter().map(|p| p.h).fold(f64::INFINITY, f64::min);
    let lo = ctx.size();
    // The largest size suits the plot, and always lets the largest word fit its width.
    let mut hi = (smallest.min(shortest * 1.6) / 7.0).clamp(1.8 * lo, 64.0);
    if let Some(text) = &top {
        let em = ctx.style.font.advance_em(text).max(0.1);
        hi = hi.min((smallest - 2.0 * PAD - 2.0) / em);
    }
    let hi = hi.max(lo);
    let ramp = ctx.style.ramp;
    let plans: Vec<Option<Plan>> = (0..per_panel.len())
        .map(|fi| {
            failures[fi].is_none().then(|| {
                ctx.derive_projected(fi, "wordcloud.plan", || {
                    plan_of(&per_panel, &failures, fi, totals[fi], &share_format, &readout)
                })
            })
        })
        .collect();
    for (fi, words) in per_panel.iter_mut().enumerate() {
        let Some(plan) = &plans[fi] else { continue };
        for (w, &index) in words.iter_mut().zip(&plan.fraction) {
            let fraction = plan.grid.at(index);
            w.size = lo + (hi - lo) * fraction;
            let t = if hi > lo { fraction } else { 0.5 };
            w.color = ctx.readable(color::ramp(&ramp, RAMP_FROM + (1.0 - RAMP_FROM) * t));
        }
        let order = &plan.order;
        let area = plots[fi];
        let mut placed: Vec<Rect> = Vec::new();
        let mut spots: Vec<(usize, Rect)> = Vec::new();
        for &k in order {
            let (w, h) = {
                let b = ctx
                    .style
                    .font
                    .bounds(&words[k].text, words[k].size, 0.0, 0.0, Anchor::Middle);
                (b.w, b.h)
            };
            if let Some(at) = find_spot(&placed, w, h, &area) {
                let bx = Rect::new(at[0] - w / 2.0, at[1] - h / 2.0, w, h);
                placed.push(bx);
                spots.push((k, bx));
            }
        }
        // Centre the cloud in the plot; it only moves within the room it already has.
        if let Some(first) = placed.first() {
            let all = placed.iter().fold(*first, |a, r| a.union(r));
            let (dx, dy) = (area.center()[0] - all.center()[0], area.center()[1] - all.center()[1]);
            for (k, bx) in spots {
                words[k].spot = Some(Rect::new(bx.x + dx, bx.y + dy, bx.w, bx.h));
            }
        }
    }
    let mut panels: Vec<Panel> = Vec::new();
    let mut total_words = 0usize;
    let mut unplaced = 0usize;
    for (fi, ((name, _), frame)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *frame, title_room, name, reason));
            continue;
        }
        let plot = plots[fi];
        let words = &per_panel[fi];
        let Some(plan) = &plans[fi] else { continue };
        let mut items: Vec<Item> = Vec::new();
        let mut observations: Vec<Observation> = Vec::new();
        for (k, w) in words.iter().enumerate() {
            total_words += 1;
            let (at, area) = match w.spot {
                Some(bx) => (bx.center(), Some(bx)),
                None => ([plot.x, plot.y], None),
            };
            observations.push(Observation {
                row: Some(w.row),
                dots: vec![],
                series: None,
                at,
                wedge: None,
                area,
                radius: 0.0,
                x: at[0],
                readout: Readout {
                    heading: w.text.clone(),
                    lines: vec![
                        ReadoutLine {
                            label: None,
                            name: value_title.clone(),
                            value: numbers::value(w.value, &readout),
                            series: None,
                            names_series: false,
                            derived: false,
                        },
                        ReadoutLine {
                            label: None,
                            name: "Share".into(),
                            value: plan.shares[k].clone(),
                            series: None,
                            names_series: false,
                            derived: true,
                        },
                    ],
                }
                .into(),
            });
            match w.spot {
                Some(bx) => items.push(Item::Text(set(ctx, &w.text, w.size, bx.center(), w.color))),
                None => {
                    unplaced += 1;
                    ctx.drop_label(&w.text, DropReason::Collision, "word", None);
                }
            }
        }
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
    if unplaced > 0 {
        ctx.note.push(format!(
            "{unplaced} words found no room and are only in the table and the keyboard order"
        ));
    }
    ctx.note
        .push("word size follows the square root of the value, on one scale in every panel".into());
    ctx.summary = format!(
        "Word cloud of {} sized by {value_title}{}.",
        super::count(total_words, "word", "words"),
        match &top {
            Some(text) => format!(", the largest being {text}"),
            None => String::new(),
        }
    );
    Painted {
        height: super::grid_height(facets.len(), columns, panel_h, super::FACET_GAP_Y),
        panels,
        figure_items: vec![],
        legend: None,
        failure: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_box_sits_in_the_middle_and_later_boxes_clear_it() {
        let area = Rect::new(0.0, 0.0, 400.0, 200.0);
        let first = find_spot(&[], 80.0, 20.0, &area).unwrap();
        assert_eq!(first, [200.0, 100.0]);
        let a = Rect::new(160.0, 90.0, 80.0, 20.0);
        let second = find_spot(&[a], 80.0, 20.0, &area).unwrap();
        let b = Rect::new(second[0] - 40.0, second[1] - 10.0, 80.0, 20.0);
        assert!(!b.overlaps(&a, 0.0));
        assert!(b.inside(&area, 0.0));
    }

    #[test]
    fn a_box_wider_than_the_area_finds_no_place() {
        assert!(find_spot(&[], 500.0, 20.0, &Rect::new(0.0, 0.0, 400.0, 200.0)).is_none());
    }

    #[test]
    fn a_full_area_finds_no_place_and_ends() {
        let area = Rect::new(0.0, 0.0, 100.0, 40.0);
        let full = Rect::new(0.0, 0.0, 100.0, 40.0);
        assert!(find_spot(&[full], 30.0, 12.0, &area).is_none());
    }

    #[test]
    fn size_follows_the_square_root_between_the_extremes() {
        assert_eq!(size_of(1.0, [1.0, 16.0], 12.0, 40.0), 12.0);
        assert_eq!(size_of(16.0, [1.0, 16.0], 12.0, 40.0), 40.0);
        // The root of 4 is a third of the way from the root of 1 to the root of 16.
        assert!((size_of(4.0, [1.0, 16.0], 12.0, 40.0) - (12.0 + 28.0 / 3.0)).abs() < 1e-9);
        assert_eq!(size_of(5.0, [5.0, 5.0], 12.0, 40.0), 26.0);
    }
}
