//! Waterfall charts: signed steps that start where the last one ended, with totals and restarts
//! the author marks. RichView adds the steps up and nothing else; a total the author gives that
//! disagrees with the sum is reported, never replaced (FEA-CH-42, FEA-CH-70).

use super::kit::{self, ValueSide};
use super::types::Facets;
use super::{Ctx, Painted, Style};
use crate::axis::Zero;
use crate::color::{self, Oklch};
use crate::diag::Level;
use crate::geometry::{
    Anchor, DropReason, Face, Fill, Item, Observation, Path, Readout, ReadoutLine, Rect, Role, Stroke, Swatch,
};
use crate::numbers;
use crate::rows::{self, Num};
use serde::{Deserialize, Serialize};
use serde_json::json;

const RATIO: f64 = 16.0 / 9.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Measure {
    Delta,
    Total,
    Start,
}

/// One step: where its bar starts and ends in value, and where the running sum stands after it.
struct Step {
    row: usize,
    name: String,
    measure: Measure,
    from: f64,
    to: f64,
    /// The signed change of a delta, or the value an author gave a total or a start.
    given: Option<f64>,
}

impl Step {
    /// 0 increase, 1 decrease, 2 total.
    fn kind(&self) -> usize {
        match self.measure {
            Measure::Delta if self.to >= self.from => 0,
            Measure::Delta => 1,
            _ => 2,
        }
    }
}

const KINDS: [&str; 3] = ["Increase", "Decrease", "Total"];

/// The chart's palette with the colours of the kinds present first, in the order they are
/// identified: a hue near blue for a rise, one near orange-red for a fall and a muted ink for a
/// total, the first such set that passes the palette rule on this surface (a rise and a fall
/// must stay apart under colour-vision simulation). The remaining colours follow.
fn semantic_palette(ctx: &Ctx, kinds: &[usize]) -> Vec<Oklch> {
    let palette = &ctx.style.palette;
    let near = |hue: f64| -> Vec<usize> {
        let distance = |c: &Oklch| ((c.h - hue + 540.0).rem_euclid(360.0) - 180.0).abs();
        let mut chromatic: Vec<usize> = (0..palette.len()).filter(|i| palette[*i].c > 0.03).collect();
        chromatic.sort_by(|a, b| distance(&palette[*a]).total_cmp(&distance(&palette[*b])));
        chromatic.truncate(3);
        chromatic
    };
    let (rises, falls) = (near(255.0), near(30.0));
    let neutrals: Vec<Oklch> = [0.3, 0.15, 0.45, 0.0]
        .into_iter()
        .map(|t| color::mix(ctx.style.ink_soft, ctx.style.surface, t))
        .collect();
    // Choices in order of preference: nearer hues and the usual neutral first.
    let mut choices: Vec<(usize, Option<usize>, Option<usize>, usize)> = Vec::new();
    for (ri, r) in rises.iter().enumerate() {
        for (fi, f) in falls.iter().enumerate() {
            for ni in 0..neutrals.len() {
                if r != f {
                    choices.push((ri + fi + ni, Some(*r), Some(*f), ni));
                }
            }
        }
    }
    choices.sort_by_key(|c| c.0);
    // A grey palette has no semantic hue pair. Keep its authored slots; ordinary palette
    // validation and automatic patterns still apply (STY-22, FEA-CH-57, FEA-CH-62).
    if choices.is_empty() {
        return palette.clone();
    }
    let build = |rise: Option<usize>, fall: Option<usize>, neutral: usize| -> (Vec<Oklch>, Vec<usize>) {
        let mut used = Vec::new();
        let mut chosen = Vec::new();
        for &kind in kinds {
            match kind {
                0 => {
                    let i = rise.unwrap_or(0);
                    used.push(i);
                    chosen.push(palette[i]);
                }
                1 => {
                    let i = fall.unwrap_or(1.min(palette.len() - 1));
                    used.push(i);
                    chosen.push(palette[i]);
                }
                _ => chosen.push(neutrals[neutral]),
            }
        }
        (chosen, used)
    };
    let slots: Vec<usize> = (0..kinds.len()).collect();
    let passes = |chosen: &[Oklch]| {
        chosen.len() < 2 || super::checks::palette_problems(chosen, &slots, ctx.style.surface).is_empty()
    };
    let first = choices.first().map_or((None, None, 0), |c| (c.1, c.2, c.3));
    let (chosen, used) = choices
        .iter()
        .map(|c| build(c.1, c.2, c.3))
        .find(|(chosen, _)| passes(chosen))
        .unwrap_or_else(|| build(first.0, first.1, first.2));
    let rest = palette
        .iter()
        .enumerate()
        .filter(|(i, _)| !used.contains(i))
        .map(|(_, c)| *c);
    chosen.into_iter().chain(rest).collect()
}

fn signed(v: f64, format: &numbers::Format) -> String {
    let text = numbers::value(v, format);
    if v > 0.0 { format!("+{text}") } else { text }
}

/// What a step is and prints, from the exact values: a step that is a rise or a fall by less
/// than a step of the grid must not change kind, side or sign on the grid (TYP-36).
#[derive(Serialize, Deserialize)]
struct StepText {
    /// 0 increase, 1 decrease, 2 total.
    kind: usize,
    /// Whether the bar ends above where it starts.
    up: bool,
    /// The text on the chart and the value the step ends at.
    label: String,
    end: String,
    /// The readout's change (or start) and running total; the change is empty for a total.
    change: String,
    running: String,
    /// Whether the running total was added up by RichView.
    derived: bool,
}

fn step_text(s: &Step, label_format: &numbers::Format, readout: &numbers::Format) -> StepText {
    let delta = s.to - s.from;
    let end = numbers::value(s.to, label_format);
    StepText {
        kind: s.kind(),
        up: s.to >= s.from,
        label: if s.measure == Measure::Delta {
            signed(delta, label_format)
        } else {
            end.clone()
        },
        end,
        change: match s.measure {
            Measure::Delta => signed(s.given.unwrap_or(delta), readout),
            Measure::Start => numbers::value(s.to, readout),
            Measure::Total => String::new(),
        },
        running: numbers::value(s.to, readout),
        derived: s.measure != Measure::Start && s.given.is_none_or(|g| g != s.to),
    }
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let x_field = block.encoding.x.clone().unwrap_or_default();
    let y_field = block.encoding.y.clone().unwrap_or_default();
    let measure_field = block.channel("measure");
    let label_format = block.label_format(&block.y, ctx.locale);
    let readout = block.readout_format(&block.y, ctx.locale);
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut excluded = 0usize;
    let mut panels_steps: Vec<Vec<Step>> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut steps: Vec<Step> = Vec::new();
        let mut running = 0.0f64;
        for &i in members {
            let row = &ctx.rows[i];
            let name = rows::text(row, &x_field).unwrap_or_else(|| format!("Row {}", i + 1));
            let measure = match measure_field.as_deref().and_then(|f| rows::text(row, f)) {
                None => Measure::Delta,
                Some(m) => match m.to_lowercase().as_str() {
                    "delta" | "relative" | "change" => Measure::Delta,
                    "total" | "subtotal" => Measure::Total,
                    "start" | "absolute" | "reset" => Measure::Start,
                    other => {
                        ctx.check(
                            "source-error",
                            Level::Error,
                            format!(
                                "Row {} has the measure '{other}'; expected delta, total or start",
                                i + 1
                            ),
                            Some("Use delta, total or start, or leave the field empty."),
                            Some(Ctx::row_path(i, measure_field.as_deref().unwrap_or("measure"))),
                            None,
                            json!({"row": i, "value": other}),
                        );
                        failures[fi].get_or_insert_with(|| "Cannot draw this waterfall · Unknown measure".into());
                        continue;
                    }
                },
            };
            let value = match rows::num(row, &y_field) {
                Num::Value(v) => Some(v),
                Num::Missing => None,
                Num::Underflow(t) => {
                    ctx.underflow(i, &y_field, &t);
                    failures[fi].get_or_insert_with(|| {
                        format!("Cannot draw this waterfall · {y_field} has a value too small to represent")
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
                        format!("Cannot draw this waterfall · {y_field} has text that is not a number")
                    });
                    continue;
                }
            };
            let step = match (measure, value) {
                (Measure::Delta, Some(v)) => Step {
                    row: i,
                    name,
                    measure,
                    from: running,
                    to: running + v,
                    given: Some(v),
                },
                (Measure::Start, Some(v)) => Step {
                    row: i,
                    name,
                    measure,
                    from: 0.0,
                    to: v,
                    given: Some(v),
                },
                (Measure::Total, given) => {
                    if let Some(v) = given
                        && (v - running).abs() > 1e-9 * running.abs().max(1.0)
                    {
                        ctx.check(
                            "waterfall-total",
                            Level::Warning,
                            format!(
                                "Row {} gives the total {} but the steps add up to {}",
                                i + 1,
                                numbers::value(v, &readout),
                                numbers::value(running, &readout)
                            ),
                            Some("Fix the total, or mark the row as start to restart the sum there."),
                            Some(Ctx::row_path(i, &y_field)),
                            None,
                            json!({"row": i, "given": v, "sum": running}),
                        );
                    }
                    Step {
                        row: i,
                        name,
                        measure,
                        from: 0.0,
                        to: running,
                        given,
                    }
                }
                _ => {
                    excluded += 1;
                    continue;
                }
            };
            running = step.to;
            steps.push(step);
        }
        panels_steps.push(steps);
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
                "{} without a value {} left out",
                super::count(excluded, "row", "rows"),
                super::verb(excluded, "is", "are")
            ),
            Some("Give every step a value."),
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
    let texts: Vec<Vec<StepText>> = panels_steps
        .iter()
        .enumerate()
        .map(|(fi, steps)| {
            ctx.derive(fi, "waterfall.steps", || {
                steps.iter().map(|s| step_text(s, &label_format, &readout)).collect()
            })
        })
        .collect();
    let present: Vec<String> = (0..3)
        .filter(|k| texts.iter().flatten().any(|t| t.kind == *k))
        .map(|k| KINDS[k].to_owned())
        .collect();
    // A rise, a fall and a total read by colour before the legend: cool, warm and neutral.
    let kinds: Vec<usize> = (0..3)
        .filter(|k| texts.iter().flatten().any(|t| t.kind == *k))
        .collect();
    ctx.style.palette = semantic_palette(ctx, &kinds);
    ctx.set_series(&present);
    let slot_of = |kind: usize| present.iter().position(|n| n == KINDS[kind]).unwrap_or(0);
    // The longest panel sets the names; panels with other steps use their own below.
    let longest = panels_steps.iter().max_by_key(|s| s.len()).map_or(0, Vec::len);
    let names: Vec<String> = (0..longest)
        .map(|k| {
            panels_steps
                .iter()
                .find_map(|s| s.get(k).map(|st| st.name.clone()))
                .unwrap_or_default()
        })
        .collect();
    let mut plan = kit::plan(ctx, facets.len(), &names, 1.0, block.style.ratio.unwrap_or(RATIO));
    // The change is written beyond each bar's end, so the axis keeps a line and a gap above the largest.
    plan.headroom = ctx.line_height() + 6.0;
    let extent = |steps: &[&Step]| {
        steps.iter().fold([f64::INFINITY, f64::NEG_INFINITY], |[lo, hi], s| {
            [lo.min(s.from).min(s.to), hi.max(s.from).max(s.to)]
        })
    };
    let all: Vec<&Step> = panels_steps.iter().flatten().collect();
    let shared = ctx.derive_extent(0, "waterfall.shared", || extent(&all));
    let frames = plan.frames(facets.len(), ctx.opts.width);
    let title = block.y.label.clone().unwrap_or_else(|| y_field.clone());
    let lh = ctx.line_height();
    let mut panels = Vec::new();
    for (fi, ((name, _), outer)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *outer, plan.title_room, name, reason));
            continue;
        }
        let steps = &panels_steps[fi];
        let mine: Vec<&Step> = steps.iter().collect();
        let mut data = if block.style.free_scales {
            ctx.derive_extent(fi, "waterfall.own", || extent(&mine))
        } else {
            shared
        };
        if !data[0].is_finite() {
            data = ctx.wanted().y.unwrap_or([0.0, 1.0]);
        } else {
            data = super::Fit::widen(data, ctx.wanted().y);
        }
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
        let t = frame.thickness(1, 56.0);
        let ink = Style::rgba(ctx.style.ink_faint);
        let mut marks: Vec<Item> = Vec::new();
        let mut observations = Vec::new();
        let mut previous: Option<(f64, f64)> = None;
        for (c, s) in steps.iter().enumerate() {
            let text_of = &texts[fi][c];
            let slot = slot_of(text_of.kind);
            if ctx.is_hidden(slot) {
                previous = None;
                continue;
            }
            let c0 = frame.slot_start(c, 0, 1, t);
            let cm = c0 + t / 2.0;
            let rect = frame.span(c0, c0 + t, s.from, s.to);
            // A step of zero is a hairline at its level, so it stays visible.
            let rect = if frame.horizontal {
                Rect::new(rect.x, rect.y, rect.w.max(1.0), rect.h)
            } else {
                Rect::new(rect.x, rect.y, rect.w, rect.h.max(1.0))
            };
            if let Some((level, edge)) = previous {
                let (a, b) = if frame.horizontal {
                    ([frame.at(level), edge], [frame.at(level), c0])
                } else {
                    ([edge, frame.at(level)], [c0, frame.at(level)])
                };
                marks.push(Item::Line {
                    path: Path::polyline(&[a, b]),
                    stroke: Stroke::solid(ink, 1.0),
                    role: Role::Leader,
                    series: None,
                    obs: None,
                });
            }
            previous = Some((s.to, c0 + t));
            let color = Style::rgba(ctx.series_color(slot));
            let hatch = ctx.series[slot].hatch;
            let r = ctx.style.bar_radius.min(rect.w / 2.0).min(rect.h / 2.0);
            marks.push(Item::Rect {
                rect,
                fill: Fill {
                    color,
                    hatch,
                    hatch_ink: hatch.map(|_| Style::rgba(ctx.style.surface)),
                    hatch_scale: 1.0,
                },
                stroke: None,
                radius: [r; 4],
                role: Role::Mark,
                series: Some(slot),
                obs: Some(observations.len()),
            });
            // The change, set beyond the bar's far end when it fits the plot and its band. The
            // author's `auto_annotations: false` draws none and records no drop.
            if ctx.block.auto_annotations {
                let label = text_of.label.clone();
                let up = text_of.up;
                let (x, y, anchor) = if frame.horizontal {
                    let edge = if up { rect.right() + 4.0 } else { rect.x - 4.0 };
                    (edge, rect.center()[1], if up { Anchor::Start } else { Anchor::End })
                } else {
                    let edge = if up {
                        rect.y - 4.0
                    } else {
                        rect.bottom() + 4.0 + lh * 0.8
                    };
                    (cm, edge, Anchor::Middle)
                };
                let text = if frame.horizontal {
                    ctx.text_mid(&label, x, y, anchor, Face::Figures, Role::Label, ctx.style.ink)
                } else {
                    ctx.text(&label, x, y, anchor, Face::Figures, Role::Label, ctx.style.ink)
                };
                let fits_band = if frame.horizontal {
                    text.bounds.h <= frame.band
                } else {
                    text.bounds.w <= frame.band
                };
                if fits_band && text.bounds.inside(&frame.plot, 0.5) {
                    marks.push(Item::Text(text));
                } else {
                    let reason = if fits_band {
                        DropReason::OutOfBounds
                    } else {
                        DropReason::Density
                    };
                    ctx.drop_label(&label, reason, "value", Some(text.bounds));
                }
            }
            let mut lines = Vec::new();
            match s.measure {
                Measure::Delta => lines.push(ReadoutLine {
                    label: None,
                    name: "Change".into(),
                    value: text_of.change.clone(),
                    series: Some(slot),
                    names_series: false,
                    derived: false,
                }),
                Measure::Start => lines.push(ReadoutLine {
                    label: None,
                    name: "Start".into(),
                    value: text_of.change.clone(),
                    series: Some(slot),
                    names_series: false,
                    derived: false,
                }),
                Measure::Total => {}
            }
            lines.push(ReadoutLine {
                label: None,
                name: if s.measure == Measure::Total {
                    "Total"
                } else {
                    "Running total"
                }
                .into(),
                value: text_of.running.clone(),
                series: (s.measure == Measure::Total).then_some(slot),
                names_series: false,
                derived: text_of.derived,
            });
            observations.push(Observation {
                row: Some(s.row),
                dots: vec![],
                series: Some(slot),
                at: frame.pt(cm, s.to),
                wedge: None,
                area: Some(rect),
                radius: 0.0,
                x: cm,
                readout: Readout {
                    heading: s.name.clone(),
                    lines,
                }
                .into(),
            });
        }
        panels.push(kit::panel(ctx, outer, frame, marks, observations, name));
    }
    ctx.summary = match texts.iter().flatten().last() {
        Some(l) if !all.is_empty() => format!("{} steps ending at {}.", all.len(), l.end),
        _ => "No steps.".into(),
    };
    let legend = kit::legend(ctx, Swatch::Square);
    kit::finish(&plan, facets.len(), panels, legend)
}

#[cfg(test)]
mod tests {
    use crate::block::ChartBlock;
    use crate::layout::{ChartTheme, Options, layout};
    use serde_json::json;

    #[test]
    fn rise_fall_and_total_keep_their_colours_and_every_bar_its_label() {
        let block = ChartBlock::from_value(&json!({
            "type": "waterfall", "title": "T",
            "encoding": {"x": "step", "y": "amount", "measure": "kind"}
        }))
        .unwrap();
        let rows = vec![
            json!({"step": "Sales", "amount": 120, "kind": "start"}),
            json!({"step": "Costs", "amount": -45.5}),
            json!({"step": "Tax", "amount": -14.5}),
            json!({"step": "Grant", "amount": 30}),
            json!({"step": "Profit", "amount": 90, "kind": "total"}),
        ];
        for width in [390.0, 1440.0] {
            let g = layout(
                &block,
                &rows,
                &ChartTheme::default(),
                &Options {
                    width,
                    ..Options::default()
                },
            );
            assert!(g.failure.is_none());
            let names: Vec<_> = g.series.iter().map(|s| s.name.as_str()).collect();
            assert_eq!(names, ["Increase", "Decrease", "Total"]);
            assert!(g.series.iter().all(|s| s.hatch.is_none()), "{width}: no decals");
            assert!(
                !g.checks.iter().any(|c| c.kind == "palette-rule"),
                "{width}: the kinds' colours pass the palette rule"
            );
            assert!(g.dropped.is_empty(), "{width}: {:?}", g.dropped);
            assert!(g.note.is_none(), "{width}: the subtitle says it");
        }
    }

    #[test]
    fn greyscale_keeps_its_palette_slots_and_automatic_patterns() {
        let block = ChartBlock::from_value(&json!({"type":"waterfall","title":"T",
            "encoding":{"x":"step","y":"amount","measure":"kind"}}))
        .unwrap();
        let rows = vec![
            json!({"step":"Start","amount":100,"kind":"start"}),
            json!({"step":"Gain","amount":30}),
            json!({"step":"Cost","amount":-20}),
            json!({"step":"Total","amount":110,"kind":"total"}),
        ];
        let theme = ChartTheme {
            palette: crate::rmd_render::palette("hand-drawn")
                .iter()
                .map(|v| (*v).into())
                .collect(),
            decals: false,
            ..Default::default()
        };
        let g = layout(
            &block,
            &rows,
            &theme,
            &Options {
                width: 640.0,
                ..Default::default()
            },
        );
        assert!(!g.checks.iter().any(|c| c.kind == "palette-rule"), "{:?}", g.checks);
        for (series, expected) in g.series.iter().zip(&theme.palette) {
            assert_eq!(
                series.color,
                crate::geometry::Rgba::from_oklch(crate::color::parse(expected).unwrap(), 1.0)
            );
            assert!(series.hatch.is_some(), "greyscale identity keeps its non-colour cue");
        }
        let bad_theme = ChartTheme {
            palette: vec![theme.palette[0].clone(); 3],
            ..theme
        };
        let bad = layout(
            &block,
            &rows,
            &bad_theme,
            &Options {
                width: 640.0,
                ..Default::default()
            },
        );
        assert!(
            bad.checks.iter().any(|c| c.kind == "palette-rule"),
            "an invalid grey palette still warns"
        );
        assert!(bad.series.iter().all(|s| s.hatch.is_some()));
    }
}
