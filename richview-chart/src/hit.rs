//! Pointer and keyboard targeting on the geometry model (FEA-CH-40, FEA-CH-55): which
//! observation a pointer position or an arrow key selects. Pure, so the WebGL page, the tests and
//! the screen-reader route share one answer.

use crate::geometry::{Geometry, Item, Panel, Targeting, Wedge};
use std::f64::consts::{FRAC_PI_2, TAU};

/// Extra reach around points, in pixels.
pub const POINT_SLACK: f64 = 4.0;
/// Extra reach around filled marks, so thin bars and slivers stay reachable.
pub const AREA_SLACK: f64 = 2.0;

/// An observation in a panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub panel: usize,
    pub observation: usize,
}

fn in_wedge(w: &Wedge, [x, y]: [f64; 2]) -> bool {
    let (dx, dy) = (x - w.center[0], y - w.center[1]);
    let d = dx.hypot(dy);
    if d > w.radius || d < w.inner {
        return false;
    }
    // Angle clockwise from 12 o'clock.
    let a = (dy.atan2(dx) + FRAC_PI_2).rem_euclid(TAU);
    (w.start..=w.end).contains(&a) || (w.end - w.start) >= TAU - 1e-9
}

/// Whether `p` lies inside a filled outline or within `AREA_SLACK` of its boundary:
/// regions of a map and other shapes a box would only approximate.
fn in_outline(panel: &Panel, index: usize, p: [f64; 2], slack: f64) -> Option<bool> {
    let mut any = false;
    for item in &panel.items {
        if let Item::Area { path, obs: Some(k), .. } = item
            && *k == index
        {
            any = true;
            for ring in path.flatten(0.5) {
                let mut inside = false;
                let mut j = ring.len().wrapping_sub(1);
                for i in 0..ring.len() {
                    let (a, b) = (ring[i], ring[j]);
                    let edge = [b[0] - a[0], b[1] - a[1]];
                    let length_squared = edge[0] * edge[0] + edge[1] * edge[1];
                    let t = if length_squared > 0.0 {
                        (((p[0] - a[0]) * edge[0] + (p[1] - a[1]) * edge[1]) / length_squared).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let delta = [a[0] + t * edge[0] - p[0], a[1] + t * edge[1] - p[1]];
                    if delta[0] * delta[0] + delta[1] * delta[1] <= slack * slack {
                        return Some(true);
                    }
                    if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
                        inside = !inside;
                    }
                    j = i;
                }
                if inside {
                    return Some(true);
                }
            }
        }
    }
    any.then_some(false)
}

fn contains(panel: &Panel, index: usize, p: [f64; 2], slack: f64) -> bool {
    let o = &panel.observations[index];
    if let Some(w) = &o.wedge {
        return in_wedge(w, p);
    }
    match o.area {
        // A filled outline decides inside its box; a point mark is reached by radius.
        Some(r) => r.inflate(slack).contains(p) && in_outline(panel, index, p, slack).unwrap_or(true),
        None => {
            o.radius > 0.0
                && (o.at[0] - p[0]).hypot(o.at[1] - p[1]) <= o.radius + if slack > 0.0 { POINT_SLACK } else { 0.0 }
        }
    }
}

/// The observation nearest to `x` along x, the first in order when two are as near, found by
/// bisection: a nearest-x panel's observations are in x order, left to right or right to left
/// (PRF-19).
fn nearest_x(panel: &Panel, x: f64) -> Option<usize> {
    let obs = &panel.observations;
    let rising = obs.first()?.x <= obs.last()?.x;
    debug_assert!(
        obs.windows(2)
            .all(|w| if rising { w[0].x <= w[1].x } else { w[0].x >= w[1].x }),
        "a nearest-x panel's observations are in x order"
    );
    // `before` is true for the observations before `x` in the panel's order.
    let before = |at: f64| if rising { at < x } else { at > x };
    let next = obs.partition_point(|o| before(o.x));
    if next == 0 {
        return Some(0);
    }
    // The first observation at the last x before `x`.
    let last_x = obs[next - 1].x;
    let previous = obs.partition_point(|o| if rising { o.x < last_x } else { o.x > last_x });
    if next == obs.len() || (x - last_x).abs() <= (obs[next].x - x).abs() {
        Some(previous)
    } else {
        Some(next)
    }
}

/// Later-painted marks win an exact-distance tie, so coincident points pick what is visible.
fn mark_order(panel: &Panel, observation: usize) -> usize {
    panel
        .items
        .iter()
        .rposition(|item| match item {
            Item::Rect { obs, role, .. }
            | Item::Line { obs, role, .. }
            | Item::Area { obs, role, .. }
            | Item::Point { obs, role, .. } => *role == crate::geometry::Role::Mark && *obs == Some(observation),
            Item::Text(_) => false,
        })
        .unwrap_or(observation)
}

fn nearest_point(panel: &Panel, p: [f64; 2], point_reach: f64) -> Option<usize> {
    if !panel.plot.inflate(point_reach).contains(p) {
        return None;
    }
    let mut nearest: Option<(usize, f64)> = None;
    for (i, o) in panel.observations.iter().enumerate() {
        if o.radius <= 0.0 {
            continue;
        }
        let reach = o.radius.max(point_reach);
        let dx = o.at[0] - p[0];
        if dx.abs() > reach {
            continue;
        }
        let dy = o.at[1] - p[1];
        if dy.abs() > reach {
            continue;
        }
        let distance = dx.hypot(dy);
        if distance <= reach
            && nearest.is_none_or(|(old_i, old)| {
                distance.total_cmp(&old).is_lt() || (distance == old && mark_order(panel, i) > mark_order(panel, old_i))
            })
        {
            nearest = Some((i, distance));
        }
    }
    nearest.map(|(i, _)| i)
}

/// The observation under a pointer, with at least 24 px reach for point marks.
pub fn at(g: &Geometry, p: [f64; 2]) -> Option<Target> {
    at_with_point_reach(g, p, 24.0)
}

/// The observation under a touch, with at least 32 px reach for point marks.
pub fn at_touch(g: &Geometry, p: [f64; 2]) -> Option<Target> {
    at_with_point_reach(g, p, 32.0)
}

fn at_with_point_reach(g: &Geometry, p: [f64; 2], point_reach: f64) -> Option<Target> {
    for (pi, panel) in g.panels.iter().enumerate() {
        if panel.failure.is_some() || panel.observations.is_empty() {
            continue;
        }
        let found = match panel.targeting {
            Targeting::NearestX => {
                // Points sit on the plot's edges at the first x and the extremes.
                if !panel.plot.inflate(POINT_SLACK).contains(p) {
                    continue;
                }
                nearest_x(panel, p[0])
            }
            Targeting::Radius => nearest_point(panel, p, point_reach),
            Targeting::Contain
                if panel
                    .observations
                    .iter()
                    .all(|o| o.area.is_none() && o.wedge.is_none() && o.radius > 0.0) =>
            {
                nearest_point(panel, p, point_reach)
            }
            // Later marks paint over earlier ones, so the last match is the visible one.
            Targeting::Contain => (0..panel.observations.len())
                .rev()
                .find(|&i| contains(panel, i, p, 0.0))
                .or_else(|| {
                    (0..panel.observations.len())
                        .rev()
                        .find(|&i| contains(panel, i, p, AREA_SLACK))
                }),
        };
        if let Some(observation) = found {
            return Some(Target { panel: pi, observation });
        }
    }
    None
}

/// Arrow-key direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Next,
    Previous,
    First,
    Last,
}

/// Keyboard order: panels in order, observations left to right, then top to bottom. Every
/// observation is in it once, drawn or not, so the keyboard reaches the same rows in every style
/// and width; the pointer reaches only drawn marks.
pub fn order(g: &Geometry) -> Vec<Target> {
    let mut out = Vec::new();
    for (pi, panel) in g.panels.iter().enumerate() {
        if panel.failure.is_some() {
            continue;
        }
        let mut idx: Vec<usize> = (0..panel.observations.len()).collect();
        // A flow chart lists its cards in reading order, rank by rank, then its lines, so a
        // keyboard walks the flow as an ordered list (FEA-CH-76).
        if g.kind != "flow" {
            idx.sort_by(|&a, &b| {
                let (oa, ob) = (&panel.observations[a], &panel.observations[b]);
                oa.at[0].total_cmp(&ob.at[0]).then(oa.at[1].total_cmp(&ob.at[1]))
            });
        }
        // Pie slices keep their clockwise order.
        if panel.observations.iter().all(|o| o.wedge.is_some()) {
            idx.sort_by(|&a, &b| {
                let (wa, wb) = (
                    panel.observations[a].wedge.unwrap_or(Wedge {
                        center: [0.0; 2],
                        radius: 0.0,
                        inner: 0.0,
                        start: 0.0,
                        end: 0.0,
                    }),
                    panel.observations[b].wedge.unwrap_or(Wedge {
                        center: [0.0; 2],
                        radius: 0.0,
                        inner: 0.0,
                        start: 0.0,
                        end: 0.0,
                    }),
                );
                wa.start.total_cmp(&wb.start)
            });
        }
        out.extend(idx.into_iter().map(|observation| Target { panel: pi, observation }));
    }
    out
}

/// The target a key selects from `current`; `None` from nothing selects the first.
pub fn step(g: &Geometry, current: Option<Target>, step: Step) -> Option<Target> {
    let order = order(g);
    let last = order.len().checked_sub(1)?;
    let here = current.and_then(|c| order.iter().position(|t| *t == c));
    let i = match (step, here) {
        (Step::First, _) | (Step::Next | Step::Previous, None) => 0,
        (Step::Last, _) => last,
        (Step::Next, Some(i)) => (i + 1).min(last),
        (Step::Previous, Some(i)) => i.saturating_sub(1),
    };
    order.get(i).copied()
}

/// What a screen reader hears for a target: the readout heading and its lines (FEA-CH-55).
pub fn announce(g: &Geometry, t: Target) -> Option<String> {
    let o = g.panels.get(t.panel)?.observations.get(t.observation)?;
    let mut parts = vec![o.readout.heading.clone()];
    parts.extend(o.readout.lines.iter().map(|l| format!("{}: {}", l.name, l.value)));
    if let Some(facet) = &g.panels[t.panel].facet {
        parts.insert(0, facet.clone());
    }
    Some(parts.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::ChartBlock;
    use crate::geometry::{Path, Rect, Role};
    use crate::layout::{ChartTheme, Options, layout};
    use serde_json::json;

    fn chart(spec: serde_json::Value, rows: &[serde_json::Value]) -> Geometry {
        layout(
            &ChartBlock::from_value(&spec).unwrap(),
            rows,
            &ChartTheme::default(),
            &Options {
                width: 640.0,
                ..Options::default()
            },
        )
    }

    fn outlined(path: Path, anchor: [f64; 2]) -> Geometry {
        let mut g = chart(
            json!({"type":"bar","title":"T","encoding":{"x":"c","y":"v"}}),
            &[json!({"c":"A","v":1})],
        );
        let fill = g.panels[0]
            .items
            .iter()
            .find_map(|i| match i {
                Item::Rect { fill, .. } => Some(*fill),
                _ => None,
            })
            .unwrap();
        let bounds = path.bounds().unwrap();
        g.width = 350.0;
        g.height = 262.5;
        let p = &mut g.panels[0];
        p.plot = Rect::new(0.0, 0.0, g.width, g.height);
        p.frame = p.plot;
        p.targeting = Targeting::Contain;
        p.items = vec![Item::Area {
            path,
            fill,
            role: Role::Mark,
            series: Some(0),
            obs: Some(0),
        }];
        p.observations[0].area = Some(bounds);
        p.observations[0].at = anchor;
        p.observations[0].x = anchor[0];
        g
    }

    #[test]
    fn integer_client_coordinates_reach_the_thin_phone_chord_ribbon() {
        // The real 350px docs chord's Oceania -> Americas ribbon and WebKit event coordinates.
        let mut path = Path::new();
        path.move_to([137.22339084049656, 53.91590208775071]);
        path.curve_to(
            [138.74330934263787, 53.1734434729901],
            [140.28478586119041, 52.47595475770196],
            [141.84581167080745, 51.82434485178186],
        );
        path.curve_to(
            [163.94860389026914, 104.77478161726062],
            [147.19924385981062, 124.16561559331811],
            [91.59773157943184, 109.99684677995431],
        );
        path.curve_to(
            [92.01543944284606, 108.35766491431961],
            [92.48138105875324, 106.73115368602484],
            [92.9949492503831, 105.11943263065275],
        );
        path.curve_to(
            [147.66498308346104, 122.53981087688425],
            [162.40779694683218, 105.47196736258357],
            [137.22339084049656, 53.91590208775071],
        );
        path.close();
        let anchor = [145.4448947017938, 105.72097427635154];
        let g = outlined(path, anchor);
        assert_eq!(
            at(&g, anchor),
            Some(Target {
                panel: 0,
                observation: 0
            })
        );
        let canvas = [20.0, 358.078125];
        let delivered = [
            (anchor[0] + canvas[0]).floor() - canvas[0],
            (anchor[1] + canvas[1]).floor() - canvas[1],
        ];
        assert_eq!(delivered, [145.0, 104.921875]);
        assert_eq!(
            at(&g, delivered),
            Some(Target {
                panel: 0,
                observation: 0
            }),
            "the existing two-pixel filled-mark reach follows the curve"
        );
        assert!(g.panels[0].observations[0].area.unwrap().contains([100.0, 70.0]));
        assert_eq!(
            at(&g, [100.0, 70.0]),
            None,
            "the ribbon's large bounding box is not its hit region"
        );
    }

    #[test]
    fn filled_outline_reach_keeps_concavities_and_the_two_pixel_limit() {
        let mut path = Path::polyline(&[
            [10.0, 10.0],
            [30.0, 10.0],
            [30.0, 30.0],
            [25.0, 30.0],
            [25.0, 15.0],
            [15.0, 15.0],
            [15.0, 30.0],
            [10.0, 30.0],
        ]);
        path.close();
        let g = outlined(path, [12.0, 25.0]);
        assert_eq!(
            at(&g, [16.9, 25.0]),
            Some(Target {
                panel: 0,
                observation: 0
            })
        );
        assert_eq!(
            at(&g, [17.1, 25.0]),
            None,
            "over two pixels from the nearest filled edge"
        );
        assert_eq!(at(&g, [20.0, 25.0]), None, "a concave gap stays empty");
    }

    #[test]
    fn exact_containment_wins_over_later_slack_but_later_exact_marks_win() {
        let mut g = chart(
            json!({"type":"bar","title":"T","encoding":{"x":"c","y":"v"}}),
            &[json!({"c":"A","v":1}), json!({"c":"B","v":2})],
        );
        g.panels[0].observations[0].area = Some(Rect::new(10.0, 10.0, 10.0, 10.0));
        g.panels[0].observations[1].area = Some(Rect::new(21.0, 10.0, 10.0, 10.0));
        assert_eq!(at(&g, [19.0, 15.0]).map(|t| t.observation), Some(0));
        assert_eq!(at(&g, [20.5, 15.0]).map(|t| t.observation), Some(1));
        assert_eq!(at(&g, [22.0, 15.0]).map(|t| t.observation), Some(1));
    }

    #[test]
    fn bars_are_hit_by_containment_and_gaps_miss() {
        let g = chart(
            json!({"type": "bar", "title": "T", "encoding": {"x": "c", "y": "v"}}),
            &[json!({"c": "A", "v": 3}), json!({"c": "B", "v": 5})],
        );
        let o = &g.panels[0].observations[1];
        let r = o.area.unwrap();
        assert_eq!(
            at(&g, r.center()),
            Some(Target {
                panel: 0,
                observation: 1
            })
        );
        let between = [
            (g.panels[0].observations[0].area.unwrap().right() + r.x) / 2.0,
            r.center()[1],
        ];
        assert_eq!(at(&g, between), None);
    }

    #[test]
    fn lines_take_the_nearest_x_anywhere_in_the_plot() {
        let g = chart(
            json!({"type": "line", "title": "T", "encoding": {"x": "m", "y": "v"}}),
            &[
                json!({"m": "2024-01", "v": 1}),
                json!({"m": "2024-02", "v": 2}),
                json!({"m": "2024-03", "v": 3}),
            ],
        );
        let p = &g.panels[0];
        let x = p.observations[2].x - 3.0;
        assert_eq!(at(&g, [x, p.plot.y + 2.0]).map(|t| t.observation), Some(2));
    }

    #[test]
    fn pie_slices_are_hit_by_angle_and_stepped_clockwise() {
        let g = chart(
            json!({"type": "pie", "title": "T", "encoding": {"x": "c", "y": "v"}}),
            &[json!({"c": "A", "v": 1}), json!({"c": "B", "v": 3})],
        );
        let w = g.panels[0].observations[0].wedge.unwrap();
        // A spans the first quarter: up and to the right of the centre.
        let p = [w.center[0] + w.radius * 0.5, w.center[1] - w.radius * 0.5];
        assert_eq!(at(&g, p).map(|t| t.observation), Some(0));
        let q = [w.center[0] - w.radius * 0.5, w.center[1] + w.radius * 0.5];
        assert_eq!(at(&g, q).map(|t| t.observation), Some(1));
        let first = step(&g, None, Step::Next).unwrap();
        assert_eq!(first.observation, 0);
        assert_eq!(step(&g, Some(first), Step::Next).unwrap().observation, 1);
        assert_eq!(
            step(
                &g,
                Some(Target {
                    panel: 0,
                    observation: 1
                }),
                Step::Next
            )
            .unwrap()
            .observation,
            1,
            "stops at the end"
        );
        assert!(announce(&g, first).unwrap().starts_with("A, "));
    }

    #[test]
    fn points_pick_the_nearest_within_twenty_four_pixels() {
        let mut g = chart(
            json!({"type":"scatter","title":"T","encoding":{"x":"x","y":"y"}}),
            &[json!({"x":0,"y":0}), json!({"x":1,"y":1})],
        );
        let panel = &mut g.panels[0];
        panel.plot = Rect::new(0.0, 0.0, 200.0, 200.0);
        panel.observations[0].at = [50.0, 50.0];
        panel.observations[1].at = [80.0, 50.0];
        panel.observations[0].radius = 3.0;
        panel.observations[1].radius = 3.0;
        assert_eq!(at(&g, [70.0, 50.0]).map(|t| t.observation), Some(1));
        assert_eq!(at(&g, [25.9, 50.0]), None);
        assert_eq!(at_touch(&g, [19.0, 50.0]).map(|t| t.observation), Some(0));
        assert_eq!(at_touch(&g, [17.9, 50.0]), None);
    }

    #[test]
    fn undrawn_radius_points_are_skipped_by_pointer_but_reached_by_keyboard() {
        let mut g = chart(
            json!({"type":"scatter","title":"T","encoding":{"x":"x","y":"y"}}),
            &[json!({"x":0,"y":0}), json!({"x":1,"y":1})],
        );
        let panel = &mut g.panels[0];
        panel.plot = Rect::new(0.0, 0.0, 200.0, 200.0);
        panel.observations[0].at = [50.0, 50.0];
        panel.observations[0].radius = 0.0;
        panel.observations[1].at = [100.0, 50.0];
        panel.observations[1].radius = 3.0;
        assert_eq!(at(&g, [50.0, 50.0]), None);
        assert_eq!(step(&g, None, Step::First).map(|t| t.observation), Some(0));
        assert_eq!(step(&g, None, Step::Last).map(|t| t.observation), Some(1));
        g.panels[0].observations[1].radius = 0.0;
        assert_eq!(order(&g).len(), 2);
    }

    #[test]
    fn undrawn_containment_marks_stay_in_the_keyboard_order() {
        let mut g = chart(
            json!({"type":"bar","title":"T","encoding":{"x":"c","y":"v"}}),
            &[json!({"c":"A","v":1}), json!({"c":"B","v":2})],
        );
        let panel = &mut g.panels[0];
        panel.observations[0].area = None;
        panel.observations[0].radius = 0.0;
        let anchor = panel.observations[0].at;
        assert_eq!(at(&g, anchor), None);
        assert_eq!(order(&g).len(), 2);
        assert_eq!(step(&g, None, Step::First).map(|t| t.observation), Some(0));
        assert_eq!(step(&g, None, Step::Last).map(|t| t.observation), Some(1));
    }

    #[test]
    fn coincident_points_choose_the_last_painted_mark() {
        let mut g = chart(
            json!({"type":"scatter","title":"T","encoding":{"x":"x","y":"y"}}),
            &[json!({"x":0,"y":0}), json!({"x":1,"y":1})],
        );
        let panel = &mut g.panels[0];
        panel.plot = Rect::new(0.0, 0.0, 200.0, 200.0);
        for o in &mut panel.observations {
            o.at = [50.0, 50.0];
            o.radius = 3.0;
        }
        assert_eq!(at(&g, [50.0, 50.0]).map(|t| t.observation), Some(1));
    }

    #[test]
    fn point_only_containment_panels_use_point_reach() {
        let mut g = chart(
            json!({"type":"scatter","title":"T","encoding":{"x":"x","y":"y"}}),
            &[json!({"x":0,"y":0}), json!({"x":1,"y":1})],
        );
        let panel = &mut g.panels[0];
        panel.targeting = Targeting::Contain;
        panel.plot = Rect::new(0.0, 0.0, 200.0, 200.0);
        panel.observations[0].at = [50.0, 50.0];
        panel.observations[1].at = [100.0, 50.0];
        panel.observations[0].radius = 3.0;
        panel.observations[1].radius = 3.0;
        assert_eq!(at(&g, [70.0, 50.0]).map(|t| t.observation), Some(0));
        assert_eq!(at_touch(&g, [130.0, 50.0]).map(|t| t.observation), Some(1));
    }

    #[test]
    fn radius_rejection_keeps_the_full_distance_scans_boundaries_and_ties() {
        let rows: Vec<_> = (0..512).map(|i| json!({"x":i,"y":i % 13})).collect();
        let mut g = chart(
            json!({"type":"scatter","title":"Source points","encoding":{"x":"x","y":"y"}}),
            &rows,
        );
        let panel = &mut g.panels[0];
        panel.plot = Rect::new(-50.0, -50.0, 400.0, 400.0);
        for (i, o) in panel.observations.iter_mut().enumerate() {
            o.at = [((i * 17) % 300) as f64, ((i * 31) % 300) as f64];
            o.radius = (i % 9) as f64;
        }
        panel.observations[0].radius = 3.0;
        panel.observations[1].at = panel.observations[0].at;
        panel.observations[1].radius = panel.observations[0].radius;
        // Subtraction may overflow for distant finite positions; hypot remains the
        // reference, and unequal radii must not turn nearest into largest.
        panel.observations[2].at = [f64::MAX, -f64::MAX];
        panel.observations[2].radius = f64::MAX;
        panel.observations[3].at = [f64::MIN_POSITIVE, 0.0];
        panel.observations[3].radius = 0.0;
        let mut positions: Vec<_> = (-2..=32)
            .flat_map(|x| (-2..=32).map(move |y| [x as f64 * 10.0, y as f64 * 10.0]))
            .collect();
        positions.extend([[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [-4.0, 0.0]]);
        for p in positions {
            let expected = g.panels[0]
                .observations
                .iter()
                .enumerate()
                .map(|(i, o)| (i, (o.at[0] - p[0]).hypot(o.at[1] - p[1]), o.radius))
                .filter(|(_, d, r)| *r > 0.0 && *d <= r.max(24.0))
                .min_by(|a, b| {
                    a.1.total_cmp(&b.1)
                        .then_with(|| mark_order(&g.panels[0], b.0).cmp(&mark_order(&g.panels[0], a.0)))
                })
                .map(|(i, ..)| i);
            assert_eq!(at(&g, p).map(|t| t.observation), expected, "pointer {p:?}");
        }
    }

    #[test]
    fn nearest_x_bisection_finds_what_a_scan_finds_in_either_order() {
        // Uneven gaps, several observations at one x, and pointer positions exactly between two
        // observations, where the first in order wins; left to right and right to left.
        let rows: Vec<_> = [0, 1, 3, 4, 9, 10]
            .iter()
            .enumerate()
            .map(|(i, x)| json!({"x": x, "y": i}))
            .collect();
        let mut g = chart(json!({"type":"line","title":"T","encoding":{"x":"x","y":"y"}}), &rows);
        assert_eq!(g.panels[0].targeting, Targeting::NearestX);
        let observations = &mut g.panels[0].observations;
        for k in [4, 1, 1] {
            let twin = observations[k].clone();
            observations.insert(k, twin);
        }
        let mut xs: Vec<f64> = g.panels[0].observations.iter().map(|o| o.x).collect();
        let midpoints: Vec<f64> = xs.windows(2).map(|w| (w[0] + w[1]) / 2.0).collect();
        xs.extend(midpoints);
        let scan = |panel: &Panel, x: f64| {
            panel
                .observations
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| (a.x - x).abs().total_cmp(&(b.x - x).abs()))
                .map(|(i, _)| i)
        };
        for _ in 0..2 {
            let panel = &g.panels[0];
            let (left, right) = (panel.plot.x, panel.plot.right());
            let pointers = (0..=400).map(|k| left - 2.0 + (right - left + 4.0) * k as f64 / 400.0);
            for x in pointers.chain(xs.iter().copied()) {
                assert_eq!(nearest_x(panel, x), scan(panel, x), "x = {x}");
            }
            g.panels[0].observations.reverse();
        }
    }
}
