use super::common;

use richview_core::document::{RenderOptions, ReportFolder, render};
use serde_json::json;

fn options() -> RenderOptions {
    RenderOptions {
        report_folder: Some(ReportFolder {
            root: "numeric-range".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into(), "observations.csv".into()],
        }),
        ..Default::default()
    }
}

#[test]
fn csv_underflow_retains_text_until_numeric_chart_validation() {
    let source = "```chart\ntype: bar\ntitle: Original observations\ndata: {csv: observations.csv}\nencoding: {x: category, y: value}\n```\n";
    let mut options = options();
    options.data.insert(
        "observations.csv".into(),
        "category,value\nFirst,2\nSecond,1e-400\n".into(),
    );
    let output = render(source, &options).unwrap();
    assert!(
        output
            .diagnostics
            .iter()
            .any(|d| d.level == "error" && d.message.contains("too small to represent")),
        "{:?}",
        output.diagnostics
    );
    // The unrepresentable text stays in the payload for the Table; the chart keeps a failed frame.
    let payload = common::payload(&output.charts[0]);
    assert_eq!(payload.rows[1][1], json!("1e-400"));
    assert!(common::geometry(&output.charts[0], 640.0).failure.is_some());
    options.data.insert(
        "observations.csv".into(),
        "category,value\nFirst,2\nSecond,3\n".into(),
    );
    let repaired = render(source, &options).unwrap();
    assert!(!repaired.diagnostics.iter().any(|d| d.level == "error"));
    assert_eq!(common::geometry(&repaired.charts[0], 640.0).mark_count(), 2);
}

#[test]
fn explicit_table_formats_do_not_replace_an_unrepresentable_source_value_with_zero() {
    for format in ["number", "percent", "number_compact"] {
        let table = json!({"data":{"inline":[{"value":"1e-400"}]},"columns":[{"key":"value","label":"Original","format":format}]});
        let output = render(&format!("```table\n{table}\n```\n"), &options()).unwrap();
        assert!(
            output.html.contains(">1e-400</td>"),
            "{format}: {}",
            output.html.split("<table").last().unwrap()
        );
    }
}

#[test]
fn csv_typing_keeps_existing_zero_subnormal_boolean_and_identifier_policies() {
    let source = "```chart\ntype: scatter\ntitle: Representable source\ndata: {csv: observations.csv}\nencoding: {x: x, y: value}\n```\n";
    let mut options = options();
    options.data.insert(
        "observations.csv".into(),
        "x,value,id,flag\n1,0,001,true\n2,5e-324,002,false\n".into(),
    );
    let output = render(source, &options).unwrap();
    assert!(!output.diagnostics.iter().any(|d| d.level == "error"));
    // Only referenced columns ship (TYP-36); zero stays zero and the smallest subnormal stays
    // above zero, so neither is confused with the other.
    let payload = common::payload(&output.charts[0]);
    assert_eq!(payload.columns, ["x", "value"]);
    let rows = payload.objects();
    let at = |row: usize, column: &str| rows[row][column].as_f64().unwrap();
    assert_eq!((at(0, "x"), at(0, "value")), (1.0, 0.0));
    assert_eq!(at(1, "x"), 2.0);
    assert!(at(1, "value") > 0.0, "{:?}", rows[1]);
}
