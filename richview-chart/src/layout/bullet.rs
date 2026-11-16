//! Bullet charts: one thin bar for the actual value per row, a short line across it for the
//! target, and optional bands behind it for the ranges the author names (FEA-CH-75). The bars
//! start at zero. RichView draws the numbers it is given and judges none of them.

use super::kit::{self, ValueSide};
use super::types::Facets;
use super::{Ctx, Painted, Style};
use crate::axis::Zero;
use crate::diag::Level;
use crate::geometry::{Anchor, Face, Fill, Item, Observation, Path, Readout, ReadoutLine, Rect, Role, Stroke, Swatch};
use crate::numbers;
use crate::rows::{self, Num};
use serde_json::{Value, json};

const RATIO: f64 = 16.0 / 9.0;
/// Most range bands a bullet chart draws.
const MAX_RANGES: usize = 3;
/// Opacity of one range band; bands overlap, so the lowest range is the darkest.
const BAND_ALPHA: f64 = 0.13;

/// One bullet: the row it reads, its actual value and its target, when it has one.
struct Bullet {
    row: usize,
    name: String,
    actual: f64,
    target: Option<f64>,
}

/// The ascending thresholds under `style.ranges`; the reason when they are not up to three
/// ascending numbers.
fn ranges(raw: &Value) -> Result<Vec<f64>, String> {
    let Some(list) = raw.get("style").and_then(|s| s.get("ranges")) else {
        return Ok(Vec::new());
    };
    let bad = || "Cannot draw these bullets · style.ranges must be up to three ascending numbers".to_owned();
    let Some(items) = list.as_array() else {
        return Err(bad());
    };
    let numbers: Vec<f64> = items
        .iter()
        .filter_map(|v| v.as_f64().filter(|n| n.is_finite()))
        .collect();
    if numbers.len() != items.len() || numbers.len() > MAX_RANGES || numbers.windows(2).any(|w| w[0] >= w[1]) {
        return Err(bad());
    }
    Ok(numbers)
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let x_field = block.encoding.x.clone().unwrap_or_default();
    let y_field = block.encoding.y.clone().unwrap_or_default();
    let target_field = block.channel("target");
    let title = block.y.label.clone().unwrap_or_else(|| y_field.clone());
    let label_format = block.label_format(&block.y, ctx.locale);
    let readout = block.readout_format(&block.y, ctx.locale);
    let thresholds = match ranges(&block.raw) {
        Ok(r) => r,
        Err(reason) => {
            ctx.check(
                "source-error",
                Level::Error,
                "style.ranges must be up to three ascending numbers",
                Some("Write ranges as a list such as [60, 80, 100], smallest first."),
                Some("chart.style.ranges".into()),
                None,
                json!({"ranges": block.raw["style"]["ranges"]}),
            );
            return kit::failed(ctx, &reason, RATIO);
        }
    };
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut excluded = 0usize;
    let mut panels_bullets: Vec<Vec<Bullet>> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut bullets: Vec<Bullet> = Vec::new();
        for &i in members {
            let row = &ctx.rows[i];
            let name = rows::text(row, &x_field).unwrap_or_else(|| format!("Row {}", i + 1));
            let mut read = |field: &str| -> Option<Option<f64>> {
                match rows::num(row, field) {
                    Num::Value(v) => Some(Some(v)),
                    Num::Missing => Some(None),
                    Num::Underflow(t) => {
                        ctx.underflow(i, field, &t);
                        failures[fi].get_or_insert_with(|| {
                            format!("Cannot draw these bullets · {field} has a value too small to represent")
                        });
                        None
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
                        failures[fi].get_or_insert_with(|| {
                            format!("Cannot draw these bullets · {field} has text that is not a number")
                        });
                        None
                    }
                }
            };
            let Some(actual) = read(&y_field) else { continue };
            let target = match &target_field {
                Some(f) => match read(f) {
                    Some(t) => t,
                    None => continue,
                },
                None => None,
            };
            let Some(actual) = actual else {
                excluded += 1;
                continue;
            };
            bullets.push(Bullet {
                row: i,
                name,
                actual,
                target,
            });
        }
        panels_bullets.push(bullets);
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
                "{} without an actual value {} left out",
                super::count(excluded, "row", "rows"),
                super::verb(excluded, "is", "are")
            ),
            Some("Give every row an actual value."),
            Some("chart.encoding.y".into()),
            None,
            json!({"count": excluded}),
        );
        ctx.note.push(format!(
            "{} without an actual value {} not shown",
            super::count(excluded, "row", "rows"),
            super::verb(excluded, "is", "are")
        ));
    }
    ctx.set_series(std::slice::from_ref(&title));
    let longest = panels_bullets.iter().max_by_key(|b| b.len()).map_or(0, Vec::len);
    let names: Vec<String> = (0..longest)
        .map(|k| {
            panels_bullets
                .iter()
                .find_map(|b| b.get(k).map(|x| x.name.clone()))
                .unwrap_or_default()
        })
        .collect();
    let mut plan = kit::plan_oriented(ctx, facets.len(), &names, 1.0, block.style.ratio.unwrap_or(RATIO), true);
    let top = thresholds.last().copied();
    // Few enough rows print the measure's value beyond its bar (FEA-CH-28). Where the longest bar
    // reaches further than every target and band, the value axis keeps room for that value.
    let most = panels_bullets.iter().map(Vec::len).max().unwrap_or(0);
    let longest = panels_bullets
        .iter()
        .flatten()
        .map(|b| b.actual)
        .fold(f64::NEG_INFINITY, f64::max);
    let beyond = panels_bullets
        .iter()
        .flatten()
        .filter_map(|b| b.target)
        .chain(top)
        .all(|v| v <= longest);
    let budget = if plan
        .frames(facets.len(), ctx.opts.width)
        .first()
        .is_some_and(|f| f.w - 30.0 < 480.0)
    {
        3
    } else {
        5
    };
    if block.auto_annotations && (1..=budget).contains(&most) && beyond {
        let text = if plan.horizontal {
            panels_bullets
                .iter()
                .flatten()
                .map(|b| ctx.text_width(&numbers::value(b.actual, &readout)))
                .fold(0.0f64, f64::max)
        } else {
            ctx.line_height()
        };
        plan.headroom = 6.0 + text;
    }
    let extent = |bullets: &[&Bullet]| {
        bullets.iter().fold([0.0f64, 0.0f64], |[lo, hi], b| {
            let t = b.target.unwrap_or(b.actual);
            [lo.min(b.actual).min(t), hi.max(b.actual).max(t)]
        })
    };
    let all: Vec<&Bullet> = panels_bullets
        .iter()
        .zip(&failures)
        .filter(|(_, f)| f.is_none())
        .flat_map(|(b, _)| b)
        .collect();
    // The extent sets the axis, so the exact rows decide it (TYP-36).
    let shared = ctx.derive_extent(0, "bullet.shared", || {
        let mut shared = extent(&all);
        if let Some(t) = top {
            shared[1] = shared[1].max(t);
        }
        shared
    });
    let frames = plan.frames(facets.len(), ctx.opts.width);
    let mut panels = Vec::new();
    for (fi, ((name, _), outer)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *outer, plan.title_room, name, reason));
            continue;
        }
        let bullets = &panels_bullets[fi];
        let mut data = if block.style.free_scales {
            let mine: Vec<&Bullet> = bullets.iter().collect();
            ctx.derive_extent(fi, "bullet.own", || {
                let mut own = extent(&mine);
                if let Some(t) = top {
                    own[1] = own[1].max(t);
                }
                own
            })
        } else {
            shared
        };
        data = super::Fit::widen(data, ctx.wanted().y);
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
        let full = frame.thickness(1, 36.0);
        let thin = (full * 0.36).max(3.0).min(full);
        let ink = Style::rgba(ctx.style.ink);
        let faint = Style::rgba(ctx.style.ink_faint);
        let mut marks: Vec<Item> = Vec::new();
        let mut observations = Vec::new();
        // The measure's value at the end of its bar (FEA-CH-28, chart design 16 Nov 2026).
        let mut wanted: Vec<super::place::SparseValue> = Vec::new();
        for (c, b) in bullets.iter().enumerate() {
            let c0 = frame.slot_start(c, 0, 1, full);
            let cm = c0 + full / 2.0;
            // The widest band first, so each narrower one darkens the part nearest zero.
            for t in thresholds.iter().rev() {
                marks.push(Item::Rect {
                    rect: frame.span(c0, c0 + full, 0.0, *t),
                    fill: Fill::solid(faint.with_alpha(BAND_ALPHA)),
                    stroke: None,
                    radius: [0.0; 4],
                    role: Role::Band,
                    series: None,
                    obs: None,
                });
            }
            let bar = frame.span(cm - thin / 2.0, cm + thin / 2.0, 0.0, b.actual);
            let bar = if frame.horizontal {
                Rect::new(bar.x, bar.y, bar.w.max(1.0), bar.h)
            } else {
                Rect::new(bar.x, bar.y, bar.w, bar.h.max(1.0))
            };
            let hatch = ctx.series[0].hatch;
            let color = Style::rgba(ctx.series_color(0));
            marks.push(Item::Rect {
                rect: bar,
                fill: Fill {
                    color,
                    hatch,
                    hatch_ink: hatch.map(|_| Style::rgba(ctx.style.surface)),
                    hatch_scale: 1.0,
                },
                stroke: None,
                radius: [0.0; 4],
                role: Role::Mark,
                series: Some(0),
                obs: Some(observations.len()),
            });
            if let Some(t) = b.target {
                let across = full * 0.8;
                marks.push(Item::Line {
                    path: Path::polyline(&[frame.pt(cm - across / 2.0, t), frame.pt(cm + across / 2.0, t)]),
                    stroke: Stroke::solid(ink, 2.5),
                    role: Role::Mark,
                    series: None,
                    obs: None,
                });
            }
            let reach = [0.0, b.actual, b.target.unwrap_or(0.0), top.unwrap_or(0.0)];
            let lo = reach.into_iter().fold(f64::INFINITY, f64::min);
            let hi = reach.into_iter().fold(f64::NEG_INFINITY, f64::max);
            let area = frame.span(c0, c0 + full, lo, hi);
            let mut lines = vec![ReadoutLine {
                name: block
                    .y
                    .label
                    .clone()
                    .unwrap_or_else(|| super::cartesian::readable_name(&y_field)),
                value: numbers::value(b.actual, &readout),
                series: Some(0),
                names_series: false,
                derived: false,
            }];
            if let Some(t) = b.target {
                lines.push(ReadoutLine {
                    name: "Target".into(),
                    value: numbers::value(t, &readout),
                    series: None,
                    names_series: false,
                    derived: false,
                });
            }
            if !thresholds.is_empty() {
                lines.push(ReadoutLine {
                    name: "Ranges".into(),
                    value: thresholds
                        .iter()
                        .map(|t| numbers::value(*t, &readout))
                        .collect::<Vec<_>>()
                        .join(" · "),
                    series: None,
                    names_series: false,
                    derived: false,
                });
            }
            // Beyond the bar's end, unless the target mark or a band's edge would stand within 4 px
            // of the text. Chart design's rule then sets it inside the bar's end, and drops it
            // when it does not fit there: the bar is at most 13 px thick and a line of figures is
            // taller, so it never fits and the value has no place.
            {
                let text = numbers::value(b.actual, &readout);
                let end = frame.at(b.actual);
                let up = b.actual >= 0.0;
                let beyond = if frame.horizontal {
                    let (x, anchor) = if up {
                        (end + 4.0, Anchor::Start)
                    } else {
                        (end - 4.0, Anchor::End)
                    };
                    ctx.text_mid(&text, x, cm, anchor, Face::Figures, Role::Label, ctx.style.ink)
                } else {
                    let half = ctx.line_height() / 2.0;
                    let y = if up { end - 4.0 - half } else { end + 4.0 + half };
                    ctx.text_mid(&text, cm, y, Anchor::Middle, Face::Figures, Role::Label, ctx.style.ink)
                };
                let reach = if frame.horizontal {
                    (beyond.bounds.x - 4.0, beyond.bounds.right() + 4.0)
                } else {
                    (beyond.bounds.y - 4.0, beyond.bounds.bottom() + 4.0)
                };
                let clear = b
                    .target
                    .iter()
                    .chain(thresholds.iter())
                    .map(|v| frame.at(*v))
                    .all(|edge| edge < reach.0 || edge > reach.1);
                wanted.push(super::place::SparseValue {
                    text,
                    series: 0,
                    observation: observations.len(),
                    places: if clear { vec![beyond] } else { Vec::new() },
                });
            }
            observations.push(Observation {
                row: Some(b.row),
                dots: vec![],
                series: Some(0),
                at: frame.pt(cm, b.actual),
                wedge: None,
                area: Some(area),
                radius: 0.0,
                x: cm,
                readout: Readout {
                    heading: b.name.clone(),
                    lines,
                }
                .into(),
            });
        }
        let rows = observations.len();
        let mut panel = kit::panel(ctx, outer, frame, marks, observations, name);
        // A value may stand on a band's tone, so the bands are not ink to keep clear of.
        super::place::sparse_values(ctx, fi, &mut panel, rows, wanted, &|item| {
            !matches!(item, Item::Rect { role: Role::Band, .. })
        });
        panels.push(panel);
    }
    ctx.summary = format!(
        "{} {} of {title}{}.",
        all.len(),
        if all.len() == 1 { "bullet" } else { "bullets" },
        if target_field.is_some() { " against targets" } else { "" }
    );
    ctx.note
        .push("bar = actual, line across it = target, shaded bands = ranges".into());
    let legend = kit::legend(ctx, Swatch::Square);
    kit::finish(&plan, facets.len(), panels, legend)
}
