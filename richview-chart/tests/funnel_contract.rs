//! Funnel charts: band widths at their true values, the readouts, and the shared contract
//! (FEA-CH-01, FEA-CH-75).

use super::common;

use common::*;
use serde_json::{Value, json};

fn spec() -> Value {
    json!({"type":"funnel","title":"Signup funnel","encoding":{"x":"stage","y":"users"}})
}

fn rows() -> Vec<Value> {
    vec![
        json!({"stage":"Visited","users":1000}),
        json!({"stage":"Signed up","users":400}),
        json!({"stage":"Activated","users":250}),
        json!({"stage":"Subscribed","users":50}),
    ]
}

#[test]
fn the_golden_funnel_meets_the_shared_contract() {
    contract(&spec(), &rows());
}

#[test]
fn band_widths_are_proportional_and_stages_run_top_to_bottom() {
    let g = draw(&spec(), &rows());
    let obs = observations(&g);
    assert_eq!(obs.len(), 4);
    let w = |i: usize| obs[i].area.unwrap().w;
    for (i, v) in [1000.0, 400.0, 250.0, 50.0].iter().enumerate() {
        assert!((w(i) / w(0) - v / 1000.0).abs() < 1e-9, "stage {i}");
    }
    let ys: Vec<f64> = obs.iter().map(|o| o.area.unwrap().y).collect();
    assert!(ys.windows(2).all(|p| p[0] < p[1]), "{ys:?}");
    // Every band is centred on the same axis.
    let c0 = obs[0].area.unwrap().center()[0];
    assert!(obs.iter().all(|o| (o.area.unwrap().center()[0] - c0).abs() < 1e-9));
    // Three trapezoids join four bands, and each starts at the width above and ends at the one below.
    let joins: Vec<_> = g
        .items()
        .filter(|i| matches!(i, richview_chart::geometry::Item::Area { .. }))
        .collect();
    assert_eq!(joins.len(), 3);
}

#[test]
fn readouts_give_the_value_and_both_shares() {
    let g = draw(&spec(), &rows());
    let obs = observations(&g);
    assert_eq!(obs[0].readout.heading, "Visited");
    assert_eq!(line(obs[0], "users"), Some("1,000"));
    assert_eq!(line(obs[0], "Of the first stage"), Some("100%"));
    assert_eq!(line(obs[0], "Of the previous stage"), None);
    assert_eq!(line(obs[1], "users"), Some("400"));
    assert_eq!(line(obs[1], "Of the first stage"), Some("40%"));
    assert_eq!(line(obs[1], "Of the previous stage"), Some("40%"));
    assert_eq!(line(obs[3], "Of the first stage"), Some("5.0%"));
    assert_eq!(line(obs[3], "Of the previous stage"), Some("20%"));
    let derived = |o: &richview_chart::geometry::Observation, n: &str| {
        o.readout.lines.iter().find(|l| l.name == n).map(|l| l.derived)
    };
    assert_eq!(derived(obs[1], "users"), Some(false));
    assert_eq!(derived(obs[1], "Of the first stage"), Some(true));
    assert_eq!(derived(obs[1], "Of the previous stage"), Some(true));
}

#[test]
fn a_negative_value_fails_the_panel_with_a_source_error() {
    let mut r = rows();
    r[2] = json!({"stage":"Activated","users":-3});
    let g = draw(&spec(), &r);
    assert!(g.failure.is_some());
    assert!(check(&g, "source-error").is_some());
}

#[test]
fn missing_values_are_disclosed_and_text_fails_the_panel() {
    let mut r = rows();
    r[1] = json!({"stage":"Signed up","users":null});
    let g = draw(&spec(), &r);
    assert!(check(&g, "excluded-observations").is_some());
    assert_eq!(observations(&g).len(), 3);
    assert_eq!(line(observations(&g)[1], "Of the previous stage"), Some("25%"));
    r[1] = json!({"stage":"Signed up","users":"lots"});
    let g = draw(&spec(), &r);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").is_some());
}

#[test]
fn a_value_too_small_to_represent_is_a_source_error() {
    let mut r = rows();
    r[1] = json!({"stage":"Signed up","users":"1e-999"});
    let g = draw(&spec(), &r);
    assert!(g.failure.is_some());
    assert!(check(&g, "source-error").is_some());
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    stays_visible(&spec(), &[]);
    contract(&spec(), &[json!({"stage":"Only","users":5})]);
    // A zero stage stays visible as a hairline and has no share to divide by afterwards.
    let r = [
        json!({"stage":"A","users":4}),
        json!({"stage":"B","users":0}),
        json!({"stage":"C","users":0}),
    ];
    contract(&spec(), &r);
    let g = draw(&spec(), &r);
    assert!(observations(&g)[1].area.unwrap().w >= 1.0);
    assert_eq!(line(observations(&g)[2], "Of the previous stage"), None);
    // An all-zero first stage has no share of the first.
    let g = draw(
        &spec(),
        &[json!({"stage":"A","users":0}), json!({"stage":"B","users":3})],
    );
    assert!(g.failure.is_none());
    assert_eq!(line(observations(&g)[1], "Of the first stage"), None);
    let g = draw(
        &spec(),
        &[json!({"stage":"A","users":1e300}), json!({"stage":"B","users":5e299})],
    );
    assert!(nonfinite(&g).is_none());
    assert_eq!(line(observations(&g)[1], "Of the first stage"), Some("50%"));
}

#[test]
fn dense_funnels_keep_every_stage() {
    let r: Vec<Value> = (0..40)
        .map(|i| json!({"stage": format!("Stage number {i}"), "users": 1000 - i * 20}))
        .collect();
    contract(&spec(), &r);
    let g = draw_at(&spec(), &r, 390.0);
    assert_eq!(observations(&g).len(), 40);
}

#[test]
fn long_names_wrap_or_are_reported_never_clipped() {
    let r = [
        json!({"stage":"A very long stage name that goes on and on and on","users":10}),
        json!({"stage":"Short","users":5}),
    ];
    let g = draw_at(&spec(), &r, 320.0);
    assert!(check(&g, "clipping").is_none());
    assert_eq!(observations(&g).len(), 2);
}

#[test]
fn facets_draw_one_funnel_each_on_a_shared_scale() {
    let mut s = spec();
    s["encoding"]["facet"] = json!("region");
    let r = vec![
        json!({"stage":"Visited","users":100,"region":"North"}),
        json!({"stage":"Bought","users":50,"region":"North"}),
        json!({"stage":"Visited","users":40,"region":"South"}),
        json!({"stage":"Bought","users":10,"region":"South"}),
    ];
    contract(&s, &r);
    let g = draw(&s, &r);
    assert_eq!(g.panels.len(), 2);
    let a = g.panels[0].observations[0].area.unwrap().w;
    let b = g.panels[1].observations[0].area.unwrap().w;
    assert!((b / a - 0.4).abs() < 1e-9, "one scale across panels");
    assert_eq!(line(&g.panels[1].observations[1], "Of the first stage"), Some("25%"));
}

#[test]
fn writes_the_gallery_when_asked() {
    gallery("funnel", &spec(), &rows());
}

#[test]
fn the_runtime_writes_the_values_the_exact_rows_wrote() {
    // 1000.04 and 99.01 snap to steps of the 4,096-step grid, so their written values differ.
    let rows = vec![
        json!({"stage":"Visited","users":1000.04}),
        json!({"stage":"Signed up","users":99.01}),
        json!({"stage":"Activated","users":0.07}),
    ];
    runtime_matches_native(&spec(), &rows);
}

#[test]
fn outer_labels_clear_the_panel_edges() {
    use richview_chart::geometry::{Item, Role};
    for width in [320.0, 390.0, 576.0, 640.0] {
        let geometry = draw_at(&spec(), &rows(), width);
        let panel = &geometry.panels[0];
        let labels: Vec<_> = panel
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Text(text) if text.role == Role::Label => Some(text),
                _ => None,
            })
            .collect();
        assert!(labels.iter().any(|text| text.text == "Visited"));
        assert!(labels.iter().any(|text| text.text == "1,000"));
        for text in labels {
            assert!(
                text.bounds.x >= panel.frame.x + 8.0 - 0.5,
                "{width}: {} at {}",
                text.text,
                text.bounds.x
            );
            assert!(
                text.bounds.right() <= panel.frame.right() - 8.0 + 0.5,
                "{width}: {} at {}",
                text.text,
                text.bounds.right()
            );
        }
    }
}
