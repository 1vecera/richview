mod common;

use richview_core::document::{
    RenderOptions, check as core_check, learn_bundle, learn_section, parse, render as core_render,
    render_document as core_render_document, validate, validate_publish,
};
use richview_core::{
    builtin_style_source, create_publish_request, custom_properties_to_css, parse_schema,
    render_compat as core_render_compat, render_table, resolve_styles_compat,
};
use serde_json::json;

#[test]
fn line_curve_is_validated_preserved_and_available_to_authors_and_smoothing_is_retired() {
    for curve in ["linear", "monotone", "step", "step_before", "step_after"] {
        let chart = json!({"type":"line","title":"Smoothed observations","encoding":{"x":"x","y":"y"},"data":{"inline":[]},"style":{"curve":curve}});
        assert_eq!(
            parse_schema("ChartSchema", chart).unwrap()["style"]["curve"],
            curve
        );
        assert!(richview_core::authoring_values("chart", &["style", "curve"]).contains(&curve));
    }
    for style in [
        json!({"curve":"catmull-rom"}),
        json!({"curve":1}),
        json!({"curvature":2}),
        json!({"line":{"smoothing":"straight"}}),
    ] {
        let chart = json!({"type":"line","title":"Smoothed observations","encoding":{"x":"x","y":"y"},"data":{"inline":[]},"style":style});
        assert!(parse_schema("ChartSchema", chart).is_err());
    }
}

#[test]
fn labeling_and_curve_options_survive_schema_adapters_and_reject_invalid_values() {
    let source = json!({"type":"scatter","title":"Named points","data":{"inline":[{"x":1,"y":2,"name":"Point"}]},"encoding":{"x":"x","y":"y","label":"name"},"style":{"curve":"linear","curvature":0.4}});
    let parsed = parse_schema("ChartSchema", source.clone()).unwrap();
    assert_eq!(parsed["encoding"]["label"], "name");
    assert_eq!(parsed["style"]["curve"], "linear");
    assert_eq!(parsed["style"]["curvature"], 0.4);
    for curve in ["linear", "monotone", "step", "step_before", "step_after"] {
        let mut chart = source.clone();
        chart["style"]["curve"] = json!(curve);
        let parsed = parse_schema("ChartSchema", chart).unwrap();
        assert_eq!(parsed["style"]["curve"], curve);
        assert!(richview_core::authoring_values("chart", &["style", "curve"]).contains(&curve));
    }
    for (field, invalid) in [
        ("curve", json!("spline")),
        ("curvature", json!(-0.1)),
        ("curvature", json!(1.1)),
        ("curvature", json!("0.5")),
    ] {
        let mut chart = source.clone();
        chart["style"][field] = invalid;
        assert!(
            parse_schema("ChartSchema", chart).is_err(),
            "{field} must be valid"
        );
    }
    let mut chart = source;
    chart["encoding"]["label"] = json!(7);
    assert!(parse_schema("ChartSchema", chart).is_err());
}

fn folder(options: &RenderOptions) -> RenderOptions {
    let mut options = options.clone();
    options.report_folder = Some(richview_core::document::ReportFolder {
        root: "contract-fixture".into(),
        source: "report.rmd".into(),
        files: std::iter::once("report.rmd".into())
            .chain(options.data.keys().cloned())
            .collect(),
    });
    options
}
fn render(
    source: &str,
    options: &RenderOptions,
) -> Result<richview_core::document::RenderedReport, Vec<richview_core::document::Diagnostic>> {
    core_render(source, &folder(options))
}
fn render_document(
    document: richview_core::document::Document,
    options: &RenderOptions,
) -> Result<richview_core::document::RenderedReport, Vec<richview_core::document::Diagnostic>> {
    core_render_document(document, &folder(options))
}
fn render_compat(
    source: &str,
    options: &serde_json::Value,
) -> Result<serde_json::Value, Vec<richview_core::document::Diagnostic>> {
    let mut options = options.clone();
    options["report_folder"] =
        serde_json::to_value(folder(&Default::default()).report_folder).unwrap();
    core_render_compat(source, &options)
}
fn check(
    source: &str,
    options: &RenderOptions,
) -> Result<richview_core::document::CheckReport, Vec<richview_core::document::Diagnostic>> {
    core_check(source, &folder(options))
}
fn rendered(source: &str) -> richview_core::document::RenderedReport {
    render(source, &RenderOptions::default()).unwrap()
}

#[test]
fn commonmark_fences_preserve_examples_unknown_languages_and_absolute_lines() {
    let source = "---\r\ntitle: A claim\r\ncustom: {key: preserved}\r\n---\r\n\r\n````markdown\n```chart\ntype: invalid\n```\n````\n\n~~~metric\nvalue: 42\nlabel: Result\n~~~\n\n```python\nprint('hello')\n```\n";
    let parsed = parse(source).unwrap();
    assert_eq!(parsed.frontmatter["custom"]["key"], "preserved");
    assert_eq!(parsed.blocks.len(), 1);
    assert_eq!(parsed.blocks[0].line, 12);
    assert!(parsed.body.contains("type: invalid"));
    assert!(parsed.body.contains("print('hello')"));
    // An unknown frontmatter key stays in the metadata and warns (TYP-84); nothing else is wrong.
    let codes: Vec<_> = parsed.diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, ["frontmatter.unknown-key"]);
    let report = rendered(source);
    assert!(report.html.contains("language-markdown"));
    assert!(report.html.contains("rv-metric__value\">42"));
}

#[test]
fn long_reports_keep_correct_source_provenance_after_examples_and_invalid_blocks() {
    let mut source = String::from("---\r\ntitle: Long authoring report\r\n---\r\n\r\n");
    for index in 0..4096 {
        source.push_str(&format!(
            "Section {index}\n\n````markdown\n```metric\nvalue: example\n```\n````\n\n~~~metric\nvalue: {index}\n~~~\n\n"
        ));
        if index % 512 == 0 {
            source.push_str("~~~chart\ntype: unsupported\n~~~\n\n");
        }
    }
    let expected_block_lines: Vec<_> = source
        .lines()
        .enumerate()
        .filter_map(|(index, line)| (line == "~~~metric").then_some(index + 1))
        .collect();
    let expected_error_lines: Vec<_> = source
        .lines()
        .enumerate()
        .filter_map(|(index, line)| (line == "~~~chart").then_some(index + 1))
        .collect();
    let document = parse(&source).unwrap();
    assert_eq!(document.blocks.len(), 4096);
    assert_eq!(
        document
            .blocks
            .iter()
            .map(|block| block.line)
            .collect::<Vec<_>>(),
        expected_block_lines
    );
    for line in expected_error_lines {
        assert!(
            document
                .diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.level == "error" && diagnostic.line == Some(line) })
        );
    }
    assert_eq!(document.blocks.last().unwrap().value["value"], 4095);
    assert_eq!(document.body.matches("value: example").count(), 4096);
    assert_eq!(
        document
            .failed
            .iter()
            .filter(|f| f.source.contains("type: unsupported"))
            .count(),
        8
    );
    let report = render_document(document, &Default::default()).unwrap();
    assert_eq!(
        report.html.matches("class=\"rv-metric__value\"").count(),
        4096
    );
    // The examples stay as text; a Markdown fence is highlighted, so its spans are set aside.
    let text = without_spans(&report.html);
    assert_eq!(text.matches("value: example").count(), 4096);
    assert_eq!(report.html.matches("type: unsupported").count(), 8);
    assert!(report.html.contains("rv-metric__value\">4,095"));
    assert!(!report.html.contains("<!--richview:block:"));
}

#[test]
fn literal_and_nested_marker_comments_cannot_duplicate_owned_rich_blocks() {
    let source = r#"<!--richview:block:0-->
<!--richview:block:1-->
<!--richview:generated:0:block:1-->

```callout
kind: neutral
body: |
  Before <!--richview:block:1--> after.
```

```metric
value: 641
label: Owned evidence
```

`<!--richview:block:1-->`
"#;
    let document = parse(source).unwrap();
    assert_eq!(document.blocks.len(), 2);
    assert_eq!(document.blocks[0].line, 5);
    assert_eq!(document.blocks[1].line, 11);
    assert_eq!(
        serde_json::to_value(&document).unwrap()["block_placeholder_prefix"].as_str(),
        Some("<!--richview:generated:1:block:")
    );
    let report = render_document(document, &Default::default()).unwrap();
    assert_eq!(
        report
            .html
            .matches("class=\"rv-callout rv-callout--neutral\"")
            .count(),
        1
    );
    assert_eq!(report.html.matches("class=\"rv-metric__value\"").count(), 1);
    // Source comments are dropped, whatever they say; the code span keeps its literal text.
    assert!(report.html.contains("Before  after."));
    assert!(!report.html.contains("<!--richview:block:1-->"));
    assert!(report.html.contains("&lt;!--richview:block:1--&gt;"));
    assert!(!report.html.contains("<!--richview:generated:1:block:"));
    let ordinary = parse("```metric\nvalue: 1\n```").unwrap();
    assert!(ordinary.body.contains("<!--richview:block:0-->"));
    assert!(
        serde_json::to_value(ordinary)
            .unwrap()
            .get("block_placeholder_prefix")
            .is_none()
    );
}

#[test]
fn repeated_embedded_assets_and_complete_report_overhead_have_a_bounded_output() {
    let payload = format!("data:image/png;base64,{}", "A".repeat(1024 * 1024));
    let options = RenderOptions {
        source: Some("amplification.rmd".into()),
        data: [("shared.png".into(), payload)].into(),
        ..Default::default()
    };
    let image = "```image\nsrc: shared.png\nalt: A\n```\n\n";
    let ordinary = render(image, &options).unwrap();
    assert!(ordinary.html.len() < richview_core::document::MAX_REPORT_BYTES);
    let errors = render(&image.repeat(65), &options).unwrap_err();
    assert_eq!(errors[0].code, "render.output-limit");
    assert_eq!(errors[0].path, "output");
    assert_eq!(errors[0].tag.as_deref(), Some("image"));
    assert!(errors[0].line.unwrap() > 1);
    // The block total remains below the budget, but title media and embedded fonts count too.
    let source = format!("---\ntitle_image: shared.png\n---\n\n{}", image.repeat(63));
    let errors = render(&source, &options).unwrap_err();
    assert_eq!(errors[0].code, "render.output-limit");
    assert!(errors[0].message.contains("split the report"));
    // One Markdown body can amplify the same resource without using any rich fences.
    let images = "![Evidence](shared.png)\n\n".repeat(65);
    let errors = render(&images, &options).unwrap_err();
    assert_eq!(errors[0].code, "render.output-limit");
    let source = format!(
        "```callout\nkind: neutral\nbody: |\n{}\n```",
        images
            .lines()
            .map(|line| format!("  {line}\n"))
            .collect::<String>()
    );
    let errors = render(&source, &options).unwrap_err();
    assert_eq!(errors[0].tag.as_deref(), Some("callout"));
    assert_eq!(errors[0].line, Some(1));
}

#[test]
fn drafts_render_without_publish_metadata_and_invalid_blocks_remain_visible() {
    assert!(validate("# Draft").is_empty());
    let report =
        rendered("# Draft\n\n```chart\ntype: imaginary\ndata: {inline: []}\nencoding: {}\n```\n");
    assert!(report.html.contains("type: imaginary"));
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "block.schema" && d.line == Some(4))
    );
    let publication = validate_publish("---\ntitle: Existing\nversion: 0.0.1\n---\n");
    // Publishing needs a description as well as a title and a version (TYP-07).
    assert!(
        publication
            .iter()
            .any(|d| d.code == "publish.metadata" && d.path == "frontmatter.description")
    );
    assert!(
        !validate_publish("---\ntitle: Existing\ndescription: Why\nversion: 0.0.1\n---\n")
            .iter()
            .any(|d| d.level == "error")
    );
    assert!(
        validate_publish("# Draft")
            .iter()
            .any(|d| d.level == "error")
    );
    assert!(
        validate_publish("---\ntitle: Existing\nversion: latest\n---\n")
            .iter()
            .any(|d| d.code == "publish.metadata")
    );
}

#[test]
fn markdown_extensions_have_real_output_and_do_not_rewrite_code() {
    let source = r#"# Evidence

| Signal | Value |
|---|---|
| A | 2 |

~~superseded~~

- [x] Measured

Link [[Other Report|the other report]]. `[[code]]`.

The claim has a source.[^source]

[^source]: Original record.

Term
: Definition

Math $x^2 + \frac{1}{2}$.

Numeric math $2x+1$.

Revenue $13.07bn ([source](https://example.org/evidence)) vs Anthropic ~$4.6bn.

$$\sum_{i=1}^n i$$
"#;
    let html = rendered(source).html;
    assert!(html.contains("<table>"));
    assert!(html.contains("<del>superseded</del>"));
    assert!(html.contains("checkbox"));
    // Wikilinks are dropped to their label (FEA-RP-12); code keeps its brackets.
    assert!(html.contains("Link the other report."), "{html}");
    assert!(!html.contains("/r/other-report") && !html.contains("rv-wikilink"));
    assert!(html.contains("<code>[[code]]</code>"));
    assert!(
        html.contains("<li id=\"rv-fn-1\">") && !html.contains("class=\"footnote-definition\"")
    );
    assert!(html.contains("<dl>"));
    assert!(html.contains("<math"));
    assert!(html.contains("<mfrac>"));
    assert_eq!(html.matches("<math").count(), 3);
    assert!(html.contains("Revenue $13.07bn"));
    assert!(html.contains("vs Anthropic ~$4.6bn"));
    assert!(html.contains("href=\"https://example.org/evidence\""));
}

#[test]
fn resolved_csv_and_json_refs_feed_charts_and_tables_with_exact_values() {
    let source = r#"---
title: Sales
---

```table
data: {csv: ./sales.csv}
columns:
  - {key: label, label: Label}
  - {key: value, format: number, align: right}
sort: {key: value, dir: desc}
max_rows: 1
total: true
```

```chart
type: bar
title: Observed values
data: {csv: ./sales.csv}
encoding: {x: label, y: value}
```

```table
data: {json: other.json}
```
"#;
    let mut options = RenderOptions::default();
    options.data.insert(
        "./sales.csv".into(),
        "label,value\n001,1000\nB,2000\n".into(),
    );
    options.data.insert(
        "other.json".into(),
        r#"[{"source":"JSON","value":5}]"#.into(),
    );
    let report = render(source, &options).unwrap();
    assert!(
        !report.diagnostics.iter().any(|d| d.level == "error"),
        "{:?}",
        report.diagnostics
    );
    assert!(report.html.contains("2,000"));
    assert!(report.html.contains("3,000"));
    assert!(report.html.contains("001"));
    assert!(report.html.contains("Showing 1 of "));
    assert!(report.html.contains("JSON"));
    assert_eq!(report.charts.len(), 1, "the chart embeds its payload");
    let mut options = RenderOptions::default();
    options
        .data
        .insert("rows.json".into(), r#"[{"a":"resolved-locally"}]"#.into());
    let report = render("```table\ndata: {json: rows.json}\n```", &options).unwrap();
    assert!(report.html.contains("resolved-locally"));
}

#[test]
fn header_datasets_and_wrong_data_definitions_fail_only_their_elements() {
    // FEA-RP-30 (lead, 3 Nov): a header dataset and the block that names it are findings at their
    // lines, the block shows its notice and the rest renders. Both used to stop the render.
    let old = "---\ndata: {sales: {inline: [{x: Jan, y: 2}]}}\n---\n\n```chart\ntype: bar\ndata: {id: sales}\nencoding: {x: x, y: y}\n```\n\nStill here.\n";
    let report = render(old, &Default::default()).unwrap();
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "frontmatter.data"
                && d.message.contains("Move")
                && d.line == Some(2))
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "data.local" && d.path == "chart.data.id")
    );
    assert!(report.html.contains("Chart not rendered:") && report.html.contains("Still here."));
    let report = rendered(
        "```chart\ntype: bar\ndata: {inline: [{x: Jan, y: 2}]}\nencoding: {x: x, y: y}\n```\n",
    );
    assert!(report.html.contains("Jan"));
    let options = RenderOptions {
        report_folder: Some(richview_core::document::ReportFolder {
            root: "unresolved".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into(), "unresolved.csv".into()],
        }),
        ..Default::default()
    };
    // A dataset that cannot be read fails only its block (FEA-RP-30).
    let report = core_render(
        "```table\ndata: {csv: unresolved.csv}\n```\n\nStill here.",
        &options,
    )
    .unwrap();
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "data.unresolved")
    );
    assert!(report.html.contains("Table not rendered:") && report.html.contains("Still here."));
}

#[test]
fn all_chart_types_embed_a_payload_and_checks_keep_actual_geometry() {
    for kind in ["line", "bar", "scatter", "area", "pie", "heatmap"] {
        let data = if kind == "scatter" {
            json!([{"x":1,"y":2,"size":6},{"x":2,"y":4,"size":10}])
        } else {
            json!([{"x":"A","y":2,"size":6},{"x":"B","y":4,"size":10}])
        };
        let channels = match kind {
            "scatter" => "x: x, y: y, size: size, color: size",
            "pie" => "x: x, y: y",
            _ => "x: x, y: y, color: size",
        };
        let source = format!(
            "```chart\ntype: {kind}\ntitle: Exact observations\ndata: {{inline: {data}}}\nencoding: {{{channels}}}\n```\n"
        );
        let report = rendered(&source);
        assert_eq!(
            report.charts.len(),
            1,
            "{kind}: one payload for the runtime"
        );
        assert!(report.html.contains("data-rv-payload"), "{kind}");
        assert_eq!(report.metadata["_richview"]["chart_count"], 1);
        let checks = check(&source, &RenderOptions::default()).unwrap();
        assert_eq!(checks.charts[0]["engine"], "rust", "{kind}");
        assert_eq!(checks.charts[0]["chartType"], kind);
        assert!(
            parse_schema("chart-diagnostics", checks.charts[0].clone()).is_ok(),
            "{kind}: {:?}",
            parse_schema("chart-diagnostics", checks.charts[0].clone())
        );
        assert_eq!(checks.charts[0]["measured"], true);
        assert_eq!(checks.charts[0]["chartId"], "rv-chart-0");
    }
    // A name too long for a vertical bar turns the bars horizontal and stays whole; an author
    // who keeps them vertical is told which name had no room.
    let long = "An excessively long categorical label that exceeds the available plot";
    let source = format!(
        "```chart\ntype: bar\ntitle: Measured labels\ndata: {{inline: [{{x: {long}, y: 4}}, {{x: B, y: 2}}]}}\nencoding: {{x: x, y: y}}\n```"
    );
    let auto = common::geometry(&rendered(&source).charts[0], 240.0);
    let names = common::texts(&auto, Some(richview_chart::geometry::Role::AxisLabel)).join(" ");
    assert!(
        names.contains(long),
        "the name stays whole, wrapped above its bar: {names}"
    );
    assert!(!names.contains('…'));
    assert!(
        !auto
            .checks
            .iter()
            .any(|c| c.kind == "label-unfit" || c.kind == "label-dropped")
    );
    let vertical = source.replace("encoding:", "style: {orientation: vertical}\nencoding:");
    let kept = common::geometry(&rendered(&vertical).charts[0], 240.0);
    let unfit = kept
        .checks
        .iter()
        .find(|c| c.kind == "label-unfit")
        .expect("the author is told which name had no room");
    assert_eq!(unfit.level, richview_chart::Level::Warning);
    assert!(
        unfit.details["names"].to_string().contains(long),
        "{unfit:?}"
    );
}

#[test]
fn source_publication_request_preserves_source_and_slug_precedence_without_rendering() {
    let source = "\u{feff}---\r\ntitle: Revenue rose, again!\r\nslug: authored-slug\r\n---\r\n\r\n# Source stays exact\r\n";
    let authored = create_publish_request(source, None).unwrap();
    assert_eq!(authored.slug, "authored-slug");
    assert_eq!(authored.source, source);
    assert_eq!(
        serde_json::to_value(&authored).unwrap(),
        json!({"slug":"authored-slug","source":source})
    );
    assert_eq!(
        create_publish_request(source, Some("explicit-slug"))
            .unwrap()
            .slug,
        "explicit-slug"
    );
    assert_eq!(
        create_publish_request("---\ntitle: Revenue rose, again!\n---\n", None)
            .unwrap()
            .slug,
        "revenue-rose-again"
    );
    let long = format!("---\ntitle: {}\n---\n", "A".repeat(120));
    assert_eq!(
        create_publish_request(&long, None).unwrap().slug,
        "a".repeat(80)
    );
}

#[test]
fn wide_markdown_tables_retain_all_cells_and_ordinary_table_semantics_across_styles() {
    let columns = [
        "A deliberately long financial heading",
        "The second complete evidence heading",
        "A third column that must remain available",
        "Final complete source reference",
    ];
    let source = format!(
        "# A narrow report with complete evidence\n\n| {} |\n| --- | --- | --- | --- |\n| {} |\n| {} |\n",
        columns.join(" | "),
        ["Alpha", "12.3456789", "Not omitted", "source.example/alpha"].join(" | "),
        [
            "Beta",
            "987654321.123",
            "Still complete",
            "source.example/beta"
        ]
        .join(" | "),
    );
    for style in ["brutalist", "scientific", "sleek"] {
        let report = render(
            &source,
            &RenderOptions {
                styles: vec![style.into()],
                width: 390.0,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(report.html.matches("<table>").count(), 1);
        assert_eq!(
            report.html.matches("<th scope=\"col\">").count(),
            columns.len()
        );
        assert_eq!(report.html.matches("<td>").count(), columns.len() * 2);
        for cell in columns.into_iter().chain([
            "Alpha",
            "12.3456789",
            "Not omitted",
            "source.example/alpha",
            "Beta",
            "987654321.123",
            "Still complete",
            "source.example/beta",
        ]) {
            assert!(report.html.contains(cell));
        }
        assert_eq!(report.metadata["_richview"]["chart_count"], 0);
        assert!(
            !report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.level == "error")
        );
    }
}

#[test]
fn offline_reports_preserve_original_notices_for_every_bundled_canonical_font() {
    let originals = [
        (
            "Big Shoulders",
            include_str!("../../../assets/showcase/fonts/licenses/big-shoulders.txt"),
        ),
        (
            "Geist",
            include_str!("../../../assets/showcase/fonts/licenses/geist.txt"),
        ),
        (
            "Geist Mono",
            include_str!("../../../assets/showcase/fonts/licenses/geist-mono.txt"),
        ),
        (
            "IBM Plex Mono",
            include_str!("../../../assets/showcase/fonts/licenses/ibm-plex-mono.txt"),
        ),
        (
            "Newsreader",
            include_str!("../../../assets/showcase/fonts/licenses/newsreader.txt"),
        ),
        // The lettering face of hand-drawn is not a showcase face; the build packages this notice.
        (
            "Patrick Hand",
            include_str!("../assets/font-licenses/patrick-hand.txt"),
        ),
        (
            "Source Code Pro",
            include_str!("../../../assets/showcase/fonts/licenses/source-code-pro.txt"),
        ),
        (
            "Source Sans 3",
            include_str!("../../../assets/showcase/fonts/licenses/source-sans-3.txt"),
        ),
        (
            "Source Serif 4",
            include_str!("../../../assets/showcase/fonts/licenses/source-serif-4.txt"),
        ),
        (
            "Space Grotesk",
            include_str!("../../../assets/showcase/fonts/licenses/space-grotesk.txt"),
        ),
        (
            "Space Mono",
            include_str!("../../../assets/showcase/fonts/licenses/space-mono.txt"),
        ),
    ];
    for style in ["brutalist", "scientific", "sleek"] {
        for runtime in [false, true] {
            let report = render(
                "# An offline report",
                &RenderOptions {
                    styles: vec![style.into()],
                    runtime_js: runtime.then(|| "const richview_runtime = async()=>{};".into()),
                    runtime_wasm: runtime.then(|| "AA==".into()),
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(report.metadata["_richview"]["offline"], true);
            assert_eq!(report.metadata["_richview"]["interactive"], runtime);
            assert!(report.html.contains("data:font/woff2;base64,"));
            let (payload, _) = report
                .html
                .split_once("<script id=\"richview-font-notices\" type=\"application/json\">")
                .unwrap()
                .1
                .split_once("</script>")
                .unwrap();
            // The inert block stays HTML-safe and decoding restores the exact original bytes.
            assert!(!payload.contains(['<', '>', '&']));
            let notices: serde_json::Value = serde_json::from_str(payload).unwrap();
            let fonts = notices["fonts"].as_array().unwrap();
            // Only the faces the style embeds carry a notice (FEA-RP-38), each with its exact text.
            assert!(!fonts.is_empty() && fonts.len() <= originals.len());
            for notice in fonts {
                let (_, original) = originals
                    .iter()
                    .find(|(font, _)| notice["font"] == *font)
                    .unwrap();
                assert_eq!(
                    notice["license"].as_str().unwrap().as_bytes(),
                    original.as_bytes()
                );
            }
        }
    }
}

#[test]
fn a_charts_own_palette_wins_over_the_styles_for_generic_and_temporal_charts() {
    let sleek = richview_chart::rmd_render::palette("sleek");
    for (kind, observations) in [
        ("bar", json!([{"x":"Jan","y":2},{"x":"Feb","y":4}])),
        ("line", json!([{"x":"Jan","y":2},{"x":"Feb","y":4}])),
        (
            "line",
            json!([{"x":"2024-01-01","y":2},{"x":"2024-02-01","y":4}]),
        ),
    ] {
        let chart = json!({"type":kind,"title":"Source palette is authoritative","style":{"palette":"sleek"},"encoding":{"x":"x","y":"y"},"data":{"inline":observations}});
        let report = render(
            &format!("```chart\n{chart}\n```"),
            &RenderOptions {
                styles: vec!["scientific".into()],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(report.charts.len(), 1);
        // The payload carries the report's theme; the chart's own palette is applied by the
        // engine, so readers see sleek's first colour in a scientific report.
        let g = common::geometry(&report.charts[0], 640.0);
        let first = richview_chart::color::parse(sleek[0]).unwrap();
        let drawn = g.series[0].color;
        let expected = richview_chart::geometry::Rgba::from_oklch(first, 1.0);
        assert_eq!(drawn, expected, "{kind}");
        let embedded: serde_json::Value = serde_json::from_str(
            report
                .html
                .split_once("data-rv-payload>")
                .unwrap()
                .1
                .split_once("</script>")
                .unwrap()
                .0,
        )
        .unwrap();
        assert_eq!(embedded, report.charts[0]);
    }
}

#[test]
fn style_cascade_preserves_canonical_fonts_and_rejects_css_injection() {
    for (name, font) in [
        ("brutalist", "Big Shoulders"),
        ("scientific", "RichView Serif"),
        ("sleek", "Space Grotesk"),
    ] {
        let report = render(
            "# Claim",
            &RenderOptions {
                styles: vec![name.into()],
                ..Default::default()
            },
        )
        .unwrap();
        assert!(report.html.contains(font));
        assert!(report.html.contains("data:font/woff2;base64,"));
        assert!(
            !report
                .diagnostics
                .iter()
                .any(|d| d.level == "error" || d.code == "style.schema")
        );
    }
    let scientific = render(
        "# Claim",
        &RenderOptions {
            styles: vec!["scientific".into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        !scientific
            .html
            .contains(".rv-report__title{text-transform:uppercase}")
    );
    assert!(
        !scientific
            .html
            .contains(".rv-report__header{border-top:4px")
    );
    assert!(scientific.html.contains("font-family: \"RichView Serif\""));
    let report = render(
        "# Claim",
        &RenderOptions {
            styles: vec!["scientific".into(), "tokens: {accent: '#123456'}".into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert!(report.html.contains("--rv-accent: #123456"));
    assert!(report.html.contains("RichView Serif"));
    for unsafe_style in [
        "tokens: {bg: 'white;}</style><script>alert(1)</script>'}",
        "tokens: {bg: 'var(--evil)'}",
        "tokens: {font-text: 'url(evil.woff)'}",
        "components: {title: {weight: '400;} evil {'}}",
        "motion: {enabled: 'url(evil)'}",
        "tokens: {size-h2: 'calc(1px)'}",
        "tokens: {measure-phone: 20rem}",
        "tokens: {bakground: white}",
    ] {
        let report = render(
            "# Claim",
            &RenderOptions {
                styles: vec![unsafe_style.into()],
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "style.schema" || d.code == "style.unknown-key"),
            "{unsafe_style}"
        );
        assert!(!report.html.contains("<script>alert(1)</script>"));
    }
    let report = render(
        "# Claim",
        &RenderOptions {
            styles: vec!["editorial".into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "style.renamed" && d.level == "error")
    );
}

#[test]
fn untrusted_html_urls_and_script_payloads_cannot_escape_the_report() {
    let source = r#"---
title: '<script>alert(1)</script>'
---

<script>alert(2)</script><img src=x onerror=alert(3)>

[unsafe](javascript:alert%284%29)

<a href="data:text/html;base64,PHNjcmlwdD4=">unsafe raw link</a>

```callout
kind: hold
body: '<svg onload="alert(5)">Hello</svg>'
```

```image
src: javascript:alert(6)
alt: x
```

```chart
type: bar
title: '</script><script>alert(7)</script>'
data: {inline: [{x: A, y: 2}]}
encoding: {x: x, y: y}
```
"#;
    let hostile = rendered(source);
    assert!(!hostile.html.contains("src=\"javascript:"));
    assert!(
        hostile
            .diagnostics
            .iter()
            .any(|d| d.code == "security.media" && d.tag.as_deref() == Some("image"))
    );
    let report = rendered(&source.replace(
        "src: javascript:alert(6)",
        "src: data:image/png;base64,AAAA",
    ));
    assert!(!report.html.contains("<script>alert"));
    assert!(!report.html.contains("onerror="));
    assert!(!report.html.contains("onload="));
    assert!(!report.html.contains("href=\"javascript:"));
    // The page's own empty icon is the one data: href it carries.
    assert!(
        !report
            .html
            .replace("<link rel=\"icon\" href=\"data:,\">", "")
            .contains("href=\"data:")
    );
    assert!(!report.html.contains("src=\"javascript:"));
    assert!(report.html.contains("\\u003c/script\\u003e"));
}

#[test]
fn title_image_audio_video_and_sandboxed_embeds_are_supported() {
    let source = r#"---
title: Story
title_image: cover.png
---

**Opening evidence**

```audio
src: voice.mp3
title: Interview
```

```video
src: clip.mp4
title: Clip
poster: cover.png
caption: Observed interaction
```

```embed
url: https://www.youtube.com/embed/example
```

```embed
url: https://youtube.com.evil.example/embed
```
"#;
    let mut options = RenderOptions::default();
    options
        .data
        .insert("cover.png".into(), "data:image/png;base64,AAAA".into());
    options
        .data
        .insert("voice.mp3".into(), "data:audio/mpeg;base64,AAAA".into());
    options
        .data
        .insert("clip.mp4".into(), "data:video/mp4;base64,AAAA".into());
    let report = render(source, &options).unwrap();
    assert!(report.html.contains("data:image/png;base64,AAAA"));
    assert!(report.html.contains("<audio controls"));
    assert!(report.html.contains("<video controls"));
    assert!(
        report
            .html
            .contains("poster=\"data:image/png;base64,AAAA\"")
    );
    assert!(
        report
            .html
            .contains("sandbox=\"allow-scripts allow-same-origin allow-presentation\"")
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "security.embed")
    );
}

#[test]
fn rendering_is_deterministic_and_knowledge_matches_new_contract() {
    let source = "---\ntitle: Evidence\nversion: 0.0.1\n---\n\n```metric\nvalue: 4234567\nformat: number_compact\nunit: $\ndelta: 0.18\ntrend: up\n```";
    assert_eq!(rendered(source).html, rendered(source).html);
    assert!(rendered(source).html.contains("$4.23 million"));
    assert!(rendered(source).html.contains("+18.0%"));
    assert!(learn_section("style").unwrap().body.contains("scientific"));
    assert!(learn_section("unknown").is_none());
    assert!(learn_bundle().contains("richview_check"));
}

#[test]
fn table_header_controls_use_the_same_rust_sort_engine_and_keep_source_payload() {
    let mut table = json!({"data":{"inline":[{"key":"A","value":1},{"key":"B","value":2}]},"columns":[{"key":"key"},{"key":"value"}],"sort":{"key":"value","dir":"desc"}});
    let descending = render_table(&table);
    assert!(
        descending.find("<td class=\"\">B</td>").unwrap()
            < descending.find("<td class=\"\">A</td>").unwrap()
    );
    assert!(descending.contains("data-rv-sort=\"value\""));
    assert!(descending.contains("aria-sort=\"descending\""));
    assert!(descending.contains("data-rv-table"));
    table["sort"]["dir"] = json!("asc");
    let ascending = render_table(&table);
    assert!(
        ascending.find("<td class=\"\">A</td>").unwrap()
            < ascending.find("<td class=\"\">B</td>").unwrap()
    );
}

#[test]
fn source_media_offline_status_and_frontmatter_image_safety_are_explicit() {
    let report = rendered(
        "---\ntitle_image: https://example.org/image.png\n---\n\n![External](https://example.org/other.png)\n\nStill here.",
    );
    assert!(report.html.contains("Still here.") && report.html.contains("External"));
    assert!(!report.html.contains("example.org/"));
    assert_eq!(
        report
            .diagnostics
            .iter()
            .filter(|d| d.code == "report.asset.remote" && d.level == "error")
            .count(),
        2
    );
    let report = rendered(
        "---\ntitle_image: {src: 'data:image/png;base64,AAAA', fit: '\" onload=alert(1) x=\"'}\n---\n",
    );
    assert!(!report.html.contains("onload="));
    let report =
        rendered("```embed\nurl: https://custom.example/embed\nallow: [custom.example]\n```");
    assert!(
        report
            .html
            .contains("sandbox=\"allow-scripts allow-presentation\"")
    );
    assert!(!report.html.contains("allow-same-origin"));
    assert_eq!(report.metadata["_richview"]["offline"], false);
}

#[test]
fn layout_component_and_motion_tokens_have_observable_output() {
    let options=RenderOptions{styles:vec!["scientific".into(),"layout: {report-max-width: 1100px}\ncomponents: {header: {border-top: 2px}, title: {weight: 700}, callout: {border-width: 3px}, table-head: {transform: uppercase}}".into()],..Default::default()};
    let report = render("# Claim", &options).unwrap();
    assert!(
        report
            .html
            .contains("max-width: var(--rv-report-max-width, 980px)")
    );
    // The page's width is a mechanic: an author's file cannot set it (OPN-59).
    assert!(report.html.contains("--rv-report-max-width: 880px"));
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "style.internal" && d.path == "styles[1].layout.report-max-width")
    );
    assert!(report.html.contains("border-top:2px solid var(--rv-ink)"));
    assert!(report.html.contains("font-weight:700"));
    assert!(report.html.contains(".rv-callout{border-top-width:3px}"));
    assert!(
        report
            .html
            .contains(".rv-report__body th,.rv-chart-dialog th{text-transform:uppercase}")
    );
    // Report text, stats and callouts are at rest on first paint; reduced motion stays a floor.
    // The only animation is a waiting chart's loading state, inside its reserved box.
    let keyframes: Vec<&str> = report
        .html
        .match_indices("@keyframes")
        .map(|(at, _)| {
            report.html[at + 10..]
                .split('{')
                .next()
                .unwrap_or("")
                .trim()
        })
        .collect();
    assert!(
        keyframes
            .iter()
            .all(|name| name.starts_with("rv-chart-loading") || name.starts_with("rv-chart-late")),
        "{keyframes:?}"
    );
    assert!(!report.html.contains("rv-text-reveal"));
    assert!(!report.html.contains("rv-chart-reveal"));
    assert!(
        report
            .html
            .contains("@media(prefers-reduced-motion:reduce)")
    );
    // Retired leaves are reported and ignored; the rest of the style still applies.
    let chart = "```chart\ntype: bar\ntitle: Motion settles\ndata: {inline: [{x: a, y: 1}]}\nencoding: {x: x, y: y}\n```";
    let report = render(
        chart,
        &RenderOptions {
            styles: vec!["motion: {enabled: false, delay: 100ms, stagger: 25}\ncomponents: {callout: {left-bar-width: 6px, border-width: 2px}}".into()],
            ..Default::default()
        },
    )
    .unwrap();
    let mut retired: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.code == "style.retired")
        .map(|d| (d.level.as_str(), d.path.as_str()))
        .collect();
    retired.sort_unstable();
    assert_eq!(
        retired,
        [
            ("warning", "styles[0].components.callout.left-bar-width"),
            ("warning", "styles[0].motion.delay"),
        ]
    );
    assert!(!report.diagnostics.iter().any(|d| d.code == "style.schema"));
    assert!(report.html.contains(".rv-callout{border-top-width:2px}"));
    assert!(!report.html.contains("url(evil)"));
    assert_eq!(
        report.charts[0]["theme"]["motion"],
        json!(false),
        "motion: {{enabled: false}} reaches the chart"
    );
}

#[test]
fn stats_and_callouts_are_typeset_without_panels_or_side_bars() {
    let source = "```metric\nvalue: 68.2\nunit: '%'\nlabel: Share\n```\n\n```metric\nvalue: 4.6\nunit: US$\n```\n\n```callout\nkind: hold\ntitle: Read this\nbody: |\n  First paragraph.\n\n  Second.\n```\n\n```callout\nkind: up\ntitle: List first\nbody: |\n  - one\n```";
    for (style, variant) in [
        ("sleek", None),
        ("air", None),
        (
            "scientific",
            Some(".rv-metric{border-bottom:1px solid var(--rv-rule)"),
        ),
        (
            "brutalist",
            Some(".rv-metric{background:var(--rv-accent);border:2px solid var(--rv-ink)"),
        ),
        (
            "hand-drawn",
            Some(
                ".rv-metric{border-top:0;padding-top:0}.rv-metric::after{content:\"\";flex:0 0 100%;order:1}",
            ),
        ),
    ] {
        let html = render(
            source,
            &RenderOptions {
                styles: vec![style.into()],
                ..Default::default()
            },
        )
        .unwrap()
        .html;
        assert!(html.contains("rv-metric__value\">68.2%</div>"), "{style}");
        assert!(html.contains("rv-metric__value\">US$4.6</div>"), "{style}");
        // The run-in title carries the author's kind; a list body keeps the title as a line.
        assert!(html.contains("<aside class=\"rv-callout rv-callout--hold\"><div class=\"rv-callout__body\"><p><b class=\"rv-callout__title\">Read this</b> First paragraph.</p>"));
        assert!(html.contains("<aside class=\"rv-callout rv-callout--up\"><div class=\"rv-callout__body\"><p class=\"rv-callout__title\">List first</p><ul>"));
        assert!(html.contains(".rv-callout--hold .rv-callout__title { color: var(--rv-hold); }"));
        // Brutalist's knockout is the one filled block besides code panels (STY-33).
        let panels: &[&str] = if style == "brutalist" {
            &[".rv-callout{border:", ".rv-callout{border-left"]
        } else {
            &[
                ".rv-metric{background",
                ".rv-callout{border:",
                ".rv-callout{border-left",
            ]
        };
        for panel in panels {
            assert!(!html.contains(panel), "{style}: {panel}");
        }
        // No other accent-filled panel; a chart control's thumb is the only accent fill (FEA-BR-34).
        let filled: Vec<&str> = html
            .split('}')
            .filter(|rule| rule.contains("background:var(--rv-accent)"))
            .filter(|rule| !rule.trim_start().starts_with(".rv-chart__"))
            .filter(|rule| style != "brutalist" || !rule.trim_start().starts_with(".rv-metric{"))
            .collect();
        assert!(filled.is_empty(), "{style}: {filled:?}");
        if let Some(variant) = variant {
            assert!(html.contains(variant), "{style}");
        }
    }
}

#[test]
fn hand_drawn_letters_its_heads_and_draws_its_marks_as_vector_strokes() {
    let source = "---\ntitle: Claim\n---\n\n## Section\n\nText.\n\n```metric\nvalue: 68.2\nunit: '%'\n```\n\n```callout\nkind: hold\ntitle: Read this\nbody: Body.\n```\n\n| A | B |\n| --- | --: |\n| x | 1 |\n";
    let html = |style: &str| {
        render(
            source,
            &RenderOptions {
                styles: vec![style.into()],
                ..Default::default()
            },
        )
        .unwrap()
        .html
    };
    let drawn = html("hand-drawn");
    assert_eq!(drawn, html("hand-drawn"), "strokes are seeded, not random");
    assert!(drawn.contains("font-family: \"Patrick Hand\""));
    // The lettering face has one weight: headings, figure titles and the stat use it as is.
    assert!(drawn.contains(".rv-report{font-synthesis-weight:none}"));
    assert!(
        drawn.contains(
            ".rv-report__body th,.rv-chart-dialog th{font-family:var(--rv-font-display)}"
        )
    );
    assert!(
        drawn.contains(".rv-report__body .rv-source__label{font-family:var(--rv-font-display)}")
    );
    // Header, footer, three section, table-head, three bracket slices, the underline and the strike.
    assert_eq!(drawn.matches("data:image/svg+xml,").count(), 11);
    for raster_or_filter in [
        "filter",
        "feTurbulence",
        "data:image/png",
        "data:image/jpeg",
    ] {
        assert!(!drawn.contains(raster_or_filter), "{raster_or_filter}");
    }
    // A stroke is drawn in its style's token, so a later style that changes the ink recolours it.
    let recoloured = render(
        source,
        &RenderOptions {
            styles: vec![
                "hand-drawn".into(),
                "tokens:\n  ink: \"oklch(0.3 0.1 250)\"\n".into(),
            ],
            ..Default::default()
        },
    )
    .unwrap()
    .html;
    assert!(drawn.contains("fill='oklch(0.22 0.004 85)'"));
    assert!(recoloured.contains("fill='oklch(0.3 0.1 250)'"));
    assert!(!recoloured.contains("fill='oklch(0.22 0.004 85)'"));
    for style in ["sleek", "air", "scientific", "brutalist"] {
        let other = html(style);
        assert!(!other.contains("data:image/svg+xml,"), "{style}");
        assert!(!other.contains("font-synthesis-weight"), "{style}");
    }
}

#[test]
fn prose_gets_typographic_punctuation_while_code_urls_math_and_currency_keep_their_characters() {
    let source = r#"OpenAI's lead and "Public perception" here. It's 'quoted' and the '90s...

Range 2020--2021, an em---dash, an ASCII arrow --> and the flag --style. **OpenAI**'s *"quote"* ("paren") and students' work.

Code `don't -- "x"...`, math $x--y$ and Revenue $13.07bn vs Anthropic ~$4.6bn.

Bare https://example.org/a--b...c and <https://example.org/x--y> and [it's "here"](https://example.org/q--r).

```text
"raw" -- code...
```
"#;
    let html = rendered(source).html;
    for expected in [
        "<p>OpenAI’s lead and “Public perception” here. It’s ‘quoted’ and the ’90s…</p>",
        "Range 2020–2021, an em—dash, an ASCII arrow --&gt; and the flag --style. <strong>OpenAI</strong>’s <em>“quote”</em> (“paren”) and students’ work.",
        "<code>don't -- \"x\"...</code>",
        "and Revenue $13.07bn vs Anthropic ~$4.6bn.",
        "Bare <a href=\"https://example.org/a--b...c\" rel=\"noopener noreferrer\">https://example.org/a--b...c</a> and <a href=\"https://example.org/x--y\" rel=\"noopener noreferrer\">https://example.org/x--y</a>",
        "<a href=\"https://example.org/q--r\" rel=\"noopener noreferrer\">it’s “here”</a>",
        "\"raw\" -- code...",
    ] {
        assert!(html.contains(expected), "{expected}");
    }
    assert_eq!(html.matches("<math").count(), 1);
}

#[test]
fn markdown_table_alignment_survives_sanitising_as_classes() {
    let html = rendered("| A | B | C |\n|:--|--:|:-:|\n| x | 1 | y |\n").html;
    assert!(html.contains("<th scope=\"col\">A</th><th scope=\"col\" class=\"num\">B</th><th scope=\"col\" class=\"center\">C</th>"));
    assert!(html.contains("<td>x</td><td class=\"num\">1</td><td class=\"center\">y</td>"));
    assert!(!html.contains("style=\"text-align"));
}

#[test]
fn only_wholly_emphasised_notes_after_a_rich_block_become_source_notes() {
    let source = "```metric\nvalue: 1\n```\n\n*Sources: [C1](https://a.example/x), `data/x.csv`.*\n\n```metric\nvalue: 2\n```\n*Note: tight.*\n\n```metric\nvalue: 3\n```\n\n*Sources: partial* and more.\n\n```metric\nvalue: 4\n```\n\n**Sources: strong**\n\nProse.\n\n*Sources: after prose*\n\n```metric\nvalue: 5\n```\n\n*Data from the survey.*\n";
    let html = rendered(source).html;
    assert_eq!(html.matches("<p class=\"rv-source\">").count(), 2);
    for expected in [
        "<p class=\"rv-source\"><span class=\"rv-source__label\">Sources:</span> <a href=\"https://a.example/x\" rel=\"noopener noreferrer\">C1</a>, <code>data/x.csv</code>.</p>",
        "<p class=\"rv-source\"><span class=\"rv-source__label\">Note:</span> tight.</p>",
        "<p><em>Sources: partial</em> and more.</p>",
        "<p><strong>Sources: strong</strong></p>",
        "<p><em>Sources: after prose</em></p>",
        "<p><em>Data from the survey.</em></p>",
    ] {
        assert!(html.contains(expected), "{expected}");
    }
}

#[test]
fn styles_define_one_type_scale_with_phone_values_and_true_italics_only() {
    for (style, text_family, italic_deck) in [
        ("sleek", "Geist", false),
        ("air", "Geist", false),
        ("scientific", "RichView Serif", true),
        ("brutalist", "Newsreader", true),
        ("hand-drawn", "Newsreader", false),
    ] {
        let html = render(
            "---\ntitle: Claim\ndescription: Interpretation\n---\n\n## Section",
            &RenderOptions {
                styles: vec![style.into()],
                ..Default::default()
            },
        )
        .unwrap()
        .html;
        for token in [
            "--rv-size-small:",
            "--rv-size-base:",
            "--rv-size-h3:",
            "--rv-size-h2:",
            "--rv-size-h1:",
            "--rv-size-display:",
        ] {
            assert!(html.contains(token), "{style} {token}");
        }
        let phone = html
            .split_once("@media (max-width: 640px) {\n:root {\n")
            .and_then(|(_, rest)| rest.split_once('}'))
            .map(|(block, _)| block)
            .unwrap_or_default();
        assert!(phone.contains("--rv-size-display:"), "{style}");
        // Quotes and decks are italic only when the text family embeds a true italic; the
        // deck also needs the style's choice, so sleek's stays upright.
        let italic_face = html.lines().any(|line| {
            line.starts_with("@font-face")
                && line.contains(&format!("\"{text_family}\""))
                && line.contains("italic")
        });
        assert_eq!(
            html.contains(".rv-report__body blockquote{font-style:italic}"),
            italic_face,
            "{style}"
        );
        assert_eq!(
            html.contains(".rv-report__lede,.rv-report__subtitle{font-style:italic}"),
            italic_face && italic_deck,
            "{style}"
        );
    }
    let css =
        custom_properties_to_css(&json!({"--rv-size-h2": "27px", "--rv-size-h2-phone": "26px"}))
            .unwrap();
    assert_eq!(
        css,
        ":root {\n  --rv-size-h2: 27px;\n}\n@media (max-width: 640px) {\n:root {\n  --rv-size-h2: 26px;\n}\n}\n"
    );
}

#[test]
fn embedded_faces_block_so_text_is_never_drawn_in_a_fallback_face_first() {
    for style in ["sleek", "air", "scientific", "brutalist", "hand-drawn"] {
        let html = render(
            "# Claim",
            &RenderOptions {
                styles: vec![style.into()],
                ..Default::default()
            },
        )
        .unwrap()
        .html;
        let faces: Vec<&str> = html
            .lines()
            .filter(|line| line.starts_with("@font-face"))
            .collect();
        assert!(!faces.is_empty(), "{style}");
        assert!(
            faces
                .iter()
                .all(|face| face.contains("font-display: block;")),
            "{style}"
        );
    }
}

#[test]
fn flagship_report_preserves_every_chart_and_sourced_narrative_across_styles() {
    let source = include_str!("../../../examples/openai-vs-anthropic/report.rmd");
    let parsed = parse(source).unwrap();
    // The count follows the flagship's source, one chart per chart fence, not a constant; the research
    // pipeline's committed chart inventory anchors it, so a chart dropped from the source fails here.
    let charts = parsed.blocks.iter().filter(|b| b.tag == "chart").count();
    assert_eq!(
        charts,
        source.matches("```chart\n").count(),
        "every chart fence parses as a chart"
    );
    let inventory: serde_json::Value = serde_json::from_str(include_str!(
        "../../../examples/openai-vs-anthropic/data/charts/index.json"
    ))
    .unwrap();
    assert_eq!(
        charts,
        inventory.as_array().unwrap().len(),
        "the source holds every chart of data/charts/index.json"
    );
    for style in ["brutalist", "scientific", "sleek"] {
        let report = core_render(
            source,
            &RenderOptions {
                styles: vec![style.into()],
                data: std::collections::BTreeMap::from([(
                    "report.rstyle.yaml".into(),
                    include_str!("../../../examples/openai-vs-anthropic/report.rstyle.yaml").into(),
                )]),
                report_folder: Some(richview_core::document::ReportFolder {
                    root: "openai-vs-anthropic".into(),
                    source: "report.rmd".into(),
                    files: std::iter::once("report.rmd".into())
                        .chain(
                            richview_core::document::report_resources(&parsed)
                                .into_iter()
                                .map(|r| r.path),
                        )
                        .collect::<std::collections::BTreeSet<_>>()
                        .into_iter()
                        .collect(),
                }),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(report.metadata["_richview"]["chart_count"], charts);
        assert_eq!(report.metadata["title"], parsed.frontmatter["title"]);
        assert_eq!(
            report.html.matches("<figure class=\"rv-chart\" ").count(),
            charts
        );
        assert_eq!(
            report.charts.len(),
            charts,
            "every chart embeds its payload"
        );
        assert!(
            report
                .html
                .contains("https://ramp.com/data/ai-index-sept-2026")
        );
        assert!(report.html.contains("$17.0bn"));
        // Each chart carries its source note, and the scorecard's metrics share one.
        assert_eq!(
            report.html.matches("<p class=\"rv-source\">").count(),
            source
                .lines()
                .filter(|line| line.starts_with("*Sources:"))
                .count()
        );
        assert!(report.html.matches("<p class=\"rv-source\">").count() > charts);
        assert!(report.html.contains("OpenAI’s"));
        assert!(report.html.contains("“Recent”"));
        assert!(
            !report.html.contains("<math"),
            "Financial dollar amounts must not become equations"
        );
        assert!(
            !report.diagnostics.iter().any(|d| d.level == "error"),
            "{style}: {:?}",
            report
                .diagnostics
                .iter()
                .filter(|d| d.level == "error")
                .collect::<Vec<_>>()
        );
        assert!(
            report.metadata["_richview"]["chart_diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .all(|chart| chart["measured"] == true)
        );
    }
}

#[test]
fn cached_preview_runtime_is_a_trusted_host_option_and_invalid_urls_do_not_execute() {
    let report = render(
        "---\nruntime_url: javascript:alert(1)\n---\n\n# Draft",
        &RenderOptions::default(),
    )
    .unwrap();
    assert!(!report.html.contains("src=\"javascript:"));
    assert_eq!(report.metadata["_richview"]["interactive"], false);
    let report = render(
        "# Draft",
        &RenderOptions {
            runtime_url: Some("https://example.org/richview_site".into()),
            runtime_js: Some("fixture".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        report
            .html
            .contains("src=\"https://example.org/richview_site.js\"")
    );
    assert!(
        report
            .html
            .contains("https://example.org/richview_site_bg.wasm")
    );
    assert_eq!(report.metadata["_richview"]["interactive"], true);
    assert_eq!(report.metadata["_richview"]["offline"], false);
    let report = render(
        "# Draft",
        &RenderOptions {
            runtime_url: Some("/_runtime/abc/richview_site".into()),
            runtime_js: Some("fixture".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        report
            .html
            .contains("src=\"/_runtime/abc/richview_site.js\"")
    );
    assert_eq!(report.metadata["_richview"]["interactive"], true);
    for unsafe_path in [
        "//evil.example/richview_site",
        "/x\"onload=\"alert(1)",
        "/x?y",
    ] {
        let report = render(
            "# Draft",
            &RenderOptions {
                runtime_url: Some(unsafe_path.into()),
                runtime_js: Some("fixture".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            !report.html.contains("<script>wasm_bindgen"),
            "{unsafe_path}"
        );
    }
    let report = render(
        "# Draft",
        &RenderOptions {
            runtime_url: Some("javascript:alert(1)".into()),
            runtime_js: Some("fixture".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!report.html.contains("<script>"));
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "security.runtime-url")
    );
}

#[test]
fn schema_diagnostics_reject_invalid_optional_fields_instead_of_silently_ignoring_them() {
    for source in [
        "```metric\nvalue: 100\nformat: {not: a string}\n```",
        "```table\ndata: {inline: [{value: 1}]}\ncolumns: [{key: value,align: diagonal}]\n```",
        "```chart\ntype: line\ntitle: Valid title\ndata: {inline: [{x: 1,y: 2}]}\nencoding: {x: 23,y: y}\n```",
        "```chart\ntype: bar\ntitle: Valid title\ndata: {inline: [{x: A,y: 2}]}\nencoding: {x: x,y: y}\nannotations: [{kind: point,at: {x: A},text: 23}]\n```",
    ] {
        let parsed = parse(source).unwrap();
        assert!(parsed.diagnostics.iter().any(|d| d.code == "block.schema"));
        assert!(parsed.blocks.is_empty());
        // A block that failed shows its notice and its source in place (FEA-RP-29).
        assert!(rendered(source).html.contains("class=\"rv-notice\""));
    }
}

#[test]
fn compatibility_export_preserves_the_public_shape_flags_and_resolved_ast() {
    let source = "---\ntitle: Compatibility\n---\n\n```chart\ntype: bar\ntitle: Exact observations\ndata: {inline: [{x: A,y: 1}]}\nencoding: {x: x,y: y}\n```";
    let output=render_compat(source,&json!({"chartWidth":640,"theme":"dark","reportCard":true,"styles":[{"kind":"builtin","name":"scientific"},{"kind":"source","source":"tokens: {accent: '#123456'}"}]})).unwrap();
    assert_eq!(output["hasCharts"], true);
    assert_eq!(output["parsed"]["frontmatter"]["title"], "Compatibility");
    assert_eq!(
        output["parsed"]["blocks"][0]["value"]["data"]["inline"][0]["y"],
        1
    );
    // The theme option is gone (FEA-RP-33): no scheme attribute is written.
    assert!(
        !output["html"]
            .as_str()
            .unwrap()
            .contains("<html lang=\"en\" data-theme")
    );
    assert!(
        output["html"]
            .as_str()
            .unwrap()
            .contains("--rv-accent: #123456")
    );
    // The in-report card is gone (FEA-RP-31).
    assert!(
        !output["html"]
            .as_str()
            .unwrap()
            .contains("Render report card")
    );
    // Checks measure the desktop column readers see; the compatibility `chartWidth` no longer
    // sizes charts, which lay out at the width their figure occupies (FEA-CH-64).
    assert!(output["diagnostics"]["charts"][0]["dimensions"]["width"].is_number());
    assert!(
        output["diagnostics"]["parse"]
            .as_array()
            .unwrap()
            .iter()
            .all(|d| !d["code"].as_str().unwrap().starts_with("chart."))
    );
    assert!(render_compat(source, &json!({"styles":[{"kind":"unknown"}]})).is_err());
}

#[test]
fn native_callers_can_render_the_parsed_ast_after_resolving_external_data() {
    let source = "---\ntitle: Draft\n---\n\n```table\ndata: {csv: sales.csv}\ncolumns: [{key: amount,label: Amount}]\n```";
    let mut document = parse(source).unwrap();
    // A native resolver can use the parsed references and normalize data without reconstructing
    // source or sending a second copy through YAML parsing.
    document.frontmatter["title"] = json!("Resolved report");
    document.blocks[0].value["data"] = json!({"inline":[{"amount":123456789}]});
    let report = render_document(document, &RenderOptions::default()).unwrap();
    assert!(report.html.contains("<title>Resolved report</title>"));
    // The resolved value reaches the table at the three-digit rule (TYP-23), and no finer digit
    // does (FEA-CH-37).
    assert!(report.html.contains(">123 million</td>"));
    assert!(!report.html.contains("123456789"));
    assert_eq!(report.metadata["_richview"]["block_count"], 1);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.code == "data.unresolved")
    );
}

#[test]
fn a_blank_title_is_as_missing_as_none_and_a_blank_alt_is_decorative() {
    for fence in [
        "```chart\ntype: bar\ntitle: \"  \"\ndata: {inline: []}\nencoding: {x: a, y: b}\n```",
        "```video\nsrc: clip.mp4\ntitle: \"\"\n```",
        "```audio\nsrc: clip.mp3\ntitle: \" \"\n```",
    ] {
        let parsed = parse(fence).unwrap();
        assert!(parsed.blocks.is_empty(), "{fence}: stays source");
        let found = parsed
            .diagnostics
            .iter()
            .find(|d| d.code == "block.schema" && d.path.ends_with(".title"))
            .unwrap_or_else(|| panic!("{fence}: {:?}", parsed.diagnostics));
        assert!(
            found.message.contains("must not be empty"),
            "{}",
            found.message
        );
        assert_eq!(found.level, "error");
    }
    let decorative = parse("```image\nsrc: photo.png\nalt: \"\"\n```").unwrap();
    assert_eq!(decorative.blocks.len(), 1, "{:?}", decorative.diagnostics);
}

#[test]
fn generated_schema_adapters_keep_required_fields_defaults_and_frontmatter_passthrough() {
    let parsed=parse("```table\ndata: {inline: [{value: 1}]}\nsort: {key: value}\n```\n\n```image\nsrc: photo.png\nalt: A\n```\n\n```chart\ntype: bar\ntitle: T\ndata: {inline: []}\nencoding: {x: a, y: b}\n```").unwrap();
    assert_eq!(parsed.blocks[0].value["max_rows"], 20);
    assert_eq!(parsed.blocks[0].value["total"], false);
    assert_eq!(parsed.blocks[0].value["sort"]["dir"], "asc");
    assert_eq!(parsed.blocks[1].value["fit"], "contain");
    assert_eq!(parsed.blocks[2].tag, "chart");
    // A chart without a title is an error and stays visible as source (TYP-13).
    let untitled =
        parse("```chart\ntype: bar\ndata: {inline: []}\nencoding: {x: a, y: b}\n```").unwrap();
    assert!(untitled.blocks.is_empty());
    assert!(
        untitled
            .diagnostics
            .iter()
            .any(|d| d.code == "block.schema" && d.path == "chart.title")
    );
    let table = parse_schema(
        "TableSchema",
        json!({"data":{"inline":[{"value":1}]},"sort":{"key":"value"},"extra":"stripped"}),
    )
    .unwrap();
    assert_eq!(table["max_rows"], 20);
    assert_eq!(table["total"], false);
    assert_eq!(table["sort"]["dir"], "asc");
    assert!(table.get("extra").is_none());
    assert!(
        parse_schema(
            "ChartSchema",
            json!({"type":"bar","data":{"inline":[]},"encoding":{"x":"x","y":"y"}})
        )
        .is_err()
    );
    let chart=parse_schema("ChartSchema",json!({"type":"bar","title":"The comparison","data":{"inline":[]},"encoding":{"x":"x","y":"y"},"style":{}})).unwrap();
    assert!(
        chart["style"].get("palette").is_none(),
        "An omitted palette inherits the report theme"
    );
    assert_eq!(
        parse_schema("ImageSchema", json!({"src":"photo.png","alt":"A"})).unwrap()["fit"],
        "contain"
    );
    assert_eq!(
        parse_schema(
            "EmbedSchema",
            json!({"url":"https://youtube.com/watch?v=abc"})
        )
        .unwrap()["height"],
        420
    );
    let fm = parse_schema(
        "DocumentFrontmatterSchema",
        json!({"custom":{"unchanged":true}}),
    )
    .unwrap();
    assert_eq!(fm["custom"]["unchanged"], true);
    assert!(parse_schema("DocumentFrontmatterSchema", json!({"title":42})).is_err());
    assert!(parse_schema("DataSourceSchema", json!({"headers":{"Authorization":42}})).is_err());
    assert_eq!(
        parse_schema("versionString", json!("1.2.3-beta")).unwrap(),
        "1.2.3-beta"
    );
    for invalid in [" 1.2.3-beta ", "01.2.3-beta.01+old.build", "1.2.3+build"] {
        assert!(parse_schema("versionString", json!(invalid)).is_err());
    }
    assert!(
        parse("---\nversion: ' 0.0.1 '\n---\n")
            .unwrap()
            .frontmatter
            .get("version")
            .is_none()
    );
    assert!(
        validate_publish("---\ntitle: Existing\ndescription: Why\nversion: 01.2.3\n---\n")
            .iter()
            .any(|d| d.level == "error" && d.code == "publish.metadata")
    );
    assert!(parse_schema("chart-check",json!({"kind":"clipping","severity":"warning","message":"Label crosses bounds","metric":"invalid"})).is_err());
    assert!(
        parse_schema(
            "dropped-label",
            json!({"text":"A","reason":"collision","wouldBeAt":{"x":2,"y":3}})
        )
        .is_ok()
    );
}

#[test]
fn public_style_helpers_preserve_cascade_and_share_the_css_security_boundary() {
    let mut diagnostics = vec![];
    let resolved=resolve_styles_compat(&json!([{"kind":"builtin","name":"scientific"},{"kind":"source","source":"tokens: {accent: '#abcdef'}\ncomponents: {title: {letter-spacing: '-.02em',weight: 950}}"}]),&mut diagnostics).unwrap();
    // The leaves still apply; each says what replaces it (STY-73).
    assert!(
        diagnostics.iter().all(|d| d.code == "style.legacy"),
        "{diagnostics:?}"
    );
    assert_eq!(resolved["doc"]["tokens"]["accent"], "#abcdef");
    assert_eq!(resolved["chart"]["palette"], "scientific");
    assert_eq!(resolved["components"]["title"]["letter-spacing"], "-.02em");
    let css = custom_properties_to_css(&resolved["customProperties"]).unwrap();
    assert!(css.contains("--rv-accent: #abcdef;"));
    assert!(
        builtin_style_source("scientific")
            .unwrap()
            .contains("RichView Serif")
    );
    assert!(
        builtin_style_source("brutalist")
            .unwrap()
            .contains("Big Shoulders")
    );
    assert!(
        custom_properties_to_css(&json!({"--rv-bg":"white;}</style><script>bad</script>"}))
            .is_err()
    );
    assert!(custom_properties_to_css(&json!({"--other":"red"})).is_err());
    assert_eq!(custom_properties_to_css(&json!({})).unwrap(), "");
}

fn pdf(source: &str, options: RenderOptions) -> richview_core::document::RenderedReport {
    render(
        source,
        &RenderOptions {
            output: richview_core::document::Output::Pdf,
            ..options
        },
    )
    .unwrap()
}

#[test]
fn pdf_output_prints_a_light_paged_report_with_settled_charts() {
    let source = "---\ntitle: Anthropic is winning the people who build with AI. OpenAI still has everyone else\ntheme: dark\n---\n\n## Revenue\n\n```chart\ntype: bar\ntitle: B exceeds A\ndata: {inline: [{category: A, value: 12}, {category: B, value: 20}]}\nencoding: {x: category, y: value}\n```\n";
    let report = pdf(
        source,
        RenderOptions {
            styles: vec!["sleek".into()],
            ..Default::default()
        },
    );
    let html = &report.html;
    // Paper is light whatever the style and colour scheme ask for (D7).
    assert!(html.starts_with("<!doctype html>\n<html lang=\"en\" data-rv-output=\"pdf\">"));
    assert!(!html.lines().nth(1).unwrap().contains("data-theme="));
    let light = render(
        source,
        &RenderOptions {
            styles: vec!["air".into()],
            output: richview_core::document::Output::Pdf,
            ..Default::default()
        },
    )
    .unwrap();
    // Sleek prints on Air's paper (colours, sizes) and draws with its own faces (STY-21).
    let paper = |name: &str| {
        let style = resolve_styles_compat(&json!([name]), &mut Vec::new()).unwrap();
        ["--rv-bg", "--rv-ink", "--rv-accent"].map(|key| {
            style["customProperties"][key]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        })
    };
    for value in paper("air") {
        assert!(
            html.contains(&value),
            "sleek's PDF carries Air's paper value {value}"
        );
    }
    assert!(light.html.contains("font-family: \"Space Grotesk\""));
    assert!(html.contains("font-family: \"Geist\""));
    assert!(!html.contains("font-family: \"RichView Sans\""));
    // The page box: A4, margins, the theme background to the paper's edge, and from the second
    // page on a running title cut at a word and page numbers.
    let light_style = resolve_styles_compat(&json!(["air"]), &mut Vec::new()).unwrap();
    let background = light_style["customProperties"]["--rv-bg"].as_str().unwrap();
    assert!(html.contains(&format!(
        "@page{{size:A4 portrait;margin:20mm 18mm 22mm;background:{background};"
    )));
    assert!(html.contains(
        "@bottom-left{content:\"Anthropic is winning the people who build with AI. OpenAI still has…\";"
    ));
    assert!(html.contains("@bottom-right{content:counter(page) \" / \" counter(pages);"));
    assert!(html.contains("@page:first{@bottom-left{content:none}@bottom-right{content:none}}"));
    // Charts are vector drawings at the printed text width, at rest, with no runtime (INT-31).
    assert!(report.charts.is_empty(), "paper needs no runtime payload");
    let width = richview_core::document::Page::A4.text_area().0;
    assert!((width - 657.64).abs() < 0.01, "{width}");
    let figure = html
        .split("class=\"rv-chart rv-chart--print\"")
        .nth(1)
        .expect("a printed figure");
    let drawn = (width * 100.0).round() / 100.0;
    assert!(
        figure.contains(&format!("viewBox=\"0 0 {drawn} ")),
        "laid out at the text width"
    );
    assert!(!figure.contains("needs JavaScript"));
    let letter = pdf(
        source,
        RenderOptions {
            page: richview_core::document::Page::Letter,
            ..Default::default()
        },
    );
    assert!(letter.html.contains("@page{size:letter portrait;"));
    // The interactive report keeps its payloads and colour scheme, and has no page box.
    let screen = render(
        source,
        &RenderOptions {
            styles: vec!["sleek".into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert!(screen.html.contains("<html lang=\"en\"><head>"));
    assert!(
        screen
            .html
            .contains("<meta name=\"color-scheme\" content=\"dark\">")
    );
    // The screen payload carries the light counterpart for browser printing (FEA-CH-67).
    assert_eq!(
        screen.charts[0]["print"]["surface"],
        light_style["customProperties"]["--rv-bg-chart"]
    );
    assert!(!screen.html.contains("@page{size:"));
}

#[test]
fn every_report_prints_light_at_paper_size_without_chart_controls() {
    let report = render(
        "---\ntitle: Paper\ntheme: dark\n---\n\nText.\n",
        &RenderOptions {
            styles: vec!["sleek".into()],
            ..Default::default()
        },
    )
    .unwrap();
    let print = &report.html[report.html.find("@media print{@page").unwrap()..];
    let print = &print[..print.find("}}").unwrap()];
    // Sleek's light counterpart, declared for an explicit colour scheme too, with type at 80%.
    let light_style = resolve_styles_compat(&json!(["air"]), &mut Vec::new()).unwrap();
    let background = light_style["customProperties"]["--rv-bg"].as_str().unwrap();
    let ink = light_style["customProperties"]["--rv-chart-ink"]
        .as_str()
        .unwrap();
    assert!(print.contains(&format!("@page{{background:{background}}}:root{{")));
    assert!(print.contains(&format!("--rv-bg:{background};")));
    assert!(print.contains(&format!("--rv-chart-ink:{ink};")));
    assert!(print.contains("--rv-size-base:14.4px;"));
    assert!(print.contains("--rv-size-display:40px;"));
    assert!(print.contains("--rv-measure:33.6rem;"));
    assert!(!print.contains("-phone"));
    // Scientific's measure is in characters, which already follow the smaller type.
    let scientific = render(
        "# Paper",
        &RenderOptions {
            styles: vec!["scientific".into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert!(scientific.html.contains("--rv-measure:78ch;"));
    assert!(scientific.html.contains("--rv-size-base:13.2px;"));
    // One print sheet replaces the old rules, including the class nothing emitted.
    assert!(!report.html.contains("rvc-controls"));
    for rule in [
        ".rv-report img { width: auto; max-width: 100%; height: auto; max-height: 60vh; }",
        ".rv-report__body > :has(+ .rv-source) { break-after: avoid; }",
        ".rv-report__body tfoot { display: table-row-group; }",
        ".rv-report details::details-content { content-visibility: visible; display: block; }",
    ] {
        assert!(report.html.contains(rule), "{rule}");
    }
}

#[test]
fn pdf_hatches_are_clipped_vector_strokes_with_unique_figure_resources() {
    let chart = "```chart\ntype: bar\ntitle: Hatch values\ndata: {inline: [{x: A, g: One, y: 4}, {x: B, g: One, y: 7}, {x: A, g: Two, y: 3}, {x: B, g: Two, y: 6}]}\nencoding: {x: x, y: y, color: g}\n```\n";
    let source = format!("{chart}\n{chart}");
    let report = pdf(
        &source,
        RenderOptions {
            styles: vec!["hand-drawn".into()],
            ..Default::default()
        },
    );
    let mut all_ids = std::collections::BTreeSet::new();
    let svgs: Vec<_> = report.html.split("<svg xmlns=").skip(1).collect();
    assert_eq!(svgs.len(), 2);
    for svg in svgs {
        let svg = format!("<svg xmlns={}", &svg[..svg.find("</svg>").unwrap() + 6]);
        assert!(
            !svg.contains("<pattern") && !svg.contains("<image"),
            "SVG pattern brushes become raster images in Chrome's PDF; paper needs explicit strokes"
        );
        let mut figure_ids = std::collections::BTreeSet::new();
        for id in svg
            .split(" id=\"")
            .skip(1)
            .map(|part| part.split('"').next().unwrap())
        {
            assert!(
                all_ids.insert(id.to_owned()),
                "figure resources must not alias: {id}"
            );
            figure_ids.insert(id);
        }
        for reference in svg
            .split("url(#")
            .skip(1)
            .map(|part| part.split(')').next().unwrap())
        {
            assert!(
                figure_ids.contains(reference),
                "resource {reference} must resolve within its own figure"
            );
        }
        assert!(
            svg.matches("<clipPath").count() > 1,
            "the hatch retains its mark outline as well as the panel clip"
        );
    }
}

#[test]
fn pdf_tables_print_every_row_once_with_their_total_and_plain_headers() {
    let rows: Vec<_> = (1..=45)
        .map(|n| json!({"id": format!("C{n:03}"), "value": n}))
        .collect();
    let source = format!(
        "```table\ndata: {{inline: {}}}\ncolumns: [{{key: id}}, {{key: value, format: number}}]\nsort: {{key: value, dir: desc}}\ntotal: true\n```\n",
        serde_json::to_string(&rows).unwrap()
    );
    let screen = rendered(&source);
    assert!(
        screen
            .html
            .contains("<details class=\"rv-table__overflow\">")
    );
    // Plain headers until the runtime upgrades them (FEA-RP-22).
    assert!(screen.html.contains("data-rv-key=\"value\""));
    assert!(!screen.html.contains("<button"));
    let printed = pdf(&source, Default::default()).html;
    assert!(!printed.contains("<details"));
    assert!(!printed.contains("<button"));
    assert!(!printed.contains("data-rv-table"));
    let table = &printed[printed.find("<div class=\"rv-table\">").unwrap()..];
    let table = &table[..table.find("</div>").unwrap()];
    assert_eq!(table.matches("<thead>").count(), 1);
    assert_eq!(
        table.matches("<tr>").count(),
        47,
        "header, 45 rows and the total"
    );
    assert!(
        table.find("C045").unwrap() < table.find("C001").unwrap(),
        "sort kept"
    );
    assert!(table.ends_with(
        "<tfoot><tr><td class=\"\">Total</td><td class=\"num\">1,035</td></tr></tfoot></table>"
    ));
}

#[test]
fn table_cells_link_web_addresses_by_host_and_keep_dates_and_ids_whole() {
    let address = "https://www.reuters.com/business/openai-revenue-2025-06-09/";
    let table = json!({"data":{"inline":[
        {"id":"C001","source":"Reuters: run-rate","date":"2025-06-09","link":address,"n":"2026-10-02"},
        {"id":"D-ITUNES","source":"App Store","date":"2026-10-02T09:30:00Z","link":"","n":"7"},
        {"id":"A","source":"javascript:alert(1)","date":"2026-10","link":"https://user:secret@example.org/x","n":"8"},
        {"id":"mid market","source":"https://example.org/a b","date":"Oct 2026","link":"http://example.org","n":"9"}
    ]},"columns":[{"key":"id"},{"key":"source"},{"key":"date"},{"key":"link"},{"key":"n","align":"right"}]});
    let html = render_table(&table);
    assert!(html.contains(&format!(
        "<td class=\"link\"><a href=\"{address}\" title=\"{address}\">reuters.com</a></td>"
    )));
    assert!(html.contains("<td class=\"link\"><a href=\"http://example.org\" title=\"http://example.org\">example.org</a></td>"));
    for cell in [
        "<td class=\"id\">C001</td>",
        "<td class=\"id\">D-ITUNES</td>",
        "<td class=\"date\">2025-06-09</td>",
        "<td class=\"date\">2026-10-02T09:30:00Z</td>",
        "<td class=\"date\">2026-10</td>",
        // Words, credentials, other schemes and right-aligned columns stay as they are.
        "<td class=\"\">A</td>",
        "<td class=\"\">mid market</td>",
        "<td class=\"\">Oct 2026</td>",
        "<td class=\"\">javascript:alert(1)</td>",
        "<td class=\"\">https://user:secret@example.org/x</td>",
        "<td class=\"\">https://example.org/a b</td>",
        "<td class=\"num\">2026-10-02</td>",
    ] {
        assert!(html.contains(cell), "{cell}\n{html}");
    }
    // Sorting and the table payload keep the source values.
    assert!(html.contains(&format!("\"link\":\"{address}\"")));
}

#[test]
fn pdf_media_print_as_frames_and_cards_and_embeds_do_not_load() {
    let source = "---\ntitle: Media\ntitle_image: cover.png\n---\n\n```video\nsrc: clip.mp4\ntitle: Clip\nposter: cover.png\ncaption: Observed interaction\n```\n\n```video\nsrc: launch.mp4\ntitle: Launch\ncaption: Launch\n```\n\n```audio\nsrc: voice.mp3\ntitle: Interview\n```\n\n```embed\nurl: https://www.youtube.com/embed/example\ncaption: Keynote\n```\n";
    let mut options = RenderOptions::default();
    for (key, value) in [
        ("cover.png", "data:image/png;base64,AAAA"),
        ("voice.mp3", "data:audio/mpeg;base64,AAAA"),
        ("clip.mp4", "data:video/mp4;base64,AAAA"),
        ("launch.mp4", "data:video/mp4;base64,AAAA"),
    ] {
        options.data.insert(key.into(), value.into());
    }
    let report = pdf(source, options);
    let html = &report.html;
    for absent in ["<video", "<audio", "<iframe", "loading=\"lazy\""] {
        assert!(!html.contains(absent), "{absent}");
    }
    assert!(html.contains("<figure class=\"rv-video\"><img src=\"data:image/png;base64,AAAA\" alt=\"Clip\" loading=\"eager\"><figcaption>Observed interaction</figcaption><p class=\"rv-media__line\">Video · clip.mp4</p></figure>"));
    assert!(html.contains("<figure class=\"rv-video rv-media-card\"><p class=\"rv-media__line\">Video · Launch · launch.mp4</p></figure>"));
    assert!(html.contains("<figure class=\"rv-audio rv-media-card\"><p class=\"rv-media__line\">Audio · Interview · voice.mp3</p></figure>"));
    assert!(html.contains("<figure class=\"rv-embed rv-media-card\"><p class=\"rv-media__line\">Embedded page · Keynote · <a href=\"https://www.youtube.com/embed/example\">https://www.youtube.com/embed/example</a></p></figure>"));
    assert!(html.contains("loading=\"eager\""));
    let posters: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.code == "export.video-poster")
        .collect();
    assert_eq!(posters.len(), 1);
    assert_eq!(posters[0].line, Some(13));
}

#[test]
fn the_running_title_cannot_close_the_style_element() {
    let report = pdf(
        "---\ntitle: 'A \"quoted\" \\ title </style><script>alert(1)</script>'\n---\n\nText.\n",
        Default::default(),
    );
    assert!(report.html.contains(
        "content:\"A \\\"quoted\\\" \\\\ title \\3C /style>\\3C script>alert(1)\\3C /script>\";"
    ));
    assert_eq!(report.html.matches("</style>").count(), 1);
    assert!(!report.html.contains("<script>alert"));
}

const TABBED: &str = "---\ntitle: Tabbed revenue\nversion: 0.0.1\ntabs:\n  default: sales\n  panels:\n    - { id: overview, label: Overview, from: overview }\n    - { id: sales, label: Sales, from: sales }\n    - { id: customers, label: Customers, from: customers }\n---\n\nIntro prose above the nav.\n\n## Overview {#overview}\n\nOverview text.\n\n## Sales {#sales}\n\nSales text.\n\n## Customers {#customers}\n\nCustomers text.\n";

/// Options whose report loads the browser runtime, as previews and served reports do.
fn interactive() -> RenderOptions {
    RenderOptions {
        runtime_url: Some("https://richview.test/richview_site".into()),
        runtime_js: Some("fixture".into()),
        ..Default::default()
    }
}

fn tabs_export_active(source: &str) -> String {
    source.replace("  default: sales\n", "  default: sales\n  export: active\n")
}

#[test]
fn tabs_split_an_interactive_report_into_panels_below_the_untabbed_intro() {
    for options in [interactive(), RenderOptions::default()] {
        let report = render(TABBED, &options).unwrap();
        let html = &report.html;
        // The runtime builds the row; the page carries no inert buttons (FEA-BR-09).
        assert!(!html.contains("<nav class=\"rv-tabs\"") && !html.contains("role=\"tab\""));
        assert!(html.contains("<section class=\"rv-tabpanel\" id=\"panel-sales\" data-rv-tab=\"sales\" data-rv-tab-label=\"Sales\" data-rv-default><h2 id=\"sales\">Sales</h2>"));
        assert!(html.contains("<section class=\"rv-tabpanel\" id=\"panel-overview\" data-rv-tab=\"overview\" data-rv-tab-label=\"Overview\"><h2 id=\"overview\">Overview</h2>"));
        assert!(
            !html.contains(" hidden>"),
            "every panel is visible until the runtime runs"
        );
        let order: Vec<_> = [
            "Intro prose above the nav.",
            "id=\"panel-overview\"",
            "Overview text.",
            "id=\"panel-sales\"",
            "Sales text.",
            "id=\"panel-customers\"",
            "Customers text.",
        ]
        .iter()
        .map(|needle| html.find(needle).unwrap())
        .collect();
        assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{order:?}");
        assert!(!html.contains("<noscript>"));
        assert!(html.contains(".rv-tabs{display:flex"));
        assert!(html.contains("html[data-rv-runtime] .rv-tabpanel::before{display:none}"));
        assert!(!html.contains("rv-tabs__export-note\" role"));
        assert!(
            report
                .diagnostics
                .iter()
                .all(|d| !d.code.starts_with("tabs.") && d.code != "frontmatter.tabs")
        );
    }
}

#[test]
fn tabs_without_a_runtime_keep_every_panel_visible_and_emit_no_inert_buttons() {
    let html = render(TABBED, &RenderOptions::default()).unwrap().html;
    assert!(!html.contains("<nav class=\"rv-tabs\""));
    assert!(!html.contains(" hidden>"));
    assert!(!html.contains("<noscript>"));
    assert!(html.contains("<section class=\"rv-tabpanel\" id=\"panel-overview\" data-rv-tab=\"overview\" data-rv-tab-label=\"Overview\"><h2 id=\"overview\">Overview</h2>"));
    assert!(html.contains(
        "<section class=\"rv-tabpanel\" id=\"panel-customers\" data-rv-tab=\"customers\" data-rv-tab-label=\"Customers\">"
    ));
    // The HTML report always carries every panel; `export: active` only shapes print and PDF.
    let html = render(&tabs_export_active(TABBED), &RenderOptions::default())
        .unwrap()
        .html;
    for text in ["Overview text.", "Sales text.", "Customers text."] {
        assert!(html.contains(text));
    }
}

#[test]
fn tabs_export_active_selects_the_default_panel_on_paper_and_says_what_is_left_out() {
    let pdf = RenderOptions {
        output: richview_core::document::Output::Pdf,
        ..interactive()
    };
    let notice = "<p class=\"rv-tabs__export-note\" role=\"note\">Showing Sales, 1 of 3 views. Overview and Customers are in the live report.</p>";
    let html = render(&tabs_export_active(TABBED), &pdf).unwrap().html;
    assert!(html.contains(notice));
    assert!(html.contains(
        "<section class=\"rv-tabpanel\" id=\"panel-sales\" data-rv-tab=\"sales\" data-rv-tab-label=\"Sales\" data-rv-default data-rv-export=\"active\">"
    ));
    assert!(!html.contains("Overview text.") && !html.contains("Customers text."));
    assert!(html.contains("Intro prose above the nav."));
    assert!(!html.contains("<nav class=\"rv-tabs\""));

    // The default export prints every panel in order, each under its label.
    let html = render(TABBED, &pdf).unwrap().html;
    for text in ["Overview text.", "Sales text.", "Customers text."] {
        assert!(html.contains(text));
    }
    assert!(!html.contains("<p class=\"rv-tabs__export-note\""));

    // The HTML report keeps every view; the runtime fills the print note with the view shown.
    let html = render(&tabs_export_active(TABBED), &interactive())
        .unwrap()
        .html;
    assert!(html.contains("<p class=\"rv-tabs__export-note\" role=\"note\"></p>"));
    for text in ["Overview text.", "Sales text.", "Customers text."] {
        assert!(html.contains(text));
    }
    assert!(
        html.contains(".rv-tabpanel[data-rv-export=\"active\"][hidden]{display:none!important}")
    );

    let two = TABBED.replace(
        "    - { id: customers, label: Customers, from: customers }\n",
        "",
    );
    let html = render(&tabs_export_active(&two), &pdf).unwrap().html;
    assert!(html.contains("Showing Sales, 1 of 2 views. Overview is in the live report."));
}

#[test]
fn invalid_tabs_render_the_whole_body_with_a_diagnostic() {
    let cases = [
        (TABBED.replace("from: sales }", "from: nope }"), "tabs.anchor-missing"),
        (
            TABBED.replace(
                "    - { id: overview, label: Overview, from: overview }\n    - { id: sales, label: Sales, from: sales }\n",
                "    - { id: sales, label: Sales, from: sales }\n    - { id: overview, label: Overview, from: overview }\n",
            ),
            "tabs.order",
        ),
        (TABBED.replace("id: sales,", "id: overview,"), "frontmatter.tabs"),
        (TABBED.replace("id: sales,", "id: \"sales tab\","), "frontmatter.tabs"),
        (TABBED.replace("from: sales }", "from: \"sales\\\"\" }"), "frontmatter.tabs"),
        (TABBED.replace("label: Sales, ", ""), "frontmatter.tabs"),
        (
            TABBED.replace(
                "  panels:\n    - { id: overview, label: Overview, from: overview }\n    - { id: sales, label: Sales, from: sales }\n    - { id: customers, label: Customers, from: customers }\n",
                "  panels: notarray\n",
            ),
            "frontmatter.tabs",
        ),
    ];
    for (source, code) in cases {
        let report = render(&source, &interactive()).unwrap();
        assert!(
            report.diagnostics.iter().any(|d| d.code == code),
            "{code}: {:?}",
            report.diagnostics
        );
        assert!(!report.html.contains("<nav class=\"rv-tabs\""), "{code}");
        assert!(
            !report.html.contains("<section class=\"rv-tabpanel\""),
            "{code}"
        );
        assert!(!report.html.contains(".rv-tabs{"), "{code}");
        for text in [
            "Intro prose above the nav.",
            "Overview text.",
            "Sales text.",
            "Customers text.",
        ] {
            assert!(report.html.contains(text), "{code} hid {text}");
        }
        // Declared anchors stay heading ids rather than literal attribute text.
        assert!(
            report.html.contains("<h2 id=\"overview\">Overview</h2>"),
            "{code}"
        );
    }
    // A malformed export disables tabs even when the default is also unknown.
    let report = render(
        &TABBED.replace(
            "  default: sales\n",
            "  default: missing\n  export: paper\n",
        ),
        &interactive(),
    )
    .unwrap();
    assert_eq!(
        report
            .diagnostics
            .iter()
            .filter(|d| d.code == "frontmatter.tabs")
            .count(),
        1
    );
    assert!(!report.html.contains("<section class=\"rv-tabpanel\""));
    for text in ["Overview text.", "Sales text.", "Customers text."] {
        assert!(report.html.contains(text));
    }
}

#[test]
fn malformed_tab_fields_keep_the_complete_body_without_tabs_in_every_output() {
    use richview_core::document::Output;

    for (field, value) in [
        ("default", "42"),
        ("default", "true"),
        ("default", "null"),
        ("default", "[]"),
        ("default", "{}"),
        ("export", "paper"),
        ("export", "42"),
        ("export", "true"),
        ("export", "null"),
        ("export", "[]"),
        ("export", "{}"),
    ] {
        let fields = if field == "default" {
            format!("  default: {value}\n  export: active\n")
        } else {
            format!("  default: sales\n  export: {value}\n")
        };
        let source = TABBED.replace("  default: sales\n", &fields);
        for output in [Output::Report, Output::Pdf] {
            let report = render(
                &source,
                &RenderOptions {
                    output,
                    ..interactive()
                },
            )
            .unwrap();
            let case = format!("{field}={value}, output={output:?}");
            let finding = report
                .diagnostics
                .iter()
                .find(|d| d.code == "frontmatter.tabs")
                .expect("malformed tabs warning");
            assert_eq!(finding.level, "warning", "{case}");
            assert_eq!(finding.path, format!("frontmatter.tabs.{field}"), "{case}");
            assert!(
                !report.html.contains("<section class=\"rv-tabpanel\""),
                "{case}"
            );
            let positions: Vec<_> = [
                "Intro prose above the nav.",
                "Overview text.",
                "Sales text.",
                "Customers text.",
            ]
            .map(|text| {
                report
                    .html
                    .find(text)
                    .unwrap_or_else(|| panic!("{case} lost {text}"))
            })
            .into();
            assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "{case}");
        }
    }
}

#[test]
fn reports_without_tabs_keep_heading_text_and_carry_no_tab_markup_or_rules() {
    let source = "---\ntitle: Plain\nversion: 0.0.1\n---\n\n## Overview {#overview}\n\n## Price {USD}\n\n```callout\nkind: neutral\nbody: \"## Note {#note}\"\n```\n";
    for options in [RenderOptions::default(), interactive()] {
        let report = render(source, &options).unwrap();
        let html = report.html;
        assert!(html.contains("<h2 id=\"overview\">Overview</h2>"));
        assert!(html.contains("<h2 id=\"price-usd\">Price {USD}</h2>"));
        assert!(html.contains(">Note</h2>") && !html.contains("{#note}"));
        assert_eq!(
            report
                .diagnostics
                .iter()
                .filter(|d| d.code == "markdown.heading-id")
                .count(),
            2
        );
        for chrome in ["rv-tabs", "rv-tabpanel", "<noscript>"] {
            assert!(!html.contains(chrome), "{chrome}");
        }
    }
    // Callout headings are outside the panel-heading vocabulary even in a tabbed report.
    let tabbed = TABBED.replace(
        "Sales text.",
        "```callout\nkind: neutral\nbody: \"## Note {#note}\"\n```",
    );
    let html = render(&tabbed, &interactive()).unwrap().html;
    assert!(html.contains(">Note</h2>") && !html.contains("{#note}"));
}

#[test]
fn public_render_options_select_print_html_and_reject_unknown_output_modes() {
    let report = render_compat("# Paper", &json!({"output":"pdf", "page":"letter"})).unwrap();
    assert!(
        report["html"]
            .as_str()
            .unwrap()
            .contains("data-rv-output=\"pdf\"")
    );
    assert!(
        report["html"]
            .as_str()
            .unwrap()
            .contains("@page{size:letter portrait;")
    );
    for (key, value, code) in [
        ("output", "unknown", "render.output.invalid"),
        ("page", "unknown", "render.page.invalid"),
    ] {
        let error = render_compat("# Paper", &json!({key: value})).unwrap_err();
        assert_eq!(error[0].code, code);
    }
}

const UNSPLASH_SRC: &str =
    "https://images.unsplash.com/photo-1559538834-8b012e20a84f?w=1600&q=80&fm=jpg&fit=max";
const CREDIT: &str = "credit:\n  author: Marek <Rucinski>\n  author_url: https://unsplash.com/@marulla?utm_source=richview&utm_medium=referral\n  source: Unsplash\n  source_url: https://unsplash.com/?utm_source=richview&utm_medium=referral\n";

#[test]
fn image_credit_renders_escaped_links_after_the_caption_and_keeps_the_hotlink() {
    let report = rendered(&format!(
        "```image\nsrc: {UNSPLASH_SRC}\nalt: Sea\ncaption: A calm sea\n{CREDIT}```\n"
    ));
    let html = &report.html;
    assert!(html.contains("<img src=\"https://images.unsplash.com/photo-1559538834-8b012e20a84f?w=1600&amp;q=80&amp;fm=jpg&amp;fit=max\""));
    let caption = html.find("<figcaption>A calm sea</figcaption>").unwrap();
    let credit = html.find("<p class=\"rv-credit\">Photo by <a href=\"https://unsplash.com/@marulla?utm_source=richview&amp;utm_medium=referral\" target=\"_blank\" rel=\"noopener noreferrer\">Marek &lt;Rucinski&gt;</a> on <a href=\"https://unsplash.com/?utm_source=richview&amp;utm_medium=referral\" target=\"_blank\" rel=\"noopener noreferrer\">Unsplash</a></p>").unwrap();
    assert!(caption < credit);
    assert!(html[credit..].find("</figure>").is_some());
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.code.starts_with("image.credit"))
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "assets.network-dependent")
    );
}

#[test]
fn frontmatter_image_credit_renders_and_prints_in_pdf() {
    let source = format!(
        "---\ntitle: Sea\ntitle_image:\n  src: {UNSPLASH_SRC}\n  alt: Sea\n  {}\n---\n\nBody.\n",
        CREDIT.replace('\n', "\n  ").trim_end()
    );
    let report = rendered(&source);
    assert!(
        report
            .html
            .contains("<p class=\"rv-credit\">Photo by <a href=")
    );
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.code == "image.credit-missing")
    );
    let pdf = render(
        &source,
        &RenderOptions {
            output: richview_core::document::Output::Pdf,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(pdf.html.contains("class=\"rv-credit\""));
}

#[test]
fn unsplash_image_without_credit_is_an_error_and_the_block_stays_source() {
    let source = format!("```image\nsrc: {UNSPLASH_SRC}\nalt: Sea\n```\n");
    let parsed = parse(&source).unwrap();
    let error = parsed
        .diagnostics
        .iter()
        .find(|d| d.code == "image.credit-missing")
        .expect("missing credit error");
    assert_eq!(
        (error.level.as_str(), error.path.as_str()),
        ("error", "image.credit")
    );
    assert_eq!(error.line, Some(1));
    assert!(error.message.contains("Photo by <name> on Unsplash"));
    assert!(parsed.blocks.is_empty());
    assert!(parsed.failed[0].source.contains(UNSPLASH_SRC));
    let report = rendered(&source);
    assert!(report.html.contains("Image not rendered:"));
    assert!(report.html.contains("src: https://images.unsplash.com"));
    assert!(!report.html.contains("<img "));
    assert!(!report.html.contains("class=\"rv-credit\""));

    // The header image is omitted; the frontmatter keeps the source and the error is reported.
    for header in [
        format!("title_image: {UNSPLASH_SRC}"),
        format!("title_image: {{src: {UNSPLASH_SRC}, credit: {{source: Unsplash}}}}"),
    ] {
        let report = rendered(&format!("---\ntitle: T\n{header}\n---\n"));
        let error = report
            .diagnostics
            .iter()
            .find(|d| d.code == "image.credit-missing")
            .expect("missing credit error");
        assert_eq!(error.level, "error");
        assert_eq!(error.path, "frontmatter.title_image.credit");
        assert!(!report.html.contains("<img "));
    }

    // Another remote host is not a hotlink: the image fails alone, with a notice.
    let other = rendered("```image\nsrc: https://example.org/a.jpg\nalt: A\n```\n\nAfter.\n");
    assert!(
        other
            .diagnostics
            .iter()
            .any(|d| d.code == "report.asset.remote")
    );
    assert!(other.html.contains("rv-notice") && other.html.contains("After."));
    assert!(!other.html.contains("src=\"https://example.org"));
}

#[test]
fn unsplash_credit_links_are_pinned_to_unsplash_com() {
    let credit = |author_url: &str, source: &str, source_url: &str| {
        let mut yaml = String::from("credit:\n  author: Ada\n");
        for (key, value) in [
            ("author_url", author_url),
            ("source", source),
            ("source_url", source_url),
        ] {
            if !value.is_empty() {
                yaml.push_str(&format!("  {key}: '{value}'\n"));
            }
        }
        yaml
    };
    let profile = "https://unsplash.com/@ada?utm_source=richview";
    // Valid: the profile alone, or with the Unsplash source and page.
    for credit in [
        credit(profile, "", ""),
        credit(
            profile,
            "unsplash",
            "https://unsplash.com/?utm_source=richview",
        ),
        credit(profile, " Unsplash ", ""),
    ] {
        let parsed = parse(&format!(
            "```image\nsrc: {UNSPLASH_SRC}\nalt: A\n{credit}```\n"
        ))
        .unwrap();
        assert!(
            !parsed.diagnostics.iter().any(|d| d.level == "error"),
            "{credit}: {:?}",
            parsed.diagnostics
        );
        assert_eq!(parsed.blocks.len(), 1, "{credit}");
    }
    // Invalid: each is an error at the offending key, and the block stays source.
    let bad = [
        (credit("", "", ""), "author_url"),
        (credit("http://unsplash.com/@ada", "", ""), "author_url"),
        (credit("https://example.org/@ada", "", ""), "author_url"),
        (credit("https://Unsplash.com/@ada", "", ""), "author_url"),
        (
            credit("https://unsplash.com.evil.test/@ada", "", ""),
            "author_url",
        ),
        (
            credit("https://unsplash.com@evil.test/", "", ""),
            "author_url",
        ),
        (
            credit("https://user@unsplash.com/@ada", "", ""),
            "author_url",
        ),
        (
            credit("https://unsplash.com:8443/@ada", "", ""),
            "author_url",
        ),
        (
            credit("https://images.unsplash.com/@ada", "", ""),
            "author_url",
        ),
        (credit("javascript:alert(1)", "", ""), "author_url"),
        (credit(profile, "", "https://example.org/"), "source_url"),
        (credit(profile, "", "http://unsplash.com/"), "source_url"),
        (credit(profile, "Flickr", ""), "source"),
        (credit(profile, "Unsplash+", ""), "source"),
    ];
    for (credit, key) in bad {
        let source = format!("```image\nsrc: {UNSPLASH_SRC}\nalt: A\n{credit}```\n");
        let parsed = parse(&source).unwrap();
        let error = parsed
            .diagnostics
            .iter()
            .find(|d| d.code == "image.credit-invalid")
            .unwrap_or_else(|| panic!("{credit}"));
        assert_eq!(error.level, "error", "{credit}");
        assert_eq!(error.path, format!("image.credit.{key}"), "{credit}");
        // The field's own line, or the line of its nearest written ancestor (TYP-56).
        let at = |key: &str| {
            source
                .lines()
                .position(|line| line.trim_start().starts_with(&format!("{key}:")))
                .map(|index| index + 1)
        };
        assert_eq!(error.line, at(key).or_else(|| at("credit")), "{credit}");
        assert!(parsed.blocks.is_empty(), "{credit}");
        let report = render(&source, &RenderOptions::default());
        assert!(
            report.as_ref().map_or(true, |r| !r.html.contains("<img ")),
            "{credit}"
        );

        // The header image is omitted and the frontmatter keeps its source.
        let flow = credit.replace('\n', ", ").replace("credit:, ", "credit: {");
        let header = rendered(&format!(
            "---\ntitle: T\ntitle_image: {{src: {UNSPLASH_SRC}, {}}}}}\n---\n",
            flow.trim_end_matches(", ")
        ));
        let error = header
            .diagnostics
            .iter()
            .find(|d| d.code == "image.credit-invalid")
            .unwrap_or_else(|| panic!("{flow}"));
        assert_eq!(error.level, "error", "{flow}");
        assert_eq!(
            error.path,
            format!("frontmatter.title_image.credit.{key}"),
            "{flow}"
        );
        assert!(!header.html.contains("<img "), "{flow}");
        assert!(!header.html.contains("class=\"rv-credit\""), "{flow}");
    }

    // A missing author is still image.credit-missing, and a non-string value is rejected.
    let parsed = parse(&format!(
        "```image\nsrc: {UNSPLASH_SRC}\nalt: A\ncredit: {{author_url: 'https://unsplash.com/@ada'}}\n```\n"
    ))
    .unwrap();
    assert!(
        parsed
            .diagnostics
            .iter()
            .any(|d| d.code == "image.credit-missing")
    );
    let parsed = parse(&format!(
        "```image\nsrc: {UNSPLASH_SRC}\nalt: A\ncredit: {{author: Ada, author_url: 5}}\n```\n"
    ))
    .unwrap();
    assert!(parsed.blocks.is_empty());

    // Credits on local images keep the lenient rule: any http(s) link is linked, no error.
    let local = rendered(
        "```image\nsrc: 'data:image/gif;base64,R0lGODlhAQABAAAAACw='\nalt: A\ncredit: {author: Ada, author_url: 'https://example.org/ada', source: Flickr, source_url: 'http://example.org/'}\n```\n",
    );
    assert!(
        local
            .html
            .contains("Photo by <a href=\"https://example.org/ada\"")
    );
    assert!(
        local
            .html
            .contains("on <a href=\"http://example.org/\" target=\"_blank\" rel=\"noopener noreferrer\">Flickr</a>")
    );
    assert!(!local.diagnostics.iter().any(|d| d.level == "error"));
}

#[test]
fn only_the_exact_unsplash_image_origin_counts_as_a_hotlink() {
    for good in [
        "https://images.unsplash.com/photo-1?w=10",
        "https://images.unsplash.com/p",
    ] {
        assert!(richview_core::unsplash_image_url(good), "{good}");
    }
    for bad in [
        "http://images.unsplash.com/p",
        "https://Images.Unsplash.com/p",
        "https://IMAGES.UNSPLASH.COM/p",
        "https://images.unsplash.com",
        "https://images.unsplash.com:443/p",
        "https://images.unsplash.com:8443/p",
        "https://user@images.unsplash.com/p",
        "https://user:pw@images.unsplash.com/p",
        "https://images.unsplash.com@evil.test/p",
        "https://images.unsplash.com.evil.test/p",
        "https://evil.test/images.unsplash.com/p",
        "https://evil.test\\@images.unsplash.com/p",
        "https://images.unsplash.com\\.evil.test/p",
        "https://plus.unsplash.com/p",
        "https://unsplash.com/p",
        "https://example.org/a.jpg",
        "https://images.unsplash.com/p q",
        "//images.unsplash.com/p",
        "data:image/png;base64,AAAA",
        "images.unsplash.com/p",
    ] {
        assert!(!richview_core::unsplash_image_url(bad), "{bad}");
    }
    // Another remote image is a remote asset when the report folder is checked, as the renderer
    // reports it (TYP-40).
    let folder = richview_core::document::ReportFolder {
        root: "r".into(),
        source: "report.rmd".into(),
        files: vec!["report.rmd".into()],
    };
    for src in [
        "https://example.org/a.jpg",
        "https://plus.unsplash.com/p",
        "https://images.unsplash.com:8443/p",
        "https://Images.Unsplash.com/p",
    ] {
        let document = parse(&format!("```image\nsrc: {src}\nalt: A\n```\n")).unwrap();
        let issues = richview_core::document::validate_report_document(&document, Some(&folder))
            .expect_err(src);
        assert!(
            issues.iter().any(|d| d.code == "report.asset.remote"),
            "{src}"
        );
        let header = parse(&format!("---\ntitle: T\ntitle_image: {src}\n---\n")).unwrap();
        assert!(
            richview_core::document::validate_report_document(&header, Some(&folder)).is_err(),
            "{src}"
        );
    }
    let document = parse(&format!(
        "```image\nsrc: {UNSPLASH_SRC}\nalt: A\n{CREDIT}```\n"
    ))
    .unwrap();
    assert!(richview_core::document::validate_report_document(&document, Some(&folder)).is_ok());
}

#[test]
fn a_photo_reserves_its_box_and_shows_its_colour_without_the_file() {
    let report = rendered(&format!(
        "```image\nsrc: {UNSPLASH_SRC}\nalt: Sea\nwidth: 1600\nheight: 1067\ncolor: '#0c2640'\n{CREDIT}```\n"
    ));
    let html = &report.html;
    assert!(html.contains("width=\"1600\" height=\"1067\" style=\"aspect-ratio:1600/1067;background-color:#0c2640;color:oklch(1 0 0);object-fit:contain\""), "{html}");
    assert!(html.contains("class=\"rv-credit\""));
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.code == "image.color-invalid")
    );

    // A light colour gets dark alt text; without a colour or size nothing is added.
    let light = rendered(&format!(
        "```image\nsrc: {UNSPLASH_SRC}\nalt: A\nwidth: 4\nheight: 3\ncolor: '#F0E0D0'\n{CREDIT}```\n"
    ));
    assert!(
        light
            .html
            .contains("aspect-ratio:4/3;background-color:#F0E0D0;color:oklch(0 0 0);")
    );
    let plain = rendered(&format!(
        "```image\nsrc: {UNSPLASH_SRC}\nalt: A\n{CREDIT}```\n"
    ));
    assert!(plain.html.contains("style=\"object-fit:contain\""));

    // The header image carries the same box.
    let header = rendered(&format!(
        "---\ntitle: T\ntitle_image: {{src: {UNSPLASH_SRC}, width: 3, height: 2, color: '#112233', credit: {{author: Ada, author_url: 'https://unsplash.com/@ada'}}}}\n---\n"
    ));
    assert!(
        header
            .html
            .contains("aspect-ratio:3/2;background-color:#112233;")
    );
}

#[test]
fn a_colour_that_is_not_rrggbb_is_dropped_with_a_warning() {
    for bad in [
        "red",
        "'#fff'",
        "'#12345g'",
        "'#11223344'",
        "'rgb(0,0,0)'",
        "'#000;background:url(//evil.test/x)'",
        "'\"><script>'",
        "12",
        "[1]",
    ] {
        let source = format!(
            "```image\nsrc: {UNSPLASH_SRC}\nalt: A\nwidth: 4\nheight: 3\ncolor: {bad}\n{CREDIT}```\n"
        );
        let report = rendered(&source);
        let warning = report
            .diagnostics
            .iter()
            .find(|d| d.code == "image.color-invalid")
            .unwrap_or_else(|| panic!("{bad}"));
        assert_eq!(
            (warning.level.as_str(), warning.path.as_str()),
            ("warning", "image.color")
        );
        assert!(report.html.contains("<img "), "{bad}");
        // The stylesheet itself uses background-color; the image box must not.
        assert!(!report.html.contains(";background-color:"), "{bad}");
        assert!(!report.html.contains("evil.test/x"), "{bad}");
    }
    let header = rendered(&format!(
        "---\ntitle: T\ntitle_image: {{src: {UNSPLASH_SRC}, color: nope, credit: {{author: Ada, author_url: 'https://unsplash.com/@ada'}}}}\n---\n"
    ));
    assert!(
        header
            .diagnostics
            .iter()
            .any(|d| d.code == "image.color-invalid" && d.path == "frontmatter.title_image.color")
    );
    assert!(!header.html.contains(";background-color:"));
}

#[test]
fn unsafe_credit_urls_drop_the_link_and_malformed_credits_are_schema_errors() {
    let report = rendered(
        "```image\nsrc: 'data:image/gif;base64,R0lGODlhAQABAAAAACw='\nalt: A\ncredit:\n  author: Ada\n  author_url: 'javascript:alert(1)'\n  source: Unsplash\n  source_url: https://unsplash.com/?utm_source=x\n```\n",
    );
    assert!(
        report
            .html
            .contains("Photo by Ada on <a href=\"https://unsplash.com/?utm_source=x\"")
    );
    assert!(!report.html.contains("javascript:"));
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "image.credit-url" && d.level == "warning")
    );
    let author_only = rendered(
        "```image\nsrc: 'data:image/gif;base64,R0lGODlhAQABAAAAACw='\nalt: A\ncredit: {author: Ada}\n```\n",
    );
    assert!(
        author_only
            .html
            .contains("<p class=\"rv-credit\">Photo by Ada</p>")
    );
    let parsed = parse(&format!(
        "```image\nsrc: {UNSPLASH_SRC}\nalt: A\ncredit: {{source: Unsplash}}\n```\n"
    ))
    .unwrap();
    assert!(
        parsed
            .diagnostics
            .iter()
            .any(|d| d.code == "image.credit-missing" && d.level == "error")
    );
    assert!(parsed.blocks.is_empty());
    let parsed = parse("```image\nsrc: a.png\nalt: A\ncredit: {source: Unsplash}\n```\n").unwrap();
    assert!(
        parsed
            .diagnostics
            .iter()
            .any(|d| d.code == "block.schema" && d.path == "image.credit.author")
    );
    assert!(parsed.blocks.is_empty());
    let parsed = parse("```image\nsrc: a.png\nalt: A\ncredit: Ada\n```\n").unwrap();
    assert!(parsed.diagnostics.iter().any(|d| d.path == "image.credit"));
    let header = rendered(
        "---\ntitle: T\ntitle_image: {src: 'data:image/gif;base64,R0lGODlhAQABAAAAACw=', credit: {source: Unsplash}}\n---\n",
    );
    assert!(
        header
            .diagnostics
            .iter()
            .any(|d| d.code == "image.credit-incomplete" && d.level == "warning")
    );
    assert!(!header.html.contains("class=\"rv-credit\""));
}

#[test]
fn unsplash_photos_are_hotlinks_not_report_resources() {
    let document = parse(&format!(
        "---\ntitle: T\ntitle_image: {UNSPLASH_SRC}\n---\n\n```image\nsrc: {UNSPLASH_SRC}\nalt: A\n{CREDIT}```\n\n```image\nsrc: local.png\nalt: L\n```\n"
    ))
    .unwrap();
    let resources: Vec<_> = richview_core::document::report_resources(&document)
        .into_iter()
        .map(|r| r.path)
        .collect();
    assert_eq!(resources, ["local.png"]);
    let video = rendered("```video\nsrc: https://example.org/clip.mp4\ntitle: Clip\n```\n");
    assert!(
        video
            .diagnostics
            .iter()
            .any(|d| d.code == "report.asset.remote")
    );
}

#[test]
fn only_the_renderer_marks_a_credited_photo_for_hosts_that_load_it() {
    let credited = rendered(&format!(
        "```image\nsrc: {UNSPLASH_SRC}\nalt: A\n{CREDIT}```\n"
    ))
    .html;
    assert!(credited.contains(&format!(
        "<img src=\"{}\" data-rv-photo=\"credited\"",
        UNSPLASH_SRC.replace('&', "&amp;")
    )));
    let header = rendered(&format!(
        "---\ntitle: T\ntitle_image: {{src: {UNSPLASH_SRC}, credit: {{author: Ada, author_url: 'https://unsplash.com/@ada'}}}}\n---\n"
    ))
    .html;
    assert!(header.contains("data-rv-photo=\"credited\""));
    // The sanitizer drops data-* attributes from raw HTML, along with other media elements.
    let spoof = rendered(
        "<figure class=\"rv-image\"><img src=\"data:image/gif;base64,R0lGODlhAQABAAAAACw=\" data-rv-photo=\"credited\"><picture><source srcset=\"https://images.unsplash.com/x 1x\"></picture><p class=\"rv-credit\">x</p></figure>\n",
    )
    .html;
    assert!(!spoof.contains("data-rv-photo"), "{spoof}");
    assert!(
        !spoof.contains("<source") && !spoof.contains("srcset"),
        "{spoof}"
    );
}

#[test]
fn unmeasured_is_not_an_accepted_diagnostics_status() {
    for status in ["clean", "issues"] {
        let value = json!({ "status": status, "charts": [], "parse": [] });
        assert!(parse_schema("RenderDiagnosticsSchema", value).is_ok());
    }
    for status in ["unmeasured", "unknown"] {
        let value = json!({ "status": status, "charts": [], "parse": [] });
        let errors = parse_schema("RenderDiagnosticsSchema", value).unwrap_err();
        assert!(errors.iter().any(|d| d.path == "status"), "{status}");
    }
}

fn style_diagnostics(source: &str) -> Vec<richview_core::document::Diagnostic> {
    render(
        "# Claim",
        &RenderOptions {
            styles: vec![source.into()],
            ..Default::default()
        },
    )
    .unwrap()
    .diagnostics
    .into_iter()
    // Token-level files are the presets' internal vocabulary: they still apply with
    // `style.legacy` (STY-73), which these grammar and safety tests do not measure.
    .filter(|d| d.code.starts_with("style.") && d.code != "style.legacy")
    .collect()
}

fn resolved_style(source: &str) -> serde_json::Value {
    let mut diagnostics = vec![];
    resolve_styles_compat(
        &json!([{"kind":"source","source":source}]),
        &mut diagnostics,
    )
    .unwrap()
}

#[test]
fn colour_names_are_the_css_keywords_and_nothing_else() {
    for ok in [
        "rebeccapurple",
        "Transparent",
        "currentColor",
        "RED",
        "lightgoldenrodyellow",
    ] {
        // A named colour is a colour like any other: the contrast floor applies to it.
        let found = style_diagnostics(&format!("tokens: {{accent: {ok}}}"));
        assert!(
            found.iter().all(|d| d.code == "style.contrast"),
            "{ok}: {found:?}"
        );
    }
    for bad in ["notacolor", "redd", "inherit", "initial"] {
        let found = style_diagnostics(&format!("tokens: {{accent: {bad}}}"));
        assert!(
            found
                .iter()
                .any(|d| d.code == "style.schema" && d.path == "styles[0].tokens.accent"),
            "{bad}: {found:?}"
        );
    }
}

#[test]
fn unsafe_chart_colours_are_refused_at_the_leaf_and_never_reach_the_cascade() {
    for (source, leaf, line) in [
        (
            "chart:\n  palette: ['red;}</style><script>', 'blue']\n",
            "chart.palette",
            2,
        ),
        (
            "chart:\n  bar-radius: 3\n  ramp: ['url(https://example.com/x)', 'red', 'blue']\n",
            "chart.ramp",
            3,
        ),
        ("chart:\n  ramp: [red, blue]\n", "chart.ramp", 2),
        ("chart:\n  curve: spline\n", "chart.curve", 2),
        ("chart:\n  font-numeric: 'a;b'\n", "chart.font-numeric", 2),
        ("chart:\n  bar-radius: 99\n", "chart.bar-radius", 2),
        ("chart:\n  palette: nonesuch\n", "chart.palette", 2),
    ] {
        let found = style_diagnostics(source);
        let d = found
            .iter()
            .find(|d| d.code == "style.schema")
            .unwrap_or_else(|| panic!("{source}: {found:?}"));
        assert_eq!(d.path, format!("styles[0].{leaf}"), "{source}");
        assert_eq!(d.line, Some(line), "{source}");
        // The refused leaf is skipped, so none of its value reaches the cascade: the default
        // builtin underneath stays as it was.
        assert_eq!(
            resolved_style(source)["chart"],
            resolved_style("")["chart"],
            "{source}"
        );
    }
}

#[test]
fn every_colour_a_chart_list_admits_is_one_the_engine_reads() {
    // Names, hex, rgb(), hsl(), lab(), lch(), oklab() and oklch() all resolve to a colour, so a
    // palette entry is never dropped and a series never takes another's colour.
    let forms = [
        "rebeccapurple",
        "NAVY",
        "#abc",
        "#abcd",
        "#aabbcc",
        "#aabbccdd",
        "rgb(10 20 30)",
        "rgb(10, 20, 30)",
        "rgba(10, 20, 30, 0.5)",
        "rgb(10% 20% 30% / 50%)",
        "hsl(210 50% 40%)",
        "hsla(210, 50%, 40%, 0.5)",
        "lab(50% 40 59.5)",
        "lch(50% 70 40)",
        "oklab(0.6 0.1 0.1)",
        "oklch(0.6 0.1 40)",
    ];
    for form in forms {
        let quoted = format!("'{form}'");
        assert!(
            richview_chart::color::parse(form).is_some(),
            "{form} is a colour the engine cannot read"
        );
        let found = style_diagnostics(&format!("chart:\n  palette: [{quoted}]\n"));
        assert!(found.is_empty(), "{form}: {found:?}");
        let found = style_diagnostics(&format!("chart:\n  ramp: [{quoted}, {quoted}, {quoted}]\n"));
        assert!(found.is_empty(), "{form}: {found:?}");
    }
    // Valid for a page token, but no colour for a chart to draw with.
    for form in ["transparent", "currentcolor", "#12345"] {
        for source in [
            format!("chart:\n  palette: ['{form}']\n"),
            format!("chart:\n  ramp: ['{form}', red, blue]\n"),
        ] {
            let found = style_diagnostics(&source);
            assert!(
                found.iter().any(|d| d.code == "style.schema"),
                "{source}: {found:?}"
            );
        }
    }
    assert!(style_diagnostics("tokens: {accent: transparent}").is_empty());
}

#[test]
fn an_unknown_chart_key_is_reported() {
    for key in ["shine", "ink", "fontText", "background"] {
        let found = style_diagnostics(&format!("chart:\n  {key}: red\n"));
        assert!(
            found.iter().any(
                |d| d.code == "style.unknown-key" && d.path == format!("styles[0].chart.{key}")
            ),
            "{key}: {found:?}"
        );
    }
}

#[test]
fn valid_chart_palettes_and_leaves_still_apply() {
    let source = "chart:\n  palette: ['#112233', rebeccapurple, 'oklch(0.5 0.1 20)']\n  ramp: [white, '#888', black]\n  negative-ramp: [white, orange, red]\n  curve: step_after\n  font-numeric: 'Geist Mono, monospace'\n  font-family: 'Geist, sans-serif'\n  bar-radius: 2\n  decals: true\n  curvature: 0.5\n  legend: bottom\n  line: {smoothing: straight}\n";
    assert!(
        style_diagnostics(source).is_empty(),
        "{:?}",
        style_diagnostics(source)
    );
    let chart = resolved_style(source)["chart"].clone();
    assert_eq!(chart["palette"][1], "rebeccapurple");
    assert_eq!(chart["curve"], "step_after");
    let named = resolved_style("chart:\n  palette: scientific\n");
    assert_eq!(named["chart"]["palette"], "scientific");
    assert!(style_diagnostics("chart:\n  palette: air\n").is_empty());
}

#[test]
fn the_report_language_sets_the_separators_of_metrics_and_tables() {
    let body = "```metric\nvalue: 1234.5\nformat: number\n```\n\n```table\ndata: {inline: [{n: 1234.5}]}\ncolumns: [{key: n, format: number}]\n```\n";
    let english = rendered(&format!("---\ntitle: T\nlang: en\n---\n\n{body}")).html;
    assert!(english.contains(">1,235<"), "english grouping");
    let czech = rendered(&format!("---\ntitle: T\nlang: cs\n---\n\n{body}")).html;
    assert!(czech.contains(">1\u{a0}235<"), "czech grouping: {czech:.0}");
    assert!(!czech.contains(">1,235<"));
    let german = rendered(&format!("---\ntitle: T\nlang: de-DE\n---\n\n{body}")).html;
    assert!(german.contains(">1.235<"), "german grouping");
}

#[test]
fn the_format_marker_is_checked_and_a_newer_major_version_is_refused() {
    let diagnostics = |marker: &str| validate(&format!("---\ntitle: T\nformat: {marker}\n---\n"));
    assert!(diagnostics("rmd/1").is_empty());
    let newer = diagnostics("rmd/2");
    assert!(
        newer
            .iter()
            .any(|d| d.code == "frontmatter.format" && d.level == "error")
    );
    let garbled = diagnostics("v1");
    assert!(garbled.iter().any(|d| d.code == "frontmatter.format"));
}

#[test]
fn retired_frontmatter_keys_fail_with_the_fix_and_never_reach_the_page() {
    let source = "---\ntitle: T\nsubtitle: S\nprolog: P\ntheme: dark\nimage: a.png\n---\n\nBody\n";
    let report = rendered(source);
    for key in ["subtitle", "prolog", "theme", "image"] {
        let found = report
            .diagnostics
            .iter()
            .find(|d| d.path == format!("frontmatter.{key}"))
            .unwrap_or_else(|| panic!("{key}"));
        assert_eq!(
            (found.code.as_str(), found.level.as_str()),
            ("frontmatter.renamed", "error")
        );
        assert!(found.fix.is_some(), "{key}");
    }
    assert!(!report.html.contains(">S<") && !report.html.contains(">P<"));
    let mut dated =
        validate("---\ntitle: T\ndate: last Tuesday\ntags: draft\nstyles: sleek\n---\n");
    dated.retain(|d| d.code != "frontmatter.unknown-key");
    assert_eq!(
        dated.iter().map(|d| d.code.as_str()).collect::<Vec<_>>(),
        ["frontmatter.date"]
    );
}

#[test]
fn hostile_motion_leaves_never_reach_the_page() {
    for hostile in [
        "motion: {easing: 'url(evil)'}",
        "motion: {block-entrance: 'none;} evil {'}",
        "motion: {duration: 'url(evil)'}",
        "motion: {stagger: '1ms;} evil {'}",
        "motion: {distance: 'calc(evil)'}",
        "motion: {character: '</style><script>evil</script>'}",
    ] {
        let report = render(
            "# Claim",
            &RenderOptions {
                styles: vec![hostile.into()],
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            report.diagnostics.iter().any(|d| d.code == "style.schema"),
            "{hostile} is reported"
        );
        for needle in ["evil", "url(evil)"] {
            assert!(!report.html.contains(needle), "{hostile} reaches the page");
        }
    }
}

#[test]
fn a_still_character_renders_everything_settled() {
    let chart = "```chart\ntype: bar\ntitle: Still\ndata: {inline: [{x: a, y: 1}]}\nencoding: {x: x, y: y}\n```";
    let report = render(
        chart,
        &RenderOptions {
            styles: vec!["motion: {character: still}".into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert!(report.html.contains("--rv-motion-block:none"));
    assert_eq!(report.charts[0]["theme"]["motion"], json!(false));
}

#[test]
fn a_report_that_prints_as_air_keeps_the_faces_it_embeds() {
    let report = render("# Claim", &RenderOptions::default()).unwrap();
    assert!(report.html.contains("font-family: \"Geist\""));
    assert!(
        !report.html.contains("font-family: \"RichView Sans\""),
        "Sleek prints with its own faces, so no second family ships (PRF-32)"
    );
    let print = report.html.split("@media print{@page").nth(1).unwrap_or("");
    assert!(
        print.contains("--rv-bg:"),
        "paper still takes Air's colours"
    );
    assert!(!print.contains("--rv-font-text:\"RichView Sans"));
}

#[test]
fn a_motion_character_alone_sets_its_timing() {
    let report = render(
        "# Claim",
        &RenderOptions {
            styles: vec!["motion: {character: crisp}".into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert!(report.html.contains("--rv-motion-duration:160ms"));
    assert!(report.html.contains("--rv-motion-block:none"));
    let report = render(
        "# Claim",
        &RenderOptions {
            styles: vec!["motion: {character: crisp, duration: 300}".into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert!(report.html.contains("--rv-motion-duration:300ms"));
}

#[test]
fn quoted_motion_numbers_apply_a_fade_stays_put_and_a_font_quote_must_close() {
    let chart = "```chart\ntype: bar\ntitle: Quoted\ndata: {inline: [{x: a, y: 1}]}\nencoding: {x: x, y: y}\n```";
    let report = render(
        chart,
        &RenderOptions {
            styles: vec!["motion: {duration: '1000', block-entrance: fade, distance: 20}".into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert!(report.html.contains("--rv-motion-duration:1000ms"));
    assert!(report.html.contains("--rv-motion-distance:0px"));
    assert_eq!(
        report.charts[0]["theme"]["motionProfile"]["durationMs"],
        json!(1000.0)
    );
    let report = render(
        "# Claim",
        &RenderOptions {
            styles: vec![r#"tokens: {font-text: 'Geist", serif'}"#.into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert!(report.diagnostics.iter().any(|d| d.code == "style.schema"));
}

#[test]
fn a_chart_title_sits_one_step_below_the_section_heading() {
    let report = render("# Claim", &RenderOptions::default()).unwrap();
    assert!(
        report
            .html
            .contains(".rv-chart .rv-chart__title{flex:1 1 15rem;min-width:0;margin:0 0 .25rem;transform:translateX(calc(-1 * var(--rv-title-bearing,0px)));font:600 calc(var(--rv-size-base,1.125rem) * .9)"),
        "the title takes a tenth under the base size (Daniel, 9 Nov), so an h3 above a chart stays the larger"
    );
}

#[test]
fn a_font_stack_whose_quotes_interleave_is_refused() {
    let stack = r#"tokens: {font-text: "\"a'b\"c', serif"}"#;
    let report = render(
        "# Claim",
        &RenderOptions {
            styles: vec![stack.into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        report.diagnostics.iter().any(|d| d.code == "style.schema"),
        "{stack}"
    );
}

#[test]
fn neighbouring_stats_and_callouts_sit_closer_than_separate_blocks() {
    let report = render("# Claim", &RenderOptions::default()).unwrap();
    assert!(
        report
            .html
            .contains(".rv-metric:has(+ .rv-metric), .rv-callout:has(+ .rv-callout)")
    );
}

#[test]
fn a_chart_printed_on_paper_draws_with_the_faces_the_file_embeds() {
    let chart = "```chart\ntype: bar\ntitle: Faces\ndata: {inline: [{x: a, y: 1}]}\nencoding: {x: x, y: y}\n```";
    for style in ["sleek", "air", "scientific", "brutalist", "hand-drawn"] {
        let report = render(
            chart,
            &RenderOptions {
                styles: vec![style.into()],
                ..Default::default()
            },
        )
        .unwrap();
        let payload = &report.charts[0];
        let Some(print) = payload.get("print").filter(|p| !p.is_null()) else {
            continue;
        };
        for face in ["dataFont", "figuresFont", "uiFont"] {
            assert_eq!(
                print[face], payload["theme"][face],
                "{style}: print {face} is the screen's, which the file embeds"
            );
        }
    }
}

/// `html` without its `<span …>` and `</span>` tags: the text a reader sees in code.
fn without_spans(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(at) = rest
        .find("<span")
        .or_else(|| rest.find("</span>"))
        .map(|_| {
            let open = rest.find("<span").unwrap_or(usize::MAX);
            let close = rest.find("</span>").unwrap_or(usize::MAX);
            open.min(close)
        })
    {
        out.push_str(&rest[..at]);
        let end = rest[at..].find('>').map_or(rest.len(), |i| at + i + 1);
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}
