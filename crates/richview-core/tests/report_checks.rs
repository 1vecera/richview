//! Author guidance from the report-feature cards is returned by check, outside the reader's report.
use richview_core::document::{
    Diagnostic, RenderOptions, ReportFolder, check, check_document, parse, render,
};

fn options() -> RenderOptions {
    RenderOptions {
        report_folder: Some(ReportFolder {
            root: "report-checks".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }),
        ..Default::default()
    }
}

fn guidance(source: &str, code: &str) -> Vec<Diagnostic> {
    check(source, &options())
        .unwrap()
        .diagnostics
        .into_iter()
        .filter(|d| d.code == code)
        .collect()
}

const TABLE: &str = "```table\ndata: {inline: [{name: Alpha, n: 1}]}\n```\n";
const CHART: &str = "```chart\ntype: bar\ntitle: Sales\nencoding: {x: name, y: n}\ndata: {inline: [{name: Alpha, n: 1}]}\n```\n";

#[test]
fn checks_require_source_notes_only_for_charts_and_tables_and_use_the_rendered_note_rule() {
    let source = format!("{CHART}\n{TABLE}\n```metric\nvalue: 1\n```\n");
    let missing = guidance(&source, "block.source");
    assert_eq!(missing.len(), 2);
    assert!(
        missing
            .iter()
            .all(|d| d.level == "warning" && d.line.is_some() && d.fix.is_some())
    );
    assert_eq!(
        missing.iter().map(|d| d.tag.as_deref()).collect::<Vec<_>>(),
        [Some("chart"), Some("table")]
    );
    for note in [
        "*Source: data.csv*",
        "*Sources: [Study](https://example.org).*",
        "_Note: Synthetic data._",
        "*Notes: estimates.*",
    ] {
        assert!(
            guidance(&format!("{CHART}\n{note}\n"), "block.source").is_empty(),
            "{note}"
        );
    }
    let forged = format!("{CHART}\nProse.\n\n<!--richview:block:0-->\n\n*Source: unrelated.*\n");
    assert_eq!(
        guidance(&forged, "block.source").len(),
        1,
        "an authored marker must not supply a generated block's note"
    );
    for note in [
        "Source: plain",
        "**Source: strong**",
        "*Source: partial* then prose",
        "Prose.\n\n*Source: later*",
    ] {
        assert_eq!(
            guidance(&format!("{CHART}\n{note}\n"), "block.source").len(),
            1,
            "{note}"
        );
    }
    let generated = render(&source, &options()).unwrap();
    assert!(
        !generated
            .diagnostics
            .iter()
            .any(|d| d.code == "block.source")
    );
    assert!(!generated.html.contains("Add a source note"));
    let parsed = check_document(parse(&source).unwrap(), &options()).unwrap();
    let checked = check(&source, &options()).unwrap();
    assert_eq!(parsed.diagnostics, checked.diagnostics);
}

#[test]
fn checks_note_unnamed_tables_and_markdown_in_inline_cells_without_rewriting_them() {
    let source = "```table\ndata: {inline: [{note: '**Literal**', link: '[Label](https://example.org)', id: snake_case, flag: true}]}\n```\n";
    let names = guidance(source, "table.title");
    assert_eq!(names.len(), 1);
    assert_eq!(names[0].level, "info");
    let cells = guidance(source, "table.markdown");
    assert_eq!(cells.len(), 2);
    assert!(cells.iter().all(|d| d.level == "info"
        && d.path.starts_with("table.data.inline[0].")
        && d.line.is_some()));
    let html = render(source, &options()).unwrap().html;
    assert!(html.contains("**Literal**") && html.contains("[Label](https://example.org)"));
    assert!(!html.contains("<strong>Literal</strong>") && html.contains("true"));
    assert!(
        guidance(
            "```table\ntitle: Named\ndata: {inline: [{id: snake_case}]}\n```\n",
            "table.title"
        )
        .is_empty()
    );
}

#[test]
fn checks_note_missing_video_captions_and_media_transcripts_without_blocking_render() {
    let source = "---\nlang: cs-CZ\n---\n\n```audio\nsrc: data:audio/mpeg;base64,AA==\ntitle: Audio\n```\n\n```video\nsrc: data:video/mp4;base64,AA==\ntitle: Video\n```\n";
    let captions = guidance(source, "media.captions");
    assert_eq!(captions.len(), 1);
    assert_eq!(captions[0].tag.as_deref(), Some("video"));
    let transcripts = guidance(source, "media.transcript");
    assert_eq!(transcripts.len(), 2);
    assert!(
        captions
            .iter()
            .chain(&transcripts)
            .all(|d| d.level == "info" && d.line.is_some())
    );
    let report = render(source, &options()).unwrap();
    assert!(report.html.contains("<audio") && report.html.contains("<video"));
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.code == "media.transcript" || d.code == "media.captions")
    );
    let mut complete = options();
    for (name, content) in [
        ("c.vtt", "WEBVTT\n\n00:00.000 --> 00:01.000\nHello\n"),
        ("t.md", "Spoken words."),
    ] {
        complete.data.insert(name.into(), content.into());
        complete
            .report_folder
            .as_mut()
            .unwrap()
            .files
            .push(name.into());
    }
    let supplied = source
        .replace("title: Audio", "title: Audio\ntranscript: t.md")
        .replace(
            "title: Video",
            "title: Video\ncaptions: c.vtt\ntranscript: t.md",
        );
    assert!(
        !check(&supplied, &complete)
            .unwrap()
            .diagnostics
            .iter()
            .any(|d| d.code == "media.transcript" || d.code == "media.captions")
    );
    let html = render(&supplied, &complete).unwrap().html;
    assert!(html.contains("srclang=\"cs-CZ\""));
    assert!(html.contains("<summary>Transcript</summary>") && html.contains("label=\"Captions\""));
}

#[test]
fn rich_fences_inside_lists_quotes_and_callouts_fail_in_place() {
    for source in [
        "- Item\n\n  ```metric\n  value: 1\n  ```\n\nAfter.\n",
        "> ```metric\n> value: 1\n> ```\n\nAfter.\n",
        "````callout\nkind: neutral\nbody: |\n  ```metric\n  value: 1\n  ```\n````\n\nAfter.\n",
    ] {
        let report = render(source, &options()).unwrap();
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "block.nested" && d.level == "error" && d.line.is_some()),
            "{:?}",
            report.diagnostics
        );
        assert!(report.html.contains("not rendered:") && report.html.contains("After."));
        assert!(report.html.contains("value: 1"));
    }
}

#[test]
fn near_miss_rich_tags_warn_but_stay_code_and_unrelated_languages_stay_silent() {
    let source = "```tabel\nname: Value\n```\n\n```mermaid\ngraph TD;\n```\n";
    let report = render(source, &options()).unwrap();
    let warnings: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.code == "block.unknown-tag")
        .collect();
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].message.contains("table"));
    assert_eq!(warnings[0].line, Some(1));
    assert!(report.html.contains("name: Value") && report.html.contains("graph TD;"));
}

#[test]
fn checks_note_header_images_without_a_caption_or_credit() {
    let source =
        "---\ntitle_image: {src: 'data:image/png;base64,AA==', alt: Illustration}\n---\n\nBody.\n";
    let notes = guidance(source, "image.context");
    assert_eq!(notes.len(), 1);
    assert_eq!(
        (
            notes[0].level.as_str(),
            notes[0].path.as_str(),
            notes[0].line
        ),
        ("info", "frontmatter.title_image", Some(2))
    );
    let captioned = source.replace(
        "alt: Illustration",
        "alt: Illustration, caption: Illustrative photograph",
    );
    assert!(guidance(&captioned, "image.context").is_empty());
}

fn author_finding(source: &str, code: &str) -> Diagnostic {
    check(source, &options())
        .unwrap()
        .diagnostics
        .into_iter()
        .find(|d| d.code == code)
        .unwrap_or_else(|| panic!("missing {code} for {source}"))
}

#[test]
fn author_chart_type_typos_offer_the_nearest_registered_type() {
    let finding = author_finding(&CHART.replace("type: bar", "type: barr"), "block.schema");
    assert_eq!(
        (finding.path.as_str(), finding.line),
        ("chart.type", Some(2))
    );
    assert!(finding.message.contains("barr") && finding.message.contains("did you mean 'bar'"));
    let fix = finding.fix.unwrap();
    assert!(fix.contains("chart.type") && fix.contains("`bar`"), "{fix}");
}

#[test]
fn author_frontmatter_date_and_version_repairs_are_copyable_yaml() {
    for (field, value, code) in [
        ("date", "30/10/2026", "frontmatter.date"),
        ("version", r#"\"1.0.0\""#, "frontmatter.version"),
    ] {
        let finding = author_finding(&format!("---\n{field}: {value}\n---\n\nBody.\n"), code);
        assert_eq!(finding.path, format!("frontmatter.{field}"));
        assert_eq!(finding.line, Some(2));
        let fix = finding
            .fix
            .unwrap_or_else(|| panic!("{code} needs a correction"));
        let example = fix
            .split('`')
            .skip(1)
            .step_by(2)
            .find(|s| s.starts_with(&format!("{field}:")))
            .unwrap_or_else(|| panic!("no copyable {field} in {fix}"));
        let repaired = format!("---\n{example}\n---\n\nBody.\n");
        assert!(
            !check(&repaired, &options())
                .unwrap()
                .diagnostics
                .iter()
                .any(|d| d.code == code)
        );
        assert!(parse(&repaired).unwrap().frontmatter[field].is_string());
    }
}

#[test]
fn author_unknown_style_tokens_suggest_a_sibling_key_and_a_fix() {
    let mut opts = options();
    opts.styles = vec!["tokens:\n  accnet: '#225588'\n".into()];
    let finding = check("# Draft", &opts)
        .unwrap()
        .diagnostics
        .into_iter()
        .find(|d| d.code == "style.unknown-key")
        .unwrap();
    assert_eq!(finding.path, "styles[0].tokens.accnet");
    assert_eq!((finding.line, finding.column), (Some(2), Some(3)));
    assert!(
        finding.message.contains("tokens.accent"),
        "{}",
        finding.message
    );
    let fix = finding.fix.unwrap();
    assert!(
        fix.contains("tokens.accnet") && fix.contains("tokens.accent"),
        "{fix}"
    );
}

#[test]
fn author_misplaced_known_fields_name_the_correct_parent() {
    let source = "```table\ntitle: Sales\ndata:\n  inline: [{name: Alpha, n: 1}]\n  max_rows: 5\n```\n\n*Source: synthetic data.*\n";
    let finding = author_finding(source, "block.unknown-key");
    assert_eq!(
        (finding.path.as_str(), finding.line),
        ("table.data.max_rows", Some(5))
    );
    let fix = finding.fix.unwrap();
    assert!(
        fix.contains("Move") && fix.contains("table.max_rows"),
        "{fix}"
    );
    let repaired = source.replace("  max_rows: 5", "max_rows: 5");
    assert!(
        !check(&repaired, &options())
            .unwrap()
            .diagnostics
            .iter()
            .any(|d| d.code == "block.unknown-key")
    );
}

#[test]
fn author_axis_and_column_titles_are_repaired_to_their_label_never_the_block_title() {
    // An axis `title` is the axis label. Following a fix that moves it to `chart.title` would
    // overwrite the chart's claim title (candidate/2 review).
    let both = CHART.replace(
        "encoding: {x: name, y: n}",
        "encoding: {x: name, y: n}\naxes: {y: {label: Visits, title: X}}",
    );
    let finding = author_finding(&both, "block.unknown-key");
    assert_eq!(finding.path, "chart.axes.y.title");
    let fix = finding.fix.unwrap();
    assert!(
        fix.contains("chart.axes.y.label") && !fix.contains("`chart.title`"),
        "{fix}"
    );
    let only_title = CHART.replace(
        "encoding: {x: name, y: n}",
        "encoding: {x: name, y: n}\naxes: {y: {title: Visits}}",
    );
    let finding = author_finding(&only_title, "block.unknown-key");
    assert_eq!(
        finding.fix.as_deref(),
        Some("Rename 'title' to 'label', or remove it.")
    );
    let repaired = only_title.replace("{title: Visits}", "{label: Visits}");
    assert!(
        !check(&repaired, &options())
            .unwrap()
            .diagnostics
            .iter()
            .any(|d| d.code == "block.unknown-key")
    );
    let column = "```table\ntitle: Sales\ndata: {inline: [{n: 1}]}\ncolumns: [{key: n, title: Count}]\n```\n";
    let fix = author_finding(column, "block.unknown-key").fix.unwrap();
    assert_eq!(fix, "Rename 'title' to 'label', or remove it.");
}

#[test]
fn author_a_nested_key_is_never_moved_onto_a_top_level_key_already_set() {
    let source = "```table\ntitle: Sales\nmax_rows: 3\ndata:\n  inline: [{name: Alpha, n: 1}]\n  max_rows: 5\n```\n";
    let finding = author_finding(source, "block.unknown-key");
    assert_eq!(finding.path, "table.data.max_rows");
    let fix = finding.fix.unwrap();
    assert!(!fix.contains("Move"), "{fix}");
    assert!(
        !finding.message.contains("belongs at"),
        "{}",
        finding.message
    );
}

#[test]
fn author_schema_repairs_describe_the_real_column_image_and_ratio_grammar() {
    for (source, path, word) in [
        (
            "```table\ntitle: Sales\ndata: {inline: [{n: 1}]}\ncolumns: [{label: Sales}]\n```\n"
                .to_owned(),
            "table.columns[0].key",
            "column",
        ),
        (
            "```image\nalt: Sales\n```\n".to_owned(),
            "image.src",
            "path",
        ),
        (
            CHART.replace("title: Sales", "title: Sales\nstyle: {ratio: 0.1}"),
            "chart.style.ratio",
            "0.5",
        ),
    ] {
        let finding = author_finding(&source, "block.schema");
        assert_eq!(finding.path, path);
        let fix = finding.fix.unwrap();
        assert!(fix.contains(path) && fix.contains(word), "{path}: {fix}");
        if path.ends_with("ratio") {
            assert!(finding.expected.unwrap().contains("6"));
            assert!(fix.contains("6"));
        }
    }
}

#[test]
fn author_late_field_and_inline_cell_findings_keep_the_authored_position() {
    let source = "# Revenue\n\n```chart\ntype: bar\ntitle: Revenue\nencoding: {x: region, y: revenue}\ndata: {inline: [{region: North, revenue: 120}]}\ncontrols: [{kind: filter, id: region, field: regoin, options: [North]}]\n```\n\n*Source: synthetic revenue.*\n";
    for (source, code, path, line, correction) in [
        (
            source.replace("y: revenue", "y: sales"),
            "block.field",
            "chart.encoding.y",
            6,
            "revenue",
        ),
        (
            source.to_owned(),
            "block.field",
            "chart.controls[0].field",
            8,
            "region",
        ),
        (
            source
                .replace("revenue: 120", "revenue: '1,234'")
                .replace("field: regoin", "field: region"),
            "chart.non-numeric",
            "rows[0].revenue",
            7,
            "number",
        ),
    ] {
        let finding = author_finding(&source, code);
        assert_eq!((finding.path.as_str(), finding.line), (path, Some(line)));
        assert!(finding.column.is_some());
        assert!(finding.fix.as_deref().unwrap().contains(correction));
        let worker = check_document(
            serde_json::from_slice(&serde_json::to_vec(&parse(&source).unwrap()).unwrap()).unwrap(),
            &options(),
        )
        .unwrap();
        assert!(worker.diagnostics.contains(&finding));
    }
}

#[test]
fn author_missing_data_assets_point_at_each_referring_block_field() {
    use richview_core::document::validate_report_document;
    let source = "# Revenue\n\n```chart\ntype: bar\ntitle: Revenue\nencoding: {x: region, y: revenue}\ndata: {csv: absent.csv}\n```\n\n```table\ntitle: Revenue\ndata: {csv: absent.csv}\n```\n";
    let document = parse(source).unwrap();
    let mut opts = options();
    let private_root = "/authors/private/revenue-report";
    opts.report_folder.as_mut().unwrap().root = private_root.into();
    let findings = validate_report_document(&document, opts.report_folder.as_ref()).unwrap_err();
    let missing: Vec<_> = findings
        .iter()
        .filter(|d| d.code == "report.asset.missing")
        .collect();
    assert_eq!(missing.len(), 2);
    for (finding, path, line) in [
        (missing[0], "chart.data.csv", 7),
        (missing[1], "table.data.csv", 12),
    ] {
        assert_eq!((finding.path.as_str(), finding.line), (path, Some(line)));
        assert!(finding.column.is_some());
        assert!(finding.message.contains("absent.csv") && finding.fix.is_some());
        assert!(
            !finding.message.contains(private_root),
            "{}",
            finding.message
        );
    }
}

#[test]
fn author_yaml_syntax_repairs_name_the_block_and_indentation() {
    let source = CHART.replace(
        "encoding: {x: name, y: n}",
        "encoding:\n  x: name\n    y: n",
    );
    let finding = author_finding(&source, "block.yaml");
    assert_eq!((finding.path.as_str(), finding.line), ("chart", Some(6)));
    assert!(finding.message.contains("chart") && finding.message.contains("YAML"));
    let fix = finding.fix.unwrap().to_lowercase();
    assert!(fix.contains("indent") && fix.contains("sibling"), "{fix}");
}

#[test]
fn author_missing_tab_anchors_offer_a_heading_that_repairs_the_panel() {
    let source =
        "---\ntabs:\n  panels: [{id: summary, label: Summary, from: missing}]\n---\n\nBody.\n";
    let finding = author_finding(source, "tabs.anchor-missing");
    assert_eq!(
        (finding.path.as_str(), finding.line),
        ("frontmatter.tabs.panels[0].from", Some(3))
    );
    assert!(finding.fix.as_deref().unwrap().contains("{#missing}"));
    let repaired = format!("{source}\n### Summary {{#missing}}\n\nPanel.\n");
    assert!(
        !check(&repaired, &options())
            .unwrap()
            .diagnostics
            .iter()
            .any(|d| d.code == "tabs.anchor-missing")
    );
}

#[test]
fn author_multiple_data_sources_explain_which_choice_to_remove() {
    let source = CHART.replace("data: {inline:", "data: {csv: revenue.csv, inline:");
    let finding = parse(&source)
        .unwrap()
        .diagnostics
        .into_iter()
        .find(|d| d.code == "data.local")
        .unwrap();
    let fix = finding.fix.unwrap();
    assert!(
        fix.contains("inline") && fix.contains("csv") && fix.contains("remove"),
        "{fix}"
    );
}

#[test]
fn learn_never_offers_a_chart_channel_that_check_refuses() {
    // `learn` lists each type's channels from the registry; `check`'s schema rules must accept
    // every one of them on that type (data rules, such as a radar's three spokes, may still
    // refuse this one-row data). A bullet once listed `lower` and `upper`, which only interval,
    // dumbbell and band read (candidate/2 review).
    for chart in richview_core::registry::CHART_TYPES {
        for channel in chart.encodings {
            let mut channels: Vec<&str> = chart.required.to_vec();
            if !channels.contains(channel) {
                channels.push(channel);
            }
            let listed = |c: &str| {
                richview_core::registry::tag("chart")
                    .unwrap()
                    .fields
                    .iter()
                    .any(|f| {
                        f.path == format!("encoding.{c}")
                            && matches!(f.ty, richview_core::registry::Ty::List(_))
                    })
            };
            let encoding: Vec<String> = channels
                .iter()
                .map(|c| {
                    if listed(c) {
                        format!("{c}: [{c}]")
                    } else {
                        format!("{c}: {c}")
                    }
                })
                .collect();
            let row: Vec<String> = channels.iter().map(|c| format!("{c}: 1")).collect();
            let source = format!(
                "```chart\ntype: {}\ntitle: T\nencoding: {{{}}}\ndata: {{inline: [{{{}}}]}}\n```\n",
                chart.name,
                encoding.join(", "),
                row.join(", ")
            );
            let path = format!("chart.encoding.{channel}");
            let refused: Vec<String> = check(&source, &options())
                .unwrap()
                .diagnostics
                .into_iter()
                .filter(|d| d.code == "block.schema" && d.path == path)
                .map(|d| d.message)
                .collect();
            assert!(
                refused.is_empty(),
                "{} lists {channel}, but check refuses it: {refused:?}",
                chart.name
            );
        }
    }
}

#[test]
fn author_a_misspelt_style_name_names_the_nearest_builtin_once() {
    let source = "---\ntitle: T\nstyles: [slick]\n---\n\n# Hi\n\nText.\n";
    let findings: Vec<Diagnostic> = check(source, &options())
        .unwrap()
        .diagnostics
        .into_iter()
        .filter(|d| d.level == "error")
        .collect();
    assert_eq!(findings.len(), 1, "{findings:?}");
    let finding = &findings[0];
    assert_eq!(finding.code, "style.unknown");
    assert!(
        finding.message.contains("did you mean 'sleek'"),
        "{}",
        finding.message
    );
    assert!(
        finding
            .fix
            .as_deref()
            .is_some_and(|fix| fix.contains("`sleek`")),
        "{:?}",
        finding.fix
    );
    // A style file stays an asset: a missing one is still reported as absent.
    let file = source.replace("[slick]", "[brand.rstyle.yaml]");
    assert!(
        check(&file, &options())
            .unwrap()
            .diagnostics
            .iter()
            .any(|d| d.code == "report.asset.missing")
    );
}

#[test]
fn author_every_digits_field_takes_2_to_6_significant_digits_once() {
    // One precision rule for charts, axes, table columns and metrics (TYP-23, FEA-CH-37).
    let metric = "```metric\nlabel: Visits\nvalue: 1234\ndigits: 12\n```\n";
    let axis = CHART.replace(
        "encoding: {x: name, y: n}",
        "encoding: {x: name, y: n}\naxes: {y: {digits: 9}}",
    );
    let chart = CHART.replace("title: Sales", "title: Sales\ndigits: 9");
    let column =
        "```table\ntitle: Sales\ndata: {inline: [{n: 1}]}\ncolumns: [{key: n, digits: 1}]\n```\n";
    for (source, path) in [
        (metric.to_owned(), "metric.digits"),
        (axis, "chart.axes.y.digits"),
        (chart, "chart.digits"),
        (column.to_owned(), "table.columns[0].digits"),
    ] {
        let found: Vec<Diagnostic> = check(&source, &options())
            .unwrap()
            .diagnostics
            .into_iter()
            .filter(|d| d.path == path)
            .collect();
        assert_eq!(found.len(), 1, "{path}: {found:?}");
        assert_eq!(found[0].code, "block.schema", "{path}");
        assert!(
            found[0].message.contains("from 2 to 6"),
            "{}",
            found[0].message
        );
    }
    let fine = "```metric\nlabel: Visits\nvalue: 1234\ndigits: 4\n```\n";
    assert!(
        !check(fine, &options())
            .unwrap()
            .diagnostics
            .iter()
            .any(|d| d.path == "metric.digits")
    );
}

#[test]
fn an_unknown_style_lists_the_builtins_in_its_expected_field() {
    // Tools read the builtin names from this field, never from the message (golden.mjs).
    let source = "---\ntitle: T\nstyles: [no-such-style]\n---\n\nText.\n";
    let finding = author_finding(source, "style.unknown");
    assert_eq!(
        finding.expected.as_deref(),
        Some(richview_core::builtin_style_names().join(", ").as_str())
    );
}
