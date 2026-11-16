//! Sunburst charts: a hierarchy as concentric rings, one ring per depth from the centre out. A
//! part's angle is its share of the whole, so children share their parent's arc. Leaves carry
//! the values the author gave; a parent's angle is the sum of its leaves and is shown as derived
//! (FEA-CH-42, FEA-CH-70). Colour follows the top-level part.

use super::hierarchy::{self, Node};
use super::pie::wedge_path;
use super::types::Facets;
use super::{Ctx, Painted, Style};
use crate::geometry::{Anchor, DropReason, Face, Fill, Item, Observation, Panel, Rect, Role, Targeting, Wedge};
use std::f64::consts::{FRAC_PI_2, PI, TAU};

const RATIO: f64 = 1.0;
/// Space kept open between neighbouring slices, in pixels.
const GAP: f64 = 0.75;
/// Room kept around a name inside its slice.
const PAD: f64 = 3.0;

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let value_field = block.encoding.y.clone().unwrap_or_default();
    let readout = block.readout_format(&block.y, ctx.locale);
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut trees: Vec<Vec<Node>> = Vec::new();
    let mut excluded = 0usize;
    for (fi, (_, members)) in facets.iter().enumerate() {
        let tree = hierarchy::read(ctx, fi, members, "sunburst");
        failures[fi] = tree.failure;
        excluded += tree.excluded;
        trees.push(tree.nodes);
    }
    let ratio = block.style.ratio.unwrap_or(RATIO);
    if let Some(painted) = hierarchy::all_failed(ctx, &failures, ratio) {
        return painted;
    }
    hierarchy::report_excluded(ctx, excluded);
    let roots = hierarchy::root_names(&trees);
    ctx.set_series(&roots);
    let share = hierarchy::share_name(ctx, &roots);
    let width = ctx.opts.width;
    let lh = ctx.line_height();
    let title_room = if block.encoding.facet.is_some() { lh + 6.0 } else { 0.0 };
    let columns = super::facet_columns(facets.len(), width);
    let pw = (width - 24.0 * (columns as f64 - 1.0)) / columns as f64;
    let panel_h = pw / ratio + title_room;
    let frames = super::grid_frames(facets.len(), width, 0.0, panel_h);
    let surface = Style::rgba(ctx.style.surface);
    let mut panels = Vec::new();
    for (fi, ((name, _), frame)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *frame, title_room, name, reason));
            continue;
        }
        let nodes = &trees[fi];
        let plot = Rect::new(frame.x, frame.y + title_room, frame.w, frame.h - title_room);
        let shown = |ctx: &Ctx, k: usize| !ctx.is_hidden(hierarchy::root_slot(nodes, &roots, k));
        let top: Vec<usize> = (0..nodes.len())
            .filter(|&k| nodes[k].parent.is_none() && shown(ctx, k))
            .collect();
        let whole: f64 = top.iter().map(|&k| nodes[k].total).sum();
        if whole <= 0.0 {
            panels.push(super::failed_facet(
                ctx,
                *frame,
                title_room,
                name,
                "Nothing to draw · No part has a positive value",
            ));
            continue;
        }
        let texts = hierarchy::part_texts(ctx, fi, nodes, whole, &readout);
        // Angles: roots share the circle, children share their parent's arc, in source order.
        let mut arcs: Vec<Option<(f64, f64)>> = vec![None; nodes.len()];
        let mut at = 0.0;
        for &k in &top {
            if nodes[k].total > 0.0 {
                let end = at + nodes[k].total / whole * TAU;
                arcs[k] = Some((at, end));
                at = end;
            }
        }
        let mut order: Vec<usize> = Vec::new();
        let mut stack: Vec<usize> = top.iter().rev().copied().collect();
        while let Some(k) = stack.pop() {
            let Some((a0, a1)) = arcs[k] else { continue };
            order.push(k);
            let mut from = a0;
            let mut done = 0.0;
            for &c in &nodes[k].children {
                if nodes[c].total > 0.0 {
                    done += nodes[c].total;
                    let to = a0 + (a1 - a0) * (done / nodes[k].total).min(1.0);
                    arcs[c] = Some((from, to));
                    from = to;
                }
            }
            stack.extend(nodes[k].children.iter().rev().copied());
        }
        let levels = order.iter().map(|&k| nodes[k].depth).max().unwrap_or(0) + 1;
        let c = plot.center();
        let radius = (plot.w.min(plot.h) / 2.0 - 2.0).max(8.0);
        let band = radius / levels as f64;
        let mut items: Vec<Item> = Vec::new();
        let mut observations = Vec::new();
        for &k in &order {
            let n = &nodes[k];
            let Some((a0, a1)) = arcs[k] else { continue };
            let (r0, r1) = (n.depth as f64 * band, (n.depth + 1) as f64 * band);
            let slot = hierarchy::root_slot(nodes, &roots, k);
            let base = ctx.series_color(slot);
            let alpha = hierarchy::depth_alpha(n.depth);
            let hatch = ctx.series[slot].hatch;
            // The drawn slice is drawn a little smaller than the one the pointer hits, so the
            // surface shows between neighbours.
            let rm = (r0 + r1) / 2.0;
            let full = a1 - a0 >= TAU - 1e-9;
            let inset = if full {
                0.0
            } else {
                (GAP / rm.max(1.0)).min((a1 - a0) / 4.0)
            };
            let inner = if r0 > 0.0 { r0 + GAP } else { 0.0 };
            items.push(Item::Area {
                path: wedge_path(c, (r1 - GAP).max(inner), inner, a0 + inset, a1 - inset),
                fill: Fill {
                    color: Style::rgba(base).with_alpha(alpha),
                    hatch,
                    hatch_ink: hatch.map(|_| surface),
                    hatch_scale: 1.0,
                },
                role: Role::Mark,
                series: Some(slot),
                obs: Some(observations.len()),
            });
            let mid = (a0 + a1) / 2.0;
            let at = [c[0] + rm * (mid - FRAC_PI_2).cos(), c[1] + rm * (mid - FRAC_PI_2).sin()];
            observations.push(Observation {
                row: n.row,
                dots: vec![],
                series: Some(slot),
                at,
                wedge: Some(Wedge {
                    center: c,
                    radius: r1,
                    inner: r0,
                    start: a0,
                    end: a1,
                }),
                area: None,
                radius: 0.0,
                x: at[0],
                readout: hierarchy::part_readout(
                    nodes,
                    k,
                    slot,
                    &texts,
                    &block
                        .y
                        .label
                        .clone()
                        .unwrap_or_else(|| super::cartesian::readable_name(&value_field)),
                    share,
                )
                .into(),
            });
            // The name inside the slice when it fits; otherwise it is reported, never clipped.
            let ink = hierarchy::ink_on(ctx, base, alpha);
            let chord = 2.0 * rm * ((a1 - a0) / 2.0).min(FRAC_PI_2).sin();
            let probe = ctx.text(&n.name, 0.0, 0.0, Anchor::Middle, Face::Data, Role::Label, ink);
            let (tw, th) = (probe.bounds.w, probe.bounds.h);
            let along = tw + 2.0 * PAD <= chord && th + PAD <= r1 - r0;
            let across = tw + 2.0 * PAD <= r1 - r0 && th + PAD <= chord;
            if !(along || across) {
                ctx.drop_label(&n.name, DropReason::Density, "slice", None);
                continue;
            }
            // Text along the arc, or along the radius when only that fits; upside-down text is
            // turned over.
            let deg = mid.to_degrees();
            let mut rotate = if along { deg } else { deg - 90.0 };
            let flip = if along { deg > 90.0 && deg < 270.0 } else { deg > 180.0 };
            if flip {
                rotate += 180.0;
            }
            if full {
                rotate = 0.0;
            }
            let theta = rotate * PI / 180.0;
            // `rotate` turns the text about its baseline anchor; put the middle of the box on `at`.
            let (ascent, descent) = ctx.style.font.vertical();
            let lift = (ascent - descent) * ctx.size() / 2.0;
            let centre = if full { [c[0], c[1] - rm] } else { at };
            let anchor = [centre[0] - lift * theta.sin(), centre[1] + lift * theta.cos()];
            let mut label = ctx.text(
                &n.name,
                anchor[0],
                anchor[1],
                Anchor::Middle,
                Face::Data,
                Role::Label,
                ink,
            );
            label.rotate = rotate;
            items.push(Item::Text(label));
        }
        let mut panel = Panel {
            facet: None,
            frame: *frame,
            plot,
            items,
            observations,
            targeting: Targeting::Contain,
            x: None,
            failure: None,
        };
        super::panel_title(ctx, &mut panel, name);
        panels.push(panel);
    }
    let parts: usize = trees.iter().map(Vec::len).sum::<usize>() - excluded;
    ctx.summary = format!("Sunburst of {parts} parts in {} groups.", roots.len());
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
