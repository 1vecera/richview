//! Waterfall charts: running sums, totals, restarts, signs and the contract every type shares
//! (FEA-CH-01, FEA-CH-75).

use super::common;

use common::*;
use serde_json::{Value, json};

fn spec() -> Value {
    json!({"type":"waterfall","title":"Profit bridge","encoding":{"x":"step","y":"amount","measure":"kind"},"axes":{"y":{"unit":"$"}}})
}

fn bridge() -> Vec<Value> {
    vec![
        json!({"step":"Revenue","amount":120,"kind":"start"}),
        json!({"step":"Costs","amount":-45.5}),
        json!({"step":"Tax","amount":-14.5}),
        json!({"step":"Grants","amount":30}),
        json!({"step":"Profit","amount":90,"kind":"total"}),
    ]
}

#[test]
fn the_golden_bridge_meets_the_shared_contract() {
    contract(&spec(), &bridge());
}

#[test]
fn bars_start_where_the_running_sum_stands() {
    let g = draw(&spec(), &bridge());
    let obs = observations(&g);
    assert_eq!(obs.len(), 5);
    let running: Vec<&str> = obs
        .iter()
        .map(|o| line(o, "Running total").or(line(o, "Total")).unwrap())
        .collect();
    assert_eq!(running, ["$120", "$74.5", "$60", "$90", "$90"]);
    assert_eq!(line(obs[1], "Change"), Some("−$45.5"));
    assert_eq!(line(obs[3], "Change"), Some("+$30"));
    // The vertical extent of each bar matches its change: costs are the tallest drop.
    let h = |i: usize| obs[i].area.unwrap().h;
    assert!(h(1) > h(2) && h(3) < h(1));
    // Costs hang from the top of revenue; their top edge is revenue's top edge.
    assert!((obs[1].area.unwrap().y - obs[0].area.unwrap().y).abs() < 0.01);
}

#[test]
fn a_total_that_disagrees_is_reported_and_never_replaced() {
    let mut rows = bridge();
    rows[4] = json!({"step":"Profit","amount":95,"kind":"total"});
    let g = draw(&spec(), &rows);
    assert!(check(&g, "waterfall-total").is_some());
    let obs = observations(&g);
    assert_eq!(
        line(obs[4], "Total"),
        Some("$90"),
        "the sum is shown, the given total is not drawn"
    );
}

#[test]
fn increases_decreases_and_totals_are_three_series() {
    let g = draw(&spec(), &bridge());
    let names: Vec<&str> = g.series.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Increase", "Decrease", "Total"]);
    assert!(g.legend.is_some());
}

#[test]
fn missing_steps_are_disclosed_and_bad_text_fails_the_panel() {
    let mut rows = bridge();
    rows[2] = json!({"step":"Tax","amount":null});
    let g = draw(&spec(), &rows);
    assert!(check(&g, "excluded-observations").is_some());
    assert_eq!(observations(&g).len(), 4);
    rows[2] = json!({"step":"Tax","amount":"1,000"});
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").is_some());
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    stays_visible(&spec(), &[]);
    // One step, a negative-only chain, and a zero step.
    contract(&spec(), &[json!({"step":"Only","amount":5})]);
    contract(
        &spec(),
        &[json!({"step":"A","amount":-5}), json!({"step":"B","amount":-7})],
    );
    let g = draw(
        &spec(),
        &[json!({"step":"A","amount":4}), json!({"step":"B","amount":0})],
    );
    assert!(observations(&g)[1].area.unwrap().h >= 1.0, "a zero step stays visible");
    // Extremes stay finite.
    let g = draw(
        &spec(),
        &[json!({"step":"A","amount":1e300}), json!({"step":"B","amount":-5e299})],
    );
    assert!(nonfinite(&g).is_none());
}

#[test]
fn dense_waterfalls_keep_every_name_and_report_what_they_drop() {
    let rows: Vec<Value> = (0..60)
        .map(|i| json!({"step": format!("Step number {i}"), "amount": (i % 7) as f64 - 3.0}))
        .collect();
    let g = draw_at(&spec(), &rows, 390.0);
    assert!(g.failure.is_none());
    assert_eq!(observations(&g).len(), 60);
}

#[test]
fn horizontal_waterfalls_place_steps_top_to_bottom() {
    let mut s = spec();
    s["style"] = json!({"orientation":"horizontal"});
    let g = draw(&s, &bridge());
    let obs = observations(&g);
    let ys: Vec<f64> = obs.iter().map(|o| o.area.unwrap().center()[1]).collect();
    assert!(ys.windows(2).all(|w| w[0] < w[1]), "{ys:?}");
    contract(&s, &bridge());
}

#[test]
fn facets_run_their_own_sums() {
    let mut s = spec();
    s["encoding"]["facet"] = json!("unit");
    let rows = vec![
        json!({"step":"A","amount":10,"unit":"North"}),
        json!({"step":"B","amount":5,"unit":"North"}),
        json!({"step":"A","amount":1,"unit":"South"}),
        json!({"step":"B","amount":2,"unit":"South"}),
    ];
    let g = draw(&s, &rows);
    assert_eq!(g.panels.len(), 2);
    assert_eq!(line(&g.panels[0].observations[1], "Running total"), Some("$15"));
    assert_eq!(line(&g.panels[1].observations[1], "Running total"), Some("$3"));
}

#[test]
fn writes_the_gallery_when_asked() {
    gallery("waterfall", &spec(), &bridge());
    let mut s = spec();
    s["style"] = json!({"orientation":"horizontal"});
    gallery("waterfall-horizontal", &s, &bridge());
}

#[test]
fn the_runtime_keeps_the_kind_and_side_of_steps_smaller_than_a_grid_step() {
    // A fall of 0.02 beside a total of 1000 is zero on the grid, and a rise of 0.02 likewise.
    let rows = vec![
        json!({"step":"Revenue","amount":1000.5,"kind":"start"}),
        json!({"step":"Rounding","amount":-0.02}),
        json!({"step":"Refund","amount":0.03}),
        json!({"step":"Costs","amount":-300.04}),
        json!({"step":"Profit","amount":700.47,"kind":"total"}),
    ];
    runtime_matches_native(&spec(), &rows);
}
