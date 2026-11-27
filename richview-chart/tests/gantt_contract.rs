//! Gantt charts: a bar for each task on a time axis in the order of the source, milestones as
//! diamonds, computed durations marked as derived, and the failures that name their cause
//! (FEA-CH-75).

use super::common;

use common::*;
use richview_chart::Level;
use richview_chart::geometry::{Geometry, Item, Rect, Role, Shape};
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/gantt.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn plain() -> Value {
    json!({"type":"gantt","title":"T","encoding":{"label":"t","start":"s","end":"e"}})
}

fn task(label: &str, start: Value, end: Value) -> Value {
    json!({"t": label, "s": start, "e": end})
}

fn bars(g: &Geometry) -> Vec<Rect> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| match i {
            Item::Rect {
                rect, role: Role::Mark, ..
            } => Some(*rect),
            _ => None,
        })
        .collect()
}

fn diamonds(g: &Geometry) -> usize {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter(|i| {
            matches!(
                i,
                Item::Point {
                    shape: Shape::Diamond,
                    role: Role::Mark,
                    ..
                }
            )
        })
        .count()
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
}

#[test]
fn a_bar_for_each_task_in_source_order_and_a_diamond_for_the_milestone() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    assert_eq!(bars(&g).len(), 7);
    assert_eq!(diamonds(&g), 1);
    let o = observations(&g);
    assert_eq!(o.len(), 8);
    let names: Vec<&str> = o.iter().map(|o| o.readout.heading.as_str()).collect();
    assert_eq!(
        names,
        [
            "Research",
            "Wireframes",
            "Prototype",
            "Build API",
            "Build app",
            "Docs",
            "Testing",
            "Launch"
        ]
    );
    assert!(o.windows(2).all(|w| w[0].at[1] < w[1].at[1]), "tasks run top to bottom");
    assert!(g.legend.is_some());
}

#[test]
fn bars_start_and_end_where_their_dates_are() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let b = bars(&g);
    // Research starts first and Build app ends last; Docs and Testing overlap in time.
    assert!(b[0].x < b[1].x && b[1].x < b[2].x);
    assert!(b.iter().all(|r| r.w >= 2.0));
    let right = |i: usize| b[i].right();
    assert!(right(4) > right(3) && right(4) > right(5));
    assert!(
        b[5].x < b[6].right() && b[6].x < b[5].right(),
        "overlapping tasks overlap"
    );
    let plot = g.panels[0].plot;
    assert!(b.iter().all(|r| r.x >= plot.x - 0.5 && r.right() <= plot.right() + 0.5));
}

#[test]
fn the_readout_names_the_dates_and_marks_the_duration_as_derived() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let o = observations(&g);
    let research = o[0];
    assert!(
        line(research, "Start").unwrap().contains("Jan 5, 2026"),
        "{:?}",
        research.readout.lines
    );
    assert!(line(research, "End").unwrap().contains("Feb 13, 2026"));
    assert_eq!(line(research, "Duration"), Some("39 days"));
    let duration = research.readout.lines.iter().find(|l| l.name == "Duration").unwrap();
    assert!(duration.derived);
    assert!(
        !research
            .readout
            .lines
            .iter()
            .find(|l| l.name == "Start")
            .unwrap()
            .derived
    );
    assert_eq!(line(research, "Phase"), Some("Plan"));
    assert_eq!(line(o[7], "Duration"), None);
    assert!(g.note.as_deref().unwrap_or_default().contains("duration"));
}

#[test]
fn numbers_work_as_well_as_dates() {
    let rows = [
        task("Warm-up", json!(0), json!(10)),
        task("Main set", json!(10), json!(45.5)),
        task("Cool-down", json!(45.5), json!(60)),
    ];
    let g = draw(&plain(), &rows);
    assert!(g.failure.is_none());
    assert_eq!(bars(&g).len(), 3);
    assert_eq!(line(observations(&g)[1], "Duration"), Some("35.5"));
    contract(&plain(), &rows);
}

#[test]
fn a_start_and_an_end_of_different_kinds_say_why() {
    let rows = [
        task("A", json!("2026-01-01"), json!(12)),
        task("B", json!("2026-02-01"), json!(15)),
    ];
    let g = draw(&plain(), &rows);
    assert!(
        g.failure
            .as_deref()
            .unwrap()
            .contains("both be dates or both be numbers")
    );
    assert!(check(&g, "source-error").is_some());
}

#[test]
fn a_task_that_ends_before_it_starts_fails_with_its_row() {
    let rows = [
        task("A", json!("2026-01-01"), json!("2026-02-01")),
        task("B", json!("2026-03-01"), json!("2026-02-01")),
    ];
    let g = draw(&plain(), &rows);
    let failure = g.failure.as_deref().unwrap();
    assert!(failure.contains("ends before it starts"));
    let c = check(&g, "source-error").unwrap();
    assert_eq!(c.path.as_deref(), Some("rows[1].e"));
}

#[test]
fn rows_without_a_label_a_start_or_an_end_are_disclosed() {
    let (spec, mut rows) = fixture();
    rows[0]["end"] = Value::Null;
    rows[1]["task"] = json!("");
    let g = draw(&spec, &rows);
    assert_eq!(check(&g, "excluded-observations").unwrap().details["count"], 2);
    assert_eq!(observations(&g).len(), 6);
    assert!(g.failure.is_none());
}

#[test]
fn text_that_is_not_a_date_or_number_fails() {
    let rows = [task("A", json!("soon"), json!("later"))];
    let g = draw(&plain(), &rows);
    assert!(g.failure.is_some());
}

#[test]
fn facets_each_get_their_own_panel_on_one_axis() {
    let mut spec = plain();
    spec["encoding"]["facet"] = json!("f");
    let rows = [
        json!({"t": "A", "s": "2026-01-01", "e": "2026-02-01", "f": "North"}),
        json!({"t": "B", "s": "2026-02-01", "e": "2026-03-01", "f": "North"}),
        json!({"t": "C", "s": "2026-05-01", "e": "2026-06-01", "f": "South"}),
    ];
    let g = draw(&spec, &rows);
    assert_eq!(g.panels.len(), 2);
    assert_eq!(bars(&g).len(), 3);
    let north = g.panels[0].x.as_ref().unwrap();
    let south = g.panels[1].x.as_ref().unwrap();
    assert_eq!(north.domain, south.domain, "one time axis for every panel");
    contract(&spec, &rows);
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    let (spec, rows) = fixture();
    stays_visible(&spec, &[]);
    contract(&spec, &rows[..1]);
    contract(&spec, &rows[7..]);
    let same = [
        task("A", json!("2026-01-01"), json!("2026-01-01")),
        task("B", json!("2026-01-01"), json!("2026-01-01")),
    ];
    let g = draw(&plain(), &same);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    let huge = [
        task("A", json!(-1e300), json!(1e300)),
        task("B", json!(0), json!(1e300)),
    ];
    let g = draw(&plain(), &huge);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    let dense: Vec<Value> = (0..60)
        .map(|i| task(&format!("Task number {i}"), json!(i), json!(i + 5)))
        .collect();
    let g = draw_at(&plain(), &dense, 320.0);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    assert_eq!(observations(&g).len(), 60);
    assert!(check(&g, "limit-exceeded").is_none());
}

#[test]
fn above_sixty_tasks_in_a_panel_the_chart_is_not_drawn() {
    let tasks = |n: usize| -> Vec<Value> {
        (0..n)
            .map(|i| task(&format!("Task {i}"), json!(i), json!(i + 5)))
            .collect()
    };
    let g = draw(&plain(), &tasks(61));
    assert!(g.failure.is_some());
    let c = check(&g, "limit-exceeded").unwrap();
    assert_eq!(c.level, Level::Error);
    assert_eq!(c.details["what"], "tasks per panel");
    assert_eq!(c.details["count"], 61);
    assert_eq!(c.details["limit"], 60);
    assert!(observations(&g).is_empty());
    assert!(check(&g, "excluded-observations").is_none());

    // The limit counts one panel: two facets of 40 tasks each draw.
    let mut spec = plain();
    spec["encoding"]["facet"] = json!("team");
    let faceted: Vec<Value> = (0..80)
        .map(|i| {
            let mut t = task(&format!("Task {i}"), json!(i), json!(i + 5));
            t["team"] = json!(if i % 2 == 0 { "North" } else { "South" });
            t
        })
        .collect();
    let g = draw(&spec, &faceted);
    assert!(g.failure.is_none());
    assert!(check(&g, "limit-exceeded").is_none());
    assert_eq!(observations(&g).len(), 80);
}

#[test]
fn a_long_task_name_wraps_and_never_leaves_the_figure() {
    let rows = [
        task("Negotiate the supplier contract", json!(0), json!(3)),
        task("Ship", json!(3), json!(5)),
    ];
    for width in WIDTHS {
        let g = draw_at(&plain(), &rows, width);
        assert!(g.failure.is_none());
        assert!(check(&g, "clipping").is_none(), "{width}: {:?}", check(&g, "clipping"));
    }
}

#[test]
fn writes_the_gallery_when_asked() {
    let (spec, rows) = fixture();
    gallery("gantt", &spec, &rows);
}

#[test]
fn a_task_shorter_than_a_day_states_hours_and_minutes() {
    let rows = [
        task("Standup", json!("2026-03-02T09:00"), json!("2026-03-02T09:45")),
        task("Review", json!("2026-03-02T09:00"), json!("2026-03-02T10:30")),
        task("Workshop", json!("2026-03-02T09:00"), json!("2026-03-02T15:00")),
    ];
    let g = draw(&plain(), &rows);
    assert!(g.failure.is_none());
    let o = observations(&g);
    assert_eq!(line(o[0], "Duration"), Some("45 minutes"));
    assert_eq!(line(o[1], "Duration"), Some("1 hour 30 minutes"));
    assert_eq!(line(o[2], "Duration"), Some("6 hours"));
    for obs in &o {
        let duration = obs.readout.lines.iter().find(|l| l.name == "Duration").unwrap();
        assert!(duration.derived);
    }
}

#[test]
fn durations_state_days_with_their_remainder_and_group_large_counts() {
    let rows = [
        task("A", json!("2026-03-02T09:00"), json!("2026-03-03T10:00")),
        task("B", json!("2026-03-02T09:00"), json!("2026-03-03T10:01")),
        task("C", json!("2026-03-02T09:00:00"), json!("2026-03-02T09:00:30")),
        task("D", json!("2026-03-02T09:00"), json!("2026-03-04T09:00")),
        task("E", json!("2020-01-01"), json!("2024-04-14T12:00")),
        task("F", json!("2020-01-01"), json!("2024-03-15")),
        task("G", json!("2026-03-02T09:00"), json!("2026-03-02T10:00")),
        task("H", json!("2026-03-02T09:00"), json!("2026-03-02T09:00")),
    ];
    let g = draw(&plain(), &rows);
    assert!(g.failure.is_none());
    let o = observations(&g);
    assert_eq!(line(o[0], "Duration"), Some("1 day 1 hour"));
    assert_eq!(line(o[1], "Duration"), Some("1 day 1 hour 1 minute"));
    assert_eq!(line(o[2], "Duration"), Some("30 seconds"));
    assert_eq!(line(o[3], "Duration"), Some("2 days"));
    assert_eq!(line(o[4], "Duration"), Some("1,570 days"));
    assert_eq!(line(o[5], "Duration"), Some("1,535 days"));
    assert_eq!(line(o[6], "Duration"), Some("1 hour"));
    assert_eq!(line(o[7], "Duration"), None);
}

#[test]
fn the_runtime_keeps_bars_milestones_and_durations_of_the_exact_rows() {
    // A task of 0.03 beside ones of hundreds: on the grid it has no length and would turn into a
    // milestone.
    let rows = vec![
        task("A", json!(0.0), json!(400.0)),
        task("B", json!(100.01), json!(100.04)),
        task("C", json!(250.0), json!(250.0)),
        task("D", json!(300.0), json!(1000.0)),
    ];
    runtime_matches_native(&plain(), &rows);
}
