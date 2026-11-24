//! Measured style checks over the five builtins and over an author's cascade (STY-22, STY-25,
//! STY-26, STY-27, STY-52, STY-56).

use richview_core::{
    StyleLeaf, builtin_style_names, resolve_styles, style_checks, style_vocabulary,
};

fn resolved(refs: &[&str]) -> richview_core::ResolvedStyle {
    let refs: Vec<String> = refs.iter().map(|r| r.to_string()).collect();
    resolve_styles(&refs, &mut vec![])
}

#[test]
fn every_builtin_passes_every_measured_style_check() {
    let mut failures = vec![];
    for name in builtin_style_names() {
        let style = resolved(&[name]);
        for finding in style_checks::style_findings(&style) {
            failures.push(format!("{name}: {} {}", finding.code, finding.message));
        }
        for issue in style_checks::palette(&style) {
            failures.push(format!("{name}: palette {issue}"));
        }
        for issue in style_checks::ramps(&style) {
            failures.push(format!("{name}: {issue}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn an_authors_cascade_is_measured_with_the_same_floors() {
    let mut weak = resolved(&["air"]);
    weak.document["tokens"]["ink-soft"] = serde_json::json!("oklch(0.8 0 0)");
    weak.document["tokens"]["size-h2"] = serde_json::json!("20px");
    let findings = style_checks::style_findings(&weak);
    let codes: Vec<_> = findings
        .iter()
        .map(|d| (d.code.as_str(), d.path.as_str()))
        .collect();
    assert!(
        codes.contains(&("style.contrast", "tokens.ink-soft")),
        "{codes:?}"
    );
    assert!(
        codes.contains(&("style.type-scale", "tokens.size-h2")),
        "{codes:?}"
    );
    assert!(
        style_checks::style_findings(&resolved(&["air"])).is_empty(),
        "a builtin alone has no findings"
    );
    let drifting = resolved(&["air", "fonts:\n  text: 'Nonesuch Sans, serif'\n"]);
    assert!(
        style_checks::style_findings(&drifting)
            .iter()
            .any(|d| d.code == "style.font-missing" && d.path == "tokens.font-text")
    );
}

#[test]
fn a_weak_palette_and_equal_arms_are_named() {
    let weak = resolved(&[
        "air",
        "palette: ['#1B6F9E', '#2277AA', '#3388BB', '#99BBDD']\n",
    ]);
    assert!(!style_checks::palette(&weak).is_empty());
    // Two arms the same colour differ by nothing: the diverging rule names it.
    let mut same = resolved(&["air"]);
    same.chart["negative-ramp"] = same.chart["ramp"].clone();
    assert!(
        style_checks::ramps(&same)
            .iter()
            .any(|i| i.contains("arms"))
    );
}

#[test]
fn the_vocabulary_lists_each_leaf_once_with_its_mark() {
    let leaves = style_vocabulary();
    let mut paths: Vec<&str> = leaves
        .iter()
        .map(|leaf: &StyleLeaf| leaf.path.as_str())
        .collect();
    let total = paths.len();
    paths.sort_unstable();
    paths.dedup();
    assert_eq!(paths.len(), total, "a leaf is listed twice");
}

fn findings(source: &str, styles: &[&str]) -> Vec<richview_core::document::Diagnostic> {
    let options = richview_core::document::RenderOptions {
        styles: styles.iter().map(|s| s.to_string()).collect(),
        report_folder: Some(richview_core::document::ReportFolder {
            root: "glyph-fixture".into(),
            source: "report.rmd".into(),
            files: std::iter::once("report.rmd".into()).collect(),
        }),
        ..Default::default()
    };
    richview_core::document::render(source, &options)
        .map(|report| report.diagnostics)
        .unwrap_or_default()
}

fn glyph_warnings(source: &str, styles: &[&str]) -> Vec<richview_core::document::Diagnostic> {
    findings(source, styles)
        .into_iter()
        .filter(|d| d.code == "style.glyph-missing")
        .collect()
}

#[test]
fn following_the_sub_and_superscript_advice_never_warns_in_any_builtin() {
    let source = "Water is H<sub>2</sub>O.\n\n```callout\nkind: neutral\nbody: \"Ice is H<sub>2</sub>O too.\"\n```\n";
    for name in builtin_style_names() {
        let advice: Vec<String> = findings(source, &[name])
            .into_iter()
            .filter(|d| d.code == "security.html")
            .filter_map(|d| d.fix)
            .collect();
        assert_eq!(
            advice.len(),
            2,
            "{name}: the body and the callout each warn"
        );
        assert_eq!(advice[0], advice[1], "{name}: one advice for the report");
        // The advice itself, written as the report's text, is set in the style's own faces.
        let warned = glyph_warnings(&format!("{}\n", advice[0]), &[name]);
        assert!(warned.is_empty(), "{name}: {} gives {warned:?}", advice[0]);
        // One rule in every style: Newsreader and Patrick Hand draw the subscript digits they
        // borrow (`scripts/fonts/README.md`).
        assert!(advice[0].contains("CO₂"), "{name}: {}", advice[0]);
    }
}

#[test]
fn exponents_and_ion_charges_never_warn_in_any_builtin() {
    // Every face draws ⁺ ⁻ ₊ ₋, its own or lent by RichView Sans (`scripts/fonts/README.md`), so
    // headings and text set 10⁻³ and SO₄²⁻ in the style's own faces.
    let source = "# Rates of 10⁻³ and 10⁺⁶\n\nSO₄²⁻ and NH₄⁺ ions, and a step of ₋₁ against ₊₂.\n";
    for name in builtin_style_names() {
        let warned = glyph_warnings(source, &[name]);
        assert!(warned.is_empty(), "{name}: {warned:?}");
    }
}

#[test]
fn characters_outside_the_embedded_faces_warn_once_for_the_report_with_the_first_line() {
    let source = "# Title\n\nPlain text with \u{1F600} here.\n\nAgain \u{1F600} and \u{6F22}.\n\n`\u{1F4A9}` and\n\n```\n\u{1F680}\n```\n";
    let found = glyph_warnings(source, &["air"]);
    let messages: Vec<_> = found.iter().map(|d| d.message.clone()).collect();
    assert_eq!(found.len(), 1, "{messages:?}");
    assert!(
        found[0].message.starts_with("2 character(s)"),
        "{messages:?}"
    );
    assert!(found[0].message.contains("U+1F600") && found[0].message.contains("U+6F22"));
    assert_eq!(found[0].line, Some(3));
    assert_eq!(found[0].level, "warning");
}

#[test]
fn the_example_reports_use_only_embedded_characters_in_every_builtin() {
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut failures = vec![];
    for entry in std::fs::read_dir(examples).unwrap() {
        let report = entry.unwrap().path().join("report.rmd");
        let Ok(source) = std::fs::read_to_string(&report) else {
            continue;
        };
        for name in builtin_style_names() {
            for d in glyph_warnings(&source, &[name]) {
                failures.push(format!(
                    "{} [{name}] line {:?}: {}",
                    report.display(),
                    d.line,
                    d.message
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_showcase_studio_themes_load_clean_over_a_builtin_and_pass_the_style_checks() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/showcase/themes");
    let mut failures = vec![];
    let mut count = 0;
    for entry in std::fs::read_dir(&dir).unwrap().flatten() {
        let path = entry.path();
        if !path.to_string_lossy().ends_with(".rstyle.yaml") {
            continue;
        }
        count += 1;
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let source = std::fs::read_to_string(&path).unwrap();
        let mut diagnostics = vec![];
        let style = resolve_styles(&[source], &mut diagnostics);
        for d in diagnostics.into_iter().filter(|d| d.level != "info") {
            failures.push(format!("{name}: {} {} ({})", d.code, d.message, d.path));
        }
        for d in style_checks::contrast(&style) {
            failures.push(format!("{name}: {} {}", d.code, d.message));
        }
    }
    assert!(count >= 20, "expected the 20 studio themes, found {count}");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn brand_report(files: &[(&str, &str)]) -> Vec<richview_core::document::Diagnostic> {
    let style = "fonts:\n  text: {family: 'Acme Sans', file: fonts/acme.woff2}\n";
    let options = richview_core::document::RenderOptions {
        styles: vec!["air".into(), style.into()],
        data: files
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        report_folder: Some(richview_core::document::ReportFolder {
            root: "brand-fixture".into(),
            source: "report.rmd".into(),
            files: std::iter::once("report.rmd".to_owned())
                .chain(files.iter().map(|(k, _)| k.to_string()))
                .collect(),
        }),
        ..Default::default()
    };
    richview_core::document::render("# T\n", &options)
        .unwrap()
        .diagnostics
}

#[test]
fn a_brand_font_counts_only_once_its_file_is_in_the_report_folder() {
    let missing = brand_report(&[]);
    assert!(
        missing
            .iter()
            .any(|d| d.code == "style.font-missing" && d.path == "tokens.font-text"),
        "{missing:?}"
    );
    let present = brand_report(&[("fonts/acme.woff2", "data:font/woff2;base64,d09GMgAAAA==")]);
    assert!(
        !present.iter().any(|d| d.code == "style.font-missing"),
        "{present:?}"
    );
}

#[test]
fn an_accent_in_a_style_file_without_tokens_does_not_panic() {
    let options = richview_core::document::RenderOptions {
        styles: vec![
            "scientific".into(),
            "accent: \"oklch(0.5 0.2 30)\"\n".into(),
        ],
        report_folder: Some(richview_core::document::ReportFolder {
            root: "x".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }),
        ..Default::default()
    };
    let report = richview_core::document::render("# T\n", &options).unwrap();
    assert!(report.diagnostics.iter().any(|d| d.code == "style.retired"));
}

#[test]
fn a_hostile_brand_font_payload_or_family_never_reaches_the_page() {
    for payload in [
        "data:x;base64,A) }</style><meta http-equiv=refresh content=0;url=https://evil.example>",
        "data:font/woff2;base64,d09GMg) }</style><b>",
        "data:font/woff2;base64,AAAA",
        "data:font/woff2,d09GMgAAAA==",
    ] {
        let style = "fonts:\n  text: {family: 'Acme Sans', file: fonts/acme.woff2}\n";
        let options = richview_core::document::RenderOptions {
            styles: vec!["air".into(), style.into()],
            data: [("fonts/acme.woff2".to_owned(), payload.to_owned())].into(),
            report_folder: Some(richview_core::document::ReportFolder {
                root: "hostile".into(),
                source: "report.rmd".into(),
                files: vec!["report.rmd".into(), "fonts/acme.woff2".into()],
            }),
            ..Default::default()
        };
        let report = richview_core::document::render("# T\n", &options).unwrap();
        assert!(!report.html.contains("evil.example"), "{payload}");
        assert!(
            !report.html.contains("font-family: \"Acme Sans\""),
            "{payload}"
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "style.font-missing"),
            "{payload}"
        );
    }
    let style = "fonts:\n  text: {family: 'Acme\"; }</style>', file: fonts/acme.woff2}\n";
    let options = richview_core::document::RenderOptions {
        styles: vec!["air".into(), style.into()],
        data: [(
            "fonts/acme.woff2".to_owned(),
            "data:font/woff2;base64,d09GMgAAAA==".to_owned(),
        )]
        .into(),
        report_folder: Some(richview_core::document::ReportFolder {
            root: "hostile".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into(), "fonts/acme.woff2".into()],
        }),
        ..Default::default()
    };
    let html = richview_core::document::render("# T\n", &options)
        .map(|r| r.html)
        .unwrap_or_default();
    assert!(!html.contains("Acme\"; }"));
}

#[test]
fn a_retired_pale_link_cannot_change_print_contrast() {
    let tweaks = "tokens:\n  link: \"oklch(0.85 0.12 250)\"\n".to_owned();
    let options = richview_core::document::RenderOptions {
        styles: vec!["sleek".into(), tweaks],
        report_folder: Some(richview_core::document::ReportFolder {
            root: "paper".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }),
        ..Default::default()
    };
    let report = richview_core::document::render("# T\n", &options).unwrap();
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.code == "style.contrast" && d.message.starts_with("On paper:"))
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "style.retired" && d.path.ends_with("tokens.link")),
        "{:?}",
        report.diagnostics
    );
}

#[test]
fn title_description_and_block_text_are_scanned_for_missing_glyphs() {
    let source = "---\ntitle: Mluva \u{1F600}\ndescription: \u{6F22}\n---\n\n```callout\nkind: hold\ntitle: Read\nbody: \u{1F680}\n```\n";
    let found = glyph_warnings(source, &["air"]);
    assert_eq!(found.len(), 1);
    assert!(
        found[0].message.starts_with("3 character(s)"),
        "{}",
        found[0].message
    );
}

#[test]
fn a_brand_font_shared_by_two_roles_is_embedded_once_with_its_weight_and_a_notice() {
    let style = "fonts:\n  text: {family: 'Acme Sans', file: fonts/acme.woff2, weight: 500}\n  ui: {family: 'Acme Sans', file: fonts/acme.woff2, weight: 500}\n";
    let options = richview_core::document::RenderOptions {
        styles: vec!["air".into(), style.into()],
        data: [(
            "fonts/acme.woff2".to_owned(),
            "data:font/woff2;base64,d09GMgAAAA==".to_owned(),
        )]
        .into(),
        report_folder: Some(richview_core::document::ReportFolder {
            root: "brand".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into(), "fonts/acme.woff2".into()],
        }),
        ..Default::default()
    };
    let html = richview_core::document::render("# T\n", &options)
        .unwrap()
        .html;
    assert_eq!(
        html.matches("font-family: \"Acme Sans\"; font-style: normal; font-weight: 500")
            .count(),
        1
    );
    assert!(html.contains("Supplied with this report by its author"));
}

fn brand_html(payload: &str) -> richview_core::document::RenderedReport {
    let style = "fonts:\n  text: {family: 'Acme Sans', file: fonts/acme.woff2}\n";
    let options = richview_core::document::RenderOptions {
        styles: vec!["air".into(), style.into()],
        data: [("fonts/acme.woff2".to_owned(), payload.to_owned())].into(),
        report_folder: Some(richview_core::document::ReportFolder {
            root: "brand".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into(), "fonts/acme.woff2".into()],
        }),
        ..Default::default()
    };
    richview_core::document::render("# Quadratic\n\nQuiet text.\n", &options).unwrap()
}

#[test]
fn an_embedded_brand_font_does_not_make_the_glyph_check_cry_wolf() {
    let report = brand_html("data:font/woff2;base64,d09GMgAAAAAAAAAA");
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.code == "style.glyph-missing"),
        "{:?}",
        report.diagnostics
    );
}

#[test]
fn woff2_files_of_every_flavour_embed_and_a_refusal_names_its_own_reason() {
    for flavour in ["d09GMk9UVE8AAAAA", "d09GMnRydWUAAAAA", "d09GMgABAAAAAAAA"] {
        let report = brand_html(&format!("data:font/woff2;base64,{flavour}"));
        assert!(
            !report
                .diagnostics
                .iter()
                .any(|d| d.code == "style.font-missing"),
            "{flavour}: {:?}",
            report.diagnostics
        );
    }
    let refused = brand_html("data:font/woff2;base64,AAAAAAAAAAAA");
    let message = refused
        .diagnostics
        .iter()
        .find(|d| d.code == "style.font-missing")
        .map(|d| d.message.clone())
        .unwrap();
    assert!(
        message.contains("is not a WOFF2 file") && !message.contains("report folder"),
        "{message}"
    );
}
