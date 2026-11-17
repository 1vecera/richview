//! Hexagonal bins: the plane tiled with hexagons, each coloured by the number of observations
//! inside it and printing that number, with a key that decodes the colour only when a hexagon
//! cannot hold its count. The lattice is laid out in a reference plot (600 px wide, of the default
//! shape) and stretched onto the plot, so the hexagons are regular only when the plot has the
//! reference shape and stretch with it otherwise; there are about as many across as the square
//! root of the number of observations. RichView computes the counts, so every readout of them is
//! derived (FEA-CH-42). Empty hexagons are not drawn, the plot grows to hold every drawn hexagon
//! whole and all panels share the colour scale.

use super::kit::{self, ValueSide, XAxis};
use super::sample::{COUNT_ROOM, Count, count_in_cell, decode_counts, disclose_excluded, read_number, shade};
use super::types::Facets;
use super::{Ctx, Painted, Style};
use crate::axis::Zero;
use crate::diag::Level;
use crate::geometry::{Fill, Item, Observation, Path, Readout, ReadoutLine, Rect, Role, Stroke, Targeting};
use crate::numbers::{self, Format, Kind};
use std::collections::BTreeMap;

const RATIO: f64 = 16.0 / 9.0;
/// Width of the reference plot the lattice is laid out in, in pixels.
const REFERENCE_WIDTH: f64 = 600.0;
/// Fewest and most hexagons across the data.
const MIN_ACROSS: usize = 5;
const MAX_ACROSS: usize = 24;
const SQRT3: f64 = 1.732_050_807_568_877_2;

/// The hexagon of a lattice of pointy-top hexagons of circumradius 1 that holds the point
/// `(u, v)`, as axial coordinates `(q, r)`.
fn hex_of(u: f64, v: f64) -> (i64, i64) {
    let q = (SQRT3 / 3.0) * u - v / 3.0;
    let r = 2.0 / 3.0 * v;
    let s = -q - r;
    let (mut rq, mut rr, rs) = (q.round(), r.round(), s.round());
    let (dq, dr, ds) = ((rq - q).abs(), (rr - r).abs(), (rs - s).abs());
    if dq > dr && dq > ds {
        rq = -rr - rs;
    } else if dr > ds {
        rr = -rq - rs;
    }
    (rq as i64, rr as i64)
}

/// The centre of hexagon `(q, r)` of that lattice.
fn centre_of(q: i64, r: i64) -> [f64; 2] {
    [SQRT3 * (q as f64 + r as f64 / 2.0), 1.5 * r as f64]
}

/// The part of the convex polygon `poly` inside `rect` (Sutherland-Hodgman).
fn clip(poly: &[[f64; 2]], rect: &Rect) -> Vec<[f64; 2]> {
    // Each edge is a side of the rectangle: the axis, the bound and which side is inside.
    let edges: [(usize, f64, bool); 4] = [
        (0, rect.x, true),
        (0, rect.right(), false),
        (1, rect.y, true),
        (1, rect.bottom(), false),
    ];
    let mut out: Vec<[f64; 2]> = poly.to_vec();
    for (axis, bound, keep_above) in edges {
        let inside = |p: &[f64; 2]| if keep_above { p[axis] >= bound } else { p[axis] <= bound };
        let input = std::mem::take(&mut out);
        for k in 0..input.len() {
            let (a, b) = (input[k], input[(k + 1) % input.len()]);
            let (ia, ib) = (inside(&a), inside(&b));
            if ia != ib {
                let t = (bound - a[axis]) / (b[axis] - a[axis]);
                let mut p = [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])];
                p[axis] = bound;
                out.push(p);
            }
            if ib {
                out.push(b);
            }
        }
        if out.is_empty() {
            break;
        }
    }
    out
}

/// Whether `rect` lies inside the convex polygon `outline` with every corner at least `room`
/// from every edge. The room is measured to the edges themselves (chart design, 16 Nov 2026): the
/// corners of a box inflated by the room would hit a hexagon's slanted edges long before the
/// digits come near them.
fn holds(outline: &[[f64; 2]], rect: &Rect, room: f64) -> bool {
    let n = outline.len() as f64;
    let centre = [
        outline.iter().map(|p| p[0]).sum::<f64>() / n,
        outline.iter().map(|p| p[1]).sum::<f64>() / n,
    ];
    let corners = [
        [rect.x, rect.y],
        [rect.right(), rect.y],
        [rect.right(), rect.bottom()],
        [rect.x, rect.bottom()],
    ];
    (0..outline.len()).all(|k| {
        let (a, b) = (outline[k], outline[(k + 1) % outline.len()]);
        let length = (b[0] - a[0]).hypot(b[1] - a[1]);
        if length < 1e-9 {
            return true;
        }
        // The distance of a point from the edge's line, positive on the side of the centre.
        let from = |p: &[f64; 2]| ((b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])) / length;
        let inward = from(&centre).signum();
        corners.iter().all(|p| from(p) * inward >= room)
    })
}

/// One hexagon of one panel, as drawn.
struct Hex {
    count: usize,
    /// Its outline on the screen, cut at the plot.
    outline: Vec<[f64; 2]>,
    /// Its centre in the units of the data.
    centre: [f64; 2],
}

/// A panel's frame and the hexagons it holds.
struct Drawn {
    frame: kit::XyFrame,
    hexes: Vec<Hex>,
}

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let (Some(x_field), Some(y_field)) = (block.encoding.x.clone(), block.encoding.y.clone()) else {
        let reason = "Cannot draw this hexbin chart · It needs encoding.x and encoding.y";
        ctx.check(
            "source-error",
            Level::Error,
            "The hexbin chart has no x or no y",
            Some("Give encoding.x and encoding.y."),
            Some("chart.encoding".into()),
            None,
            serde_json::Value::Null,
        );
        return kit::failed(ctx, reason, RATIO);
    };
    let x_title = block.x.label.clone().unwrap_or_else(|| x_field.clone());
    let y_title = block.y.label.clone().unwrap_or_else(|| y_field.clone());
    let x_readout = block.readout_format(&block.x, ctx.locale);
    let y_readout = block.readout_format(&block.y, ctx.locale);
    let y_labels = block.label_format(&block.y, ctx.locale);
    let count_format = Format {
        kind: Kind::Number,
        unit: None,
        digits: numbers::MAX_DIGITS,
        locale: ctx.locale,
    };
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut excluded = 0usize;
    let mut per_panel: Vec<Vec<[f64; 2]>> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut points: Vec<[f64; 2]> = Vec::new();
        for &i in members {
            let x = read_number(ctx, "hexbin chart", &mut failures[fi], i, &x_field);
            let y = read_number(ctx, "hexbin chart", &mut failures[fi], i, &y_field);
            match (x, y) {
                (Ok(Some(x)), Ok(Some(y))) => points.push([x, y]),
                (Ok(_), Ok(_)) => excluded += 1,
                _ => {}
            }
        }
        if points.is_empty() && failures[fi].is_none() {
            failures[fi] = Some("Cannot draw this hexbin chart · No row has both an x and a y".into());
        }
        per_panel.push(points);
    }
    if failures.iter().all(Option::is_some) {
        let reason = failures.iter().flatten().next().cloned().unwrap_or_default();
        return kit::failed(ctx, &reason, RATIO);
    }
    disclose_excluded(ctx, excluded, "an x or a y");
    let good: Vec<&Vec<[f64; 2]>> = per_panel
        .iter()
        .zip(&failures)
        .filter(|(_, f)| f.is_none())
        .map(|(p, _)| p)
        .collect();
    let total: usize = good.iter().map(|p| p.len()).sum();
    let across = ((total as f64).sqrt().ceil() as usize).clamp(MIN_ACROSS, MAX_ACROSS);
    // The data's extent, with about one hexagon of room around it.
    let extent_of = |axis: usize| {
        good.iter()
            .flat_map(|p| p.iter().map(move |q| q[axis]))
            .fold([f64::INFINITY, f64::NEG_INFINITY], |[lo, hi], v| [lo.min(v), hi.max(v)])
    };
    let (xe, ye) = (extent_of(0), extent_of(1));
    let ratio = block.style.ratio.unwrap_or(RATIO);
    // The room a hexagon needs as a share of the plot's width and height.
    let share_x = 1.0 / across as f64;
    let rho = ratio / ((across + 2) as f64 * SQRT3);
    let share_y = rho / (1.0 - 2.0 * rho).max(0.2);
    let pad = |e: [f64; 2], share: f64| {
        let span = e[1] - e[0];
        let room = if span > 0.0 {
            span * share
        } else if e[0] == 0.0 {
            1.0
        } else {
            e[0].abs() * 0.1
        };
        [e[0] - room, e[1] + room]
    };
    // The lattice and its counts jump with the data, so the exact rows decide the extents they
    // sit in and the counts (TYP-36).
    let (x_extent, y_extent): ([f64; 2], [f64; 2]) = ctx.derive(0, "hexbin.extents", || {
        (
            crate::derived::coarse(pad(xe, share_x)),
            crate::derived::coarse(pad(ye, share_y)),
        )
    });
    let finite = |e: [f64; 2]| e[0].is_finite() && e[1].is_finite() && (e[1] - e[0]).is_finite() && e[1] > e[0];
    if !finite(x_extent) || !finite(y_extent) {
        let reason = "Cannot draw this hexbin chart · The values are too extreme to represent";
        ctx.check(
            "source-error",
            Level::Error,
            reason,
            Some("Give finite numbers within a range RichView can represent."),
            Some("chart.encoding".into()),
            None,
            serde_json::Value::Null,
        );
        return kit::failed(ctx, reason, RATIO);
    }
    // The lattice lives in a reference plot, not in the pixels of this one, so the same data bins
    // into the same hexagons at every width (static and hydrated figures agree); the hexagons are
    // regular at the reference shape and stretch with the plot.
    let (pw, ph) = (REFERENCE_WIDTH, REFERENCE_WIDTH / ratio * 0.75);
    let (x_span, y_span) = (x_extent[1] - x_extent[0], y_extent[1] - y_extent[0]);
    let radius = pw / ((across + 2) as f64 * SQRT3);
    let (ox, oy) = (SQRT3 / 2.0 * radius, radius);
    let tallies: Vec<Vec<((i64, i64), usize)>> = (0..facets.len())
        .map(|fi| {
            if failures[fi].is_some() {
                return Vec::new();
            }
            ctx.derive(fi, "hexbin.cells", || {
                let mut tally: BTreeMap<(i64, i64), usize> = BTreeMap::new();
                for p in &per_panel[fi] {
                    let px = (p[0] - x_extent[0]) / x_span * pw;
                    let py = (y_extent[1] - p[1]) / y_span * ph;
                    *tally.entry(hex_of((px - ox) / radius, (py - oy) / radius)).or_insert(0) += 1;
                }
                tally.into_iter().collect()
            })
        })
        .collect();
    // A hexagon at the edge of the data reaches past the room left around it. The plot grows to
    // hold every drawn hexagon whole, so none is cut to a sliver that cannot carry its count.
    let (plot_x, plot_y): ([f64; 2], [f64; 2]) = ctx.derive(0, "hexbin.plot", || {
        let (mut px, mut py) = ([0.0, pw], [0.0, ph]);
        for &((q, r), _) in tallies.iter().flatten() {
            let c = centre_of(q, r);
            let (cx, cy) = (ox + c[0] * radius, oy + c[1] * radius);
            px = [px[0].min(cx - ox), px[1].max(cx + ox)];
            py = [py[0].min(cy - radius), py[1].max(cy + radius)];
        }
        // Reaching past the reference plot by less than a thousandth of a pixel is no reach.
        let x = if px[0] < -1e-3 || px[1] > pw + 1e-3 {
            crate::derived::coarse([x_extent[0] + px[0] / pw * x_span, x_extent[0] + px[1] / pw * x_span])
        } else {
            x_extent
        };
        let y = if py[0] < -1e-3 || py[1] > ph + 1e-3 {
            crate::derived::coarse([y_extent[1] - py[1] / ph * y_span, y_extent[1] - py[0] / ph * y_span])
        } else {
            y_extent
        };
        (x, y)
    });
    let plan = kit::Plan::xy(ctx, facets.len(), ratio);
    let frames = plan.frames(facets.len(), ctx.opts.width);
    let mut drawn: Vec<Option<Drawn>> = Vec::new();
    for ((_, outer), tally) in facets.iter().zip(&frames).zip(&tallies) {
        if tally.is_empty() {
            // A failed panel is drawn in the second pass, in its place.
            drawn.push(None);
            continue;
        }
        ctx.fit.x = Some(plot_x);
        ctx.fit.y = Some(plot_y);
        let side = ValueSide {
            data: plot_y,
            zero: Zero::None,
            title: y_title.clone(),
            format: &y_labels,
            min: Some(plot_y[0]),
            max: Some(plot_y[1]),
        };
        let frame = kit::xy_frame(
            ctx,
            &plan,
            outer,
            &XAxis::Numeric { extent: plot_x },
            Some(&x_title),
            &side,
        );
        let screen = |px: f64, py: f64| {
            [
                frame.x(x_extent[0] + px / pw * x_span),
                frame.y(y_extent[1] - py / ph * y_span),
            ]
        };
        let mut hexes = Vec::new();
        for &((q, r), count) in tally {
            let c = centre_of(q, r);
            let (cx, cy) = (ox + c[0] * radius, oy + c[1] * radius);
            let corners: Vec<[f64; 2]> = (0..6)
                .map(|k| {
                    let a = (60.0 * k as f64 - 30.0).to_radians();
                    screen(cx + radius * a.cos(), cy + radius * a.sin())
                })
                .collect();
            // The plot holds every hexagon, some to the last digit: only one that reaches visibly
            // past it (a panel of another shape than the reference) is cut.
            let outline = clip(&corners, &frame.plot.inflate(0.01));
            if outline.len() < 3 {
                continue;
            }
            hexes.push(Hex {
                count,
                outline,
                centre: [x_extent[0] + cx / pw * x_span, y_extent[1] - cy / ph * y_span],
            });
        }
        drawn.push(Some(Drawn { frame, hexes }));
    }
    let top = drawn
        .iter()
        .flatten()
        .flat_map(|d| d.hexes.iter().map(|h| h.count))
        .max()
        .unwrap_or(0);
    let ramp = ctx.style.ramp;
    let surface = Style::rgba(ctx.style.surface);
    let mut panels = Vec::new();
    let mut counts: Vec<Count> = Vec::new();
    let mut hexagons = 0usize;
    for (((name, _), outer), (fi, d)) in facets.iter().zip(&frames).zip(drawn.into_iter().enumerate()) {
        let Some(Drawn { frame, hexes }) = d else {
            let reason = failures[fi].clone().unwrap_or_default();
            panels.push(super::failed_facet(ctx, *outer, plan.title_room, name, &reason));
            continue;
        };
        let mut marks: Vec<Item> = Vec::new();
        let mut observations: Vec<Observation> = Vec::new();
        for hex in &hexes {
            let index = observations.len();
            let mut path = Path::polyline(&hex.outline);
            path.close();
            let area = path
                .bounds()
                .unwrap_or(Rect::new(hex.outline[0][0], hex.outline[0][1], 0.0, 0.0));
            let n = hex.outline.len() as f64;
            let anchor = [
                hex.outline.iter().map(|p| p[0]).sum::<f64>() / n,
                hex.outline.iter().map(|p| p[1]).sum::<f64>() / n,
            ];
            let fill = shade(&ramp, hex.count, top);
            let text = numbers::value(hex.count as f64, &count_format);
            counts.push(count_in_cell(ctx, [fi, index], &text, anchor, fill, |own| {
                holds(&hex.outline, own, COUNT_ROOM)
            }));
            marks.push(Item::Area {
                path,
                fill: Fill::solid(Style::rgba(fill)),
                role: Role::Mark,
                series: None,
                obs: Some(index),
            });
            observations.push(Observation {
                row: None,
                dots: vec![],
                series: None,
                at: anchor,
                wedge: None,
                area: Some(area),
                radius: 0.0,
                x: anchor[0],
                readout: Readout {
                    heading: format!(
                        "Near {}, {}",
                        numbers::readout_coordinate(hex.centre[0], &x_readout),
                        numbers::readout_coordinate(hex.centre[1], &y_readout)
                    ),
                    lines: vec![ReadoutLine {
                        name: "Observations".into(),
                        value: text,
                        series: None,
                        names_series: false,
                        derived: true,
                    }],
                }
                .into(),
            });
        }
        // Outlines in the surface colour separate neighbours.
        for hex in &hexes {
            let mut path = Path::polyline(&hex.outline);
            path.close();
            marks.push(Item::Line {
                path,
                stroke: Stroke::solid(surface, 0.75),
                role: Role::Grid,
                series: None,
                obs: None,
            });
        }
        hexagons += hexes.len();
        let mut panel = kit::xy_panel(ctx, outer, frame, marks, observations, name);
        panel.targeting = Targeting::Contain;
        panels.push(panel);
    }
    ctx.note.push(format!(
        "hexagons tile the plot with about {across} across; empty hexagons are not drawn"
    ));
    ctx.summary = format!(
        "{} in {}, coloured by count up to {top}.",
        super::count(total, "observation", "observations"),
        super::count(hexagons, "hexagon", "hexagons")
    );
    let mut painted = kit::finish(&plan, facets.len(), panels, None);
    decode_counts(ctx, &mut painted, top, &count_format, counts);
    painted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hexagons_centre_is_in_its_hexagon() {
        for q in -3..=3 {
            for r in -3..=3 {
                let c = centre_of(q, r);
                assert_eq!(hex_of(c[0], c[1]), (q, r));
            }
        }
    }

    #[test]
    fn a_point_near_a_corner_belongs_to_the_closest_centre() {
        let c = centre_of(0, 0);
        assert_eq!(hex_of(c[0] + 0.05, c[1] + 0.05), (0, 0));
        // Just past the right-hand edge, half a width along, is the neighbour.
        assert_eq!(hex_of(SQRT3 * 0.51, 0.0), (1, 0));
    }

    #[test]
    fn a_hexagon_holds_a_box_by_the_room_to_its_edges() {
        // A pointy-top hexagon 26 px wide: its upright edges are 13 px from the centre.
        let r = 26.0 / SQRT3;
        let hexagon: Vec<[f64; 2]> = (0..6)
            .map(|k| {
                let a = (60.0 * f64::from(k) - 30.0).to_radians();
                [r * a.cos(), r * a.sin()]
            })
            .collect();
        // Two digits, 14 px by 12 px: 6 px from the upright edges and clear of the slanted ones.
        assert!(holds(&hexagon, &Rect::new(-7.0, -6.0, 14.0, 12.0), 4.0));
        // Three digits reach within 2.5 px of the upright edges.
        assert!(!holds(&hexagon, &Rect::new(-10.5, -6.0, 21.0, 12.0), 4.0));
        // A box off centre comes too close to a slanted edge.
        assert!(!holds(&hexagon, &Rect::new(-7.0, 0.0, 14.0, 12.0), 4.0));
    }

    #[test]
    fn the_cut_keeps_what_is_inside_and_nothing_else() {
        let square = [[-1.0, -1.0], [3.0, -1.0], [3.0, 3.0], [-1.0, 3.0]];
        let rect = Rect::new(0.0, 0.0, 2.0, 2.0);
        let cut = clip(&square, &rect);
        assert_eq!(cut.len(), 4);
        assert!(
            cut.iter()
                .all(|p| (0.0..=2.0).contains(&p[0]) && (0.0..=2.0).contains(&p[1]))
        );
        assert!(clip(&[[5.0, 5.0], [6.0, 5.0], [6.0, 6.0]], &rect).len() < 3);
    }
}
