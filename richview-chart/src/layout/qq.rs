//! Normal quantile-quantile charts: each sample value against the standard-normal quantile at its
//! plotting position, with a reference line through the first and third quartile pairs. RichView
//! computes the quantiles and the line, so those readouts are derived and the note names the
//! method (FEA-CH-14, FEA-CH-42).

use super::kit::{self, ValueSide, XAxis};
use super::types::Facets;
use super::{Ctx, POINT_OPACITY, POINT_RADIUS, Painted, Style};
use crate::axis::Zero;
use crate::box_summary;
use crate::derived::{Position, statistic_grid_with_shown};
use crate::diag::Level;
use crate::geometry::{Fill, Item, Observation, Path, Readout, ReadoutLine, Role, Shape, Stroke, Swatch, Targeting};
use crate::numbers::{self, Format, Kind};
use crate::payload::{QUANT_STEPS, Quant};
use crate::rows::{self, Num};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;

const RATIO: f64 = 1.0;
/// Room beyond the data, as a share of its range.
const PAD: f64 = 0.04;

/// The standard-normal quantile at probability `p` in (0, 1): Acklam's rational approximation,
/// accurate to a relative 1.2e-9.
pub(crate) fn normal_quantile(p: f64) -> f64 {
    const A: [f64; 6] = [
        -3.969_683_028_665_376e1,
        2.209_460_984_245_205e2,
        -2.759_285_104_469_687e2,
        1.383_577_518_672_69e2,
        -3.066_479_806_614_716e1,
        2.506_628_277_459_239,
    ];
    const B: [f64; 5] = [
        -5.447_609_879_822_406e1,
        1.615_858_368_580_409e2,
        -1.556_989_798_598_866e2,
        6.680_131_188_771_972e1,
        -1.328_068_155_288_572e1,
    ];
    const C: [f64; 6] = [
        -7.784_894_002_430_293e-3,
        -3.223_964_580_411_365e-1,
        -2.400_758_277_161_838,
        -2.549_732_539_343_734,
        4.374_664_141_464_968,
        2.938_163_982_698_783,
    ];
    const D: [f64; 4] = [
        7.784_695_709_041_462e-3,
        3.224_671_290_700_398e-1,
        2.445_134_137_142_996,
        3.754_408_661_907_416,
    ];
    const LOW: f64 = 0.02425;
    let tail = |q: f64| {
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    };
    if p < LOW {
        tail((-2.0 * p.ln()).sqrt())
    } else if p <= 1.0 - LOW {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    } else {
        -tail((-2.0 * (1.0 - p).ln()).sqrt())
    }
}

/// One sample value in rank order.
struct Entry {
    row: usize,
    value: f64,
    z: f64,
    z_text: String,
}

/// One group's sorted sample in one panel with its reference line `y = intercept + slope * z`.
struct Sample {
    entries: Vec<Entry>,
    line: Option<(f64, f64)>,
    /// Smallest and largest value, which set the value axis.
    extent: [f64; 2],
}

/// What of a group jumps with the data: the rows in rank order and the reference line
/// (TYP-36). Values closer than the grid change their order, so the exact rows decide it.
#[derive(Serialize, Deserialize)]
struct Ranking<P = f64> {
    order: Vec<usize>,
    z: Vec<P>,
    z_texts: Vec<String>,
    /// The first and third quartile pairs, rather than coefficients with hidden digits.
    line: Option<[(P, P); 2]>,
    extent: [P; 2],
}

#[derive(Serialize, Deserialize)]
struct Rankings {
    x_grid: Option<Quant>,
    value_grid: Option<Quant>,
    panels: Vec<Vec<Option<Ranking<Position>>>>,
}

impl Ranking {
    fn encode(self, x: Option<&Quant>, y: Option<&Quant>, z_format: &Format, readout: &Format) -> Ranking<Position> {
        Ranking {
            order: self.order,
            z: self.z.into_iter().map(|v| Position::encode(v, x, z_format)).collect(),
            z_texts: self.z_texts,
            line: self
                .line
                .map(|pairs| pairs.map(|(a, b)| (Position::encode(a, x, z_format), Position::encode(b, y, readout)))),
            extent: self.extent.map(|v| Position::encode(v, y, readout)),
        }
    }
}

impl Ranking<Position> {
    fn decode(self, x: Option<&Quant>, y: Option<&Quant>) -> Ranking {
        Ranking {
            order: self.order,
            z: self.z.into_iter().map(|v| v.at(x)).collect(),
            z_texts: self.z_texts,
            line: self.line.map(|pairs| pairs.map(|(a, b)| (a.at(x), b.at(y)))),
            extent: self.extent.map(|v| v.at(y)),
        }
    }
}

fn ranking(found: &[(usize, f64)], z_format: &Format) -> Ranking {
    let mut sorted = found.to_vec();
    sorted.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
    let line = (sorted.len() >= 2)
        .then(|| {
            let s = box_summary::summarize(&sorted).ok()?;
            let (z1, z3) = (normal_quantile(0.25), normal_quantile(0.75));
            let slope = (s.q3 - s.q1) / (z3 - z1);
            let intercept = s.q1 - slope * z1;
            (slope.is_finite() && intercept.is_finite()).then_some([(z1, s.q1), (z3, s.q3)])
        })
        .flatten();
    let z: Vec<_> = (0..sorted.len())
        .map(|k| normal_quantile((k as f64 + 0.5) / sorted.len() as f64))
        .collect();
    Ranking {
        order: sorted.iter().map(|e| e.0).collect(),
        z_texts: z.iter().map(|v| numbers::value(*v, z_format)).collect(),
        z,
        line,
        extent: [sorted.first().map_or(0.0, |e| e.1), sorted.last().map_or(0.0, |e| e.1)],
    }
}

/// The sample of `found` in the order and with the line of `ranking`.
fn sample(found: &[(usize, f64)], ranking: Ranking) -> Sample {
    let values: HashMap<usize, f64> = found.iter().copied().collect();
    let entries: Vec<Entry> = ranking
        .order
        .into_iter()
        .zip(ranking.z)
        .zip(ranking.z_texts)
        .filter_map(|((row, z), z_text)| {
            Some(Entry {
                row,
                value: *values.get(&row)?,
                z,
                z_text,
            })
        })
        .collect();
    Sample {
        entries,
        line: ranking.line.and_then(|[(z1, q1), (z3, q3)]| {
            let slope = (q3 - q1) / (z3 - z1);
            let intercept = q1 - slope * z1;
            (slope.is_finite() && intercept.is_finite()).then_some((intercept, slope))
        }),
        extent: ranking.extent,
    }
}

/// The part of `y = intercept + slope * x` over `x` that stays inside `y_range`.
fn clip(intercept: f64, slope: f64, x: [f64; 2], y_range: [f64; 2]) -> Option<[[f64; 2]; 2]> {
    let (mut a, mut b) = (x[0], x[1]);
    if slope != 0.0 {
        let (u, v) = ((y_range[0] - intercept) / slope, (y_range[1] - intercept) / slope);
        a = a.max(u.min(v));
        b = b.min(u.max(v));
    } else if intercept < y_range[0] || intercept > y_range[1] {
        return None;
    }
    (a < b).then_some([[a, intercept + slope * a], [b, intercept + slope * b]])
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let y_field = block.encoding.y.clone().unwrap_or_default();
    let groups: Vec<String> = match &block.encoding.color {
        Some(f) => rows::distinct(ctx.rows, f),
        None => vec![block.y.label.clone().unwrap_or_else(|| y_field.clone())],
    };
    ctx.set_series(&groups);
    let label_format = block.label_format(&block.y, ctx.locale);
    let readout = block.readout_format(&block.y, ctx.locale);
    let z_format = Format {
        kind: Kind::Number,
        unit: None,
        digits: block.readout_digits,
        locale: ctx.locale,
    };
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut excluded = 0usize;
    let mut per_panel = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut found: Vec<Vec<(usize, f64)>> = groups.iter().map(|_| Vec::new()).collect();
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
            match rows::num(row, &y_field) {
                Num::Value(v) => found[group].push((i, v)),
                Num::Missing => excluded += 1,
                Num::Underflow(t) => {
                    ctx.underflow(i, &y_field, &t);
                    failures[fi].get_or_insert_with(|| {
                        format!("Cannot draw this quantile plot · {y_field} has a value too small to represent")
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
                        format!("Cannot draw this quantile plot · {y_field} has text that is not a number")
                    });
                }
            }
        }
        if failures[fi].is_none() && found.iter().all(Vec::is_empty) {
            failures[fi] = Some("Cannot draw this quantile plot · No row has a value".into());
        }
        per_panel.push(found);
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
                "{} without a value or group {} left out",
                super::count(excluded, "row", "rows"),
                super::verb(excluded, "is", "are")
            ),
            Some("Give every row a number."),
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
    // Exact rows decide ranks and quartiles once. Native and replay painters both read the
    // recorded positions; theoretical quantile text never comes from a grid coordinate.
    let records: Rankings = ctx.derive_projected(0, "qq.ranking", || {
        let panels: Vec<Vec<Option<Ranking>>> = per_panel
            .iter()
            .enumerate()
            .map(|(fi, found)| {
                found
                    .iter()
                    .map(|f| (!f.is_empty() && failures[fi].is_none()).then(|| ranking(f, &z_format)))
                    .collect()
            })
            .collect();
        let theoretical: Vec<_> = panels
            .iter()
            .flatten()
            .flatten()
            .flat_map(|r| r.z.iter().copied().chain(r.line.iter().flatten().map(|p| p.0)))
            .collect();
        let x_grid = Quant::for_values(
            theoretical.iter().copied(),
            theoretical.iter().map(|z| numbers::shown(*z, &z_format)),
        )
        .map(|mut grid| {
            // Refine by a whole multiple so the theoretical scale keeps its exact zero.
            if grid.steps < QUANT_STEPS {
                grid.steps *= QUANT_STEPS.div_ceil(grid.steps);
            }
            grid.resolution = None;
            grid
        });
        let value_grid = statistic_grid_with_shown(
            ctx.grid_for(&y_field),
            panels
                .iter()
                .flatten()
                .flatten()
                .flat_map(|r| r.extent.into_iter().chain(r.line.iter().flatten().map(|p| p.1)))
                .map(|v| numbers::shown(v, &readout)),
        );
        Rankings {
            panels: panels
                .into_iter()
                .map(|rankings| {
                    rankings
                        .into_iter()
                        .map(|r| r.map(|r| r.encode(x_grid.as_ref(), value_grid.as_ref(), &z_format, &readout)))
                        .collect()
                })
                .collect(),
            x_grid,
            value_grid,
        }
    });
    let data: Vec<Vec<Option<Sample>>> = per_panel
        .iter()
        .zip(records.panels)
        .map(|(found, rankings)| {
            found
                .iter()
                .zip(rankings)
                .map(|(f, r)| r.map(|r| sample(f, r.decode(records.x_grid.as_ref(), records.value_grid.as_ref()))))
                .collect()
        })
        .collect();
    let plan = kit::Plan::xy(ctx, facets.len(), RATIO);
    let frames = plan.frames(facets.len(), ctx.opts.width);
    let y_title = block.y.label.clone().unwrap_or_else(|| y_field.clone());
    let x_title = block.x.label.clone().unwrap_or_else(|| "Theoretical quantile".into());
    let visible = |ctx: &Ctx, fi: usize| -> Vec<(usize, &Sample)> {
        data[fi]
            .iter()
            .enumerate()
            .filter(|(g, _)| !ctx.is_hidden(*g))
            .filter_map(|(g, s)| s.as_ref().map(|s| (g, s)))
            .collect()
    };
    let extents = |fis: &mut dyn Iterator<Item = usize>| {
        let mut x = [f64::INFINITY, f64::NEG_INFINITY];
        let mut y = [f64::INFINITY, f64::NEG_INFINITY];
        for fi in fis {
            for s in data[fi].iter().flatten() {
                for e in &s.entries {
                    x = [x[0].min(e.z), x[1].max(e.z)];
                }
                y = [y[0].min(s.extent[0]), y[1].max(s.extent[1])];
            }
        }
        (x, y)
    };
    let shared = extents(&mut (0..facets.len()).filter(|&fi| failures[fi].is_none()));
    let padded = |e: [f64; 2]| {
        let span = e[1] - e[0];
        let pad = if span > 0.0 {
            span * PAD
        } else if e[0] == 0.0 {
            1.0
        } else {
            e[0].abs() * 0.1
        };
        [e[0] - pad, e[1] + pad]
    };
    let mut panels = Vec::new();
    let mut shown = 0usize;
    for (fi, ((name, _), outer)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *outer, plan.title_room, name, reason));
            continue;
        }
        let (x, y) = if block.style.free_scales {
            extents(&mut std::iter::once(fi))
        } else {
            shared
        };
        let (x, y) = if x[0].is_finite() {
            (
                super::Fit::widen(padded(x), ctx.wanted().x),
                super::Fit::widen(padded(y), ctx.wanted().y),
            )
        } else {
            (
                ctx.wanted().x.unwrap_or([-1.0, 1.0]),
                ctx.wanted().y.unwrap_or([0.0, 1.0]),
            )
        };
        if ![x[0], x[1], y[0], y[1], y[1] - y[0]].iter().all(|v| v.is_finite()) {
            panels.push(super::failed_facet(
                ctx,
                *outer,
                plan.title_room,
                name,
                "Cannot draw this quantile plot · The values are too extreme to represent",
            ));
            continue;
        }
        ctx.fit.x = Some(x);
        ctx.fit.y = Some(y);
        let side = ValueSide {
            data: y,
            zero: Zero::None,
            title: y_title.clone(),
            format: &label_format,
            min: block.y.min,
            max: block.y.max,
        };
        let frame = kit::xy_frame(
            ctx,
            &plan,
            outer,
            &XAxis::Numeric { extent: x },
            Some(x_title.as_str()),
            &side,
        );
        let [d0, d1] = frame.scale.domain;
        let mut marks: Vec<Item> = Vec::new();
        let mut observations = Vec::new();
        let multi = block.encoding.color.is_some();
        for (g, s) in visible(ctx, fi) {
            let color = Style::rgba(ctx.series_color(g));
            if let Some((intercept, slope)) = s.line
                && let Some(seg) = clip(intercept, slope, [d0, d1], [frame.value.lo, frame.value.hi])
            {
                let pts: Vec<[f64; 2]> = seg.iter().map(|p| [frame.x(p[0]), frame.y(p[1])]).collect();
                marks.push(Item::Line {
                    path: Path::polyline(&pts),
                    stroke: Stroke {
                        dash: vec![6.0, 4.0],
                        ..Stroke::solid(color, 1.5)
                    },
                    role: Role::Annotation,
                    series: Some(g),
                    obs: None,
                });
            }
            let shape = ctx.series[g].shape;
            let n = s.entries.len();
            for (k, e) in s.entries.iter().enumerate() {
                if e.z < d0 || e.z > d1 {
                    continue;
                }
                let center = [frame.x(e.z), frame.y(e.value)];
                let obs = observations.len();
                marks.push(match shape {
                    Some(shape) => Item::Point {
                        center,
                        radius: POINT_RADIUS,
                        shape,
                        fill: None,
                        stroke: Some(Stroke::solid(color, 1.5)),
                        role: Role::Mark,
                        series: Some(g),
                        obs: Some(obs),
                    },
                    None => Item::Point {
                        center,
                        radius: POINT_RADIUS,
                        shape: Shape::Circle,
                        fill: Some(Fill::solid(color.with_alpha(POINT_OPACITY))),
                        stroke: None,
                        role: Role::Mark,
                        series: Some(g),
                        obs: Some(obs),
                    },
                });
                let mut lines = vec![
                    ReadoutLine {
                        label: None,
                        name: block
                            .y
                            .label
                            .clone()
                            .unwrap_or_else(|| super::cartesian::readable_name(&y_field)),
                        value: numbers::value(e.value, &readout),
                        series: Some(g),
                        names_series: false,
                        derived: false,
                    },
                    ReadoutLine {
                        label: None,
                        name: x_title.clone(),
                        value: e.z_text.clone(),
                        series: None,
                        names_series: false,
                        derived: true,
                    },
                    ReadoutLine {
                        label: None,
                        name: "Rank".into(),
                        value: format!("{} of {n}", k + 1),
                        series: None,
                        names_series: false,
                        derived: true,
                    },
                ];
                let label_on_y = block.encoding.label.as_ref() == Some(&y_field);
                let heading = if label_on_y {
                    format!("{} {}", lines[0].name, lines[0].value)
                } else {
                    block
                        .encoding
                        .label
                        .as_ref()
                        .and_then(|f| rows::text(&ctx.rows[e.row], f))
                        .or_else(|| block.encoding.color.as_ref().map(|_| groups[g].clone()))
                        .unwrap_or_else(|| format!("Row {}", e.row + 1))
                };
                if label_on_y {
                    lines.remove(0);
                }
                if multi && block.encoding.color != block.encoding.label && heading != groups[g] {
                    lines.push(ReadoutLine {
                        label: None,
                        name: block
                            .encoding
                            .color
                            .as_deref()
                            .map(super::cartesian::readable_name)
                            .unwrap_or_default(),
                        value: groups[g].clone(),
                        series: Some(g),
                        names_series: true,
                        derived: false,
                    });
                }
                observations.push(Observation {
                    row: Some(e.row),
                    dots: Vec::new(),
                    series: Some(g),
                    at: center,
                    wedge: None,
                    area: None,
                    radius: POINT_RADIUS,
                    x: center[0],
                    readout: Readout { heading, lines }.into(),
                });
            }
            shown += n;
        }
        let mut panel = kit::xy_panel(ctx, outer, frame, marks, observations, name);
        panel.targeting = Targeting::Radius;
        panels.push(panel);
    }
    ctx.note.push(
        "x is the standard-normal quantile at plotting position (rank - 0.5) / n; the dashed line runs through the \
         first and third quartile pairs"
            .into(),
    );
    let visible_groups = (0..groups.len()).filter(|g| !ctx.is_hidden(*g)).count();
    ctx.summary = format!(
        "Normal quantile plot of {}{}.",
        super::count(shown, "observation", "observations"),
        if visible_groups > 1 {
            format!(" in {visible_groups} groups")
        } else {
            String::new()
        }
    );
    let legend = kit::legend(ctx, Swatch::Dot);
    kit::finish(&plan, facets.len(), panels, legend)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_inverse_normal_matches_known_values() {
        for (p, z) in [
            (0.975, 1.959_963_984_540_054),
            (0.5, 0.0),
            (0.84134474606854, 1.0),
            (0.995, 2.575_829_303_548_901),
            (0.025, -1.959_963_984_540_054),
            (0.001, -3.090_232_306_167_813),
            (0.75, 0.674_489_750_196_081_7),
        ] {
            assert!(
                (normal_quantile(p) - z).abs() < 1e-7,
                "{p}: {} vs {z}",
                normal_quantile(p)
            );
        }
        assert!((normal_quantile(0.975) - 1.95996).abs() < 1e-5);
    }

    #[test]
    fn the_quantile_is_odd_about_one_half() {
        for p in [0.001, 0.02, 0.1, 0.3, 0.49] {
            assert!((normal_quantile(p) + normal_quantile(1.0 - p)).abs() < 1e-9);
        }
    }

    #[test]
    fn the_reference_line_runs_through_the_quartile_pairs() {
        // 1..=5 has quartiles 2 and 4 (R-7), against the normal quartiles -0.6745 and 0.6745.
        let found: Vec<(usize, f64)> = (0..5).map(|i| (i, i as f64 + 1.0)).collect();
        let s = sample(&found, ranking(&found, &Format::default()));
        let (a, b) = s.line.unwrap();
        let z3 = normal_quantile(0.75);
        assert!((a - 3.0).abs() < 1e-9);
        assert!((b - 2.0 / (2.0 * z3)).abs() < 1e-9);
    }

    #[test]
    fn a_line_is_cut_to_the_axis() {
        let seg = clip(0.0, 1.0, [-10.0, 10.0], [-2.0, 3.0]).unwrap();
        assert_eq!(seg, [[-2.0, -2.0], [3.0, 3.0]]);
        assert!(clip(5.0, 0.0, [-1.0, 1.0], [0.0, 1.0]).is_none());
    }
}
