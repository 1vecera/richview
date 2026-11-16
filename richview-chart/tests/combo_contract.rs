//! Combo charts: bars and a line on two axes, each at its true coordinates, gaps in the line,
//! readouts of both values and the shared contract (FEA-CH-01, FEA-CH-75).

mod common;

use common::*;
use richview_chart::geometry::{Geometry, Item, Role};
use serde_json::{Value, json};

fn spec() -> Value {
    json!({
        "type":"combo","title":"Revenue and margin",
        "encoding":{"x":"quarter","y":"revenue","line":"margin"},
        "axes":{"y":{"label":"Revenue","unit":"$"},"y2":{"label":"Margin","unit":"%"}}
    })
}

fn rows() -> Vec<Value> {
    vec![
        json!({"quarter":"Q1","revenue":100,"margin":10}),
        json!({"quarter":"Q2","revenue":150,"margin":20}),
        json!({"quarter":"Q3","revenue":125,"margin":15}),
        json!({"quarter":"Q4","revenue":200,"margin":30}),
    ]
}

/// The points of the line, and the line items themselves (the halo and the line of each run).
fn points(g: &Geometry) -> Vec<[f64; 2]> {
    g.items()
        .filter_map(|i| match i {
            Item::Point { center, .. } => Some(*center),
            _ => None,
        })
        .collect()
}

fn runs(g: &Geometry) -> usize {
    // Each run is drawn as a halo and a line.
    g.items()
        .filter(|i| matches!(i, Item::Line { role: Role::Mark, .. }))
        .count()
        / 2
}

fn texts(g: &Geometry, role: Role) -> Vec<String> {
    g.items()
        .filter_map(|i| match i {
            Item::Text(t) if t.role == role => Some(t.text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_golden_combo_meets_the_shared_contract() {
    contract(&spec(), &rows());
}

#[test]
fn bars_and_line_points_stand_at_their_values_on_their_own_axes() {
    let g = draw(&spec(), &rows());
    let obs = observations(&g);
    assert_eq!(obs.len(), 4);
    let bars: Vec<_> = g
        .items()
        .filter_map(|i| match i {
            Item::Rect {
                rect, role: Role::Mark, ..
            } => Some(*rect),
            _ => None,
        })
        .collect();
    assert_eq!(bars.len(), 4);
    // Bars start at zero and are proportional to the revenue.
    assert!((bars[3].h / bars[0].h - 2.0).abs() < 1e-6);
    assert!((bars[1].h / bars[0].h - 1.5).abs() < 1e-6);
    let pts = points(&g);
    assert_eq!(pts.len(), 4);
    // The line has its own scale: equal steps of value are equal steps of height, and its
    // points are centred over their bars.
    let step = pts[0][1] - pts[1][1];
    assert!(step > 0.0);
    assert!(
        ((pts[1][1] - pts[3][1]) - step).abs() < 1e-6,
        "10 -> 20 -> 30 is linear"
    );
    assert!(((pts[2][1] - pts[1][1]) - step / 2.0).abs() < 1e-6, "15 is halfway");
    for (p, b) in pts.iter().zip(&bars) {
        assert!((p[0] - b.center()[0]).abs() < 1e-6);
    }
    // The right axis has its own labels and title; its unit shows.
    let labels = texts(&g, Role::AxisLabel);
    assert!(labels.iter().any(|l| l.contains('%')), "{labels:?}");
    assert!(labels.iter().any(|l| l.contains('$')), "{labels:?}");
    let titles = texts(&g, Role::AxisTitle);
    assert!(
        titles.contains(&"Revenue".to_string()) && titles.contains(&"Margin".to_string()),
        "{titles:?}"
    );
    assert_eq!(runs(&g), 1);
}

#[test]
fn the_second_axis_takes_its_bounds_from_the_author() {
    let mut s = spec();
    s["axes"]["y2"]["min"] = json!(0);
    s["axes"]["y2"]["max"] = json!(100);
    let g = draw(&s, &rows());
    let plot = g.panels[0].plot;
    let pts = points(&g);
    // 30 on an axis from 0 to 100 stands 30 % up the plot.
    assert!((pts[3][1] - (plot.bottom() - 0.3 * plot.h)).abs() < 1e-6);
}

#[test]
fn readouts_give_both_values_as_given() {
    let g = draw(&spec(), &rows());
    let obs = observations(&g);
    assert_eq!(obs[1].readout.heading, "Q2");
    assert_eq!(line(obs[1], "Revenue"), Some("$150"));
    assert_eq!(line(obs[1], "Margin"), Some("20%"));
    assert!(obs[1].readout.lines.iter().all(|l| !l.derived));
}

#[test]
fn a_missing_line_value_breaks_the_line_and_is_disclosed() {
    let mut r = rows();
    r[2] = json!({"quarter":"Q3","revenue":125,"margin":null});
    let g = draw(&spec(), &r);
    assert!(check(&g, "excluded-observations").is_some());
    assert_eq!(points(&g).len(), 3);
    // Q1-Q2 is one run; Q4 stands alone as a point.
    assert_eq!(runs(&g), 1);
    assert_eq!(line(observations(&g)[2], "Margin"), None);
    assert_eq!(line(observations(&g)[2], "Revenue"), Some("$125"));
    // Bridged, the line runs across the gap.
    let mut s = spec();
    s["style"] = json!({"gaps":"bridge"});
    let g = draw(&s, &r);
    assert_eq!(runs(&g), 1);
    let line_item = g.items().find_map(|i| match i {
        Item::Line {
            path, role: Role::Mark, ..
        } => Some(path.cmds.len()),
        _ => None,
    });
    assert_eq!(line_item, Some(3), "three points joined");
    // An interior gap of two leaves two runs.
    let mut r = rows();
    r[1] = json!({"quarter":"Q2","revenue":150,"margin":null});
    let g = draw(&spec(), &r);
    assert_eq!(runs(&g), 1, "Q1 stands alone and Q3-Q4 is a run of two");
    assert_eq!(points(&g).len(), 3);
}

#[test]
fn a_missing_bar_keeps_its_line_point_and_text_fails_the_panel() {
    let mut r = rows();
    r[1] = json!({"quarter":"Q2","revenue":null,"margin":20});
    let g = draw(&spec(), &r);
    assert!(check(&g, "excluded-observations").is_some());
    assert_eq!(observations(&g).len(), 4);
    assert_eq!(line(observations(&g)[1], "Revenue"), None);
    assert_eq!(line(observations(&g)[1], "Margin"), Some("20%"));
    assert_eq!(points(&g).len(), 4);
    r[1] = json!({"quarter":"Q2","revenue":"lots","margin":20});
    let g = draw(&spec(), &r);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").is_some());
    r[1] = json!({"quarter":"Q2","revenue":150,"margin":"1e-999"});
    let g = draw(&spec(), &r);
    assert!(g.failure.is_some());
    assert!(check(&g, "source-error").is_some());
}

#[test]
fn a_combo_without_a_line_channel_says_so() {
    let mut s = spec();
    s["encoding"] = json!({"x":"quarter","y":"revenue"});
    let g = draw(&s, &rows());
    assert!(g.failure.is_some());
    assert!(check(&g, "source-error").is_some());
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    stays_visible(&spec(), &[]);
    contract(&spec(), &[json!({"quarter":"Only","revenue":5,"margin":2})]);
    // Negative bars hang below zero and negative line values stand on their own scale.
    let neg = [
        json!({"quarter":"A","revenue":-5,"margin":-2}),
        json!({"quarter":"B","revenue":7,"margin":3}),
    ];
    contract(&spec(), &neg);
    let g = draw(&spec(), &neg);
    let plot = g.panels[0].plot;
    assert!(observations(&g)[0].area.unwrap().bottom() <= plot.bottom() + 1e-6);
    // The same value for every row leaves the line flat, not broken.
    let flat = [
        json!({"quarter":"A","revenue":1,"margin":4}),
        json!({"quarter":"B","revenue":2,"margin":4}),
    ];
    contract(&spec(), &flat);
    let g = draw(&spec(), &flat);
    let pts = points(&g);
    assert!((pts[0][1] - pts[1][1]).abs() < 1e-9);
    let g = draw(
        &spec(),
        &[
            json!({"quarter":"A","revenue":1e300,"margin":-1e300}),
            json!({"quarter":"B","revenue":5e299,"margin":1e300}),
        ],
    );
    assert!(nonfinite(&g).is_none());
    // A repeated category fails rather than guessing which row counts.
    let g = draw(
        &spec(),
        &[
            json!({"quarter":"A","revenue":1,"margin":1}),
            json!({"quarter":"A","revenue":2,"margin":2}),
        ],
    );
    assert!(g.failure.is_some());
}

#[test]
fn dense_combos_keep_every_category() {
    let r: Vec<Value> = (0..60)
        .map(|i| json!({"quarter": format!("Period number {i}"), "revenue": i % 9, "margin": (i * 7) % 11}))
        .collect();
    let g = draw_at(&spec(), &r, 390.0);
    assert!(g.failure.is_none());
    assert_eq!(observations(&g).len(), 60);
    assert_eq!(points(&g).len(), 60);
}

#[test]
fn colour_groups_get_bars_and_a_line_each() {
    let mut s = spec();
    s["encoding"]["color"] = json!("region");
    let r = vec![
        json!({"quarter":"Q1","revenue":10,"margin":1,"region":"North"}),
        json!({"quarter":"Q1","revenue":20,"margin":2,"region":"South"}),
        json!({"quarter":"Q2","revenue":30,"margin":3,"region":"North"}),
        json!({"quarter":"Q2","revenue":40,"margin":4,"region":"South"}),
    ];
    contract(&s, &r);
    let g = draw(&s, &r);
    assert_eq!(g.series.len(), 2);
    assert!(g.legend.is_some());
    assert_eq!(runs(&g), 2);
    assert_eq!(observations(&g).len(), 4);
    assert_eq!(line(observations(&g)[1], "South Margin"), Some("2%"));
}

#[test]
fn facets_draw_their_own_panels_on_shared_scales() {
    let mut s = spec();
    s["encoding"]["facet"] = json!("region");
    let r = vec![
        json!({"quarter":"Q1","revenue":10,"margin":1,"region":"North"}),
        json!({"quarter":"Q2","revenue":20,"margin":2,"region":"North"}),
        json!({"quarter":"Q1","revenue":40,"margin":4,"region":"South"}),
        json!({"quarter":"Q2","revenue":80,"margin":8,"region":"South"}),
    ];
    contract(&s, &r);
    let g = draw(&s, &r);
    assert_eq!(g.panels.len(), 2);
    // Same scale: South's Q1 bar (40) is four times North's (10), measured by the bars drawn.
    let bar = |p: usize| {
        g.panels[p]
            .items
            .iter()
            .find_map(|i| match i {
                Item::Rect {
                    rect, role: Role::Mark, ..
                } => Some(rect.h),
                _ => None,
            })
            .unwrap()
    };
    assert!((bar(1) / bar(0) - 4.0).abs() < 1e-6);
}

#[test]
fn horizontal_combos_keep_the_second_axis_above_the_plot() {
    let mut s = spec();
    s["style"] = json!({"orientation":"horizontal"});
    contract(&s, &rows());
    let g = draw(&s, &rows());
    let plot = g.panels[0].plot;
    let top_labels = g
        .items()
        .filter_map(|i| match i {
            Item::Text(t) if t.role == Role::AxisLabel && t.text.contains('%') => Some(t.y),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        !top_labels.is_empty() && top_labels.iter().all(|y| *y < plot.y),
        "{top_labels:?}"
    );
    let ys: Vec<f64> = observations(&g).iter().map(|o| o.area.unwrap().center()[1]).collect();
    assert!(ys.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn writes_the_gallery_when_asked() {
    gallery("combo", &spec(), &rows());
    let mut r = rows();
    r[2] = json!({"quarter":"Q3","revenue":125,"margin":null});
    gallery("combo-gap", &spec(), &r);
    let mut s = spec();
    s["encoding"]["color"] = json!("region");
    let grouped = vec![
        json!({"quarter":"Q1","revenue":10,"margin":1,"region":"North"}),
        json!({"quarter":"Q1","revenue":20,"margin":2,"region":"South"}),
        json!({"quarter":"Q2","revenue":30,"margin":3,"region":"North"}),
        json!({"quarter":"Q2","revenue":40,"margin":2,"region":"South"}),
    ];
    gallery("combo-groups", &s, &grouped);
    let mut s = spec();
    s["style"] = json!({"orientation":"horizontal"});
    gallery("combo-horizontal", &s, &rows());
}

#[test]
fn the_runtime_keeps_the_axes_of_values_a_grid_step_apart() {
    // The line sits a hair above a tick on a scale the bars stretch to a thousand.
    let near = vec![
        json!({"quarter":"Q1","revenue":1000,"margin":100.01}),
        json!({"quarter":"Q2","revenue":0.5,"margin":100.04}),
        json!({"quarter":"Q3","revenue":640,"margin":100.0}),
    ];
    runtime_matches_native(&spec(), &near);
}
