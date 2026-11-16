//! Contour lines: iso-lines of a value `z` over the plane `x`, `y`, one line at each of a few round
//! levels, coloured along the style's ramp with a key that decodes the colour. Rows on a complete
//! grid are drawn as they are; scattered rows are first interpolated onto a regular grid by
//! inverse-distance weighting, which RichView computes, so every readout of it is derived
//! (FEA-CH-42). The lines come from marching squares and are chained into long paths. A reader
//! points at a place and reads `z` at the nearest grid node.

use super::kit::{self, ValueSide, XAxis};
use super::sample::{disclose_excluded, read_number};
use super::types::Facets;
use super::{Ctx, Painted, Style};
use crate::axis::Zero;
use crate::color::{self, Oklch};
use crate::derived::Positions;
use crate::diag::Level;
use crate::geometry::{
    Anchor, Cap, DropReason, Face, Fill, Item, Observation, Path, Readout, ReadoutLine, Rect, Role, Shape, Stroke,
    Targeting,
};
use crate::limits;
use crate::numbers::{self, Format};
use crate::payload::Quant;
use crate::rows::{self, Num};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const RATIO: f64 = 16.0 / 9.0;
/// Most grid nodes a complete grid may have before it is treated as scattered rows, which the
/// limit of scattered rows then bounds. A cost guard for the grid, not a cap on the data.
const MAX_NODES: usize = 250_000;
/// Nodes of the grid scattered rows are interpolated onto.
const INTERP_X: usize = 48;
const INTERP_Y: usize = 36;
/// Levels aimed at.
const LEVELS: usize = 7;
const LINE_WIDTH: f64 = 1.6;
/// Where on the ramp the lowest and the highest line sit, so the lowest stays visible.
const RAMP_FROM: f64 = 0.4;
/// Most readout cells across and down.
const CELLS_X: usize = 24;
const CELLS_Y: usize = 18;
const KEY_W: f64 = 240.0;
const KEY_H: f64 = 8.0;

/// The surface of one panel: `z` at each node of a grid, row-major with `y` rising.
struct Surface {
    xs: Vec<f64>,
    ys: Vec<f64>,
    z: Vec<f64>,
    /// The source row of each node of a complete grid; empty when interpolated.
    rows: Vec<usize>,
    /// Where the rows were, when interpolated.
    points: Vec<[f64; 2]>,
}

/// A panel's surface as the payload carries it (TYP-36): the nodes and values of a complete grid
/// on their source channels' grids, interpolated ones on the refined statistic grids, so the
/// record holds no digit the author does not show (FEA-CH-37). Native layouts and the runtime
/// both draw the decoded copy.
#[derive(Serialize, Deserialize)]
struct Record {
    xs: Positions,
    ys: Positions,
    z: Positions,
    rows: Vec<usize>,
    points: [Positions; 2],
}

impl Record {
    fn of(s: &Surface, grids: &[Option<Quant>; 3], formats: [&Format; 3]) -> Self {
        let node_grid = |grid: &Option<Quant>| {
            if s.interpolated() {
                crate::derived::statistic_grid(grid.clone())
            } else {
                grid.clone()
            }
        };
        let points =
            |axis: usize| Positions::encode(s.points.iter().map(|p| p[axis]), grids[axis].clone(), formats[axis]);
        Self {
            xs: Positions::encode(s.xs.iter().copied(), node_grid(&grids[0]), formats[0]),
            ys: Positions::encode(s.ys.iter().copied(), node_grid(&grids[1]), formats[1]),
            z: Positions::encode(s.z.iter().copied(), node_grid(&grids[2]), formats[2]),
            rows: s.rows.clone(),
            points: [points(0), points(1)],
        }
    }

    fn surface(&self) -> Surface {
        let (x, y) = (self.points[0].decode(), self.points[1].decode());
        Surface {
            xs: self.xs.decode(),
            ys: self.ys.decode(),
            z: self.z.decode(),
            rows: self.rows.clone(),
            points: x.into_iter().zip(y).map(|(x, y)| [x, y]).collect(),
        }
    }
}

/// Why a panel makes no surface.
#[derive(Debug, Serialize, Deserialize)]
enum Unsurfaced {
    /// The rows cannot make a contour chart.
    Why(String),
    /// There are more scattered rows than the interpolation takes: how many.
    Over(usize),
}

impl Surface {
    fn interpolated(&self) -> bool {
        self.rows.is_empty()
    }
    fn range(&self) -> [f64; 2] {
        self.z.iter().fold([f64::INFINITY, f64::NEG_INFINITY], |[lo, hi], &v| {
            [lo.min(v), hi.max(v)]
        })
    }
}

fn sorted_distinct(points: &[[f64; 3]], axis: usize) -> Vec<f64> {
    let mut v: Vec<f64> = points.iter().map(|p| p[axis]).collect();
    v.sort_by(f64::total_cmp);
    v.dedup();
    v
}

/// The grid the points complete, if they fill every node of `xs` by `ys` exactly once.
fn complete_grid(points: &[[f64; 3]], rows: &[usize], xs: &[f64], ys: &[f64]) -> Option<Surface> {
    let nx = xs.len();
    let mut z = vec![f64::NAN; nx * ys.len()];
    let mut from = vec![0usize; nx * ys.len()];
    for (p, &row) in points.iter().zip(rows) {
        let ix = xs.partition_point(|v| *v < p[0]);
        let iy = ys.partition_point(|v| *v < p[1]);
        let at = iy * nx + ix;
        if ix >= nx || iy >= ys.len() || !z[at].is_nan() {
            return None;
        }
        z[at] = p[2];
        from[at] = row;
    }
    Some(Surface {
        xs: xs.to_vec(),
        ys: ys.to_vec(),
        z,
        rows: from,
        points: Vec::new(),
    })
}

/// `z` at the nodes of a regular grid over the points' extent, by inverse-distance weighting
/// in coordinates scaled to the extent.
fn interpolate(points: &[[f64; 3]], x: [f64; 2], y: [f64; 2], z: [f64; 2]) -> Surface {
    let (xspan, yspan) = (x[1] - x[0], y[1] - y[0]);
    let zspan = if z[1] > z[0] { z[1] - z[0] } else { 1.0 };
    let weighed: Vec<[f64; 3]> = points
        .iter()
        .map(|p| [(p[0] - x[0]) / xspan, (p[1] - y[0]) / yspan, (p[2] - z[0]) / zspan])
        .collect();
    let line = |lo: f64, span: f64, n: usize| -> Vec<f64> {
        (0..n)
            .map(|i| (lo + span * (i as f64 / (n - 1) as f64)).min(lo + span))
            .collect()
    };
    let xs = line(x[0], xspan, INTERP_X);
    let ys = line(y[0], yspan, INTERP_Y);
    let mut values = Vec::with_capacity(xs.len() * ys.len());
    for j in 0..INTERP_Y {
        for i in 0..INTERP_X {
            let (u, v) = (i as f64 / (INTERP_X - 1) as f64, j as f64 / (INTERP_Y - 1) as f64);
            let (mut num, mut den, mut exact) = (0.0, 0.0, None);
            for p in &weighed {
                let d2 = (p[0] - u).powi(2) + (p[1] - v).powi(2);
                if d2 < 1e-24 {
                    exact = Some(p[2]);
                    break;
                }
                let w = 1.0 / d2;
                num += w * p[2];
                den += w;
            }
            let zn = exact.unwrap_or(num / den);
            values.push((z[0] + zn * zspan).clamp(z[0], z[1]));
        }
    }
    Surface {
        xs,
        ys,
        z: values,
        rows: Vec::new(),
        points: points.iter().map(|p| [p[0], p[1]]).collect(),
    }
}

/// The surface the rows of a panel make, or why they make none.
fn surface(points: &[[f64; 3]], rows: &[usize]) -> Result<Surface, Unsurfaced> {
    let (xs, ys) = (sorted_distinct(points, 0), sorted_distinct(points, 1));
    if points.len() < 3 || xs.len() < 2 || ys.len() < 2 {
        return Err(Unsurfaced::Why(
            "Cannot draw this contour chart · It needs at least three rows spread over x and y".into(),
        ));
    }
    let extent = |v: &[f64]| [v[0], v[v.len() - 1]];
    let (x, y) = (extent(&xs), extent(&ys));
    let z = points.iter().fold([f64::INFINITY, f64::NEG_INFINITY], |[lo, hi], p| {
        [lo.min(p[2]), hi.max(p[2])]
    });
    if ![x[1] - x[0], y[1] - y[0], z[1] - z[0]].iter().all(|s| s.is_finite()) {
        return Err(Unsurfaced::Why(
            "Cannot draw this contour chart · The values are too extreme to represent".into(),
        ));
    }
    if xs.len() * ys.len() == points.len()
        && points.len() <= MAX_NODES
        && let Some(grid) = complete_grid(points, rows, &xs, &ys)
    {
        return Ok(grid);
    }
    if points.len() > limits::max("contour", "scattered rows per panel") {
        return Err(Unsurfaced::Over(points.len()));
    }
    Ok(interpolate(points, x, y, z))
}

/// One piece of an iso-line inside a cell: the edges it joins and where it meets them.
struct Seg {
    ids: [u64; 2],
    pts: [[f64; 2]; 2],
}

/// Pairs of cell edges joined by a line, by which corners are at or above the level. Corners
/// count from the lower left anticlockwise; edges are bottom 0, right 1, top 2, left 3. The two
/// saddles (5 and 10) are resolved by the value at the cell's centre.
fn joins(case: usize, centre_above: bool) -> &'static [(usize, usize)] {
    match case {
        1 | 14 => &[(3, 0)],
        2 | 13 => &[(0, 1)],
        3 | 12 => &[(3, 1)],
        4 | 11 => &[(1, 2)],
        6 | 9 => &[(0, 2)],
        7 | 8 => &[(3, 2)],
        5 if centre_above => &[(0, 1), (3, 2)],
        5 => &[(3, 0), (1, 2)],
        10 if centre_above => &[(3, 0), (1, 2)],
        10 => &[(0, 1), (3, 2)],
        _ => &[],
    }
}

/// The iso-lines of `surface` at `level`, as chains of points in data units.
fn iso_lines(s: &Surface, level: f64) -> Vec<Vec<[f64; 2]>> {
    let (nx, ny) = (s.xs.len(), s.ys.len());
    let at = |ix: usize, iy: usize| s.z[iy * nx + ix];
    let mut segs: Vec<Seg> = Vec::new();
    for iy in 0..ny - 1 {
        for ix in 0..nx - 1 {
            // Corners: lower left, lower right, upper right, upper left.
            let corners = [(ix, iy), (ix + 1, iy), (ix + 1, iy + 1), (ix, iy + 1)];
            let above: Vec<bool> = corners.iter().map(|&(a, b)| at(a, b) >= level).collect();
            let case = above
                .iter()
                .enumerate()
                .fold(0usize, |acc, (k, &up)| acc | (usize::from(up) << k));
            let centre = corners.iter().map(|&(a, b)| at(a, b) / 4.0).sum::<f64>();
            for &(e0, e1) in joins(case, centre >= level) {
                let end = |e: usize| -> (u64, [f64; 2]) {
                    // The edge runs between two corners; the line crosses it where the value
                    // reaches the level.
                    let (a, b) = match e {
                        0 => (corners[0], corners[1]),
                        1 => (corners[1], corners[2]),
                        2 => (corners[3], corners[2]),
                        _ => (corners[0], corners[3]),
                    };
                    let (za, zb) = (at(a.0, a.1), at(b.0, b.1));
                    let t = if zb != za {
                        ((level - za) / (zb - za)).clamp(0.0, 1.0)
                    } else {
                        0.5
                    };
                    let (xa, ya, xb, yb) = (s.xs[a.0], s.ys[a.1], s.xs[b.0], s.ys[b.1]);
                    // Horizontal edges are numbered before vertical ones.
                    let id = if a.1 == b.1 {
                        (a.1 * nx + a.0) as u64
                    } else {
                        (nx * ny + a.1 * nx + a.0) as u64
                    };
                    (id, [xa + (xb - xa) * t, ya + (yb - ya) * t])
                };
                let (i0, p0) = end(e0);
                let (i1, p1) = end(e1);
                segs.push(Seg {
                    ids: [i0, i1],
                    pts: [p0, p1],
                });
            }
        }
    }
    chain(&segs)
}

/// Segments that share an edge joined into the fewest chains.
fn chain(segs: &[Seg]) -> Vec<Vec<[f64; 2]>> {
    let mut at: HashMap<u64, Vec<usize>> = HashMap::new();
    for (k, s) in segs.iter().enumerate() {
        for id in s.ids {
            at.entry(id).or_default().push(k);
        }
    }
    let mut used = vec![false; segs.len()];
    let mut out = Vec::new();
    for (first, seed) in segs.iter().enumerate() {
        if used[first] {
            continue;
        }
        used[first] = true;
        let mut line = vec![seed.pts[0], seed.pts[1]];
        let mut ends = [seed.ids[0], seed.ids[1]];
        // Grow the tail until no segment continues it, then turn round and grow the head.
        for turn in 0..2 {
            loop {
                let id = ends[1];
                let next = at.get(&id).and_then(|list| list.iter().copied().find(|&k| !used[k]));
                let Some(k) = next else { break };
                used[k] = true;
                let s = &segs[k];
                let to = if s.ids[0] == id { 1 } else { 0 };
                line.push(s.pts[to]);
                ends[1] = s.ids[to];
            }
            if turn == 0 {
                line.reverse();
                ends.reverse();
            }
        }
        out.push(line);
    }
    out
}

/// The part of the segment `a` to `b` inside `rect` (Liang-Barsky).
fn clip_segment(a: [f64; 2], b: [f64; 2], rect: &Rect) -> Option<([f64; 2], [f64; 2])> {
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    let d = [b[0] - a[0], b[1] - a[1]];
    let bounds = [
        (-d[0], a[0] - rect.x),
        (d[0], rect.right() - a[0]),
        (-d[1], a[1] - rect.y),
        (d[1], rect.bottom() - a[1]),
    ];
    for (p, q) in bounds {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                if t > t1 {
                    return None;
                }
                t0 = t0.max(t);
            } else {
                if t < t0 {
                    return None;
                }
                t1 = t1.min(t);
            }
        }
    }
    Some((
        [a[0] + t0 * d[0], a[1] + t0 * d[1]],
        [a[0] + t1 * d[0], a[1] + t1 * d[1]],
    ))
}

/// The pieces of the polyline `points` inside `rect`.
fn clip_line(points: &[[f64; 2]], rect: &Rect) -> Vec<Vec<[f64; 2]>> {
    let mut out: Vec<Vec<[f64; 2]>> = Vec::new();
    let mut current: Vec<[f64; 2]> = Vec::new();
    for pair in points.windows(2) {
        match clip_segment(pair[0], pair[1], rect) {
            Some((p, q)) => {
                if current.last() != Some(&p) {
                    if current.len() > 1 {
                        out.push(std::mem::take(&mut current));
                    }
                    current.clear();
                    current.push(p);
                }
                current.push(q);
            }
            None => {
                if current.len() > 1 {
                    out.push(std::mem::take(&mut current));
                }
                current.clear();
            }
        }
    }
    if current.len() > 1 {
        out.push(current);
    }
    out
}

/// The nodes a readout stands for along one axis: every `stride`-th, each with the span of
/// coordinates nearer to it than to its neighbours.
fn cells(coords: &[f64], wanted: usize) -> Vec<(usize, [f64; 2])> {
    let stride = coords.len().div_ceil(wanted).max(1);
    let picked: Vec<usize> = (0..coords.len()).step_by(stride).collect();
    picked
        .iter()
        .enumerate()
        .map(|(k, &i)| {
            let lo = if k == 0 {
                coords[0]
            } else {
                (coords[picked[k - 1]] + coords[i]) / 2.0
            };
            let hi = match picked.get(k + 1) {
                Some(&next) => (coords[i] + coords[next]) / 2.0,
                None => coords[coords.len() - 1],
            };
            (i, [lo, hi])
        })
        .collect()
}

/// The colour of the line for a value at `t` (0 to 1) along the levels.
pub(super) fn line_color(ctx: &Ctx, t: f64) -> Oklch {
    let ramp = ctx.style.ramp;
    color::with_contrast(
        color::ramp(&ramp, RAMP_FROM + (1.0 - RAMP_FROM) * t.clamp(0.0, 1.0)),
        ctx.style.surface,
        3.0,
    )
}

/// A key under the figure that decodes a colour as a value between `range[0]` and `range[1]`,
/// named `title`; the figure grows by its height. Each of `levels` is written under its place on
/// the key; the labels that do not fit beside the others are returned, to be reported as dropped.
pub(super) fn add_value_key(
    ctx: &Ctx,
    painted: &mut Painted,
    title: &str,
    range: [f64; 2],
    levels: &[f64],
    format: &Format,
    color_at: &dyn Fn(f64) -> Oklch,
) -> Vec<(String, Rect)> {
    let y = painted.height + 12.0;
    let key_w = KEY_W.min(ctx.opts.width);
    let steps = 32;
    for s in 0..steps {
        let t = (s as f64 + 0.5) / steps as f64;
        painted.figure_items.push(Item::Rect {
            rect: Rect::new(s as f64 * key_w / steps as f64, y, key_w / steps as f64 + 0.3, KEY_H),
            fill: Fill::solid(Style::rgba(color_at(t))),
            stroke: None,
            radius: [0.0; 4],
            role: Role::Key,
            series: None,
            obs: None,
        });
    }
    let baseline = y + KEY_H + 4.0 + ctx.style.font.vertical().0 * ctx.size();
    let (low, high) = (numbers::value(range[0], format), numbers::value(range[1], format));
    let ink = ctx.style.ink;
    let mut texts = vec![(low.clone(), 0.0, Anchor::Start), (high.clone(), key_w, Anchor::End)];
    let room = key_w - ctx.text_width(&low) - ctx.text_width(&high) - 16.0;
    if ctx.text_width(title) <= room {
        texts.push((title.to_owned(), key_w / 2.0, Anchor::Middle));
    }
    for (text, x, anchor) in texts {
        painted.figure_items.push(Item::Text(ctx.text(
            &text,
            x,
            baseline,
            anchor,
            Face::Figures,
            Role::Key,
            ink,
        )));
    }
    let mut dropped = Vec::new();
    let mut below = baseline;
    if !levels.is_empty() {
        below = baseline + ctx.line_height();
        let mut last_right = f64::NEG_INFINITY;
        for &level in levels {
            let t = crate::scale::position(level, range[0], range[1]).clamp(0.0, 1.0);
            let label = numbers::value(level, format);
            let half = ctx.text_width(&label) / 2.0;
            let x = (t * key_w).clamp(half.min(key_w / 2.0), (key_w - half).max(key_w / 2.0));
            let text = ctx.text(&label, x, below, Anchor::Middle, Face::Figures, Role::Key, ink);
            if text.bounds.x < last_right + 6.0 {
                dropped.push((label, text.bounds));
                continue;
            }
            last_right = text.bounds.right();
            painted.figure_items.push(Item::Line {
                path: Path::polyline(&[[t * key_w, y + KEY_H - 2.0], [t * key_w, y + KEY_H + 2.0]]),
                stroke: Stroke::solid(Style::rgba(ink), 1.0),
                role: Role::Key,
                series: None,
                obs: None,
            });
            painted.figure_items.push(Item::Text(text));
        }
    }
    painted.height = below + ctx.style.font.vertical().1 * ctx.size() + 4.0;
    dropped
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let (Some(x_field), Some(y_field), Some(z_field)) =
        (block.encoding.x.clone(), block.encoding.y.clone(), block.channel("z"))
    else {
        let reason = "Cannot draw this contour chart · It needs encoding.x, encoding.y and encoding.z";
        ctx.check(
            "source-error",
            Level::Error,
            "The contour chart has no x, y or z",
            Some("Give encoding.x, encoding.y and encoding.z."),
            Some("chart.encoding".into()),
            None,
            serde_json::Value::Null,
        );
        return kit::failed(ctx, reason, RATIO);
    };
    let x_title = block.x.label.clone().unwrap_or_else(|| x_field.clone());
    let y_title = block.y.label.clone().unwrap_or_else(|| y_field.clone());
    let z_title = block.color.label.clone().unwrap_or_else(|| z_field.clone());
    let x_readout = block.readout_format(&block.x, ctx.locale);
    let y_readout = block.readout_format(&block.y, ctx.locale);
    let z_readout = block.readout_format(&block.color, ctx.locale);
    let y_labels = block.label_format(&block.y, ctx.locale);
    let z_labels = block.label_format(&block.color, ctx.locale);
    let grids = [ctx.grid_for(&x_field), ctx.grid_for(&y_field), ctx.grid_for(&z_field)];
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut excluded = 0usize;
    let mut surfaces: Vec<Option<Surface>> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut points: Vec<[f64; 3]> = Vec::new();
        let mut from: Vec<usize> = Vec::new();
        for &i in members {
            let x = read_number(ctx, "contour chart", &mut failures[fi], i, &x_field);
            let y = read_number(ctx, "contour chart", &mut failures[fi], i, &y_field);
            let z = read_number(ctx, "contour chart", &mut failures[fi], i, &z_field);
            match (x, y, z) {
                (Ok(Some(x)), Ok(Some(y)), Ok(Some(z))) => {
                    points.push([x, y, z]);
                    from.push(i);
                }
                (Ok(_), Ok(_), Ok(_)) => excluded += 1,
                _ => {}
            }
        }
        let made = if failures[fi].is_some() {
            None
        } else {
            // Whether the rows are a complete grid and what the interpolation made jump with the
            // data, and the levels and lines follow from it, so the exact rows decide it (TYP-36).
            let formats = [&x_readout, &y_readout, &z_readout];
            let record = ctx.derive_projected(fi, "contour.surface", || {
                surface(&points, &from).map(|s| Record::of(&s, &grids, formats))
            });
            match record {
                Ok(r) => Some(r.surface()),
                Err(Unsurfaced::Why(reason)) => {
                    failures[fi] = Some(reason);
                    None
                }
                Err(Unsurfaced::Over(count)) => {
                    failures[fi] = Some(kit::over_limit(
                        ctx,
                        "contour",
                        "scattered rows per panel",
                        count,
                        Some("chart.data"),
                    ));
                    None
                }
            }
        };
        surfaces.push(made);
    }
    if failures.iter().all(Option::is_some) {
        let reason = failures.iter().flatten().next().cloned().unwrap_or_default();
        return kit::failed(ctx, &reason, RATIO);
    }
    disclose_excluded(ctx, excluded, "an x, a y or a z");
    let live: Vec<&Surface> = surfaces.iter().flatten().collect();
    let z_range = live
        .iter()
        .map(|s| s.range())
        .fold([f64::INFINITY, f64::NEG_INFINITY], |[lo, hi], r| {
            [lo.min(r[0]), hi.max(r[1])]
        });
    if z_range[1] <= z_range[0] {
        let reason = format!("Nothing to draw · {z_field} has a single value, so there are no contour lines");
        return kit::failed(ctx, &reason, RATIO);
    }
    let mut levels: Vec<f64> = crate::scale::ticks_within(z_range[0], z_range[1], LEVELS)
        .values
        .into_iter()
        .filter(|v| *v > z_range[0] && *v < z_range[1])
        .collect();
    if levels.is_empty() {
        levels.push(z_range[0] / 2.0 + z_range[1] / 2.0);
    }
    let t_of = |v: f64| crate::scale::position(v, z_range[0], z_range[1]).clamp(0.0, 1.0);
    let x_extent = {
        let all = live.iter().flat_map(|s| [s.xs[0], s.xs[s.xs.len() - 1]]);
        all.fold([f64::INFINITY, f64::NEG_INFINITY], |[lo, hi], v| [lo.min(v), hi.max(v)])
    };
    let y_extent = {
        let all = live.iter().flat_map(|s| [s.ys[0], s.ys[s.ys.len() - 1]]);
        all.fold([f64::INFINITY, f64::NEG_INFINITY], |[lo, hi], v| [lo.min(v), hi.max(v)])
    };
    let ratio = block.style.ratio.unwrap_or(RATIO);
    let plan = kit::Plan::xy(ctx, facets.len(), ratio);
    let frames = plan.frames(facets.len(), ctx.opts.width);
    let mut panels = Vec::new();
    let mut any_interpolated = false;
    let surface_color = Style::rgba(ctx.style.ink_faint);
    for (fi, ((name, _), outer)) in facets.iter().zip(&frames).enumerate() {
        let Some(s) = &surfaces[fi] else {
            let reason = failures[fi].clone().unwrap_or_default();
            panels.push(super::failed_facet(ctx, *outer, plan.title_room, name, &reason));
            continue;
        };
        any_interpolated |= s.interpolated();
        ctx.fit.x = Some(x_extent);
        ctx.fit.y = Some(y_extent);
        let side = ValueSide {
            data: y_extent,
            zero: Zero::None,
            title: y_title.clone(),
            format: &y_labels,
            min: Some(y_extent[0]),
            max: Some(y_extent[1]),
        };
        let frame = kit::xy_frame(
            ctx,
            &plan,
            outer,
            &XAxis::Numeric { extent: x_extent },
            Some(&x_title),
            &side,
        );
        let mut marks: Vec<Item> = Vec::new();
        if s.interpolated() {
            for p in &s.points {
                let at = [frame.x(p[0]), frame.y(p[1])];
                if frame.plot.contains(at) {
                    marks.push(Item::Point {
                        center: at,
                        radius: 1.5,
                        shape: Shape::Circle,
                        fill: Some(Fill::solid(surface_color.with_alpha(0.55))),
                        stroke: None,
                        role: Role::Rug,
                        series: None,
                        obs: None,
                    });
                }
            }
        }
        for &level in &levels {
            let rgba = Style::rgba(line_color(ctx, t_of(level)));
            let stroke = Stroke {
                cap: Cap::Round,
                ..Stroke::solid(rgba, LINE_WIDTH)
            };
            for run in iso_lines(s, level) {
                let pixels: Vec<[f64; 2]> = run.iter().map(|p| [frame.x(p[0]), frame.y(p[1])]).collect();
                for piece in clip_line(&pixels, &frame.plot) {
                    marks.push(Item::Line {
                        path: Path::polyline(&piece),
                        stroke: stroke.clone(),
                        role: Role::Mark,
                        series: None,
                        obs: None,
                    });
                }
            }
        }
        let nx = s.xs.len();
        let derived = s.interpolated();
        let mut observations: Vec<Observation> = Vec::new();
        for &(iy, [ya, yb]) in &cells(&s.ys, CELLS_Y) {
            for &(ix, [xa, xb]) in &cells(&s.xs, CELLS_X) {
                let (left, right) = (frame.x(xa), frame.x(xb));
                let (top, bottom) = (frame.y(yb), frame.y(ya));
                let area = Rect::new(
                    left.max(frame.plot.x),
                    top.max(frame.plot.y),
                    right.min(frame.plot.right()) - left.max(frame.plot.x),
                    bottom.min(frame.plot.bottom()) - top.max(frame.plot.y),
                );
                if area.w <= 0.0 || area.h <= 0.0 {
                    continue;
                }
                let mut node = [s.xs[ix], s.ys[iy]];
                let mut value = s.z[iy * nx + ix];
                // A node that is a row reads as the row writes it, not as the derived copy.
                if let Some(&row) = s.rows.get(iy * nx + ix) {
                    let exact = |field: &str, fallback: f64| match rows::num(&ctx.rows[row], field) {
                        Num::Value(v) => v,
                        _ => fallback,
                    };
                    node = [exact(&x_field, node[0]), exact(&y_field, node[1])];
                    value = exact(&z_field, value);
                }
                let mut at = [frame.x(node[0]), frame.y(node[1])];
                at[0] = at[0].clamp(area.x, area.right());
                at[1] = at[1].clamp(area.y, area.bottom());
                observations.push(Observation {
                    row: s.rows.get(iy * nx + ix).copied(),
                    dots: vec![],
                    series: None,
                    at,
                    wedge: None,
                    area: Some(area),
                    radius: 0.0,
                    x: at[0],
                    readout: Readout {
                        heading: format!(
                            "Near {}, {}",
                            numbers::readout_coordinate(node[0], &x_readout),
                            numbers::readout_coordinate(node[1], &y_readout)
                        ),
                        lines: vec![ReadoutLine {
                            name: block
                                .color
                                .label
                                .clone()
                                .unwrap_or_else(|| super::cartesian::readable_name(&z_field)),
                            value: format!(
                                "{}{}",
                                if derived { "≈ " } else { "" },
                                numbers::value(value, &z_readout)
                            ),
                            series: None,
                            names_series: false,
                            derived,
                        }],
                    }
                    .into(),
                });
            }
        }
        let mut panel = kit::xy_panel(ctx, outer, frame, marks, observations, name);
        panel.targeting = Targeting::Contain;
        panels.push(panel);
    }
    if any_interpolated {
        ctx.note.push(format!(
            "scattered rows are interpolated by inverse-distance weighting onto a {INTERP_X} by {INTERP_Y} grid, so {z_title} between rows is computed"
        ));
    }
    ctx.note
        .push("a readout stands for the grid node nearest to the pointer".into());
    ctx.summary = format!(
        "Contour lines of {z_title} over {x_title} and {y_title}: {} from {} to {}.",
        super::count(levels.len(), "level", "levels"),
        numbers::value(levels[0], &z_readout),
        numbers::value(levels[levels.len() - 1], &z_readout),
    );
    let mut painted = kit::finish(&plan, facets.len(), panels, None);
    let colors = |t: f64| line_color(ctx, t);
    let dropped = add_value_key(ctx, &mut painted, &z_title, z_range, &levels, &z_labels, &colors);
    for (text, at) in dropped {
        ctx.drop_label(&text, DropReason::Collision, "key level", Some(at));
    }
    painted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cone(n: usize) -> Surface {
        let xs: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let mut z = Vec::new();
        for j in 0..n {
            for i in 0..n {
                let c = (n as f64 - 1.0) / 2.0;
                z.push(((i as f64 - c).powi(2) + (j as f64 - c).powi(2)).sqrt());
            }
        }
        Surface {
            xs: xs.clone(),
            ys: xs,
            z,
            rows: vec![0; n * n],
            points: Vec::new(),
        }
    }

    #[test]
    fn a_level_through_a_cone_is_one_closed_ring() {
        let lines = iso_lines(&cone(9), 2.0);
        assert_eq!(lines.len(), 1);
        let ring = &lines[0];
        assert!(ring.len() > 8);
        assert_eq!(ring.first(), ring.last());
        for p in ring {
            assert!(((p[0] - 4.0).hypot(p[1] - 4.0) - 2.0).abs() < 0.2);
        }
    }

    #[test]
    fn a_level_outside_the_surface_draws_nothing() {
        assert!(iso_lines(&cone(5), 100.0).is_empty());
    }

    #[test]
    fn a_complete_grid_is_kept_and_a_gap_is_interpolated() {
        let mut rows = Vec::new();
        for y in 0..3 {
            for x in 0..4 {
                rows.push([x as f64, y as f64, (x + y) as f64]);
            }
        }
        let from: Vec<usize> = (0..rows.len()).collect();
        let s = surface(&rows, &from).unwrap();
        assert!(!s.interpolated());
        assert_eq!((s.xs.len(), s.ys.len()), (4, 3));
        rows.pop();
        let s = surface(&rows, &from[..rows.len()]).unwrap();
        assert!(s.interpolated());
        assert_eq!(s.z.len(), INTERP_X * INTERP_Y);
    }

    #[test]
    fn a_line_is_cut_at_the_plot() {
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        let pieces = clip_line(&[[-5.0, 5.0], [5.0, 5.0], [15.0, 5.0]], &rect);
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].first(), Some(&[0.0, 5.0]));
        assert_eq!(pieces[0].last(), Some(&[10.0, 5.0]));
        assert!(clip_line(&[[20.0, 20.0], [30.0, 30.0]], &rect).is_empty());
    }
}
