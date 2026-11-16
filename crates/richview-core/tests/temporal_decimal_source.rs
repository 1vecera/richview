mod common;

use richview_core::document::{RenderOptions, ReportFolder, render};
use serde_json::json;

/// Inline decimal text in a faceted time series reaches every panel and group readout at the
/// author's precision, never with the source's extra digits (FEA-CH-37, TYP-36).
#[test]
fn inline_decimal_text_reaches_each_temporal_facet_at_the_authors_precision() {
    let values = [
        ("First", "1.234567890123456789", "1.23"),
        ("Second", "9.876543210987654321", "9.88"),
    ];
    let mut rows = Vec::new();
    for panel in ["Left", "Right"] {
        for date in ["2024-03", "2024-01"] {
            for (group, value, _) in values {
                rows.push(json!({"panel":panel,"date":date,"group":group,"value":value}));
            }
        }
    }
    let chart = json!({"type":"line","title":"Readings","encoding":{"x":"date","y":"value","color":"group","facet":"panel"},"data":{"inline":rows}});
    let source = format!("```chart\n{chart}\n```\n");
    let options = RenderOptions {
        report_folder: Some(ReportFolder {
            root: "decimal-source".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }),
        ..Default::default()
    };
    let output = render(&source, &options).unwrap();
    assert!(!output.diagnostics.iter().any(|d| d.level == "error"));
    let payload = common::payload(&output.charts[0]);
    for row in &payload.rows {
        assert!(
            row.iter().all(|v| !v.to_string().contains("1.234567")),
            "{row:?}"
        );
    }
    let g = common::geometry(&output.charts[0], 720.0);
    assert_eq!(g.panels.len(), 2);
    for panel in &g.panels {
        let first = &panel.observations[0].readout;
        assert!(first.heading.contains("2024"), "{}", first.heading);
        for (name, _, shown) in values {
            let line = first.lines.iter().find(|l| l.name == name).unwrap();
            assert!(line.value.starts_with(shown), "{name}: {}", line.value);
        }
    }
}
