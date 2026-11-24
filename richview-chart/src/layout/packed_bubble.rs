//! Packed bubbles: one circle for each named part, as large in area as its value, packed
//! together without overlap and coloured by group. The packing is deterministic: circles go in
//! from the largest to the smallest, each at the first free place along a spiral out of the
//! centre. A name is written inside its circle when it fits and is otherwise reported, never
//! clipped (FEA-CH-26). Equal values are equal areas in every panel and at every size
//! (FEA-CH-42).

use super::graph;
use super::kit;
use super::sample::{disclose_excluded, read_number};
use super::types::Facets;
use super::{Ctx, Painted, Style};
use crate::color;
use crate::diag::Level;
use crate::geometry::{
    Anchor, DropReason, Face, Fill, Item, Observation, Panel, Readout, ReadoutLine, Rect, Role, Shape, Stroke, Swatch,
    Targeting,
};
use crate::limits;
use crate::numbers::{self, Format, Kind};
use crate::rows;
use std::f64::consts::TAU;

const RATIO: f64 = 4.0 / 3.0;
/// A circle with a radius under this share of the largest is too small to see and is not drawn.
const MIN_SHARE: f64 = 1.0 / 400.0;
/// Most steps along the spiral for one circle.
const MAX_STEPS: usize = 200_000;
const SEPARATION: f64 = 1.5;

/// One part: its row, group, value and, once packed, its place in units of the square root of
/// the value.
struct Bubble {
    row: usize,
    name: String,
    group: usize,
    value: f64,
    at: [f64; 2],
    /// Radius in the packing's units, from the shown value.
    radius: f64,
    drawn: bool,
}

/// Places for circles of `radii`, largest first, each at the first free place along a spiral.
fn pack(radii: &[f64]) -> Vec<[f64; 2]> {
    let mut placed: Vec<([f64; 2], f64)> = Vec::with_capacity(radii.len());
    for &r in radii {
        if placed.is_empty() {
            placed.push(([0.0, 0.0], r));
            continue;
        }
        let first = placed[0].1;
        // Nothing can sit nearer the middle than the first circle allows.
        let pitch = r * 0.5;
        let step = r * 0.35;
        let mut rho = first + r;
        let mut theta = TAU * rho / pitch;
        let mut spot = [rho * crate::det::cos(theta), rho * crate::det::sin(theta)];
        for _ in 0..MAX_STEPS {
            spot = [rho * crate::det::cos(theta), rho * crate::det::sin(theta)];
            let free = placed
                .iter()
                .all(|(c, cr)| crate::det::hypot(c[0] - spot[0], c[1] - spot[1]) >= cr + r - 1e-9 * (cr + r));
            if free {
                break;
            }
            theta += step / rho.max(step);
            rho = pitch * theta / TAU;
        }
        placed.push((spot, r));
    }
    placed.into_iter().map(|(c, _)| c).collect()
}

/// A point on the packed circle's edge inside the box reserved for selection.
fn omitted_anchor(room: Rect, center: [f64; 2], radius: f64) -> Option<[f64; 2]> {
    let offsets = |position: f64, low: f64, high: f64| {
        let (low, high) = (low - position, high - position);
        let near = 0.0_f64.clamp(low, high);
        let far = if low.abs() > high.abs() { low } else { high };
        (near, far)
    };
    let (near_x, far_x) = offsets(center[0], room.x, room.right());
    let (near_y, far_y) = offsets(center[1], room.y, room.bottom());
    let square = radius * radius;
    if near_x * near_x + near_y * near_y > square || far_x * far_x + far_y * far_y < square {
        return None;
    }
    // The nearest and farthest points straddle the perimeter. Take the widest chord
    // that fits between them, leaving at least the required vertical clearance.
    let dx = (square - near_y * near_y).max(0.0).sqrt().min(far_x.abs());
    let dy = (square - dx * dx).max(0.0).sqrt();
    Some([center[0] + dx.copysign(far_x), center[1] + dy.copysign(far_y)])
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let (Some(name_field), Some(value_field)) = (block.encoding.label.clone(), block.encoding.y.clone()) else {
        let reason = "Cannot draw this packed bubble chart · It needs encoding.label and encoding.y";
        ctx.check(
            "source-error",
            Level::Error,
            "The packed bubble chart has no label or no y",
            Some("Give encoding.label for the names and encoding.y for the values."),
            Some("chart.encoding".into()),
            None,
            serde_json::Value::Null,
        );
        return kit::failed(ctx, reason, RATIO);
    };
    let multi = block.encoding.color.is_some();
    let groups: Vec<String> = match &block.encoding.color {
        Some(f) => rows::distinct(ctx.rows, f),
        None => vec!["Bubbles".to_owned()],
    };
    ctx.set_series(&groups);
    let value_title = block.y.label.clone().unwrap_or_else(|| value_field.clone());
    let readout = block.readout_format(&block.y, ctx.locale);
    let share_format = Format {
        kind: Kind::Percent,
        unit: None,
        digits: 3,
        locale: ctx.locale,
    };
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut excluded = 0usize;
    let mut per_panel: Vec<Vec<Bubble>> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut bubbles: Vec<Bubble> = Vec::new();
        let skipped = graph::skipped(ctx, fi, members, Some(&value_field));
        for &i in members {
            let Some(name) = rows::text(&ctx.rows[i], &name_field) else {
                excluded += 1;
                continue;
            };
            let group = match &block.encoding.color {
                Some(f) => match rows::text(&ctx.rows[i], f).and_then(|g| groups.iter().position(|x| *x == g)) {
                    Some(g) => g,
                    None => {
                        excluded += 1;
                        continue;
                    }
                },
                None => 0,
            };
            match read_number(ctx, "packed bubble chart", &mut failures[fi], i, &value_field) {
                // Whether a value is positive is the exact rows' call: a small one is zero on the grid.
                Ok(Some(v)) if !skipped.zero.contains(&i) && !skipped.negative.contains(&i) => bubbles.push(Bubble {
                    row: i,
                    name,
                    group,
                    value: v.max(f64::MIN_POSITIVE),
                    at: [0.0, 0.0],
                    radius: 0.0,
                    drawn: false,
                }),
                Ok(_) => excluded += 1,
                Err(()) => {}
            }
        }
        if bubbles.len() > limits::max("packed-bubble", "bubbles per panel") {
            failures[fi] = Some(kit::over_limit(
                ctx,
                "packed-bubble",
                "bubbles per panel",
                bubbles.len(),
                Some("chart.encoding.label"),
            ));
        }
        if bubbles.is_empty() && failures[fi].is_none() {
            failures[fi] = Some("Cannot draw this packed bubble chart · No row has a name and a positive value".into());
        }
        per_panel.push(bubbles);
    }
    if failures.iter().all(Option::is_some) {
        let reason = failures.iter().flatten().next().cloned().unwrap_or_default();
        return kit::failed(ctx, &reason, RATIO);
    }
    disclose_excluded(ctx, excluded, "a name, a group or a positive value");
    // Pack every panel in units of the square root of the value, then share one scale.
    let mut too_small = 0usize;
    for (fi, bubbles) in per_panel.iter_mut().enumerate() {
        if failures[fi].is_some() {
            continue;
        }
        // Sizes, ordering and placement use shown values, so radii and
        // distances between touching circles cannot recover finer source values (TYP-36).
        let spots: Vec<(usize, [f64; 2], f64)> = ctx.derive_projected(fi, "packed-bubble.packing", || {
            let sizes: Vec<f64> = bubbles.iter().map(|b| numbers::shown(b.value, &readout)).collect();
            let top = sizes.iter().map(|v| v.sqrt()).fold(0.0, f64::max);
            let mut order: Vec<usize> = (0..bubbles.len())
                .filter(|&k| sizes[k].sqrt() >= top * MIN_SHARE)
                .collect();
            order.sort_by(|&a, &b| sizes[b].total_cmp(&sizes[a]).then(a.cmp(&b)));
            let radii: Vec<f64> = order.iter().map(|&k| sizes[k].sqrt()).collect();
            order
                .into_iter()
                .zip(pack(&radii))
                .map(|(k, spot)| (k, spot.map(|v| numbers::round_sig(v, 6)), sizes[k]))
                .collect()
        });
        for (k, spot, shown_size) in spots {
            if let Some(b) = bubbles.get_mut(k) {
                b.at = spot;
                b.radius = shown_size.sqrt();
                b.drawn = true;
            }
        }
        too_small += bubbles.iter().filter(|b| !b.drawn && !ctx.is_hidden(b.group)).count();
    }
    let lh = ctx.line_height();
    let width = ctx.opts.width;
    let title_room = if block.encoding.facet.is_some() { lh + 6.0 } else { 0.0 };
    let columns = super::facet_columns(facets.len(), width);
    let pw = (width - 24.0 * (columns as f64 - 1.0)) / columns as f64;
    let ratio = block.style.ratio.unwrap_or(RATIO);
    let panel_h = pw / ratio + title_room;
    let frames = super::grid_frames(facets.len(), width, 0.0, panel_h);
    let margin = SEPARATION + 1.0;
    let plots: Vec<Rect> = frames
        .iter()
        .map(|f| Rect::new(f.x, f.y + title_room, f.w, (f.h - title_room).max(1.0)))
        .collect();
    // The extent of each packing in its units, and the scale that fits the tightest.
    let extent_of = |bubbles: &[Bubble]| -> Option<Rect> {
        bubbles.iter().filter(|b| b.drawn).fold(None, |acc: Option<Rect>, b| {
            let r = b.radius;
            let c = Rect::new(b.at[0] - r, b.at[1] - r, 2.0 * r, 2.0 * r);
            Some(acc.map_or(c, |a| a.union(&c)))
        })
    };
    let mut scale = f64::INFINITY;
    for (fi, bubbles) in per_panel.iter().enumerate() {
        if let (None, Some(e)) = (&failures[fi], extent_of(bubbles))
            && e.w > 0.0
            && e.h > 0.0
        {
            let plot = plots[fi];
            scale = scale.min(((plot.w - 2.0 * margin) / e.w).min((plot.h - 2.0 * margin) / e.h));
        }
    }
    if !scale.is_finite() || scale <= 0.0 {
        let reason = "Nothing to draw · Every part is hidden or too small to draw";
        return kit::failed(ctx, reason, RATIO);
    }
    let surface = Style::rgba(ctx.style.surface);
    let mut panels: Vec<Panel> = Vec::new();
    let mut total_parts = 0usize;
    for (fi, ((name, _), frame)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *frame, title_room, name, reason));
            continue;
        }
        let plot = plots[fi];
        let bubbles = &per_panel[fi];
        let sum: f64 = bubbles
            .iter()
            .filter(|b| !ctx.is_hidden(b.group))
            .map(|b| b.value)
            .sum::<f64>();
        let shares: Vec<String> = ctx.derive_visible(fi, "packed-bubble.shares", || {
            bubbles
                .iter()
                .map(|b| numbers::value(b.value / sum, &share_format))
                .collect()
        });
        let (cx, cy) = match extent_of(bubbles) {
            Some(e) => (
                plot.center()[0] - e.center()[0] * scale,
                plot.center()[1] - e.center()[1] * scale,
            ),
            None => (plot.center()[0], plot.center()[1]),
        };
        let mut items: Vec<Item> = Vec::new();
        let mut labels: Vec<Item> = Vec::new();
        let mut observations: Vec<Observation> = Vec::new();
        let visible: Vec<(&Bubble, &String)> = bubbles
            .iter()
            .zip(&shares)
            .filter(|(b, _)| !ctx.is_hidden(b.group))
            .collect();
        let packed_center = |b: &Bubble| [cx + b.at[0] * scale, cy + b.at[1] * scale];
        let mut anchors = Vec::new();
        if visible.iter().any(|(b, _)| !b.drawn) {
            // The 4 px location marker and its ring and tail reach 18.75 px.
            let room = plot.inflate(-19.0);
            anchors.extend(
                bubbles
                    .iter()
                    .filter(|p| p.drawn)
                    .filter_map(|p| omitted_anchor(room, packed_center(p), p.radius * scale).map(|at| (p, at))),
            );
        }
        for (b, share) in visible {
            total_parts += 1;
            let centre = if b.drawn {
                packed_center(b)
            } else {
                // Sub-threshold parts never enter the packing. Anchor both the readout and the
                // keyboard ring to the largest visible member of their group when possible.
                let parent = anchors.iter().max_by(|(a, _), (c, _)| {
                    let rank = |p: &Bubble| (!ctx.is_hidden(p.group), p.group == b.group);
                    rank(a).cmp(&rank(c)).then_with(|| a.radius.total_cmp(&c.radius))
                });
                // If every packed circle is outside the selection room, its centre is free.
                parent.map_or_else(|| plot.center(), |(_, at)| *at)
            };
            let r = if b.drawn { b.radius * scale } else { 0.0 };
            let base = ctx.series_color(b.group);
            let readout_lines = {
                let mut lines = vec![ReadoutLine {
                    label: None,
                    name: block
                        .y
                        .label
                        .clone()
                        .unwrap_or_else(|| super::cartesian::readable_name(&value_field)),
                    value: numbers::value(b.value, &readout),
                    series: None,
                    names_series: false,
                    derived: false,
                }];
                if multi
                    && b.name != groups[b.group]
                    && let Some(field) = &block.encoding.color
                {
                    lines.insert(
                        0,
                        ReadoutLine {
                            label: None,
                            name: super::cartesian::readable_name(field),
                            value: groups[b.group].clone(),
                            series: Some(b.group),
                            names_series: true,
                            derived: false,
                        },
                    );
                }
                lines.push(ReadoutLine {
                    label: None,
                    name: "Share".into(),
                    value: share.clone(),
                    series: None,
                    names_series: false,
                    derived: true,
                });
                lines
            };
            let index = observations.len();
            observations.push(Observation {
                row: Some(b.row),
                dots: vec![],
                series: Some(b.group),
                at: centre,
                wedge: None,
                area: None,
                radius: r,
                x: centre[0],
                readout: Readout {
                    heading: b.name.clone(),
                    lines: readout_lines,
                }
                .into(),
            });
            if !b.drawn {
                ctx.drop_label(&b.name, DropReason::Density, "bubble", None);
                continue;
            }
            let hatch = ctx.series[b.group].hatch;
            items.push(Item::Point {
                center: centre,
                radius: r,
                shape: Shape::Circle,
                fill: Some(Fill {
                    color: Style::rgba(base).with_alpha(0.86),
                    hatch,
                    hatch_ink: hatch.map(|_| surface),
                    hatch_scale: 1.0,
                }),
                stroke: Some(Stroke::solid(surface, SEPARATION)),
                role: Role::Mark,
                series: Some(b.group),
                obs: Some(index),
            });
            // The name when it fits the chord at the text's height; otherwise reported.
            let ink = if color::contrast(ctx.style.ink, base) >= color::contrast(ctx.style.surface, base) {
                ctx.style.ink
            } else {
                ctx.style.surface
            };
            let available = 2.0 * (r * r - (lh / 2.0).powi(2)).max(0.0).sqrt() - 6.0;
            let text = ctx.text_mid(
                &b.name,
                centre[0],
                centre[1],
                Anchor::Middle,
                Face::Data,
                Role::Label,
                ink,
            );
            if text.bounds.w <= available {
                labels.push(Item::Text(text));
            } else {
                ctx.drop_label(
                    &b.name,
                    DropReason::Density,
                    "bubble",
                    Some(Rect::new(centre[0] - r, centre[1] - r, 2.0 * r, 2.0 * r)),
                );
            }
        }
        items.extend(labels);
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
    if too_small > 0 {
        ctx.note.push(format!(
            "{too_small} parts are too small to draw and are only in the table and the keyboard order"
        ));
    }
    ctx.note
        .push("circle area is proportional to the value, on one scale in every panel".into());
    ctx.summary = format!(
        "Packed bubbles of {} sized by {value_title}{}.",
        super::count(total_parts, "part", "parts"),
        if multi {
            format!(", in {}", super::count(groups.len(), "group", "groups"))
        } else {
            String::new()
        }
    );
    let legend = kit::legend(ctx, Swatch::Dot);
    Painted {
        height: super::grid_height(facets.len(), columns, panel_h, super::FACET_GAP_Y),
        panels,
        figure_items: vec![],
        legend,
        failure: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clear(radii: &[f64], at: &[[f64; 2]]) -> bool {
        (0..radii.len()).all(|i| {
            (i + 1..radii.len())
                .all(|j| crate::det::hypot(at[i][0] - at[j][0], at[i][1] - at[j][1]) >= radii[i] + radii[j] - 1e-6)
        })
    }

    #[test]
    fn circles_never_overlap_and_the_first_is_at_the_centre() {
        let radii: Vec<f64> = (0..40).map(|i| 10.0 - i as f64 * 0.2).collect();
        let at = pack(&radii);
        assert_eq!(at.len(), 40);
        assert_eq!(at[0], [0.0, 0.0]);
        assert!(clear(&radii, &at));
    }

    #[test]
    fn the_packing_is_deterministic_and_compact() {
        let radii = vec![5.0, 4.0, 4.0, 3.0, 3.0, 3.0, 2.0, 2.0, 2.0, 2.0];
        assert_eq!(pack(&radii), pack(&radii));
        let at = pack(&radii);
        let reach = at
            .iter()
            .zip(&radii)
            .map(|(c, r)| crate::det::hypot(c[0], c[1]) + r)
            .fold(0.0, f64::max);
        let area: f64 = radii.iter().map(|r| r * r).sum();
        // The enclosing circle holds the circles with room to spare, but not wastefully.
        assert!(reach * reach < area * 4.0, "reach {reach}, area {area}");
    }

    #[test]
    fn equal_circles_and_one_circle_pack() {
        assert_eq!(pack(&[1.0]), vec![[0.0, 0.0]]);
        let at = pack(&[1.0; 7]);
        assert!(clear(&[1.0; 7], &at));
    }
}
