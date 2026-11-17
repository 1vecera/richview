//! Where the "Drag to zoom" line stands inside a plot when no place outside it is free
//! (FEA-CH-49): pure geometry, so it is tested without a browser.

use richview_chart::geometry::{Geometry, Item, Rect, Role};

/// How far the hint's paper reaches around its text inside the plot, and how far that paper
/// stays from the plot's edges, in CSS pixels.
pub(super) const PLATE: f64 = 4.0;
const INSET: f64 = 6.0;

/// The hint's place inside the plot: the first of the top right, top left, bottom right and
/// bottom left corners where its paper touches no mark and no chart text of the chart at rest
/// (Chart design, 16 November); `None` when every corner holds data, since no hint is better
/// than one over data. Grid lines, the baseline and ticks may pass under the paper.
pub(super) fn corner(g: &Geometry, plot: Rect, size: [f64; 2]) -> Option<Rect> {
    let room = INSET + PLATE;
    if plot.w < size[0] + 2.0 * room || plot.h < size[1] + 2.0 * room {
        return None;
    }
    let (left, right) = (plot.x + room, plot.right() - room - size[0]);
    let (top, bottom) = (plot.y + room, plot.bottom() - room - size[1]);
    [[right, top], [left, top], [right, bottom], [left, bottom]]
        .into_iter()
        .map(|[x, y]| Rect::new(x, y, size[0], size[1]))
        .find(|rect| {
            let paper = rect.inflate(PLATE);
            !g.items().any(|item| {
                !matches!(item.role(), Role::Grid | Role::Baseline | Role::Tick)
                    && touches(item, &paper)
            })
        })
}

/// Whether `item` is drawn anywhere on `r`: its text or shape box, a line's own run or an
/// area's fill, which can leave the corners of their boxes free.
fn touches(item: &Item, r: &Rect) -> bool {
    let reach = |b: Rect| b.inflate(2.0).overlaps(r, 0.0);
    if !item.bounds().is_some_and(reach) {
        return false;
    }
    let (path, filled) = match item {
        Item::Line { path, .. } => (path, false),
        Item::Area { path, .. } => (path, true),
        _ => return true,
    };
    let near = r.inflate(2.0);
    path.flatten(0.5).iter().any(|run| {
        let n = run.len();
        let ends = if filled { n } else { n.saturating_sub(1) };
        run.iter().any(|p| near.contains(*p))
            || (0..ends).any(|i| segment_meets(run[i], run[(i + 1) % n], &near))
            || (filled && ring_holds(run, r.center()))
    })
}

/// Whether the segment from `a` to `b` passes through `r` (Liang–Barsky).
fn segment_meets(a: [f64; 2], b: [f64; 2], r: &Rect) -> bool {
    let d = [b[0] - a[0], b[1] - a[1]];
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [
        (-d[0], a[0] - r.x),
        (d[0], r.right() - a[0]),
        (-d[1], a[1] - r.y),
        (d[1], r.bottom() - a[1]),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
        } else if p < 0.0 {
            t0 = t0.max(q / p);
        } else {
            t1 = t1.min(q / p);
        }
        if t0 > t1 {
            return false;
        }
    }
    true
}

/// Whether the closed `ring` holds `p` (even-odd).
fn ring_holds(ring: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut inside = false;
    let n = ring.len();
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + n - 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            inside = !inside;
        }
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;
    use richview_chart::block::ChartBlock;
    use richview_chart::layout::{ChartTheme, Options, layout};
    use serde_json::json;

    fn draw(spec: serde_json::Value, rows: &[serde_json::Value]) -> Geometry {
        let block = ChartBlock::from_value(&spec).unwrap();
        let g = layout(
            &block,
            rows,
            &ChartTheme::default(),
            &Options {
                width: 640.0,
                ..Options::default()
            },
        );
        assert!(g.failure.is_none());
        g
    }

    const SIZE: [f64; 2] = [80.0, 16.0];

    #[test]
    fn a_rising_line_leaves_the_top_left_corner_and_a_falling_one_the_top_right() {
        let rows =
            |f: fn(i32) -> i32| -> Vec<_> { (0..30).map(|i| json!({"x": i, "y": f(i)})).collect() };
        let spec = json!({"type": "line", "encoding": {"x": "x", "y": "y"}, "labels": false});
        let rising = draw(spec.clone(), &rows(|i| i));
        let plot = rising.panels[0].plot;
        let place = corner(&rising, plot, SIZE).expect("a free corner");
        assert!(
            place.x < plot.x + plot.w / 2.0 && place.y < plot.y + plot.h / 2.0,
            "{place:?}"
        );
        let falling = draw(spec, &rows(|i| 30 - i));
        let plot = falling.panels[0].plot;
        let place = corner(&falling, plot, SIZE).expect("a free corner");
        assert!(
            place.x > plot.x + plot.w / 2.0 && place.y < plot.y + plot.h / 2.0,
            "{place:?}"
        );
        // The paper stays inside the plot, clear of its edges.
        assert!(place.inflate(PLATE + INSET - 0.01).inside(&plot, 0.0));
    }

    #[test]
    fn a_plot_full_of_marks_takes_no_hint_and_a_small_one_neither() {
        use richview_chart::geometry::{Fill, Rgba};
        let rows: Vec<_> = (0..30).map(|i| json!({"x": i, "y": i})).collect();
        let mut g = draw(
            json!({"type": "line", "encoding": {"x": "x", "y": "y"}, "labels": false}),
            &rows,
        );
        let plot = g.panels[0].plot;
        // Too small a plot for the line and its paper.
        assert_eq!(
            corner(&g, Rect::new(plot.x, plot.y, 90.0, plot.h), SIZE),
            None
        );
        let cover = |role: Role| Item::Rect {
            rect: plot,
            fill: Fill::solid(Rgba([0.0, 0.0, 0.0, 1.0])),
            stroke: None,
            radius: [0.0; 4],
            role,
            series: None,
            obs: None,
        };
        // Grid furniture may pass under the paper; a mark over the whole plot leaves no corner.
        g.panels[0].items.push(cover(Role::Grid));
        assert!(corner(&g, plot, SIZE).is_some());
        g.panels[0].items.push(cover(Role::Mark));
        assert_eq!(corner(&g, plot, SIZE), None);
    }

    #[test]
    fn a_line_counts_where_it_runs_and_an_area_where_it_is_filled() {
        let r = Rect::new(10.0, 10.0, 20.0, 10.0);
        // A segment through the box with both ends outside, and one that passes beside it.
        assert!(segment_meets([0.0, 15.0], [40.0, 15.0], &r));
        assert!(segment_meets([0.0, 0.0], [40.0, 30.0], &r));
        assert!(!segment_meets([0.0, 30.0], [40.0, 30.0], &r));
        assert!(!segment_meets([0.0, 25.0], [12.0, 40.0], &r));
        // A ring around the box holds its middle; a ring beside it does not.
        let around = [[0.0, 0.0], [50.0, 0.0], [50.0, 50.0], [0.0, 50.0]];
        let beside = [[40.0, 0.0], [50.0, 0.0], [50.0, 50.0], [40.0, 50.0]];
        assert!(ring_holds(&around, r.center()));
        assert!(!ring_holds(&beside, r.center()));
    }
}
