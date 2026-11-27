//! Calendar charts: a square per day in week columns with Monday on top, a strip for each year,
//! empty outlines for days without a row, a colour key, and the failures that name their cause
//! (FEA-CH-75).

use super::common;

use common::*;
use richview_chart::geometry::{Geometry, Item, Observation, Role};
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/calendar.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn day<'a>(g: &'a Geometry, heading: &str) -> &'a Observation {
    observations(g)
        .into_iter()
        .find(|o| o.readout.heading == heading)
        .unwrap_or_else(|| panic!("no observation for {heading}"))
}

fn texts(g: &Geometry, role: Role) -> Vec<String> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .chain(&g.figure_items)
        .filter_map(|i| match i {
            Item::Text(t) if t.role == role => Some(t.text.clone()),
            _ => None,
        })
        .collect()
}

fn count(g: &Geometry, role: Role) -> usize {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter(|i| i.role() == role)
        .count()
}

fn plain() -> Value {
    json!({"type":"calendar","title":"T","encoding":{"x":"d","y":"v"}})
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
}

#[test]
fn every_row_is_a_day_and_the_other_days_of_the_months_are_empty() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    assert_eq!(observations(&g).len(), rows.len());
    assert_eq!(count(&g, Role::Mark), rows.len());
    // February and March 2025 have 59 days.
    assert_eq!(count(&g, Role::Missing), 59 - rows.len());
    let o = day(&g, "Mar 4, 2025");
    assert_eq!(line(o, "Commits"), Some("5"));
    assert!(!o.readout.lines[0].derived);
    assert_eq!(day(&g, "Mar 3, 2025").readout.lines[0].value, "0");
}

#[test]
fn days_fill_week_columns_with_monday_on_top() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let (mon, tue, sat) = (day(&g, "Mar 3, 2025"), day(&g, "Mar 4, 2025"), day(&g, "Mar 8, 2025"));
    let next_mon = day(&g, "Mar 10, 2025");
    let pitch = tue.at[1] - mon.at[1];
    assert!(pitch > 1.0);
    assert!((tue.at[0] - mon.at[0]).abs() < 1e-9, "a week is one column");
    assert!((sat.at[1] - mon.at[1] - 5.0 * pitch).abs() < 1e-6);
    assert!(
        (next_mon.at[0] - mon.at[0] - pitch).abs() < 1e-6,
        "the next Monday is one column on"
    );
    assert!((next_mon.at[1] - mon.at[1]).abs() < 1e-9);
    // Sunday the 9th has no row, so its cell is empty and the column keeps its place.
    assert!(observations(&g).iter().all(|o| o.readout.heading != "Mar 9, 2025"));
}

#[test]
fn months_and_weekdays_are_labelled() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let labels = texts(&g, Role::AxisLabel);
    for want in ["Feb", "Mar", "2025", "Mon", "Wed", "Fri"] {
        assert!(labels.iter().any(|l| l == want), "{want} in {labels:?}");
    }
    // One outline for each month.
    assert_eq!(count(&g, Role::Grid), 2);
}

#[test]
fn a_key_decodes_the_colour_and_names_the_empty_outline() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let key = texts(&g, Role::Key);
    assert!(key.contains(&"No data".to_string()));
    assert!(
        key.contains(&"0".to_string()) && key.contains(&"20".to_string()),
        "{key:?}"
    );
    assert!(g.figure_items.iter().any(|i| i.role() == Role::Key));
}

#[test]
fn negative_values_get_the_second_arm_and_a_zero_in_the_key() {
    let (spec, mut rows) = fixture();
    rows[3]["commits"] = json!(-4);
    let g = draw(&spec, &rows);
    let key = texts(&g, Role::Key);
    assert!(
        key.contains(&"\u{2212}20".to_string()) && key.contains(&"0".to_string()),
        "{key:?}"
    );
    contract(&spec, &rows);
}

#[test]
fn a_range_over_two_years_gets_a_strip_for_each() {
    let spec = plain();
    let rows = [json!({"d": "2024-12-30", "v": 1}), json!({"d": "2025-01-02", "v": 2})];
    let g = draw(&spec, &rows);
    let labels = texts(&g, Role::AxisLabel);
    assert!(labels.contains(&"2024".to_string()) && labels.contains(&"2025".to_string()));
    // December 2024 and January 2025: 31 + 31 days.
    assert_eq!(count(&g, Role::Mark) + count(&g, Role::Missing), 62);
    contract(&spec, &rows);
}

#[test]
fn a_repeated_date_fails_the_panel() {
    let rows = [json!({"d": "2025-03-01", "v": 1}), json!({"d": "2025-03-01", "v": 2})];
    let g = draw(&plain(), &rows);
    assert!(g.failure.as_deref().unwrap().contains("twice"));
    assert!(check(&g, "source-error").unwrap().message.contains("Row 2"));
}

#[test]
fn dates_without_a_day_or_numbers_for_dates_fail() {
    let g = draw(
        &plain(),
        &[json!({"d": "2025-03", "v": 1}), json!({"d": "2025-04", "v": 2})],
    );
    assert!(g.failure.as_deref().unwrap().contains("day"));
    let g = draw(&plain(), &[json!({"d": 5, "v": 1}), json!({"d": 6, "v": 2})]);
    assert!(g.failure.is_some());
}

#[test]
fn text_that_is_not_a_number_fails_and_a_missing_value_is_disclosed() {
    let (spec, mut rows) = fixture();
    rows[1]["commits"] = json!("lots");
    let g = draw(&spec, &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").is_some());
    let (spec, mut rows) = fixture();
    rows[1]["commits"] = Value::Null;
    rows[2]["day"] = Value::Null;
    let g = draw(&spec, &rows);
    assert!(g.failure.is_none());
    assert_eq!(check(&g, "excluded-observations").unwrap().details["count"], 2);
    assert_eq!(observations(&g).len(), rows.len() - 2);
}

#[test]
fn more_than_twelve_years_is_refused() {
    let rows = [json!({"d": "2001-01-01", "v": 1}), json!({"d": "2025-01-01", "v": 2})];
    let g = draw(&plain(), &rows);
    assert!(g.failure.as_deref().unwrap().contains("years"));
    assert!(check(&g, "calendar-range").is_some());
}

#[test]
fn facets_each_get_their_own_strips() {
    let spec = json!({"type":"calendar","title":"T","encoding":{"x":"d","y":"v","facet":"f"}});
    let rows = [
        json!({"d": "2025-03-01", "v": 1, "f": "North"}),
        json!({"d": "2025-03-02", "v": 5, "f": "North"}),
        json!({"d": "2025-03-01", "v": 3, "f": "South"}),
    ];
    let g = draw(&spec, &rows);
    assert_eq!(g.panels.len(), 2);
    assert_eq!(g.panels[0].observations.len(), 2);
    assert_eq!(g.panels[1].observations.len(), 1);
    contract(&spec, &rows);
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    let (spec, rows) = fixture();
    stays_visible(&spec, &[]);
    contract(&spec, &rows[..1]);
    contract(&plain(), &[json!({"d": "2025-02-28", "v": 0})]);
    let huge = [
        json!({"d": "2025-03-01", "v": 1e300}),
        json!({"d": "2025-03-02", "v": 1.0}),
    ];
    let g = draw(&plain(), &huge);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    // A leap day, and a whole year of days.
    let first = richview_chart::time::days_from_civil(2024, 1, 1);
    let year: Vec<Value> = (0..366)
        .map(|k| {
            let (y, m, d) = richview_chart::time::civil_from_days(first + k);
            json!({"d": format!("{y:04}-{m:02}-{d:02}"), "v": k % 9})
        })
        .collect();
    let g = draw_at(&plain(), &year, 320.0);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    assert_eq!(observations(&g).len(), 366);
    contract(&plain(), &year);
}

#[test]
fn writes_the_gallery_when_asked() {
    let (spec, rows) = fixture();
    gallery("calendar", &spec, &rows);
}

#[test]
fn unlabelled_measure_uses_a_readable_field_name() {
    let g = draw(&plain(), &[json!({"d":"2026-03-10","v":3})]);
    let o = day(&g, "Mar 10, 2026");
    assert_eq!(line(o, "V"), Some("3"));
}
