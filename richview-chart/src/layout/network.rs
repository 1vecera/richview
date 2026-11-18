//! Network diagrams (force graphs): nodes joined by straight links (FEA-CH-75). The layout is a
//! Fruchterman-Reingold relaxation that starts from a spiral in node order and runs a fixed
//! number of steps, so the same rows always give the same picture. A position carries no value
//! and the chart says so. A node's radius shows how many links it has; an optional value widens
//! its links. Colour separates the connected parts. Names go beside their nodes and are
//! reported when no spot is clear.

use super::graph::{self, Graph, Left, Link, Rule, Values};
use super::hierarchy;
use super::kit;
use super::place;
use super::types::Facets;
use super::{Ctx, Painted, Style};
use crate::geometry::{
    Anchor, DropReason, Face, Fill, Item, Observation, Panel, Path, Readout, ReadoutLine, Rect, Role, Shape, Stroke,
    Swatch, Targeting,
};
use crate::numbers;

const RATIO: f64 = 16.0 / 9.0;
/// The golden angle, which spreads a spiral evenly.
const GOLDEN_ANGLE: f64 = 2.399_963_229_728_653;
const LINK_ALPHA: f64 = 0.5;

/// Positions in a box `aspect` wide and 1 tall, after a fixed number of steps.
fn force(n: usize, edges: &[(usize, usize, f64)], aspect: f64) -> Vec<[f64; 2]> {
    let mut pos: Vec<[f64; 2]> = (0..n)
        .map(|i| {
            let radius = 0.45 * ((i as f64 + 0.5) / n as f64).sqrt();
            let angle = i as f64 * GOLDEN_ANGLE;
            [
                aspect / 2.0 + radius * aspect * crate::det::cos(angle),
                0.5 + radius * crate::det::sin(angle),
            ]
        })
        .collect();
    if n < 2 {
        return pos;
    }
    let k = (aspect / n as f64).sqrt();
    let iterations = ((4.0e7 / ((n * n) as f64)) as usize).clamp(40, 300);
    let mut push = vec![[0.0f64; 2]; n];
    for step in 0..iterations {
        let temperature = 0.1 * (1.0 - step as f64 / iterations as f64) + 0.002;
        for p in push.iter_mut() {
            *p = [0.0, 0.0];
        }
        for i in 0..n {
            for j in (i + 1)..n {
                let mut dx = pos[i][0] - pos[j][0];
                let mut dy = pos[i][1] - pos[j][1];
                let mut d2 = dx * dx + dy * dy;
                if d2 < 1e-12 {
                    dx = (i as f64 - j as f64) * 1e-3;
                    dy = 1e-3;
                    d2 = dx * dx + dy * dy;
                }
                let f = k * k / d2;
                push[i][0] += dx * f;
                push[i][1] += dy * f;
                push[j][0] -= dx * f;
                push[j][1] -= dy * f;
            }
        }
        for &(a, b, w) in edges {
            let dx = pos[a][0] - pos[b][0];
            let dy = pos[a][1] - pos[b][1];
            let d = crate::det::hypot(dx, dy);
            let f = d / k * w;
            push[a][0] -= dx * f;
            push[a][1] -= dy * f;
            push[b][0] += dx * f;
            push[b][1] += dy * f;
        }
        for (p, d) in pos.iter_mut().zip(push.iter()) {
            let towards = [(aspect / 2.0 - p[0]) * k * 0.6, (0.5 - p[1]) * k * 0.6];
            let (fx, fy) = (d[0] + towards[0], d[1] + towards[1]);
            let len = crate::det::hypot(fx, fy);
            if len > 1e-12 {
                let travel = len.min(temperature);
                p[0] = (p[0] + fx / len * travel).clamp(0.0, aspect);
                p[1] = (p[1] + fy / len * travel).clamp(0.0, 1.0);
            }
        }
    }
    pos
}

fn find(parent: &mut [usize], mut k: usize) -> usize {
    while parent[k] != k {
        parent[k] = parent[parent[k]];
        k = parent[k];
    }
    k
}

/// The connected part of each node, numbered from the biggest part down.
fn parts(n: usize, links: &[Link]) -> Vec<usize> {
    let mut parent: Vec<usize> = (0..n).collect();
    for l in links {
        let (a, b) = (find(&mut parent, l.source), find(&mut parent, l.target));
        if a != b {
            parent[a.max(b)] = a.min(b);
        }
    }
    let root: Vec<usize> = (0..n).map(|k| find(&mut parent, k)).collect();
    let mut size = vec![0usize; n];
    for &r in &root {
        size[r] += 1;
    }
    let mut roots: Vec<usize> = (0..n).filter(|&k| root[k] == k).collect();
    roots.sort_by(|&a, &b| size[b].cmp(&size[a]).then(a.cmp(&b)));
    (0..n)
        .map(|k| roots.iter().position(|&r| r == root[k]).unwrap_or(0))
        .collect()
}

fn degrees(g: &Graph) -> Vec<usize> {
    let mut degree = vec![0usize; g.names.len()];
    for l in &g.links {
        degree[l.source] += 1;
        degree[l.target] += 1;
    }
    degree
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let rule = Rule {
        values: Values::Weight,
        loops: false,
    };
    let mut graphs = Vec::new();
    let mut left = Left::default();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut g = graph::read(ctx, fi, members, "network", rule);
        graph::enforce_limits(ctx, &mut g, "network");
        left.add(std::mem::take(&mut g.left));
        graphs.push(g);
    }
    graph::report(
        ctx,
        &left,
        "network",
        "a source, a target or a non-negative value",
        "chart.encoding.source",
    );
    let failures: Vec<Option<String>> = graphs.iter().map(|g| g.failure.clone()).collect();
    let ratio = block.style.ratio.unwrap_or(RATIO);
    if let Some(painted) = hierarchy::all_failed(ctx, &failures, ratio) {
        return painted;
    }
    // Colour slots: the connected parts, the biggest first.
    let mut groups: Vec<Option<Vec<usize>>> = Vec::new();
    let mut most = 0usize;
    for g in &graphs {
        if g.failure.is_some() {
            groups.push(None);
            continue;
        }
        let found = parts(g.names.len(), &g.links);
        most = most.max(found.iter().copied().max().map_or(0, |m| m + 1));
        groups.push(Some(found));
    }
    let series: Vec<String> = (1..=most).map(|i| format!("Group {i}")).collect();
    ctx.set_series(&series);
    let readout = block.readout_format(&block.y, ctx.locale);
    let weighted = block.encoding.y.is_some();
    let value_name = block
        .y
        .label
        .clone()
        .or_else(|| block.encoding.y.clone())
        .unwrap_or_else(|| "Value".into());
    let width = ctx.opts.width;
    let lh = ctx.line_height();
    let title_room = if block.encoding.facet.is_some() { lh + 6.0 } else { 0.0 };
    let columns = super::facet_columns(facets.len(), width);
    let pw = (width - 24.0 * (columns as f64 - 1.0)) / columns as f64;
    let panel_h = pw / ratio + title_room;
    let frames = super::grid_frames(facets.len(), width, 0.0, panel_h);
    let surface = Style::rgba(ctx.style.surface);
    let mut panels = Vec::new();
    let (mut node_count, mut link_count) = (0usize, 0usize);
    for (fi, ((name, _), frame)) in facets.iter().zip(&frames).enumerate() {
        let (g, Some(slots)) = (&graphs[fi], &groups[fi]) else {
            let reason = graphs[fi].failure.clone().unwrap_or_default();
            panels.push(super::failed_facet(ctx, *frame, title_room, name, &reason));
            continue;
        };
        let n = g.names.len();
        let shown: Vec<bool> = slots.iter().map(|&s| !ctx.is_hidden(s)).collect();
        if !shown.iter().any(|s| *s) {
            panels.push(super::failed_facet(
                ctx,
                *frame,
                title_room,
                name,
                "Nothing to draw · Every group is hidden",
            ));
            continue;
        }
        node_count += n;
        link_count += g.links.len();
        let degree = degrees(g);
        let mut strength = vec![0.0; n];
        for l in &g.links {
            strength[l.source] += l.value;
            strength[l.target] += l.value;
        }
        let vmax = g.links.iter().map(|l| l.value).fold(0.0, f64::max);
        let weight = |l: &Link| if l.given && vmax > 0.0 { l.value / vmax } else { 0.0 };
        let plot = Rect::new(frame.x, frame.y + title_room, frame.w, frame.h - title_room);
        let (r_lo, r_hi) = if n > 150 { (2.5, 7.0) } else { (4.0, 13.0) };
        let longest = g
            .names
            .iter()
            .map(|x| ctx.text_width(x))
            .fold(0.0, f64::max)
            .min(plot.w * 0.12);
        let pad_x = r_hi + 6.0 + longest;
        let pad_y = r_hi + lh + 4.0;
        let edges: Vec<(usize, usize, f64)> = g.links.iter().map(|l| (l.source, l.target, 0.5 + weight(l))).collect();
        // The force layout is chaotic: the exact rows run it, in a box as wide as the plot is tall
        // times its aspect, which no width changes, and every layout takes the positions (TYP-36).
        let pos: Vec<[f64; 2]> = ctx.derive(fi, "network.positions", || force(n, &edges, plot.w / plot.h));
        let strength_text: Vec<String> = ctx.derive(fi, "network.strength", || {
            strength.iter().map(|&v| numbers::value(v, &readout)).collect()
        });
        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for p in &pos {
            for axis in 0..2 {
                lo[axis] = lo[axis].min(p[axis]);
                hi[axis] = hi[axis].max(p[axis]);
            }
        }
        let place = |p: &[f64; 2]| -> [f64; 2] {
            let span = [hi[0] - lo[0], hi[1] - lo[1]];
            let room = [plot.w - 2.0 * pad_x, plot.h - 2.0 * pad_y];
            let origin = [plot.x + pad_x, plot.y + pad_y];
            let mut out = [0.0; 2];
            for axis in 0..2 {
                out[axis] = if span[axis] > 1e-9 {
                    origin[axis] + (p[axis] - lo[axis]) / span[axis] * room[axis]
                } else {
                    origin[axis] + room[axis] / 2.0
                };
            }
            out
        };
        let at: Vec<[f64; 2]> = pos.iter().map(place).collect();
        let (dmin, dmax) = (
            degree.iter().copied().min().unwrap_or(0),
            degree.iter().copied().max().unwrap_or(0),
        );
        let radius: Vec<f64> = degree
            .iter()
            .map(|&d| {
                if dmax > dmin {
                    r_lo + (r_hi - r_lo) * ((d - dmin) as f64 / (dmax - dmin) as f64).sqrt()
                } else {
                    (r_lo + r_hi) / 2.0
                }
            })
            .collect();
        let nodes: Vec<usize> = (0..n).filter(|&k| shown[k]).collect();
        let mut lines_items: Vec<Item> = Vec::new();
        let mut node_items: Vec<Item> = Vec::new();
        let mut node_observations: Vec<Observation> = Vec::new();
        let mut link_observations: Vec<Observation> = Vec::new();
        for &k in &nodes {
            let slot = slots[k];
            let hatch = ctx.series.get(slot).and_then(|s| s.hatch);
            node_items.push(Item::Point {
                center: at[k],
                radius: radius[k],
                shape: Shape::Circle,
                fill: Some(Fill {
                    color: Style::rgba(ctx.series_color(slot)),
                    hatch,
                    hatch_ink: hatch.map(|_| surface),
                    hatch_scale: 1.0,
                }),
                stroke: Some(Stroke::solid(surface, 1.5)),
                role: Role::Mark,
                series: Some(slot),
                obs: Some(node_observations.len()),
            });
            let mut lines = vec![ReadoutLine {
                label: None,
                name: "Links".into(),
                value: degree[k].to_string(),
                series: Some(slot),
                names_series: false,
                derived: true,
            }];
            if weighted {
                lines.push(ReadoutLine {
                    label: None,
                    name: format!("Total {value_name}"),
                    value: strength_text[k].clone(),
                    series: None,
                    names_series: false,
                    derived: true,
                });
            }
            node_observations.push(Observation {
                row: None,
                dots: vec![],
                series: Some(slot),
                at: at[k],
                wedge: None,
                area: None,
                radius: radius[k] + 2.0,
                x: at[k][0],
                readout: Readout {
                    heading: g.names[k].clone(),
                    lines,
                }
                .into(),
            });
        }
        let mut link_ink = Vec::with_capacity(g.links.len());
        for l in &g.links {
            if !(shown[l.source] && shown[l.target]) {
                continue;
            }
            let (a, b) = (at[l.source], at[l.target]);
            let slot = slots[l.source];
            let stroke_w = if l.given { 1.0 + 3.0 * weight(l) } else { 1.5 };
            link_ink.push((a, b, stroke_w / 2.0));
            lines_items.push(Item::Line {
                path: Path::polyline(&[a, b]),
                stroke: Stroke::solid(Style::rgba(ctx.series_color(slot)).with_alpha(LINK_ALPHA), stroke_w),
                role: Role::Mark,
                series: Some(slot),
                obs: Some(node_observations.len() + link_observations.len()),
            });
            let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
            let (from, to) = (&g.names[l.source], &g.names[l.target]);
            let lines = if l.given {
                vec![ReadoutLine {
                    label: None,
                    name: value_name.clone(),
                    value: numbers::value(l.value, &readout),
                    series: Some(slot),
                    names_series: false,
                    derived: false,
                }]
            } else {
                vec![
                    ReadoutLine {
                        label: None,
                        name: "From".into(),
                        value: from.clone(),
                        series: Some(slot),
                        names_series: false,
                        derived: false,
                    },
                    ReadoutLine {
                        label: None,
                        name: "To".into(),
                        value: to.clone(),
                        series: None,
                        names_series: false,
                        derived: false,
                    },
                ]
            };
            link_observations.push(Observation {
                row: Some(l.row),
                dots: vec![],
                series: Some(slot),
                at: mid,
                wedge: None,
                area: None,
                radius: (stroke_w / 2.0 + 3.0).max(4.0),
                x: mid[0],
                readout: Readout {
                    heading: format!("{from} – {to}"),
                    lines,
                }
                .into(),
            });
        }
        // Names beside the nodes with most links first: right, left, above, then below; a name
        // that touches a node, link ink or another name gives way and is reported.
        let mut texts: Vec<Item> = Vec::new();
        let mut taken: Vec<Rect> = Vec::new();
        let circles: Vec<Rect> = (0..n)
            .map(|k| {
                Rect::new(
                    at[k][0] - radius[k],
                    at[k][1] - radius[k],
                    2.0 * radius[k],
                    2.0 * radius[k],
                )
            })
            .collect();
        let mut by_links = nodes.clone();
        by_links.sort_by(|&a, &b| degree[b].cmp(&degree[a]).then(a.cmp(&b)));
        for k in by_links {
            let (x, y, r) = (at[k][0], at[k][1], radius[k]);
            let spots = [
                (Anchor::Start, x + r + 3.0, y),
                (Anchor::End, x - r - 3.0, y),
                (Anchor::Middle, x, y - r - 3.0 - lh / 2.0),
                (Anchor::Middle, x, y + r + 3.0 + lh / 2.0),
            ];
            let mut placed = false;
            for (anchor, tx, cy) in spots {
                let t = ctx.text_mid(&g.names[k], tx, cy, anchor, Face::Data, Role::Label, ctx.style.ink);
                let rect = t.bounds;
                let clear = rect.inside(&plot, 0.0)
                    && taken.iter().all(|o| !o.overlaps(&rect, 0.0))
                    && nodes.iter().all(|&j| j == k || !circles[j].overlaps(&rect, 0.0))
                    && link_ink
                        .iter()
                        .all(|&(a, b, half)| !place::segment_touches_rect(a, b, &rect.inflate(half)));
                if clear {
                    taken.push(rect);
                    texts.push(Item::Text(t));
                    placed = true;
                    break;
                }
            }
            if !placed {
                let b = ctx.text_mid(
                    &g.names[k],
                    x + r + 3.0,
                    y,
                    Anchor::Start,
                    Face::Data,
                    Role::Label,
                    ctx.style.ink,
                );
                ctx.drop_label(&g.names[k], DropReason::Collision, "node", Some(b.bounds));
            }
        }
        let mut items = lines_items;
        items.extend(node_items);
        items.extend(texts);
        let mut observations = node_observations;
        observations.extend(link_observations);
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
    ctx.summary = format!(
        "Network of {} joined by {}; larger nodes have more links.",
        super::count(node_count, "node", "nodes"),
        super::count(link_count, "link", "links")
    );
    ctx.note
        .push("node size shows the number of links; positions come from a fixed layout and carry no value".into());
    Painted {
        height: super::grid_height(facets.len(), columns, panel_h, super::FACET_GAP_Y),
        panels,
        figure_items: vec![],
        legend: kit::legend(ctx, Swatch::Square),
        failure: None,
    }
}
