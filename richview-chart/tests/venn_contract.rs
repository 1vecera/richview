//! Venn diagrams: circle areas from the totals, pairs at the distance that gives their overlap,
//! regions found by outline, and the contract every type shares (FEA-CH-75).

use super::common;

use common::*;
use richview_chart::geometry::{Geometry, Item};
use richview_chart::hit;
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/venn.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn spec() -> Value {
    json!({"type":"venn","title":"T","encoding":{"sets":"s","y":"v"}})
}

fn sets(vs: &[(&str, f64)]) -> Vec<Value> {
    vs.iter().map(|(s, v)| json!({"s": s, "v": v})).collect()
}

/// The circles of a diagram as (centre, radius, observation index).
fn circles(g: &Geometry) -> Vec<([f64; 2], f64, usize)> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| match i {
            Item::Area {
                path,
                series: Some(_),
                obs: Some(o),
                ..
            } => {
                let b = path.bounds()?;
                Some((b.center(), b.w / 2.0, *o))
            }
            _ => None,
        })
        .collect()
}

/// The area of every overlap outline, with its observation index.
fn lenses(g: &Geometry) -> Vec<(f64, usize)> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| match i {
            Item::Area {
                path,
                series: None,
                obs: Some(o),
                ..
            } => {
                let ring = path.flatten(0.5).into_iter().next()?;
                let n = ring.len();
                let twice: f64 = (0..n)
                    .map(|k| ring[k][0] * ring[(k + 1) % n][1] - ring[(k + 1) % n][0] * ring[k][1])
                    .sum();
                Some((twice.abs() / 2.0, *o))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
    gallery("venn", &spec, &rows);
}

#[test]
fn circle_areas_follow_the_totals_and_overlaps_follow_their_sizes() {
    let (spec, rows) = fixture();
    for width in WIDTHS {
        let g = draw_at(&spec, &rows, width);
        let panel = &g.panels[0];
        let cs = circles(&g);
        assert_eq!(cs.len(), 3);
        // Area per unit of total, the same for every set.
        let totals = |o: usize| rows[panel.observations[o].row.unwrap()]["readers"].as_f64().unwrap();
        let record = &g.derived.iter().find(|d| d.key == "venn.solved").unwrap().value;
        let grid: richview_chart::payload::Quant = serde_json::from_value(record["area_grid"].clone()).unwrap();
        let step = (grid.max - grid.min) / f64::from(grid.steps);
        let largest_total = cs.iter().map(|(_, _, o)| totals(*o)).fold(0.0f64, f64::max);
        let largest_radius = cs.iter().map(|(_, r, _)| *r).fold(0.0f64, f64::max);
        for (_, r, o) in &cs {
            let drawn = (r / largest_radius).powi(2);
            assert!(
                (drawn - totals(*o) / largest_total).abs() <= step,
                "{width}: set area differs by more than one recorded step"
            );
        }
        let k = largest_radius.powi(2) / largest_total;
        // A pair's lens has its size times the same unit, within what a polygon allows.
        for (area, o) in lenses(&g) {
            let ob = &panel.observations[o];
            if ob.readout.lines.len() == 3 {
                let want = std::f64::consts::PI * k * totals(o);
                assert!(
                    (area - want).abs() < want * 0.03,
                    "{width}: {} is {area}, wanted {want}",
                    ob.readout.heading
                );
            }
        }
    }
}

#[test]
fn the_pointer_finds_the_smallest_region_by_its_outline() {
    let (spec, rows) = fixture();
    let g = draw_at(&spec, &rows, 1440.0);
    let panel = &g.panels[0];
    let heading = |p: [f64; 2]| {
        let t = hit::at(&g, p)?;
        Some(g.panels[t.panel].observations[t.observation].readout.heading.clone())
    };
    // Every observation is found at its own anchor.
    for (i, o) in panel.observations.iter().enumerate() {
        let t = hit::at(&g, o.at).unwrap();
        assert_eq!(t.panel, 0);
        // A set's anchor may lie in an overlap only when the overlap is all that is left of it.
        assert!(t.observation >= i || panel.observations[t.observation].readout.heading.contains(" and "));
    }
    let cs = circles(&g);
    let outside = [panel.plot.x + 1.0, panel.plot.y + 1.0];
    assert!(
        cs.iter()
            .all(|(c, r, _)| (c[0] - outside[0]).hypot(c[1] - outside[1]) > *r)
    );
    assert_eq!(heading(outside), None);
    let triple = panel
        .observations
        .iter()
        .find(|o| o.readout.heading == "Email, App and Web")
        .unwrap();
    assert_eq!(heading(triple.at).as_deref(), Some("Email, App and Web"));
}

#[test]
fn readouts_give_the_authors_sizes_and_derived_shares() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let obs = observations(&g);
    let email = obs.iter().find(|o| o.readout.heading == "Email").unwrap();
    assert_eq!(line(email, "Readers"), Some("620"));
    assert!(email.readout.lines.iter().all(|l| !l.derived));
    let pair = obs.iter().find(|o| o.readout.heading == "Email and App").unwrap();
    assert_eq!(line(pair, "Readers"), Some("180"));
    let share = pair.readout.lines.iter().find(|l| l.name == "Share of App").unwrap();
    assert!(share.derived && share.value.starts_with("37.5"), "{}", share.value);
    assert_eq!(obs.len(), 7);
}

#[test]
fn two_sets_nested_sets_and_apart_sets_draw() {
    let two = sets(&[("A", 10.0), ("B", 6.0), ("A&B", 3.0)]);
    let g = draw(&spec(), &two);
    assert_eq!(circles(&g).len(), 2);
    assert_eq!(lenses(&g).len(), 1);
    contract(&spec(), &two);
    let nested = sets(&[("A", 10.0), ("B", 4.0), ("A,B", 4.0)]);
    contract(&spec(), &nested);
    let apart = sets(&[("A", 10.0), ("B", 6.0)]);
    let g = draw(&spec(), &apart);
    assert!(lenses(&g).is_empty());
    let cs = circles(&g);
    let d = (cs[0].0[0] - cs[1].0[0]).hypot(cs[0].0[1] - cs[1].0[1]);
    assert!(d > cs[0].1 + cs[1].1, "the circles are apart");
    contract(&spec(), &apart);
    let zero = sets(&[("A", 10.0), ("B", 6.0), ("A&B", 0.0)]);
    assert!(lenses(&draw(&spec(), &zero)).is_empty());
}

#[test]
fn names_come_as_lists_or_with_commas_and_spaces() {
    let rows = vec![
        json!({"s": "Mobile", "v": 10}),
        json!({"s": "Web", "v": 8}),
        json!({"s": ["Mobile", "Web"], "v": 3}),
    ];
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_none());
    assert_eq!(lenses(&g).len(), 1);
    let rows = sets(&[("Mobile", 10.0), ("Web", 8.0), (" Web , Mobile ", 3.0)]);
    assert_eq!(lenses(&draw(&spec(), &rows)).len(), 1);
}

#[test]
fn a_set_name_is_drawn_inside_its_circle_or_reported() {
    let (spec, rows) = fixture();
    for width in WIDTHS {
        let g = draw_at(&spec, &rows, width);
        let names = g
            .items()
            .filter(|i| matches!(i, Item::Text(t) if t.role == richview_chart::geometry::Role::Label))
            .count();
        let dropped = g
            .dropped
            .iter()
            .filter(|d| d.kind.as_deref() == Some("set-name"))
            .count();
        assert_eq!(names + dropped, 3, "{width}");
    }
    assert!(draw(&spec, &rows).legend.is_some());
}

#[test]
fn sources_that_cannot_make_a_diagram_say_why() {
    stays_visible(&spec(), &[]);
    stays_visible(&spec(), &sets(&[("A", 5.0)]));
    stays_visible(&spec(), &sets(&[("A", 5.0), ("B", 5.0), ("C", 5.0), ("D", 5.0)]));
    // An overlap larger than its set, or than a pair it lies in.
    let g = draw(&spec(), &sets(&[("A", 5.0), ("B", 4.0), ("A&B", 4.5)]));
    assert!(g.failure.as_deref().unwrap_or_default().contains("larger"));
    assert!(check(&g, "source-error").is_some());
    let g = draw(
        &spec(),
        &sets(&[
            ("A", 9.0),
            ("B", 9.0),
            ("C", 9.0),
            ("A&B", 3.0),
            ("A&C", 3.0),
            ("B&C", 1.0),
            ("A&B&C", 2.0),
        ]),
    );
    assert!(g.failure.is_some());
    let s = json!({"type":"venn","title":"T","encoding":{"sets":"s"}});
    let g = draw(&s, &sets(&[("A", 1.0), ("B", 1.0)]));
    assert!(g.failure.is_some());
    assert!(check(&g, "source-error").is_some());
}

#[test]
fn unknown_sets_missing_and_negative_values_are_left_out_and_bad_text_fails() {
    let mut rows = sets(&[("A", 5.0), ("B", 4.0), ("A&Z", 2.0), ("B", -1.0)]);
    rows.push(json!({"s": null, "v": 3}));
    rows.push(json!({"s": "A", "v": null}));
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_none());
    assert_eq!(circles(&g).len(), 2);
    assert!(lenses(&g).is_empty());
    assert!(check(&g, "excluded-observations").is_some());
    rows.push(json!({"s": "A&B", "v": "many"}));
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").is_some());
}

#[test]
fn extreme_ranges_stay_finite() {
    let g = draw(&spec(), &sets(&[("A", 1e300), ("B", 1e299), ("A&B", 1e298)]));
    assert!(nonfinite(&g).is_none());
    assert!(g.failure.is_none());
    let g = draw(&spec(), &sets(&[("A", 1e-300), ("B", 3e-300), ("A&B", 1e-300)]));
    assert!(nonfinite(&g).is_none());
    let g = draw(&spec(), &sets(&[("A", 1e12), ("B", 1.0)]));
    assert!(nonfinite(&g).is_none());
}

#[test]
fn facets_share_one_scale() {
    let mut s = spec();
    s["encoding"]["facet"] = json!("f");
    let mut rows = Vec::new();
    for (f, a) in [("p", 16.0), ("q", 4.0)] {
        for (set, v) in [("A", a), ("B", 4.0)] {
            rows.push(json!({"s": set, "v": v, "f": f}));
        }
    }
    let g = draw_at(&s, &rows, 1440.0);
    assert_eq!(g.panels.len(), 2);
    let radius = |panel: usize, set: &str| {
        let o = &g.panels[panel];
        let obs = o.observations.iter().find(|x| x.readout.heading == set).unwrap();
        let area = obs.area.unwrap();
        area.w / 2.0
    };
    assert!((radius(0, "B") - radius(1, "B")).abs() < 1e-6);
    let record = &g.derived.iter().find(|d| d.key == "venn.solved").unwrap().value;
    let grid: richview_chart::payload::Quant = serde_json::from_value(record["area_grid"].clone()).unwrap();
    assert!(((radius(1, "A") / radius(0, "A")).powi(2) - 0.25).abs() <= (grid.max - grid.min) / f64::from(grid.steps));
    contract(&s, &rows);
}

#[test]
fn three_pair_overlaps_that_cannot_meet_warn_and_the_note_says_so() {
    let rows = sets(&[
        ("A", 100.0),
        ("B", 100.0),
        ("C", 100.0),
        ("A & B", 50.0),
        ("A & C", 50.0),
        ("B & C", 0.0),
    ]);
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_none());
    let warning = check(&g, "points-overlap").expect("a warning for overlaps that do not fit");
    assert_eq!(warning.level, richview_chart::diag::Level::Warning);
    assert!(g.note.as_deref().unwrap_or_default().contains("do not all fit"));
    let fits = sets(&[
        ("A", 100.0),
        ("B", 100.0),
        ("C", 100.0),
        ("A & B", 30.0),
        ("A & C", 30.0),
        ("B & C", 30.0),
    ]);
    assert!(check(&draw(&spec(), &fits), "points-overlap").is_none());
}

#[test]
fn the_runtime_keeps_the_placement_the_exact_rows_solved() {
    // Sizes closer than a grid step: the overlap of A and B is nearly all of B, and a pair is 0.05.
    let rows = sets(&[
        ("A", 1000.0),
        ("B", 500.1),
        ("C", 300.5),
        ("A&B", 500.04),
        ("A&C", 100.01),
        ("B&C", 0.05),
        ("A&B&C", 0.0),
    ]);
    runtime_matches_native(&spec(), &rows);
}
