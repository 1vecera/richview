//! Hexbin charts: counts in a tiling of hexagons coloured by count and printing it, a key that
//! decodes the colour when a count does not fit, outline hits, and the contract every type shares
//! (FEA-CH-75).

mod common;

use common::*;
use richview_chart::geometry::{Item, Role};
use richview_chart::hit;
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/hexbin.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn spec() -> Value {
    json!({"type":"hexbin","title":"T","encoding":{"x":"a","y":"b"},"axes":{"x":{"label":"A"},"y":{"label":"B"}}})
}

fn pairs(ps: &[(f64, f64)]) -> Vec<Value> {
    ps.iter().map(|(a, b)| json!({"a": a, "b": b})).collect()
}

fn counted(g: &richview_chart::geometry::Geometry) -> usize {
    observations(g)
        .iter()
        .map(|o| line(o, "Observations").unwrap().parse::<usize>().unwrap())
        .sum()
}

fn hexagons(g: &richview_chart::geometry::Geometry) -> Vec<&Item> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter(|i| matches!(i, Item::Area { obs: Some(_), .. }))
        .collect()
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
    gallery("hexbin", &spec, &rows);
}

#[test]
fn the_counts_of_the_hexagons_add_up_to_the_rows() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    assert_eq!(counted(&g), rows.len());
    assert_eq!(hexagons(&g).len(), observations(&g).len());
    assert!(
        observations(&g)
            .iter()
            .all(|o| o.readout.lines.iter().all(|l| l.derived))
    );
}

#[test]
fn a_full_hexagon_is_regular_and_the_pointer_finds_it_by_its_outline() {
    let (spec, rows) = fixture();
    let g = draw_at(&spec, &rows, 1440.0);
    let plot = g.panels[0].plot;
    // The lattice is laid out in a 600 px wide reference plot of the default ratio and stretched
    // onto the plot, so a hexagon is regular only once that stretch is undone.
    let (ref_w, ref_h) = (600.0, 600.0 / (16.0 / 9.0) * 0.75);
    let inside = |p: &[f64; 2]| {
        p[0] > plot.x + 0.5 && p[0] < plot.right() - 0.5 && p[1] > plot.y + 0.5 && p[1] < plot.bottom() - 0.5
    };
    let (mut full, mut stretched) = (0, 0);
    for item in hexagons(&g) {
        let Item::Area { path, .. } = item else { continue };
        let rings = path.flatten(0.5);
        let mut pts = rings[0].clone();
        if pts.len() == 7 && pts[0] == pts[6] {
            pts.pop();
        }
        // A hexagon the plot cuts does not have six corners inside it.
        if pts.len() != 6 || !pts.iter().all(inside) {
            continue;
        }
        full += 1;
        let c = [
            pts.iter().map(|p| p[0]).sum::<f64>() / 6.0,
            pts.iter().map(|p| p[1]).sum::<f64>() / 6.0,
        ];
        // Opposite corners mirror each other through the centre, whatever the stretch.
        for (a, b) in pts.iter().zip(&pts[3..]) {
            assert!(((a[0] + b[0]) / 2.0 - c[0]).abs() < 0.5 && ((a[1] + b[1]) / 2.0 - c[1]).abs() < 0.5);
        }
        let reach = |scale: [f64; 2]| -> (f64, f64) {
            let d: Vec<f64> = pts
                .iter()
                .map(|p| ((p[0] - c[0]) * scale[0]).hypot((p[1] - c[1]) * scale[1]))
                .collect();
            (
                d.iter().copied().fold(f64::INFINITY, f64::min),
                d.iter().copied().fold(0.0, f64::max),
            )
        };
        // At the reference shape the six corners are one distance from the centre.
        let (lo, hi) = reach([ref_w / plot.w, ref_h / plot.h]);
        assert!(hi - lo < 0.5, "corner distances {lo} to {hi} at the reference shape");
        let (lo, hi) = reach([1.0, 1.0]);
        if hi - lo > 0.2 {
            stretched += 1;
        }
    }
    assert!(full > 3, "{full} whole hexagons");
    let reference_aspect = ref_w / ref_h;
    if (plot.w / plot.h - reference_aspect).abs() > 0.1 * reference_aspect {
        assert!(stretched > 0, "a plot of another shape stretches the hexagons");
    }
    for o in observations(&g) {
        assert!(hit::at(&g, o.at).is_some());
    }
}

/// The numbers of a chart's value labels, with those it records as dropped, in order.
fn counts(g: &richview_chart::geometry::Geometry) -> Vec<String> {
    let mut all: Vec<String> = g
        .panels
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| match i {
            Item::Text(t) if t.role == Role::Label => Some(t.text.clone()),
            _ => None,
        })
        .chain(g.dropped.iter().map(|d| d.text.clone()))
        .collect();
    all.sort();
    all
}

fn has_key(g: &richview_chart::geometry::Geometry) -> bool {
    g.figure_items.iter().any(|i| i.role() == Role::Key)
}

fn key_ends(g: &richview_chart::geometry::Geometry) -> Vec<String> {
    g.figure_items
        .iter()
        .filter_map(|i| match i {
            Item::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_fuller_hexagon_is_darker_and_each_prints_its_count() {
    let mut rows = pairs(&[(0.0, 0.0), (0.0, 0.0), (0.0, 0.0), (10.0, 10.0), (10.0, 10.0)]);
    rows.push(json!({"a": 5.0, "b": 5.0}));
    let g = draw(&spec(), &rows);
    let obs = observations(&g);
    assert_eq!(obs.len(), 3);
    let mut by_count: Vec<(usize, _)> = hexagons(&g)
        .iter()
        .zip(&obs)
        .map(|(i, o)| {
            let n = line(o, "Observations").unwrap().parse::<usize>().unwrap();
            match i {
                Item::Area { fill, .. } => (n, fill.color),
                _ => unreachable!(),
            }
        })
        .collect();
    by_count.sort_by_key(|(n, _)| *n);
    assert_ne!(by_count[0].1, by_count[2].1);
    assert_eq!(counts(&g), ["1", "2", "3"]);
    assert!(g.dropped.is_empty());
    assert!(!has_key(&g), "the counts decode the colour");
}

#[test]
fn every_hexagon_of_the_golden_chart_is_whole_and_holds_its_count() {
    let (spec, rows) = fixture();
    for width in [390.0, 720.0, 1440.0] {
        let g = draw_at(&spec, &rows, width);
        let plot = g.panels[0].plot;
        let obs = observations(&g);
        // The plot grows to hold a hexagon at the edge of the data: none is cut.
        for item in hexagons(&g) {
            let Item::Area { path, .. } = item else { continue };
            let mut pts = path.flatten(0.5)[0].clone();
            if pts.first() == pts.last() {
                pts.pop();
            }
            assert_eq!(pts.len(), 6, "a cut hexagon at {width}");
            assert!(pts.iter().all(|p| {
                p[0] >= plot.x - 0.01
                    && p[0] <= plot.right() + 0.01
                    && p[1] >= plot.y - 0.01
                    && p[1] <= plot.bottom() + 0.01
            }));
        }
        let labels: Vec<_> = g.panels[0]
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Text(t) if t.role == Role::Label => Some(t),
                _ => None,
            })
            .collect();
        assert_eq!(labels.len(), obs.len(), "at {width}");
        let ids = g.mark_ids();
        for (k, (label, o)) in labels.iter().zip(&obs).enumerate() {
            assert_eq!(Some(label.text.as_str()), line(o, "Observations"));
            assert_eq!(label.mark.as_deref(), Some(ids[k].as_str()));
            // The count stands on its own hexagon.
            let centre = [
                label.bounds.x + label.bounds.w / 2.0,
                label.bounds.y + label.bounds.h / 2.0,
            ];
            assert_eq!(
                hit::at(&g, centre).map(|h| h.observation),
                Some(k),
                "{} at {width}",
                label.text
            );
        }
        assert!(!has_key(&g), "at {width}");
        assert!(g.dropped.is_empty(), "at {width}");
    }
}

#[test]
fn a_hexagon_too_small_for_its_count_brings_the_key_back_and_records_each_count() {
    // 600 observations make 24 hexagons across, too small for a count on a phone.
    let rows: Vec<(f64, f64)> = (0..600)
        .map(|i| ((i * 37 % 101) as f64, (i * 11 % 53) as f64))
        .collect();
    let g = draw_at(&spec(), &pairs(&rows), 320.0);
    let obs = observations(&g);
    let most = obs
        .iter()
        .map(|o| line(o, "Observations").unwrap().parse::<usize>().unwrap())
        .max()
        .unwrap();
    assert!(!g.dropped.is_empty());
    assert!(has_key(&g));
    assert_eq!(key_ends(&g), ["1".to_string(), most.to_string()]);
    // Never neither: a hexagon without its count is one the key decodes, and its count is on record.
    let mut expected: Vec<String> = obs
        .iter()
        .map(|o| line(o, "Observations").unwrap().to_owned())
        .collect();
    expected.sort();
    assert_eq!(counts(&g), expected);
    assert!(g.dropped.iter().all(|d| d.kind.as_deref() == Some("cell-value")));
}

#[test]
fn a_chart_carries_the_same_counts_at_every_width() {
    let rows: Vec<(f64, f64)> = (0..600)
        .map(|i| ((i * 37 % 101) as f64, (i * 11 % 53) as f64))
        .collect();
    let rows = pairs(&rows);
    let wide = counts(&draw_at(&spec(), &rows, 1440.0));
    assert!(!wide.is_empty());
    for width in [1100.0, 720.0, 390.0, 320.0] {
        assert_eq!(counts(&draw_at(&spec(), &rows, width)), wide, "at {width}");
    }
}

#[test]
fn the_author_can_have_the_key_instead_of_the_counts() {
    let (mut spec, rows) = fixture();
    spec["auto_annotations"] = json!(false);
    let g = draw(&spec, &rows);
    let most = observations(&g)
        .iter()
        .map(|o| line(o, "Observations").unwrap().parse::<usize>().unwrap())
        .max()
        .unwrap();
    assert!(
        counts(&g).is_empty(),
        "no count printed and none dropped: the author chose it"
    );
    assert_eq!(key_ends(&g), ["1".to_string(), most.to_string()]);
}

#[test]
fn one_row_is_one_hexagon_and_a_constant_is_not_a_failure() {
    let one = pairs(&[(4.0, 7.0)]);
    let g = draw(&spec(), &one);
    assert_eq!(counted(&g), 1);
    contract(&spec(), &one);
    contract(&spec(), &pairs(&[(3.0, 3.0), (3.0, 3.0), (3.0, 3.0)]));
}

#[test]
fn a_dense_sample_stays_inside_the_plot() {
    let rows: Vec<(f64, f64)> = (0..600)
        .map(|i| ((i * 37 % 101) as f64, (i * 11 % 53) as f64))
        .collect();
    let rows = pairs(&rows);
    contract(&spec(), &rows);
    assert_eq!(counted(&draw(&spec(), &rows)), 600);
}

#[test]
fn missing_values_are_disclosed_and_bad_text_fails_the_panel() {
    let mut rows = pairs(&[(1.0, 1.0), (2.0, 2.0), (3.0, 3.0)]);
    rows.push(json!({"a": 4.0, "b": null}));
    let g = draw(&spec(), &rows);
    assert!(check(&g, "excluded-observations").is_some());
    assert_eq!(counted(&g), 3);
    rows.push(json!({"a": "many", "b": 1.0}));
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").is_some());
    stays_visible(&spec(), &[]);
    stays_visible(&spec(), &[json!({"a": null, "b": null})]);
}

#[test]
fn extreme_ranges_stay_finite() {
    let g = draw(&spec(), &pairs(&[(-1e300, 0.0), (0.0, 1e300), (1e300, -1e300)]));
    assert!(nonfinite(&g).is_none());
    let g = draw(&spec(), &pairs(&[(1e-300, 2e-300), (3e-300, 5e-300)]));
    assert!(nonfinite(&g).is_none());
}

#[test]
fn facets_share_one_colour_scale() {
    let mut s = spec();
    s["encoding"]["facet"] = json!("f");
    let mut rows = Vec::new();
    for f in ["p", "q"] {
        for i in 0..20 {
            rows.push(json!({"a": i as f64, "b": (i * 3 % 7) as f64, "f": f}));
        }
    }
    let g = draw(&s, &rows);
    assert_eq!(g.panels.len(), 2);
    contract(&s, &rows);
}

#[test]
fn the_same_data_bins_into_the_same_hexagons_at_every_width() {
    let (spec, rows) = fixture();
    let counts: Vec<usize> = [320.0, 390.0, 640.0, 1440.0]
        .into_iter()
        .map(|w| observations(&draw_at(&spec, &rows, w)).len())
        .collect();
    assert!(counts.iter().all(|c| *c == counts[0]), "{counts:?}");
}

#[test]
fn readout_headings_are_independent_of_plot_size() {
    let spec = spec();
    let rows = pairs(
        &(0..=10)
            .flat_map(|x| (0..=2).map(move |y| (f64::from(x), f64::from(y))))
            .collect::<Vec<_>>(),
    );
    let headings = |width| {
        let g = draw_at(&spec, &rows, width);
        observations(&g)
            .iter()
            .map(|o| o.readout.heading.clone())
            .collect::<Vec<_>>()
    };
    let narrow = headings(320.0);
    assert!(!narrow.is_empty());
    assert_eq!(narrow, headings(1440.0));
}

#[test]
fn points_on_a_regular_lattice_fall_in_the_exact_hexagons_on_the_grid() {
    // Whole-number pairs over 0 to 1000 sit on hexagon boundaries again and again; a grid step
    // moves some across, but the runtime keeps the counts of the exact rows.
    let ps: Vec<(f64, f64)> = (0..10)
        .flat_map(|i| (0..10).map(move |j| (f64::from(i) * 111.0, f64::from(j) * 111.0)))
        .collect();
    runtime_matches_native(&spec(), &pairs(&ps));
}

#[test]
fn readout_names_the_centre_once() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    for o in observations(&g) {
        assert!(o.readout.heading.starts_with("Near "), "{}", o.readout.heading);
        assert_eq!(o.readout.lines.len(), 1);
        assert_eq!(o.readout.lines[0].name, "Observations");
    }
}
