use super::common;

use richview_chart::geometry::Role;
use richview_core::document::{RenderOptions, ReportFolder, render};
use richview_core::{authoring_values, parse_schema};
use serde_json::json;

#[test]
fn source_schema_and_authoring_agree_on_boxplot_channels_and_orientation() {
    let chart = json!({"type":"boxplot","title":"Observed distribution","data":{"inline":[{"value":1}]},"encoding":{"y":"value"}});
    assert_eq!(parse_schema("chart", chart.clone()).unwrap(), chart);
    assert!(authoring_values("chart", &["type"]).contains(&"boxplot"));
    for orientation in ["auto", "vertical", "horizontal"] {
        let mut next = chart.clone();
        next["style"] = json!({"orientation":orientation});
        assert!(parse_schema("chart", next).is_ok());
    }
    for (key, value) in [
        ("encoding", json!({"x":"value"})),
        ("encoding", json!({"y":"value","size":"weight"})),
        ("binning", json!({"count":2})),
        ("style", json!({"orientation":"diagonal"})),
        ("axes", json!({"y":{"min":2,"max":1}})),
    ] {
        let mut next = chart.clone();
        next[key] = value;
        assert!(parse_schema("chart", next).is_err());
    }
}

#[test]
fn report_boxplots_keep_original_source_and_recoverable_validation_errors() {
    let chart = json!({"type":"boxplot","title":"Observed distribution","data":{"inline":[{"value":"1.00","record":"First"},{"value":"2.00","record":"Second"}]},"encoding":{"y":"value"}});
    let source = format!("# Sample\n\n```chart\n{chart}\n```\n");
    let options = RenderOptions {
        report_folder: Some(ReportFolder {
            root: "boxplot-fixture".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }),
        ..Default::default()
    };
    let out = render(&source, &options).unwrap();
    let g = common::geometry(&out.charts[0], 640.0);
    assert!(g.mark_count() > 0, "the box is drawn");
    assert!(g.items().any(|i| i.role() == Role::Mark));
    // Decimal text reaches readouts as numbers at the author's precision (FEA-CH-37).
    let payload = common::payload(&out.charts[0]);
    assert_eq!(
        payload.shown.values().next().unwrap(),
        &[json!(1), json!(2)]
    );
    assert!(
        common::readout_values(&g).contains(&"2"),
        "{:?}",
        common::readout_values(&g)
    );
    assert!(!out.diagnostics.iter().any(|d| d.level == "error"));
    let invalid = source.replace("\"y\":\"value\"", "\"size\":\"value\"");
    let out = render(&invalid, &options).unwrap();
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.level == "error" && d.message.contains("encoding.y"))
    );
    assert!(out.html.contains("encoding"));
}
