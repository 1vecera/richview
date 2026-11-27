//! Flow charts: cards in ranks along the flow, right-angled lines that never cross a card, labels
//! that never collide, groups that keep their cards together, loops that go round the outside,
//! the diagnostics of a bad graph, and the shared contract every type meets (FEA-CH-76,
//! FEA-CH-75).

use super::common;

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

/// The example's questions, and the cards of its main path from top to bottom: the first-listed
/// answer of each question.
const QUESTIONS: [&str; 4] = [
    "New files since the last run?",
    "Any check failed?",
    "Schema as yesterday?",
    "Publish on?",
];
const SPINE: [&str; 11] = [
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

fn points(path: &richview_chart::geometry::Path) -> Vec<[f64; 2]> {
    path.cmds
        .iter()
        .filter_map(|c| match c {
            Cmd::M { p } | Cmd::L { p } | Cmd::C { p, .. } => Some(*p),
            Cmd::Z => None,
        })
        .collect()
}

/// Each solid line with the cards it joins: (from, to, corners).
fn joins(g: &Geometry) -> Vec<(String, String, Vec<[f64; 2]>)> {
    g.panels
        .iter()
        .flat_map(|panel| {
            panel.items.iter().filter_map(move |i| match i {
                Item::Line {
                    path,
                    stroke,
                    role: Role::Mark,
                    obs: Some(o),
                    ..
                } if stroke.dash.is_empty() => {
                    let heading = &panel.observations.get(*o)?.readout.heading;
                    let (a, b) = heading.split_once(" → ")?;
                    Some((a.to_string(), b.to_string(), points(path)))
                }
                _ => None,
            })
        })
        .collect()
}

/// Whether `r` keeps `d` clear of a card: of a question's diamond, of any other card's box.
fn clear_of_card(name: &str, card: &Rect, r: &Rect, d: f64) -> bool {
    if !name.ends_with('?') {
        return !card.overlaps(r, -d + 1e-6);
    }
    let (a, b) = (card.w / 2.0, card.h / 2.0);
    let [cx, cy] = card.center();
    let dx = (r.x - cx).max(cx - r.right()).max(0.0);
    let dy = (r.y - cy).max(cy - r.bottom()).max(0.0);
    dx / a + dy / b >= 1.0 + (d - 1e-6) * (1.0 / (a * a) + 1.0 / (b * b)).sqrt()
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
fn a_lanes_cards_stand_on_its_centre_line() {
    let (spec, rows) = lanes_fixture();
    for width in [390.0, 632.0, 1440.0] {
        let g = draw_at(&spec, &rows, width);
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
        // A card alone in its rank within its lane, and cards that share a rank as a group.
        let lanes: [(usize, &[&[&str]]); 3] = [
            (0, &[&["Request"], &["Pay"]]),
            (1, &[&["Triage"], &["Fix"], &["Close"]]),
            (2, &[&["Invoice", "Refund"]]),
        ];
        for (lane, ranks) in lanes {
            let line = bands[lane].center()[0];
            for names in ranks {
                let boxes: Vec<Rect> = names.iter().map(|n| rect_of(&g, n)).collect();
                let left = boxes.iter().map(|b| b.x).fold(f64::INFINITY, f64::min);
                let right = boxes.iter().map(Rect::right).fold(f64::NEG_INFINITY, f64::max);
                assert!(
                    ((left + right) / 2.0 - line).abs() < 0.5,
                    "{width}: {names:?} stand at {:.1}, the lane's centre line at {line:.1}",
                    (left + right) / 2.0
                );
            }
        }
    }
}

/// The sizes of every text of a drawing: the smallest and the largest.
fn type_sizes(g: &Geometry) -> (f64, f64) {
    let sizes: Vec<f64> = g
        .items()
        .filter_map(|i| match i {
            Item::Text(t) => Some(t.size),
            _ => None,
        })
        .collect();
    (
        sizes.iter().copied().fold(f64::INFINITY, f64::min),
        sizes.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    )
}

/// Paper gives way before type: on a phone a flow's titles are never more than 2 px under the
/// style's, and no text more than 1 px under its smallest. The lanes flow once drew its titles
/// at 10 px and its labels under 9 px at 390 px.
#[test]
fn a_flows_type_holds_its_floor_on_a_phone() {
    for (name, (spec, rows)) in [("cards", fixture()), ("lanes", lanes_fixture())] {
        let (small, title) = type_sizes(&draw_at(&spec, &rows, 1440.0));
        for width in [320.0, 360.0, 390.0, 430.0] {
            let (least, most) = type_sizes(&draw_at(&spec, &rows, width));
            assert!(
                least >= small - 1.0 - 0.05,
                "{name} at {width}: text of {least:.2} px, the floor is {:.0}",
                small - 1.0
            );
            assert!(
                most >= title - 2.0 - 0.05,
                "{name} at {width}: titles of {most:.2} px, the floor is {:.0}",
                title - 2.0
            );
        }
    }
    // Three lanes are three columns at 390 px, and fit it at the style's own sizes.
    let (spec, rows) = lanes_fixture();
    assert_eq!(
        type_sizes(&draw_at(&spec, &rows, 390.0)),
        type_sizes(&draw_at(&spec, &rows, 1440.0))
    );
}

/// Where its lanes do not fit side by side, a lane is one column: two of its cards that share
/// a rank stand one under the other in source order. With room they stand side by side.
#[test]
fn on_a_phone_a_lane_is_one_column() {
    let (spec, rows) = lanes_fixture();
    for width in [320.0, 390.0] {
        let g = draw_at(&spec, &rows, width);
        let (invoice, refund) = (rect_of(&g, "Invoice"), rect_of(&g, "Refund"));
        assert!(
            (invoice.center()[0] - refund.center()[0]).abs() < 0.5,
            "{width}: Invoice and Refund stand in one column"
        );
        assert!(invoice.bottom() < refund.y, "{width}: Invoice stands above Refund");
    }
    let g = draw_at(&spec, &rows, 1440.0);
    let (invoice, refund) = (rect_of(&g, "Invoice"), rect_of(&g, "Refund"));
    assert!((invoice.y - refund.y).abs() < 0.5 && invoice.right() < refund.x);
}

/// Lanes stand one lane's space apart, no farther: the gaps between the bands are alike.
#[test]
fn lanes_stand_evenly_apart() {
    let (spec, rows) = lanes_fixture();
    for width in [320.0, 390.0, 632.0, 1440.0] {
        let g = draw_at(&spec, &rows, width);
        let bands: Vec<Rect> = g
            .items()
            .filter_map(|i| match i {
                Item::Rect {
                    rect, role: Role::Band, ..
                } => Some(*rect),
                _ => None,
            })
            .collect();
        let gaps: Vec<f64> = bands.windows(2).map(|w| w[1].x - w[0].right()).collect();
        assert!(
            (gaps[0] - gaps[1]).abs() < 0.5 && gaps[0] > 0.0,
            "{width}: the lanes stand {gaps:?} apart"
        );
    }
}

/// A line's run down the page: (x, top, bottom).
type Run = (f64, f64, f64);

/// On a phone the lanes flow's lines stay apart and straight: no line runs down on top of
/// another's, and a long line does not step aside by a few pixels on its way.
#[test]
fn a_lanes_flows_lines_neither_share_a_run_nor_step_aside() {
    let (spec, rows) = lanes_fixture();
    for width in [320.0, 390.0] {
        let g = draw_at(&spec, &rows, width);
        // Each line's runs down the page: (x, top, bottom).
        let lines: Vec<(String, String, Vec<Run>)> = joins(&g)
            .into_iter()
            .map(|(a, b, pts)| {
                let downs = pts
                    .windows(2)
                    .filter(|w| (w[0][0] - w[1][0]).abs() < 0.01 && (w[0][1] - w[1][1]).abs() > 0.01)
                    .map(|w| (w[0][0], w[0][1].min(w[1][1]), w[0][1].max(w[1][1])))
                    .collect();
                (a, b, downs)
            })
            .collect();
        for (i, (a, b, downs)) in lines.iter().enumerate() {
            for w in downs.windows(2) {
                let step = (w[0].0 - w[1].0).abs();
                assert!(step < 0.01 || step > 6.0, "{width}: {a} → {b} steps aside {step:.1} px");
            }
            for (c, d, others) in &lines[i + 1..] {
                // Lines into one card may share their last run.
                if b == d {
                    continue;
                }
                for (x, top, bottom) in downs {
                    for (ox, otop, obottom) in others {
                        let shared = bottom.min(*obottom) - top.max(*otop);
                        assert!(
                            (x - ox).abs() > 0.5 || shared < 1.0,
                            "{width}: {a} → {b} and {c} → {d} share {shared:.1} px of a run at x = {x:.1}"
                        );
                    }
                }
            }
        }
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
            // At least 4 px from every card on the page, also where the flow is shrunk to fit.
            for (name, c) in &cards {
                assert!(
                    clear_of_card(name, c, r, 4.0),
                    "{width}: '{text}' is within 4 px of {name}"
                );
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
fn the_main_path_runs_straight_down_one_column_with_the_failure_to_its_right() {
    let (spec, rows) = decision();
    for width in WIDTHS {
        let g = draw_at(&spec, &rows, width);
        let x = rect_of(&g, SPINE[0]).center()[0];
        for name in SPINE {
            let c = rect_of(&g, name).center()[0];
            assert!(
                (c - x).abs() < 0.5,
                "{width}: {name} stands at {c:.1}, the main path at {x:.1}"
            );
        }
        assert!(
            rect_of(&g, "Run fails").x > x,
            "{width}: the failure stands right of the main path"
        );
    }
}

#[test]
fn a_question_goes_on_by_its_bottom_corner_and_branches_by_the_side_corner_facing_the_branch() {
    let (spec, rows) = decision();
    for width in WIDTHS {
        let g = draw_at(&spec, &rows, width);
        let labels = label_boxes(&g);
        for question in QUESTIONS {
            let r = rect_of(&g, question);
            let [cx, cy] = r.center();
            let leaving: Vec<Vec<[f64; 2]>> = joins(&g).into_iter().filter(|j| j.0 == question).map(|j| j.2).collect();
            let down = leaving
                .iter()
                .filter(|l| (l[0][0] - cx).abs() < 0.5 && (l[0][1] - r.bottom()).abs() < 0.5)
                .count();
            let side: Vec<&Vec<[f64; 2]>> = leaving
                .iter()
                .filter(|l| {
                    (l[0][1] - cy).abs() < 0.5 && ((l[0][0] - r.x).abs() < 0.5 || (l[0][0] - r.right()).abs() < 0.5)
                })
                .collect();
            assert_eq!(down, 1, "{width}: {question} goes on by its bottom corner: {leaving:?}");
            assert_eq!(
                side.len(),
                1,
                "{width}: {question} branches by a side corner: {leaving:?}"
            );
            let s = side[0];
            let left = (s[0][0] - r.x).abs() < 0.5;
            assert!(
                if left { s[1][0] < s[0][0] } else { s[1][0] > s[0][0] },
                "{width}: {question}'s branch runs away from its corner"
            );
            // Its name stands beside that corner.
            let corner = s[0];
            let near = labels.iter().any(|(_, b)| {
                let dx = (b.x - corner[0]).max(corner[0] - b.right()).max(0.0);
                let dy = (b.y - corner[1]).max(corner[1] - b.bottom()).max(0.0);
                dx < 48.0 && dy < 8.0 && if left { b.right() <= corner[0] } else { b.x >= corner[0] }
            });
            assert!(near, "{width}: {question}'s branch is named beside its corner");
        }
    }
}

#[test]
fn a_question_is_two_and_a_half_times_as_wide_as_tall_with_its_text_12_px_inside() {
    let (spec, rows) = decision();
    let g = draw_at(&spec, &rows, 1440.0);
    for question in QUESTIONS {
        let r = rect_of(&g, question);
        assert!((r.w / r.h - 2.5).abs() < 0.01, "{question}: {} × {}", r.w, r.h);
        let (a, b) = (r.w / 2.0, r.h / 2.0);
        let [cx, cy] = r.center();
        let reach = 1.0 - 12.0 * (1.0 / (a * a) + 1.0 / (b * b)).sqrt();
        let lines: Vec<Rect> = g
            .items()
            .filter_map(|i| match i {
                Item::Text(t) if r.contains(t.bounds.center()) && question.contains(t.text.as_str()) => Some(t.bounds),
                _ => None,
            })
            .collect();
        assert!(!lines.is_empty(), "{question} is written in its diamond");
        for t in lines {
            let dx = (t.x - cx).abs().max((t.right() - cx).abs());
            let dy = (t.y - cy).abs().max((t.bottom() - cy).abs());
            assert!(
                dx / a + dy / b <= reach + 0.02,
                "{question}: a line of its text comes within 12 px of the outline"
            );
        }
    }
}

#[test]
fn a_line_between_neighbouring_ranks_bends_at_most_twice_and_never_jogs() {
    let (spec, rows) = decision();
    for width in WIDTHS {
        let g = draw_at(&spec, &rows, width);
        let cards = boxes(&g);
        for (a, b, pts) in joins(&g) {
            let (ra, rb) = (rect_of(&g, &a), rect_of(&g, &b));
            // Straight runs and the rounded corners between them.
            let runs: Vec<([f64; 2], [f64; 2])> = pts
                .windows(2)
                .map(|w| (w[0], w[1]))
                .filter(|(p, q)| (p[0] - q[0]).hypot(p[1] - q[1]) > 0.01)
                .collect();
            let axis = |(p, q): &([f64; 2], [f64; 2])| {
                if (p[0] - q[0]).abs() < 0.01 {
                    Some('v')
                } else if (p[1] - q[1]).abs() < 0.01 {
                    Some('h')
                } else {
                    None
                }
            };
            let mut bends = runs.iter().filter(|r| axis(r).is_none()).count();
            bends += runs
                .windows(2)
                .filter(|w| matches!((axis(&w[0]), axis(&w[1])), (Some(x), Some(y)) if x != y))
                .count();
            let between = cards
                .iter()
                .any(|(_, c)| c.y >= ra.bottom() - 0.5 && c.bottom() <= rb.y + 0.5);
            if rb.y > ra.bottom() && !between {
                assert!(bends <= 2, "{width}: {a} → {b} bends {bends} times");
            }
            // Down runs that differ by 2 px or less are drawn as one.
            let xs: Vec<f64> = runs.iter().filter(|r| axis(r) == Some('v')).map(|r| r.0[0]).collect();
            for w in xs.windows(2) {
                let d = (w[0] - w[1]).abs();
                assert!(!(d > 0.01 && d <= 2.0), "{width}: {a} → {b} jogs {d:.2} px");
            }
        }
    }
}

/// A bypass leaves the main path and comes back to it further down: it runs beside the path and
/// meets its target's facing side at the middle, in two bends, and the top keeps one arrow.
#[test]
fn a_bypass_meets_its_main_path_target_by_the_facing_side_in_two_bends() {
    let (spec, rows) = decision();
    for width in [320.0, 390.0, 720.0, 1440.0] {
        let g = draw_at(&spec, &rows, width);
        let (from, to) = (rect_of(&g, "Publish on?"), rect_of(&g, "Run ends"));
        let all = joins(&g);
        let pts = &all
            .iter()
            .find(|j| j.0 == "Publish on?" && j.1 == "Run ends")
            .unwrap_or_else(|| panic!("{width}: no bypass"))
            .2;
        let (first, last) = (pts[0], pts[pts.len() - 1]);
        assert!(
            (first[0] - from.right()).abs() < 0.5 && (first[1] - from.center()[1]).abs() < 0.5,
            "{width}: the bypass leaves by the right corner: {pts:?}"
        );
        // A line stops at the base of its arrowhead, 6 px short of the card.
        assert!(
            last[0] > to.right() - 0.5 && last[0] < to.right() + 6.5 && (last[1] - to.center()[1]).abs() < 0.5,
            "{width}: the bypass meets the middle of the facing side: {pts:?}"
        );
        // Out, down beside the main path, in: every run is level or upright, and two corners.
        let runs: Vec<char> = pts
            .windows(2)
            .filter(|w| (w[0][0] - w[1][0]).hypot(w[0][1] - w[1][1]) > 0.01)
            .filter_map(|w| {
                if (w[0][1] - w[1][1]).abs() < 0.01 {
                    Some('h')
                } else if (w[0][0] - w[1][0]).abs() < 0.01 {
                    Some('v')
                } else {
                    None
                }
            })
            .collect();
        let mut turns = runs.clone();
        turns.dedup();
        assert_eq!(turns, vec!['h', 'v', 'h'], "{width}: the bypass bends twice: {pts:?}");
        let down = pts.iter().map(|q| q[0]).fold(f64::NEG_INFINITY, f64::max);
        assert!(down > to.right() + 7.0, "{width}: the arrow has room beside the card");
        // The top keeps the main path's arrow alone.
        let onto_top = all
            .iter()
            .filter(|j| {
                let end = j.2[j.2.len() - 1];
                j.1 == "Run ends" && end[1] < to.y + 0.5 && end[1] > to.y - 6.5
            })
            .count();
        assert_eq!(onto_top, 1, "{width}: one arrow on the top of Run ends");
    }
}

/// Nothing draws on the chart's edge: every card, line, border, band and label keeps 4 px from it.
#[test]
fn every_mark_keeps_4_px_from_the_charts_edge() {
    let (lanes, lane_rows) = lanes_fixture();
    let (mut lanes_across, _) = lanes_fixture();
    lanes_across["direction"] = json!("right");
    let cases = [
        ("cards", fixture()),
        ("lanes", (lanes, lane_rows.clone())),
        ("lanes across", (lanes_across, lane_rows)),
    ];
    for (name, (spec, rows)) in cases {
        for width in (280..=1440).step_by(20) {
            let width = f64::from(width);
            let g = draw_at(&spec, &rows, width);
            let inner = Rect::new(4.0, 4.0, g.width - 8.0, g.height - 8.0);
            // Measured to each outline; a painted extent adds half its stroke, 1 px at most.
            for item in g.items() {
                if let Some(b) = item.bounds() {
                    assert!(b.inside(&inner, 1.0), "{name} at {width}: {item:?} reaches the edge");
                }
            }
        }
    }
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

/// A main-path card beside a group, in a rank the group spans, is placed like any other card:
/// held on the column, it pushed the group's other cards thousands of pixels away.
#[test]
fn a_main_path_card_beside_a_group_in_a_rank_the_group_spans_keeps_the_drawing_compact() {
    let (spec, rows) = spec_rows(json!({
        "type": "flow", "title": "T",
        "nodes": [
            {"title": "Start", "kind": "start"},
            {"id": "build", "title": "Build and test"},
            {"id": "green", "title": "Tests green?", "kind": "decision"},
            {"id": "review", "title": "Review approved?", "kind": "decision"},
            {"id": "safe", "title": "Migration reversible?", "kind": "decision"},
            {"id": "fix", "title": "Fix and push again", "tone": "muted"},
            {"id": "hold", "title": "Release held", "tone": "alert"},
            {"id": "stage", "title": "Deploy to staging"},
            {"id": "done", "title": "Live", "kind": "end"}
        ],
        "groups": [{"label": "Gate", "nodes": ["green", "review", "safe", "hold"]}],
        "edges": [
            "Start -> build", "build -> green", "green -> review: yes", "green -> fix: no",
            "review -> safe: yes", "review -> fix: no", "safe -> stage: yes",
            {"edge": "safe -> hold: no", "tone": "alert"}, "fix ..> build: again", "stage -> done"
        ]
    }));
    gallery("flow-group-rank", &spec, &rows);
    // Four columns wide, it needs a report's width; narrower, it is shrunk whole to fit.
    let mut width = 720.0;
    while width <= 1440.0 {
        legible(&draw_at(&spec, &rows, width), 9, &format!("{width} px"));
        width += 20.0;
    }
    // On a desktop it is drawn at its own size, not shrunk to fit a runaway layout.
    let g = draw_at(&spec, &rows, 1440.0);
    assert!(rect_of(&g, "Deploy to staging").w > 100.0);
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
