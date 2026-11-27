//! Normal quantile-quantile charts: theoretical quantiles at the plotting positions, the
//! reference line through the quartile pairs, point hit testing, groups, facets, and the shared
//! contract every type meets (FEA-CH-14, FEA-CH-75).

use super::common;

use common::*;
use richview_chart::geometry::{Cmd, Geometry, Item, Role};
use richview_chart::hit;
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/qq.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn spec(extra: Value) -> Value {
    let mut s = json!({"type":"qq","title":"T","readout_digits":6,"encoding":{"y":"v"}});
    for (k, v) in extra.as_object().unwrap() {
        s[k] = v.clone();
    }
    s
}

fn values(v: &[f64]) -> Vec<Value> {
    v.iter().map(|x| json!({ "v": x })).collect()
}

fn number(text: &str) -> f64 {
    text.replace('\u{2212}', "-")
        .parse()
        .unwrap_or_else(|_| panic!("{text}"))
}

fn points(g: &Geometry) -> usize {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter(|i| matches!(i, Item::Point { role: Role::Mark, .. }))
        .count()
}

fn reference_lines(g: &Geometry) -> Vec<[[f64; 2]; 2]> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| match i {
            Item::Line {
                path,
                role: Role::Annotation,
                ..
            } => {
                let p: Vec<[f64; 2]> = path
                    .cmds
                    .iter()
                    .map(|c| match c {
                        Cmd::M { p } | Cmd::L { p } => *p,
                        other => panic!("{other:?}"),
                    })
                    .collect();
                Some([p[0], p[1]])
            }
            _ => None,
        })
        .collect()
}

#[test]
fn the_golden_sample_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
}

#[test]
fn quantiles_sit_at_the_plotting_positions() {
    // 1..=5 at (i - 0.5) / 5 = 0.1, 0.3, 0.5, 0.7, 0.9.
    let g = draw(&spec(json!({})), &values(&[3.0, 1.0, 5.0, 2.0, 4.0]));
    let obs = observations(&g);
    assert_eq!(obs.len(), 5);
    let want = [-1.281_551_6, -0.524_400_5, 0.0, 0.524_400_5, 1.281_551_6];
    for (k, (o, z)) in obs.iter().zip(want).enumerate() {
        assert!(
            (number(line(o, "Theoretical quantile").unwrap()) - z).abs() < 1e-5,
            "{k}"
        );
        assert_eq!(number(line(o, "V").unwrap()), k as f64 + 1.0);
        assert_eq!(line(o, "Rank").unwrap(), format!("{} of 5", k + 1));
    }
    // The sample is as given; the quantile and rank are computed.
    let derived: Vec<(&str, bool)> = obs[0]
        .readout
        .lines
        .iter()
        .map(|l| (l.name.as_str(), l.derived))
        .collect();
    assert_eq!(derived, [("V", false), ("Theoretical quantile", true), ("Rank", true)]);
    // Rows keep their place in the source: the smallest value is row 2.
    assert_eq!(obs[0].row, Some(1));
    assert!(obs[0].readout.heading.contains('2'));
    let note = g.note.as_deref().unwrap();
    assert!(note.contains("standard-normal") && note.contains("quartile"), "{note}");
}

#[test]
fn the_reference_line_passes_through_the_median_pair_with_the_quartile_slope() {
    let g = draw(&spec(json!({})), &values(&[1.0, 2.0, 3.0, 4.0, 5.0]));
    let lines = reference_lines(&g);
    assert_eq!(lines.len(), 1);
    let [a, b] = lines[0];
    // The point (0, 3) is on the line y = 3 + 1.4826 z: the middle observation.
    let mid = observations(&g)[2].at;
    let cross = (b[0] - a[0]) * (mid[1] - a[1]) - (b[1] - a[1]) * (mid[0] - a[0]);
    assert!(cross.abs() < 1e-6 * (b[0] - a[0]).abs().max(1.0), "{cross}");
    // It rises to the right and stays inside the plot.
    assert!(b[1] < a[1] && b[0] > a[0]);
    let plot = g.panels[0].plot;
    for p in [a, b] {
        assert!(p[0] >= plot.x - 1e-6 && p[0] <= plot.right() + 1e-6);
        assert!(p[1] >= plot.y - 1e-6 && p[1] <= plot.bottom() + 1e-6);
    }
}

#[test]
fn every_point_is_reached_by_pointer_at_its_centre() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    assert_eq!(points(&g), rows.len());
    for (k, o) in observations(&g).iter().enumerate() {
        let t = hit::at(&g, o.at).unwrap_or_else(|| panic!("point {k} is not reached"));
        let found = &g.panels[t.panel].observations[t.observation];
        assert_eq!(found.at, o.at, "point {k}: the pointer finds the point under it");
    }
}

#[test]
fn the_axes_are_titled() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let titles: Vec<&str> = g.panels[0]
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Text(t) if t.role == Role::AxisTitle => Some(t.text.as_str()),
            _ => None,
        })
        .collect();
    assert!(titles.contains(&"Theoretical quantile"), "{titles:?}");
    assert!(titles.contains(&"Days to deliver"), "{titles:?}");
    assert!(
        (g.width / g.height - 1.0).abs() < 0.02,
        "ratio 1:1 by default: {} x {}",
        g.width,
        g.height
    );
}

#[test]
fn groups_draw_their_own_points_and_lines() {
    let rows: Vec<Value> = (0..15)
        .flat_map(|i| {
            [
                json!({"v": i as f64, "g": "A"}),
                json!({"v": 3.0 * i as f64 + 1.0, "g": "B"}),
            ]
        })
        .collect();
    let s = spec(json!({"encoding":{"y":"v","color":"g"}}));
    contract(&s, &rows);
    let g = draw(&s, &rows);
    assert_eq!(g.series.len(), 2);
    assert_eq!(points(&g), 30);
    assert_eq!(reference_lines(&g).len(), 2);
    assert_eq!(observations(&g)[0].readout.heading, "A");
    assert!(g.legend.is_some());
}

#[test]
fn facets_make_one_panel_each() {
    let rows: Vec<Value> = (0..20)
        .map(|i| json!({"v": ((i % 10) as f64).powi(2), "p": if i < 10 { "left" } else { "right" }}))
        .collect();
    let s = spec(json!({"encoding":{"y":"v","facet":"p"}}));
    contract(&s, &rows);
    assert_eq!(draw(&s, &rows).panels.len(), 2);
}

#[test]
fn missing_values_are_disclosed_and_text_fails_the_panel() {
    let mut rows = values(&[1.0, 2.0, 4.0, 8.0]);
    rows.push(json!({"v": null}));
    let g = draw(&spec(json!({})), &rows);
    assert!(g.failure.is_none());
    assert!(check(&g, "excluded-observations").is_some());
    rows.push(json!({"v": "many"}));
    let g = draw(&spec(json!({})), &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").unwrap().message.contains("Row 6"));
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    let s = spec(json!({}));
    stays_visible(&s, &[]);
    // One value: its point at quantile zero, and no line to draw.
    contract(&s, &values(&[3.0]));
    let g = draw(&s, &values(&[3.0]));
    assert_eq!(points(&g), 1);
    assert!(reference_lines(&g).is_empty());
    // Identical values lie flat; the line is flat with them.
    contract(&s, &values(&[2.0, 2.0, 2.0, 2.0]));
    let g = draw(&s, &values(&[2.0, 2.0, 2.0, 2.0]));
    assert_eq!(points(&g), 4);
    contract(&s, &values(&[-4.0, 0.0, 9.0]));
    let g = draw(&s, &values(&[-1e300, 1e300]));
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    let dense: Vec<f64> = (0..5000).map(|i| ((i * 7919) % 5003) as f64 / 7.0).collect();
    let rows = values(&dense);
    contract(&s, &rows);
    let g = draw_at(&s, &rows, 320.0);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    assert_eq!(observations(&g).len(), 5000);
}

#[test]
fn writes_the_gallery_when_asked() {
    let (spec, rows) = fixture();
    gallery("qq", &spec, &rows);
    let two: Vec<Value> = (0..40)
        .map(|i| {
            let t = i as f64;
            if i % 2 == 0 {
                json!({"v": 4.0 + (t * 0.37).sin() * 2.0 + t * 0.1, "who": "Support"})
            } else {
                json!({"v": 9.0 + (t * 0.53).cos() * 3.0 + t * 0.15, "who": "Sales"})
            }
        })
        .collect();
    let s = json!({"type":"qq","title":"Replies","encoding":{"y":"v","color":"who"}});
    gallery("qq-groups", &s, &two);
}

#[test]
fn rows_closer_than_a_grid_step_keep_their_exact_rank_in_the_runtime() {
    // 100.04 comes before 100.01 in the rows; the grid ties them and would rank them by row, but
    // the runtime keeps the order of the exact rows.
    runtime_matches_native(&spec(json!({})), &values(&[500.0, 100.04, 100.01, 0.0, 1000.0]));
}

#[test]
fn a_label_shared_with_the_value_axis_is_named_once() {
    let s = spec(json!({"encoding":{"y":"v","label":"v"}}));
    let g = draw(&s, &values(&[1.0, 2.0, 3.0, 4.0, 5.0]));
    let o = observations(&g)[0];
    assert_eq!(o.readout.heading, "V 1");
    assert_eq!(line(o, "V"), None);
    assert!(line(o, "Theoretical quantile").is_some());
}
