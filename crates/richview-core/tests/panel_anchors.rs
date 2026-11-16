//! FEA-BR-48: only explicit anchors named by tab panels belong to the authored heading vocabulary.
use richview_core::document::{RenderOptions, ReportFolder, render};

fn report(source: &str) -> richview_core::document::RenderedReport {
    render(
        source,
        &RenderOptions {
            report_folder: Some(ReportFolder {
                root: "anchors".into(),
                source: "report.rmd".into(),
                files: vec!["report.rmd".into()],
            }),
            ..Default::default()
        },
    )
    .unwrap()
}

const TABS: &str =
    "---\ntitle: Report\ntabs:\n  panels:\n    - {id: sales, label: Sales, from: sales}\n---\n\n";

#[test]
fn tab_defaults_and_export_modes_preserve_the_expected_views() {
    use richview_core::document::Output;

    for (default, selected, warns) in [
        ("", "one", false),
        ("  default: one\n", "one", false),
        ("  default: two\n", "two", false),
        ("  default: missing\n", "one", true),
    ] {
        for (export, active) in [
            ("", false),
            ("  export: all\n", false),
            ("  export: active\n", true),
        ] {
            let source = format!(
                "---\ntitle: Tabs\ntabs:\n{default}{export}  panels:\n    - {{id: one, label: One, from: one}}\n    - {{id: two, label: Two, from: two}}\n---\n\nIntroduction stays.\n\n## One {{#one}}\n\nFirst panel body.\n\n## Two {{#two}}\n\nSecond panel body.\n"
            );
            for output in [Output::Report, Output::Pdf] {
                let out = render(
                    &source,
                    &RenderOptions {
                        output,
                        report_folder: Some(ReportFolder {
                            root: "tab-defaults".into(),
                            source: "report.rmd".into(),
                            files: vec!["report.rmd".into()],
                        }),
                        ..Default::default()
                    },
                )
                .unwrap();
                let case = format!("default={default:?}, export={export:?}, output={output:?}");
                assert!(out.html.contains("Introduction stays."), "{case}");
                assert_eq!(out.html.matches(" data-rv-default").count(), 1, "{case}");
                let start = out.html.find(&format!("id=\"panel-{selected}\"")).unwrap();
                let tag = out.html[start..].split('>').next().unwrap();
                assert!(tag.contains(" data-rv-default"), "{case}: {tag}");
                for (id, text) in [("one", "First panel body."), ("two", "Second panel body.")] {
                    let shown = output == Output::Report || !active || id == selected;
                    assert_eq!(out.html.contains(text), shown, "{case}: {id} body");
                    assert_eq!(
                        out.html.contains(&format!("id=\"panel-{id}\"")),
                        shown,
                        "{case}: {id} section"
                    );
                }
                if output == Output::Report || !active {
                    assert!(
                        out.html.find("First panel body.") < out.html.find("Second panel body."),
                        "{case}: author order survives default selection"
                    );
                }
                let findings: Vec<_> = out
                    .diagnostics
                    .iter()
                    .filter(|d| d.code == "frontmatter.tabs")
                    .collect();
                assert_eq!(findings.len(), usize::from(warns), "{case}: {findings:?}");
                if warns {
                    assert_eq!(findings[0].level, "warning", "{case}");
                    assert_eq!(findings[0].path, "frontmatter.tabs.default", "{case}");
                }
            }
        }
    }
}

#[test]
fn only_explicit_ids_named_by_panels_survive_and_other_ids_warn_at_their_heading() {
    let source = format!("{TABS}## Sales {{#sales}}\n\nText.\n\n### Other {{#custom}}\n");
    let out = report(&source);
    assert!(out.html.contains("<h2 id=\"sales\">") && out.html.contains("id=\"panel-sales\""));
    assert!(out.html.contains("<h3 id=\"other\">"));
    assert!(!out.html.contains("id=\"custom\"") && !out.html.contains("{#custom}"));
    let diagnostic = out
        .diagnostics
        .iter()
        .find(|d| d.code == "markdown.heading-id")
        .expect("unreferenced id warning");
    assert_eq!(diagnostic.level, "warning");
    assert_eq!(
        diagnostic.line,
        Some(
            source
                .lines()
                .position(|l| l.starts_with("### Other"))
                .unwrap()
                + 1
        )
    );
}

#[test]
fn an_id_outside_a_tabbed_report_is_ignored_and_the_generated_anchor_stays() {
    let out = report("## Method {#custom}\n\nBody.\n");
    assert!(out.html.contains("<h2 id=\"method\">"));
    assert!(!out.html.contains("{#custom}") && !out.html.contains("id=\"custom\""));
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.code == "markdown.heading-id" && d.level == "warning")
    );
}

#[test]
fn a_generated_heading_id_alone_cannot_start_a_panel() {
    let out = report(&format!("{TABS}## Sales\n\nBody stays.\n"));
    assert!(out.html.contains("id=\"sales\"") && out.html.contains("Body stays."));
    assert!(!out.html.contains("id=\"panel-sales\""));
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.code == "tabs.anchor-missing")
    );
}

#[test]
fn an_earlier_generated_slug_cannot_take_the_explicit_panel_anchor() {
    let source = format!(
        "{TABS}## Sales\n\nIntro before the panel.\n\n## Results {{#sales}}\n\nPanel body.\n"
    );
    let out = report(&source);
    assert_eq!(out.html.matches("<h2 id=\"sales\">").count(), 1);
    let start = out.html.find("id=\"panel-sales\"").unwrap();
    assert!(out.html.find("Intro before the panel.").unwrap() < start);
    assert!(out.html[start..].contains("<h2 id=\"sales\">Results</h2>"));
}

#[test]
fn panel_ids_and_anchors_follow_the_slug_grammar() {
    for field in ["id", "from"] {
        for bad in [
            "Sales",
            "sales_tab",
            "-sales",
            "sales--detail",
            &"x".repeat(81),
        ] {
            let source = format!(
                "{}## Sales {{#sales}}\n\nBody.\n",
                TABS.replace(&format!("{field}: sales"), &format!("{field}: {bad}"))
            );
            let out = report(&source);
            assert!(
                out.diagnostics
                    .iter()
                    .any(|d| d.code == "frontmatter.tabs" && d.level == "warning"),
                "{field}={bad}: {:?}",
                out.diagnostics
            );
            assert!(!out.html.contains("id=\"panel-sales\""));
        }
    }
}

#[test]
fn panel_validation_points_at_the_failing_field() {
    let source = "---\ntitle: Tabs\ntabs:\n  export: all\n  default: one\n  panels:\n    - id: one\n      label: One\n      from: one\n    - id: two\n      label: Two\n      from: two\n---\n\n## One {#one}\n\n## Two {#two}\n";
    for (before, after, path, line, column, message) in [
        (
            "id: two",
            "id: one",
            "panels[1].id",
            10,
            7,
            "frontmatter.tabs.panels[1].id must be unique",
        ),
        (
            "from: two",
            "from: Bad",
            "panels[1].from",
            12,
            7,
            "frontmatter.tabs.panels[1].from:",
        ),
        (
            "label: Two",
            "label: ''",
            "panels[1].label",
            11,
            7,
            "frontmatter.tabs.panels[1].label must be a non-empty string",
        ),
        (
            "export: all",
            "export: wrong",
            "export",
            4,
            3,
            "frontmatter.tabs.export must be",
        ),
        (
            "default: one",
            "default: missing",
            "default",
            5,
            3,
            "frontmatter.tabs.default \"missing\" does not match any panel id",
        ),
    ] {
        let doc = richview_core::document::parse(&source.replace(before, after)).unwrap();
        let diagnostic = doc
            .diagnostics
            .iter()
            .find(|d| d.code == "frontmatter.tabs")
            .unwrap();
        assert_eq!(diagnostic.path, format!("frontmatter.tabs.{path}"));
        assert_eq!(
            (diagnostic.line, diagnostic.column),
            (Some(line), Some(column)),
            "{diagnostic:?}"
        );
        assert_eq!(diagnostic.tag.as_deref(), Some("frontmatter"));
        assert!(diagnostic.message.starts_with(message), "{diagnostic:?}");
    }
}

#[test]
fn panel_render_findings_keep_nested_positions_across_worker_documents() {
    let source = "---\ntitle: Tabs\ntabs:\n  panels:\n    - id: one\n      label: One\n      from: one\n    - id: two\n      label: Two\n      from: two\n---\n\n## One {#one}\n\nFirst panel body.\n\n## Two {#two}\n\nSecond panel body.\n";
    for (source, code, line) in [
        (
            source.replace("## Two {#two}", "## Other"),
            "tabs.anchor-missing",
            10,
        ),
        (
            source
                .replace("from: one", "from: two")
                .replace("      from: two\n---", "      from: one\n---"),
            "tabs.order",
            10,
        ),
        (source.replace("from: two", "from: one"), "tabs.order", 10),
    ] {
        let native = report(&source);
        assert!(!native.html.contains("<section class=\"rv-tabpanel\""));
        assert!(native.html.contains("First panel body."));
        assert!(native.html.contains("Second panel body."));
        let diagnostic = native.diagnostics.iter().find(|d| d.code == code).unwrap();
        assert_eq!(diagnostic.path, "frontmatter.tabs.panels[1].from");
        assert_eq!((diagnostic.line, diagnostic.column), (Some(line), Some(7)));
        assert_eq!(diagnostic.tag.as_deref(), Some("frontmatter"));
        let options = RenderOptions {
            report_folder: Some(ReportFolder {
                root: "anchors".into(),
                source: "report.rmd".into(),
                files: vec!["report.rmd".into()],
            }),
            ..Default::default()
        };
        let mut encoded =
            serde_json::to_value(richview_core::document::parse(&source).unwrap()).unwrap();
        let worker = richview_core::document::render_document(
            serde_json::from_value(encoded.clone()).unwrap(),
            &options,
        )
        .unwrap();
        assert_eq!(native.diagnostics, worker.diagnostics);
        assert_eq!(native.html, worker.html);
        // An older worker document without columns still resolves the stored field's line.
        encoded
            .as_object_mut()
            .unwrap()
            .remove("frontmatter_columns");
        let legacy = richview_core::document::render_document(
            serde_json::from_value(encoded).unwrap(),
            &options,
        )
        .unwrap();
        let diagnostic = legacy.diagnostics.iter().find(|d| d.code == code).unwrap();
        assert_eq!(diagnostic.line, Some(line));
    }
}
