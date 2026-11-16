use richview_core::document::{RenderOptions, ReportFolder, parse, render};
use richview_editor::authoring::diagnostics;
use serde_json::{Value, json};

fn source_error(source: &str, data: Option<(&str, &str)>) -> Value {
    let mut options = RenderOptions::default();
    let mut files = vec!["report.rmd".to_string()];
    if let Some((path, content)) = data {
        options.data.insert(path.into(), content.into());
        files.push(path.into());
    }
    options.report_folder = Some(ReportFolder {
        root: "editor-source-ranges".into(),
        source: "report.rmd".into(),
        files,
    });
    let document = parse(source).unwrap();
    let report = render(source, &options).unwrap();
    let original = report
        .diagnostics
        .iter()
        .find(|d| d.code == "chart.source-error")
        .unwrap_or_else(|| panic!("Expected a chart source error: {:?}", report.diagnostics))
        .clone();
    let mut issues = diagnostics(source, report.diagnostics, &document, &options);
    let issue = issues
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|d| d["code"] == "chart.source-error")
        .unwrap()
        .take();
    let mut metadata = issue.clone();
    metadata.as_object_mut().unwrap().remove("range");
    assert_eq!(
        metadata,
        serde_json::to_value(original).unwrap(),
        "Location must preserve the complete original diagnostic"
    );
    issue
}

fn assert_token(issue: &Value, source: &str, token: &str) {
    let prefix = &source[..source.find(token).unwrap()];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count();
    let character = prefix.rsplit('\n').next().unwrap().encode_utf16().count();
    assert_eq!(
        issue["range"],
        json!({"start":{"line":line,"character":character},"end":{"line":line,"character":character+token.encode_utf16().count()}}),
        "{token}: {issue}"
    );
}

#[test]
fn interval_diagnostic_points_to_the_original_row_before_faceting() {
    let source = "```chart\ntype: interval\ntitle: Authored ranges\ndata:\n  inline:\n    - {category: A, panel: Earlier, lo: 1, hi: 2}\n    - {category: B, panel: Earlier, lo: 2, hi: 3}\n    - category: C\n      panel: Later\n      lo: 9\n      hi: 7\nencoding: {x: category, lower: lo, upper: hi, facet: panel}\n```\n";
    let issue = source_error(source, None);
    assert_eq!(issue["details"]["path"], "/data/inline/2/lo");
    assert_eq!(
        issue["range"],
        json!({"start":{"line":9,"character":6},"end":{"line":9,"character":8}})
    );
}

#[test]
fn quoted_escaped_and_unicode_keys_keep_their_actual_utf16_source_spans() {
    for (spelling, name) in [
        ("\"lo/~.🦀\"", "lo/~.🦀"),
        (r#""lo\u002Fbound""#, "lo/bound"),
        ("'lo''bound'", "lo'bound"),
        (r#""lo\"bound""#, "lo\"bound"),
        ("lo🦀", "lo🦀"),
        ("lo.part", "lo.part"),
    ] {
        let encoded = serde_json::to_string(name).unwrap();
        let source = format!(
            "# 🦀 Prefix\r\n\r\n```chart\r\ntype: interval\r\ntitle: Authored ranges\r\ndata: {{inline: [{{category: \"🦀 First\", {spelling}: 5, hi: 4}}]}}\r\nencoding: {{x: category, lower: {encoded}, upper: hi}}\r\n```\r\n"
        );
        let issue = source_error(&source, None);
        assert_token(&issue, &source, spelling);
    }
}

#[test]
fn numeric_source_errors_in_existing_chart_types_select_the_actual_field() {
    let source = "```chart\ntype: bar\ntitle: Original readings\ndata:\n  inline:\n    - {category: First, value: 1}\n    - category: Second\n      value: '1e-400'\nencoding: {x: category, y: value}\n```\n";
    let issue = source_error(source, None);
    assert_eq!(issue["details"]["path"], "/data/inline/1/value");
    assert_eq!(
        issue["range"],
        json!({"start":{"line":7,"character":6},"end":{"line":7,"character":11}})
    );
}

#[test]
fn external_rows_select_the_written_reference_without_inventing_file_positions() {
    for (channel, path, content, spelling) in [
        (
            "csv",
            "data/ranges.csv",
            "category,lo,hi\nFirst,5,4\n",
            "data/ranges.csv",
        ),
        (
            "csv",
            "data/ranges.csv",
            "category,lo,hi\nFirst,5,4\n",
            "'data/ranges.csv'",
        ),
        (
            "csv",
            "data/ranges.csv",
            "category,lo,hi\nFirst,5,4\n",
            r#""data/ranges\u002Ecsv""#,
        ),
        (
            "json",
            "data/ranges.json",
            r#"[{"category":"First","lo":5,"hi":4}]"#,
            "data/ranges.json",
        ),
    ] {
        let source = format!(
            "```chart\ntype: interval\ntitle: Authored ranges\ndata: {{{channel}: {spelling}}}\nencoding: {{x: category, lower: lo, upper: hi}}\n```\n"
        );
        let issue = source_error(&source, Some((path, content)));
        assert_eq!(issue["details"]["path"], "/data/inline/0/lo");
        assert_token(&issue, &source, spelling);
    }
}

#[test]
fn invalid_row_schema_keeps_its_location_and_an_anchor_points_at_itself() {
    let source = "```chart\ntype: interval\ntitle: Authored ranges\ndata: {inline: [\"invalid 🦀\"]}\nencoding: {x: category, lower: lo, upper: hi}\n```\n";
    let document = parse(source).unwrap();
    let issues = diagnostics(
        source,
        document.diagnostics.clone(),
        &document,
        &RenderOptions::default(),
    );
    // The row that is not a mapping is the finding's place (TYP-38), and the editor marks the
    // row's whole token, counted in UTF-16 units.
    let issue = issues
        .as_array()
        .unwrap()
        .iter()
        .find(|issue| issue["path"] == "chart.data.inline[0]")
        .unwrap_or_else(|| panic!("{issues}"));
    assert_eq!(issue["code"], "data.inline", "{issue}");
    assert_token(issue, source, "\"invalid 🦀\"");
    // The YAML profile refuses anchors and aliases (TYP-02); the editor marks the anchor.
    let source = "```chart\ntype: interval\ntitle: Authored ranges\nrecords: &rows [{category: First, lo: 5, hi: 4}]\ndata: {inline: *rows}\nencoding: {x: category, lower: lo, upper: hi}\n```\n";
    let document = parse(source).unwrap();
    let issues = diagnostics(
        source,
        document.diagnostics.clone(),
        &document,
        &RenderOptions::default(),
    );
    let issue = issues
        .as_array()
        .unwrap()
        .iter()
        .find(|issue| issue["code"] == "yaml.unsupported")
        .unwrap_or_else(|| panic!("{issues}"));
    assert_eq!(
        issue["range"]["start"],
        json!({"line": 3, "character": 15}),
        "{issue}"
    );
}
