//! Three-dimensional bar charts: a box for each pair of an `x` category and a `color` category
//! with the height of `y`, seen from `style.azimuth` and `style.elevation` (FEA-CH-75). Without
//! `color` there is one row of bars. Each bar is drawn as the three faces the viewer can see (top,
//! and the two sides that face it, shaded apart), bars far to near. A bar stands on zero, so a
//! negative value hangs below it. Several rows for one pair are added up and the readout says so
//! (FEA-CH-42). The box, grid and ticks are shared with the other 3-D types (`project3d`);
//! turning the view with the pointer is host work.

use super::kit;
use super::project3d::{self, Axis3, Pt, View};
use super::sample::{disclose_excluded, read_number};
use super::types::Facets;
use super::{Ctx, MIN_PLOT, Painted, Style};
use crate::axis::Zero;
use crate::diag::Level;
use crate::geometry::{
    Fill, Item, Observation, Panel, Path, Readout, ReadoutLine, Rect, Role, Stroke, Swatch, Targeting,
};
use crate::limits;
use crate::numbers;
use crate::rows;
use std::collections::HashMap;

const RATIO: f64 = 4.0 / 3.0;
/// Share of a cell its bar takes.
const FILL: f64 = 0.62;

/// The sum of the values of one pair and how many rows made it.
#[derive(Debug, Clone, Copy, Default)]
struct Tally {
    sum: f64,
    count: usize,
    first: usize,
}

/// One bar placed in the box.
struct Bar {
    xi: usize,
    gi: usize,
    tally: Tally,
    /// Depth of the bar's footing, which orders the bars.
    depth: f64,
    /// Corners of the footing in the cube.
    x: [f64; 2],
    y: [f64; 2],
    /// Bottom and top of the bar in the cube.
    z: [f64; 2],
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

/// A face of a bar as a filled polygon with a hairline round it.
fn face(points: &[Pt], fill: Fill, outline: &Stroke, series: usize, obs: usize, out: &mut Vec<Item>) {
    out.push(project3d::polygon(points, fill, Some(series), Some(obs)));
    let mut ring: Vec<[f64; 2]> = points.iter().map(|p| [p.x, p.y]).collect();
    ring.push(ring[0]);
    out.push(Item::Line {
        path: Path::polyline(&ring),
        stroke: outline.clone(),
        role: Role::Grid,
        series: None,
        obs: None,
    });
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let (Some(xf), Some(yf)) = (block.encoding.x.clone(), block.encoding.y.clone()) else {
        return fail(
            ctx,
            "Cannot draw this 3-D bar chart · It needs encoding.x and encoding.y",
            "Give encoding.x and encoding.y.",
        );
    };
    let color_field = block.encoding.color.clone();
    let data = ctx.rows;
    let mut failure: Option<String> = None;
    let mut excluded = 0usize;
    let mut x_names: Vec<String> = Vec::new();
    let mut g_names: Vec<String> = Vec::new();
    let mut x_at: HashMap<String, usize> = HashMap::new();
    let mut g_at: HashMap<String, usize> = HashMap::new();
    let mut pairs: HashMap<(usize, usize), Tally> = HashMap::new();
    let members: Vec<usize> = facets.iter().flat_map(|(_, m)| m.iter().copied()).collect();
    for i in members {
        let value = read_number(ctx, "3-D bar chart", &mut failure, i, &yf);
        let Ok(value) = value else {
            continue;
        };
        let x = rows::text(&data[i], &xf);
        let g = match &color_field {
            Some(f) => rows::text(&data[i], f),
            None => Some(String::new()),
        };
        let (Some(x), Some(g), Some(value)) = (x, g, value) else {
            excluded += 1;
            continue;
        };
        let xi = *x_at.entry(x.clone()).or_insert_with(|| {
            x_names.push(x.clone());
            x_names.len() - 1
        });
        let gi = *g_at.entry(g.clone()).or_insert_with(|| {
            g_names.push(g.clone());
            g_names.len() - 1
        });
        let t = pairs.entry((xi, gi)).or_default();
        if t.count == 0 {
            t.first = i;
        }
        t.sum += value;
        t.count += 1;
    }
    if let Some(reason) = failure {
        return kit::failed(ctx, &reason, RATIO);
    }
    disclose_excluded(ctx, excluded, "an x, a value or a group");
    if pairs.is_empty() {
        return fail(
            ctx,
            "Cannot draw this 3-D bar chart · No row has an x and a value",
            "Give every bar an x and a value.",
        );
    }
    if pairs.values().any(|t| !t.sum.is_finite()) {
        return fail(
            ctx,
            "Cannot draw this 3-D bar chart · A sum is too large to represent",
            "Scale the values.",
        );
    }
    // Above a limit the chart is not drawn, so no bar is ever left out.
    let mut over: Option<String> = None;
    for (what, count, path) in [
        ("x categories", x_names.len(), "chart.encoding.x"),
        ("groups", g_names.len(), "chart.encoding.color"),
    ] {
        if count > limits::max("bar3d", what) {
            let reason = kit::over_limit(ctx, "bar3d", what, count, Some(path));
            over.get_or_insert(reason);
        }
    }
    if let Some(reason) = over {
        return kit::failed(ctx, &reason, RATIO);
    }
    let (nx, ng) = (x_names.len(), g_names.len());
    let series_keys: Vec<String> = if color_field.is_some() {
        g_names.clone()
    } else {
        vec!["Bars".to_owned()]
    };
    ctx.set_series(&series_keys);

    let locale = ctx.locale;
    let width = ctx.opts.width;
    let height = (width / block.style.ratio.unwrap_or(RATIO)).max(MIN_PLOT);
    let figure = Rect::new(0.0, 0.0, width, height);
    let inner = Rect::new(4.0, 4.0, (width - 8.0).max(1.0), (height - 8.0).max(1.0));
    let edge = project3d::edge_estimate(inner);
    let drawn: Vec<((usize, usize), Tally)> = pairs.iter().map(|(k, t)| (*k, *t)).collect();
    let label_y = block.label_format(&block.y, locale);
    let read_y = block.readout_format(&block.y, locale);
    // The extent of the sums sets the value axis, and the sums of several rows are written in
    // the readouts: both jump with the data, so the exact rows decide them (TYP-36); the extent is
    // widened to short decimals (FEA-CH-37).
    let ([lo, hi], sum_texts): ([f64; 2], Vec<(usize, usize, String)>) = ctx.derive(0, "bar3d.sums", || {
        let (lo, hi) = drawn
            .iter()
            .fold((0.0f64, 0.0f64), |(lo, hi), (_, t)| (lo.min(t.sum), hi.max(t.sum)));
        let mut texts: Vec<(usize, usize, String)> = drawn
            .iter()
            .map(|((xi, gi), t)| (*xi, *gi, numbers::value(t.sum, &read_y)))
            .collect();
        texts.sort();
        (crate::derived::coarse([lo, hi]), texts)
    });
    let title = |label: &Option<String>, field: &str| label.clone().unwrap_or_else(|| field.to_owned());
    let x_axis = project3d::category_axis(&x_names, title(&block.x.label, &xf));
    let depth_axis = match &color_field {
        Some(f) => project3d::category_axis(&g_names, title(&block.color.label, f)),
        None => Axis3 {
            ticks: Vec::new(),
            title: String::new(),
        },
    };
    let (z_axis, lz) = project3d::linear_axis(ctx, [lo, hi], Zero::Include, edge, &label_y, title(&block.y.label, &yf));
    let axes: [Axis3; 3] = [x_axis, depth_axis, z_axis];
    let view = View::of(block);
    let scene = project3d::fit(ctx, view, &axes, inner, 0.0);
    let boxed = project3d::draw_box(ctx, &scene, &axes, Some(figure));
    for (text, reason, rect) in boxed.dropped {
        ctx.drop_label(&text, reason, "axis label", Some(rect));
    }

    // Bars stand on square footings centred in their cells.
    let cell = 1.0 / nx as f64;
    let cell_depth = 1.0 / ng.max(1) as f64;
    let half = FILL * cell.min(cell_depth) / 2.0;
    let mut bars: Vec<Bar> = Vec::new();
    for ((xi, gi), t) in &drawn {
        if ctx.is_hidden(if color_field.is_some() { *gi } else { 0 }) {
            continue;
        }
        let cx = (*xi as f64 + 0.5) * cell - 0.5;
        let cy = (*gi as f64 + 0.5) * cell_depth - 0.5;
        bars.push(Bar {
            xi: *xi,
            gi: *gi,
            tally: *t,
            depth: view.project([cx, cy, 0.0]).depth,
            x: [cx - half, cx + half],
            y: [cy - half, cy + half],
            z: [lz.t(t.sum.min(0.0)), lz.t(t.sum.max(0.0))],
        });
    }
    bars.sort_by(|a, b| a.depth.total_cmp(&b.depth).then(a.xi.cmp(&b.xi)).then(a.gi.cmp(&b.gi)));

    let toward = view.toward_viewer();
    let outline = Stroke::solid(Style::rgba(ctx.style.surface).with_alpha(0.6), 0.6);
    let surface = Style::rgba(ctx.style.surface);
    let mut marks: Vec<Item> = Vec::new();
    let mut observations: Vec<Observation> = Vec::new();
    for (k, bar) in bars.iter().enumerate() {
        let series = if color_field.is_some() { bar.gi } else { 0 };
        let base = ctx.series_color(series);
        let hatch = ctx.series[series].hatch;
        let paint = |c| Fill {
            color: Style::rgba(c),
            hatch,
            hatch_ink: hatch.map(|_| surface),
            hatch_scale: 1.0,
        };
        let top = paint(project3d::lighter(ctx, base, 0.18));
        let side_x = paint(base);
        let side_y = paint(project3d::darker(ctx, base, 0.22));
        let at = |x: f64, y: f64, z: f64| scene.at([x, y, z]);
        let [x0, x1] = bar.x;
        let [y0, y1] = bar.y;
        let [z0, z1] = bar.z;
        let tall = z1 - z0 > 1e-9;
        if tall {
            let xs = if toward[0] > 0.0 { x1 } else { x0 };
            face(
                &[at(xs, y0, z0), at(xs, y1, z0), at(xs, y1, z1), at(xs, y0, z1)],
                side_x,
                &outline,
                series,
                k,
                &mut marks,
            );
            let ys = if toward[1] > 0.0 { y1 } else { y0 };
            face(
                &[at(x0, ys, z0), at(x1, ys, z0), at(x1, ys, z1), at(x0, ys, z1)],
                side_y,
                &outline,
                series,
                k,
                &mut marks,
            );
        }
        let top_points = [at(x0, y0, z1), at(x1, y0, z1), at(x1, y1, z1), at(x0, y1, z1)];
        face(&top_points, top, &outline, series, k, &mut marks);
        // The pointer finds the bar through the box round everything it draws.
        let foot = [at(x0, y0, z0), at(x1, y0, z0), at(x1, y1, z0), at(x0, y1, z0)];
        let mut all: Vec<Pt> = top_points.to_vec();
        all.extend(foot);
        let derived = bar.tally.count > 1;
        let x_name = &x_names[bar.xi];
        let heading = if color_field.is_some() {
            format!("{x_name} · {}", g_names[bar.gi])
        } else {
            x_name.clone()
        };
        let mut lines = Vec::new();
        lines.push(ReadoutLine {
            name: block
                .y
                .label
                .clone()
                .unwrap_or_else(|| super::cartesian::readable_name(&yf)),
            value: sum_texts
                .iter()
                .find(|(xi, gi, _)| (*xi, *gi) == (bar.xi, bar.gi))
                .map_or_else(|| numbers::value(bar.tally.sum, &read_y), |(_, _, text)| text.clone()),
            series: Some(series),
            names_series: false,
            derived,
        });
        if derived {
            lines.push(ReadoutLine {
                name: "Source rows".into(),
                value: bar.tally.count.to_string(),
                series: None,
                names_series: false,
                derived: true,
            });
        }
        let centre = project3d::centroid(&top_points);
        observations.push(Observation {
            row: (!derived).then_some(bar.tally.first),
            dots: Vec::new(),
            series: Some(series),
            at: centre,
            wedge: None,
            area: Some(project3d::bounds_of(&all)),
            radius: 0.0,
            x: centre[0],
            readout: Readout { heading, lines }.into(),
        });
    }
    let mut items = boxed.items;
    items.extend(marks);
    if bars.iter().any(|b| b.tally.count > 1) {
        ctx.note.push("bars of several rows are sums".into());
    }
    ctx.summary = format!(
        "3-D bar chart of {yf} by {xf}{}: {} from {} to {}.",
        color_field.as_ref().map(|f| format!(" and {f}")).unwrap_or_default(),
        super::count(bars.len(), "bar", "bars"),
        numbers::value(lo, &read_y),
        numbers::value(hi, &read_y)
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
    let legend = kit::legend(ctx, Swatch::Square);
    Painted {
        height,
        panels: vec![panel],
        figure_items: Vec::new(),
        legend,
        failure: None,
    }
}
