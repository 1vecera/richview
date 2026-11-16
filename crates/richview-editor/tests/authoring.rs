use richview_core::document::{RenderOptions, parse, render as core_render};
use richview_editor::authoring::{complete, diagnostics, format};
use serde_json::{Value, json};

fn render(
    source: &str,
    options: &RenderOptions,
) -> Result<richview_core::document::RenderedReport, Vec<richview_core::document::Diagnostic>> {
    let mut options = options.clone();
    options.report_folder = Some(richview_core::document::ReportFolder {
        root: "editor-fixture".into(),
        source: "report.rmd".into(),
        files: vec!["report.rmd".into()],
    });
    core_render(source, &options)
}
fn suggestions(source: &str) -> Value {
    let (source, cursor) = source.split_once('|').unwrap();
    let full = format!("{source}{cursor}");
    let line = source.bytes().filter(|b| *b == b'\n').count();
    let character = source.rsplit('\n').next().unwrap().encode_utf16().count();
    complete(
        &full,
        line,
        character,
        &parse(&full).unwrap(),
        &Default::default(),
    )
}

#[test]
fn formatting_preserves_literal_source_and_rendered_meaning() {
    let source = "---\r\ntitle:   Exact values\r\ncustom:   keep\r\nprolog: |\r\n  Text with  spaces.\r\n---\r\n\r\nProse  and trailing spaces.  \r\n\r\n```unknown\r\nkeep:   untouched\r\n```\r\n\r\n```table\r\n# keep comment exactly\r\ndata:\r\n    inline:\r\n        - name:   'Café 👋' # inline comment\r\n          value:   18446744073709551615\r\ncolumns:\r\n    - key:   name\r\n      label:   Name\r\n    - key:   value\r\n      align:   right\r\n```\r\n\r\n```callout\r\nkind:   hold\r\nbody: |\r\n  Code:   literal\r\n  and  spacing\r\n```\r\n";
    let formatted = format(source);
    assert_ne!(source, formatted);
    assert!(formatted.contains("title: Exact values\r\n"));
    assert!(
        formatted.contains(
            "    - name: 'Café 👋' # inline comment\r\n      value: 18446744073709551615"
        )
    );
    assert!(formatted.contains("```unknown\r\nkeep:   untouched"));
    assert!(formatted.contains("Prose  and trailing spaces.  \r\n"));
    assert!(formatted.contains("  Code:   literal\r\n  and  spacing"));
    assert_eq!(format(&formatted), formatted);
    assert_eq!(
        render(source, &Default::default()).unwrap().html,
        render(&formatted, &Default::default()).unwrap().html
    );
    let invalid = "```chart\ntype: [\n```\n";
    assert_eq!(format(invalid), invalid);
    let scalar = "```callout\nkind: hold\nbody: |\n    Preserve indentation\n```\n";
    assert_eq!(format(scalar), scalar);
    let quoted = "```callout\nkind: hold\nbody: \"multiline\n  with:   spaces\"\n```\n";
    assert_eq!(format(quoted), quoted);
    let root_sequence = "```table\n- value\n```\n";
    assert_eq!(format(root_sequence), root_sequence);
    let nested = "> ```table\n> data: {inline: [{x: 1}]}\n> ```\n";
    assert_eq!(format(nested), nested);
}

#[test]
fn suggestions_cover_report_tabs() {
    assert_eq!(suggestions("---\ntab|\n---")[0]["label"], "tabs");
    assert_eq!(suggestions("---\ntabs:\n  ex|\n---")[0]["label"], "export");
    let labels = |source| {
        suggestions(source)
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["label"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(labels("---\ntabs:\n  export: a|\n---"), ["all", "active"]);
    assert_eq!(
        labels("---\ntabs:\n  panels:\n    - id: sales\n      fr|\n---"),
        ["from"]
    );
}

#[test]
fn suggestions_follow_yaml_context_and_never_prose_or_row_payloads() {
    let source = "```chart\ntype: b|\ntitle: Claim\ndata: {inline: [{month: Jan, value: 100}]}\nencoding: {x: month, y: value}\n```";
    assert_eq!(suggestions(source)[0]["label"], "bar");
    let source = "```chart\ntype: bar\ntitle: Claim\ndata: {inline: [{month: Jan, value: 100}]}\nencoding: {x: mo|, y: value}\n```";
    assert_eq!(suggestions(source)[0]["insert_text"], "month");
    let source = "```table\ndata: {inline: [{month: Jan, value: 100}]}\ncolumns:\n  - key: month\n    al|\n```";
    assert_eq!(suggestions(source)[0]["label"], "align");
    assert_eq!(suggestions("---\nver|\n---")[0]["label"], "version");
    assert_eq!(
        suggestions("---\nstyles: [s|]\n---")[0]["label"],
        "scientific"
    );
    for source in [
        "---\nstyles: [brutalist, s|]\n---",
        "---\nstyles:\n  - s|\n---",
    ] {
        assert!(
            !suggestions(source).as_array().unwrap().is_empty(),
            "{source}"
        );
    }
    assert_eq!(suggestions("---\ndata:\n  sales:\n    in|\n---"), json!([]));
    for source in [
        "Prose the|",
        "```yaml\nthe|\n```",
        "```chart\n# type: b|\n```",
        "```callout\nkind: hold\nbody: |\n  ty|\n```",
        "```table\ndata:\n  inline:\n    - customer: the|\n```",
        "```chart\n# 🦀|\n```",
        "```chart\ntitle: 🦀|\n```",
        "```chart\ntype:\u{00a0}ba|\n```",
    ] {
        assert_eq!(suggestions(source), json!([]), "{source}");
    }
    for tag in ["chart", "table"] {
        for key in ["format", "kind", "palette", "orientation", "curve", "total"] {
            let source = format!("```{tag}\ndata:\n  inline:\n    - {key}: |\n```");
            assert_eq!(suggestions(&source), json!([]), "data field {key}");
        }
    }
    assert_eq!(
        suggestions("```chart\naxes:\n  x:\n    lo|\n```")[0]["label"],
        "locale"
    );
    assert_eq!(
        suggestions("```chart\naxes: {x: {locale: cs|}}\n```")[0]["label"],
        "cs-CZ"
    );
    let unicode = "```chart\ntype: bar\ndata: {inline: [{měsíc: Jan, hodnota: 1}]}\nencoding: {x: mě|, y: hodnota}\n```";
    assert_eq!(suggestions(unicode)[0]["insert_text"], "měsíc");
}

#[test]
fn field_completions_preserve_yaml_values_in_block_and_flow_slots() {
    for (name, block_insert, flow_insert) in [
        ("month", "month", "month"),
        ("měsíc", "měsíc", "měsíc"),
        ("yes", "yes", "yes"),
        ("007", "007", "007"),
        ("true", "\"true\"", "\"true\""),
        ("42", "\"42\"", "\"42\""),
        (".nan", "\".nan\"", "\".nan\""),
        ("a,b", "a,b", "\"a,b\""),
        ("space name", "space name", "space name"),
        ("a #b", "\"a #b\"", "\"a #b\""),
        ("a: b", "\"a: b\"", "\"a: b\""),
        (" leading", "\" leading\"", "\" leading\""),
        ("line\nbreak", "\"line\\nbreak\"", "\"line\\nbreak\""),
    ] {
        let key = serde_json::to_string(name).unwrap();
        for (encoding, expected) in [
            ("encoding:\n  x: |\n  y: fixed\n", block_insert),
            ("encoding: {x: |, y: fixed}\n", flow_insert),
        ] {
            let source =
                format!("```chart\ntype: bar\ndata: {{inline: [{{{key}: 1}}]}}\n{encoding}```\n");
            let choices = suggestions(&source);
            let choice = choices
                .as_array()
                .unwrap()
                .iter()
                .find(|item| item["label"] == name)
                .unwrap_or_else(|| panic!("missing {name:?} in {source}"));
            assert_eq!(choice["insert_text"], expected, "{name:?} in {encoding}");
            let edited = source.replacen('|', expected, 1);
            let yaml = edited
                .split_once('\n')
                .unwrap()
                .1
                .split_once("```")
                .unwrap()
                .0;
            let value = richview_core::yaml::from_str(yaml).unwrap();
            assert_eq!(value["encoding"]["x"], name, "{edited}");
            assert_eq!(value["encoding"]["y"], "fixed", "{edited}");
        }
    }
}

#[test]
fn field_completion_preserves_quoted_slots_and_sequence_paths() {
    for (slot, expected) in [("x: 'mo|'", "month"), ("x: \"mo|\"", "month")] {
        let source = format!(
            "```chart\ntype: bar\ndata: {{inline: [{{month: Jan}}]}}\nencoding: {{{slot}}}\n```\n"
        );
        let item = &suggestions(&source)[0];
        assert_eq!(item["label"], "month");
        assert_eq!(item["insert_text"], expected);
        let edited = source.replace("mo|", expected);
        let yaml = edited
            .split_once('\n')
            .unwrap()
            .1
            .split_once("```")
            .unwrap()
            .0;
        assert_eq!(
            richview_core::yaml::from_str(yaml).unwrap()["encoding"]["x"],
            "month"
        );
    }
    for (section, pointer) in [("columns", "/columns/0/key"), ("sort", "/sort/0/key")] {
        let source =
            format!("```table\ndata: {{inline: [{{\"a,b\": 1}}]}}\n{section}:\n  - key: |\n```\n");
        let choices = suggestions(&source);
        let choice = choices
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["label"] == "a,b")
            .unwrap();
        assert_eq!(choice["insert_text"], "a,b");
        let edited = source.replacen('|', "a,b", 1);
        let yaml = edited
            .split_once('\n')
            .unwrap()
            .1
            .split_once("```")
            .unwrap()
            .0;
        assert_eq!(
            richview_core::yaml::from_str(yaml)
                .unwrap()
                .pointer(pointer)
                .unwrap(),
            "a,b"
        );
    }
}

#[test]
fn supported_calendar_locale_does_not_report_an_unused_key() {
    let source = "```chart\ntype: line\ntitle: Calendar\ndata: {inline: [{date: '2024-01', value: 1}]}\nencoding: {x: date, y: value}\naxes: {x: {locale: cs-CZ}}\n```";
    let document = parse(source).unwrap();
    let issues = diagnostics(
        source,
        document.diagnostics.clone(),
        &document,
        &Default::default(),
    );
    assert_eq!(issues, json!([]));
}

#[test]
fn lint_marks_actual_nested_keys_in_flow_yaml_and_bad_bindings() {
    let source = "---\ntitle: Test\ncustom: preserved\n---\n```chart\ntype: bar\ntitle: Claim\ndata: {inline: [{month: Jan, value: 100}]}\nencoding: {x: typo, y: value, colour: value}\n```\n";
    let document = parse(source).unwrap();
    let report = render(source, &Default::default()).unwrap();
    let issues = diagnostics(source, report.diagnostics, &document, &Default::default());
    let issues = issues.as_array().unwrap();
    // The editor adds no rules (ARC-08): nothing carries an editor-only code.
    assert!(
        !issues
            .iter()
            .any(|d| d["code"].as_str().unwrap().starts_with("authoring."))
    );
    assert!(
        issues
            .iter()
            .any(|d| d["message"].as_str().unwrap().contains("custom"))
    );
    let source = "```chart\ntype: invalid\n```";
    let document = parse(source).unwrap();
    let issues = diagnostics(
        source,
        document.diagnostics.clone(),
        &document,
        &Default::default(),
    );
    assert_eq!(issues[0]["range"]["start"], json!({"line":1,"character":0}));
    let source = "```chart\ntype: bar\ndata: [\n```";
    let document = parse(source).unwrap();
    let issues = diagnostics(
        source,
        document.diagnostics.clone(),
        &document,
        &RenderOptions::default(),
    );
    assert_eq!(issues[0]["range"]["start"]["line"], 3);
}

#[test]
fn a_schema_error_reaches_the_editor_with_expected_and_fix() {
    let source = "```callout\nkind: loud\ntext: Careful\n```\n";
    let report = render(source, &Default::default()).unwrap();
    let document = parse(source).unwrap();
    let issues = diagnostics(source, report.diagnostics, &document, &Default::default());
    let issue = issues
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "block.schema")
        .expect("a schema error");
    assert!(issue["expected"].as_str().unwrap().contains("hold"));
    assert!(
        issue["fix"]
            .as_str()
            .unwrap()
            .starts_with("Set `callout.kind` to")
    );
}
