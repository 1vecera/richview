//! Three-dimensional scatterplots: points at `x`, `y` and `z` in a box seen from `style.azimuth`
//! and `style.elevation`, drawn as dots far to near (FEA-CH-75). Without `size`, a nearer dot is
//! larger and more opaque so depth reads; with `size`, the radius shows the size (area
//! proportional to the value, as in a bubble chart) and only the opacity follows depth. `color`
//! splits the points into series. A few points stand on a faint line to the floor so their
//! position reads. The box, grid and ticks are shared with the other 3-D types (`project3d`);
//! turning the view with the pointer is host work.

use super::kit;
use super::project3d::{self, Axis3, View};
use super::sample::{disclose_excluded, read_number};
use super::types::Facets;
use super::{BUBBLE_MAX, Ctx, MIN_PLOT, POINT_RADIUS, Painted, Style};
use crate::axis::Zero;
use crate::derived::DataRange;
use crate::diag::Level;
use crate::geometry::{
    Anchor, Face, Fill, Item, Observation, Panel, Path, Readout, ReadoutLine, Rect, Role, Shape, Stroke, Swatch,
    Targeting,
};
use crate::limits;
use crate::numbers;
use crate::rows;

const RATIO: f64 = 4.0 / 3.0;
/// Fewest dots for which a line to the floor is not drawn.
const STEMS_UP_TO: usize = 80;
/// Smallest radius a sized dot gets.
const MIN_RADIUS: f64 = 1.5;
/// Opacity of the farthest and of the nearest dot.
const FAR: f64 = 0.5;
const NEAR: f64 = 0.85;

struct Dot {
    row: usize,
    p: [f64; 3],
    size: Option<f64>,
    group: usize,
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

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let (Some(xf), Some(yf), Some(zf)) = (block.encoding.x.clone(), block.encoding.y.clone(), block.channel("z"))
    else {
        return fail(
            ctx,
            "Cannot draw this 3-D scatterplot · It needs encoding.x, encoding.y and encoding.z",
            "Give encoding.x, encoding.y and encoding.z.",
        );
    };
    let size_field = block.encoding.size.clone();
    let groups: Vec<String> = match &block.encoding.color {
        Some(f) => rows::distinct(ctx.rows, f),
        None => vec!["Points".to_owned()],
    };
    ctx.set_series(&groups);
    let data = ctx.rows;
    let mut failure: Option<String> = None;
    let mut excluded = 0usize;
    let mut dots: Vec<Dot> = Vec::new();
    let members: Vec<usize> = facets.iter().flat_map(|(_, m)| m.iter().copied()).collect();
    for i in members {
        let x = read_number(ctx, "3-D scatterplot", &mut failure, i, &xf);
        let y = read_number(ctx, "3-D scatterplot", &mut failure, i, &yf);
        let z = read_number(ctx, "3-D scatterplot", &mut failure, i, &zf);
        let size = match &size_field {
            Some(f) => read_number(ctx, "3-D scatterplot", &mut failure, i, f),
            None => Ok(None),
        };
        let (Ok(x), Ok(y), Ok(z), Ok(size)) = (x, y, z, size) else {
            continue;
        };
        let group = match &block.encoding.color {
            Some(f) => rows::text(&data[i], f).and_then(|g| groups.iter().position(|k| *k == g)),
            None => Some(0),
        };
        let (Some(x), Some(y), Some(z), Some(group)) = (x, y, z, group) else {
            excluded += 1;
            continue;
        };
        if size_field.is_some() && size.is_none() {
            excluded += 1;
            continue;
        }
        dots.push(Dot {
            row: i,
            p: [x, y, z],
            size,
            group,
        });
    }
    if let Some(reason) = failure {
        return kit::failed(ctx, &reason, RATIO);
    }
    // A size below zero leaves its dot out; whether a size is below zero jumps with the data, so
    // the exact rows decide which rows stay (TYP-36).
    let kept: Vec<usize> = ctx.derive(0, "scatter3d.rows", || {
        let mut rows: Vec<usize> = dots
            .iter()
            .filter(|d| d.size.is_none_or(|s| s >= 0.0))
            .map(|d| d.row)
            .collect();
        rows.sort_unstable();
        rows
    });
    let before = dots.len();
    dots.retain(|d| kept.binary_search(&d.row).is_ok());
    excluded += before - dots.len();
    disclose_excluded(ctx, excluded, "an x, a y, a z, a group or a size");
    if dots.is_empty() {
        return fail(
            ctx,
            "Cannot draw this 3-D scatterplot · No row has an x, a y and a z",
            "Give every point an x, a y and a z.",
        );
    }
    if dots.len() > limits::max("scatter3d", "points") {
        let reason = kit::over_limit(ctx, "scatter3d", "points", dots.len(), Some("chart.data"));
        return kit::failed(ctx, &reason, RATIO);
    }

    let locale = ctx.locale;
    let width = ctx.opts.width;
    let height = (width / block.style.ratio.unwrap_or(RATIO)).max(MIN_PLOT);
    let figure = Rect::new(0.0, 0.0, width, height);
    let inner = Rect::new(4.0, 4.0, (width - 8.0).max(1.0), (height - 8.0).max(1.0));
    let edge = project3d::edge_estimate(inner);
    let extent = |pick: fn(&Dot) -> f64| {
        dots.iter()
            .map(pick)
            .fold([f64::INFINITY, f64::NEG_INFINITY], |[lo, hi], v| [lo.min(v), hi.max(v)])
    };
    let label_x = block.label_format(&block.x, locale);
    let label_y = block.label_format(&block.y, locale);
    let label_z = block.label_format(&block.color, locale);
    let title = |label: &Option<String>, field: &str| label.clone().unwrap_or_else(|| field.to_owned());
    // The extents set the axes (their ticks and where the dots stand), so the exact rows decide
    // them (TYP-36).
    let read_x = block.readout_format(&block.x, locale);
    let read_y = block.readout_format(&block.y, locale);
    let read_z = block.readout_format(&block.color, locale);
    let ranges: [DataRange; 3] = ctx.derive_projected(0, "scatter3d.extents", || {
        [
            DataRange::encode(extent(|d| d.p[0]), ctx.grid_for(&xf), &read_x),
            DataRange::encode(extent(|d| d.p[1]), ctx.grid_for(&yf), &read_y),
            DataRange::encode(extent(|d| d.p[2]), ctx.grid_for(&zf), &read_z),
        ]
    });
    let extents = ranges.each_ref().map(DataRange::bounds);
    let (ax, lx) = project3d::linear_axis(ctx, extents[0], Zero::None, edge, &label_x, title(&block.x.label, &xf));
    let (ay, ly) = project3d::linear_axis(ctx, extents[1], Zero::None, edge, &label_y, title(&block.y.label, &yf));
    let (az, lz) = project3d::linear_axis(
        ctx,
        extents[2],
        Zero::None,
        edge,
        &label_z,
        title(&block.color.label, &zf),
    );
    let axes: [Axis3; 3] = [ax, ay, az];
    let max_size = dots
        .iter()
        .filter_map(|d| d.size)
        .fold(ctx.wanted().size.unwrap_or(0.0), f64::max);
    let sized = size_field.is_some();
    let reach = (if sized { BUBBLE_MAX } else { POINT_RADIUS * 1.3 }) + 1.0;
    let view = View::of(block);
    let scene = project3d::fit(ctx, view, &axes, inner, reach);
    let boxed = project3d::draw_box(ctx, &scene, &axes, Some(figure));
    for (text, reason, rect) in boxed.dropped {
        ctx.drop_label(&text, reason, "axis label", Some(rect));
    }

    // Depth runs over the cube, so a dot's nearness does not depend on the other dots.
    let (mut near, mut far) = (f64::NEG_INFINITY, f64::INFINITY);
    for cx in [-0.5, 0.5] {
        for cy in [-0.5, 0.5] {
            for cz in [-0.5, 0.5] {
                let d = view.project([cx, cy, cz]).depth;
                near = near.max(d);
                far = far.min(d);
            }
        }
    }
    let nearness = |depth: f64| {
        let t = (depth - far) / (near - far).max(1e-9);
        t.clamp(0.0, 1.0)
    };
    let position = |d: &Dot| {
        [
            ranges[0].project(d.p[0], &read_x),
            ranges[1].project(d.p[1], &read_y),
            ranges[2].project(d.p[2], &read_z),
        ]
    };
    let at = |d: &Dot| {
        let p = position(d);
        scene.at([lx.t(p[0]), ly.t(p[1]), lz.t(p[2])])
    };
    // Dots are painted far to near. Near ties jump with the data, so the exact rows decide the
    // order (TYP-36). The observations keep the source order, so a dot's readout is its own
    // whatever the painting order, and a mark points at its observation.
    let depth_order: Vec<usize> = ctx.derive(0, "scatter3d.depth", || {
        let mut all: Vec<usize> = (0..dots.len()).collect();
        let exact_at = |d: &Dot| scene.at([lx.t(d.p[0]), ly.t(d.p[1]), lz.t(d.p[2])]);
        all.sort_by(|a, b| exact_at(&dots[*a]).depth.total_cmp(&exact_at(&dots[*b]).depth));
        all
    });
    let mut rank: Vec<Option<usize>> = vec![None; dots.len()];
    let shown = (0..dots.len()).filter(|j| !ctx.is_hidden(dots[*j].group));
    for (r, j) in shown.enumerate() {
        rank[j] = Some(r);
    }
    let placed: Vec<(usize, &Dot, project3d::Pt)> = depth_order
        .iter()
        .filter(|j| rank.get(**j).is_some_and(Option::is_some))
        .map(|&j| (j, &dots[j], at(&dots[j])))
        .collect();
    let surface = Style::rgba(ctx.style.surface);
    let stem = Stroke::solid(Style::rgba(ctx.style.rule_soft), 1.0);
    let readout_name =
        |label: &Option<String>, field: &str| label.clone().unwrap_or_else(|| super::cartesian::readable_name(field));
    let x_name = readout_name(&block.x.label, &xf);
    let y_name = readout_name(&block.y.label, &yf);
    let z_name = readout_name(&block.color.label, &zf);
    let read_size = block.readout_format(&crate::block::Axis::default(), locale);
    let mut stems: Vec<Item> = Vec::new();
    let mut marks: Vec<Item> = Vec::new();
    let mut observations: Vec<(usize, Observation)> = Vec::new();
    for (j, d, p) in &placed {
        let k = rank[*j].unwrap_or_default();
        let t = nearness(p.depth);
        let r = match d.size {
            Some(s) if max_size > 0.0 => (BUBBLE_MAX * (s / max_size).sqrt()).max(MIN_RADIUS),
            Some(_) => MIN_RADIUS,
            None => POINT_RADIUS * (0.7 + 0.6 * t),
        };
        let color = Style::rgba(ctx.series_color(d.group));
        let center = [p.x, p.y];
        if placed.len() <= STEMS_UP_TO {
            let p = position(d);
            let floor = scene.at([lx.t(p[0]), ly.t(p[1]), -0.5]);
            stems.push(Item::Line {
                path: Path::polyline(&[center, [floor.x, floor.y]]),
                stroke: stem.clone(),
                role: Role::Leader,
                series: None,
                obs: None,
            });
        }
        marks.push(match ctx.series[d.group].shape {
            Some(shape) => Item::Point {
                center,
                radius: r,
                shape,
                fill: None,
                stroke: Some(Stroke::solid(color, 1.5)),
                role: Role::Mark,
                series: Some(d.group),
                obs: Some(k),
            },
            None => Item::Point {
                center,
                radius: r,
                shape: Shape::Circle,
                fill: Some(Fill::solid(color.with_alpha(FAR + (NEAR - FAR) * t))),
                stroke: Some(Stroke::solid(surface.with_alpha(0.8), 0.75)),
                role: Role::Mark,
                series: Some(d.group),
                obs: Some(k),
            },
        });
        let label_on_x = block.encoding.label.as_ref() == Some(&xf);
        let label_on_y = block.encoding.label.as_ref() == Some(&yf);
        let label_on_z = block.encoding.label.as_ref() == Some(&zf);
        let mut lines = vec![
            ReadoutLine {
                name: x_name.clone(),
                value: numbers::value(d.p[0], &read_x),
                series: None,
                names_series: false,
                derived: false,
            },
            ReadoutLine {
                name: y_name.clone(),
                value: numbers::value(d.p[1], &read_y),
                series: None,
                names_series: false,
                derived: false,
            },
            ReadoutLine {
                name: z_name.clone(),
                value: numbers::value(d.p[2], &read_z),
                series: None,
                names_series: false,
                derived: false,
            },
        ];
        if let (Some(field), Some(s)) = (&size_field, d.size) {
            lines.push(ReadoutLine {
                name: block
                    .size_axis()
                    .label
                    .clone()
                    .unwrap_or_else(|| super::cartesian::readable_name(field)),
                value: numbers::value(s, &read_size),
                series: None,
                names_series: false,
                derived: false,
            });
        }
        let heading = if label_on_x {
            format!("{} {}", lines[0].name, lines[0].value)
        } else if label_on_y {
            format!("{} {}", lines[1].name, lines[1].value)
        } else if label_on_z {
            format!("{} {}", lines[2].name, lines[2].value)
        } else {
            block
                .encoding
                .label
                .as_ref()
                .and_then(|f| rows::text(&data[d.row], f))
                .or_else(|| block.encoding.color.as_ref().map(|_| groups[d.group].clone()))
                .unwrap_or_else(|| format!("Point {}", d.row + 1))
        };
        if label_on_z {
            lines.remove(2);
        }
        if label_on_y {
            lines.remove(1);
        }
        if label_on_x {
            lines.remove(0);
        }
        if let Some(field) = &block.encoding.color
            && block.encoding.color != block.encoding.label
            && heading != groups[d.group]
        {
            lines.push(ReadoutLine {
                name: super::cartesian::readable_name(field),
                value: groups[d.group].clone(),
                series: Some(d.group),
                names_series: true,
                derived: false,
            });
        }
        let observation = Observation {
            row: Some(d.row),
            dots: Vec::new(),
            series: Some(d.group),
            at: center,
            wedge: None,
            area: None,
            radius: r.max(POINT_RADIUS),
            x: center[0],
            readout: Readout { heading, lines }.into(),
        };
        observations.push((k, observation));
    }
    observations.sort_by_key(|(k, _)| *k);
    let observations: Vec<Observation> = observations.into_iter().map(|(_, o)| o).collect();
    let mut items = boxed.items;
    items.extend(stems);
    items.extend(marks);

    // The size of the largest dot, as under a bubble chart.
    let mut figure_items: Vec<Item> = Vec::new();
    let mut total = height;
    if sized && max_size > 0.0 {
        let r = BUBBLE_MAX;
        let cy = height + 14.0 + r;
        figure_items.push(Item::Point {
            center: [r + 1.0, cy],
            radius: r,
            shape: Shape::Circle,
            fill: None,
            stroke: Some(Stroke::solid(Style::rgba(ctx.style.ink_soft), 1.0)),
            role: Role::Key,
            series: None,
            obs: None,
        });
        let f = block.label_format(&crate::block::Axis::default(), locale);
        let field = size_field.clone().unwrap_or_default();
        figure_items.push(Item::Text(ctx.text_mid(
            &format!("{field} {}", numbers::value(max_size, &f)),
            2.0 * r + 10.0,
            cy,
            Anchor::Start,
            Face::Data,
            Role::Key,
            ctx.style.ink_soft,
        )));
        total = height + 14.0 + 2.0 * r + 4.0;
    }
    ctx.summary = format!(
        "3-D scatterplot of {} with {xf}, {yf} and {zf}.",
        super::count(placed.len(), "point", "points")
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
    let legend = kit::legend(ctx, Swatch::Dot);
    Painted {
        height: total,
        panels: vec![panel],
        figure_items,
        legend,
        failure: None,
    }
}
