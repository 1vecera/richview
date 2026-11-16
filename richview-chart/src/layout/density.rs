//! Density charts: a Gaussian kernel density estimate per group over a numeric sample. RichView
//! computes the curve, so every value it reads out is derived and the note names the method and
//! the bandwidth (FEA-CH-14, FEA-CH-42).

use super::kit::{self, ValueSide, XAxis};
use super::types::Facets;
use super::{BAND_OPACITY, Ctx, LINE_WIDTH, Painted, Style};
use crate::axis::Zero;
use crate::box_summary;
use crate::derived::{Position, statistic_grid, statistic_grid_with_shown};
use crate::diag::Level;
use crate::geometry::{Dot, Fill, Item, Observation, Path, Readout, ReadoutLine, Role, Stroke, Swatch};
use crate::numbers::{self, Format};
use crate::payload::Quant;
use crate::rows::{self, Num};
use serde::{Deserialize, Serialize};
use serde_json::json;

const RATIO: f64 = 16.0 / 9.0;
/// Fewest points of the drawn curve.
const GRID: usize = 128;
/// Most points of the drawn curve.
const MAX_GRID: usize = 2048;
/// Grid steps per bandwidth: fine enough that the curve is smooth wherever it has mass.
const PER_BANDWIDTH: f64 = 3.0;
/// Bandwidths of padding either side of the data.
const PAD: f64 = 3.0;
/// Full-view readout positions: one per this share of the recorded range.
const STEPS: usize = 16;
/// At most 257 shared exact-source readout anchors per curve, including the 17 full-view anchors.
const FINE_ANCHORS: usize = 240;

/// One group's estimate in one panel.
struct Curve {
    /// Bandwidth in the units of the data.
    h: f64,
    /// Scale that maps the data into [-1, 1] so extreme values do not overflow.
    scale: f64,
    /// The sample divided by `scale`.
    sample: Vec<f64>,
    /// Bandwidth in scaled units.
    hs: f64,
    /// The curve's grid and densities.
    grid: Vec<[f64; 2]>,
    count: usize,
}

/// What of a group's estimate jumps with the data, in the units of the data (TYP-36).
#[derive(Serialize, Deserialize)]
struct Shape<P = f64> {
    grid: Vec<[P; 2]>,
    count: usize,
}

/// How a group's estimate came out.
#[derive(Serialize, Deserialize)]
enum Outcome<T> {
    /// The group has this many distinct values, too few for a curve.
    Few(usize),
    Drawn(T),
    /// The estimate cannot be represented, and why.
    Failed(String),
}

#[derive(Serialize, Deserialize)]
struct Evaluation<P = f64> {
    x: P,
    full: bool,
    heading: String,
    /// Each group's density position and exact-statistic text, when its curve spans this x.
    values: Vec<Option<(P, String)>>,
}

#[derive(Serialize, Deserialize)]
struct PanelEstimate<P = f64> {
    groups: Vec<Option<Outcome<Shape<P>>>>,
    readouts: Vec<Evaluation<P>>,
}

#[derive(Serialize, Deserialize)]
struct Estimates {
    x_grid: Option<Quant>,
    density_grid: Option<Quant>,
    bandwidth: [String; 2],
    panels: Vec<PanelEstimate<Position>>,
}

impl Curve {
    fn range(&self) -> [f64; 2] {
        [self.grid[0][0], self.grid[self.grid.len() - 1][0]]
    }
    /// The estimate at data value `x`.
    fn at(&self, x: f64) -> f64 {
        estimate(&self.sample, self.hs, self.scale, x / self.scale)
    }
}

impl Shape {
    fn range(&self) -> [f64; 2] {
        [self.grid[0][0], self.grid[self.grid.len() - 1][0]]
    }

    fn max(&self) -> f64 {
        self.grid.iter().map(|p| p[1]).fold(0.0, f64::max)
    }
}

/// Extend the source's statistic lattice for Gaussian tails, preserving its origin and step.
fn padded_grid(source: Option<Quant>, positions: &[f64], format: &Format) -> Option<Quant> {
    let Some(grid) = statistic_grid_with_shown(source, positions.iter().map(|p| numbers::shown(*p, format))) else {
        return statistic_grid(Quant::for_values(
            positions.iter().copied(),
            positions.iter().map(|p| numbers::shown(*p, format)),
        ));
    };
    let step = (grid.max - grid.min) / f64::from(grid.steps);
    let lo = positions.iter().copied().fold(grid.min, f64::min);
    let hi = positions.iter().copied().fold(grid.max, f64::max);
    let before = ((grid.min - lo) / step).ceil().max(0.0);
    let after = ((hi - grid.max) / step).ceil().max(0.0);
    let steps = f64::from(grid.steps) + before + after;
    let (min, max) = (grid.min - before * step, grid.max + after * step);
    (steps <= f64::from(u32::MAX >> 1) && min.is_finite() && max.is_finite() && (max - min).is_finite()).then_some(
        Quant {
            min,
            max,
            steps: steps as u32,
            resolution: None,
        },
    )
}

/// Keep the 17 full-view positions and at most 240 finer shared positions, sampling both
/// the adaptive curve grids and the whole range so a narrow group retains zoom readouts.
fn readout_anchors<'a>(range: [f64; 2], curves: impl Iterator<Item = &'a Curve>) -> Vec<(f64, bool)> {
    let at = |t: f64| range[0] * (1.0 - t) + range[1] * t;
    let mut candidates: Vec<_> = curves
        .flat_map(|c| c.grid.iter().map(|p| p[0]))
        .chain((0..FINE_ANCHORS).map(|k| at(k as f64 / (FINE_ANCHORS - 1) as f64)))
        .collect();
    candidates.sort_by(f64::total_cmp);
    candidates.dedup();
    let n = candidates.len().min(FINE_ANCHORS);
    let mut anchors: Vec<_> = (0..=STEPS).map(|k| (at(k as f64 / STEPS as f64), true)).collect();
    anchors.extend((0..n).map(|k| {
        (
            candidates[k * (candidates.len() - 1) / n.saturating_sub(1).max(1)],
            false,
        )
    }));
    anchors.sort_by(|a, b| a.0.total_cmp(&b.0));
    anchors.dedup_by(|later, earlier| {
        if later.0 == earlier.0 {
            earlier.1 |= later.1;
            true
        } else {
            false
        }
    });
    anchors
}

/// The Gaussian kernel density estimate at scaled position `u`, in units of the data.
fn estimate(sample: &[f64], hs: f64, scale: f64, u: f64) -> f64 {
    let norm = (2.0 * std::f64::consts::PI).sqrt();
    let sum: f64 = sample
        .iter()
        .map(|&v| {
            let z = (u - v) / hs;
            (-0.5 * z * z).exp()
        })
        .sum();
    sum / (norm * hs * scale * sample.len() as f64)
}

/// Silverman's rule of thumb: 0.9 times the smaller of the standard deviation and the
/// interquartile range over 1.34, times n to the minus one fifth. A zero interquartile range
/// falls back to the standard deviation.
fn silverman(sample: &[f64]) -> Option<f64> {
    let n = sample.len() as f64;
    let mean = sample.iter().sum::<f64>() / n;
    let sd = (sample.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt();
    let indexed: Vec<(usize, f64)> = sample.iter().copied().enumerate().collect();
    let s = box_summary::summarize(&indexed).ok()?;
    let spread = (s.q3 - s.q1) / 1.34;
    let low = if spread > 0.0 { sd.min(spread) } else { sd };
    let h = 0.9 * low * n.powf(-0.2);
    (h.is_finite() && h > 0.0).then_some(h)
}

/// Scaled positions where the curve is evaluated: a step of at most a third of the bandwidth
/// wherever the data lies (within [`PAD`] bandwidths of a value), so skewed data keeps its bulk
/// whatever sits far away, and nothing between distant clumps, where the density is nil. Steps
/// widen only to stay within [`MAX_GRID`] points, and the grid has at least [`GRID`].
fn grid_points(sample: &[f64], hs: f64) -> Vec<f64> {
    let mut sorted = sample.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mut spans: Vec<[f64; 2]> = Vec::new();
    for v in sorted {
        let (a, b) = (v - PAD * hs, v + PAD * hs);
        match spans.last_mut() {
            Some(last) if a <= last[1] => last[1] = last[1].max(b),
            _ => spans.push([a, b]),
        }
    }
    let total: f64 = spans.iter().map(|s| s[1] - s[0]).sum();
    let step = (hs / PER_BANDWIDTH)
        .min(total / (GRID - 1) as f64)
        .max(total / MAX_GRID.saturating_sub(spans.len()).max(1) as f64);
    let mut points = Vec::new();
    for [a, b] in spans {
        let n = (((b - a) / step).ceil() as usize).max(1);
        points.extend((0..=n).map(|j| a + (b - a) * j as f64 / n as f64));
    }
    points
}

/// Estimate `values`: bandwidth, grid and densities, or why there is no curve.
fn estimate_group(values: &[f64], bandwidth: Option<f64>) -> Result<Curve, &'static str> {
    let scale = values.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    if !scale.is_finite() || scale <= 0.0 {
        return Err("values too large to represent");
    }
    let sample: Vec<f64> = values.iter().map(|v| v / scale).collect();
    let hs = match bandwidth {
        Some(h) => h / scale,
        None => silverman(&sample).ok_or("the bandwidth cannot be represented")?,
    };
    if !hs.is_finite() || hs <= 0.0 {
        return Err("the bandwidth cannot be represented");
    }
    let grid: Vec<[f64; 2]> = grid_points(&sample, hs)
        .into_iter()
        .map(|u| [u * scale, estimate(&sample, hs, scale, u)])
        .collect();
    let h = hs * scale;
    if !h.is_finite() || grid.iter().any(|p| !p[0].is_finite() || !p[1].is_finite()) {
        return Err("values too extreme to represent");
    }
    Ok(Curve {
        h,
        scale,
        sample,
        hs,
        grid,
        count: values.len(),
    })
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let x_field = block.encoding.x.clone().unwrap_or_default();
    let groups: Vec<String> = match &block.encoding.color {
        Some(f) => rows::distinct(ctx.rows, f),
        None => vec![block.x.label.clone().unwrap_or_else(|| x_field.clone())],
    };
    ctx.set_series(&groups);
    let label_format = block.label_format(&block.y, ctx.locale);
    let readout = block.readout_format(&block.y, ctx.locale);
    let x_readout = block.readout_format(&block.x, ctx.locale);
    let given = block
        .raw
        .get("style")
        .and_then(|s| s.get("bandwidth"))
        .filter(|v| !v.is_null());
    let bandwidth = match block.style_number("bandwidth") {
        Some(h) if h > 0.0 && h.is_finite() => Some(h),
        _ => {
            if let Some(v) = given {
                ctx.check(
                    "style-invalid",
                    Level::Warning,
                    format!("style.bandwidth is {v}; it must be a positive number, so Silverman's rule is used"),
                    Some("Give a positive bandwidth, or remove it."),
                    Some("chart.style.bandwidth".into()),
                    None,
                    json!({"bandwidth": v}),
                );
            }
            None
        }
    };
    let fell_back = bandwidth.is_none() && given.is_some();
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut excluded = 0usize;
    let mut per_panel = Vec::new();
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
                        format!("Cannot draw this density · {x_field} has a value too small to represent")
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
                        format!("Cannot draw this density · {x_field} has text that is not a number")
                    });
                }
            }
        }
        per_panel.push(values);
    }
    // Every estimate and readout is computed from exact source values only when this author
    // state's record is absent. Width, theme, hide and zoom only paint/clip the record.
    let records: Estimates = ctx.derive_projected(0, "density.estimates", || {
        let raw: Vec<Vec<Option<Outcome<Curve>>>> = per_panel
            .iter()
            .enumerate()
            .map(|(fi, values)| {
                values
                    .iter()
                    .map(|v| {
                        if failures[fi].is_some() || v.is_empty() {
                            return None;
                        }
                        let mut distinct = v.clone();
                        distinct.sort_by(f64::total_cmp);
                        distinct.dedup();
                        Some(if distinct.len() < 2 {
                            Outcome::Few(distinct.len())
                        } else {
                            match estimate_group(v, bandwidth) {
                                Ok(c) => Outcome::Drawn(c),
                                Err(why) => Outcome::Failed(why.to_owned()),
                            }
                        })
                    })
                    .collect()
            })
            .collect();
        let valid =
            |fi: usize| failures[fi].is_none() && !raw[fi].iter().any(|o| matches!(o, Some(Outcome::Failed(_))));
        let curves = |fi: usize| {
            raw[fi].iter().filter_map(|o| match o {
                Some(Outcome::Drawn(c)) => Some(c),
                _ => None,
            })
        };
        let extent = |indices: &mut dyn Iterator<Item = usize>| {
            indices
                .flat_map(curves)
                .fold([f64::INFINITY, f64::NEG_INFINITY], |e, c| {
                    let r = c.range();
                    [e[0].min(r[0]), e[1].max(r[1])]
                })
        };
        let shared = extent(&mut (0..raw.len()).filter(|fi| valid(*fi)));
        let evaluations: Vec<Vec<Evaluation>> = (0..raw.len())
            .map(|fi| {
                let range = if block.style.free_scales {
                    extent(&mut std::iter::once(fi))
                } else {
                    shared
                };
                if !valid(fi) || !range.iter().all(|v| v.is_finite()) {
                    return Vec::new();
                }
                let indices: Vec<_> = if block.style.free_scales {
                    vec![fi]
                } else {
                    (0..raw.len()).filter(|fi| valid(*fi)).collect()
                };
                readout_anchors(range, indices.into_iter().flat_map(curves))
                    .into_iter()
                    .map(|(x, full)| Evaluation {
                        x,
                        full,
                        heading: numbers::value(x, &x_readout),
                        values: raw[fi]
                            .iter()
                            .map(|outcome| match outcome {
                                Some(Outcome::Drawn(c)) if x >= c.range()[0] && x <= c.range()[1] => {
                                    let d = c.at(x);
                                    Some((d, numbers::value(d, &readout)))
                                }
                                _ => None,
                            })
                            .collect(),
                    })
                    .collect()
            })
            .collect();
        let representable: Vec<_> = evaluations
            .iter()
            .map(|p| p.iter().all(|e| e.values.iter().flatten().all(|v| v.0.is_finite())))
            .collect();
        let valid = |fi: usize| valid(fi) && representable[fi];
        let positions: Vec<_> = (0..raw.len())
            .filter(|fi| valid(*fi))
            .flat_map(curves)
            .flat_map(|c| c.grid.iter().map(|p| p[0]))
            .chain(
                evaluations
                    .iter()
                    .enumerate()
                    .filter(|(fi, _)| valid(*fi))
                    .flat_map(|(_, p)| p.iter().map(|e| e.x)),
            )
            .collect();
        let x_grid = padded_grid(ctx.grid_for(&x_field), &positions, &x_readout);
        let densities: Vec<_> = (0..raw.len())
            .filter(|fi| valid(*fi))
            .flat_map(curves)
            .flat_map(|c| c.grid.iter().map(|p| p[1]))
            .chain(
                evaluations
                    .iter()
                    .enumerate()
                    .filter(|(fi, _)| valid(*fi))
                    .flat_map(|(_, p)| p)
                    .flat_map(|e| e.values.iter().flatten().map(|v| v.0)),
            )
            .collect();
        let density_grid = statistic_grid(Quant::for_values(
            densities.iter().copied().chain([0.0]),
            densities.iter().map(|d| numbers::shown(*d, &readout)),
        ));
        let (lo, hi) = (0..raw.len())
            .filter(|fi| valid(*fi))
            .flat_map(curves)
            .map(|c| c.h)
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), h| (lo.min(h), hi.max(h)));
        let bandwidth = if lo.is_finite() && hi.is_finite() {
            [numbers::value(lo, &x_readout), numbers::value(hi, &x_readout)]
        } else {
            Default::default()
        };
        Estimates {
            panels: raw
                .into_iter()
                .zip(evaluations)
                .enumerate()
                .map(|(fi, (groups, readouts))| PanelEstimate {
                    groups: groups
                        .into_iter()
                        .map(|o| {
                            o.map(|o| match o {
                                Outcome::Few(n) => Outcome::Few(n),
                                Outcome::Failed(why) => Outcome::Failed(why),
                                Outcome::Drawn(_) if !representable[fi] => {
                                    Outcome::Failed("the density is too extreme to represent".into())
                                }
                                Outcome::Drawn(c) => Outcome::Drawn(Shape {
                                    count: c.count,
                                    grid: c
                                        .grid
                                        .into_iter()
                                        .map(|[x, y]| {
                                            [
                                                Position::encode(x, x_grid.as_ref(), &x_readout),
                                                Position::encode(y, density_grid.as_ref(), &readout),
                                            ]
                                        })
                                        .collect(),
                                }),
                            })
                        })
                        .collect(),
                    readouts: readouts
                        .into_iter()
                        .filter(|_| representable[fi])
                        .map(|e| Evaluation {
                            x: Position::encode(e.x, x_grid.as_ref(), &x_readout),
                            full: e.full,
                            heading: e.heading,
                            values: e
                                .values
                                .into_iter()
                                .map(|v| {
                                    v.map(|(d, text)| (Position::encode(d, density_grid.as_ref(), &readout), text))
                                })
                                .collect(),
                        })
                        .collect(),
                })
                .collect(),
            x_grid,
            density_grid,
            bandwidth,
        }
    });
    let mut data: Vec<Vec<Option<Shape>>> = Vec::new();
    let mut readouts = Vec::new();
    for (fi, panel) in records.panels.into_iter().enumerate() {
        let mut curves = Vec::new();
        for (g, outcome) in panel.groups.into_iter().enumerate() {
            let Some(outcome) = outcome else {
                curves.push(None);
                continue;
            };
            match outcome {
                Outcome::Few(distinct) => {
                    let who = if block.encoding.color.is_some() {
                        format!("The group '{}'", groups[g])
                    } else {
                        "The data".to_owned()
                    };
                    ctx.check(
                        "too-few-observations",
                        Level::Warning,
                        format!("{who} has fewer than 2 distinct values, so no curve is drawn"),
                        Some("A density needs at least two different values per group."),
                        Some("chart.encoding.x".into()),
                        None,
                        json!({"group": groups[g], "distinct": distinct}),
                    );
                    curves.push(None);
                }
                Outcome::Drawn(shape) => curves.push(Some(Shape {
                    count: shape.count,
                    grid: shape
                        .grid
                        .into_iter()
                        .map(|[x, y]| [x.at(records.x_grid.as_ref()), y.at(records.density_grid.as_ref())])
                        .collect(),
                })),
                Outcome::Failed(why) => {
                    failures[fi].get_or_insert_with(|| format!("Cannot draw this density · {why}"));
                    curves.push(None);
                }
            }
        }
        readouts.push(
            panel
                .readouts
                .into_iter()
                .map(|e| Evaluation {
                    x: e.x.at(records.x_grid.as_ref()),
                    full: e.full,
                    heading: e.heading,
                    values: e
                        .values
                        .into_iter()
                        .map(|v| v.map(|(d, text)| (d.at(records.density_grid.as_ref()), text)))
                        .collect(),
                })
                .collect::<Vec<_>>(),
        );
        if failures[fi].is_none() && curves.iter().all(Option::is_none) {
            failures[fi] = Some("Cannot draw this density · No group has two distinct values".into());
        }
        data.push(curves);
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
    let title = block.y.label.clone().unwrap_or_else(|| "Density".into());
    let x_title = block.x.label.clone().unwrap_or_else(|| x_field.clone());
    let visible = |ctx: &Ctx, fi: usize| -> Vec<(usize, &Shape)> {
        data[fi]
            .iter()
            .enumerate()
            .filter(|(g, _)| !ctx.is_hidden(*g))
            .filter_map(|(g, c)| c.as_ref().map(|c| (g, c)))
            .collect()
    };
    let extents = |fis: &mut dyn Iterator<Item = usize>| {
        let mut x = [f64::INFINITY, f64::NEG_INFINITY];
        let mut top = 0.0f64;
        for fi in fis {
            for c in data[fi].iter().flatten() {
                x = [x[0].min(c.range()[0]), x[1].max(c.range()[1])];
                top = top.max(c.max());
            }
            for (density, _) in readouts[fi].iter().flat_map(|e| e.values.iter().flatten()) {
                top = top.max(*density);
            }
        }
        (x, top)
    };
    let shared = extents(&mut (0..facets.len()).filter(|&fi| failures[fi].is_none()));
    let mut panels = Vec::new();
    let mut shown = 0usize;
    for (fi, ((name, _), outer)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *outer, plan.title_room, name, reason));
            continue;
        }
        let (x, top) = if block.style.free_scales {
            extents(&mut std::iter::once(fi))
        } else {
            shared
        };
        let x = if x[0].is_finite() {
            super::Fit::widen(x, ctx.wanted().x)
        } else {
            ctx.wanted().x.unwrap_or([0.0, 1.0])
        };
        let y = super::Fit::widen([0.0, if top > 0.0 { top } else { 1.0 }], ctx.wanted().y);
        ctx.fit.x = Some(x);
        ctx.fit.y = Some(y);
        let side = ValueSide {
            data: y,
            zero: Zero::Include,
            title: title.clone(),
            format: &label_format,
            min: block.y.min,
            max: block.y.max,
        };
        let frame = kit::xy_frame(ctx, &plan, outer, &XAxis::Numeric { extent: x }, Some(&x_title), &side);
        let mut marks: Vec<Item> = Vec::new();
        let curves = visible(ctx, fi);
        for &(g, c) in &curves {
            let color = Style::rgba(ctx.series_color(g));
            let hatch = ctx.series[g].hatch;
            let surface = Style::rgba(ctx.style.surface);
            let pts: Vec<[f64; 2]> = c.grid.iter().map(|p| [frame.x(p[0]), frame.y(p[1])]).collect();
            let base = frame.y(0.0);
            let mut area = Path::new();
            area.move_to([pts[0][0], base]);
            for p in &pts {
                area.line_to(*p);
            }
            area.line_to([pts[pts.len() - 1][0], base]);
            area.close();
            marks.push(Item::Area {
                path: area,
                fill: Fill {
                    color: color.with_alpha(BAND_OPACITY),
                    hatch,
                    hatch_ink: hatch.map(|_| surface),
                    hatch_scale: 1.0,
                },
                role: Role::Band,
                series: Some(g),
                obs: None,
            });
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
            shown += c.count;
        }
        // Full view uses the 17 stable anchors. Zoom clips the bounded finer recorded set.
        let [d0, d1] = frame.scale.domain;
        let multi = block.encoding.color.is_some();
        let mut observations = Vec::new();
        for evaluation in &readouts[fi] {
            if ctx.opts.zoom.is_none() && !evaluation.full {
                continue;
            }
            let xv = evaluation.x;
            if xv < d0 || xv > d1 {
                continue;
            }
            let px = frame.x(xv);
            let mut lines = Vec::new();
            let mut dots = Vec::new();
            for (g, value) in evaluation.values.iter().enumerate() {
                if ctx.is_hidden(g) {
                    continue;
                }
                let Some((d, text)) = value else {
                    continue;
                };
                let prefix = if multi {
                    format!("{} · ", groups[g])
                } else {
                    String::new()
                };
                lines.push(ReadoutLine {
                    name: format!("{prefix}Density"),
                    value: text.clone(),
                    series: Some(g),
                    names_series: multi,
                    derived: true,
                });
                dots.push(Dot {
                    at: [px, frame.y(*d)],
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
                    heading: evaluation.heading.clone(),
                    lines,
                }
                .into(),
            });
        }
        let mut panel = kit::xy_panel(ctx, outer, frame, marks, observations, name);
        super::place::value_labels(ctx, fi, &mut panel, super::place::ValueLabels::Density);
        panels.push(panel);
    }
    let note = |h: String, how: &str| format!("Gaussian kernel density estimate, bandwidth h = {h} ({how})");
    let how = if bandwidth.is_some() {
        "set by the author"
    } else if fell_back {
        "Silverman's rule of thumb, because style.bandwidth is not a positive number"
    } else {
        "Silverman's rule of thumb"
    };
    let [flo, fhi] = records.bandwidth;
    ctx.note.push(if flo == fhi {
        note(flo, how)
    } else {
        note(format!("{flo} to {fhi}, one per group"), how)
    });
    let visible_groups = (0..groups.len())
        .filter(|g| !ctx.is_hidden(*g) && data.iter().any(|p| p[*g].is_some()))
        .count();
    ctx.summary = format!(
        "Density of {}{}.",
        super::count(shown, "observation", "observations"),
        if visible_groups > 1 {
            format!(" in {visible_groups} groups")
        } else {
            String::new()
        }
    );
    let legend = kit::legend(ctx, Swatch::Square);
    kit::finish(&plan, facets.len(), panels, legend)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_estimate_matches_a_hand_computed_value() {
        // Two points at 0 and 1, h = 0.5: at 0.5 each term is phi(1) / (2 * 0.5).
        let c = estimate_group(&[0.0, 1.0], Some(0.5)).unwrap();
        assert!((c.at(0.5) - 2.0 * 0.241_970_724_519_143_37).abs() < 1e-12);
    }

    #[test]
    fn silverman_follows_the_rule_of_thumb() {
        // 1..=5: sd = sqrt(2.5), IQR = 2 so IQR / 1.34 = 1.4925 < sd = 1.5811.
        let h = silverman(&[1.0, 2.0, 3.0, 4.0, 5.0]).unwrap();
        assert!((h - 0.9 * (2.0 / 1.34) * 5f64.powf(-0.2)).abs() < 1e-12);
    }

    #[test]
    fn the_curve_integrates_to_about_one() {
        let c = estimate_group(&[1.0, 2.0, 2.5, 7.0, 9.0], None).unwrap();
        let area: f64 = c
            .grid
            .windows(2)
            .map(|w| (w[1][0] - w[0][0]) * (w[0][1] + w[1][1]) / 2.0)
            .sum();
        assert!((area - 1.0).abs() < 0.01, "{area}");
    }

    #[test]
    fn skewed_data_keeps_a_fine_grid_in_its_bulk() {
        // 200 values in 0..100 and one at 10000: the grid must not stretch over the gap.
        let mut values: Vec<f64> = (0..200).map(|i| i as f64 * 0.5).collect();
        values.push(10_000.0);
        let c = estimate_group(&values, None).unwrap();
        let step = c.h / PER_BANDWIDTH * 1.001;
        let bulk: Vec<f64> = c.grid.iter().map(|p| p[0]).filter(|&x| x <= 100.0).collect();
        assert!(bulk.windows(2).all(|w| w[1] - w[0] <= step), "{bulk:?} vs {step}");
        assert!(c.grid.iter().any(|p| (p[0] - 10_000.0).abs() <= step && p[1] > 0.0));
        assert!(c.grid.len() <= MAX_GRID);
    }

    #[test]
    fn huge_values_stay_finite() {
        let c = estimate_group(&[-1e300, 1e300], None).unwrap();
        assert!(c.grid.iter().all(|p| p[0].is_finite() && p[1].is_finite()));
    }
}
