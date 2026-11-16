#[cfg(target_arch = "wasm32")]
mod docs_ui;
#[cfg(target_arch = "wasm32")]
mod preview_frames;
// Rust browser host: chart interactions, report authoring and theme controls execute in Wasm.
#[cfg(target_arch = "wasm32")]
mod browser;
// The live example's scene and its completion run in the browser; their pure parts and the site
// checks build natively.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
mod completion;
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
mod reveal;
pub mod share;
pub mod source_buffer;
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
mod tools;
#[cfg(target_arch = "wasm32")]
pub use browser::*;

/// Derive the live company preview through the same validated style cascade as report rendering.
pub fn company_style_json(input: &str) -> String {
    use serde_json::{Value, json};
    let failure = |message: &str| {
        json!({"tokens": {}, "palette": [], "diagnostics": [{"level": "warning", "code": "style.schema", "message": message}]}).to_string()
    };
    let Ok(Value::Object(mut company)) = serde_json::from_str::<Value>(input) else {
        return failure("Company style input must be a JSON object");
    };
    company.retain(|_, value| !value.is_null());
    if !company.contains_key("color") {
        return failure("Company style needs a color");
    }
    let source = json!({"company": company}).to_string();
    let mut diagnostics = vec![];
    let style = richview_core::resolve_styles(&[source], &mut diagnostics);
    // The demo needs no embedded font catalog. Its absence must not suppress the palette.
    diagnostics.retain(|d| {
        d.code.starts_with("style.company-")
            || d.code == "style.schema"
            || d.code == "style.unknown-key"
    });
    let failed = diagnostics
        .iter()
        .any(|d| d.level == "warning" || d.level == "error");
    let findings: Vec<_> = diagnostics
        .into_iter()
        .map(|d| json!({"level":d.level, "code":d.code, "message":d.message}))
        .collect();
    if failed {
        return json!({"tokens": {}, "palette": [], "diagnostics": findings}).to_string();
    }
    let names = [
        "bg",
        "bg-elev",
        "bg-chart",
        "rule",
        "rule-soft",
        "ink",
        "chart-ink",
        "ink-soft",
        "ink-faint",
        "accent",
        "accent-soft",
        "link",
    ];
    let tokens: serde_json::Map<String, Value> = names
        .into_iter()
        .filter_map(|name| {
            style.document["tokens"]
                .get(name)
                .map(|v| (name.into(), v.clone()))
        })
        .collect();
    json!({"tokens": tokens, "palette": style.chart["palette"], "diagnostics": findings})
        .to_string()
}

pub fn render_source_native(source: &str, options: &str) -> Result<String, String> {
    let options: richview_core::document::RenderOptions =
        serde_json::from_str(options).map_err(|e| e.to_string())?;
    let report = richview_core::document::render(source, &options)
        .map_err(|errors| serde_json::to_string(&errors).unwrap_or_default())?;
    serde_json::to_string(&report).map_err(|e| e.to_string())
}

/// A draft as the playground renders it: the parsed source with every file finding attached, and
/// the files to fetch. A missing, escaping or remote file is a finding on its element and the rest
/// renders, as in the native host (FEA-RP-30); only an unusable folder stops the draft, with its
/// findings, as does a source that cannot be parsed.
#[allow(clippy::type_complexity)]
pub fn prepare_draft(
    source: &str,
    folder: &richview_core::document::ReportFolder,
) -> Result<
    (
        richview_core::document::Document,
        Vec<richview_core::document::ReportResource>,
    ),
    Vec<richview_core::document::Diagnostic>,
> {
    let mut document = richview_core::document::parse(source)?;
    richview_core::document::attach_asset_findings(&mut document, Some(folder))?;
    let files = folder_files(&document, folder);
    Ok((document, files))
}

/// The files a host fetches for a report: those the folder inventory holds, never a link. A file
/// the folder lacks already carries its finding, so fetching it would only fail the whole preview.
pub fn folder_files(
    document: &richview_core::document::Document,
    folder: &richview_core::document::ReportFolder,
) -> Vec<richview_core::document::ReportResource> {
    // A report-relative path as an inventory names it, without `./` segments.
    let named = |path: &str| {
        path.split('/')
            .filter(|part| *part != ".")
            .collect::<Vec<_>>()
            .join("/")
    };
    let held: std::collections::BTreeSet<String> = folder.files.iter().map(|f| named(f)).collect();
    richview_core::document::report_resources(document)
        .into_iter()
        .filter(|r| {
            r.kind != "link"
                && richview_core::document::report_relative_path(&r.path)
                && held.contains(&named(&r.path))
        })
        .collect()
}

/// `validate`: the structure of the report and, when the host names the report folder, its files,
/// as one result object with every finding (ARC-08: the same answer as `richview validate`).
pub fn validate_result(
    source: &str,
    folder: Option<richview_core::document::ReportFolder>,
) -> serde_json::Value {
    let (blocks, diagnostics) = match richview_core::document::parse(source) {
        Err(found) => (0, found),
        Ok(mut document) => {
            let own = document.diagnostics.clone();
            match folder {
                None => (document.blocks.len(), own),
                Some(folder) => {
                    match richview_core::document::attach_asset_findings(
                        &mut document,
                        Some(&folder),
                    ) {
                        Ok(()) => (document.blocks.len(), document.diagnostics),
                        Err(mut found) => {
                            found.extend(own);
                            found.dedup();
                            (0, found)
                        }
                    }
                }
            }
        }
    };
    richview_core::document::operation_result(diagnostics, serde_json::json!({"blocks": blocks}))
}

/// `check`: render and return every finding and each chart's geometry, as one result object.
pub fn check_result(
    source: &str,
    options: &richview_core::document::RenderOptions,
) -> serde_json::Value {
    let mut document = match richview_core::document::parse(source) {
        Ok(document) => document,
        Err(found) => {
            return richview_core::document::operation_result(found, serde_json::json!({}));
        }
    };
    if options.report_folder.is_some()
        && let Err(found) = richview_core::document::attach_asset_findings(
            &mut document,
            options.report_folder.as_ref(),
        )
    {
        return richview_core::document::operation_result(found, serde_json::json!({}));
    }
    match richview_core::document::check_document(document, options) {
        Ok(report) => richview_core::document::operation_result(
            report.diagnostics,
            serde_json::json!({"charts": report.charts}),
        ),
        Err(found) => richview_core::document::operation_result(found, serde_json::json!({})),
    }
}

#[cfg(test)]
mod result_tests {
    use super::*;

    #[test]
    fn company_demo_returns_the_report_cascade_and_rejects_bad_input() {
        let input = r##"{"color":"#0B6E69","secondary":null,"base":"air"}"##;
        let output: serde_json::Value = serde_json::from_str(&company_style_json(input)).unwrap();
        let mut diagnostics = vec![];
        let style = richview_core::resolve_styles(
            &["company:\n  color: '#0B6E69'\n  base: air\n".into()],
            &mut diagnostics,
        );
        assert_eq!(output["palette"], style.chart["palette"]);
        assert_eq!(
            output["tokens"]["accent"],
            style.document["tokens"]["accent"]
        );
        assert_eq!(output["tokens"]["bg"], style.document["tokens"]["bg"]);
        assert_eq!(output["diagnostics"], serde_json::json!([]));
        for bad in ["{}", "[]", "not json", r##"{"color":"not a colour"}"##] {
            let found: serde_json::Value = serde_json::from_str(&company_style_json(bad)).unwrap();
            assert_eq!(found["palette"], serde_json::json!([]), "{bad}");
            assert_eq!(found["diagnostics"][0]["code"], "style.schema", "{bad}");
        }
    }

    const MISSING: &str =
        include_str!("../../../tools/rv-gate/fixtures/contract/missing-data/report.rmd");

    fn folder() -> richview_core::document::ReportFolder {
        richview_core::document::ReportFolder {
            root: "broken".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }
    }

    #[test]
    fn validate_reports_every_finding_in_the_shared_envelope() {
        let result = validate_result(MISSING, Some(folder()));
        assert_eq!(result["ok"], false);
        assert_eq!(result["status"], "issues");
        let codes: Vec<&str> = result["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|d| d["code"].as_str())
            .collect();
        assert!(codes.contains(&"report.asset.missing"), "{codes:?}");
        assert!(
            codes.len() >= 2,
            "a missing file must not hide the other findings: {codes:?}"
        );
    }

    /// The playground's own folder for a new draft: the source alone.
    fn draft_folder() -> richview_core::document::ReportFolder {
        richview_core::document::ReportFolder {
            root: "playground-report".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }
    }

    const DRAFT: &str = "---\ntitle: Revenue rose\nstyles: [scientific]\n---\n\n## What moved\n\n![Company logo](logo.png)\n\n```chart\ntype: bar\ntitle: Revenue by month\ndata:\n  inline:\n    - {month: Jan, revenue: 100}\n    - {month: Feb, revenue: 140}\nencoding: {x: month, y: revenue}\n```\n\n```chart\ntype: line\ntitle: Orders from the file\ndata: {csv: sales.csv}\nencoding: {x: month, y: orders}\n```\n";

    #[test]
    fn a_draft_naming_files_its_folder_lacks_renders_with_one_finding_each() {
        // Daniel, 4 Nov: "the playground did not work". A draft naming an image or a data file
        // blanked the whole preview and printed the findings as raw JSON.
        let (document, files) = prepare_draft(DRAFT, &draft_folder()).expect("the draft renders");
        assert!(
            files.is_empty(),
            "nothing the folder lacks is fetched: {files:?}"
        );
        let options = richview_core::document::RenderOptions {
            report_folder: Some(draft_folder()),
            ..Default::default()
        };
        let report = richview_core::document::render_document(document, &options)
            .expect("a missing file fails only its element");
        let missing: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == "report.asset.missing")
            .map(|d| d.path.as_str())
            .collect();
        assert_eq!(
            missing,
            ["markdown", "chart.data.csv"],
            "{:?}",
            report.diagnostics
        );
        assert!(
            report.html.contains("Company logo"),
            "the image's alt text stands in for it"
        );
        assert!(
            report.html.contains("Revenue by month"),
            "the other chart renders"
        );
        assert!(report.html.contains("What moved"), "the text renders");
    }

    #[test]
    fn only_files_the_folder_holds_are_fetched() {
        let folder = richview_core::document::ReportFolder {
            root: "orders".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into(), "data/sales.csv".into()],
        };
        let source = "---\ntitle: Orders\n---\n\n[Notes](notes.md) ![Logo](media/logo.svg) ![Remote](https://example.com/a.png)\n\n```chart\ntype: line\ntitle: Orders\ndata: {csv: ./data/sales.csv}\nencoding: {x: month, y: orders}\n```\n";
        let (_, files) = prepare_draft(source, &folder).expect("the draft renders");
        let fetched: Vec<_> = files.iter().map(|f| (f.path.as_str(), f.kind)).collect();
        assert_eq!(fetched, [("./data/sales.csv", "data")]);
    }

    #[test]
    fn an_unusable_folder_stops_the_draft_with_its_findings() {
        let shared = richview_core::document::ReportFolder {
            root: "shared".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into(), "another.rmd".into()],
        };
        let found = prepare_draft(DRAFT, &shared).expect_err("a shared folder is unusable");
        assert!(
            found.iter().any(|d| d.code == "report.root.shared"),
            "{found:?}"
        );
    }

    #[test]
    fn check_carries_the_same_envelope_and_the_charts() {
        let options = richview_core::document::RenderOptions {
            report_folder: Some(folder()),
            ..Default::default()
        };
        let result = check_result(MISSING, &options);
        assert!(result["ok"].is_boolean() && result["status"].is_string());
        assert!(result["charts"].is_array() || result["diagnostics"].is_array());
    }
}
