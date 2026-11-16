//! Pivot tables: sums by row and column with computed totals, right-aligned numbers, the caps on
//! wide matrices with the rest reported, and the failures that name their cause (FEA-CH-75).

mod common;

use common::*;
use richview_chart::Level;
use richview_chart::geometry::{Anchor, Geometry, Item, Role};
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/pivot.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn plain() -> Value {
    json!({"type":"pivot","title":"T","encoding":{"rows":"r","columns":"c","values":"v"}})
}

fn cell(r: &str, c: &str, v: Value) -> Value {
    json!({"r": r, "c": c, "v": v})
}

/// The digits of a printed number, so the test does not depend on the thousands separator.
fn digits(text: &str) -> String {
    text.chars().filter(char::is_ascii_digit).collect()
}

fn headings(g: &Geometry) -> Vec<&str> {
    observations(g)
        .into_iter()
        .map(|o| o.readout.heading.as_str())
        .collect()
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
    gallery("pivot", &spec, &rows);
}

#[test]
fn every_cell_is_the_sum_of_its_rows_and_a_repeated_cell_says_so() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let o = observations(&g);
    let east_q4 = o.iter().find(|o| o.readout.heading == "East · Q4").unwrap();
    assert_eq!(digits(line(east_q4, "Sales").unwrap()), "135");
    let sum = east_q4.readout.lines.iter().find(|l| l.name == "Sales").unwrap();
    assert!(sum.derived, "a sum of several rows is computed");
    assert_eq!(line(east_q4, "Source rows"), Some("2"));
    let north_q1 = o.iter().find(|o| o.readout.heading == "North · Q1").unwrap();
    assert_eq!(digits(line(north_q1, "Sales").unwrap()), "120");
    assert!(
        !north_q1
            .readout
            .lines
            .iter()
            .find(|l| l.name == "Sales")
            .unwrap()
            .derived
    );
    assert_eq!(north_q1.row, Some(0), "a cell of one row points at it");
    assert_eq!(line(north_q1, "region"), None);
    assert_eq!(line(north_q1, "quarter"), None);
}

#[test]
fn totals_are_computed_for_rows_columns_and_the_whole() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let o = observations(&g);
    let total = |heading: &str| {
        let o = o.iter().find(|o| o.readout.heading == heading).unwrap();
        let l = o.readout.lines.iter().find(|l| l.name == "Sales").unwrap();
        assert!(l.derived, "{heading} is computed");
        digits(&l.value)
    };
    assert_eq!(total("North · Total"), "595");
    assert_eq!(total("West · Total"), "660");
    assert_eq!(total("Total · Q1"), "445");
    assert_eq!(total("Total · Q4"), "675");
    assert_eq!(total("Total"), "2130");
    assert!(g.note.as_deref().unwrap_or_default().contains("totals"));
}

#[test]
fn observations_run_along_each_row_with_its_total_and_the_total_row_last() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let h = headings(&g);
    assert_eq!(h.len(), 25);
    assert_eq!(
        h[..6],
        [
            "North · Q1",
            "North · Q2",
            "North · Q3",
            "North · Q4",
            "North · Total",
            "South · Q1"
        ]
    );
    assert_eq!(
        h[20..],
        ["Total · Q1", "Total · Q2", "Total · Q3", "Total · Q4", "Total"]
    );
}

#[test]
fn numbers_are_right_aligned_in_their_cells() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let areas: Vec<_> = observations(&g).iter().filter_map(|o| o.area).collect();
    let numbers: Vec<_> = g
        .items()
        .filter_map(|i| match i {
            Item::Text(t) if t.role == Role::Label => Some(t),
            _ => None,
        })
        .collect();
    assert_eq!(numbers.len(), 25);
    for t in numbers {
        assert_eq!(t.anchor, Anchor::End, "{} is right-aligned", t.text);
        assert!(
            areas
                .iter()
                .any(|a| (t.x - (a.right() - 8.0)).abs() < 1e-6 && a.contains(t.bounds.center())),
            "{} sits at the right edge of a cell",
            t.text
        );
    }
}

#[test]
fn a_cell_without_rows_stays_empty_and_has_no_readout() {
    let (spec, mut rows) = fixture();
    rows.remove(5);
    let g = draw(&spec, &rows);
    assert!(g.failure.is_none());
    assert_eq!(observations(&g).len(), 24);
    assert!(!headings(&g).contains(&"South · Q2"));
    assert_eq!(marks(&g).len(), 25, "the empty cell is still a place to point at");
    contract(&spec, &rows);
}

#[test]
fn shading_tints_the_cells_by_value_only_when_asked() {
    let (mut spec, rows) = fixture();
    let alpha = |g: &Geometry| -> Vec<f64> {
        g.items()
            .filter_map(|i| match i {
                Item::Rect {
                    fill,
                    role: Role::Mark,
                    obs: Some(0..=3),
                    ..
                } => Some(fill.color.alpha()),
                _ => None,
            })
            .collect()
    };
    assert!(alpha(&draw(&spec, &rows)).iter().all(|a| *a == 0.0));
    spec["style"] = json!({"shade": true});
    let shaded = draw(&spec, &rows);
    assert!(alpha(&shaded).iter().all(|a| *a > 0.9));
    let fills: Vec<_> = shaded
        .items()
        .filter_map(|i| match i {
            Item::Rect {
                fill,
                role: Role::Mark,
                obs: Some(0..=3),
                ..
            } => Some(fill.color),
            _ => None,
        })
        .collect();
    assert_ne!(fills[0], fills[3], "a larger value has another tint");
    contract(&spec, &rows);
}

#[test]
fn twelve_columns_and_forty_rows_are_drawn_and_one_more_is_not() {
    let table = |rows: usize, cols: usize| -> Vec<Value> {
        (0..cols)
            .flat_map(|c| (0..rows).map(move |r| cell(&format!("R{r:02}"), &format!("C{c:02}"), json!(1))))
            .collect()
    };
    let at_limit = table(40, 12);
    let g = draw_at(&plain(), &at_limit, 1400.0);
    assert!(g.failure.is_none());
    assert!(check(&g, "limit-exceeded").is_none());
    assert_eq!(observations(&g).len(), 41 * 13);
    let grand = observations(&g).into_iter().last().unwrap();
    assert_eq!(digits(line(grand, "V").unwrap()), "480", "every row and column counts");
    let narrow = draw_at(&plain(), &at_limit, 320.0);
    assert!(narrow.failure.is_none() && nonfinite(&narrow).is_none());
    assert_eq!(
        observations(&narrow).len(),
        41 * 13,
        "a narrow figure keeps every column"
    );

    let wide = table(3, 13);
    let g = draw_at(&plain(), &wide, 1400.0);
    assert!(g.failure.is_some());
    let c = check(&g, "limit-exceeded").unwrap();
    assert_eq!(c.level, Level::Error);
    assert_eq!(c.details["what"], "columns");
    assert_eq!(c.details["count"], 13);
    assert_eq!(c.details["limit"], 12);
    assert!(observations(&g).is_empty());

    let tall = table(41, 2);
    let g = draw_at(&plain(), &tall, 640.0);
    assert!(g.failure.is_some());
    let c = check(&g, "limit-exceeded").unwrap();
    assert_eq!(c.level, Level::Error);
    assert_eq!(c.details["what"], "rows");
    assert_eq!(c.details["count"], 41);
    assert_eq!(c.details["limit"], 40);
    assert!(check(&g, "excluded-observations").is_none());
}

#[test]
fn a_narrow_figure_narrows_the_columns_and_never_leaves_one_out() {
    let (spec, rows) = fixture();
    let g = draw_at(&spec, &rows, 240.0);
    assert!(g.failure.is_none());
    assert!(nonfinite(&g).is_none());
    assert!(check(&g, "excluded-observations").is_none());
    let grand = observations(&g).into_iter().last().unwrap();
    assert_eq!(digits(line(grand, "Sales").unwrap()), "2130");
    let total_row = observations(&g)
        .into_iter()
        .filter(|o| o.readout.heading.starts_with("Total ·"))
        .count();
    assert_eq!(total_row, 4, "a total for every column, drawn or not");
}

#[test]
fn rows_without_a_name_or_a_value_are_disclosed() {
    let (spec, mut rows) = fixture();
    rows[0]["sales"] = Value::Null;
    rows[1]["region"] = json!("");
    let g = draw(&spec, &rows);
    assert_eq!(check(&g, "excluded-observations").unwrap().details["count"], 2);
    assert!(g.failure.is_none());
}

#[test]
fn a_value_that_is_not_a_number_fails_with_its_row() {
    let rows = [cell("A", "X", json!(1)), cell("B", "X", json!("lots"))];
    let g = draw(&plain(), &rows);
    assert!(g.failure.is_some());
    let c = check(&g, "non-numeric").unwrap();
    assert_eq!(c.path.as_deref(), Some("rows[1].v"));
}

#[test]
fn missing_channels_say_which_are_needed() {
    let spec = json!({"type":"pivot","title":"T","encoding":{"rows":"r"}});
    let g = draw(&spec, &[cell("A", "X", json!(1))]);
    assert!(g.failure.as_deref().unwrap().contains("encoding.columns"));
    stays_visible(&plain(), &[]);
}

#[test]
fn negative_and_single_cell_tables_draw() {
    contract(&plain(), &[cell("A", "X", json!(-4.5))]);
    contract(
        &plain(),
        &[
            cell("A", "X", json!(-4.5)),
            cell("A", "Y", json!(10)),
            cell("B", "X", json!(2)),
        ],
    );
}

#[test]
fn long_names_wrap_and_are_never_cut() {
    let rows = [
        cell("A very long region name that wraps", "A long column title", json!(12)),
        cell("Short", "A long column title", json!(7)),
    ];
    contract(&plain(), &rows);
    let g = draw(&plain(), &rows);
    let shown: Vec<&str> = g
        .items()
        .filter_map(|i| match i {
            Item::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .collect();
    for word in ["very", "long", "region", "name", "that", "wraps", "title"] {
        assert!(shown.iter().any(|t| t.contains(word)), "{word} is drawn whole");
    }
}
