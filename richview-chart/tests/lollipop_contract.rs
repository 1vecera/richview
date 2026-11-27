//! Lollipop charts: stems from zero, dots at the value, groups side by side, signs, orientation
//! and the contract every type shares (FEA-CH-75).

use super::common;

use common::*;
use richview_chart::geometry::{Item, Role};
use serde_json::{Value, json};

fn spec() -> Value {
    json!({"type":"lollipop","title":"Orders","encoding":{"x":"region","y":"orders"},"axes":{"y":{"label":"Orders"}}})
}

fn orders() -> Vec<Value> {
    vec![
        json!({"region":"North","orders":120}),
        json!({"region":"South","orders":90}),
        json!({"region":"East","orders":-20}),
        json!({"region":"West","orders":60}),
    ]
}

fn grouped_spec() -> Value {
    let mut s = spec();
    s["encoding"]["color"] = json!("year");
    s
}

fn grouped_rows() -> Vec<Value> {
    vec![
        json!({"region":"North","year":"2025","orders":120}),
        json!({"region":"North","year":"2026","orders":150}),
        json!({"region":"South","year":"2025","orders":90}),
        json!({"region":"South","year":"2026","orders":70}),
    ]
}

fn points(g: &richview_chart::geometry::Geometry) -> Vec<[f64; 2]> {
    marks(g)
        .into_iter()
        .filter_map(|i| match i {
            Item::Point { center, .. } => Some(*center),
            _ => None,
        })
        .collect()
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    contract(&spec(), &orders());
    contract(&grouped_spec(), &grouped_rows());
}

#[test]
fn dots_sit_at_their_values_and_stems_start_at_zero() {
    let g = draw(&spec(), &orders());
    let obs = observations(&g);
    assert_eq!(obs.len(), 4);
    assert_eq!(obs[0].readout.heading, "North");
    assert_eq!(line(obs[0], "Orders"), Some("120"));
    assert_eq!(line(obs[2], "Orders"), Some("−20"));
    let dots = points(&g);
    // Vertical by default: a larger value is higher; the negative value is below the others.
    assert!(dots[0][1] < dots[1][1] && dots[1][1] < dots[3][1] && dots[3][1] < dots[2][1]);
    // Each observation's dot is its point, and every stem runs from the baseline to its dot.
    let baseline = g.panels[0]
        .items
        .iter()
        .find_map(|i| match i {
            Item::Line {
                path,
                role: Role::Baseline,
                ..
            } => path.bounds().map(|b| b.y),
            _ => None,
        })
        .expect("a zero baseline");
    let stems: Vec<_> = marks(&g)
        .into_iter()
        .filter_map(|i| match i {
            Item::Line { path, .. } => path.bounds(),
            _ => None,
        })
        .collect();
    assert_eq!(stems.len(), 4);
    for (stem, dot) in stems.iter().zip(&dots) {
        assert!((stem.y - baseline).abs() < 0.01 || (stem.bottom() - baseline).abs() < 0.01);
        assert!((stem.x - dot[0]).abs() < 0.01);
        assert!((stem.y - dot[1]).abs() < 0.01 || (stem.bottom() - dot[1]).abs() < 0.01);
    }
    // The hit area covers the stem and the dot.
    for (o, dot) in obs.iter().zip(&dots) {
        let a = o.area.unwrap();
        assert!(a.x <= dot[0] && dot[0] <= a.right() && a.y <= dot[1] && dot[1] <= a.bottom());
    }
}

#[test]
fn groups_sit_side_by_side_in_one_band() {
    let g = draw(&grouped_spec(), &grouped_rows());
    assert_eq!(g.series.len(), 2);
    assert!(g.legend.is_some());
    let obs = observations(&g);
    assert_eq!(obs.len(), 4);
    assert_eq!(line(obs[0], "2025"), Some("120"));
    assert_eq!(line(obs[1], "2026"), Some("150"));
    assert!(
        obs[0].x < obs[1].x && obs[1].x < obs[2].x,
        "groups share North's band first"
    );
    assert!(obs[1].x - obs[0].x < obs[2].x - obs[1].x);
}

#[test]
fn missing_values_are_disclosed_and_bad_text_fails_the_panel() {
    let mut rows = orders();
    rows[1] = json!({"region":"South","orders":null});
    let g = draw(&spec(), &rows);
    assert!(check(&g, "excluded-observations").is_some());
    assert_eq!(observations(&g).len(), 3);
    rows[1] = json!({"region":"South","orders":"1,000"});
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").is_some());
    rows[1] = json!({"region":"South","orders":"1e-400"});
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "source-error").is_some());
}

#[test]
fn a_repeated_category_is_a_source_error() {
    let mut rows = orders();
    rows.push(json!({"region":"North","orders":5}));
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "source-error").is_some());
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    stays_visible(&spec(), &[]);
    contract(&spec(), &[json!({"region":"Only","orders":5})]);
    contract(
        &spec(),
        &[json!({"region":"A","orders":-5}), json!({"region":"B","orders":-7})],
    );
    // A zero value is a dot on the baseline and stays reachable.
    let g = draw(
        &spec(),
        &[json!({"region":"A","orders":4}), json!({"region":"B","orders":0})],
    );
    assert!(observations(&g)[1].area.unwrap().h >= 1.0);
    let g = draw(
        &spec(),
        &[
            json!({"region":"A","orders":1e300}),
            json!({"region":"B","orders":-5e299}),
        ],
    );
    assert!(nonfinite(&g).is_none());
}

#[test]
fn dense_charts_keep_every_name() {
    let rows: Vec<Value> = (0..60)
        .map(|i| json!({"region": format!("Region number {i}"), "orders": (i % 7) as f64 - 3.0}))
        .collect();
    let g = draw_at(&spec(), &rows, 390.0);
    assert!(g.failure.is_none());
    assert_eq!(observations(&g).len(), 60);
}

#[test]
fn horizontal_charts_place_categories_top_to_bottom() {
    let mut s = spec();
    s["style"] = json!({"orientation":"horizontal"});
    let g = draw(&s, &orders());
    let obs = observations(&g);
    let ys: Vec<f64> = obs.iter().map(|o| o.area.unwrap().center()[1]).collect();
    assert!(ys.windows(2).all(|w| w[0] < w[1]), "{ys:?}");
    let dots = points(&g);
    assert!(dots[0][0] > dots[1][0] && dots[2][0] < dots[3][0], "values run along x");
    contract(&s, &orders());
}

#[test]
fn facets_share_a_scale_and_keep_their_own_marks() {
    let mut s = spec();
    s["encoding"]["facet"] = json!("unit");
    let rows = vec![
        json!({"region":"A","orders":10,"unit":"North"}),
        json!({"region":"B","orders":5,"unit":"North"}),
        json!({"region":"A","orders":1,"unit":"South"}),
        json!({"region":"B","orders":2,"unit":"South"}),
    ];
    let g = draw(&s, &rows);
    assert_eq!(g.panels.len(), 2);
    assert_eq!(g.panels[0].observations.len(), 2);
    assert_eq!(line(&g.panels[1].observations[1], "Orders"), Some("2"));
    contract(&s, &rows);
}

#[test]
fn writes_the_gallery_when_asked() {
    gallery("lollipop", &spec(), &orders());
    gallery("lollipop-grouped", &grouped_spec(), &grouped_rows());
    let mut s = spec();
    s["style"] = json!({"orientation":"horizontal"});
    gallery("lollipop-horizontal", &s, &orders());
}

#[test]
fn the_runtime_keeps_the_axis_of_values_a_grid_step_apart() {
    let rows = vec![
        json!({"region":"North","orders":100.01}),
        json!({"region":"South","orders":1000}),
        json!({"region":"East","orders":100.04}),
        json!({"region":"West","orders":0.5}),
    ];
    runtime_matches_native(&spec(), &rows);
}
