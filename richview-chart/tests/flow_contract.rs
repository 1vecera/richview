//! Flow charts: cards in ranks along the flow, right-angled lines that never cross a card, labels
//! that never collide, groups that keep their cards together, loops that go round the outside,
//! the diagnostics of a bad graph, and the shared contract every type meets (FEA-CH-76,
//! FEA-CH-75).

mod common;

use common::*;
use richview_chart::Level;
use richview_chart::geometry::{Cmd, Geometry, Item, Rect, Role};
use richview_chart::layout::flow_table_rows;
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let text = include_str!("fixtures/types/flow.json");
    let v: Value = serde_json::from_str(text).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

/// A flow written as cards and edges, with the Table rows the core gives it.
fn spec_rows(spec: Value) -> (Value, Vec<Value>) {
    let rows = flow_table_rows(&spec).unwrap();
    (spec, rows)
}

fn lanes_fixture() -> (Value, Vec<Value>) {
    let spec = json!({
        "type": "flow",
        "title": "A support request takes one of three paths to close",
        "encoding": {"label": "step", "source": "from", "target": "to", "lane": "lane", "y": "note"}
    });
    let node = |s: &str, l: &str| json!({"step": s, "lane": l});
    let link = |a: &str, b: &str, n: Option<&str>| match n {
        Some(n) => json!({"from": a, "to": b, "note": n}),
        None => json!({"from": a, "to": b}),
    };
    let rows = vec![
        node("Request", "Customer"),
        node("Triage", "Support"),
        node("Fix", "Support"),
        node("Invoice", "Billing"),
        node("Refund", "Billing"),
        node("Pay", "Customer"),
        node("Close", "Support"),
        link("Request", "Triage", Some("new")),
        link("Triage", "Fix", Some("bug")),
        link("Triage", "Invoice", Some("bill")),
        link("Triage", "Refund", Some("ask")),
        link("Fix", "Close", None),
        link("Invoice", "Pay", None),
        link("Pay", "Close", Some("paid")),
        link("Request", "Close", Some("spam")),
    ];
    (spec, rows)
}

/// Each card's box by title, from the observation that owns it.
fn boxes(g: &Geometry) -> Vec<(String, Rect)> {
    observations(g)
        .iter()
        .filter(|o| !o.readout.heading.contains(" → "))
        .map(|o| (o.readout.heading.clone(), o.area.unwrap()))
        .collect()
}

fn rect_of(g: &Geometry, name: &str) -> Rect {
    boxes(g)
        .into_iter()
        .find(|b| b.0 == name)
        .unwrap_or_else(|| panic!("no card {name}"))
        .1
}

/// The corners of every line, with the curves of its rounded corners taken as their ends.
fn routes(g: &Geometry) -> Vec<Vec<[f64; 2]>> {
    g.items()
        .filter_map(|i| match i {
            Item::Line {
                path,
                role: Role::Mark,
                obs: Some(o),
                ..
            } if is_edge(g, *o) => Some(
                path.cmds
                    .iter()
                    .map(|c| match c {
                        Cmd::M { p } | Cmd::L { p } | Cmd::C { p, .. } => *p,
                        Cmd::Z => [f64::NAN; 2],
                    })
                    .collect(),
            ),
            _ => None,
        })
        .collect()
}

fn is_edge(g: &Geometry, obs: usize) -> bool {
    g.panels.iter().any(|p| {
        p.observations
            .get(obs)
            .is_some_and(|o| o.readout.heading.contains(" → "))
    })
}

/// Text boxes of the labels set on lines and group borders: those with a surface patch.
fn label_boxes(g: &Geometry) -> Vec<(String, Rect)> {
    let patches: Vec<Rect> = g
        .items()
        .filter_map(|i| match i {
            Item::Rect {
                rect,
                role: Role::Annotation,
                ..
            } => Some(*rect),
            _ => None,
        })
        .collect();
    g.items()
        .filter_map(|i| match i {
            Item::Text(t) if patches.iter().any(|p| t.bounds.inside(p, 1.0)) => Some((t.text.clone(), t.bounds)),
            _ => None,
        })
        .collect()
}

fn decision() -> (Value, Vec<Value>) {
    fixture()
}

#[test]
fn the_golden_flow_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    gallery("flow", &spec, &rows);
    let (lanes, lane_rows) = lanes_fixture();
    gallery("flow-lanes", &lanes, &lane_rows);
    contract(&spec, &rows);
    stays_visible(&json!({"type": "flow", "title": "T", "nodes": []}), &[]);
    contract(&lanes, &lane_rows);
}

#[test]
fn a_clean_flow_reports_nothing() {
    for (spec, rows) in [fixture(), lanes_fixture()] {
        for width in WIDTHS {
            let g = draw_at(&spec, &rows, width);
            assert!(g.checks.is_empty(), "{width}: unexpected {:?}", g.checks);
            assert!(g.dropped.is_empty(), "{width}: dropped {:?}", g.dropped);
        }
    }
}

#[test]
fn ranks_run_down_the_page_in_flow_order() {
    let (spec, rows) = decision();
    let g = draw_at(&spec, &rows, 720.0);
    let y = |n: &str| rect_of(&g, n).y;
    let order = [
        "Nightly run starts",
        "New files since the last run?",
        "Load the new files",
        "Run the checks",
        "Any check failed?",
        "Schema as yesterday?",
        "Write the tables",
        "Refresh the summaries",
        "Publish on?",
        "Dashboard extract",
        "Run ends",
    ];
    for w in order.windows(2) {
        assert!(y(w[0]) < y(w[1]), "{} is above {}", w[0], w[1]);
    }
}

#[test]
fn across_the_page_ranks_are_columns_and_lanes_are_bands() {
    let (mut spec, rows) = lanes_fixture();
    spec["direction"] = json!("right");
    let g = draw_at(&spec, &rows, 1440.0);
    let x = |n: &str| rect_of(&g, n).x;
    for w in ["Request", "Triage", "Fix", "Close"].windows(2) {
        assert!(x(w[0]) < x(w[1]), "{} is left of {}", w[0], w[1]);
    }
    assert!(
        (x("Fix") - x("Invoice")).abs() < 0.5,
        "cards of one rank share a column"
    );
    let bands: Vec<Rect> = g
        .items()
        .filter_map(|i| match i {
            Item::Rect {
                rect, role: Role::Band, ..
            } => Some(*rect),
            _ => None,
        })
        .collect();
    assert_eq!(bands.len(), 3);
    for (i, a) in bands.iter().enumerate() {
        for b in &bands[i + 1..] {
            assert!(!a.overlaps(b, 0.0), "lanes do not overlap");
        }
    }
    for (name, lane) in [("Request", 0), ("Pay", 0), ("Triage", 1), ("Close", 1), ("Invoice", 2)] {
        assert!(
            rect_of(&g, name).inside(&bands[lane], 0.0),
            "{name} sits in lane {lane}"
        );
    }
}

/// Every step is a card a reader can read: the drawing is never shrunk to nothing, which a
/// layout that runs away sideways once was, with no check and no failure.
fn legible(g: &Geometry, steps: usize, at: &str) {
    assert!(g.failure.is_none(), "{at}: {:?}", g.failure);
    let cards = boxes(g);
    assert_eq!(cards.len(), steps, "{at}: every step is drawn");
    for (name, c) in &cards {
        assert!(
            c.w >= 40.0 && c.h >= 14.0,
            "{at}: '{name}' is drawn {:.1} × {:.1} px",
            c.w,
            c.h
        );
    }
    assert!(g.height >= 100.0, "{at}: the figure is {:.0} px tall", g.height);
}

#[test]
fn a_flow_is_legible_at_every_width_from_rows_and_from_cards_down_and_across() {
    let (lanes, lane_rows) = lanes_fixture();
    let mut lanes_across = lanes.clone();
    lanes_across["direction"] = json!("right");
    // The same rows without lanes.
    let mut plain = lanes.clone();
    plain["encoding"].as_object_mut().unwrap().remove("lane");
    let (cards, card_rows) = fixture();
    let cases = [
        ("rows with lanes, down", &lanes, &lane_rows, 7),
        ("rows with lanes, across", &lanes_across, &lane_rows, 7),
        ("rows without lanes", &plain, &lane_rows, 7),
        ("cards and edges", &cards, &card_rows, 13),
    ];
    for (name, spec, rows, steps) in cases {
        let mut width = 280.0;
        while width <= 1440.0 {
            let g = draw_at(spec, rows, width);
            legible(&g, steps, &format!("{name} at {width} px"));
            width += 20.0;
        }
    }
}

#[test]
fn a_flow_from_rows_across_the_page_meets_the_shared_contract() {
    let (mut spec, rows) = lanes_fixture();
    spec["direction"] = json!("right");
    contract(&spec, &rows);
}

#[test]
fn a_flow_across_the_page_turns_down_where_it_does_not_fit() {
    let (mut spec, rows) = lanes_fixture();
    spec["direction"] = json!("right");
    let g = draw_at(&spec, &rows, 320.0);
    let (a, b) = (rect_of(&g, "Request"), rect_of(&g, "Triage"));
    assert!(b.y > a.bottom(), "on a phone the flow runs down");
}

#[test]
fn lines_are_orthogonal_and_clear_every_card() {
    for (spec, rows) in [fixture(), lanes_fixture()] {
        for width in WIDTHS {
            let g = draw_at(&spec, &rows, width);
            let cards = boxes(&g);
            for route in routes(&g) {
                for w in route.windows(2) {
                    let (a, b) = (w[0], w[1]);
                    for (name, r) in &cards {
                        let inner = r.inflate(-1.5);
                        let (lx, hx) = (a[0].min(b[0]), a[0].max(b[0]));
                        let (ly, hy) = (a[1].min(b[1]), a[1].max(b[1]));
                        assert!(
                            !(lx < inner.right() && hx > inner.x && ly < inner.bottom() && hy > inner.y),
                            "{width}: a line runs through {name}: {a:?} {b:?}"
                        );
                    }
                }
            }
            for (i, (_, a)) in cards.iter().enumerate() {
                for (_, b) in &cards[i + 1..] {
                    assert!(!a.overlaps(b, 0.5), "{width}: cards overlap");
                }
            }
        }
    }
}

#[test]
fn labels_never_touch_a_card_another_label_or_another_line() {
    let (spec, rows) = decision();
    for width in WIDTHS {
        let g = draw_at(&spec, &rows, width);
        let labels = label_boxes(&g);
        assert_eq!(
            labels.len(),
            11,
            "{width}: every edge label and the group label are drawn: {labels:?}"
        );
        let cards = boxes(&g);
        for (i, (text, r)) in labels.iter().enumerate() {
            for (name, c) in &cards {
                assert!(!r.overlaps(c, 0.0), "{width}: '{text}' touches {name}");
            }
            for (other, o) in &labels[i + 1..] {
                assert!(!r.overlaps(o, 0.0), "{width}: '{text}' touches '{other}'");
            }
        }
    }
}

#[test]
fn a_decision_answers_by_its_corners_and_the_failure_path_is_red() {
    let (spec, rows) = decision();
    let g = draw_at(&spec, &rows, 720.0);
    let corners = |r: Rect| {
        let [cx, cy] = r.center();
        [[cx, r.bottom()], [r.x, cy], [r.right(), cy]]
    };
    for question in [
        "New files since the last run?",
        "Any check failed?",
        "Schema as yesterday?",
        "Publish on?",
    ] {
        let r = rect_of(&g, question);
        let leaving: Vec<[f64; 2]> = routes(&g)
            .iter()
            .map(|route| route[0])
            .filter(|s| r.inflate(1.0).contains(*s))
            .collect();
        assert_eq!(leaving.len(), 2, "{question} has two answers: {leaving:?}");
        for s in leaving {
            assert!(
                corners(r)
                    .iter()
                    .any(|c| (c[0] - s[0]).abs() < 0.5 && (c[1] - s[1]).abs() < 0.5),
                "{question}: an answer leaves {s:?}, not a corner"
            );
        }
    }
    let red = g.items().any(|i| match i {
        Item::Line { stroke, .. } => {
            let c = stroke.color.oklch();
            c.c > 0.1 && (c.h < 50.0 || c.h > 340.0)
        }
        _ => false,
    });
    assert!(red, "the failure path takes the alert colour");
}

#[test]
fn a_loop_back_runs_dashed_round_the_outside() {
    let (spec, rows) = decision();
    let g = draw_at(&spec, &rows, 720.0);
    let wait = rect_of(&g, "Wait and look again");
    let fresh = rect_of(&g, "New files since the last run?");
    let back = g
        .items()
        .find_map(|i| match i {
            Item::Line { path, stroke, .. } if !stroke.dash.is_empty() => Some(path.clone()),
            _ => None,
        })
        .expect("a dashed loop");
    let xs: Vec<f64> = back
        .cmds
        .iter()
        .filter_map(|c| match c {
            Cmd::M { p } | Cmd::L { p } => Some(p[0]),
            _ => None,
        })
        .collect();
    let outside = xs.iter().any(|&x| x < wait.x.min(fresh.x) - 1.0)
        || xs.iter().any(|&x| x > wait.right().max(fresh.right()) + 1.0);
    assert!(outside, "the loop runs outside both cards: {xs:?}");
    assert!(check(&g, "cycle").is_none(), "a loop written dashed is meant");
}

#[test]
fn a_group_holds_its_cards_and_no_other() {
    let (spec, rows) = decision();
    for width in WIDTHS {
        let g = draw_at(&spec, &rows, width);
        let group = g
            .items()
            .find_map(|i| match i {
                Item::Rect {
                    rect,
                    role: Role::Band,
                    stroke: Some(_),
                    ..
                } => Some(*rect),
                _ => None,
            })
            .expect("a group box");
        let members = [
            "Run the checks",
            "Any check failed?",
            "Schema as yesterday?",
            "Write the tables",
            "Refresh the summaries",
            "Run fails",
        ];
        for (name, r) in boxes(&g) {
            if members.contains(&name.as_str()) {
                assert!(r.inside(&group, 0.0), "{width}: {name} is in the group");
            } else {
                assert!(!r.overlaps(&group, 0.0), "{width}: {name} is outside the group");
            }
        }
    }
}

#[test]
fn every_kind_has_its_own_outline() {
    let (spec, rows) = spec_rows(json!({
        "type": "flow", "title": "Kinds",
        "nodes": [
            {"title": "Begin", "kind": "start"},
            {"title": "Read the archive", "kind": "store"},
            {"title": "Clean the rows"},
            {"title": "Clean?", "kind": "decision"},
            {"title": "orders.parquet", "kind": "object"},
            {"title": "Finish", "kind": "end"}
        ],
        "edges": ["Begin -> Read the archive", "Read the archive -> Clean the rows", "Clean the rows -> Clean?",
                  "Clean? -> orders.parquet: yes", "orders.parquet -> Finish"]
    }));
    contract(&spec, &rows);
    let g = draw_at(&spec, &rows, 720.0);
    // A start is a pill: its corners round to half its height.
    let begin = rect_of(&g, "Begin");
    let outline = |name: &str| {
        let r = rect_of(&g, name);
        g.items()
            .find_map(|i| match i {
                Item::Area {
                    path, role: Role::Mark, ..
                } if path.bounds().is_some_and(|b| {
                    (b.x - r.x).abs() < 0.5
                        && (b.y - r.y).abs() < 0.5
                        && (b.w - r.w).abs() < 0.5
                        && (b.h - r.h).abs() < 0.5
                }) =>
                {
                    Some(path.clone())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("{name} has an outline"))
    };
    let curves = |name: &str| outline(name).cmds.iter().filter(|c| matches!(c, Cmd::C { .. })).count();
    assert_eq!(curves("Clean?"), 0, "a diamond has straight sides");
    assert!(curves("Begin") >= 4 && begin.h > 0.0);
    assert!(curves("Read the archive") >= 4, "a cylinder has curved ends");
    let lines = |name: &str| {
        let r = rect_of(&g, name);
        g.items()
            .filter(|i| matches!(i, Item::Line { path, .. } if path.bounds().is_some_and(|b| b.inside(&r, 0.5))))
            .count()
    };
    assert!(lines("orders.parquet") >= 2, "an object folds its corner");
}

#[test]
fn readouts_name_what_a_card_is_and_what_joins_it() {
    let (spec, rows) = decision();
    let g = draw(&spec, &rows);
    let o = observations(&g)
        .into_iter()
        .find(|o| o.readout.heading == "Any check failed?")
        .unwrap();
    assert_eq!(line(o, "Kind"), Some("Decision"));
    assert_eq!(line(o, "Group"), Some("One job · checks before any write"));
    assert_eq!(line(o, "Comes from"), Some("Run the checks"));
    assert_eq!(line(o, "Leads to"), Some("Schema as yesterday? (no), Run fails (yes)"));
}

#[test]
fn the_keyboard_walks_the_cards_in_reading_order() {
    let (spec, rows) = decision();
    let g = draw_at(&spec, &rows, 720.0);
    let order = richview_chart::hit::order(&g);
    let headings: Vec<String> = order
        .iter()
        .map(|t| g.panels[t.panel].observations[t.observation].readout.heading.clone())
        .collect();
    assert_eq!(headings[0], "Nightly run starts");
    assert_eq!(headings[1], "New files since the last run?");
    let ends = headings.iter().position(|h| h == "Run ends").unwrap();
    assert!(
        headings[..ends].iter().all(|h| !h.contains(" → ")),
        "cards come before lines"
    );
}

#[test]
fn the_summary_lists_the_steps_and_connections() {
    let (spec, rows) = decision();
    let g = draw(&spec, &rows);
    assert!(
        g.summary.starts_with(
            "Flow chart of 13 steps and 15 connections: Nightly run starts to New files since the last run?"
        )
    );
}

#[test]
fn the_same_block_gives_the_same_geometry() {
    let (spec, rows) = decision();
    assert_eq!(draw(&spec, &rows), draw(&spec, &rows));
}

#[test]
fn long_titles_wrap_inside_their_card() {
    let long = "Review the application with the whole committee before the end of the month";
    let (spec, rows) = spec_rows(json!({
        "type": "flow", "title": "T",
        "nodes": [{"title": "Open"}, {"title": long}, {"title": "Close"}],
        "edges": [format!("Open -> {long}"), format!("{long} -> Close")]
    }));
    for width in WIDTHS {
        let g = draw_at(&spec, &rows, width);
        let r = rect_of(&g, long);
        let texts: Vec<Rect> = g
            .items()
            .filter_map(|i| match i {
                Item::Text(t) if long.contains(t.text.as_str()) && t.text.len() > 3 => Some(t.bounds),
                _ => None,
            })
            .collect();
        assert!(texts.len() >= 2, "{width}: the title wraps");
        for t in texts {
            assert!(t.inside(&r, 0.5), "{width}: a line of the title stays in its card");
        }
    }
}

#[test]
fn a_bad_graph_is_reported_and_the_rest_draws() {
    let spec = json!({
        "type": "flow", "title": "T",
        "nodes": [{"title": "A"}, {"title": "B"}, {"title": "A", "id": "a"}, {"lines": "no title"}],
        "edges": ["A -> B", "A -> Nowhere", "B -> B", "A -> B", "garbled"],
        "groups": [{"label": "G", "nodes": ["Missing"]}]
    });
    let rows = flow_table_rows(&spec).unwrap();
    let g = draw(&spec, &rows);
    assert!(g.failure.is_none());
    let paths: Vec<&str> = g.checks.iter().filter_map(|c| c.path.as_deref()).collect();
    for p in [
        "chart.nodes[2].id",
        "chart.nodes[3].title",
        "chart.edges[1]",
        "chart.edges[2]",
        "chart.edges[3]",
        "chart.edges[4]",
        "chart.groups[0].nodes[0]",
    ] {
        assert!(paths.contains(&p), "{p} is reported: {paths:?}");
    }
    assert_eq!(boxes(&g).len(), 2);
}

#[test]
fn a_dense_graph_with_cycles_finishes_and_stays_in_the_figure() {
    let names: Vec<String> = (0..24).map(|i| format!("Step {i}")).collect();
    let mut edges: Vec<String> = Vec::new();
    for i in 0..24 {
        for d in [1, 3, 7] {
            edges.push(format!("Step {i} -> Step {}", (i + d) % 24));
        }
    }
    let (spec, rows) = spec_rows(json!({
        "type": "flow", "title": "T",
        "nodes": names.iter().map(|n| json!({"title": n})).collect::<Vec<_>>(),
        "edges": edges,
    }));
    for width in WIDTHS {
        let g = draw_at(&spec, &rows, width);
        assert!(g.failure.is_none());
        assert!(nonfinite(&g).is_none());
        let figure = Rect::new(0.0, 0.0, g.width, g.height);
        for item in g.items() {
            if let Some(b) = item.bounds() {
                assert!(b.inside(&figure, 1.0), "{width}: {item:?} is outside");
            }
        }
    }
}

#[test]
fn rows_with_nothing_to_draw_say_why() {
    let spec = json!({"type": "flow", "encoding": {"label": "step", "source": "from", "target": "to"}});
    let g = draw(&spec, &[json!({"other": 1})]);
    assert!(g.failure.as_deref().is_some_and(|f| f.contains("No row names a step")));
}

/// Card widths come in steps, so renaming a card or editing its lines within its step moves no
/// other card (FEA-CH-76).
#[test]
fn editing_one_card_moves_few_others() {
    let (spec, rows) = decision();
    for (node, field, text) in [
        (8, "title", "Rebuild the summaries"),
        (7, "lines", "replace today's partitions"),
    ] {
        for width in [720.0, 1440.0] {
            let before = boxes(&draw_at(&spec, &rows, width));
            let mut edited = spec.clone();
            edited["nodes"][node][field] = json!(text);
            let after = boxes(&draw_at(&edited, &flow_table_rows(&edited).unwrap(), width));
            let moved = before
                .iter()
                .filter(|(n, r)| {
                    let Some(new) = after.iter().find(|(m, _)| m == n) else {
                        return false;
                    };
                    (new.1.x - r.x).abs() > 0.5 || (new.1.y - r.y).abs() > 0.5
                })
                .count();
            assert_eq!(
                moved, 0,
                "{width}: editing {field} of card {node} moved {moved} other cards"
            );
        }
    }
}

#[test]
fn a_flow_over_a_limit_is_not_drawn_and_names_the_limit() {
    let spec = json!({"type": "flow", "encoding": {"label": "step", "source": "from", "target": "to"}});
    let rows: Vec<Value> = (0..151).map(|i| json!({"step": format!("s{i}")})).collect();
    let g = draw_at(&spec, &rows, 1440.0);
    assert!(g.failure.is_some());
    let c = check(&g, "limit-exceeded").expect("limit-exceeded");
    assert_eq!(c.level, Level::Error);
    assert_eq!(c.details["what"], "steps");
    assert_eq!(c.details["limit"], 150);
    assert!(observations(&g).is_empty());
}

#[test]
fn the_runtime_keeps_the_connection_labels_the_exact_rows_wrote() {
    let spec = json!({"type": "flow", "encoding": {"label": "step", "source": "from", "target": "to", "y": "note"}});
    let rows: Vec<Value> = vec![
        json!({"step": "A"}),
        json!({"step": "B"}),
        json!({"step": "C"}),
        json!({"from": "A", "to": "B", "note": 1000}),
        json!({"from": "B", "to": "C", "note": 100.04}),
        json!({"from": "A", "to": "C", "note": 0.05}),
    ];
    runtime_matches_native(&spec, &rows);
}
