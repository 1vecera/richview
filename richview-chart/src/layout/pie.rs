//! Pie charts (FEA-CH-08, FEA-CH-09, FEA-CH-70): slices from 12 o'clock, clockwise in source
//! order. Each slice is named beside it, without a leader, when every name stands clear of the
//! disk, of the other names and of the panel; otherwise the whole chart takes a key: wrapped below
//! the disk, or in a column beside it when the room is wider than tall (S27-875, S27-1017).

use super::graph;
use super::{Ctx, Painted, Style};
use crate::diag::Level;
use crate::geometry::{
    Anchor, Face, Fill, Item, Observation, Panel, Path, Readout, ReadoutLine, Rect, Role, Stroke, Targeting, Wedge,
};
use crate::numbers::{self, Format, Kind};
use crate::rows::{self, Num};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::f64::consts::{FRAC_PI_2, TAU};

/// Share text: `<0.1%` for a positive share that rounds to zero and `>99.9%` for a partial share
/// that rounds to a whole.
pub fn share_text(share: f64, locale: numbers::Locale) -> String {
    if share > 0.0 && share < 0.0005 {
        return "<0.1%".into();
    }
    if share < 1.0 && share > 0.9995 {
        return ">99.9%".into();
    }
    let f = Format {
        kind: Kind::Percent,
        unit: None,
        digits: 3,
        locale,
    };
    let pct = (share * 1000.0).round() / 10.0;
    let text = numbers::grouped(
        pct,
        if pct.fract() == 0.0 && pct >= 10.0 { 0 } else { 1 },
        f.locale,
        false,
    );
    format!("{text}%")
}

/// An arc from `a0` to `a1` (radians, clockwise from 12 o'clock) as cubic Béziers, cut at every
/// quarter turn so the arc's extremes are end points: its bounds are then exact (the geometry
/// model's own readers bound a path by its end points).
pub(super) fn arc(path: &mut Path, c: [f64; 2], r: f64, a0: f64, a1: f64) {
    let point = |a: f64| [c[0] + r * (a - FRAC_PI_2).cos(), c[1] + r * (a - FRAC_PI_2).sin()];
    // Breakpoints: the start, every multiple of a quarter turn strictly between, the end.
    let (lo, hi) = (a0.min(a1), a0.max(a1));
    let mut cuts = vec![a0];
    let mut k = (lo / FRAC_PI_2).floor() as i64 + 1;
    let mut inner = Vec::new();
    while (k as f64) * FRAC_PI_2 < hi - 1e-9 {
        let a = k as f64 * FRAC_PI_2;
        if a > lo + 1e-9 {
            inner.push(a);
        }
        k += 1;
    }
    if a1 < a0 {
        inner.reverse();
    }
    cuts.extend(inner);
    cuts.push(a1);
    for pair in cuts.windows(2) {
        let (s, e) = (pair[0], pair[1]);
        let k = 4.0 / 3.0 * ((e - s) / 4.0).tan();
        let (p0, p3) = (point(s), point(e));
        let t0 = [-(s - FRAC_PI_2).sin(), (s - FRAC_PI_2).cos()];
        let t1 = [-(e - FRAC_PI_2).sin(), (e - FRAC_PI_2).cos()];
        path.curve_to(
            [p0[0] + k * r * t0[0], p0[1] + k * r * t0[1]],
            [p3[0] - k * r * t1[0], p3[1] - k * r * t1[1]],
            p3,
        );
    }
}

/// Width of the surface-coloured gap between slices.
const SEAM: f64 = 2.0;

pub(super) fn wedge_path(c: [f64; 2], r: f64, inner: f64, a0: f64, a1: f64) -> Path {
    let mut path = Path::new();
    if inner > 0.0 {
        // A ring slice: out along the outer arc, back along the inner one. A full ring is cut
        // open at 12 o'clock with no width, so it stays one simple outline.
        let at = |radius: f64, a: f64| {
            [
                c[0] + radius * (a - FRAC_PI_2).cos(),
                c[1] + radius * (a - FRAC_PI_2).sin(),
            ]
        };
        path.move_to(at(r, a0));
        arc(&mut path, c, r, a0, a1);
        path.line_to(at(inner, a1));
        arc(&mut path, c, inner, a1, a0);
        path.close();
        return path;
    }
    if (a1 - a0) >= TAU - 1e-9 {
        path.move_to([c[0], c[1] - r]);
        arc(&mut path, c, r, 0.0, TAU);
        path.close();
        return path;
    }
    path.move_to(c);
    path.line_to([c[0] + r * (a0 - FRAC_PI_2).cos(), c[1] + r * (a0 - FRAC_PI_2).sin()]);
    arc(&mut path, c, r, a0, a1);
    path.close();
    path
}

/// What a panel's slices read out, from the exact totals: the share of each category and its
/// total. The shares set the width of the labels, which decides labels or a key (TYP-36).
#[derive(Serialize, Deserialize)]
struct Texts {
    shares: Vec<String>,
    values: Vec<String>,
}

/// Cumulative shares on one normalized grid, shared by wedges and their label anchors.
#[derive(Serialize, Deserialize)]
struct Angles {
    grid: crate::payload::Quant,
    edges: Vec<[u64; 2]>,
}

impl Angles {
    fn at(&self, series: usize) -> [f64; 2] {
        self.edges[series].map(|k| self.grid.at(k) * TAU)
    }
}

fn angles(ctx: &Ctx, panel: usize, totals: &[f64], total: f64, readout: &Format) -> Angles {
    ctx.derive_visible_projected(panel, "pie.angles", || {
        let last = totals
            .iter()
            .enumerate()
            .rposition(|(k, v)| !ctx.is_hidden(k) && *v > 0.0);
        let mut running = 0.0;
        let edges: Vec<[f64; 2]> = totals
            .iter()
            .enumerate()
            .map(|(k, v)| {
                let start = running;
                if total > 0.0 && total.is_finite() && !ctx.is_hidden(k) && *v > 0.0 {
                    running = if Some(k) == last {
                        1.0
                    } else {
                        (running + v / total).min(1.0)
                    };
                }
                [start, running]
            })
            .collect();
        let positions: Vec<_> = edges.iter().flatten().copied().collect();
        let format = Format {
            kind: Kind::Percent,
            unit: None,
            ..readout.clone()
        };
        let grid = crate::derived::normalized_grid(&positions, &format);
        Angles {
            edges: edges.iter().map(|e| e.map(|v| grid.index_shown(v, &format))).collect(),
            grid,
        }
    })
}

/// What the key and the summary say, from the exact totals of every panel that draws.
#[derive(Serialize, Deserialize)]
struct Overall {
    /// The share of each category over all panels, for the key of a single pie.
    key: Vec<String>,
    /// Categories with a positive total, and the largest with its share.
    slices: usize,
    largest: Option<(usize, String)>,
    overflow: bool,
}

pub(super) fn layout(ctx: &mut Ctx, facets: &[(Option<String>, Vec<usize>)]) -> Painted {
    let block = ctx.block;
    let x_field = block.encoding.x.clone().unwrap_or_default();
    let y_field = block.encoding.y.clone().unwrap_or_default();
    let names = rows::distinct(ctx.rows, &x_field);
    ctx.set_series(&names);
    let width = ctx.opts.width;
    let readout = block.readout_format(&block.y, ctx.locale);
    let lh = ctx.line_height();
    let size = ctx.size();
    let font = ctx.style.font.clone();
    let title_room = if block.encoding.facet.is_some() { lh + 6.0 } else { 0.0 };
    let columns = super::facet_columns(facets.len(), width);
    let pw = (width - 24.0 * (columns as f64 - 1.0)) / columns as f64;
    let ratio = block.style.ratio.unwrap_or(1.0);
    // Totals per panel per category.
    let mut panels_data = Vec::new();
    let mut gaps = 0usize;
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut totals = vec![0.0f64; names.len()];
        let mut seen = vec![false; names.len()];
        // Which values are zero or negative is the exact rows' call: a small one is zero on the grid.
        let skipped = graph::skipped(ctx, fi, members, Some(&y_field));
        for &i in members {
            let Some(name) = rows::text(&ctx.rows[i], &x_field) else {
                continue;
            };
            let c = names.iter().position(|n| *n == name).unwrap_or(0);
            match rows::num(&ctx.rows[i], &y_field) {
                Num::Value(v) if skipped.negative.contains(&i) => {
                    ctx.check(
                        "negative-pie-value",
                        Level::Error,
                        format!(
                            "Row {} has a negative value {}; a slice cannot be negative, so the row is skipped",
                            i + 1,
                            numbers::value(v, &block.label_format(&block.y, ctx.locale))
                        ),
                        Some("Pies show parts of a positive whole; use a bar chart for signed values."),
                        Some(Ctx::row_path(i, &y_field)),
                        None,
                        json!({"row": i}),
                    );
                }
                Num::Value(v) => {
                    let v = if ctx.opts.hidden.is_empty() {
                        v
                    } else {
                        numbers::shown(v, &readout)
                    };
                    totals[c] += if skipped.zero.contains(&i) {
                        0.0
                    } else {
                        v.max(f64::MIN_POSITIVE)
                    };
                    seen[c] = true;
                }
                Num::Missing => gaps += 1,
                Num::Underflow(t) => ctx.underflow(i, &y_field, &t),
                Num::Invalid(t) => {
                    ctx.check(
                        "non-numeric",
                        Level::Error,
                        format!("Row {} has {y_field} '{t}', which is not a number", i + 1),
                        None,
                        Some(Ctx::row_path(i, &y_field)),
                        None,
                        json!({"row": i}),
                    );
                }
            }
        }
        panels_data.push((totals, seen));
    }
    // A row with no value gets no slice, and the author is told (RMD-SPEC, Missing values).
    if gaps > 0 {
        ctx.check(
            "missing-observations",
            Level::Info,
            format!(
                "{} no {y_field} value and {} no slice",
                super::count(gaps, "row has", "rows have"),
                super::verb(gaps, "gets", "get")
            ),
            None,
            None,
            None,
            json!({"count": gaps}),
        );
    }
    let mut panels = Vec::new();
    let mut figure_items = Vec::new();
    let mut panel_h_max = 0.0f64;
    // Every panel has the box the first pass plans names in: the width of a column and its height.
    let panel_box = [pw, pw / ratio];
    // First pass: the names each panel wants beside its slices.
    let mut totals_by_panel = Vec::new();
    let mut wanted_by_panel = Vec::new();
    let mut texts_by_panel: Vec<Texts> = Vec::new();
    let mut angles_by_panel = Vec::new();
    for (fi, (totals, _)) in panels_data.iter().enumerate() {
        let total: f64 = totals
            .iter()
            .enumerate()
            .filter(|(c, _)| !ctx.is_hidden(*c))
            .map(|(_, v)| v)
            .sum();
        let texts: Texts = ctx.derive_visible(fi, "pie.texts", || Texts {
            shares: totals
                .iter()
                .enumerate()
                .map(|(k, v)| {
                    if !ctx.is_hidden(k) && total > 0.0 && total.is_finite() {
                        share_text(v / total, ctx.locale)
                    } else {
                        String::new()
                    }
                })
                .collect(),
            values: totals.iter().map(|v| numbers::value(*v, &readout)).collect(),
        });
        let angles = angles(ctx, fi, totals, total, &readout);
        wanted_by_panel.push(wanted_labels(ctx, &angles, totals, total, &texts.shares));
        totals_by_panel.push(total);
        texts_by_panel.push(texts);
        angles_by_panel.push(angles);
    }
    // Then one plan for every panel: the names stand beside their slices in all of them, at one
    // disk size, or the whole chart takes the key (never a mixed view).
    let planned = plan_labels(&wanted_by_panel, panel_box);
    let key_needed = planned.is_none();
    let plans: Vec<(f64, Option<Plan>)> = match planned {
        Some(found) => totals_by_panel
            .iter()
            .copied()
            .zip(found.into_iter().map(Some))
            .collect(),
        None => totals_by_panel.iter().map(|total| (*total, None)).collect(),
    };
    let overall: Overall = ctx.derive_visible(0, "pie.overall", || {
        let totals: Vec<f64> = (0..names.len())
            .map(|k| panels_data.iter().map(|(t, _)| t[k]).sum())
            .collect();
        let total: f64 = totals
            .iter()
            .enumerate()
            .filter(|(k, _)| !ctx.is_hidden(*k))
            .map(|(_, v)| v)
            .sum();
        let key = totals
            .iter()
            .enumerate()
            .map(|(k, v)| {
                if !ctx.is_hidden(k) && facets.len() == 1 && total > 0.0 && total.is_finite() {
                    share_text(v / total, ctx.locale)
                } else {
                    String::new()
                }
            })
            .collect();
        // The summary describes the panels that draw; failed ones are counted, not described.
        let drawn: Vec<&Vec<f64>> = panels_data
            .iter()
            .zip(&plans)
            .filter(|(_, (total, _))| total.is_finite() && *total > 0.0)
            .map(|((t, _), _)| t)
            .collect();
        let all_totals: Vec<f64> = (0..names.len())
            .map(|k| {
                if ctx.is_hidden(k) {
                    0.0
                } else {
                    drawn.iter().map(|t| t[k]).sum()
                }
            })
            .collect();
        let grand: f64 = all_totals.iter().sum();
        let largest = all_totals.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1));
        Overall {
            key,
            slices: all_totals.iter().filter(|v| **v > 0.0).count(),
            largest: match largest {
                Some((k, v)) if grand.is_finite() && grand > 0.0 => Some((k, share_text(v / grand, ctx.locale))),
                _ => None,
            },
            overflow: !grand.is_finite(),
        }
    });
    // Each key entry: a name and, for one disk, its share.
    let key_labels: Vec<String> = names
        .iter()
        .enumerate()
        .map(|(k, name)| match overall.key.get(k) {
            Some(share) if !share.is_empty() => format!("{name} {share}"),
            _ => name.clone(),
        })
        .collect();
    // In a room wider than tall the key stands in a column beside the disks, which take the width
    // it leaves (S27-875); otherwise it wraps below them.
    let beside = key_needed && ctx.opts.aside;
    let key_w = key_labels
        .iter()
        .map(|label| KEY_SWATCH + KEY_SWATCH_GAP + font.width(label, size))
        .fold(0.0, f64::max);
    let area = if beside {
        (width - key_w - KEY_GAP).max(MIN_DISK_AREA)
    } else {
        width
    };
    let columns = super::facet_columns(facets.len(), area);
    let pw = (area - 24.0 * (columns as f64 - 1.0)) / columns as f64;
    let disk_box = pw / ratio;
    let frames = super::grid_frames(facets.len(), area, 0.0, disk_box + title_room);
    for (fi, (((name, _), frame), ((totals, _), (total, plan)))) in facets
        .iter()
        .zip(&frames)
        .zip(panels_data.iter().zip(&plans))
        .enumerate()
    {
        let texts = &texts_by_panel[fi];
        let plot = Rect::new(frame.x, frame.y + title_room, frame.w, frame.h - title_room);
        let mut items = Vec::new();
        let mut observations = Vec::new();
        if !total.is_finite() {
            ctx.check(
                "value-overflow",
                Level::Error,
                "A category total is too large to represent",
                None,
                Some("chart.encoding.y".into()),
                None,
                Value::Null,
            );
            let p = super::failed_panel(ctx, plot, "Cannot draw this pie · Category total is too large");
            panels.push(p);
            continue;
        }
        if *total <= 0.0 {
            let p = super::failed_panel(ctx, plot, "No positive values to show");
            panels.push(p);
            continue;
        }
        // With names beside the slices the plan has solved the radius together with their room.
        let r = match plan {
            Some(plan) => plan.radius,
            None => ((plot.w.min(plot.h) / 2.0) - DISK_MARGIN).max(24.0),
        };
        let c = [plot.x + plot.w / 2.0, plot.y + plot.h / 2.0];
        // `style.inner` is the hole's share of the radius: a donut (0 is a pie).
        let inner = r * block.style_number("inner").unwrap_or(0.0).clamp(0.0, 0.9);
        let surface = Style::rgba(ctx.style.surface);
        for (k, v) in totals.iter().enumerate() {
            if ctx.is_hidden(k) || *v <= 0.0 {
                continue;
            }
            let [a, end] = angles_by_panel[fi].at(k);
            let sweep = end - a;
            let color = Style::rgba(ctx.series_color(k));
            let hatch = ctx.series[k].hatch;
            items.push(Item::Area {
                path: wedge_path(c, r, inner, a, end),
                fill: Fill {
                    color,
                    hatch,
                    hatch_ink: hatch.map(|_| surface),
                    hatch_scale: 1.0,
                },
                role: Role::Mark,
                series: Some(k),
                obs: Some(observations.len()),
            });
            if sweep < TAU {
                // A gap in the surface colour separates slices (never an ink stroke).
                let edge = [c[0] + r * (a - FRAC_PI_2).cos(), c[1] + r * (a - FRAC_PI_2).sin()];
                let from = [
                    c[0] + inner * (a - FRAC_PI_2).cos(),
                    c[1] + inner * (a - FRAC_PI_2).sin(),
                ];
                items.push(Item::Line {
                    path: Path::polyline(&[from, edge]),
                    stroke: Stroke::solid(surface, SEAM),
                    role: Role::Mark,
                    series: Some(k),
                    obs: None,
                });
            }
            let mid = a + sweep / 2.0;
            let lines = vec![
                ReadoutLine {
                    name: block
                        .y
                        .label
                        .clone()
                        .unwrap_or_else(|| super::cartesian::readable_name(&y_field)),
                    value: texts.values[k].clone(),
                    series: Some(k),
                    names_series: false,
                    derived: true,
                },
                ReadoutLine {
                    name: "Share".into(),
                    value: texts.shares[k].clone(),
                    series: None,
                    names_series: false,
                    derived: true,
                },
            ];
            let anchor_r = if inner > 0.0 { (inner + r) / 2.0 } else { r * 0.6 };
            let at = [
                c[0] + anchor_r * (mid - FRAC_PI_2).cos(),
                c[1] + anchor_r * (mid - FRAC_PI_2).sin(),
            ];
            observations.push(Observation {
                row: None,
                dots: vec![],
                series: Some(k),
                at,
                wedge: Some(Wedge {
                    center: c,
                    radius: r,
                    inner,
                    start: a,
                    end,
                }),
                area: None,
                radius: 0.0,
                x: at[0],
                readout: Readout {
                    heading: names[k].clone(),
                    lines,
                }
                .into(),
            });
        }
        if let Some(plan) = plan {
            for l in &plan.labels {
                // The box stands beside its slice; the text is centred on the box's line.
                items.push(Item::Text(ctx.text_mid(
                    &l.text,
                    c[0] + l.rect.x,
                    c[1] + l.rect.y + l.rect.h / 2.0,
                    Anchor::Start,
                    Face::Data,
                    Role::Label,
                    ctx.style.ink,
                )));
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
        if let Some(name) = name {
            let t = ctx.text(
                name,
                frame.x,
                frame.y + font.baseline_offset(size),
                Anchor::Start,
                Face::Data,
                Role::PanelTitle,
                ctx.style.ink,
            );
            panel.items.push(Item::Text(crate::geometry::Text { weight: 600, ..t }));
        }
        panel_h_max = panel_h_max.max(frame.bottom());
        panels.push(panel);
    }
    let mut height = super::grid_height(facets.len(), columns, disk_box + title_room, super::FACET_GAP_Y);
    if beside {
        // One entry a row, from the top of the disks.
        let x = area + KEY_GAP;
        let mut y = 0.0;
        for (k, label) in key_labels.iter().enumerate() {
            key_entry(ctx, &mut figure_items, k, label, x, y);
            y += lh + 4.0;
        }
        height = height.max(y - 4.0);
    } else if key_needed {
        // A wrapped key below the disk; the chart grows taller instead of abbreviating.
        let mut x = 0.0;
        let mut y = height + 12.0;
        for (k, label) in key_labels.iter().enumerate() {
            let w = KEY_SWATCH + KEY_SWATCH_GAP + font.width(label, size) + 18.0;
            if x > 0.0 && x + w > width {
                x = 0.0;
                y += lh + 4.0;
            }
            key_entry(ctx, &mut figure_items, k, label, x, y);
            x += w;
        }
        height = y + lh + 4.0;
    }
    let failed = panels_data.len()
        - plans
            .iter()
            .filter(|(total, _)| total.is_finite() && *total > 0.0)
            .count();
    ctx.summary = match &overall.largest {
        _ if overall.overflow => "A category total is too large to represent.".into(),
        Some((k, share)) => format!(
            "{}; the largest is {} at {share}.",
            super::count(overall.slices, "slice", "slices"),
            names[*k],
        ),
        None => "No positive values to show.".into(),
    };
    if failed > 0 && panels_data.len() > 1 {
        ctx.summary.push_str(&format!(
            " {failed} of {} panels could not be drawn.",
            panels_data.len()
        ));
    }
    Painted {
        height,
        panels,
        figure_items,
        legend: None,
        failure: None,
    }
}

/// A key entry's swatch, the gap after it, and the gap between the disks and a key beside them.
const KEY_SWATCH: f64 = 9.0;
const KEY_SWATCH_GAP: f64 = 6.0;
const KEY_GAP: f64 = 24.0;
/// The least width the disks keep beside their key.
const MIN_DISK_AREA: f64 = 48.0;

/// One key entry, its swatch and its text, its row's top at `y`.
fn key_entry(ctx: &Ctx, items: &mut Vec<Item>, k: usize, label: &str, x: f64, y: f64) {
    let lh = ctx.line_height();
    items.push(Item::Rect {
        rect: Rect::new(x, y + lh / 2.0 - KEY_SWATCH / 2.0, KEY_SWATCH, KEY_SWATCH),
        fill: Fill::solid(Style::rgba(ctx.series_color(k))),
        stroke: None,
        radius: [1.0; 4],
        role: Role::Key,
        series: Some(k),
        obs: None,
    });
    let mut t = ctx.text_mid(
        label,
        x + KEY_SWATCH + KEY_SWATCH_GAP,
        y + lh / 2.0,
        Anchor::Start,
        Face::Data,
        Role::Key,
        ctx.style.ink,
    );
    t.series = Some(k);
    items.push(Item::Text(t));
}

/// A character of a name at the reference size, so the choice between names beside the slices
/// and a key is the same in every style (DEL-52).
const REFERENCE_CHAR: f64 = 9.0;
/// A line of a name at the reference size: above the tallest line of the built-in faces.
const REFERENCE_LINE: f64 = 18.0;
/// The gap between a disk's rim and the nearest edge of a name beside it. A hovered slice's soft
/// band reaches 8 px past its rim, and a name must stay clear of it.
const LABEL_RIM_GAP: f64 = 12.0;
/// The least gap between two names.
const LABEL_APART: f64 = 4.0;
/// The least gap between a name and the edge of its panel.
const LABEL_EDGE: f64 = 2.0;
/// The room a disk keeps to its panel's edge where no name stands.
const DISK_MARGIN: f64 = 4.0;
/// The smallest disk that keeps names beside the slices: this share of the panel's radius, and not
/// under this many pixels; a smaller one takes the key.
const LEAST_DISK_SHARE: f64 = 0.45;
const LEAST_DISK: f64 = 40.0;

/// A slice's name and share as the panel wants it beside the slice.
struct Wanted {
    text: String,
    /// The middle of the slice, clockwise from 12 o'clock.
    angle: f64,
    /// The text's box.
    size: [f64; 2],
    /// The box the choice reckons with: at least the text's, and at least a reference measure.
    reference: [f64; 2],
}

/// A name placed beside its slice.
struct Label {
    text: String,
    /// The text's box, from the disk's centre.
    rect: Rect,
}

struct Plan {
    labels: Vec<Label>,
    /// The disk's radius, solved together with the room the names take.
    radius: f64,
}

/// The names a panel wants beside its slices: every visible positive slice, in source order. A
/// panel that cannot draw wants none.
fn wanted_labels(ctx: &Ctx, angles: &Angles, totals: &[f64], total: f64, shares: &[String]) -> Vec<Wanted> {
    if !(total > 0.0 && total.is_finite()) {
        return Vec::new();
    }
    let lh = ctx.line_height();
    let mut wanted = Vec::new();
    for (k, v) in totals.iter().enumerate() {
        if ctx.is_hidden(k) || *v <= 0.0 {
            continue;
        }
        let share_label = shares.get(k).cloned().unwrap_or_default();
        let [a, b] = angles.at(k);
        let text = format!("{} {share_label}", ctx.series[k].name);
        // Whether names sit beside the slices or in a key must not depend on the style's face
        // (DEL-52), so the choice reckons with the larger of the measured text and a reference.
        let width = ctx.text_width(&text);
        let reference = width.max(text.chars().count() as f64 * REFERENCE_CHAR);
        wanted.push(Wanted {
            angle: a + (b - a) / 2.0,
            size: [width, lh],
            reference: [reference, lh.max(REFERENCE_LINE)],
            text,
        });
    }
    wanted
}

/// The square of the distance from the origin to the nearest point of `rect`.
fn clearance_sq(rect: &Rect) -> f64 {
    let dx = rect.x.max(-rect.right()).max(0.0);
    let dy = rect.y.max(-rect.bottom()).max(0.0);
    dx * dx + dy * dy
}

/// The box of a name of `size` beside a disk of radius `r` centred on the origin, along `angle`
/// (clockwise from 12 o'clock). The box grows away from the slice: its edge towards the disk is the
/// edge facing the centre, and it slides outward along the slice's middle until its nearest point
/// is [`LABEL_RIM_GAP`] from the rim, so the gap is the same for every name, at every angle. The
/// ray through the slice's middle always meets the box.
fn name_box(angle: f64, r: f64, [w, h]: [f64; 2]) -> Rect {
    let (sx, sy) = ((angle - FRAC_PI_2).cos(), (angle - FRAC_PI_2).sin());
    // The box whose anchor on the ray is `reach` from the centre: a side facing the disk at the
    // anchor, the box extending away from it, centred on the ray where the ray is level or plumb.
    let at = |reach: f64| {
        Rect::new(
            reach * sx - w * (1.0 - sx) / 2.0,
            reach * sy - h * (1.0 - sy) / 2.0,
            w,
            h,
        )
    };
    let want = (r + LABEL_RIM_GAP) * (r + LABEL_RIM_GAP);
    // The clearance grows with the reach (the box contains the centre at reach 0), and the
    // anchor at the rim plus the box's two sides is clear for certain.
    let (mut near, mut far) = (0.0, r + LABEL_RIM_GAP + w + h);
    for _ in 0..40 {
        let mid = (near + far) / 2.0;
        if clearance_sq(&at(mid)) < want {
            near = mid;
        } else {
            far = mid;
        }
    }
    at(far)
}

/// Names beside the slices of every panel, at one disk radius; `None` when they do not all fit
/// clearly, so the chart takes the key. `[w, h]` is each panel's box, the disk at its centre.
///
/// The radius is the largest at which the boxes the choice reckons with stay in the panel: how
/// far a box reaches grows with the radius, so one bisection finds it. Names are then checked
/// at that radius, as reckoned and as drawn, for overlap and for the panel's edge. A name is never
/// moved from its slice to make room.
fn plan_labels(panels: &[Vec<Wanted>], [w, h]: [f64; 2]) -> Option<Vec<Plan>> {
    let full = w.min(h) / 2.0;
    let widest = full - DISK_MARGIN;
    let least = (full * LEAST_DISK_SHARE).max(LEAST_DISK);
    // Names stay clear of the chart's bottom edge as the figure's lowest text must, so the figure
    // is never made taller for them.
    let frame = Rect::new(
        LABEL_EDGE - w / 2.0,
        LABEL_EDGE - h / 2.0,
        w - 2.0 * LABEL_EDGE,
        h - LABEL_EDGE - super::finish::BOTTOM_CLEARANCE,
    );
    let mut radius = widest;
    for wanted in panels.iter().filter(|p| !p.is_empty()) {
        let fits = |r: f64| {
            wanted
                .iter()
                .all(|l| name_box(l.angle, r, l.reference).inside(&frame, 0.0))
        };
        if widest < least || !fits(least) {
            return None;
        }
        if !fits(radius) {
            let (mut lo, mut hi) = (least, radius);
            for _ in 0..32 {
                let mid = (lo + hi) / 2.0;
                if fits(mid) {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            radius = lo;
        }
    }
    panels
        .iter()
        .map(|wanted| {
            let boxes = |size: fn(&Wanted) -> [f64; 2]| -> Vec<Rect> {
                wanted.iter().map(|l| name_box(l.angle, radius, size(l))).collect()
            };
            let (reference, drawn) = (boxes(|l| l.reference), boxes(|l| l.size));
            let clear = |set: &[Rect]| {
                set.iter().all(|b| b.inside(&frame, 0.0))
                    && set
                        .iter()
                        .enumerate()
                        .all(|(i, a)| set[i + 1..].iter().all(|b| !a.overlaps(b, -LABEL_APART)))
            };
            (clear(&reference) && clear(&drawn)).then(|| Plan {
                labels: wanted
                    .iter()
                    .zip(drawn)
                    .map(|(l, rect)| Label {
                        text: l.text.clone(),
                        rect,
                    })
                    .collect(),
                radius,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        DISK_MARGIN, FRAC_PI_2, LABEL_EDGE, LABEL_RIM_GAP, REFERENCE_CHAR, REFERENCE_LINE, Rect, Wanted, clearance_sq,
        name_box, plan_labels, share_text,
    };
    use crate::block::ChartBlock;
    use crate::layout::{ChartTheme, Options, layout};
    use crate::numbers::Locale;
    use serde_json::json;

    #[test]
    fn shares_round_honestly() {
        assert_eq!(share_text(0.0001, Locale::EnUs), "<0.1%");
        assert_eq!(share_text(0.99991, Locale::EnUs), ">99.9%");
        assert_eq!(share_text(0.25, Locale::EnUs), "25%");
        assert_eq!(share_text(0.123, Locale::EnUs), "12.3%");
    }

    #[test]
    fn slices_follow_source_order_and_negative_rows_are_skipped() {
        let block =
            ChartBlock::from_value(&json!({"type": "pie", "title": "T", "encoding": {"x": "c", "y": "v"}})).unwrap();
        let rows = vec![
            json!({"c": "B", "v": 1}),
            json!({"c": "A", "v": 3}),
            json!({"c": "C", "v": -2}),
        ];
        let g = layout(
            &block,
            &rows,
            &ChartTheme::default(),
            &Options {
                width: 480.0,
                ..Options::default()
            },
        );
        assert!(g.failure.is_none());
        let headings: Vec<_> = g.panels[0]
            .observations
            .iter()
            .map(|o| o.readout.heading.clone())
            .collect();
        assert_eq!(headings, ["B", "A"]);
        assert!(g.checks.iter().any(|c| c.kind == "negative-pie-value"));
        assert!(g.summary.contains("A at 75%"));
    }

    #[test]
    fn a_key_stands_beside_the_disk_when_the_room_is_wider_than_tall() {
        // Names this long never fit around the disk, so the pie takes a key (S27-875).
        use crate::geometry::{Geometry, Item, Rect, Role};
        let block =
            ChartBlock::from_value(&json!({"type": "pie", "title": "T", "encoding": {"x": "c", "y": "v"}})).unwrap();
        let rows: Vec<_> = [
            "Search engines",
            "Social networks",
            "Direct visits",
            "Referral partners",
            "Email campaigns",
        ]
        .iter()
        .enumerate()
        .map(|(i, c)| json!({"c": c, "v": i + 1}))
        .collect();
        let at = |aside| {
            layout(
                &block,
                &rows,
                &ChartTheme::default(),
                &Options {
                    width: 480.0,
                    aside,
                    ..Options::default()
                },
            )
        };
        let (under, beside) = (at(false), at(true));
        let keys = |g: &Geometry| -> Vec<Rect> {
            g.figure_items
                .iter()
                .filter(|i| i.role() == Role::Key)
                .filter_map(Item::bounds)
                .collect()
        };
        let texts = |g: &Geometry| -> Vec<String> {
            g.figure_items
                .iter()
                .filter_map(|i| match i {
                    Item::Text(t) if t.role == Role::Key => Some(t.text.clone()),
                    _ => None,
                })
                .collect()
        };
        let disk = |g: &Geometry| g.panels[0].plot;
        // Under the disk, the key makes the chart taller than the disk's box.
        assert!(!keys(&under).is_empty() && keys(&under).iter().all(|k| k.y >= disk(&under).bottom()));
        assert!(under.height > disk(&under).h + 20.0);
        // Beside it, every entry stands right of the disk's box, one a row from its top, and the
        // chart is as tall as the box or the key, whichever is taller.
        let side = keys(&beside);
        assert!(
            side.iter()
                .all(|k| k.x >= disk(&beside).right() + 20.0 && k.right() <= 480.5),
            "{side:?}"
        );
        assert!(side.iter().map(|k| k.y).fold(f64::INFINITY, f64::min) < 12.0);
        let key_bottom = side.iter().map(Rect::bottom).fold(0.0, f64::max);
        assert!(
            (beside.height - disk(&beside).h.max(key_bottom)).abs() < 4.5,
            "{} {:?}",
            beside.height,
            disk(&beside)
        );
        assert!(beside.height < under.height - 100.0);
        // The same entries in the same order, each a name and its share.
        assert_eq!(texts(&beside), texts(&under));
        assert_eq!(texts(&beside)[0], "Search engines 6.7%");
    }

    #[test]
    fn a_name_stands_a_constant_gap_from_the_rim_at_every_angle() {
        for degrees in 0..72 {
            let angle = f64::from(degrees) * 5.0_f64.to_radians();
            for size in [[20.0, 15.0], [63.0, 15.0], [108.0, 18.0], [190.0, 18.0]] {
                for r in [40.0, 120.0, 300.0] {
                    let found = name_box(angle, r, size);
                    let gap = clearance_sq(&found).sqrt() - r;
                    assert!(
                        (gap - LABEL_RIM_GAP).abs() < 1e-6,
                        "{degrees}x5 deg {size:?} r {r}: gap {gap}"
                    );
                    assert!(
                        (found.w, found.h) == (size[0], size[1]),
                        "the box keeps the text's size"
                    );
                    // The ray through the slice's middle meets the box, so the name is on its slice.
                    let (dx, dy) = ((angle - FRAC_PI_2).cos(), (angle - FRAC_PI_2).sin());
                    let met = (0..4000).any(|step| {
                        let t = f64::from(step) * 0.25;
                        found.inflate(1e-6).contains([t * dx, t * dy])
                    });
                    assert!(met, "{degrees}x5 deg {size:?} r {r}: the ray misses {found:?}");
                }
            }
        }
    }

    fn wanted(angle_degrees: f64, text: &str) -> Wanted {
        let width = text.chars().count() as f64 * REFERENCE_CHAR;
        Wanted {
            text: text.into(),
            angle: angle_degrees.to_radians(),
            size: [width * 0.75, 15.0],
            reference: [width, REFERENCE_LINE],
        }
    }

    #[test]
    fn names_that_fit_take_the_largest_disk_the_panel_allows() {
        let panel = vec![
            wanted(72.0, "Ads 40%"),
            wanted(198.0, "SEO 30%"),
            wanted(288.0, "Mail 20%"),
        ];
        let plans = plan_labels(&[panel], [390.0, 390.0]).expect("these names fit");
        let plan = &plans[0];
        assert!(plan.radius > 0.45 * 195.0 && plan.radius < 195.0 - DISK_MARGIN);
        // One more pixel of radius would push a reference box out of the panel.
        let panel = vec![
            wanted(72.0, "Ads 40%"),
            wanted(198.0, "SEO 30%"),
            wanted(288.0, "Mail 20%"),
        ];
        let frame = Rect::new(
            LABEL_EDGE - 195.0,
            LABEL_EDGE - 195.0,
            390.0 - 2.0 * LABEL_EDGE,
            390.0 - LABEL_EDGE - crate::layout::finish::BOTTOM_CLEARANCE,
        );
        assert!(
            panel
                .iter()
                .all(|l| name_box(l.angle, plan.radius, l.reference).inside(&frame, 0.0))
        );
        assert!(
            panel
                .iter()
                .any(|l| !name_box(l.angle, plan.radius + 1.0, l.reference).inside(&frame, 0.0))
        );
        for (label, want) in plan.labels.iter().zip(&panel) {
            assert_eq!(label.text, want.text);
            assert!((clearance_sq(&label.rect).sqrt() - plan.radius - LABEL_RIM_GAP).abs() < 1e-6);
        }
    }

    #[test]
    fn names_that_would_meet_each_other_leave_no_plan() {
        // Four slices of 2 % at the top, names a good deal wider than the arc they sit on.
        let crowd: Vec<_> = [3.6, 10.8, 18.0, 25.2]
            .iter()
            .enumerate()
            .map(|(i, a)| wanted(*a, &format!("Name {i} 2%")))
            .collect();
        assert!(plan_labels(&[crowd], [390.0, 390.0]).is_none());
        // Names too wide for the panel's sides leave no plan either.
        let wide = vec![wanted(90.0, "A name far too long to stand beside a small disk 50%")];
        assert!(plan_labels(&[wide], [390.0, 390.0]).is_none());
        // A panel with nothing to name asks for nothing.
        assert!(plan_labels(&[Vec::new()], [390.0, 390.0]).is_some());
    }

    #[test]
    fn a_panel_that_cannot_hold_the_names_takes_the_key_for_every_panel() {
        let fine = vec![wanted(90.0, "A 50%"), wanted(270.0, "B 50%")];
        assert!(plan_labels(&[fine, Vec::new()], [340.0, 340.0]).is_some());
        let fine = vec![wanted(90.0, "A 50%"), wanted(270.0, "B 50%")];
        let wide = vec![wanted(90.0, "A name far too long to stand beside a small disk 50%")];
        assert!(plan_labels(&[fine, wide], [340.0, 340.0]).is_none());
    }

    #[test]
    fn no_positive_values_says_so() {
        let block =
            ChartBlock::from_value(&json!({"type": "pie", "title": "T", "encoding": {"x": "c", "y": "v"}})).unwrap();
        let g = layout(
            &block,
            &[json!({"c": "A", "v": 0})],
            &ChartTheme::default(),
            &Options {
                width: 480.0,
                ..Options::default()
            },
        );
        assert_eq!(g.panels[0].failure.as_deref(), Some("No positive values to show"));
    }
}
