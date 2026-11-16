//! Tree charts: a hierarchy as a node-link diagram, layered left to right by depth. Leaves are
//! spaced evenly in source order, each parent sits centred over its children, and elbow curves
//! join them. The value is optional context shown in the readout, never a size (FEA-CH-42).
//! Colour follows the top-level part.

use super::hierarchy::{self, Node};
use super::types::Facets;
use super::{Ctx, Painted, Style};
use crate::geometry::{
    Anchor, DropReason, Face, Fill, Item, Observation, Panel, Path, Readout, ReadoutLine, Rect, Role, Shape, Stroke,
    Targeting,
};
use crate::numbers;

const WIDE: f64 = 16.0 / 9.0;
/// Node radius in pixels, and the smaller one for crowded trees.
const RADIUS: f64 = 4.5;
const SMALL: f64 = 3.0;

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let value_field = block.encoding.y.clone().unwrap_or_default();
    let readout = block.readout_format(&block.y, ctx.locale);
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut trees: Vec<Vec<Node>> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let tree = hierarchy::read_with(ctx, fi, members, "tree", false);
        failures[fi] = tree.failure;
        trees.push(tree.nodes);
    }
    let ratio = block.style.ratio.unwrap_or(WIDE);
    if let Some(painted) = hierarchy::all_failed(ctx, &failures, ratio) {
        return painted;
    }
    let roots = hierarchy::root_names(&trees);
    ctx.set_series(&roots);
    let width = ctx.opts.width;
    let lh = ctx.line_height();
    let title_room = if block.encoding.facet.is_some() { lh + 6.0 } else { 0.0 };
    let columns = super::facet_columns(facets.len(), width);
    let pw = (width - 24.0 * (columns as f64 - 1.0)) / columns as f64;
    let panel_h = pw / ratio + title_room;
    let frames = super::grid_frames(facets.len(), width, 0.0, panel_h);
    let surface = Style::rgba(ctx.style.surface);
    let mut panels = Vec::new();
    let mut deepest = 0;
    for (fi, ((name, _), frame)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *frame, title_room, name, reason));
            continue;
        }
        let nodes = &trees[fi];
        let plot = Rect::new(frame.x, frame.y + title_room, frame.w, frame.h - title_room);
        let top: Vec<usize> = (0..nodes.len())
            .filter(|&k| nodes[k].parent.is_none() && !ctx.is_hidden(hierarchy::root_slot(nodes, &roots, k)))
            .collect();
        if top.is_empty() {
            panels.push(super::failed_facet(
                ctx,
                *frame,
                title_room,
                name,
                "Nothing to draw · Every group is hidden",
            ));
            continue;
        }
        // Depth-first in source order: the visiting order is the leaves' vertical order.
        let mut order: Vec<usize> = Vec::new();
        let mut stack: Vec<usize> = top.iter().rev().copied().collect();
        while let Some(k) = stack.pop() {
            order.push(k);
            stack.extend(nodes[k].children.iter().rev().copied());
        }
        let leaves = order.iter().filter(|&&k| nodes[k].children.is_empty()).count();
        let levels = order.iter().map(|&k| nodes[k].depth).max().unwrap_or(0) + 1;
        deepest = deepest.max(levels);
        // Names of the top and bottom leaves sit level with their nodes, so rows start half a line in.
        let top_pad = (RADIUS + 3.0).max(lh / 2.0 + 2.0);
        let step = if leaves > 1 {
            (plot.h - 2.0 * top_pad) / (leaves as f64 - 1.0)
        } else {
            0.0
        };
        let r = if step < 9.0 && leaves > 1 { SMALL } else { RADIUS };
        // Room on the right for the names of the deepest leaves, up to a third of the width.
        let longest = order
            .iter()
            .filter(|&&k| nodes[k].children.is_empty())
            .map(|&k| ctx.text_width(&nodes[k].name))
            .fold(0.0, f64::max);
        let right_room = (longest + r + 8.0).min(plot.w / 3.0);
        // A top-level part with parts under it has no edge on its left: its name goes there.
        let roots_wide = top
            .iter()
            .filter(|&&k| !nodes[k].children.is_empty())
            .map(|&k| ctx.text_width(&nodes[k].name))
            .fold(0.0, f64::max);
        let left_pad = (roots_wide + r + 8.0).min(plot.w / 4.0).max(r + 3.0);
        let column = if levels > 1 {
            (plot.w - left_pad - right_room) / (levels as f64 - 1.0)
        } else {
            0.0
        };
        let mut at: Vec<[f64; 2]> = vec![[0.0; 2]; nodes.len()];
        let mut next_leaf = 0usize;
        let grid = Grid {
            plot,
            top_pad,
            step,
            left_pad,
            column,
        };
        for &k in &top {
            place(nodes, k, &mut at, &mut next_leaf, &grid);
        }
        let mut items: Vec<Item> = Vec::new();
        let mut observations = Vec::new();
        // Edges first, so nodes and names sit over them.
        let mut edges: Vec<Vec<[f64; 2]>> = Vec::new();
        for &k in &order {
            let Some(p) = nodes[k].parent else { continue };
            let (a, b) = (at[p], at[k]);
            let mid = (a[0] + b[0]) / 2.0;
            let mut path = Path::new();
            path.move_to(a);
            path.curve_to([mid, a[1]], [mid, b[1]], b);
            edges.extend(path.flatten(0.5));
            let slot = hierarchy::root_slot(nodes, &roots, k);
            items.push(Item::Line {
                path,
                stroke: Stroke::solid(Style::rgba(ctx.series_color(slot)).with_alpha(0.55), 1.5),
                role: Role::Mark,
                series: Some(slot),
                obs: None,
            });
        }
        let mut taken: Vec<Rect> = Vec::new();
        let circles: Vec<Rect> = order
            .iter()
            .map(|&k| Rect::new(at[k][0] - r, at[k][1] - r, 2.0 * r, 2.0 * r))
            .collect();
        for (i, &k) in order.iter().enumerate() {
            let n = &nodes[k];
            let slot = hierarchy::root_slot(nodes, &roots, k);
            let base = ctx.series_color(slot);
            let hatch = ctx.series[slot].hatch;
            items.push(Item::Point {
                center: at[k],
                radius: r,
                shape: Shape::Circle,
                fill: Some(Fill {
                    color: Style::rgba(base),
                    hatch,
                    hatch_ink: hatch.map(|_| surface),
                    hatch_scale: 1.0,
                }),
                stroke: Some(Stroke::solid(surface, 1.5)),
                role: Role::Mark,
                series: Some(slot),
                obs: Some(observations.len()),
            });
            let mut lines = Vec::new();
            if let Some(g) = n.given {
                lines.push(ReadoutLine {
                    name: value_field.clone(),
                    value: numbers::value(g, &readout),
                    series: Some(slot),
                    names_series: false,
                    derived: false,
                });
            }
            lines.push(ReadoutLine {
                name: "Children".into(),
                value: n.children.len().to_string(),
                series: None,
                names_series: false,
                derived: true,
            });
            observations.push(Observation {
                row: n.row,
                dots: vec![],
                series: Some(slot),
                at: at[k],
                wedge: None,
                area: None,
                radius: r + 2.0,
                x: at[k][0],
                readout: Readout {
                    heading: hierarchy::ancestry(nodes, k).join(" › "),
                    lines,
                }
                .into(),
            });
            // The name beside its node: right of a leaf; above, below or to the side of a parent,
            // wherever it touches no edge, node or other name. Otherwise it is reported.
            let x = at[k][0];
            let y = at[k][1];
            let leaf = n.children.is_empty();
            let gap = r + 3.0;
            let spots: Vec<(Anchor, f64, f64)> = if leaf {
                vec![(Anchor::Start, x + r + 4.0, y)]
            } else {
                let mut spots = Vec::new();
                if n.parent.is_none() {
                    spots.push((Anchor::End, x - r - 4.0, y));
                }
                spots.extend([
                    (Anchor::Start, x - r, y - gap - lh / 2.0),
                    (Anchor::Start, x - r, y + gap + lh / 2.0),
                    (Anchor::End, x + r, y - gap - lh / 2.0),
                    (Anchor::End, x + r, y + gap + lh / 2.0),
                ]);
                spots
            };
            let mut placed = false;
            for (anchor, tx, cy) in spots {
                let t = ctx.text_mid(&n.name, tx, cy, anchor, Face::Data, Role::Label, ctx.style.ink);
                let b = t.bounds;
                let clear = b.inside(&plot, 0.0)
                    && taken.iter().all(|o| !o.overlaps(&b, 2.0))
                    && circles.iter().enumerate().all(|(j, c)| j == i || !c.overlaps(&b, 1.0))
                    && !touches(&edges, &b.inflate(1.0));
                if clear {
                    taken.push(b);
                    items.push(Item::Text(t));
                    placed = true;
                    break;
                }
            }
            if !placed {
                let b = ctx.text_mid(
                    &n.name,
                    x + r + 4.0,
                    y,
                    Anchor::Start,
                    Face::Data,
                    Role::Label,
                    ctx.style.ink,
                );
                ctx.drop_label(&n.name, DropReason::Collision, "node", Some(b.bounds));
            }
        }
        let mut panel = Panel {
            facet: None,
            frame: *frame,
            plot,
            items,
            observations,
            targeting: Targeting::Radius,
            x: None,
            failure: None,
        };
        super::panel_title(ctx, &mut panel, name);
        panels.push(panel);
    }
    let parts: usize = trees.iter().map(Vec::len).sum();
    ctx.summary = format!("Tree of {parts} parts, {deepest} levels deep.");
    let legend = (roots.len() > 1).then(|| crate::geometry::Legend {
        swatch: crate::geometry::Swatch::Square,
        entries: (0..roots.len()).collect(),
        bottom: ctx.style.legend_bottom,
    });
    Painted {
        height: super::grid_height(facets.len(), columns, panel_h, super::FACET_GAP_Y),
        panels,
        figure_items: vec![],
        legend,
        failure: None,
    }
}

/// Whether any polyline passes through `rect`.
fn touches(lines: &[Vec<[f64; 2]>], rect: &Rect) -> bool {
    lines.iter().any(|line| {
        line.windows(2).any(|w| {
            let (a, b) = (w[0], w[1]);
            let len = (b[0] - a[0]).hypot(b[1] - a[1]);
            let n = (len / 1.5).ceil().max(1.0) as usize;
            (0..=n).any(|i| {
                let t = i as f64 / n as f64;
                rect.contains([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t])
            })
        })
    })
}

/// Where the columns and rows of one panel lie.
struct Grid {
    plot: Rect,
    top_pad: f64,
    step: f64,
    left_pad: f64,
    column: f64,
}

/// Post-order: leaves take the next row, parents the middle of their first and last child.
fn place(nodes: &[Node], k: usize, at: &mut [[f64; 2]], next_leaf: &mut usize, grid: &Grid) {
    let x = grid.plot.x + grid.left_pad + nodes[k].depth as f64 * grid.column;
    if nodes[k].children.is_empty() {
        let y = if grid.step == 0.0 {
            grid.plot.y + grid.plot.h / 2.0
        } else {
            grid.plot.y + grid.top_pad + *next_leaf as f64 * grid.step
        };
        *next_leaf += 1;
        at[k] = [x, y];
    } else {
        for &c in &nodes[k].children {
            place(nodes, c, at, next_leaf, grid);
        }
        let first = nodes[k].children[0];
        let last = nodes[k].children[nodes[k].children.len() - 1];
        at[k] = [x, (at[first][1] + at[last][1]) / 2.0];
    }
}
