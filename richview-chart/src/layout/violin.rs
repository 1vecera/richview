//! Violin charts: the distribution of a value for each category as a Gaussian kernel density
//! mirrored about the category's centre, with the median and the quartiles inside. The shape is
//! computed from the rows by RichView and says so (`derived`); it stops at the smallest and the
//! largest value, so it shows no value the data does not hold (FEA-CH-42). Every violin uses one
//! density scale, so equal numbers of observations give equal areas.

use super::kit::{self, ValueSide};
use super::sample::{disclose_excluded, quantile, read_number};
use super::types::Facets;
use super::{Ctx, POINT_OPACITY, Painted, Style};
use crate::axis::Zero;
use crate::derived::{Position, statistic_grid, statistic_grid_with_shown};
use crate::diag::Level;
use crate::geometry::{Fill, Item, Observation, Path, Readout, ReadoutLine, Rect, Role, Shape, Stroke, Swatch};
use crate::numbers::{self, Format, Kind};
use crate::payload::Quant;
use crate::rows;
use serde::{Deserialize, Serialize};

const RATIO: f64 = 16.0 / 9.0;
/// Steps along the value axis at which the density is evaluated.
const STEPS: usize = 48;
/// Widest violin, in pixels.
const WIDEST: f64 = 96.0;
/// Radius of the dot that stands for a sample without a spread.
const DOT: f64 = 5.0;

/// The values of one category and group, ascending.
struct Sample {
    category: usize,
    group: usize,
    row: usize,
    values: Vec<f64>,
}

impl Sample {
    fn spread(&self) -> bool {
        self.values.len() > 1 && self.values[0] < self.values[self.values.len() - 1]
    }
}

/// What of one sample jumps with the data: whether it has a shape, the shape, and the quartiles
/// with the texts the readout shows for them (TYP-36).
#[derive(Serialize, Deserialize)]
struct Form<P = f64> {
    curve: Option<Vec<(P, P)>>,
    /// Smallest and largest value; they set the value axis, so the exact rows decide them.
    extent: [P; 2],
    quartiles: [P; 3],
    texts: [String; 3],
    extent_texts: [String; 2],
}

#[derive(Serialize, Deserialize)]
struct Forms {
    value_grid: Option<Quant>,
    density_grid: Option<Quant>,
    panels: Vec<Vec<Form<Position>>>,
}

impl Form {
    fn encode(self, value: Option<&Quant>, density: Option<&Quant>, format: &Format) -> Form<Position> {
        let density_format = Format {
            kind: Kind::Auto,
            unit: None,
            ..format.clone()
        };
        Form {
            curve: self.curve.map(|curve| {
                curve
                    .into_iter()
                    .map(|(v, d)| {
                        (
                            Position::encode(v, value, format),
                            Position::encode(d, density, &density_format),
                        )
                    })
                    .collect()
            }),
            extent: self.extent.map(|v| Position::encode(v, value, format)),
            quartiles: self.quartiles.map(|v| Position::encode(v, value, format)),
            texts: self.texts,
            extent_texts: self.extent_texts,
        }
    }
}

impl Form<Position> {
    fn decode(self, value: Option<&Quant>, density: Option<&Quant>) -> Form {
        Form {
            curve: self
                .curve
                .map(|curve| curve.into_iter().map(|(v, d)| (v.at(value), d.at(density))).collect()),
            extent: self.extent.map(|v| v.at(value)),
            quartiles: self.quartiles.map(|v| v.at(value)),
            texts: self.texts,
            extent_texts: self.extent_texts,
        }
    }
}

/// The density of an ascending sample at `STEPS + 1` evenly spaced values from its smallest to
/// its largest, per unit of the value, and the values. Silverman's bandwidth; the arithmetic runs
/// on values scaled to at most one in size, so a huge range cannot overflow.
fn density(values: &[f64], grid: Option<&Quant>, format: &Format) -> Vec<(f64, f64)> {
    let (lo, hi) = (values[0], values[values.len() - 1]);
    let scale = lo.abs().max(hi.abs());
    let z: Vec<f64> = values.iter().map(|v| v / scale).collect();
    let n = z.len() as f64;
    let mean = z.iter().sum::<f64>() / n;
    let sd = (z.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt();
    let iqr = (quantile(&z, 0.75) - quantile(&z, 0.25)) / 1.34;
    let spread = if iqr > 0.0 { sd.min(iqr) } else { sd };
    let range = z[z.len() - 1] - z[0];
    let bandwidth = (0.9 * spread * n.powf(-0.2)).max(range / 1000.0);
    (0..=STEPS)
        .map(|k| {
            let t = k as f64 / STEPS as f64;
            let at = lo * (1.0 - t) + hi * t;
            let at = grid.map_or_else(
                || numbers::shown(at, format),
                |grid| grid.at(grid.index_shown(at, format)),
            );
            let zk = at / scale;
            let sum: f64 = z
                .iter()
                .map(|v| {
                    let u = (zk - v) / bandwidth;
                    (-0.5 * u * u).exp()
                })
                .sum();
            let d = sum / (n * bandwidth * (2.0 * std::f64::consts::PI).sqrt()) / scale;
            (at, if d.is_finite() { d } else { 0.0 })
        })
        .collect()
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let (Some(x_field), Some(y_field)) = (block.encoding.x.clone(), block.encoding.y.clone()) else {
        let reason = "Cannot draw this violin chart · It needs encoding.x (the category) and encoding.y (the value)";
        ctx.check(
            "source-error",
            Level::Error,
            "The violin chart has no category or no value",
            Some("Give encoding.x and encoding.y."),
            Some("chart.encoding".into()),
            None,
            serde_json::Value::Null,
        );
        return kit::failed(ctx, reason, RATIO);
    };
    let categories = rows::distinct(ctx.rows, &x_field);
    let multi = block.encoding.color.is_some();
    let title = block.y.label.clone().unwrap_or_else(|| y_field.clone());
    let groups: Vec<String> = match &block.encoding.color {
        Some(f) => rows::distinct(ctx.rows, f),
        None => vec![title.clone()],
    };
    ctx.set_series(&groups);
    let label_format = block.label_format(&block.y, ctx.locale);
    let readout = block.readout_format(&block.y, ctx.locale);
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut excluded = 0usize;
    let mut per_panel: Vec<Vec<Sample>> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut samples: Vec<Sample> = Vec::new();
        for &i in members {
            let row = &ctx.rows[i];
            let Some(category) = rows::text(row, &x_field).and_then(|c| categories.iter().position(|x| *x == c)) else {
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
            let value = match read_number(ctx, "violin chart", &mut failures[fi], i, &y_field) {
                Ok(Some(v)) => v,
                Ok(None) => {
                    excluded += 1;
                    continue;
                }
                Err(()) => continue,
            };
            match samples.iter_mut().find(|s| s.category == category && s.group == group) {
                Some(s) => s.values.push(value),
                None => samples.push(Sample {
                    category,
                    group,
                    row: i,
                    values: vec![value],
                }),
            }
        }
        for s in &mut samples {
            s.values.sort_by(f64::total_cmp);
        }
        if samples.is_empty() && failures[fi].is_none() {
            failures[fi] = Some("Cannot draw this violin chart · It has no values to draw".into());
        }
        per_panel.push(samples);
    }
    if failures.iter().all(Option::is_some) {
        let reason = failures.iter().flatten().next().cloned().unwrap_or_default();
        return kit::failed(ctx, &reason, RATIO);
    }
    disclose_excluded(ctx, excluded, "a category, group or value");
    // The shape (or the dot a sample without spread gets) and the quartiles jump with the data, so
    // the exact rows decide them (TYP-36). Hidden series keep theirs, for when they come back.
    let records: Forms = ctx.derive_projected(0, "violin.forms", || {
        let mut panels: Vec<Vec<Form>> = per_panel
            .iter()
            .map(|samples| {
                samples
                    .iter()
                    .map(|s| {
                        let quartiles = [0.25, 0.5, 0.75].map(|p| quantile(&s.values, p));
                        let extent = [s.values[0], s.values[s.values.len() - 1]];
                        Form {
                            curve: None,
                            extent,
                            quartiles,
                            texts: quartiles.map(|q| numbers::value(q, &readout)),
                            extent_texts: extent.map(|v| numbers::value(v, &readout)),
                        }
                    })
                    .collect()
            })
            .collect();
        let shown = panels
            .iter()
            .flatten()
            .flat_map(|f| f.extent.into_iter().chain(f.quartiles))
            .map(|v| numbers::shown(v, &readout));
        let value_grid = statistic_grid_with_shown(ctx.grid_for(&y_field), shown);
        for (forms, samples) in panels.iter_mut().zip(&per_panel) {
            for (form, sample) in forms.iter_mut().zip(samples) {
                form.curve = sample
                    .spread()
                    .then(|| density(&sample.values, value_grid.as_ref(), &readout));
            }
        }
        let densities: Vec<_> = panels
            .iter()
            .flatten()
            .filter_map(|f| f.curve.as_ref())
            .flatten()
            .map(|p| p.1)
            .collect();
        let density_format = Format {
            kind: Kind::Auto,
            unit: None,
            ..readout.clone()
        };
        let density_grid = statistic_grid(Quant::for_values(
            densities.iter().copied().chain([0.0]),
            densities.iter().map(|d| numbers::shown(*d, &density_format)),
        ));
        Forms {
            panels: panels
                .into_iter()
                .map(|forms| {
                    forms
                        .into_iter()
                        .map(|f| f.encode(value_grid.as_ref(), density_grid.as_ref(), &readout))
                        .collect()
                })
                .collect(),
            value_grid,
            density_grid,
        }
    });
    let forms: Vec<Vec<Form>> = records
        .panels
        .into_iter()
        .map(|forms| {
            forms
                .into_iter()
                .map(|f| f.decode(records.value_grid.as_ref(), records.density_grid.as_ref()))
                .collect()
        })
        .collect();
    // One density scale for every violin, so their areas compare.
    type Curve = Vec<(f64, f64)>;
    let curves: Vec<Vec<Option<&Curve>>> = per_panel
        .iter()
        .zip(&forms)
        .map(|(samples, forms)| {
            samples
                .iter()
                .zip(forms)
                .map(|(s, f)| if ctx.is_hidden(s.group) { None } else { f.curve.as_ref() })
                .collect()
        })
        .collect();
    let peak = forms
        .iter()
        .flatten()
        .filter_map(|f| f.curve.as_ref())
        .flatten()
        .map(|p| p.1)
        .fold(0.0f64, f64::max);
    let visible: Vec<usize> = (0..groups.len()).filter(|g| !ctx.is_hidden(*g)).collect();
    let extent = |fi: usize| {
        forms[fi]
            .iter()
            .fold([f64::INFINITY, f64::NEG_INFINITY], |[lo, hi], f| {
                [lo.min(f.extent[0]), hi.max(f.extent[1])]
            })
    };
    let all: Vec<&Sample> = per_panel
        .iter()
        .zip(&failures)
        .filter(|(_, f)| f.is_none())
        .flat_map(|(p, _)| p)
        .filter(|s| !ctx.is_hidden(s.group))
        .collect();
    let shared = (0..facets.len())
        .filter(|&fi| failures[fi].is_none())
        .map(extent)
        .fold([f64::INFINITY, f64::NEG_INFINITY], |[lo, hi], e| {
            [lo.min(e[0]), hi.max(e[1])]
        });
    let plan = kit::plan(
        ctx,
        facets.len(),
        &categories,
        if multi { groups.len() as f64 * 0.7 } else { 1.0 },
        block.style.ratio.unwrap_or(RATIO),
    );
    let frames = plan.frames(facets.len(), ctx.opts.width);
    let surface = Style::rgba(ctx.style.surface);
    let ink = Style::rgba(ctx.style.ink);
    let mut panels = Vec::new();
    for (fi, ((name, _), outer)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *outer, plan.title_room, name, reason));
            continue;
        }
        let samples = &per_panel[fi];
        let mut data = if block.style.free_scales { extent(fi) } else { shared };
        if !data[0].is_finite() {
            data = ctx.wanted().y.unwrap_or([0.0, 1.0]);
        } else {
            data = super::Fit::widen(data, ctx.wanted().y);
        }
        ctx.fit.y = Some(data);
        let side = ValueSide {
            data,
            zero: Zero::None,
            title: title.clone(),
            format: &label_format,
            min: block.y.min,
            max: block.y.max,
        };
        let frame = kit::frame(ctx, &plan, outer, &side, fi == 0);
        let t = frame.thickness(visible.len(), WIDEST);
        let mut marks: Vec<Item> = Vec::new();
        let mut observations = Vec::new();
        for (si, s) in samples.iter().enumerate() {
            let Some(slot) = visible.iter().position(|g| *g == s.group) else {
                continue;
            };
            let cm = frame.slot_start(s.category, slot, visible.len(), t) + t / 2.0;
            let color = Style::rgba(ctx.series_color(s.group));
            let hatch = ctx.series[s.group].hatch;
            let n = s.values.len();
            let [q1, median, q3] = forms[fi][si].quartiles;
            let index = observations.len();
            let heading = if multi {
                format!("{} · {}", categories[s.category], groups[s.group])
            } else {
                categories[s.category].clone()
            };
            let count = ReadoutLine {
                name: "Observations".into(),
                value: numbers::grouped(n as f64, 0, ctx.locale, true),
                series: Some(s.group),
                names_series: false,
                derived: true,
            };
            let [q1_text, median_text, q3_text] = forms[fi][si].texts.clone();
            let [lo_text, hi_text] = forms[fi][si].extent_texts.clone();
            let text_line = |name: &str, value: String, derived: bool| ReadoutLine {
                name: name.into(),
                value,
                series: Some(s.group),
                names_series: false,
                derived,
            };
            let observation = match &curves[fi][si] {
                Some(curve) if peak > 0.0 => {
                    let half = t / 2.0 * 0.96;
                    let side_of = |sign: f64| -> Vec<[f64; 2]> {
                        curve
                            .iter()
                            .map(|(v, d)| frame.pt(cm + sign * d / peak * half, *v))
                            .collect()
                    };
                    let mut ring = side_of(1.0);
                    ring.extend(side_of(-1.0).into_iter().rev());
                    let mut path = Path::polyline(&ring);
                    path.close();
                    let area = path.bounds().unwrap_or(Rect::new(cm, frame.at(median), 0.0, 0.0));
                    marks.push(Item::Area {
                        path: path.clone(),
                        fill: Fill {
                            color: color.with_alpha(POINT_OPACITY),
                            hatch,
                            hatch_ink: hatch.map(|_| surface),
                            hatch_scale: 1.0,
                        },
                        role: Role::Mark,
                        series: Some(s.group),
                        obs: Some(index),
                    });
                    marks.push(Item::Line {
                        path,
                        stroke: Stroke::solid(color, 1.5),
                        role: Role::Mark,
                        series: Some(s.group),
                        obs: None,
                    });
                    marks.push(Item::Line {
                        path: Path::polyline(&[frame.pt(cm, q1), frame.pt(cm, q3)]),
                        stroke: Stroke::solid(ink, 3.0),
                        role: Role::Mark,
                        series: Some(s.group),
                        obs: None,
                    });
                    marks.push(Item::Point {
                        center: frame.pt(cm, median),
                        radius: 3.0,
                        shape: Shape::Circle,
                        fill: Some(Fill::solid(surface)),
                        stroke: Some(Stroke::solid(ink, 1.0)),
                        role: Role::Mark,
                        series: Some(s.group),
                        obs: None,
                    });
                    Observation {
                        row: Some(s.row),
                        dots: vec![],
                        series: Some(s.group),
                        at: frame.pt(cm, median),
                        wedge: None,
                        area: Some(area),
                        radius: 0.0,
                        x: cm,
                        readout: Readout {
                            heading,
                            lines: vec![
                                count,
                                text_line("Maximum", hi_text, false),
                                text_line("Upper quartile", q3_text, true),
                                text_line("Median", median_text, true),
                                text_line("Lower quartile", q1_text, true),
                                text_line("Minimum", lo_text, false),
                            ],
                        }
                        .into(),
                    }
                }
                _ => {
                    // One value, or all equal: a dot on the value, with no shape to invent.
                    let center = frame.pt(cm, median);
                    marks.push(Item::Point {
                        center,
                        radius: DOT,
                        shape: ctx.series[s.group].shape.unwrap_or(Shape::Circle),
                        fill: Some(Fill::solid(color)),
                        stroke: None,
                        role: Role::Mark,
                        series: Some(s.group),
                        obs: Some(index),
                    });
                    Observation {
                        row: Some(s.row),
                        dots: vec![],
                        series: Some(s.group),
                        at: center,
                        wedge: None,
                        area: None,
                        radius: DOT,
                        x: cm,
                        readout: Readout {
                            heading,
                            lines: vec![count, text_line("Value", lo_text, false)],
                        }
                        .into(),
                    }
                }
            };
            observations.push(observation);
        }
        panels.push(kit::panel(ctx, outer, frame, marks, observations, name));
    }
    let total: usize = all.iter().map(|s| s.values.len()).sum();
    ctx.summary = format!(
        "{} {} of {} across {} categories.",
        all.len(),
        if all.len() == 1 {
            "distribution"
        } else {
            "distributions"
        },
        super::count(total, "observation", "observations"),
        categories.len()
    );
    ctx.note.push(
        "shape: Gaussian kernel density with Silverman's bandwidth, from the smallest to the largest value".into(),
    );
    let legend = kit::legend(ctx, Swatch::Square);
    kit::finish(&plan, facets.len(), panels, legend)
}
