//! Turning the three-dimensional charts: the reader's view replaces the author's, draws what the
//! author asking for it would, every angle draws clean, and the pointer finds the mark drawn on
//! top at every angle, so a readout names the point the reader points at (FEA-CH-75).

use super::common;

use common::*;
use richview_chart::block::ChartBlock;
use richview_chart::geometry::{DropReason, Geometry, Item, Role};
use richview_chart::hit;
use richview_chart::interaction::{ROTATES, rotates};
use richview_chart::layout::{
    ChartTheme, MAX_ELEVATION, MIN_ELEVATION, Options, authored_view, layout, turn_by_degrees, turn_by_pixels,
};
use serde_json::{Value, json};

/// Every type a reader turns, with its golden fixture.
fn fixtures() -> Vec<(&'static str, Value, Vec<Value>)> {
    [
        ("surface", include_str!("fixtures/types/surface.json")),
        ("scatter3d", include_str!("fixtures/types/scatter3d.json")),
        ("bar3d", include_str!("fixtures/types/bar3d.json")),
    ]
    .into_iter()
    .map(|(kind, text)| {
        let v: Value = serde_json::from_str(text).unwrap();
        (kind, v["spec"].clone(), v["rows"].as_array().unwrap().clone())
    })
    .collect()
}

/// `spec` laid out at `width` with the reader's `view` and, when given, the records an earlier
/// layout derived.
fn draw_view(
    spec: &Value,
    rows: &[Value],
    width: f64,
    view: Option<[f64; 2]>,
    derived: &[richview_chart::derived::Derived],
) -> Geometry {
    let block = ChartBlock::from_value(spec).expect("a chart block");
    layout(
        &block,
        rows,
        &ChartTheme::default(),
        &Options {
            width,
            view,
            derived: derived.to_vec(),
            ..Options::default()
        },
    )
}

/// What the page shows of a layout: its panels and figure, without the records it derived.
fn shown(g: &Geometry) -> Value {
    json!({"panels": g.panels, "figure": g.figure_items, "summary": g.summary, "height": g.height})
}

/// The views the tests turn to: round the vertical axis, and from lowest to highest.
fn views() -> Vec<[f64; 2]> {
    let mut all = Vec::new();
    for azimuth in [0.0, 17.5, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 359.0] {
        for elevation in [MIN_ELEVATION, 28.0, 60.0, MAX_ELEVATION] {
            all.push([azimuth, elevation]);
        }
    }
    all
}

#[test]
fn every_three_dimensional_type_turns_and_only_those() {
    for (kind, ..) in fixtures() {
        assert!(rotates(kind), "{kind} turns");
        assert!(ROTATES.contains(&kind));
    }
    assert_eq!(ROTATES.len(), fixtures().len());
    for flat in ["bar", "line", "scatter", "heatmap", "contour", "map"] {
        assert!(!rotates(flat), "{flat} stays as drawn");
    }
}

/// What the page says of a layout, whatever its size and place: the axis titles, the readout of
/// each mark and the summary. Tick labels are left out, since how many fit follows the size.
fn said(g: &Geometry) -> Value {
    let readouts: Vec<Value> = observations(g)
        .iter()
        .map(|o| json!({"heading": o.readout.heading, "lines": o.readout.lines.len()}))
        .collect();
    json!({"titles": axis_titles(g), "readouts": readouts, "summary": g.summary})
}

#[test]
fn a_view_the_reader_turns_to_is_drawn_as_the_same_view_the_author_asked_for() {
    // The box has the size the author's own view fits, so a turned view and the view the author
    // asks for are the same drawing at their own sizes.
    for (kind, spec, rows) in fixtures() {
        for view in [[0.0, 5.0], [90.0, 45.0], [200.0, 60.0], [359.5, 85.0], [-30.0, 28.0]] {
            let mut author = spec.clone();
            author["style"] = json!({"azimuth": view[0], "elevation": view[1]});
            let asked = draw(&author, &rows);
            let turned = draw_view(&spec, &rows, 640.0, Some(view), &[]);
            assert_eq!(said(&asked), said(&turned), "{kind} at {view:?}");
        }
    }
}

#[test]
fn without_a_reader_view_the_authors_stays_and_is_where_turning_starts() {
    for (kind, mut spec, rows) in fixtures() {
        spec["style"] = json!({"azimuth": 300, "elevation": 61});
        let block = ChartBlock::from_value(&spec).unwrap();
        assert_eq!(authored_view(&block), [300.0, 61.0], "{kind}");
        let plain = draw(&spec, &rows);
        let same = draw_view(&spec, &rows, 640.0, Some(authored_view(&block)), &[]);
        assert_eq!(shown(&plain), shown(&same), "{kind}: the start is the author's view");
        let elsewhere = draw_view(&spec, &rows, 640.0, Some([120.0, 40.0]), &[]);
        assert_ne!(shown(&plain), shown(&elsewhere), "{kind}: a turn changes the picture");
        // Angles out of range are wrapped and held where the projection holds them.
        let mut bare = spec.clone();
        bare["style"] = json!({"azimuth": 725, "elevation": 200});
        let bare = ChartBlock::from_value(&bare).unwrap();
        assert_eq!(authored_view(&bare), [5.0, MAX_ELEVATION], "{kind}");
    }
}

#[test]
fn angles_past_the_limits_draw_as_the_limits_do() {
    for (kind, spec, rows) in fixtures() {
        let held = draw_view(&spec, &rows, 640.0, Some([10.0, MAX_ELEVATION]), &[]);
        for view in [[10.0, 200.0], [370.0, 1e9], [-350.0, MAX_ELEVATION + 0.001]] {
            let g = draw_view(&spec, &rows, 640.0, Some(view), &[]);
            assert_eq!(shown(&held), shown(&g), "{kind} at {view:?}");
        }
        let low = draw_view(&spec, &rows, 640.0, Some([10.0, MIN_ELEVATION]), &[]);
        let below = draw_view(&spec, &rows, 640.0, Some([10.0, -40.0]), &[]);
        assert_eq!(shown(&low), shown(&below), "{kind}");
        // A view that is not a number is dropped for the author's.
        let none = draw_view(&spec, &rows, 640.0, None, &[]);
        let nan = draw_view(&spec, &rows, 640.0, Some([f64::NAN, 30.0]), &[]);
        assert_eq!(shown(&none), shown(&nan), "{kind}");
    }
}

#[test]
fn every_angle_draws_clean_at_every_width() {
    for (kind, spec, rows) in fixtures() {
        for view in views() {
            for width in WIDTHS {
                let g = draw_view(&spec, &rows, width, Some(view), &[]);
                let at = format!("{kind} at {view:?} and {width} px");
                assert!(g.failure.is_none(), "{at} failed: {:?}", g.failure);
                assert!(nonfinite(&g).is_none(), "{at}: {:?}", nonfinite(&g));
                let figure = richview_chart::geometry::Rect::new(0.0, 0.0, g.width, g.height);
                for item in g.items() {
                    if let Some(b) = item.bounds() {
                        assert!(b.inside(&figure, 1.0), "{at}: {item:?} paints outside the figure");
                    }
                }
                for check_kind in ["clipping", "label-collision"] {
                    assert!(check(&g, check_kind).is_none(), "{at}: {check_kind}");
                }
                assert_eq!(hit::order(&g).len(), observations(&g).len(), "{at}: keyboard order");
            }
        }
    }
}

/// The cube's edge on the page, in pixels, read from its longest vertical edge: a vertical edge
/// is the cube's edge times the cosine of the elevation.
fn cube_edge(g: &Geometry, elevation: f64) -> f64 {
    let longest = g
        .items()
        .filter_map(|item| match item {
            Item::Line {
                path,
                role: Role::Baseline,
                ..
            } => path.bounds(),
            _ => None,
        })
        .filter(|b| b.w < 1e-6)
        .map(|b| b.h)
        .fold(0.0, f64::max);
    longest / elevation.to_radians().cos()
}

#[test]
fn the_box_keeps_one_size_at_every_angle() {
    // The smallest the cube may be at each width: a fit that shrinks it to nothing fails here.
    let least = [(320.0, 20.0), (390.0, 30.0), (1440.0, 80.0)];
    for (kind, spec, rows) in fixtures() {
        for (width, least) in least {
            let mut sizes = Vec::new();
            // Turned square to the viewer, floor edges seen end-on stand upright too.
            for view in views().into_iter().filter(|v| v[0] % 90.0 != 0.0) {
                let g = draw_view(&spec, &rows, width, Some(view), &[]);
                sizes.push((view, cube_edge(&g, view[1])));
            }
            let first = sizes[0].1;
            assert!(
                first >= least,
                "{kind} at {width} px: the cube is {first} px, under {least}"
            );
            for (view, edge) in &sizes {
                assert!(
                    (edge - first).abs() < 1e-6 * first,
                    "{kind} at {width} px: the cube is {edge} px at {view:?} and {first} px at {:?}",
                    sizes[0].0
                );
            }
        }
    }
}

#[test]
fn no_label_leaves_the_plot_at_any_angle() {
    for (kind, spec, rows) in fixtures() {
        let mut views = views();
        // Between the angles the fit tries, too.
        for azimuth in (0..360).step_by(11) {
            for elevation in [5.0, 20.0, 50.0, 71.0, 85.0] {
                views.push([f64::from(azimuth), elevation]);
            }
        }
        for view in views {
            for width in WIDTHS {
                let g = draw_view(&spec, &rows, width, Some(view), &[]);
                let out: Vec<_> = g
                    .dropped
                    .iter()
                    .filter(|d| d.reason == DropReason::OutOfBounds)
                    .map(|d| &d.text)
                    .collect();
                assert!(
                    out.is_empty(),
                    "{kind} at {view:?} and {width} px: {out:?} left the plot"
                );
            }
        }
    }
}

/// How far the vertical axis's tick labels stand from the axis: its x less the right end the
/// labels level with it share (the tick labels of one axis end at one x).
fn z_label_gap(g: &Geometry) -> f64 {
    let axis = g
        .items()
        .filter_map(|item| match item {
            Item::Line {
                path,
                role: Role::Baseline,
                ..
            } => path.bounds(),
            _ => None,
        })
        .filter(|b| b.w < 1e-6)
        .min_by(|a, b| a.x.total_cmp(&b.x))
        .expect("the vertical axis");
    let mut ends: Vec<f64> = g
        .items()
        .filter_map(|item| match item {
            Item::Text(t) if t.role == Role::AxisLabel => Some(t.bounds),
            _ => None,
        })
        .filter(|b| {
            let middle = (b.y + b.bottom()) / 2.0;
            b.right() <= axis.x + 1e-6 && middle >= axis.y - 1.0 && middle <= axis.bottom() + 1.0
        })
        .map(|b| b.right())
        .collect();
    ends.sort_by(f64::total_cmp);
    // The nearest right end that two labels share.
    ends.windows(2)
        .rev()
        .find(|pair| (pair[1] - pair[0]).abs() < 1e-6)
        .map_or(f64::INFINITY, |pair| axis.x - pair[1])
}

#[test]
fn the_vertical_axis_labels_stand_further_out_as_the_view_rises() {
    for (kind, spec, rows) in fixtures() {
        let gap = |elevation: f64| z_label_gap(&draw_view(&spec, &rows, 640.0, Some([35.0, elevation]), &[]));
        let (low, mid, high) = (gap(28.0), gap(60.0), gap(MAX_ELEVATION));
        assert!(low.is_finite() && high.is_finite(), "{kind}: the axis has labels");
        assert!(low >= 6.5, "{kind}: {low} px from the axis at 28 degrees");
        assert!(
            mid > low + 2.0 && high > mid,
            "{kind}: {low}, {mid}, {high} px at 28, 60 and 85 degrees"
        );
        assert!(high >= low + 5.0, "{kind}: {high} px at 85 degrees against {low}");
    }
}

/// The texts of the axis titles a layout draws.
fn axis_titles(g: &Geometry) -> Vec<String> {
    let mut titles: Vec<String> = g
        .items()
        .filter_map(|item| match item {
            Item::Text(t) if t.role == Role::AxisTitle => Some(t.text.clone()),
            _ => None,
        })
        .collect();
    titles.sort();
    titles
}

#[test]
fn every_axis_title_stays_at_every_angle() {
    for (kind, spec, rows) in fixtures() {
        for width in WIDTHS {
            let author = axis_titles(&draw_view(&spec, &rows, width, None, &[]));
            assert_eq!(author.len(), 3, "{kind} at {width} px: {author:?}");
            for view in views() {
                let g = draw_view(&spec, &rows, width, Some(view), &[]);
                assert_eq!(axis_titles(&g), author, "{kind} at {view:?} and {width} px");
            }
        }
    }
}

/// The distance between two page points.
fn apart(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

#[test]
fn the_pointer_names_the_mark_drawn_on_top_at_every_angle() {
    for (kind, spec, rows) in fixtures() {
        for view in views() {
            let g = draw_view(&spec, &rows, 640.0, Some(view), &[]);
            let obs = observations(&g);
            assert!(!obs.is_empty());
            for (k, o) in obs.iter().enumerate() {
                let at = format!("{kind} at {view:?}, observation {k} ({})", o.readout.heading);
                let target = hit::at(&g, o.at).unwrap_or_else(|| panic!("{at}: not reachable"));
                let found = &g.panels[target.panel].observations[target.observation];
                if o.area.is_some() {
                    // Filled marks are painted in observation order, so the pointer takes the
                    // last that holds the point: the one on top, never one behind this one.
                    assert!(target.observation >= k, "{at}: found {} behind it", target.observation);
                    assert!(
                        found.area.is_some_and(|a| a.inflate(hit::AREA_SLACK).contains(o.at)),
                        "{at}: found a mark that does not hold the point"
                    );
                } else {
                    // Points are found by their nearest centre.
                    for other in &obs {
                        assert!(
                            apart(found.at, o.at) <= apart(other.at, o.at) + 1e-9,
                            "{at}: {} is nearer than the one found",
                            other.readout.heading
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn a_dot_is_found_at_its_own_centre_unless_another_covers_it() {
    let (_, spec, rows) = fixtures().into_iter().find(|(k, ..)| *k == "scatter3d").unwrap();
    for view in views() {
        let g = draw_view(&spec, &rows, 640.0, Some(view), &[]);
        let obs = observations(&g);
        let mut alone = 0;
        for o in &obs {
            let found = hit::at(&g, o.at)
                .map(|t| &g.panels[t.panel].observations[t.observation])
                .unwrap();
            if apart(found.at, o.at) < 1e-9 {
                alone += 1;
            }
        }
        // Dots that sit exactly on one another are the only ones another can take.
        assert!(
            alone * 10 >= obs.len() * 9,
            "{view:?}: only {alone} of {} found at their centre",
            obs.len()
        );
    }
}

#[test]
fn every_readout_stays_with_its_mark_at_every_angle() {
    // What a readout says belongs to its row or cell, not to its place in the drawing order: the
    // same readouts come back at every angle, and a row's own readout is the same at all of them.
    for (kind, spec, rows) in fixtures() {
        let canon = |g: &Geometry| -> Vec<String> {
            let mut all: Vec<String> = observations(g)
                .iter()
                .map(|o| {
                    let lines: Vec<String> = o
                        .readout
                        .lines
                        .iter()
                        .map(|l| format!("{}={}", l.name, l.value))
                        .collect();
                    format!("{} | {}", o.readout.heading, lines.join("; "))
                })
                .collect();
            all.sort();
            all
        };
        let first = draw_view(&spec, &rows, 640.0, Some([0.0, 28.0]), &[]);
        let reference = canon(&first);
        for view in views() {
            let g = draw_view(&spec, &rows, 640.0, Some(view), &[]);
            assert_eq!(reference, canon(&g), "{kind} at {view:?}");
        }
    }
}

#[test]
fn a_dot_keeps_its_row_and_its_readout_when_the_view_turns() {
    let (_, spec, rows) = fixtures().into_iter().find(|(k, ..)| *k == "scatter3d").unwrap();
    let rows_of = |g: &Geometry| -> Vec<(Option<usize>, String)> {
        observations(g)
            .iter()
            .map(|o| (o.row, o.readout.heading.clone()))
            .collect()
    };
    let first = rows_of(&draw_view(&spec, &rows, 640.0, Some([0.0, 28.0]), &[]));
    for view in views() {
        assert_eq!(
            first,
            rows_of(&draw_view(&spec, &rows, 640.0, Some(view), &[])),
            "{view:?}"
        );
    }
}

#[test]
fn the_depth_order_the_author_recorded_is_not_replayed_in_a_turned_view() {
    let (_, spec, rows) = fixtures().into_iter().find(|(k, ..)| *k == "scatter3d").unwrap();
    let author = draw(&spec, &rows);
    assert!(
        author.derived.iter().any(|d| d.key == "scatter3d.depth"),
        "the author's layout records the order it painted"
    );
    let turned = [200.0, 60.0];
    let replayed = draw_view(&spec, &rows, 640.0, Some(turned), &author.derived);
    let fresh = draw_view(&spec, &rows, 640.0, Some(turned), &[]);
    assert_eq!(shown(&replayed), shown(&fresh));
    assert!(
        replayed.derived.iter().all(|d| d.key != "scatter3d.depth"),
        "a turned view records no order"
    );
    assert_ne!(shown(&author), shown(&fresh));
}

#[test]
fn the_records_that_do_not_depend_on_the_view_are_replayed_when_it_turns() {
    for (kind, spec, rows) in fixtures() {
        let author = draw(&spec, &rows);
        let turned = [140.0, 50.0];
        let with = draw_view(&spec, &rows, 640.0, Some(turned), &author.derived);
        let without = draw_view(&spec, &rows, 640.0, Some(turned), &[]);
        assert_eq!(shown(&with), shown(&without), "{kind}");
    }
}

#[test]
fn turning_by_the_pointer_follows_it_and_keeps_the_elevation_in_range() {
    // Right and down, as dragged: the near side goes right and the view looks down from higher.
    let [az, el] = turn_by_pixels([35.0, 28.0], [10.0, 0.0]);
    assert!((az - 40.0).abs() < 1e-9 && (el - 28.0).abs() < 1e-9, "{az} {el}");
    let [az, el] = turn_by_pixels([35.0, 28.0], [0.0, 10.0]);
    assert!((az - 35.0).abs() < 1e-9 && (el - 33.0).abs() < 1e-9, "{az} {el}");
    // The elevation stops at its limits however far the pointer goes; the azimuth goes round.
    assert_eq!(turn_by_pixels([35.0, 28.0], [0.0, 1e6])[1], MAX_ELEVATION);
    assert_eq!(turn_by_pixels([35.0, 28.0], [0.0, -1e6])[1], MIN_ELEVATION);
    let [az, _] = turn_by_pixels([350.0, 28.0], [40.0, 0.0]);
    assert!((az - 10.0).abs() < 1e-9, "{az}");
    let [az, _] = turn_by_pixels([10.0, 28.0], [-40.0, 0.0]);
    assert!((az - 350.0).abs() < 1e-9, "{az}");
    // A drag that comes back returns to where it started.
    let there = turn_by_pixels([35.0, 28.0], [63.0, -21.0]);
    let back = turn_by_pixels(there, [-63.0, 21.0]);
    assert!(
        (back[0] - 35.0).abs() < 1e-9 && (back[1] - 28.0).abs() < 1e-9,
        "{back:?}"
    );
    // A key turns by whole degrees the same way.
    assert_eq!(turn_by_degrees([35.0, 28.0], [5.0, -5.0]), [40.0, 23.0]);
}
