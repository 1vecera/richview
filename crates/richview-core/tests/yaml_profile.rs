//! The RichView YAML profile and diagnostic positions through the public parser and renderer:
//! profile violations are errors at their own line (TYP-02), and diagnostics from YAML point at
//! the field's line and column, with the style file named (TYP-53, TYP-56).

use richview_core::document::{Diagnostic, RenderOptions, parse, render};

fn find<'a>(diagnostics: &'a [Diagnostic], code: &str) -> &'a Diagnostic {
    diagnostics
        .iter()
        .find(|d| d.code == code)
        .unwrap_or_else(|| panic!("no {code} in {diagnostics:#?}"))
}

#[test]
fn a_duplicate_frontmatter_key_is_an_error_at_the_second_key() {
    let doc = parse("---\ntitle: One\nauthor: Ada\ntitle: Two\n---\n\nBody\n").unwrap();
    let d = find(&doc.diagnostics, "yaml.duplicate-key");
    assert_eq!(d.level, "error");
    assert_eq!((d.line, d.column), (Some(4), Some(1)));
    assert_eq!(d.path, "frontmatter");
    assert!(d.message.contains("`title`"), "{}", d.message);
    // The frontmatter is not applied, so neither title wins.
    assert!(doc.frontmatter.get("title").is_none());
}

#[test]
fn anchors_tags_and_non_finite_numbers_in_a_fence_are_errors_and_the_block_shows_as_source() {
    let source = "# Report\n\n```chart\ntype: bar\ntitle: T\nstyle: &s {}\n```\n\n```table\ncolumns: [a]\nrows: !!seq []\n```\n\n```metric\nvalue: .inf\nlabel: L\n```\n";
    let doc = parse(source).unwrap();
    let codes: Vec<(&str, Option<usize>)> = doc
        .diagnostics
        .iter()
        .filter(|d| d.code.starts_with("yaml."))
        .map(|d| (d.code.as_str(), d.line))
        .collect();
    assert_eq!(
        codes,
        [
            ("yaml.unsupported", Some(6)),
            ("yaml.unsupported", Some(11)),
            ("yaml.unsupported", Some(15)),
        ]
    );
    assert!(doc.blocks.is_empty());
    // Each failed fence keeps its source for the notice (FEA-RP-29).
    assert_eq!(doc.failed.len(), 3);
    assert!(doc.failed.iter().all(|f| !f.source.trim().is_empty()));
}

#[test]
fn text_that_is_not_yaml_keeps_its_family_code_with_a_position() {
    let doc = parse("Intro\n\n```chart\ntype: bar\ntitle: [unclosed\n```\n").unwrap();
    let d = find(&doc.diagnostics, "block.yaml");
    assert_eq!(d.level, "error");
    assert!(d.line.is_some_and(|line| (4..=6).contains(&line)), "{d:?}");
    assert!(d.column.is_some(), "{d:?}");
    assert_eq!(d.tag.as_deref(), Some("chart"));
}

#[test]
fn profile_scalars_reach_the_document_as_the_profile_reads_them() {
    let doc = parse(
        "---\ntitle: T\nslug: yes\nversion: 007\n---\n\n```table\ndata:\n  inline:\n    - {a: 0x10, b: 0b11, c: 1_000, d: 2026-10-27}\n```\n",
    )
    .unwrap();
    // `yes` and `007` stay text, so they reach the frontmatter's string fields.
    assert_eq!(doc.frontmatter["slug"], "yes");
    assert_eq!(doc.blocks.len(), 1, "{:#?}", doc.diagnostics);
    let row = &doc.blocks[0].value["data"]["inline"][0];
    assert_eq!(row["a"], 16);
    assert_eq!(row["b"], "0b11");
    assert_eq!(row["c"], "1_000");
    assert_eq!(row["d"], "2026-10-27");
}

#[test]
fn block_diagnostics_point_at_the_field_not_the_fence() {
    let source = "# Report\n\n```chart\ntype: bar\ntitle: T\nencoding:\n  x: region\n  y: 12\ndata:\n  inline:\n    - {region: A, sales: 1}\n```\n";
    let doc = parse(source).unwrap();
    let field: Vec<&Diagnostic> = doc
        .diagnostics
        .iter()
        .filter(|d| d.path.starts_with("chart.encoding"))
        .collect();
    assert!(!field.is_empty(), "{:#?}", doc.diagnostics);
    for d in field {
        // `encoding.y` is on line 8, column 3; a diagnostic on `encoding` itself is on line 6.
        assert!(
            matches!((d.line, d.column), (Some(8), Some(3)) | (Some(6), Some(1))),
            "{d:?}"
        );
        assert_eq!(d.tag.as_deref(), Some("chart"));
    }
}

#[test]
fn frontmatter_field_diagnostics_point_at_their_key() {
    let doc = parse("---\ntitle: T\nauthor: 3\ntags: 3\n---\n\nBody\n").unwrap();
    let author = doc
        .diagnostics
        .iter()
        .find(|d| d.path == "frontmatter.author")
        .expect("author diagnostic");
    assert_eq!((author.line, author.column), (Some(3), Some(1)));
    let tags = doc
        .diagnostics
        .iter()
        .find(|d| d.path == "frontmatter.tags")
        .expect("tags diagnostic");
    assert_eq!((tags.line, tags.column), (Some(4), Some(1)));
}

#[test]
fn style_file_diagnostics_name_the_file_and_the_line_inside_it() {
    let options = RenderOptions {
        data: std::collections::BTreeMap::from([(
            "custom.rstyle.yaml".into(),
            "tokens:\n  ink: \"#111111\"\n  accent: \"#12\"\n".into(),
        )]),
        report_folder: Some(richview_core::document::ReportFolder {
            root: "style-lines".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into(), "custom.rstyle.yaml".into()],
        }),
        ..Default::default()
    };
    let report = render(
        "---\ntitle: T\nstyles: [brutalist, custom.rstyle.yaml]\n---\n\nBody\n",
        &options,
    )
    .unwrap();
    let d = report
        .diagnostics
        .iter()
        .find(|d| d.path == "styles[1].tokens.accent" && d.code == "style.retired")
        .unwrap_or_else(|| panic!("{:#?}", report.diagnostics));
    assert_eq!(d.file.as_deref(), Some("custom.rstyle.yaml"));
    assert_eq!((d.line, d.column), (Some(3), Some(3)), "{d:?}");

    let duplicate = render(
        "---\ntitle: T\nstyles: [custom.rstyle.yaml]\n---\n\nBody\n",
        &RenderOptions {
            data: std::collections::BTreeMap::from([(
                "custom.rstyle.yaml".into(),
                "tokens:\n  ink: \"#111111\"\n  ink: \"#222222\"\n".into(),
            )]),
            report_folder: options.report_folder.clone(),
            ..Default::default()
        },
    )
    .unwrap();
    let d = find(&duplicate.diagnostics, "yaml.duplicate-key");
    assert_eq!(d.file.as_deref(), Some("custom.rstyle.yaml"));
    assert_eq!(d.line, Some(3));
}

#[test]
fn the_diagnostic_record_serialises_only_the_fields_it_has() {
    let d = Diagnostic::new("error", "block.schema", "Bad value", "chart.type")
        .fix("Use one of the chart types")
        .expected("bar, line, area, scatter, pie, heatmap");
    let json = serde_json::to_value(&d).unwrap();
    assert_eq!(json["fix"], "Use one of the chart types");
    assert_eq!(json["expected"], "bar, line, area, scatter, pie, heatmap");
    for absent in ["retry", "file", "line", "column", "tag", "details"] {
        assert!(json.get(absent).is_none(), "{absent} in {json}");
    }
    let host = Diagnostic::new("error", "publish.quota", "Quota reached", "").retry(true);
    assert_eq!(serde_json::to_value(&host).unwrap()["retry"], true);
}
