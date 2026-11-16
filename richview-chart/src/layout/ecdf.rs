//! Empirical distribution charts: for each group a step line of the share of observations at or
//! below every value. The step is drawn exactly, each jump at its value; RichView computes the
//! shares, so every readout of them is derived (FEA-CH-14, FEA-CH-42).

use super::kit::{self, ValueSide, XAxis};
use super::types::Facets;
use super::{Ctx, LINE_WIDTH, Painted, Style};
use crate::axis::Zero;
use crate::derived::Positions;
use crate::diag::Level;
use crate::geometry::{Dot, Item, Observation, Path, Readout, ReadoutLine, Role, Stroke, Swatch};
use crate::numbers::{self, Format, Kind};
use crate::payload::Quant;
use crate::rows::{self, Num};
use serde::{Deserialize, Serialize};
use serde_json::json;

const RATIO: f64 = 16.0 / 9.0;
/// Room either side of the data, as a share of its range.
const PAD: f64 = 0.04;

/// One group in one panel: its distinct values, each with the number of observations at or below
/// it. These jump with the data (values closer than the grid merge, counts move), so the exact
/// rows decide them (TYP-36).
struct Sample {
    levels: Vec<(f64, usize)>,
}

impl Sample {
    /// The levels of `sorted`, ascending.
    fn levels_of(sorted: &[f64]) -> Vec<(f64, usize)> {
        let mut levels: Vec<(f64, usize)> = Vec::new();
        for (k, v) in sorted.iter().enumerate() {
            match levels.last_mut() {
                Some(last) if last.0 == *v => last.1 = k + 1,
                _ => levels.push((*v, k + 1)),
            }
        }
        levels
    }
    /// A sample from derived levels; values that rounding made equal become one level.
    fn from_levels(levels: Vec<(f64, usize)>) -> Sample {
        let mut merged: Vec<(f64, usize)> = Vec::new();
        for level in levels {
            match merged.last_mut() {
                Some(last) if last.0 == level.0 => last.1 = level.1,
                _ => merged.push(level),
            }
        }
        Sample { levels: merged }
    }
    fn len(&self) -> usize {
        self.levels.last().map_or(0, |l| l.1)
    }
    /// Observations at or below `x`.
    fn at_or_below(&self, x: f64) -> usize {
        match self.levels.partition_point(|l| l.0 <= x) {
            0 => 0,
            at => self.levels[at - 1].1,
        }
    }
    /// Observations exactly at `x`.
    fn equal(&self, x: f64) -> usize {
        let below = self.levels.partition_point(|l| l.0 < x);
        self.at_or_below(x) - if below == 0 { 0 } else { self.levels[below - 1].1 }
    }
    fn share(&self, x: f64) -> f64 {
        self.at_or_below(x) as f64 / self.len() as f64
    }
    fn extent(&self) -> [f64; 2] {
        [self.levels[0].0, self.levels[self.levels.len() - 1].0]
    }
}

/// The step line as points: from zero at the left edge, a jump at each distinct value, and one
/// at the right edge.
fn steps(sample: &Sample, left: f64, right: f64, x: impl Fn(f64) -> f64, y: impl Fn(f64) -> f64) -> Vec<[f64; 2]> {
    let n = sample.len();
    let mut pts = vec![[x(left), y(0.0)]];
    for &(v, cumulative) in &sample.levels {
        let before = *pts.last().map_or(&[0.0, 0.0], |p| p);
        pts.push([x(v), before[1]]);
        pts.push([x(v), y(cumulative as f64 / n as f64)]);
    }
    pts.push([x(right), y(1.0)]);
    pts
}

/// A group's levels as the payload carries them (TYP-36): each value on the source channel's grid
/// with the number of observations at or below it, so the record holds no digit the author does
/// not show (FEA-CH-37). Values the grid makes equal become one level (`Sample::from_levels`).
#[derive(Serialize, Deserialize)]
struct Levels {
    values: Positions,
    counts: Vec<usize>,
}

impl Levels {
    fn of(levels: &[(f64, usize)], grid: Option<Quant>, format: &Format) -> Self {
        Self {
            values: Positions::encode(levels.iter().map(|l| l.0), grid, format),
            counts: levels.iter().map(|l| l.1).collect(),
        }
    }

    fn levels(&self) -> Vec<(f64, usize)> {
        self.values
            .decode()
            .into_iter()
            .zip(self.counts.iter().copied())
            .collect()
    }
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let x_field = block.encoding.x.clone().unwrap_or_default();
    let groups: Vec<String> = match &block.encoding.color {
        Some(f) => rows::distinct(ctx.rows, f),
        None => vec![block.x.label.clone().unwrap_or_else(|| x_field.clone())],
    };
    ctx.set_series(&groups);
    let percent = |digits: u8| Format {
        kind: Kind::Percent,
        unit: None,
        digits,
        locale: ctx.locale,
    };
    let label_format = percent(block.digits);
    let readout_percent = percent(block.readout_digits);
    let x_readout = block.readout_format(&block.x, ctx.locale);
    let x_grid = ctx.grid_for(&x_field);
    let count_format = Format {
        kind: Kind::Number,
        unit: None,
        digits: numbers::MAX_DIGITS,
        locale: ctx.locale,
    };
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut excluded = 0usize;
    let mut data: Vec<Vec<Option<Sample>>> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut values: Vec<Vec<f64>> = groups.iter().map(|_| Vec::new()).collect();
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
            match rows::num(row, &x_field) {
                Num::Value(v) => values[group].push(v),
                Num::Missing => excluded += 1,
                Num::Underflow(t) => {
                    ctx.underflow(i, &x_field, &t);
                    failures[fi].get_or_insert_with(|| {
                        format!("Cannot draw this distribution · {x_field} has a value too small to represent")
                    });
                }
                Num::Invalid(t) => {
                    ctx.check(
                        "non-numeric",
                        Level::Error,
                        format!("Row {} has {x_field} '{t}', which is not a number", i + 1),
                        Some("Write plain numbers; missing values may be empty or NA."),
                        Some(Ctx::row_path(i, &x_field)),
                        None,
                        json!({"row": i}),
                    );
                    failures[fi].get_or_insert_with(|| {
                        format!("Cannot draw this distribution · {x_field} has text that is not a number")
                    });
                }
            }
        }
        let samples: Vec<Option<Sample>> = values
            .into_iter()
            .enumerate()
            .map(|(g, mut v)| {
                (!v.is_empty() && failures[fi].is_none()).then(|| {
                    let record: Levels = ctx.derive_projected(fi, &format!("ecdf.levels.{g}"), || {
                        v.sort_by(f64::total_cmp);
                        Levels::of(&Sample::levels_of(&v), x_grid.clone(), &x_readout)
                    });
                    Sample::from_levels(record.levels())
                })
            })
            .collect();
        if failures[fi].is_none() && samples.iter().all(Option::is_none) {
            failures[fi] = Some("Cannot draw this distribution · No row has a value".into());
        }
        data.push(samples);
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
                "{} without an x or group {} left out",
                super::count(excluded, "row", "rows"),
                super::verb(excluded, "is", "are")
            ),
            Some("Give every row a number."),
            Some("chart.encoding.x".into()),
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
    let title = block.y.label.clone().unwrap_or_else(|| "Share at or below".into());
    let x_title = block.x.label.clone().unwrap_or_else(|| x_field.clone());
    let visible = |ctx: &Ctx, fi: usize| -> Vec<(usize, &Sample)> {
        data[fi]
            .iter()
            .enumerate()
            .filter(|(g, _)| !ctx.is_hidden(*g))
            .filter_map(|(g, s)| s.as_ref().map(|s| (g, s)))
            .collect()
    };
    let extent = |fis: &mut dyn Iterator<Item = usize>, ctx: &Ctx| {
        let mut x = [f64::INFINITY, f64::NEG_INFINITY];
        for fi in fis {
            for (_, s) in visible(ctx, fi) {
                let e = s.extent();
                x = [x[0].min(e[0]), x[1].max(e[1])];
            }
        }
        x
    };
    let shared = extent(&mut (0..facets.len()).filter(|&fi| failures[fi].is_none()), ctx);
    let mut panels = Vec::new();
    let mut shown = 0usize;
    for (fi, ((name, _), outer)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *outer, plan.title_room, name, reason));
            continue;
        }
        let mut x = if block.style.free_scales {
            extent(&mut std::iter::once(fi), ctx)
        } else {
            shared
        };
        if x[0].is_finite() {
            let span = x[1] - x[0];
            let pad = if span > 0.0 {
                span * PAD
            } else if x[0] == 0.0 {
                1.0
            } else {
                x[0].abs() * 0.1
            };
            x = [x[0] - pad, x[1] + pad];
            x = super::Fit::widen(x, ctx.wanted().x);
        } else {
            x = ctx.wanted().x.unwrap_or([0.0, 1.0]);
        }
        if !x[0].is_finite() || !x[1].is_finite() || !(x[1] - x[0]).is_finite() {
            panels.push(super::failed_facet(
                ctx,
                *outer,
                plan.title_room,
                name,
                "Cannot draw this distribution · The values are too extreme to represent",
            ));
            continue;
        }
        ctx.fit.x = Some(x);
        ctx.fit.y = Some([0.0, 1.0]);
        let side = ValueSide {
            data: [0.0, 1.0],
            zero: Zero::Include,
            title: title.clone(),
            format: &label_format,
            min: Some(0.0),
            max: Some(1.0),
        };
        let frame = kit::xy_frame(ctx, &plan, outer, &XAxis::Numeric { extent: x }, Some(&x_title), &side);
        let mut marks: Vec<Item> = Vec::new();
        let samples = visible(ctx, fi);
        for &(g, s) in &samples {
            let color = Style::rgba(ctx.series_color(g));
            let pts = steps(s, x[0], x[1], |v| frame.x(v), |share| frame.y(share));
            marks.push(Item::Line {
                path: Path::polyline(&pts),
                stroke: Stroke {
                    dash: ctx.series[g].dash.clone(),
                    ..Stroke::solid(color, LINE_WIDTH)
                },
                role: Role::Mark,
                series: Some(g),
                obs: None,
            });
            shown += s.len();
        }
        // One observation per distinct value, with every group's share at or below it.
        let mut xs: Vec<f64> = samples.iter().flat_map(|(_, s)| s.levels.iter().map(|l| l.0)).collect();
        xs.sort_by(f64::total_cmp);
        xs.dedup();
        let multi = block.encoding.color.is_some();
        let mut observations = Vec::new();
        for xv in xs {
            let px = frame.x(xv);
            if px < frame.plot.x - 0.5 || px > frame.plot.right() + 0.5 {
                continue;
            }
            let mut lines = Vec::new();
            let mut dots = Vec::new();
            for &(g, s) in &samples {
                let prefix = if multi {
                    format!("{} · ", groups[g])
                } else {
                    String::new()
                };
                let share = s.share(xv);
                lines.push(ReadoutLine {
                    name: format!("{prefix}Share at or below"),
                    value: numbers::value(share, &readout_percent),
                    series: Some(g),
                    names_series: multi,
                    derived: true,
                });
                lines.push(ReadoutLine {
                    name: format!("{prefix}Observations"),
                    value: numbers::value(s.equal(xv) as f64, &count_format),
                    series: Some(g),
                    names_series: multi,
                    derived: true,
                });
                dots.push(Dot {
                    at: [px, frame.y(share)],
                    series: g,
                });
            }
            let Some(first) = dots.first().copied() else { continue };
            observations.push(Observation {
                row: None,
                dots,
                series: Some(first.series),
                at: first.at,
                wedge: None,
                area: None,
                radius: 0.0,
                x: px,
                readout: Readout {
                    heading: numbers::value(xv, &x_readout),
                    lines,
                }
                .into(),
            });
        }
        panels.push(kit::xy_panel(ctx, outer, frame, marks, observations, name));
    }
    ctx.note.push(
        "each step is the share of observations at or below a value; \"Observations\" counts those exactly at it"
            .into(),
    );
    ctx.summary = format!(
        "Empirical distribution of {shown} observations{}.",
        if groups.len() > 1 {
            format!(" in {} groups", groups.len())
        } else {
            String::new()
        }
    );
    let legend = kit::legend(ctx, Swatch::Line);
    kit::finish(&plan, facets.len(), panels, legend)
}
