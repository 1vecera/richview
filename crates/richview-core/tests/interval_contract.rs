mod common;

use richview_core::document::{RenderOptions, ReportFolder, render};
use richview_core::{authoring_keys, authoring_values, parse_schema};
use serde_json::json;

#[test]
fn interval_schema_retains_bounds_and_editor_vocabulary_matches_the_contract() {
    let chart = json!({"type":"interval","title":"Supplied ranges","data":{"inline":[{"category":"A","lo":1,"hi":3,"estimate":2}]},"encoding":{"x":"category","lower":"lo","upper":"hi","y":"estimate"}});
    assert_eq!(parse_schema("chart", chart.clone()).unwrap(), chart);
    assert!(authoring_values("chart", &["type"]).contains(&"interval"));
    let keys = authoring_keys("chart", &["encoding"]).unwrap();
    assert!(keys.contains(&"lower") && keys.contains(&"upper"));
    for orientation in ["auto", "vertical", "horizontal"] {
        let mut next = chart.clone();
        next["style"] = json!({"orientation":orientation});
        assert!(parse_schema("chart", next).is_ok());
    }
    let mut range_only = chart.clone();
    range_only["encoding"].as_object_mut().unwrap().remove("y");
    assert!(parse_schema("chart", range_only).is_ok());
    for (key, value) in [
        ("encoding", json!({"x":"category","upper":"hi"})),
        ("encoding", json!({"lower":"lo","upper":"hi"})),
        ("encoding", json!({"x":"category","lower":1,"upper":"hi"})),
        (
            "encoding",
            json!({"x":"category","lower":"lo","upper":"hi","size":"weight"}),
        ),
        (
            "encoding",
            json!({"x":"category","lower":"lo","upper":"hi","label":"label"}),
        ),
        ("binning", json!({"count":2})),
        ("style", json!({"orientation":"diagonal"})),
        ("axes", json!({"y":{"min":2,"max":1}})),
    ] {
        let mut next = chart.clone();
        next[key] = value;
        assert!(parse_schema("chart", next).is_err());
    }
    let mut other = chart;
    other["type"] = json!("bar");
    assert!(
        parse_schema("chart", other).is_err(),
        "bounds must not be silently ignored on another chart type"
    );
}

#[test]
fn interval_reports_keep_external_decimal_bounds_and_actionable_source_errors() {
    let source = "# Authored ranges\n\n```chart\ntype: interval\ntitle: Supplied bounds\ndata: {csv: ranges.csv}\nencoding: {x: category, lower: lo, upper: hi, y: estimate}\n```\n";
    let mut options = RenderOptions {
        report_folder: Some(ReportFolder {
            root: "interval-fixture".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into(), "ranges.csv".into()],
        }),
        ..Default::default()
    };
    options.data.insert(
        "ranges.csv".into(),
        "category,lo,hi,estimate\nA,1.234567890123456789,2.345678901234567891,1.9\nB,3,4,\n".into(),
    );
    let out = render(source, &options).unwrap();
    let g = common::geometry(&out.charts[0], 640.0);
    assert!(g.mark_count() >= 2, "a range and an estimate");
    // Readers get the author's precision, never the source's 19 digits (FEA-CH-37, TYP-36).
    assert!(
        !out.html.contains("1.234567890123456789") && !out.html.contains("2.345678901234567891")
    );
    let payload = common::payload(&out.charts[0]);
    // The Table and the CSV read the author's precision; positions sit on the grid (TYP-36).
    assert_eq!(payload.shown["lo"][0], json!(1.23));
    assert_eq!(payload.shown["hi"][0], json!(2.35));
    let lo = payload.objects()[0]["lo"].as_f64().unwrap();
    assert!((lo - 1.23).abs() < 0.01, "{lo}");
    assert_ne!(lo, 1.234_567_890_123_456_7);
    assert!(
        !out.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.level == "error")
    );
    options.data.insert(
        "ranges.csv".into(),
        "category,lo,hi,estimate\nA,1,3,2\nB,5,4,\n".into(),
    );
    let invalid = render(source, &options).unwrap();
    assert!(
        invalid
            .diagnostics
            .iter()
            .any(|d| d.level == "error" && d.message.contains("lower bound"))
    );
    let failed = common::geometry(&invalid.charts[0], 640.0);
    assert!(
        failed.failure.is_some(),
        "the bad interval chart keeps a failed frame"
    );
    let diagnostics = richview_core::document::check(source, &options).unwrap();
    assert!(
        diagnostics
            .charts
            .iter()
            .flat_map(|chart| chart["checks"].as_array().into_iter().flatten())
            .any(|check| check["message"]
                .as_str()
                .is_some_and(|message| message.to_lowercase().contains("row 2")))
    );
}
