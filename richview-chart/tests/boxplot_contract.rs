//! Box plots: R-7 quartiles, whiskers at the last observations inside 1.5 IQR, outliers drawn
//! as the source rows they are, and the contract every type shares (FEA-CH-75).

mod common;

use common::*;
use serde_json::{Value, json};

fn spec() -> Value {
    json!({"type":"boxplot","title":"Response times","encoding":{"x":"k","y":"v"}})
}

/// Group a holds 1 to 9 and an outlier of 30; group b holds 2 to 6.
fn rows() -> Vec<Value> {
    let a = (1..=9).chain([30]).map(|v| json!({"k": "a", "v": v}));
    let b = (2..=6).map(|v| json!({"k": "b", "v": v}));
    a.chain(b).collect()
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    contract(&spec(), &rows());
}

#[test]
fn boxes_use_r7_quartiles_observed_whiskers_and_source_outliers() {
    let g = draw(&spec(), &rows());
    let obs = observations(&g);
    let a = obs.iter().find(|o| o.readout.heading == "a").expect("a box for a");
    assert_eq!(line(a, "Observations"), Some("10"));
    // R-7 over 1..9 and 30: Q1 3.25, median 5.5, Q3 7.75, so the upper fence is 14.5.
    assert_eq!(line(a, "First quartile"), Some("3.25"));
    assert_eq!(line(a, "Median"), Some("5.5"));
    assert_eq!(line(a, "Third quartile"), Some("7.75"));
    assert_eq!(
        line(a, "Lower whisker"),
        Some("1"),
        "the lowest observation inside the fence"
    );
    assert_eq!(
        line(a, "Upper whisker"),
        Some("9"),
        "the highest observation inside the fence"
    );
    let outliers: Vec<_> = obs.iter().filter(|o| o.row.is_some()).collect();
    assert_eq!(outliers.len(), 1, "30 alone lies beyond the fence");
    assert_eq!(outliers[0].row, Some(9), "the outlier is its source row");
    assert!(outliers[0].readout.lines.iter().all(|l| !l.derived && l.value == "30"));
}

#[test]
fn a_value_on_the_fence_is_not_an_outlier_on_the_grid_either() {
    let values = [
        0.0, 100.0, 100.0, 100.0, 100.0, 200.0, 200.0, 200.0, 200.0, 350.0, 1000.0,
    ];
    let rows: Vec<Value> = values.iter().map(|v| json!({"k": "a", "v": v})).collect();
    let spec = json!({"type":"boxplot","title":"T","encoding":{"x":"k","y":"v"}});
    // 350 sits exactly on the upper fence (Q3 + 1.5 IQR); a grid step can push it over.
    common::runtime_matches_native(&spec, &rows);
}

#[test]
fn value_ticks_with_units_are_spaced_by_the_face_that_draws_them() {
    // A style whose figures face is wider than its data face (unmeasured families fall back to
    // 0.6 em a character): "1.5 seconds" must still not run into "1.0 seconds".
    let rows: Vec<Value> = [
        ("A longer first category name", [1.2, 1.4, 1.8]),
        ("A longer second category name", [1.5, 1.7, 2.2]),
    ]
    .into_iter()
    .flat_map(|(k, vs)| vs.map(|v| json!({"k": k, "v": v})))
    .collect();
    let spec = json!({
        "type":"boxplot","title":"T","encoding":{"x":"k","y":"v"},
        "style":{"orientation":"horizontal"},"axes":{"y":{"label":"Duration","unit":"seconds"}}
    });
    let block = richview_chart::block::ChartBlock::from_value(&spec).unwrap();
    let theme = richview_chart::layout::ChartTheme {
        figures_font: Some("No Such Wide Figures".into()),
        ..richview_chart::layout::ChartTheme::default()
    };
    for width in common::WIDTHS {
        let options = richview_chart::layout::Options {
            width,
            ..Default::default()
        };
        let g = richview_chart::layout::layout(&block, &rows, &theme, &options);
        assert!(g.failure.is_none(), "{width}: {:?}", g.failure);
        for kind in ["label-collision", "clipping"] {
            assert!(
                common::check(&g, kind).is_none(),
                "{width} px: {kind}: {:?}",
                common::check(&g, kind)
            );
        }
    }
}
