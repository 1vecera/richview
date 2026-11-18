//! Strip charts: every observation is a point on one value axis, one strip per category and
//! groups side by side within a category. Points that would overlap are spread sideways by a
//! deterministic beeswarm: values in order, each point at the smallest sideways offset that
//! clears the earlier points by one diameter, alternating sides on a tie. The same data always
//! gives the same picture (FEA-CH-42).

use super::kit::{self, ValueSide};
use super::types::Facets;
use super::{Ctx, POINT_OPACITY, POINT_RADIUS, Painted, Style};
use crate::axis::Zero;
use crate::derived::Position;
use crate::diag::Level;
use crate::geometry::{Fill, Item, Observation, Readout, ReadoutLine, Role, Shape, Stroke, Swatch, Targeting};
use crate::numbers::{self, Format};
use crate::payload::Quant;
use crate::rows::{self, Num};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;

const RATIO: f64 = 16.0 / 9.0;
/// Smallest point radius before points are allowed to overlap.
const MIN_RADIUS: f64 = 1.5;
/// Earlier points a new point is checked against; beyond them the strip is solid anyway.
const WINDOW: usize = 64;
/// Room beyond the data, as a share of its range.
const PAD: f64 = 0.03;

/// One observation.
struct Pt {
    row: usize,
    category: usize,
    group: usize,
    value: f64,
}

/// Sideways offsets, in pixels, for points whose positions along the value axis are `pos`
/// (monotonic), so that no two are closer than `d`. Each point takes the smallest offset that
/// clears the earlier ones; when the offsets either side are equally small the sides alternate.
fn swarm(pos: &[f64], d: f64) -> Vec<f64> {
    let mut off = vec![0.0f64; pos.len()];
    let mut flip = false;
    for k in 0..pos.len() {
        let near: Vec<usize> = (k.saturating_sub(WINDOW)..k)
            .rev()
            .take_while(|&j| (pos[k] - pos[j]).abs() < d)
            .collect();
        let clear = |c: f64| near.iter().all(|&j| (pos[k] - pos[j]).hypot(c - off[j]) >= d - 1e-9);
        if clear(0.0) {
            continue;
        }
        let mut candidates: Vec<f64> = Vec::with_capacity(near.len() * 2);
        for &j in &near {
            let dy = pos[k] - pos[j];
            let dx = (d * d - dy * dy).max(0.0).sqrt();
            candidates.push(off[j] + dx);
            candidates.push(off[j] - dx);
        }
        let valid: Vec<f64> = candidates.iter().copied().filter(|&c| clear(c)).collect();
        let chosen = if valid.is_empty() {
            candidates.iter().copied().fold(f64::NEG_INFINITY, f64::max)
        } else {
            let least = valid.iter().fold(f64::INFINITY, |m, c| m.min(c.abs()));
            let ties: Vec<f64> = valid.iter().copied().filter(|c| c.abs() - least <= 1e-9).collect();
            ties.iter().copied().find(|c| (*c > 0.0) == flip).unwrap_or(ties[0])
        };
        off[k] = chosen;
        flip = !flip;
    }
    off
}

/// Each strip's points in value order as the payload carries them (TYP-36): the rows, with their
/// values on the value channel's grid, so the record holds no digit the author does not show
/// (FEA-CH-37).
#[derive(Serialize, Deserialize)]
struct Cells {
    grid: Option<Quant>,
    strips: Vec<Vec<(usize, Position)>>,
}

impl Cells {
    fn of(strips: &[Vec<(usize, f64)>], grid: Option<Quant>, format: &Format) -> Self {
        let strips = strips
            .iter()
            .map(|strip| {
                strip
                    .iter()
                    .map(|&(row, value)| (row, Position::encode(value, grid.as_ref(), format)))
                    .collect()
            })
            .collect();
        Self { grid, strips }
    }

    fn strips(&self) -> Vec<Vec<(usize, f64)>> {
        self.strips
            .iter()
            .map(|strip| strip.iter().map(|&(row, p)| (row, p.at(self.grid.as_ref()))).collect())
            .collect()
    }
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let y_field = block.encoding.y.clone().unwrap_or_default();
    let title = block.y.label.clone().unwrap_or_else(|| y_field.clone());
    let x_field = block.encoding.x.clone();
    let categories: Vec<String> = match &x_field {
        Some(f) => rows::distinct(ctx.rows, f),
        None => vec![title.clone()],
    };
    let multi = block.encoding.color.is_some();
    let groups: Vec<String> = match &block.encoding.color {
        Some(f) => rows::distinct(ctx.rows, f),
        None => vec![title.clone()],
    };
    ctx.set_series(&groups);
    let label_format = block.label_format(&block.y, ctx.locale);
    let readout = block.readout_format(&block.y, ctx.locale);
    let value_grid = ctx.grid_for(&y_field);
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut excluded = 0usize;
    let mut data: Vec<Vec<Pt>> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut pts: Vec<Pt> = Vec::new();
        for &i in members {
            let row = &ctx.rows[i];
            let category = match &x_field {
                Some(f) => match rows::text(row, f).and_then(|c| categories.iter().position(|x| *x == c)) {
                    Some(c) => c,
                    None => {
                        excluded += 1;
                        continue;
                    }
                },
                None => 0,
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
            match rows::num(row, &y_field) {
                Num::Value(value) => pts.push(Pt {
                    row: i,
                    category,
                    group,
                    value,
                }),
                Num::Missing => excluded += 1,
                Num::Underflow(t) => {
                    ctx.underflow(i, &y_field, &t);
                    failures[fi].get_or_insert_with(|| {
                        format!("Cannot draw this strip chart · {y_field} has a value too small to represent")
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
                        format!("Cannot draw this strip chart · {y_field} has text that is not a number")
                    });
                }
            }
        }
        if failures[fi].is_none() && pts.is_empty() {
            failures[fi] = Some("Cannot draw this strip chart · No row has a value".into());
        }
        data.push(pts);
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
                "{} without a category, group or value {} left out",
                super::count(excluded, "row", "rows"),
                super::verb(excluded, "is", "are")
            ),
            Some("Give every row a value."),
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
    let visible: Vec<usize> = (0..groups.len()).filter(|g| !ctx.is_hidden(*g)).collect();
    // The value axis follows the smallest and largest value of each group, which the exact rows
    // decide (TYP-36); a hidden group's values leave the axis.
    let group_extents: Vec<Vec<Option<[f64; 2]>>> = (0..facets.len())
        .map(|fi| {
            if failures[fi].is_some() {
                return Vec::new();
            }
            ctx.derive(fi, "strip.extents", || {
                (0..groups.len())
                    .map(|g| {
                        data[fi]
                            .iter()
                            .filter(|p| p.group == g)
                            .fold(None, |e: Option<[f64; 2]>, p| {
                                Some(e.map_or([p.value, p.value], |[lo, hi]| [lo.min(p.value), hi.max(p.value)]))
                            })
                            .map(crate::derived::coarse)
                    })
                    .collect()
            })
        })
        .collect();
    let extent = |fis: &mut dyn Iterator<Item = usize>| {
        fis.flat_map(|fi| group_extents[fi].iter().enumerate())
            .filter(|(g, _)| visible.contains(g))
            .filter_map(|(_, e)| *e)
            .fold([f64::INFINITY, f64::NEG_INFINITY], |[lo, hi], e| {
                [lo.min(e[0]), hi.max(e[1])]
            })
    };
    let shared = extent(&mut (0..facets.len()).filter(|&fi| failures[fi].is_none()));
    let plan = kit::plan(
        ctx,
        facets.len(),
        &categories,
        if multi { groups.len() as f64 * 0.7 } else { 1.0 },
        block.style.ratio.unwrap_or(RATIO),
    );
    let frames = plan.frames(facets.len(), ctx.opts.width);
    let mut panels = Vec::new();
    let mut overlapped = 0usize;
    for (fi, ((name, _), outer)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *outer, plan.title_room, name, reason));
            continue;
        }
        let mut data_extent = if block.style.free_scales {
            extent(&mut std::iter::once(fi))
        } else {
            shared
        };
        let zero = if data_extent[0].is_finite() {
            let [lo, hi] = data_extent;
            let span = hi - lo;
            // Values that are all non-negative and start close to zero keep zero on the axis.
            let from_zero = lo >= 0.0 && lo <= 0.2 * hi;
            let pad = if span > 0.0 {
                span * PAD
            } else if lo == 0.0 {
                1.0
            } else {
                lo.abs() * 0.1
            };
            data_extent = [if from_zero { lo } else { lo - pad }, hi + pad];
            data_extent = super::Fit::widen(data_extent, ctx.wanted().y);
            if from_zero { Zero::Include } else { Zero::None }
        } else {
            data_extent = ctx.wanted().y.unwrap_or([0.0, 1.0]);
            Zero::None
        };
        if !data_extent[0].is_finite() || !data_extent[1].is_finite() || !(data_extent[1] - data_extent[0]).is_finite()
        {
            panels.push(super::failed_facet(
                ctx,
                *outer,
                plan.title_room,
                name,
                "Cannot draw this strip chart · The values are too extreme to represent",
            ));
            continue;
        }
        ctx.fit.y = Some(data_extent);
        let side = ValueSide {
            data: data_extent,
            zero,
            title: title.clone(),
            format: &label_format,
            min: block.y.min,
            max: block.y.max,
        };
        let frame = kit::frame(ctx, &plan, outer, &side, fi == 0);
        let slots = visible.len().max(1);
        let t = frame.thickness(slots, f64::INFINITY);
        // The points of each strip (category and group), in value order. The order, the ties and
        // the values the swarm spreads jump with the data, so the exact rows decide them (TYP-36);
        // hidden series keep theirs, for when they come back.
        let record: Cells = ctx.derive_projected(fi, "strip.cells", || {
            let mut strips: Vec<Vec<(usize, f64)>> = vec![Vec::new(); categories.len() * groups.len()];
            for p in &data[fi] {
                strips[p.category * groups.len() + p.group].push((p.row, p.value));
            }
            for strip in &mut strips {
                strip.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
            }
            Cells::of(&strips, value_grid.clone(), &readout)
        });
        let strips = record.strips();
        let cells: Vec<Vec<(usize, f64)>> = strips
            .into_iter()
            .enumerate()
            .map(|(idx, strip)| {
                if visible.contains(&(idx % groups.len())) {
                    strip
                } else {
                    Vec::new()
                }
            })
            .collect();
        let own: HashMap<usize, &Pt> = data[fi].iter().map(|p| (p.row, p)).collect();
        let along = if frame.horizontal { frame.plot.w } else { frame.plot.h };
        let fullest = cells.iter().map(Vec::len).max().unwrap_or(1).max(1) as f64;
        let mut radius = POINT_RADIUS
            .min(0.5 * (0.6 * t * along / fullest).sqrt())
            .max(MIN_RADIUS);
        // The largest radius whose swarms fit their strip; past the smallest, they are squeezed.
        let (offsets, squeezed) = loop {
            let offsets: Vec<Vec<f64>> = cells
                .iter()
                .map(|c| {
                    let pos: Vec<f64> = c.iter().map(|p| frame.at(p.1)).collect();
                    swarm(&pos, 2.0 * radius)
                })
                .collect();
            let worst = offsets.iter().flatten().fold(0.0f64, |m, o| m.max(o.abs()));
            let room = (t / 2.0 - radius).max(0.0);
            if worst <= room + 1e-9 {
                break (offsets, false);
            }
            if radius <= MIN_RADIUS {
                let scale = if worst > 0.0 { room / worst } else { 1.0 };
                break (
                    offsets
                        .into_iter()
                        .map(|c| c.into_iter().map(|o| o * scale).collect())
                        .collect(),
                    true,
                );
            }
            radius = (radius - 0.5).max(MIN_RADIUS);
        };
        if squeezed {
            overlapped += 1;
        }
        let mut marks: Vec<Item> = Vec::new();
        let mut observations = Vec::new();
        for (idx, cell) in cells.iter().enumerate() {
            let (category, group) = (idx / groups.len(), idx % groups.len());
            let Some(slot) = visible.iter().position(|g| *g == group) else {
                continue;
            };
            let centre = frame.slot_start(category, slot, slots, t) + t / 2.0;
            let color = Style::rgba(ctx.series_color(group));
            let shape = ctx.series[group].shape;
            for (&(row, spread), off) in cell.iter().zip(&offsets[idx]) {
                let Some(p) = own.get(&row) else { continue };
                let at = frame.pt(centre + off, spread);
                let obs = observations.len();
                marks.push(match shape {
                    Some(shape) => Item::Point {
                        center: at,
                        radius,
                        shape,
                        fill: None,
                        stroke: Some(Stroke::solid(color, 1.5)),
                        role: Role::Mark,
                        series: Some(group),
                        obs: Some(obs),
                    },
                    None => Item::Point {
                        center: at,
                        radius,
                        shape: Shape::Circle,
                        fill: Some(Fill::solid(color.with_alpha(POINT_OPACITY))),
                        stroke: None,
                        role: Role::Mark,
                        series: Some(group),
                        obs: Some(obs),
                    },
                });
                let mut lines = vec![ReadoutLine {
                    label: None,
                    name: title.clone(),
                    value: numbers::value(p.value, &readout),
                    series: Some(group),
                    names_series: false,
                    derived: false,
                }];
                if multi && x_field.is_some() && categories[category] != groups[group] {
                    lines.push(ReadoutLine {
                        label: None,
                        name: block
                            .encoding
                            .color
                            .as_deref()
                            .map(super::cartesian::readable_name)
                            .unwrap_or_default(),
                        value: groups[group].clone(),
                        series: Some(group),
                        names_series: true,
                        derived: false,
                    });
                }
                observations.push(Observation {
                    row: Some(p.row),
                    dots: Vec::new(),
                    series: Some(group),
                    at,
                    wedge: None,
                    area: None,
                    radius: radius.max(POINT_RADIUS),
                    x: at[0],
                    readout: Readout {
                        heading: if x_field.is_some() {
                            categories[category].clone()
                        } else if block.encoding.color.is_some() {
                            groups[group].clone()
                        } else {
                            format!("Row {}", p.row + 1)
                        },
                        lines,
                    }
                    .into(),
                });
            }
        }
        let mut panel = kit::panel(ctx, outer, frame, marks, observations, name);
        panel.targeting = Targeting::Radius;
        panels.push(panel);
    }
    if overlapped > 0 {
        ctx.check(
            "points-overlap",
            Level::Info,
            format!("Points overlap in {overlapped} panels because their strips are too narrow to spread them"),
            Some("Widen the chart, filter the data, or facet it."),
            None,
            None,
            json!({"panels": overlapped}),
        );
    }
    let shown: usize = data
        .iter()
        .zip(&failures)
        .filter(|(_, f)| f.is_none())
        .map(|(d, _)| d.len())
        .sum();
    ctx.note.push(
        "points that would overlap are spread sideways in value order; the sideways position carries no value".into(),
    );
    ctx.summary = format!(
        "{shown} {}{}.",
        if shown == 1 { "observation" } else { "observations" },
        if x_field.is_some() {
            format!(" in {} categories", categories.len())
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

    fn clear(pos: &[f64], off: &[f64], d: f64) -> bool {
        (0..pos.len()).all(|a| (0..a).all(|b| (pos[a] - pos[b]).hypot(off[a] - off[b]) >= d - 1e-6))
    }

    #[test]
    fn separated_points_stay_on_the_centre_line() {
        assert_eq!(swarm(&[0.0, 20.0, 40.0], 8.0), vec![0.0, 0.0, 0.0]);
    }

    #[test]
    fn coincident_points_alternate_sides_by_one_diameter() {
        let off = swarm(&[10.0, 10.0, 10.0, 10.0, 10.0], 8.0);
        assert_eq!(off[0], 0.0);
        assert!((off[1].abs() - 8.0).abs() < 1e-9 && (off[2].abs() - 8.0).abs() < 1e-9);
        assert!(off[1] * off[2] < 0.0, "the sides alternate: {off:?}");
        assert!(clear(&[10.0; 5], &off, 8.0));
    }

    #[test]
    fn the_same_data_gives_the_same_swarm() {
        let pos: Vec<f64> = (0..200).map(|i| ((i * 37) % 101) as f64 / 3.0).collect();
        let mut sorted = pos.clone();
        sorted.sort_by(f64::total_cmp);
        let a = swarm(&sorted, 6.0);
        assert_eq!(a, swarm(&sorted, 6.0));
        assert!(clear(&sorted, &a, 6.0));
    }
}
