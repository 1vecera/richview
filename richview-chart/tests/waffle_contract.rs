//! Waffle charts: squares shared out by the largest remainder, column by column from the bottom
//! left, one observation per category, and the failures that name their cause (FEA-CH-75).

mod common;

use common::*;
use richview_chart::geometry::{Item, Rect, Role};
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/waffle.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn squares_of(g: &richview_chart::geometry::Geometry, series: usize) -> Vec<Rect> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| match i {
            Item::Rect {
                rect,
                role: Role::Mark,
                series: Some(s),
                ..
            } if *s == series => Some(*rect),
            _ => None,
        })
        .collect()
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
}

#[test]
fn squares_add_up_to_exactly_the_grid_by_largest_remainder() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let counts: Vec<usize> = (0..5).map(|s| squares_of(&g, s).len()).collect();
    // 44.6, 25.5, 12.8, 10.5, 6.6: the three left over go to .77, .60 and .59.
    assert_eq!(counts, vec![45, 25, 13, 10, 7]);
    assert_eq!(counts.iter().sum::<usize>(), 100);
    let obs = observations(&g);
    assert_eq!(obs.len(), 5);
    assert_eq!(line(obs[0], "Squares"), Some("45 of 100"));
    assert!(
        obs[0]
            .readout
            .lines
            .iter()
            .find(|l| l.name == "Squares")
            .unwrap()
            .derived
    );
    assert!(obs[0].readout.lines.iter().find(|l| l.name == "Share").unwrap().derived);
    let value = obs[0].readout.lines.iter().find(|l| l.name == "Trips").unwrap();
    assert_eq!(value.value, "412");
    assert!(!value.derived);
    assert!(g.note.as_deref().unwrap().contains("1/100"));
}

#[test]
fn squares_fill_column_by_column_from_the_bottom_left() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let plot = g.panels[0].plot;
    let all: Vec<Rect> = (0..5).flat_map(|s| squares_of(&g, s)).collect();
    // Source order is paint order: the first square is at the bottom left, the second above it.
    assert!(all[0].x < plot.x + plot.w / 10.0 && all[0].bottom() > plot.bottom() - plot.h / 10.0);
    assert!(all[1].y < all[0].y && (all[1].x - all[0].x).abs() < 1e-9);
    // Ten squares fill a column; the eleventh starts the next one at the bottom.
    assert!(all[10].x > all[0].x && (all[10].y - all[0].y).abs() < 1e-9);
    // The last square is in the top right column.
    assert!(all[99].right() > plot.right() - plot.w / 10.0 && all[99].y < plot.y + plot.h / 10.0);
}

#[test]
fn an_observation_covers_the_box_of_its_squares() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let obs = observations(&g);
    let sq = squares_of(&g, 1);
    let area = obs[1].area.unwrap();
    for s in sq {
        assert!(s.x >= area.x - 1e-9 && s.right() <= area.right() + 1e-9);
        assert!(s.y >= area.y - 1e-9 && s.bottom() <= area.bottom() + 1e-9);
    }
    let items: Vec<&Item> = marks(&g);
    assert!(items.iter().all(|i| matches!(i, Item::Rect { obs: Some(_), .. })));
}

#[test]
fn style_cells_changes_the_grid() {
    let (mut spec, rows) = fixture();
    for cells in [25, 50] {
        spec["style"] = json!({"cells": cells});
        let g = draw(&spec, &rows);
        let total: usize = (0..5).map(|s| squares_of(&g, s).len()).sum();
        assert_eq!(total, cells);
        contract(&spec, &rows);
    }
    spec["style"] = json!({"cells": 40});
    let g = draw(&spec, &rows);
    assert!(check(&g, "style-invalid").is_some());
    assert_eq!((0..5).map(|s| squares_of(&g, s).len()).sum::<usize>(), 100);
}

#[test]
fn a_category_too_small_for_a_square_is_reported_not_drawn() {
    let (spec, mut rows) = fixture();
    rows.push(json!({"mode": "Ferry", "trips": 1}));
    let g = draw(&spec, &rows);
    assert!(check(&g, "no-square").unwrap().message.contains("Ferry"));
    assert_eq!(observations(&g).len(), 5);
    assert_eq!(squares_of(&g, 5).len(), 0);
}

#[test]
fn shares_of_a_given_whole_are_read_as_shares() {
    let spec = json!({"type":"waffle","title":"T","encoding":{"x":"k","y":"v"}});
    let rows = [json!({"k":"Yes","v":0.62}), json!({"k":"No","v":0.38})];
    let g = draw(&spec, &rows);
    assert_eq!(squares_of(&g, 0).len(), 62);
    assert_eq!(squares_of(&g, 1).len(), 38);
}

#[test]
fn negative_values_fail_the_panel() {
    let (spec, mut rows) = fixture();
    rows[1]["trips"] = json!(-3);
    let g = draw(&spec, &rows);
    assert!(g.failure.as_deref().unwrap().contains("negative"));
    assert!(check(&g, "source-error").unwrap().message.contains("Row 2"));
}

#[test]
fn text_that_is_not_a_number_fails_the_panel() {
    let (spec, mut rows) = fixture();
    rows[2]["trips"] = json!("many");
    let g = draw(&spec, &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").is_some());
}

#[test]
fn a_missing_value_is_disclosed_and_never_a_zero() {
    let (spec, mut rows) = fixture();
    rows[4]["trips"] = Value::Null;
    let g = draw(&spec, &rows);
    assert!(check(&g, "excluded-observations").is_some());
    assert_eq!(observations(&g).len(), 4);
    assert_eq!((0..5).map(|s| squares_of(&g, s).len()).sum::<usize>(), 100);
}

#[test]
fn a_repeated_category_fails_the_panel() {
    let (spec, mut rows) = fixture();
    rows.push(json!({"mode": "Car", "trips": 5}));
    let g = draw(&spec, &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "source-error").unwrap().message.contains("Car"));
}

#[test]
fn facets_each_get_a_whole_grid() {
    let spec = json!({"type":"waffle","title":"T","encoding":{"x":"k","y":"v","facet":"f"}});
    let rows = [
        json!({"k":"A","v":3,"f":"North"}),
        json!({"k":"B","v":1,"f":"North"}),
        json!({"k":"A","v":1,"f":"South"}),
        json!({"k":"B","v":1,"f":"South"}),
    ];
    let g = draw(&spec, &rows);
    assert_eq!(g.panels.len(), 2);
    assert_eq!(squares_of(&g, 0).len() + squares_of(&g, 1).len(), 200);
    assert_eq!(g.panels[1].observations[0].readout.lines[2].value, "50 of 100");
    contract(&spec, &rows);
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    let (spec, rows) = fixture();
    stays_visible(&spec, &[]);
    contract(&spec, &rows[..1]);
    let zero = [json!({"mode":"A","trips":0}), json!({"mode":"B","trips":0})];
    stays_visible(&spec, &zero);
    let huge = [json!({"mode":"A","trips":1e300}), json!({"mode":"B","trips":1e300})];
    let g = draw(&spec, &huge);
    assert!(nonfinite(&g).is_none());
    let dense: Vec<Value> = (0..300)
        .map(|i| json!({"mode": format!("Category number {i}"), "trips": (i % 7 + 1) as f64}))
        .collect();
    let g = draw_at(&spec, &dense, 320.0);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    assert_eq!((0..300).map(|s| squares_of(&g, s).len()).sum::<usize>(), 100);
}

#[test]
fn writes_the_gallery_when_asked() {
    let (spec, rows) = fixture();
    gallery("waffle", &spec, &rows);
}

#[test]
fn the_runtime_shares_the_squares_the_exact_rows_shared() {
    // Remainders closer than a grid step decide which part earns the last squares.
    let spec = json!({"type":"waffle","title":"T","encoding":{"x":"mode","y":"trips"}});
    let rows = vec![
        json!({"mode": "Walk", "trips": 333.34}),
        json!({"mode": "Bike", "trips": 333.33}),
        json!({"mode": "Bus", "trips": 333.33}),
        json!({"mode": "Boat", "trips": 0.04}),
        json!({"mode": "Car", "trips": 1000.0}),
    ];
    runtime_matches_native(&spec, &rows);
}
