//! The public every-point field survives validation, payload preparation and vector output;
//! reduction never changes the author's precision or the exported row multiplicity.
mod common;

use common::{geometry, payload};
use richview_chart::geometry::{Item, Role};
use richview_core::document::{RenderOptions, ReportFolder, render};
use serde_json::{Value, json};

fn report(block: &Value) -> richview_core::document::RenderedReport {
    render(
        &format!("```chart\n{block}\n```"),
        &RenderOptions {
            report_folder: Some(ReportFolder {
                root: "points".into(),
                source: "report.rmd".into(),
                files: vec!["report.rmd".into()],
            }),
            ..RenderOptions::default()
        },
    )
    .expect("the report renders")
}

fn block(kind: &str, mode: Value) -> Value {
    json!({"type":kind,"title":"Every source observation","digits":3,"readout_digits":4,
        "encoding":{"x":"x","y":"y"},"style":{"points":mode,"curve":"linear"},
        "data":{"inline":(0..2_000).map(|i|json!({"x":i,"y":12.3456789 + (i%7) as f64})).collect::<Vec<_>>()}})
}

#[test]
fn every_point_reaches_the_runtime_and_vector_output_without_extra_source_digits() {
    let report = report(&block("line", json!("all")));
    assert!(
        !report.diagnostics.iter().any(|d| d.level == "error"),
        "{:?}",
        report.diagnostics
    );
    let value = &report.charts[0];
    let payload = payload(value);
    assert_eq!(payload.block["style"]["points"], "all");
    let g = geometry(value, 390.0);
    let path = g
        .items()
        .find_map(|item| match item {
            Item::Line {
                path,
                role: Role::Mark,
                ..
            } => Some(path),
            _ => None,
        })
        .expect("the full line");
    assert_eq!(path.cmds.len(), 2_000);
    assert_eq!(richview_chart::vector::d(path).matches('L').count(), 1_999);
    let shown = payload.shown_rows();
    assert_eq!(shown.len(), 2_000);
    let y = payload.columns.iter().position(|c| c == "y").unwrap();
    assert_eq!(shown[0][y], json!(12.35));
    assert!(
        !report.html.contains("12.3456789"),
        "source digits cannot escape with the point policy"
    );
    assert!(
        g.panels[0].observations[0]
            .readout
            .lines
            .iter()
            .any(|line| line.value == "12.35")
    );
}

#[test]
fn point_reduction_changes_neither_table_rows_nor_allowed_precision() {
    let reports: Vec<_> = ["all", "m4", "band"]
        .map(|mode| report(&block("line", json!(mode))))
        .into();
    let rows: Vec<_> = reports
        .iter()
        .map(|r| payload(&r.charts[0]).shown_rows())
        .collect();
    assert_eq!(rows[0], rows[1]);
    assert_eq!(rows[0], rows[2]);
    for report in &reports {
        assert!(
            !report.diagnostics.iter().any(|d| d.level == "error"),
            "{:?}",
            report.diagnostics
        );
    }
    assert!(
        reports[1]
            .diagnostics
            .iter()
            .any(|d| d.code == "chart.decimation")
    );
}

#[test]
fn invalid_point_modes_and_ineligible_types_fail_the_authored_field() {
    for (kind, mode) in [
        ("line", json!("adaptive")),
        ("area", json!(false)),
        ("scatter", json!("m4")),
        ("bar", json!("all")),
    ] {
        let report = report(&block(kind, mode));
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "block.schema" && d.path.contains("style.points")),
            "{:?}",
            report.diagnostics
        );
    }
}
