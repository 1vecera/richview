use super::common;

use richview_core::document::{RenderOptions, ReportFolder, render};
use richview_core::{authoring_keys, authoring_values, parse_schema};
use serde_json::json;

#[test]
fn histogram_source_schema_and_completion_expose_one_binning_contract() {
    let chart = json!({"type":"histogram","title":"Distribution","data":{"inline":[{"value":0},{"value":1}]},"encoding":{"x":"value"},"binning":{"edges":[0,0.5,1],"measure":"density"}});
    let parsed = parse_schema("chart", chart.clone()).unwrap();
    assert_eq!(parsed["binning"], chart["binning"]);
    assert!(authoring_values("chart", &["type"]).contains(&"histogram"));
    assert_eq!(
        authoring_values("chart", &["binning", "measure"]),
        ["count", "proportion", "density"]
    );
    assert!(authoring_keys("chart", &[]).unwrap().contains(&"binning"));
    assert_eq!(
        authoring_keys("chart", &["binning"]).unwrap(),
        ["count", "edges", "extent", "measure"]
    );
    for binning in [
        json!({"count":0}),
        json!({"count":257}),
        json!({"edges":[1,0]}),
        json!({"extent":[0,0]}),
        json!({"edges":[0,1],"count":1}),
        json!({"measure":"percent"}),
    ] {
        let mut invalid = chart.clone();
        invalid["binning"] = binning;
        assert!(parse_schema("chart", invalid).is_err());
    }
    let mut invalid = chart;
    invalid["encoding"]["y"] = json!("weight");
    assert!(
        parse_schema("chart", invalid)
            .unwrap_err()
            .iter()
            .any(|d| d.message.contains("encoding.y"))
    );
}

#[test]
fn report_render_keeps_original_histogram_data_and_emits_recoverable_bad_source() {
    let chart = json!({"type":"histogram","title":"A sample distribution","data":{"inline":[{"value":"0.00","note":"first"},{"value":"1.00","note":"last"}]},"encoding":{"x":"value"},"binning":{"count":2}});
    let source = format!("# Distribution\n\n```chart\n{chart}\n```\n");
    let options = RenderOptions {
        report_folder: Some(ReportFolder {
            root: "histogram-fixture".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }),
        ..Default::default()
    };
    let output = render(&source, &options).unwrap();
    let g = common::geometry(&output.charts[0], 640.0);
    assert_eq!(g.mark_count(), 2, "one bar per bin");
    // Decimal text reaches the payload as numbers at the author's precision (FEA-CH-37).
    let payload = common::payload(&output.charts[0]);
    assert_eq!(
        payload.columns,
        ["value"],
        "the unused note column never ships (TYP-36)"
    );
    assert_eq!(payload.shown["value"], [json!(0), json!(1)]);
    assert!(!output.diagnostics.iter().any(|d| d.level == "error"));
    let bad = source.replace("\"count\":2", "\"count\":0");
    let output = render(&bad, &options).unwrap();
    assert!(
        output
            .diagnostics
            .iter()
            .any(|d| d.level == "error" && d.message.contains("bin count"))
    );
    assert!(
        output.html.contains("binning"),
        "Invalid source stays available to repair"
    );
}
