//! Chord diagrams (dependency wheels): nodes as arcs around a circle, with ribbons between them
//! (FEA-CH-75). An arc is as long as the flow that enters and leaves its node, and a ribbon is
//! as wide at each end as the flow it carries, so every unit of flow takes up space at both of
//! its nodes. Ribbons take the colour of their source; the order of the ends along an arc follows
//! the circle, so ribbons to neighbours leave from the sides facing them. Names stand outside
//! the ring and are reported when they do not fit. Arc sizes and shares are sums RichView
//! computed and a readout marks them as derived (FEA-CH-42).

use super::graph::{self, Left, Rule, Values};
use super::hierarchy;
use super::kit;
use super::pie::{arc, share_text, wedge_path};
use super::types::Facets;
use super::{Ctx, Painted, Style};
use crate::geometry::{
    Anchor, DropReason, Face, Fill, Item, Observation, Panel, Path, Readout, ReadoutLine, Rect, Role, Swatch,
    Targeting, Wedge,
};
use crate::numbers;
use serde::{Deserialize, Serialize};
use std::f64::consts::{FRAC_PI_2, TAU};

const RATIO: f64 = 4.0 / 3.0;
const RIBBON_ALPHA: f64 = 0.45;

fn point(c: [f64; 2], r: f64, angle: f64) -> [f64; 2] {
    [
        c[0] + r * (angle - FRAC_PI_2).cos(),
        c[1] + r * (angle - FRAC_PI_2).sin(),
    ]
}

/// A quadratic curve from `from` to `to` pulled towards the centre `c`, as a cubic.
fn join(path: &mut Path, from: [f64; 2], to: [f64; 2], c: [f64; 2]) {
    let k = 2.0 / 3.0;
    path.curve_to(
        [from[0] + k * (c[0] - from[0]), from[1] + k * (c[1] - from[1])],
        [to[0] + k * (c[0] - to[0]), to[1] + k * (c[1] - to[1])],
        to,
    );
}

/// The ribbon between the arcs `a` and `b` (angles in radians clockwise from 12 o'clock) of
/// radius `r`.
fn ribbon(c: [f64; 2], r: f64, a: (f64, f64), b: (f64, f64)) -> Path {
    let mut path = Path::new();
    path.move_to(point(c, r, a.0));
    arc(&mut path, c, r, a.0, a.1);
    join(&mut path, point(c, r, a.1), point(c, r, b.0), c);
    arc(&mut path, c, r, b.0, b.1);
    join(&mut path, point(c, r, b.1), point(c, r, a.0), c);
    path.close();
    path
}

/// The sums and shares a chord diagram reads out, written from the exact values (TYP-36).
#[derive(Serialize, Deserialize)]
struct Texts {
    /// Per node: what leaves, what enters and the share of the circle; empty when there is none.
    nodes: Vec<[String; 3]>,
    /// Per link: the share of the whole.
    links: Vec<String>,
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let rule = Rule {
        values: Values::Flow,
        loops: true,
    };
    let mut graphs = Vec::new();
    let mut left = Left::default();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut g = graph::read(ctx, fi, members, "chord diagram", rule);
        graph::enforce_limits(ctx, &mut g, "chord");
        left.add(std::mem::take(&mut g.left));
        graphs.push(g);
    }
    graph::report(
        ctx,
        &left,
        "chord diagram",
        "a source, a target or a non-negative value",
        "chart.encoding.source",
    );
    let failures: Vec<Option<String>> = graphs.iter().map(|g| g.failure.clone()).collect();
    let ratio = block.style.ratio.unwrap_or(RATIO);
    if let Some(painted) = hierarchy::all_failed(ctx, &failures, ratio) {
        return painted;
    }
    // Colour slots: the nodes, in first-seen order across panels.
    let mut names: Vec<String> = Vec::new();
    for g in graphs.iter().filter(|g| g.failure.is_none()) {
        for n in &g.names {
            if !names.contains(n) {
                names.push(n.clone());
            }
        }
    }
    ctx.set_series(&names);
    let readout = block.readout_format(&block.y, ctx.locale);
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
    let dropped_before = ctx.dropped.len();
    let mut panels = Vec::new();
    let (mut node_count, mut link_count) = (0usize, 0usize);
    for (fi, ((name, _), frame)) in facets.iter().zip(&frames).enumerate() {
        let g = &graphs[fi];
        if let Some(reason) = &g.failure {
            panels.push(super::failed_facet(ctx, *frame, title_room, name, reason));
            continue;
        }
        let n = g.names.len();
        let slots: Vec<usize> = g
            .names
            .iter()
            .map(|x| names.iter().position(|y| y == x).unwrap_or(0))
            .collect();
        let shown: Vec<bool> = slots.iter().map(|&s| !ctx.is_hidden(s)).collect();
        let live: Vec<usize> = (0..g.links.len())
            .filter(|&li| shown[g.links[li].source] && shown[g.links[li].target])
            .collect();
        if live.is_empty() {
            panels.push(super::failed_facet(
                ctx,
                *frame,
                title_room,
                name,
                "Nothing to draw · Every node is hidden",
            ));
            continue;
        }
        node_count += n;
        link_count += g.links.len();
        // What leaves and enters each node, and its share of the circle.
        let sum: f64 = live.iter().map(|&li| g.links[li].value).sum();
        let mut out = vec![0.0; n];
        let mut inn = vec![0.0; n];
        for &li in &live {
            let l = &g.links[li];
            out[l.source] += l.value;
            inn[l.target] += l.value;
        }
        let frac: Vec<f64> = out.iter().zip(&inn).map(|(o, i)| (o / sum + i / sum) / 2.0).collect();
        let order: Vec<usize> = (0..n).filter(|&k| frac[k] > 0.0).collect();
        let texts: Texts = ctx.derive_visible(fi, "chord.texts", || Texts {
            nodes: (0..n)
                .map(|k| {
                    let sum_text = |v: f64| {
                        if v > 0.0 {
                            numbers::value(v, &readout)
                        } else {
                            String::new()
                        }
                    };
                    [sum_text(out[k]), sum_text(inn[k]), share_text(frac[k], ctx.locale)]
                })
                .collect(),
            links: g.links.iter().map(|l| share_text(l.value / sum, ctx.locale)).collect(),
        });
        let gap = (0.025 * TAU).min(TAU / (3.0 * order.len() as f64));
        let avail = TAU - gap * order.len() as f64;
        let mut start = vec![0.0; n];
        let mut size = vec![0.0; n];
        let mut at = gap / 2.0;
        for &k in &order {
            start[k] = at;
            size[k] = frac[k] * avail;
            at += size[k] + gap;
        }
        // The ends of the ribbons along each arc: ribbons to the next nodes round the circle
        // leave from the side that faces them.
        let key = |k: usize, other: usize| if other == k { n } else { (other + n - k) % n };
        let mut ends: Vec<Vec<(usize, usize, bool)>> = vec![Vec::new(); n];
        for &li in &live {
            let l = &g.links[li];
            ends[l.source].push((key(l.source, l.target), li, true));
            ends[l.target].push((key(l.target, l.source), li, false));
        }
        let mut source_arc = vec![(0.0, 0.0); g.links.len()];
        let mut target_arc = vec![(0.0, 0.0); g.links.len()];
        for &k in &order {
            ends[k].sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(b.2.cmp(&a.2)));
            let mut a = start[k];
            for &(_, li, is_source) in &ends[k] {
                let w = g.links[li].value / sum / 2.0 * avail;
                if is_source {
                    source_arc[li] = (a, a + w);
                } else {
                    target_arc[li] = (a, a + w);
                }
                a += w;
            }
        }
        let plot = Rect::new(frame.x, frame.y + title_room, frame.w, frame.h - title_room);
        let c = plot.center();
        let label_w = order
            .iter()
            .map(|&k| ctx.text_width(&g.names[k]))
            .fold(0.0, f64::max)
            .min(plot.w * 0.3);
        let room = (plot.w / 2.0 - label_w - 12.0).min(plot.h / 2.0 - lh - 12.0);
        let r = (room / 1.1).max(plot.w.min(plot.h) * 0.2);
        let r_in = r - (r * 0.07).clamp(5.0, 12.0);
        let r_ribbon = r_in - 2.0;
        let mut ribbons: Vec<Item> = Vec::new();
        let mut arcs: Vec<Item> = Vec::new();
        let mut node_observations: Vec<Observation> = Vec::new();
        let mut link_observations: Vec<Observation> = Vec::new();
        for &li in &live {
            let l = &g.links[li];
            let (a, b) = (source_arc[li], target_arc[li]);
            let path = ribbon(c, r_ribbon, a, b);
            let (ma, mb) = (
                point(c, r_ribbon, (a.0 + a.1) / 2.0),
                point(c, r_ribbon, (b.0 + b.1) / 2.0),
            );
            let here = [
                0.25 * ma[0] + 0.5 * c[0] + 0.25 * mb[0],
                0.25 * ma[1] + 0.5 * c[1] + 0.25 * mb[1],
            ];
            let area = path.bounds().unwrap_or(Rect::new(here[0], here[1], 0.0, 0.0));
            let slot = slots[l.source];
            let hatch = ctx.series.get(slot).and_then(|s| s.hatch);
            ribbons.push(Item::Area {
                path,
                fill: Fill {
                    color: Style::rgba(ctx.series_color(slot)).with_alpha(RIBBON_ALPHA),
                    hatch,
                    hatch_ink: hatch.map(|_| surface),
                    hatch_scale: 1.0,
                },
                role: Role::Mark,
                series: Some(slot),
                obs: Some(order.len() + link_observations.len()),
            });
            link_observations.push(Observation {
                row: Some(l.row),
                dots: vec![],
                series: Some(slot),
                at: here,
                wedge: None,
                area: Some(area),
                radius: 0.0,
                x: here[0],
                readout: Readout {
                    heading: format!("{} → {}", g.names[l.source], g.names[l.target]),
                    lines: vec![
                        ReadoutLine {
                            name: value_name.clone(),
                            value: numbers::value(l.value, &readout),
                            series: Some(slot),
                            names_series: false,
                            derived: false,
                        },
                        ReadoutLine {
                            name: "Share of the whole".into(),
                            value: texts.links[li].clone(),
                            series: None,
                            names_series: false,
                            derived: true,
                        },
                    ],
                }
                .into(),
            });
        }
        for (idx, &k) in order.iter().enumerate() {
            let (a0, a1) = (start[k], start[k] + size[k]);
            let slot = slots[k];
            let hatch = ctx.series.get(slot).and_then(|s| s.hatch);
            arcs.push(Item::Area {
                path: wedge_path(c, r, r_in, a0, a1),
                fill: Fill {
                    color: Style::rgba(ctx.series_color(slot)),
                    hatch,
                    hatch_ink: hatch.map(|_| surface),
                    hatch_scale: 1.0,
                },
                role: Role::Mark,
                series: Some(slot),
                obs: Some(idx),
            });
            let here = point(c, (r + r_in) / 2.0, (a0 + a1) / 2.0);
            let mut lines = Vec::new();
            let [out_text, in_text, share] = &texts.nodes[k];
            for (label, text) in [("Out", out_text), ("In", in_text)] {
                if !text.is_empty() {
                    lines.push(ReadoutLine {
                        name: label.into(),
                        value: text.clone(),
                        series: Some(slot),
                        names_series: false,
                        derived: true,
                    });
                }
            }
            lines.push(ReadoutLine {
                name: "Share of the circle".into(),
                value: share.clone(),
                series: None,
                names_series: false,
                derived: true,
            });
            node_observations.push(Observation {
                row: None,
                dots: vec![],
                series: Some(slot),
                at: here,
                wedge: Some(Wedge {
                    center: c,
                    radius: r,
                    inner: r_in,
                    start: a0,
                    end: a1,
                }),
                area: None,
                radius: 0.0,
                x: here[0],
                readout: Readout {
                    heading: g.names[k].clone(),
                    lines,
                }
                .into(),
            });
        }
        // Names outside the ring, longest arcs first so that the small ones give way.
        let mut texts: Vec<Item> = Vec::new();
        let mut taken: Vec<Rect> = Vec::new();
        let mut by_size = order.clone();
        by_size.sort_by(|&a, &b| size[b].total_cmp(&size[a]).then(a.cmp(&b)));
        for k in by_size {
            let mid = start[k] + size[k] / 2.0;
            let dir = [(mid - FRAC_PI_2).cos(), (mid - FRAC_PI_2).sin()];
            let anchor = if dir[0] > 0.25 {
                Anchor::Start
            } else if dir[0] < -0.25 {
                Anchor::End
            } else {
                Anchor::Middle
            };
            let t = ctx.text_mid(
                &g.names[k],
                c[0] + (r + 6.0) * dir[0],
                c[1] + (r + 6.0) * dir[1] + dir[1] * lh / 2.0,
                anchor,
                Face::Data,
                Role::Label,
                ctx.style.ink,
            );
            let b = t.bounds;
            if b.inside(&plot, 0.0) && taken.iter().all(|o| !o.overlaps(&b, 0.0)) {
                taken.push(b);
                texts.push(Item::Text(t));
            } else {
                ctx.drop_label(&g.names[k], DropReason::Collision, "node", Some(b));
            }
        }
        let mut items = ribbons;
        items.extend(arcs);
        items.extend(texts);
        let mut observations = node_observations;
        observations.extend(link_observations);
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
    ctx.summary = format!(
        "Chord diagram of {} between {}.",
        super::count(link_count, "flow", "flows"),
        super::count(node_count, "node", "nodes")
    );
    ctx.note.push(
        "an arc is as long as the flow into and out of its node; a ribbon is as wide as its flow at both ends".into(),
    );
    // Names that did not fit are still told apart by the key.
    let legend = (ctx.dropped.len() > dropped_before)
        .then(|| kit::legend(ctx, Swatch::Square))
        .flatten();
    Painted {
        height: super::grid_height(facets.len(), columns, panel_h, super::FACET_GAP_Y),
        panels,
        figure_items: vec![],
        legend,
        failure: None,
    }
}
