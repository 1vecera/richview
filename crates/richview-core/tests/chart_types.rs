//! Every chart type the engine draws is a type core accepts (FEA-CH-01, FEA-CH-75): each golden
//! fixture of the engine validates as a chart block, and the list core completes from is the
//! engine's list.

use richview_core::{authoring_values, parse_schema};

use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;

fn fixtures() -> Vec<(String, Value)> {
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../richview-chart/tests/fixtures/types"
    );
    let mut out: Vec<(String, Value)> = fs::read_dir(dir)
        .expect("fixture directory")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .map(|p| {
            let name = p.file_stem().expect("stem").to_string_lossy().into_owned();
            (
                name,
                serde_json::from_str(&fs::read_to_string(&p).expect("read")).expect("json"),
            )
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn every_golden_fixture_validates_as_a_chart_block() {
    for (name, fixture) in fixtures() {
        let mut chart = fixture["spec"].clone();
        assert_eq!(
            chart["type"],
            name.as_str(),
            "{name}: the fixture is named for its type"
        );
        // A flow written as cards brings its own rows; the others take the fixture's.
        if chart.get("nodes").is_none() {
            chart["data"] = serde_json::json!({"inline": fixture["rows"]});
        }
        if let Err(error) = parse_schema("chart", chart) {
            panic!("{name}: {error:?}");
        }
    }
}

#[test]
fn core_completes_exactly_the_engine_types() {
    let core: BTreeSet<&str> = authoring_values("chart", &["type"])
        .iter()
        .copied()
        .collect();
    let engine: BTreeSet<&str> = richview_chart::block::ChartType::all()
        .map(|t| t.name())
        .collect();
    assert_eq!(core, engine);
}

/// A type's own example draws through the whole pipeline: the payload keeps every channel the
/// type reads, so the plot is never empty and no row is left out (FEA-CH-75).
#[test]
fn every_type_example_draws_and_keeps_its_rows() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples");
    let mut drawn = 0;
    let nine = [
        "line",
        "area",
        "bar",
        "scatter",
        "pie",
        "heatmap",
        "histogram",
        "boxplot",
        "interval",
    ];
    for name in authoring_values("chart", &["type"]) {
        if nine.contains(name) {
            continue;
        }
        let path = format!("{root}/{name}/report.rmd");
        let Ok(source) = fs::read_to_string(&path) else {
            continue;
        };
        let options = richview_core::document::RenderOptions {
            report_folder: Some(richview_core::document::ReportFolder {
                root: name.to_string(),
                source: "report.rmd".into(),
                files: std::iter::once("report.rmd".to_string()).collect(),
            }),
            ..Default::default()
        };
        let report = richview_core::document::check(&source, &options)
            .unwrap_or_else(|d| panic!("{name}: {d:?}"));
        let bad: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| {
                d.level == "error" || d.code == "chart.empty-region" || d.code.contains("excluded")
            })
            .map(|d| format!("{}: {}", d.code, d.message))
            .collect();
        assert!(bad.is_empty(), "{name}: {bad:?}");
        drawn += 1;
    }
    assert!(drawn >= 20, "the examples were not found");
}

/// The every-block report renders every chart without an error: an unclosed fence or a retired
/// key in one block fails that block and the gates that render this report.
#[test]
fn the_every_block_report_has_no_error_diagnostics() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/every-block");
    let source = fs::read_to_string(format!("{root}/report.rmd")).expect("every-block report");
    // The report's own CSV files are supplied, so every chart that reads one is checked too.
    let mut files: std::collections::BTreeSet<String> = ["report.rmd".to_string()].into();
    let mut data = std::collections::BTreeMap::new();
    for entry in fs::read_dir(format!("{root}/data"))
        .expect("every-block data")
        .flatten()
    {
        let name = format!("data/{}", entry.file_name().to_string_lossy());
        data.insert(
            name.clone(),
            fs::read_to_string(entry.path()).expect("a CSV file"),
        );
        files.insert(name);
    }
    assert!(data.len() >= 2, "the data files were not found");
    let options = richview_core::document::RenderOptions {
        report_folder: Some(richview_core::document::ReportFolder {
            root: "every-block".into(),
            source: "report.rmd".into(),
            files: files.into_iter().collect(),
        }),
        data,
        ..Default::default()
    };
    let report =
        richview_core::document::check(&source, &options).unwrap_or_else(|d| panic!("{d:?}"));
    let errors: Vec<_> = report
        .diagnostics
        .iter()
        // The test supplies the CSV files but not the media, which do not resolve here.
        .filter(|d| d.level == "error" && d.code != "report.asset.missing")
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
}
