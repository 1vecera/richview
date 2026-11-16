//! Icicle charts: a hierarchy as stacked bands, one band per depth from the top down. A part's
//! width is its share of the whole, so children sit under their parent in source order and
//! together span exactly its width. Leaves carry the values the author gave; a parent's width is
//! the sum of its leaves and is shown as derived (FEA-CH-42, FEA-CH-70). Colour follows the
//! top-level part.

use super::hierarchy::{self, Node};
use super::types::Facets;
use super::{Ctx, Painted, Style};
use crate::geometry::{Anchor, DropReason, Face, Fill, Item, Observation, Panel, Rect, Role, Stroke, Targeting};

const WIDE: f64 = 16.0 / 9.0;
/// Room between a part's edge and its name.
const PAD: f64 = 5.0;

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let value_field = block.encoding.y.clone().unwrap_or_default();
    let readout = block.readout_format(&block.y, ctx.locale);
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut trees: Vec<Vec<Node>> = Vec::new();
    let mut excluded = 0usize;
    for (fi, (_, members)) in facets.iter().enumerate() {
        let tree = hierarchy::read(ctx, fi, members, "icicle");
        failures[fi] = tree.failure;
        excluded += tree.excluded;
        trees.push(tree.nodes);
    }
    let ratio = block.style.ratio.unwrap_or(WIDE);
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
        let top: Vec<usize> = (0..nodes.len())
            .filter(|&k| nodes[k].parent.is_none() && !ctx.is_hidden(hierarchy::root_slot(nodes, &roots, k)))
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
        // Spans along the width as shares of the whole: roots side by side, children inside
        // their parent in source order.
        let mut spans: Vec<Option<(f64, f64)>> = vec![None; nodes.len()];
        let mut at = 0.0;
        for &k in &top {
            if nodes[k].total > 0.0 {
                let end = at + nodes[k].total / whole;
                spans[k] = Some((at, end));
                at = end;
            }
        }
        let mut order: Vec<usize> = Vec::new();
        let mut stack: Vec<usize> = top.iter().rev().copied().collect();
        while let Some(k) = stack.pop() {
            let Some((s0, s1)) = spans[k] else { continue };
            order.push(k);
            let mut from = s0;
            let mut done = 0.0;
            for &c in &nodes[k].children {
                if nodes[c].total > 0.0 {
                    done += nodes[c].total;
                    let to = s0 + (s1 - s0) * (done / nodes[k].total).min(1.0);
                    spans[c] = Some((from, to));
                    from = to;
                }
            }
            stack.extend(nodes[k].children.iter().rev().copied());
        }
        let levels = order.iter().map(|&k| nodes[k].depth).max().unwrap_or(0) + 1;
        let band = plot.h / levels as f64;
        let mut items: Vec<Item> = Vec::new();
        let mut observations = Vec::new();
        for &k in &order {
            let n = &nodes[k];
            let Some((s0, s1)) = spans[k] else { continue };
            let rect = Rect::new(
                plot.x + s0 * plot.w,
                plot.y + n.depth as f64 * band,
                (s1 - s0) * plot.w,
                band,
            );
            let slot = hierarchy::root_slot(nodes, &roots, k);
            let base = ctx.series_color(slot);
            let alpha = hierarchy::depth_alpha(n.depth);
            let hatch = ctx.series[slot].hatch;
            items.push(Item::Rect {
                rect,
                fill: Fill {
                    color: Style::rgba(base).with_alpha(alpha),
                    hatch,
                    hatch_ink: hatch.map(|_| surface),
                    hatch_scale: 1.0,
                },
                stroke: Some(Stroke::solid(surface, 1.5)),
                radius: [0.0; 4],
                role: Role::Mark,
                series: Some(slot),
                obs: Some(observations.len()),
            });
            observations.push(Observation {
                row: n.row,
                dots: vec![],
                series: Some(slot),
                at: rect.center(),
                wedge: None,
                area: Some(rect),
                radius: 0.0,
                x: rect.center()[0],
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
            // The name when it fits the band; otherwise it is reported, never clipped.
            let ink = hierarchy::ink_on(ctx, base, alpha);
            let label = ctx.text_mid(
                &n.name,
                rect.x + PAD,
                rect.y + rect.h / 2.0,
                Anchor::Start,
                Face::Data,
                Role::Label,
                ink,
            );
            if label.bounds.inside(&rect.inflate(-2.0), 0.0) && label.bounds.right() + PAD <= rect.right() {
                items.push(Item::Text(label));
            } else {
                ctx.drop_label(&n.name, DropReason::Density, "band", Some(rect));
            }
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
    ctx.summary = format!("Icicle chart of {parts} parts in {} groups.", roots.len());
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
