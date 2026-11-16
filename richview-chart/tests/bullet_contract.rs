//! Bullet charts: the actual bar, the target line, range bands, orientation and the contract
//! every type shares (FEA-CH-75).

mod common;

use common::*;
use richview_chart::geometry::{Geometry, Item, Role};
use serde_json::{Value, json};

fn spec() -> Value {
    json!({
        "type":"bullet","title":"Sales",
        "encoding":{"x":"region","y":"sales","target":"goal"},
        "axes":{"y":{"label":"Sales"}},
        "style":{"ranges":[60,80,100]}
    })
}

fn sales() -> Vec<Value> {
    vec![
        json!({"region":"North","sales":72,"goal":85}),
        json!({"region":"South","sales":91,"goal":80}),
        json!({"region":"East","sales":55,"goal":70}),
    ]
}

fn bars(g: &Geometry) -> Vec<richview_chart::geometry::Rect> {
    marks(g)
        .into_iter()
        .filter_map(|i| match i {
            Item::Rect { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect()
}

fn targets(g: &Geometry) -> Vec<richview_chart::geometry::Rect> {
    marks(g)
        .into_iter()
        .filter_map(|i| match i {
            Item::Line { path, .. } => path.bounds(),
            _ => None,
        })
        .collect()
}

fn bands(g: &Geometry) -> usize {
    g.panels[0].items.iter().filter(|i| i.role() == Role::Band).count()
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    contract(&spec(), &sales());
}

#[test]
fn bullets_are_horizontal_by_default_with_the_bar_target_and_ranges_in_place() {
    let g = draw(&spec(), &sales());
    let obs = observations(&g);
    assert_eq!(obs.len(), 3);
    let ys: Vec<f64> = obs.iter().map(|o| o.area.unwrap().center()[1]).collect();
    assert!(
        ys.windows(2).all(|w| w[0] < w[1]),
        "one bullet per row, top to bottom: {ys:?}"
    );
    let bar = bars(&g);
    assert_eq!(bar.len(), 3, "one bar per row");
    // The bar's length follows the actual value: South (91) is longer than North (72) and East (55).
    assert!(bar[1].w > bar[0].w && bar[0].w > bar[2].w);
    // The target line stands at its value: South's target (80) is left of its bar end (91),
    // North's (85) is right of its bar end (72).
    let t = targets(&g);
    assert_eq!(t.len(), 3);
    assert!(t[1].x < bar[1].right() && t[0].x > bar[0].right());
    // Three ranges give three bands behind each bar.
    assert_eq!(bands(&g), 9);
    // The target spans more than the bar's thickness.
    assert!(t[0].h > bar[0].h);
}

#[test]
fn readouts_carry_actual_target_and_ranges_as_given() {
    let g = draw(&spec(), &sales());
    let obs = observations(&g);
    assert_eq!(obs[0].readout.heading, "North");
    assert_eq!(line(obs[0], "Sales"), Some("72"));
    assert_eq!(line(obs[0], "Target"), Some("85"));
    assert_eq!(line(obs[0], "Ranges"), Some("60 · 80 · 100"));
    assert!(obs[0].readout.lines.iter().all(|l| !l.derived));
}

#[test]
fn without_ranges_or_target_the_bar_still_draws() {
    let mut s = spec();
    s["style"] = json!({});
    s["encoding"].as_object_mut().unwrap().remove("target");
    let g = draw(&s, &sales());
    assert!(g.failure.is_none());
    assert_eq!(bands(&g), 0);
    assert!(targets(&g).is_empty());
    assert_eq!(line(observations(&g)[0], "Target"), None);
    contract(&s, &sales());
}

#[test]
fn ranges_that_are_not_ascending_numbers_fail_with_a_reason() {
    for bad in [json!([80, 60]), json!([1, 2, 3, 4]), json!(["a"]), json!("60")] {
        let mut s = spec();
        s["style"] = json!({"ranges": bad});
        let g = draw(&s, &sales());
        assert!(g.failure.is_some(), "{bad}");
        assert!(check(&g, "source-error").is_some());
    }
}

#[test]
fn missing_values_are_disclosed_and_bad_text_fails_the_panel() {
    let mut rows = sales();
    rows[1] = json!({"region":"South","sales":null,"goal":80});
    let g = draw(&spec(), &rows);
    assert!(check(&g, "excluded-observations").is_some());
    assert_eq!(observations(&g).len(), 2);
    // A missing target leaves the bar and no target line.
    rows[1] = json!({"region":"South","sales":91,"goal":null});
    let g = draw(&spec(), &rows);
    assert_eq!(observations(&g).len(), 3);
    assert_eq!(targets(&g).len(), 2);
    assert_eq!(line(observations(&g)[1], "Target"), None);
    rows[1] = json!({"region":"South","sales":"1,000","goal":80});
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").is_some());
    rows[1] = json!({"region":"South","sales":91,"goal":"1e-400"});
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "source-error").is_some());
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    stays_visible(&spec(), &[]);
    contract(&spec(), &[json!({"region":"Only","sales":5,"goal":9})]);
    contract(
        &spec(),
        &[
            json!({"region":"A","sales":-5,"goal":-2}),
            json!({"region":"B","sales":3,"goal":-7}),
        ],
    );
    let g = draw(&spec(), &[json!({"region":"A","sales":0,"goal":0})]);
    assert!(observations(&g)[0].area.unwrap().w >= 1.0);
    let g = draw(
        &spec(),
        &[
            json!({"region":"A","sales":1e300,"goal":-5e299}),
            json!({"region":"B","sales":1,"goal":2}),
        ],
    );
    assert!(nonfinite(&g).is_none());
}

#[test]
fn dense_charts_keep_every_name() {
    let rows: Vec<Value> = (0..40)
        .map(|i| json!({"region": format!("Region number {i}"), "sales": (i % 7) * 10, "goal": 40}))
        .collect();
    let g = draw_at(&spec(), &rows, 390.0);
    assert!(g.failure.is_none());
    assert_eq!(observations(&g).len(), 40);
}

#[test]
fn vertical_charts_put_values_up_the_page() {
    let mut s = spec();
    s["style"]["orientation"] = json!("vertical");
    let g = draw(&s, &sales());
    let bar = bars(&g);
    assert!(bar[1].h > bar[0].h && bar[0].h > bar[2].h);
    let xs: Vec<f64> = observations(&g).iter().map(|o| o.area.unwrap().center()[0]).collect();
    assert!(xs.windows(2).all(|w| w[0] < w[1]), "{xs:?}");
    contract(&s, &sales());
}

#[test]
fn facets_keep_their_own_rows() {
    let mut s = spec();
    s["encoding"]["facet"] = json!("unit");
    let rows = vec![
        json!({"region":"A","sales":10,"goal":20,"unit":"North"}),
        json!({"region":"B","sales":15,"goal":10,"unit":"North"}),
        json!({"region":"A","sales":5,"goal":6,"unit":"South"}),
    ];
    let g = draw(&s, &rows);
    assert_eq!(g.panels.len(), 2);
    assert_eq!(g.panels[0].observations.len(), 2);
    assert_eq!(line(&g.panels[1].observations[0], "Sales"), Some("5"));
    contract(&s, &rows);
}

#[test]
fn writes_the_gallery_when_asked() {
    gallery("bullet", &spec(), &sales());
    let mut s = spec();
    s["style"]["orientation"] = json!("vertical");
    gallery("bullet-vertical", &s, &sales());
}

#[test]
fn the_runtime_keeps_the_axis_of_values_a_grid_step_apart() {
    let rows = vec![
        json!({"region":"North","sales":100.01,"goal":100.04}),
        json!({"region":"South","sales":1000,"goal":640}),
        json!({"region":"East","sales":0.5,"goal":100.02}),
    ];
    runtime_matches_native(&spec(), &rows);
}
