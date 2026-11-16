//! Check whether a readout touches a pie or ring sector.

use richview_chart::geometry::{Rect, Wedge};
use std::f64::consts::TAU;

/// A conservative intersection: radius and angle must both reach the rectangle.
pub(super) fn touches(w: &Wedge, r: Rect) -> bool {
    let [cx, cy] = w.center;
    let closest = [cx.clamp(r.x, r.right()), cy.clamp(r.y, r.bottom())];
    let min_radius = (closest[0] - cx).hypot(closest[1] - cy);
    let corners = [
        [r.x, r.y],
        [r.right(), r.y],
        [r.right(), r.bottom()],
        [r.x, r.bottom()],
    ];
    let max_radius = corners
        .iter()
        .map(|p| (p[0] - cx).hypot(p[1] - cy))
        .fold(0.0, f64::max);
    if min_radius > w.radius || max_radius < w.inner {
        return false;
    }
    if r.contains(w.center) || w.end - w.start >= TAU {
        return true;
    }
    let in_angle =
        |[x, y]: [f64; 2]| ((x - cx).atan2(cy - y) - w.start).rem_euclid(TAU) <= w.end - w.start;
    if corners.into_iter().any(in_angle) {
        return true;
    }
    // A sector may cross a box even when all four corners lie outside its angles.
    [w.start, w.end].into_iter().any(|a| {
        let direction = [a.sin(), -a.cos()];
        let mut lo: f64 = 0.0;
        let mut hi = w.radius;
        for (origin, delta, min, max) in [
            (cx, direction[0], r.x, r.right()),
            (cy, direction[1], r.y, r.bottom()),
        ] {
            if delta.abs() < 1e-12 {
                if origin < min || origin > max {
                    return false;
                }
            } else {
                let a = (min - origin) / delta;
                let b = (max - origin) / delta;
                lo = lo.max(a.min(b));
                hi = hi.min(a.max(b));
            }
        }
        lo <= hi
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sector_intersection_tracks_the_wedge_not_its_bounding_box() {
        let wedge = Wedge {
            center: [100.0, 100.0],
            radius: 50.0,
            inner: 0.0,
            start: 0.0,
            end: std::f64::consts::FRAC_PI_2,
        };
        assert!(touches(&wedge, Rect::new(110.0, 70.0, 10.0, 10.0)));
        assert!(!touches(&wedge, Rect::new(65.0, 110.0, 10.0, 10.0)));
        assert!(!touches(&wedge, Rect::new(160.0, 70.0, 10.0, 10.0)));
    }
    #[test]
    fn a_sector_edge_crossing_a_box_is_detected_without_a_corner_inside() {
        let wedge = Wedge {
            center: [0.0, 0.0],
            radius: 100.0,
            inner: 0.0,
            start: 0.0,
            end: 0.05,
        };
        assert!(touches(&wedge, Rect::new(-10.0, -80.0, 20.0, 10.0)));
    }
}
