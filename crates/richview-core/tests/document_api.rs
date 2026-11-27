//! The document boundary used by hosts: caller-owned resources, worker round trips and diagnostics.
use richview_core::document as api;
use serde_json::json;

fn options(files: &[&str]) -> api::RenderOptions {
    api::RenderOptions {
        report_folder: Some(api::ReportFolder {
            root: "document-api".into(),
            source: "report.rmd".into(),
            files: std::iter::once("report.rmd")
                .chain(files.iter().copied())
                .map(str::to_owned)
                .collect(),
        }),
        output: api::Output::Report,
        page: api::Page::A4,
        ..Default::default()
    }
}

#[test]
fn hosts_inventory_and_supply_resources_before_rendering_a_document() {
    let source = "---\ntitle: Shared document\ndescription: Host-supplied resources\nversion: '0.0.1'\n---\n\n```table\ntitle: Ledger\ndata: {csv: data.csv}\n```\n\n*Source: synthetic data.*\n\n```video\nsrc: data:video/mp4;base64,AA==\ntitle: Clip\ncaptions: captions.vtt\ntranscript: transcript.md\n```\n";
    let mut document: api::Document = api::parse(source).unwrap();
    assert!(!api::validate(source).iter().any(|d| d.level == "error"));
    assert!(
        !api::validate_publish(source)
            .iter()
            .any(|d| d.level == "error")
    );
    let resources: Vec<api::ReportResource> = api::report_resources(&document);
    assert_eq!(
        resources
            .iter()
            .map(|r| (r.path.as_str(), r.kind))
            .collect::<Vec<_>>(),
        [
            ("captions.vtt", "media"),
            ("data.csv", "data"),
            ("transcript.md", "media")
        ]
    );
    let mut options = options(&["captions.vtt", "data.csv", "transcript.md"]);
    api::validate_report_folder(options.report_folder.as_ref()).unwrap();
    api::validate_report_document(&document, options.report_folder.as_ref()).unwrap();
    api::attach_asset_findings(&mut document, options.report_folder.as_ref()).unwrap();
    options.data.extend([
        ("data.csv".into(), "name,n\nAlpha,1234.56\n".into()),
        (
            "captions.vtt".into(),
            "WEBVTT\n\n00:00.000 --> 00:01.000\nSpoken words\n".into(),
        ),
        ("transcript.md".into(), "Spoken **words**.".into()),
    ]);
    let report: api::RenderedReport = api::render_document(document, &options).unwrap();
    assert!(
        !report.diagnostics.iter().any(|d| d.level == "error"),
        "{:?}",
        report.diagnostics
    );
    assert!(report.html.contains("Ledger") && report.html.contains("Alpha"));
    assert!(
        report.html.contains("kind=\"captions\"") && report.html.contains("<strong>words</strong>")
    );
    assert!(
        api::render(source, &options)
            .unwrap()
            .html
            .contains("Ledger")
    );
}

#[test]
fn worker_documents_keep_diagnostic_identity_and_owned_blocks() {
    let source = "---\ntitle: Worker document\nmystery: kept\ntitle_image: {src: 'data:image/png;base64,AA==', alt: Illustration}\n---\n\n<!--richview:block:0-->\n\n```metric\nvalue: 12\nlabel: Owned once\n```\n\n<b>Raw prose</b>\n";
    let document = api::parse(source).unwrap();
    let diagnostic: api::Diagnostic = document
        .diagnostics
        .iter()
        .find(|d| d.code == "frontmatter.unknown-key")
        .unwrap()
        .clone();
    let encoded = serde_json::to_vec(&document).unwrap();
    let document: api::Document = serde_json::from_slice(&encoded).unwrap();
    let report = api::render_document(document, &options(&[])).unwrap();
    assert!(report.diagnostics.contains(&diagnostic));
    assert_eq!(
        report.diagnostics,
        api::render(source, &options(&[])).unwrap().diagnostics
    );
    assert_eq!(report.html.matches("class=\"rv-metric__value\"").count(), 1);
    assert!(!report.html.contains("<!--richview:block:0-->"));
    let checked: api::CheckReport = api::check(source, &options(&[])).unwrap();
    assert!(checked.diagnostics.contains(&diagnostic));
    let checked_document =
        api::check_document(serde_json::from_slice(&encoded).unwrap(), &options(&[])).unwrap();
    assert_eq!(checked.diagnostics, checked_document.diagnostics);
    let result = api::operation_result(checked.diagnostics, json!({"output": "report.html"}));
    assert_eq!(result["output"], "report.html");
    assert!(
        result["diagnostics"]
            .as_array()
            .unwrap()
            .contains(&serde_json::to_value(diagnostic).unwrap())
    );
    // Older worker documents remain readable; their absent provenance means unknown positions.
    let mut legacy: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
    let fields = legacy.as_object_mut().unwrap();
    fields.remove("body_line");
    fields.remove("frontmatter_lines");
    let report =
        api::render_document(serde_json::from_value(legacy).unwrap(), &options(&[])).unwrap();
    assert_eq!(report.html.matches("class=\"rv-metric__value\"").count(), 1);
}

#[test]
fn authoring_knowledge_is_available_without_a_host() {
    let grammar: api::LearnSection = api::learn_section("grammar").unwrap();
    assert_eq!(grammar.kind, "grammar");
    assert_eq!(
        api::learn_sections()
            .iter()
            .map(|section| section.kind.as_str())
            .collect::<Vec<_>>(),
        [
            "grammar",
            "chart-contract",
            "style",
            "workflow",
            "anti-slop",
            "slides"
        ]
    );
    assert!(grammar.body.starts_with("Start a first report:"));
    for step in [
        "richview init my-report",
        "my-report/report.rmd",
        "my-report/data/",
        "richview validate my-report/report.rmd",
        "richview check my-report/report.rmd",
        "richview generate my-report/report.rmd",
        "my-report/Rendered/report.html",
        "$HOME/.richview/bin/richview",
    ] {
        assert!(
            grammar.body.contains(step),
            "first-report guide omits {step}"
        );
    }
    assert!(
        api::learn_sections()
            .iter()
            .any(|section| section.kind == grammar.kind)
    );
    assert!(api::learn_bundle().contains(&grammar.title));
    // Authors paste inline YAML examples; Markdown raw strings must not add transport escapes.
    let versions: Vec<_> = grammar
        .body
        .split('`')
        .skip(1)
        .step_by(2)
        .filter(|example| example.starts_with("version:"))
        .collect();
    assert!(!versions.is_empty(), "grammar must teach a version example");
    for example in versions {
        let source = format!(
            "---\ntitle: Copied example\ndescription: A report using the grammar example\n{example}\n---\n\nBody.\n"
        );
        let document = api::parse(&source).unwrap();
        assert_eq!(document.frontmatter["version"], "1.0.0", "{example}");
        let findings = api::validate_publish(&source);
        assert!(
            !findings.iter().any(|d| d.level == "error"),
            "{example}: {findings:?}"
        );
    }
}

#[test]
fn slides_have_a_learn_section_of_their_own_that_the_grammar_points_to() {
    // The grammar names the format and where its guide is, so it stays within one tool output.
    let grammar = api::learn_section("grammar").unwrap().body;
    assert!(grammar.contains("`richview learn slides`"));
    assert!(grammar.contains("`.rslide`"));
    assert!(!grammar.contains("slides.density"));
    let slides = api::learn_section("slides").unwrap();
    assert_eq!(slides.title, "Slides");
    assert!(slides.body.starts_with("A deck is a `.rslide` file"));
    assert!(!slides.body.to_lowercase().contains("experimental"));
    for named in [
        "format: rslide/1",
        "slides.density",
        "deck.layout",
        "deck.span",
    ] {
        assert!(slides.body.contains(named), "{named}");
    }
    // The guide's placeholders are filled from the registry and the layout table.
    assert!(!slides.body.contains('@'), "an unfilled placeholder");
    assert!(slides.body.contains("left-right") || slides.body.contains("`left`"));
}

#[test]
fn connector_queries_are_taught_in_the_workflow_section_that_the_grammar_points_to() {
    // The grammar names the source and where its guide is, so it stays within one tool output.
    let grammar = api::learn_section("grammar").unwrap().body;
    assert!(grammar.contains("`report.connectors.yaml`"));
    assert!(grammar.contains("`richview learn workflow`"));
    assert!(!grammar.contains("RICHVIEW_CONNECTORS"));
    let workflow = api::learn_section("workflow").unwrap().body;
    for named in [
        "### SQL and other connector queries",
        "kind: duckdb",
        "cache: { ttl: 1h }",
        "RICHVIEW_CONNECTORS",
    ] {
        assert!(workflow.contains(named), "{named}");
    }
}

#[test]
fn every_learn_section_fits_one_tool_output() {
    // Claude Code shows 30,000 characters of one command's output and spills the rest to a
    // file, so a section an agent asks for by name must arrive whole.
    for section in api::learn_sections() {
        let length = section.body.chars().count();
        assert!(
            length < 30_000,
            "learn {} is {length} characters; move text to another section",
            section.kind
        );
    }
}
