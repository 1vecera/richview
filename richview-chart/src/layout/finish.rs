//! Passes over a painted chart that hold for every type: text stays inside the figure and clear
//! of its bottom edge, hatches get a pitch that suits the mark, and a value label drawn over a
//! hatch sits on a surface-coloured box so it stays legible.

use super::Painted;
use crate::geometry::{Cmd, Fill, Item, Path, Rect, Rgba, Role};

/// Least room between the lowest text and the figure's bottom edge, which is where the caption
/// or the page continues.
pub(crate) const BOTTOM_CLEARANCE: f64 = 12.0;

/// The mark side below which a hatch's pitch starts to grow.
const HATCH_FULL_SIDE: f64 = 40.0;
/// How much coarser than the base pitch the hatch of a mark with no size gets.
const HATCH_MOST: f64 = 2.5;
/// Opacity of the box behind a label drawn over a hatch.
const HALO_OPACITY: f64 = 0.88;
/// How far the box reaches past the label.
const HALO_REACH: f64 = 1.5;

/// A mark's outline as closed contours; curves are sampled.
type Contours = Vec<Vec<[f64; 2]>>;

/// Runs every pass in order.
pub(super) fn finish(out: &mut Painted, width: f64, surface: Rgba, ink: Rgba) {
    for panel in &mut out.panels {
        let shapes = scale_hatches(&mut panel.items);
        if !shapes.is_empty() {
            halo_labels(&mut panel.items, &shapes, surface, ink);
        }
    }
    keep_text_inside(out, width);
}

/// Keeps every horizontal text inside the figure and clear of its bottom edge: a label the
/// painters placed past a side edge (the last tick, an end label) leans inward, and a figure
/// whose lowest text sits closer than [`BOTTOM_CLEARANCE`] to the bottom grows by the
/// difference. A text wider than the figure stays where it was put and the clipping check
/// reports it.
fn keep_text_inside(out: &mut Painted, width: f64) {
    let mut lowest = 0.0f64;
    for item in out
        .panels
        .iter_mut()
        .flat_map(|p| p.items.iter_mut())
        .chain(out.figure_items.iter_mut())
    {
        let Item::Text(t) = item else { continue };
        if t.rotate != 0.0 {
            continue;
        }
        if t.bounds.w <= width {
            let shift = if t.bounds.x < 0.0 {
                -t.bounds.x
            } else if t.bounds.right() > width {
                width - t.bounds.right()
            } else {
                0.0
            };
            t.x += shift;
            t.bounds.x += shift;
        }
        lowest = lowest.max(t.bounds.bottom());
    }
    if out.failure.is_none() && lowest > 0.0 && out.height < lowest + BOTTOM_CLEARANCE {
        out.height = lowest + BOTTOM_CLEARANCE;
    }
}

/// The hatch pitch multiplier for a mark whose shorter side is `side`: 1 from 40 px up, growing
/// to 2.5 as the mark shrinks, so a pie slice or a small bar keeps a few clear stripes.
pub(crate) fn hatch_scale(side: f64) -> f64 {
    1.0 + (HATCH_MOST - 1.0) * ((HATCH_FULL_SIDE - side) / HATCH_FULL_SIDE).clamp(0.0, 1.0)
}

/// Sets every hatched fill's pitch from its mark's size and returns the hatched outlines.
fn scale_hatches(items: &mut [Item]) -> Vec<Contours> {
    let mut shapes = Vec::new();
    for item in items {
        match item {
            Item::Rect { rect, fill, .. } => {
                if fill.hatch.is_none() {
                    continue;
                }
                fill.hatch_scale = hatch_scale(rect.w.min(rect.h));
                shapes.push(vec![vec![
                    [rect.x, rect.y],
                    [rect.right(), rect.y],
                    [rect.right(), rect.bottom()],
                    [rect.x, rect.bottom()],
                ]]);
            }
            Item::Area { path, fill, .. } => {
                if fill.hatch.is_none() {
                    continue;
                }
                let contours = contours(path);
                fill.hatch_scale = hatch_scale(bounds(&contours).map_or(0.0, |b| b.w.min(b.h)));
                shapes.push(contours);
            }
            Item::Point {
                center,
                radius,
                fill: Some(fill),
                ..
            } => {
                if fill.hatch.is_none() {
                    continue;
                }
                let r = *radius;
                fill.hatch_scale = hatch_scale(2.0 * r);
                let ring = (0..16)
                    .map(|k| {
                        let a = f64::from(k) * std::f64::consts::TAU / 16.0;
                        [center[0] + r * a.cos(), center[1] + r * a.sin()]
                    })
                    .collect();
                shapes.push(vec![ring]);
            }
            _ => {}
        }
    }
    shapes
}

/// Puts a surface-coloured box under every value label that lies over a hatched mark.
fn halo_labels(items: &mut Vec<Item>, shapes: &[Contours], surface: Rgba, ink: Rgba) {
    let old = std::mem::take(items);
    for mut item in old {
        if let Item::Text(t) = &mut item
            && t.role == Role::Label
            && t.rotate == 0.0
        {
            let b = t.bounds;
            let probes = [
                b.center(),
                [b.x, b.y],
                [b.right(), b.y],
                [b.x, b.bottom()],
                [b.right(), b.bottom()],
            ];
            if shapes.iter().any(|s| probes.iter().any(|p| contains(s, *p))) {
                items.push(Item::Rect {
                    rect: b.inflate(HALO_REACH),
                    fill: Fill::solid(surface.with_alpha(HALO_OPACITY)),
                    stroke: None,
                    radius: [0.0; 4],
                    role: Role::Annotation,
                    series: None,
                    obs: None,
                });
                // The label now sits on the chart surface, so use that surface's ink rather
                // than the light ink a painter may have selected for the dark mark beneath it.
                t.color = ink;
            }
        }
        items.push(item);
    }
}

fn contours(path: &Path) -> Contours {
    let mut out: Contours = Vec::new();
    for cmd in &path.cmds {
        match cmd {
            Cmd::M { p } => out.push(vec![*p]),
            Cmd::L { p } => {
                if let Some(c) = out.last_mut() {
                    c.push(*p);
                }
            }
            Cmd::C { c1, c2, p } => {
                if let Some(c) = out.last_mut() {
                    let from = c.last().copied().unwrap_or(*p);
                    for k in 1..=4 {
                        let t = f64::from(k) / 4.0;
                        let u = 1.0 - t;
                        let at = |i: usize| {
                            u * u * u * from[i] + 3.0 * u * u * t * c1[i] + 3.0 * u * t * t * c2[i] + t * t * t * p[i]
                        };
                        c.push([at(0), at(1)]);
                    }
                }
            }
            Cmd::Z => {}
        }
    }
    out
}

fn bounds(contours: &Contours) -> Option<Rect> {
    let mut points = contours.iter().flatten();
    let first = points.next()?;
    let (mut x0, mut y0, mut x1, mut y1) = (first[0], first[1], first[0], first[1]);
    for p in points {
        x0 = x0.min(p[0]);
        y0 = y0.min(p[1]);
        x1 = x1.max(p[0]);
        y1 = y1.max(p[1]);
    }
    Some(Rect::new(x0, y0, x1 - x0, y1 - y0))
}

/// Whether `p` lies inside the contours, by the even-odd rule.
fn contains(contours: &Contours, p: [f64; 2]) -> bool {
    let mut inside = false;
    for contour in contours {
        let Some(last) = contour.last() else { continue };
        let mut a = *last;
        for &b in contour {
            if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
                inside = !inside;
            }
            a = b;
        }
    }
    inside
}
