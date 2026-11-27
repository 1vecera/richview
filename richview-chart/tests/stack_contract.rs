//! Stacked bars and areas: the runtime's layout from grid rows takes the axis extent and the
//! stack totals from the exact rows (TYP-36).

use super::common;

use common::*;
use serde_json::{Value, json};

fn rows(kind: &str) -> (Value, Vec<Value>) {
    let spec = json!({
        "type": kind, "title": "T", "stack": true,
        "encoding": {"x": "x", "y": "v", "color": "g"}
    });
    let mut out = Vec::new();
    // Stack tops a grid step apart, and one tall stack that sets the axis.
    for (x, a, b) in [
        ("A", 100.01, 0.02),
        ("B", 100.04, 0.01),
        ("C", 900.0, 100.02),
        ("D", 640.0, 0.03),
    ] {
        for (g, v) in [("one", a), ("two", b)] {
            out.push(json!({"x": x, "v": v, "g": g}));
        }
    }
    (spec, out)
}

#[test]
fn stacked_bars_keep_the_axis_and_totals_of_the_exact_rows() {
    let (spec, rows) = rows("bar");
    runtime_matches_native(&spec, &rows);
}

#[test]
fn stacked_areas_keep_the_axis_and_totals_of_the_exact_rows() {
    let (spec, mut rows) = rows("area");
    for (i, row) in rows.iter_mut().enumerate() {
        row["x"] = json!((i / 2) as f64 * 1.5 + 100.01);
    }
    runtime_matches_native(&spec, &rows);
}

#[test]
fn nearly_equal_line_ends_do_not_swap_at_a_half_pixel_boundary() {
    let (mut spec, mut rows) = rows("area");
    spec["axes"] = json!({"y":{"min":0,"max":1200}});
    for (i, row) in rows.iter_mut().enumerate() {
        row["x"] = json!((i / 2) as f64 * 1.5 + 100.01);
    }
    runtime_matches_native(&spec, &rows);
}

#[test]
fn line_ends_that_read_the_same_value_keep_series_order() {
    use richview_chart::geometry::{Item, Role};
    for (format, a, b, max) in [
        ("number", 640.2, 640.4, 1200.0),
        ("number_compact", 1001.1, 1004.1, 1200.0),
        ("percent", 12.341, 12.349, 20.0),
    ] {
        let spec = json!({"type":"line","title":"T","encoding":{"x":"x","y":"v","color":"g"},
            "axes":{"y":{"min":0,"max":max,"format":format}}});
        let rows = vec![
            json!({"x":0,"v":0,"g":"A"}),
            json!({"x":0,"v":0,"g":"B"}),
            json!({"x":1,"v":a,"g":"A"}),
            json!({"x":1,"v":b,"g":"B"}),
        ];
        let g = draw(&spec, &rows);
        let y = |series| {
            g.items()
                .find_map(|item| match item {
                    Item::Text(t) if t.role == Role::Label && t.series == Some(series) => Some(t.bounds.center()[1]),
                    _ => None,
                })
                .unwrap()
        };
        assert!(y(0) < y(1), "{format}: equal shown values must keep A above B");
        runtime_matches_native(&spec, &rows);
    }
}

#[test]
fn end_order_records_are_invariant_across_label_and_legend_layouts() {
    use richview_chart::{
        block::ChartBlock,
        layout::{ChartTheme, Options, layout},
    };
    for kind in ["line", "area"] {
        let block = ChartBlock::from_value(&json!({"type":kind,"title":"T",
            "encoding":{"x":"x","y":"v","color":"g"}}))
        .unwrap();
        let rows = vec![
            json!({"x":0,"v":10,"g":"North region"}),
            json!({"x":1,"v":20,"g":"North region"}),
            json!({"x":2,"v":20,"g":"North region"}),
            json!({"x":0,"v":20,"g":"South region"}),
            json!({"x":1,"v":10,"g":"South region"}),
            json!({"x":2,"v":10,"g":"South region"}),
        ];
        let mut presentations = std::collections::BTreeSet::new();
        for hidden in [vec![], vec!["North region".into()]] {
            for zoom in [None, Some([0.0, 1.0])] {
                let mut expected = None;
                for width in [320.0, 1440.0] {
                    for size in [8.0, 28.0] {
                        let g = layout(
                            &block,
                            &rows,
                            &ChartTheme {
                                size,
                                ..Default::default()
                            },
                            &Options {
                                width,
                                hidden: hidden.clone(),
                                zoom,
                                ..Default::default()
                            },
                        );
                        presentations.insert(g.legend.is_some());
                        assert!(
                            g.derived.iter().any(|d| d.key.starts_with("cartesian.end-order.")),
                            "{kind}/{width}/{size}: data order exists even when names use a legend"
                        );
                        if let Some(expected) = &expected {
                            assert_eq!(&g.derived, expected, "{kind}/{width}/{size}");
                        } else {
                            expected = Some(g.derived);
                        }
                    }
                }
            }
        }
        assert_eq!(
            presentations.len(),
            2,
            "{kind}: exercise both direct names and a legend: {presentations:?}"
        );
    }
}
