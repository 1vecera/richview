//! Surfaces: `z` over an `x`-`y` plane as a mesh of quadrilaterals seen from `style.azimuth` and
//! `style.elevation` (FEA-CH-75). Rows that fill a complete grid are drawn as given; any other rows are
//! interpolated onto a 24 by 24 grid by inverse-distance weighting, and the readouts say so. The
//! mesh is projected orthographically into plain 2-D items (see `project3d`), far quads first,
//! each quad coloured by the mean z of its corners on the style's ramp with a key under the chart.
//! Turning the view with the pointer is host work that lays the chart out again with new angles.

use super::kit;
use super::project3d::{self, Axis3, Pt, View};
use super::sample::{disclose_excluded, read_number};
use super::types::Facets;
use super::{Ctx, MIN_PLOT, Painted, Style};
use crate::axis::Zero;
use crate::derived::Positions;
use crate::diag::Level;
use crate::geometry::{
    Anchor, Face, Fill, Item, Observation, Panel, Path, Readout, ReadoutLine, Rect, Role, Stroke, Targeting,
};
use crate::limits;
use crate::numbers::{self, Format};
use crate::payload::Quant;
use serde::{Deserialize, Serialize};

const RATIO: f64 = 4.0 / 3.0;
/// Nodes of the interpolated grid along each side.
const IDW_NODES: usize = 24;
const KEY_W: f64 = 240.0;
const KEY_H: f64 = 8.0;

/// The mesh: nodes along x and y with the z of each node at `z[ix][iy]`.
struct Mesh {
    xs: Vec<f64>,
    ys: Vec<f64>,
    z: Vec<Vec<f64>>,
    /// The z values were computed from scattered rows.
    interpolated: bool,
}

/// The mesh as the payload carries it (TYP-36): a complete grid's nodes and values on their
/// source channels' grids, interpolated ones on the refined statistic grids, so the record holds
/// no digit the author does not show (FEA-CH-37). Native layouts and the runtime both draw the
/// decoded copy.
#[derive(Serialize, Deserialize)]
struct Record {
    xs: Positions,
    ys: Positions,
    /// The z of each node, x-major: `ys` values for each x.
    z: Positions,
    interpolated: bool,
}

impl Record {
    fn of(m: &Mesh, grids: &[Option<Quant>; 3], formats: [&Format; 3]) -> Self {
        let grid = |axis: usize| {
            if m.interpolated {
                crate::derived::statistic_grid(grids[axis].clone())
            } else {
                grids[axis].clone()
            }
        };
        Self {
            xs: Positions::encode(m.xs.iter().copied(), grid(0), formats[0]),
            ys: Positions::encode(m.ys.iter().copied(), grid(1), formats[1]),
            z: Positions::encode(m.z.iter().flatten().copied(), grid(2), formats[2]),
            interpolated: m.interpolated,
        }
    }

    fn mesh(&self) -> Mesh {
        let ys = self.ys.decode();
        let z = self.z.decode();
        Mesh {
            xs: self.xs.decode(),
            z: z.chunks(ys.len().max(1)).map(<[f64]>::to_vec).collect(),
            ys,
            interpolated: self.interpolated,
        }
    }
}

/// Why no mesh could be made.
#[derive(Debug, Serialize, Deserialize)]
enum Unmeshable {
    /// The rows cannot make a surface.
    Why(String),
    /// The rows are more than a limit of the surface takes: what is counted and how many there are.
    Over(String, usize),
}

/// A quadrilateral of the mesh on the page.
struct Quad {
    /// Its place among the quads of the mesh, x-major.
    id: usize,
    pts: [Pt; 4],
    depth: f64,
    /// Mean z of the corners.
    z: f64,
}

fn fail(ctx: &mut Ctx, reason: &str, fix: &str) -> Painted {
    ctx.check(
        "source-error",
        Level::Error,
        reason,
        Some(fix),
        Some("chart.encoding".into()),
        None,
        serde_json::Value::Null,
    );
    kit::failed(ctx, reason, RATIO)
}

/// The smallest and the largest of `values`.
fn extent(values: impl Iterator<Item = f64>) -> [f64; 2] {
    values.fold([f64::INFINITY, f64::NEG_INFINITY], |[lo, hi], v| [lo.min(v), hi.max(v)])
}

/// Index of `v` among the sorted `values`.
fn index(values: &[f64], v: f64) -> usize {
    values.partition_point(|a| *a < v)
}

/// `v` rounded to a short decimal, three digits finer than the leading digit of `span`: a
/// computed node never says more than the data's range does (FEA-CH-37).
fn snap(v: f64, span: f64) -> f64 {
    if !(span.is_finite() && span > 0.0 && v.is_finite()) {
        return v;
    }
    let e = span.log10().floor() as i32 - 3;
    let scale = 10f64.powi(-e);
    let k = (v * scale).round();
    if !k.is_finite() {
        return v;
    }
    if e < 0 { k / scale } else { k * 10f64.powi(e) }
}

/// The z at `(x, y)` from the rows by inverse-distance weighting with power 2, in coordinates
/// scaled to the unit square so the two axes weigh alike. A row exactly there is the answer.
fn idw(rows: &[[f64; 3]], x: f64, y: f64) -> f64 {
    let (mut num, mut den) = (0.0, 0.0);
    for p in rows {
        let d2 = (p[0] - x).powi(2) + (p[1] - y).powi(2);
        if d2 < 1e-18 {
            return p[2];
        }
        let w = 1.0 / d2;
        num += w * p[2];
        den += w;
    }
    num / den
}

/// The mesh of the rows: the grid they fill, or a grid interpolated from them.
fn mesh_of(pts: &[[f64; 3]]) -> Result<Mesh, Unmeshable> {
    if pts.len() < 3 {
        return Err(Unmeshable::Why(
            "It needs at least three rows with an x, a y and a z".into(),
        ));
    }
    let mut xs: Vec<f64> = pts.iter().map(|p| p[0]).collect();
    xs.sort_by(f64::total_cmp);
    xs.dedup();
    let mut ys: Vec<f64> = pts.iter().map(|p| p[1]).collect();
    ys.sort_by(f64::total_cmp);
    ys.dedup();
    if xs.len() < 2 || ys.len() < 2 {
        return Err(Unmeshable::Why(
            "It needs at least two different x values and two different y values".into(),
        ));
    }
    let (nx, ny) = (xs.len(), ys.len());
    let mut grid = pts.len() == nx * ny;
    let mut cell: Vec<Option<f64>> = Vec::new();
    if grid {
        cell = vec![None; nx * ny];
        for p in pts {
            let slot = &mut cell[index(&xs, p[0]) * ny + index(&ys, p[1])];
            if slot.is_some() {
                grid = false;
                break;
            }
            *slot = Some(p[2]);
        }
    }
    if grid {
        // Every node is drawn: a grid wider than the limit is not drawn at all.
        let wide = nx.max(ny);
        if wide > limits::max("surface", "grid nodes per axis") {
            return Err(Unmeshable::Over("grid nodes per axis".into(), wide));
        }
        return Ok(Mesh {
            xs,
            ys,
            z: (0..nx)
                .map(|i| (0..ny).map(|j| cell[i * ny + j].unwrap_or(0.0)).collect())
                .collect(),
            interpolated: false,
        });
    }
    if pts.len() > limits::max("surface", "scattered rows") {
        return Err(Unmeshable::Over("scattered rows".into(), pts.len()));
    }
    let (x0, y0) = (xs[0], ys[0]);
    let (sx, sy) = (xs[nx - 1] - x0, ys[ny - 1] - y0);
    if !(sx.is_finite() && sy.is_finite()) {
        return Err(Unmeshable::Why(
            "The x or y values span more than can be represented".into(),
        ));
    }
    let unit: Vec<[f64; 3]> = pts.iter().map(|p| [(p[0] - x0) / sx, (p[1] - y0) / sy, p[2]]).collect();
    // The nodes lie between the data's ends, which stay as given; the ones between are computed,
    // so they are rounded to a short decimal.
    let at = |lo: f64, span: f64, k: usize| match k {
        0 => lo,
        k if k == IDW_NODES - 1 => lo + span,
        k => snap(lo + span * k as f64 / (IDW_NODES - 1) as f64, span),
    };
    let gx: Vec<f64> = (0..IDW_NODES).map(|k| at(x0, sx, k)).collect();
    let gy: Vec<f64> = (0..IDW_NODES).map(|k| at(y0, sy, k)).collect();
    let (z_lo, z_hi) = pts.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
        (lo.min(p[2]), hi.max(p[2]))
    });
    let z: Vec<Vec<f64>> = (0..IDW_NODES)
        .map(|i| {
            (0..IDW_NODES)
                .map(|j| {
                    let v = idw(
                        &unit,
                        i as f64 / (IDW_NODES - 1) as f64,
                        j as f64 / (IDW_NODES - 1) as f64,
                    );
                    snap(v, z_hi - z_lo).clamp(z_lo, z_hi)
                })
                .collect()
        })
        .collect();
    Ok(Mesh {
        xs: gx,
        ys: gy,
        z,
        interpolated: true,
    })
}

/// What each quad of `mesh` reads: its x range, its y range and its mean z, in the quads' order
/// (x-major), written from the values the mesh was made of.
fn quad_texts(mesh: &Mesh, x: &Format, y: &Format, z: &Format) -> Vec<[String; 3]> {
    let prefix = if mesh.interpolated { "≈ " } else { "" };
    let range = |a: f64, b: f64, f: &Format| format!("{prefix}{}–{}", numbers::value(a, f), numbers::value(b, f));
    let mut out = Vec::new();
    for (ix, xw) in mesh.xs.windows(2).enumerate() {
        for (iy, yw) in mesh.ys.windows(2).enumerate() {
            let mean = (mesh.z[ix][iy] + mesh.z[ix + 1][iy] + mesh.z[ix + 1][iy + 1] + mesh.z[ix][iy + 1]) / 4.0;
            out.push([
                range(xw[0], xw[1], x),
                range(yw[0], yw[1], y),
                format!("{prefix}{}", numbers::value(mean, z)),
            ]);
        }
    }
    out
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let (Some(xf), Some(yf), Some(zf)) = (block.encoding.x.clone(), block.encoding.y.clone(), block.channel("z"))
    else {
        return fail(
            ctx,
            "Cannot draw this surface · It needs encoding.x, encoding.y and encoding.z",
            "Give encoding.x, encoding.y and encoding.z.",
        );
    };
    let mut failure: Option<String> = None;
    let mut excluded = 0usize;
    let mut pts: Vec<[f64; 3]> = Vec::new();
    let members: Vec<usize> = facets.iter().flat_map(|(_, m)| m.iter().copied()).collect();
    for i in members {
        let x = read_number(ctx, "surface", &mut failure, i, &xf);
        let y = read_number(ctx, "surface", &mut failure, i, &yf);
        let z = read_number(ctx, "surface", &mut failure, i, &zf);
        let (Ok(x), Ok(y), Ok(z)) = (x, y, z) else {
            continue;
        };
        match (x, y, z) {
            (Some(x), Some(y), Some(z)) => pts.push([x, y, z]),
            _ => excluded += 1,
        }
    }
    if let Some(reason) = failure {
        return kit::failed(ctx, &reason, RATIO);
    }
    disclose_excluded(ctx, excluded, "an x, a y or a z");
    let locale = ctx.locale;
    let read_x = block.readout_format(&block.x, locale);
    let read_y = block.readout_format(&block.y, locale);
    let read_z = block.readout_format(&block.color, locale);
    // Whether the rows fill a grid, the values interpolated
    // between them jump with the data, and so do the readout texts they make, so the exact rows
    // decide them (TYP-36).
    let grids = [ctx.grid_for(&xf), ctx.grid_for(&yf), ctx.grid_for(&zf)];
    let build = || match mesh_of(&pts) {
        Ok(m) => {
            let texts = quad_texts(&m, &read_x, &read_y, &read_z);
            (Ok(Record::of(&m, &grids, [&read_x, &read_y, &read_z])), texts)
        }
        Err(why) => (Err(why), Vec::new()),
    };
    let (made, texts): (Result<Record, Unmeshable>, Vec<[String; 3]>) = ctx.derive_projected(0, "surface.mesh", build);
    let mesh = match made {
        Ok(r) => r.mesh(),
        Err(Unmeshable::Over(what, count)) => {
            let reason = kit::over_limit(ctx, "surface", &what, count, Some("chart.data"));
            return kit::failed(ctx, &reason, RATIO);
        }
        Err(Unmeshable::Why(why)) => {
            return fail(
                ctx,
                &format!("Cannot draw this surface · {why}"),
                "Give at least three rows that differ in x and in y.",
            );
        }
    };

    let width = ctx.opts.width;
    let height = (width / block.style.ratio.unwrap_or(RATIO)).max(MIN_PLOT);
    let figure = Rect::new(0.0, 0.0, width, height);
    let inner = Rect::new(4.0, 4.0, (width - 8.0).max(1.0), (height - 8.0).max(1.0));
    let edge = project3d::edge_estimate(inner);
    let x_range = extent(mesh.xs.iter().copied());
    let y_range = extent(mesh.ys.iter().copied());
    let z_range = extent(mesh.z.iter().flatten().copied());
    let label_x = block.label_format(&block.x, locale);
    let label_y = block.label_format(&block.y, locale);
    let label_z = block.label_format(&block.color, locale);
    let title = |label: &Option<String>, field: &str| label.clone().unwrap_or_else(|| field.to_owned());
    let (ax, lx) = project3d::linear_axis(ctx, x_range, Zero::None, edge, &label_x, title(&block.x.label, &xf));
    let (ay, ly) = project3d::linear_axis(ctx, y_range, Zero::None, edge, &label_y, title(&block.y.label, &yf));
    let (az, lz) = project3d::linear_axis(ctx, z_range, Zero::None, edge, &label_z, title(&block.color.label, &zf));
    let axes: [Axis3; 3] = [ax, ay, az];
    let scene = project3d::fit(ctx, View::of(block), &axes, inner, 0.0);
    let boxed = project3d::draw_box(ctx, &scene, &axes, Some(figure));
    for (text, reason, rect) in boxed.dropped {
        ctx.drop_label(&text, reason, "axis label", Some(rect));
    }

    // The mesh, far quads first.
    let mut quads: Vec<Quad> = Vec::new();
    for (ix, xw) in mesh.xs.windows(2).enumerate() {
        for (iy, yw) in mesh.ys.windows(2).enumerate() {
            let id = quads.len();
            let zs = [
                mesh.z[ix][iy],
                mesh.z[ix + 1][iy],
                mesh.z[ix + 1][iy + 1],
                mesh.z[ix][iy + 1],
            ];
            let corner = |x: f64, y: f64, z: f64| scene.at([lx.t(x), ly.t(y), lz.t(z)]);
            let pts = [
                corner(xw[0], yw[0], zs[0]),
                corner(xw[1], yw[0], zs[1]),
                corner(xw[1], yw[1], zs[2]),
                corner(xw[0], yw[1], zs[3]),
            ];
            quads.push(Quad {
                id,
                depth: pts.iter().map(|p| p.depth).sum::<f64>() / 4.0,
                z: zs.iter().sum::<f64>() / 4.0,
                pts,
            });
        }
    }
    quads.sort_by(|a, b| a.depth.total_cmp(&b.depth));
    let wire = Stroke::solid(Style::rgba(ctx.style.surface).with_alpha(0.5), 0.6);
    let mut marks: Vec<Item> = Vec::new();
    let mut observations: Vec<Observation> = Vec::new();
    for (k, q) in quads.iter().enumerate() {
        let fill = Fill::solid(Style::rgba(project3d::ramp_at(ctx, lz.t(q.z) + 0.5)));
        marks.push(project3d::polygon(&q.pts, fill, None, Some(k)));
        let mut ring: Vec<[f64; 2]> = q.pts.iter().map(|p| [p.x, p.y]).collect();
        ring.push(ring[0]);
        marks.push(Item::Line {
            path: Path::polyline(&ring),
            stroke: wire.clone(),
            role: Role::Grid,
            series: None,
            obs: None,
        });
        let [xr, yr, zr] = texts.get(q.id).cloned().unwrap_or_default();
        let at = project3d::centroid(&q.pts);
        observations.push(Observation {
            row: None,
            dots: vec![],
            series: None,
            at,
            wedge: None,
            area: Some(project3d::bounds_of(&q.pts)),
            radius: 0.0,
            x: at[0],
            readout: Readout {
                heading: format!("{xr} × {yr}"),
                lines: vec![ReadoutLine {
                    label: None,
                    name: block
                        .color
                        .label
                        .clone()
                        .unwrap_or_else(|| super::cartesian::readable_name(&zf)),
                    value: zr,
                    series: None,
                    names_series: false,
                    derived: true,
                }],
            }
            .into(),
        });
    }
    let mut items = boxed.items;
    items.extend(marks);

    // A key that decodes the colour.
    let size = ctx.size();
    let (ascent, descent) = ctx.style.font.vertical();
    let key_w = KEY_W.min((width - 8.0).max(40.0));
    let key_x = (width - key_w) / 2.0;
    let key_y = height + 10.0;
    let steps = 48;
    let mut figure_items: Vec<Item> = Vec::new();
    for s in 0..steps {
        let t0 = s as f64 / steps as f64;
        figure_items.push(Item::Rect {
            rect: Rect::new(key_x + t0 * key_w, key_y, key_w / steps as f64 + 0.3, KEY_H),
            fill: Fill::solid(Style::rgba(project3d::ramp_at(ctx, t0 + 0.5 / steps as f64))),
            stroke: None,
            radius: [0.0; 4],
            role: Role::Key,
            series: None,
            obs: None,
        });
    }
    let baseline = key_y + KEY_H + 4.0 + ascent * size;
    let (lo_text, hi_text) = (numbers::value(lz.lo, &label_z), numbers::value(lz.hi, &label_z));
    let soft = ctx.style.ink_soft;
    figure_items.push(Item::Text(ctx.text(
        &lo_text,
        key_x,
        baseline,
        Anchor::Start,
        Face::Figures,
        Role::Key,
        soft,
    )));
    figure_items.push(Item::Text(ctx.text(
        &hi_text,
        key_x + key_w,
        baseline,
        Anchor::End,
        Face::Figures,
        Role::Key,
        soft,
    )));
    let z_title = title(&block.color.label, &zf);
    if ctx.text_width(&z_title) + ctx.text_width(&lo_text) + ctx.text_width(&hi_text) + 24.0 <= key_w {
        figure_items.push(Item::Text(ctx.text(
            &z_title,
            key_x + key_w / 2.0,
            baseline,
            Anchor::Middle,
            Face::Data,
            Role::Key,
            soft,
        )));
    }

    if mesh.interpolated {
        ctx.note.push(format!(
            "z is interpolated onto a {IDW_NODES} by {IDW_NODES} grid by inverse-distance weighting"
        ));
    }
    ctx.summary = format!(
        "Surface of {zf} over {xf} and {yf}, from {} to {}.",
        numbers::value(z_range[0], &read_z),
        numbers::value(z_range[1], &read_z)
    );
    let panel = Panel {
        facet: None,
        frame: figure,
        plot: inner,
        items,
        observations,
        targeting: Targeting::Contain,
        x: None,
        failure: None,
    };
    Painted {
        height: baseline + descent * size + 4.0,
        panels: vec![panel],
        figure_items,
        legend: None,
        failure: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_grid_is_drawn_as_given_and_anything_else_is_interpolated() {
        let grid: Vec<[f64; 3]> = (0..3)
            .flat_map(|i| (0..4).map(move |j| [i as f64, j as f64, (i * j) as f64]))
            .collect();
        let m = mesh_of(&grid).unwrap();
        assert!(!m.interpolated);
        assert_eq!((m.xs.len(), m.ys.len()), (3, 4));
        assert_eq!(m.z[2][3], 6.0);
        let scattered = [[0.0, 0.0, 1.0], [1.0, 0.0, 2.0], [0.5, 1.0, 3.0], [0.2, 0.3, 4.0]];
        let m = mesh_of(&scattered).unwrap();
        assert!(m.interpolated);
        assert_eq!((m.xs.len(), m.ys.len()), (IDW_NODES, IDW_NODES));
        assert!(m.z.iter().flatten().all(|z| (1.0..=4.0).contains(z)));
    }

    #[test]
    fn a_repeated_position_is_not_a_grid() {
        let rows = [[0.0, 0.0, 1.0], [0.0, 0.0, 3.0], [1.0, 1.0, 2.0], [1.0, 0.0, 2.0]];
        assert!(mesh_of(&rows).unwrap().interpolated);
    }

    #[test]
    fn too_few_or_flat_rows_say_why() {
        assert!(mesh_of(&[[0.0, 0.0, 1.0], [1.0, 1.0, 2.0]]).is_err());
        assert!(mesh_of(&[[0.0, 0.0, 1.0], [0.0, 1.0, 2.0], [0.0, 2.0, 3.0]]).is_err());
    }

    #[test]
    fn interpolation_returns_a_row_exactly_where_it_lies() {
        let rows = [[0.0, 0.0, 5.0], [1.0, 1.0, 9.0]];
        assert_eq!(idw(&rows, 1.0, 1.0), 9.0);
        let mid = idw(&rows, 0.5, 0.5);
        assert!((mid - 7.0).abs() < 1e-9);
    }

    #[test]
    fn a_grid_over_the_limit_or_too_many_scattered_rows_are_refused_not_thinned() {
        let n = limits::max("surface", "grid nodes per axis") + 1;
        let grid: Vec<[f64; 3]> = (0..n)
            .flat_map(|i| (0..2).map(move |j| [i as f64, j as f64, 1.0]))
            .collect();
        assert!(matches!(mesh_of(&grid), Err(Unmeshable::Over(_, c)) if c == n));
        let rows = limits::max("surface", "scattered rows") + 1;
        let scattered: Vec<[f64; 3]> = (0..rows).map(|i| [(i % 7) as f64, (i % 11) as f64, i as f64]).collect();
        assert!(matches!(mesh_of(&scattered), Err(Unmeshable::Over(_, c)) if c == rows));
    }

    #[test]
    fn a_grid_at_the_limit_keeps_every_node() {
        let n = limits::max("surface", "grid nodes per axis");
        let grid: Vec<[f64; 3]> = (0..n)
            .flat_map(|i| (0..2).map(move |j| [i as f64, j as f64, 1.0]))
            .collect();
        let m = mesh_of(&grid).unwrap();
        assert_eq!((m.xs.len(), m.ys.len()), (n, 2));
    }

    #[test]
    fn computed_nodes_are_short_decimals() {
        assert_eq!(snap(1.234_567_89, 3.0), 1.235);
        assert_eq!(snap(5.0, 0.0), 5.0);
    }
}
