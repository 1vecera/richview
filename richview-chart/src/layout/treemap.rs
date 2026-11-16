//! Treemaps: a hierarchy of named parts whose areas are their values, squarified so tiles stay
//! near square. Leaves carry the values the author gave; a parent's area is the sum of its
//! leaves and is shown as derived (FEA-CH-42, FEA-CH-70). Children fill their parent's tile
//! without padding, so areas stay proportional to values.

use super::types::Facets;
use super::{Ctx, Painted, Style};
use crate::geometry::{Anchor, DropReason, Face, Fill, Item, Observation, Panel, Rect, Role, Stroke, Targeting};
use crate::numbers::{Format, Kind};
use crate::payload::Quant;
use serde::{Deserialize, Serialize};

const RATIO: f64 = 4.0 / 3.0;
/// A tile with a side shorter than this many pixels is not drawn; its part is reported and stays
/// reachable by keyboard.
const MIN_TILE: f64 = 1.0;

use super::hierarchy::{self, Node};

/// Shared normalized edges keep adjoining tiles together when positions move to the grid.
#[derive(Serialize, Deserialize)]
struct Tiles {
    grid: Quant,
    tiles: Vec<Option<Tile>>,
}

#[derive(Serialize, Deserialize)]
struct Tile {
    depth: usize,
    /// Left, top, right and bottom; each is an index on the same [0, 1] grid.
    bounds: [u64; 4],
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let value_field = block.encoding.y.clone().unwrap_or_default();
    let readout = block.readout_format(&block.y, ctx.locale);
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut trees: Vec<Vec<Node>> = Vec::new();
    let mut excluded = 0usize;
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut tree = hierarchy::read(ctx, fi, members, "treemap");
        // A hidden view re-tiles only the values readers can retrieve, in source order (TYP-36).
        if tree.failure.is_none() && !ctx.opts.hidden.is_empty() {
            let mut order: Vec<usize> = (0..tree.nodes.len()).collect();
            order.sort_by_key(|&k| std::cmp::Reverse(tree.nodes[k].depth));
            for k in order {
                tree.nodes[k].total = if tree.nodes[k].children.is_empty() {
                    crate::numbers::shown(tree.nodes[k].given.unwrap_or(0.0), &readout)
                } else {
                    tree.nodes[k].children.iter().map(|&c| tree.nodes[c].total).sum()
                };
            }
        }
        failures[fi] = tree.failure;
        excluded += tree.excluded;
        trees.push(tree.nodes);
    }
    if failures.iter().all(Option::is_some) {
        let reason = failures.iter().flatten().next().cloned().unwrap_or_default();
        let width = ctx.opts.width;
        let h = width / RATIO;
        let panel = super::failed_panel(ctx, Rect::new(0.0, 0.0, width, h), &reason);
        return Painted {
            height: h,
            panels: vec![panel],
            figure_items: vec![],
            legend: None,
            failure: Some(reason),
        };
    }
    hierarchy::report_excluded(ctx, excluded);
    // Colour follows the top-level part a tile sits under.
    let roots = hierarchy::root_names(&trees);
    ctx.set_series(&roots);
    let share_label = hierarchy::share_name(ctx, &roots);
    let mut untiled = 0usize;
    let width = ctx.opts.width;
    let lh = ctx.line_height();
    let title_room = if block.encoding.facet.is_some() { lh + 6.0 } else { 0.0 };
    let columns = super::facet_columns(facets.len(), width);
    let pw = (width - 24.0 * (columns as f64 - 1.0)) / columns as f64;
    let ratio = block.style.ratio.unwrap_or(RATIO);
    let panel_h = pw / ratio + title_room;
    let frames = super::grid_frames(facets.len(), width, 0.0, panel_h);
    let surface = Style::rgba(ctx.style.surface);
    let mut panels = Vec::new();
    for (fi, ((name, _), frame)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *frame, title_room, name, reason));
            continue;
        }
        let nodes = &mut trees[fi];
        let plot = Rect::new(frame.x, frame.y + title_room, frame.w, frame.h - title_room);
        let top: Vec<usize> = (0..nodes.len()).filter(|&k| nodes[k].parent.is_none()).collect();
        let visible = |ctx: &Ctx, nodes: &[Node], k: usize| {
            let mut at = k;
            while let Some(p) = nodes[at].parent {
                at = p;
            }
            let slot = roots.iter().position(|r| *r == nodes[at].name).unwrap_or(0);
            !ctx.is_hidden(slot)
        };
        let shown: Vec<usize> = top.iter().copied().filter(|&k| visible(ctx, nodes, k)).collect();
        let total: f64 = shown.iter().map(|&k| nodes[k].total).sum();
        let mut items: Vec<Item> = Vec::new();
        let mut observations = Vec::new();
        if total <= 0.0 {
            panels.push(super::failed_facet(
                ctx,
                *frame,
                title_room,
                name,
                "Nothing to draw · No part has a positive value",
            ));
            continue;
        }
        // The squarified arrangement jumps with the values (which tile closes a row, which of two
        // near-equal parts comes first), so the exact rows decide it, in a box as wide as the plot
        // is tall times its aspect, which no width changes. Every layout scales it to its plot.
        let aspect = plot.w / plot.h;
        let tiles: Tiles = ctx.derive_visible_projected(fi, "treemap.tiles", || {
            place(nodes, &shown, Rect::new(0.0, 0.0, aspect, 1.0), 0);
            let bounds: Vec<_> = nodes
                .iter()
                .map(|n| {
                    let r = n.rect;
                    (r.w > 0.0 || r.h > 0.0).then_some([r.x / aspect, r.y, r.right() / aspect, r.bottom()])
                })
                .collect();
            let positions: Vec<_> = bounds.iter().flatten().flatten().copied().collect();
            let format = Format {
                kind: Kind::Percent,
                unit: None,
                ..readout.clone()
            };
            let grid = crate::derived::normalized_grid(&positions, &format);
            Tiles {
                tiles: nodes
                    .iter()
                    .zip(bounds)
                    .map(|(n, b)| {
                        b.map(|b| Tile {
                            depth: n.depth,
                            bounds: b.map(|v| grid.index_shown(v, &format)),
                        })
                    })
                    .collect(),
                grid,
            }
        });
        for (n, tile) in nodes.iter_mut().zip(&tiles.tiles) {
            if let Some(tile) = tile {
                let [left, top, right, bottom] = tile.bounds.map(|k| tiles.grid.at(k));
                n.rect = Rect::new(
                    plot.x + left * plot.w,
                    plot.y + top * plot.h,
                    (right - left) * plot.w,
                    (bottom - top) * plot.h,
                );
                n.depth = tile.depth;
            } else {
                n.rect = Rect::default();
            }
        }
        let texts = hierarchy::part_texts(ctx, fi, nodes, total, &readout);
        // Parents first so children paint over them.
        let mut order: Vec<usize> = (0..nodes.len()).filter(|&k| visible(ctx, nodes, k)).collect();
        order.sort_by_key(|&k| nodes[k].depth);
        for &k in &order {
            let n = &nodes[k];
            if n.total <= 0.0 {
                continue;
            }
            let mut at = k;
            while let Some(p) = nodes[at].parent {
                at = p;
            }
            let slot = roots.iter().position(|r| *r == nodes[at].name).unwrap_or(0);
            if n.rect.w < MIN_TILE || n.rect.h < MIN_TILE {
                // Too small to draw: a leaf is reported and stays in the keyboard order.
                if n.children.is_empty() {
                    untiled += 1;
                    ctx.drop_label(&n.name, DropReason::Density, "tile", Some(n.rect));
                    let at = if n.rect.w > 0.0 || n.rect.h > 0.0 {
                        n.rect.center()
                    } else {
                        plot.center()
                    };
                    observations.push(Observation {
                        row: n.row,
                        dots: vec![],
                        series: Some(slot),
                        at,
                        wedge: None,
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
                            share_label,
                        )
                        .into(),
                    });
                }
                continue;
            }
            let base = ctx.series_color(slot);
            let color = Style::rgba(base);
            // Names take whichever of ink and surface reads better on the tile.
            let ink = if crate::color::contrast(ctx.style.ink, base) >= crate::color::contrast(ctx.style.surface, base)
            {
                ctx.style.ink
            } else {
                ctx.style.surface
            };
            let leaf = n.children.is_empty();
            let hatch = ctx.series[slot].hatch;
            let obs = leaf.then_some(observations.len());
            if leaf || n.depth == 0 {
                items.push(Item::Rect {
                    rect: n.rect,
                    fill: if leaf {
                        Fill {
                            color: color.with_alpha(0.86 - 0.12 * (n.depth.min(3) as f64)),
                            hatch,
                            hatch_ink: hatch.map(|_| surface),
                            hatch_scale: 1.0,
                        }
                    } else {
                        Fill::solid(color.with_alpha(0.18))
                    },
                    stroke: Some(Stroke::solid(surface, 1.5)),
                    radius: [0.0; 4],
                    role: Role::Mark,
                    series: Some(slot),
                    obs,
                });
            }
            if leaf {
                observations.push(Observation {
                    row: n.row,
                    dots: vec![],
                    series: Some(slot),
                    at: n.rect.center(),
                    wedge: None,
                    area: Some(n.rect),
                    radius: 0.0,
                    x: n.rect.center()[0],
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
                        share_label,
                    )
                    .into(),
                });
                // The name when it fits the tile; otherwise it is reported, never clipped.
                let label = ctx.text(
                    &n.name,
                    n.rect.x + 5.0,
                    n.rect.y + 4.0 + ctx.style.font.vertical().0 * ctx.size(),
                    Anchor::Start,
                    Face::Data,
                    Role::Label,
                    ink,
                );
                if label.bounds.inside(&n.rect.inflate(-3.0), 0.0) {
                    items.push(Item::Text(label));
                } else {
                    ctx.drop_label(&n.name, DropReason::Density, "tile", Some(n.rect));
                }
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
    if untiled > 0 {
        ctx.note.push(format!(
            "{untiled} parts are too small to draw and are only in the table and the keyboard order"
        ));
    }
    let leaves: usize = trees
        .iter()
        .map(|t| t.iter().filter(|n| n.children.is_empty()).count())
        .sum::<usize>()
        - excluded;
    ctx.summary = format!("Treemap of {leaves} parts in {} groups.", roots.len());
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

/// Squarify `ids` into `area`, then each parent's children into what is left inside it.
fn place(nodes: &mut Vec<Node>, ids: &[usize], area: Rect, depth: usize) {
    let mut sorted: Vec<usize> = ids.iter().copied().filter(|&k| nodes[k].total > 0.0).collect();
    sorted.sort_by(|&a, &b| nodes[b].total.total_cmp(&nodes[a].total));
    let sum: f64 = sorted.iter().map(|&k| nodes[k].total).sum();
    if sorted.is_empty() || sum <= 0.0 || area.w <= 0.0 || area.h <= 0.0 {
        return;
    }
    let scale = area.w * area.h / sum;
    let mut rest = area;
    let mut row: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < sorted.len() {
        let k = sorted[i];
        let side = rest.w.min(rest.h);
        let mut with = row.clone();
        with.push(k);
        if row.is_empty() || worst(nodes, &with, side, scale) <= worst(nodes, &row, side, scale) {
            row = with;
            i += 1;
        } else {
            rest = lay_row(nodes, &row, rest, scale, depth);
            row.clear();
        }
    }
    if !row.is_empty() {
        lay_row(nodes, &row, rest, scale, depth);
    }
}

/// The worst aspect ratio in `row` laid along a side of `side` pixels.
fn worst(nodes: &[Node], row: &[usize], side: f64, scale: f64) -> f64 {
    let areas: Vec<f64> = row.iter().map(|&k| nodes[k].total * scale).collect();
    let s: f64 = areas.iter().sum();
    let (min, max) = areas
        .iter()
        .fold((f64::INFINITY, 0.0f64), |(lo, hi), &a| (lo.min(a), hi.max(a)));
    if s <= 0.0 || min <= 0.0 {
        return f64::INFINITY;
    }
    (side * side * max / (s * s)).max(s * s / (side * side * min))
}

/// Lay `row` along the short side of `rest`; returns what remains.
fn lay_row(nodes: &mut Vec<Node>, row: &[usize], rest: Rect, scale: f64, depth: usize) -> Rect {
    let s: f64 = row.iter().map(|&k| nodes[k].total * scale).sum();
    let horizontal = rest.w >= rest.h;
    let thickness = if horizontal { s / rest.h } else { s / rest.w };
    let mut at = if horizontal { rest.y } else { rest.x };
    for &k in row {
        let len = nodes[k].total * scale / thickness;
        let rect = if horizontal {
            Rect::new(rest.x, at, thickness, len)
        } else {
            Rect::new(at, rest.y, len, thickness)
        };
        at += len;
        nodes[k].rect = rect;
        nodes[k].depth = depth;
        // Children share the parent's whole tile, so a part's area is always its share of the
        // whole and equal values give equal tiles however they are grouped.
        let children = nodes[k].children.clone();
        if !children.is_empty() {
            place(nodes, &children, rect, depth + 1);
        }
    }
    if horizontal {
        Rect::new(rest.x + thickness, rest.y, rest.w - thickness, rest.h)
    } else {
        Rect::new(rest.x, rest.y + thickness, rest.w, rest.h - thickness)
    }
}
