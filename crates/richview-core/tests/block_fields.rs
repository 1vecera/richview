//! The field rules of the block cards, each at the public boundary (TYP-05, TYP-08, TYP-09, TYP-14,
//! TYP-19, TYP-29, TYP-30, TYP-32, TYP-35, TYP-78). Every case asserts the code and the path; the
//! message is only pinned where it names the repair.

use richview_core::document::{Diagnostic, RenderOptions, ReportFolder, check, parse, render};

fn found(source: &str) -> Vec<Diagnostic> {
    parse(source).unwrap().diagnostics
}

fn has<'a>(diagnostics: &'a [Diagnostic], code: &str, path: &str) -> &'a Diagnostic {
    diagnostics
        .iter()
        .find(|d| d.code == code && d.path == path)
        .unwrap_or_else(|| panic!("no {code} at {path}: {diagnostics:#?}"))
}

fn options() -> RenderOptions {
    RenderOptions {
        report_folder: Some(ReportFolder {
            root: "block-fields".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }),
        ..Default::default()
    }
}

#[test]
fn a_retired_frontmatter_name_fails_naming_its_replacement_and_an_unsupported_lang_warns() {
    let d = found("---\ntitle: T\ntitle-image: hero.png\n---\n");
    let renamed = has(&d, "frontmatter.renamed", "frontmatter.title-image");
    assert_eq!(renamed.level, "error");
    assert!(
        renamed.message.contains("title_image"),
        "{}",
        renamed.message
    );

    // An unsupported tag still sets `<html lang>` and says formatting stays English (TYP-08).
    let d = found("---\ntitle: T\nlang: xx-YY\n---\n");
    let lang = has(&d, "frontmatter.lang", "frontmatter.lang");
    assert_eq!(lang.level, "warning");
    assert!(lang.message.contains("stays English"), "{}", lang.message);
    let page = render("---\ntitle: T\nlang: xx-YY\n---\n\nBody.\n", &options()).unwrap();
    assert!(page.html.contains("<html lang=\"xx-YY\">"));

    for supported in ["cs-CZ", "cs", "de", "ja-JP", "en-GB", "en"] {
        let d = found(&format!("---\ntitle: T\nlang: {supported}\n---\n"));
        assert!(
            d.iter().all(|d| d.code != "frontmatter.lang"),
            "{supported}: {d:?}"
        );
    }
}

#[test]
fn big_is_renamed_to_metric_and_a_block_body_must_be_a_mapping() {
    let document = parse("```big\nvalue: 3\n```\n").unwrap();
    assert!(document.blocks.is_empty(), "the retired tag stays source");
    let renamed = has(&document.diagnostics, "block.renamed", "big");
    assert_eq!(renamed.level, "error");
    assert!(renamed.message.contains("metric"), "{}", renamed.message);

    let d = found("```metric\n- value\n- label\n```\n");
    let body = has(&d, "block.type", "metric");
    assert_eq!(body.level, "error");
    assert!(body.message.contains("mapping"), "{}", body.message);
}

#[test]
fn rotate_belongs_to_text_annotations_only() {
    let chart = |annotation: &str| {
        found(&format!(
            "```chart\ntype: line\ntitle: T\ndata: {{inline: [{{x: 1, y: 2}}, {{x: 2, y: 3}}]}}\nencoding: {{x: x, y: y}}\nannotations:\n  - {annotation}\n```\n"
        ))
    };
    let d = chart("{kind: line, at: {x: 1}, rotate: 45}");
    let rotate = has(&d, "block.schema", "chart.annotations[0].rotate");
    assert!(
        rotate.message.contains("text annotations only"),
        "{}",
        rotate.message
    );
    let d = chart("{kind: text, at: {x: 1, y: 2}, text: Hello, rotate: 45}");
    assert!(
        d.iter().all(|d| d.path != "chart.annotations[0].rotate"),
        "{d:?}"
    );
    let d = chart("{kind: text, at: {x: 1, y: 2}, text: Hello, rotate: steep}");
    has(&d, "block.schema", "chart.annotations[0].rotate");
}

#[test]
fn an_image_needs_alt_and_audio_needs_a_title() {
    has(
        &found("```image\nsrc: photo.png\n```\n"),
        "block.schema",
        "image.alt",
    );
    has(
        &found("```audio\nsrc: clip.mp3\n```\n"),
        "block.schema",
        "audio.title",
    );
    has(
        &found("```video\nsrc: clip.mp4\n```\n"),
        "block.schema",
        "video.title",
    );
    // Fully specified, all three are blocks.
    let ok = parse(
        "```image\nsrc: photo.png\nalt: A photo\n```\n\n```audio\nsrc: clip.mp3\ntitle: A clip\n```\n\n```video\nsrc: clip.mp4\ntitle: A film\n```\n",
    )
    .unwrap();
    assert_eq!(ok.blocks.len(), 3, "{:?}", ok.diagnostics);
}

#[test]
fn an_embed_needs_https_no_credentials_and_an_allowed_host() {
    let embed = |url: &str| {
        render(&format!("```embed\nurl: {url}\n```\n"), &options())
            .unwrap()
            .diagnostics
    };
    for (url, reason) in [
        ("http://www.youtube.com/embed/abc", "must be https"),
        (
            "https://user:pass@www.youtube.com/embed/abc",
            "user name or password",
        ),
        (
            "https://evil.example/embed/abc",
            "host 'evil.example' is not allowed",
        ),
        (
            "https://www.youtube.com.evil.example/embed",
            "is not allowed",
        ),
    ] {
        let d = embed(url);
        let refused = has(&d, "security.embed", "embed.url");
        assert_eq!(refused.level, "error", "{url}");
        assert!(
            refused.message.contains(reason),
            "{url}: {}",
            refused.message
        );
    }
    let d = embed("https://www.youtube-nocookie.com/embed/abc");
    assert!(d.iter().all(|d| d.code != "security.embed"), "{d:?}");
    has(&d, "assets.network-dependent", "embed.url");
    // `allow` lists a host the default list lacks.
    let d = render(
        "```embed\nurl: https://player.example/embed/1\nallow: [player.example]\n```\n",
        &options(),
    )
    .unwrap()
    .diagnostics;
    assert!(d.iter().all(|d| d.code != "security.embed"), "{d:?}");
}

#[test]
fn a_data_source_is_one_of_the_listed_keys_and_never_headers() {
    let d = parse("```table\ndata:\n  inline: [{a: 1}]\n  headers: {x: y}\n```\n")
        .map(|d| d.diagnostics)
        .unwrap_or_default();
    let render_d = render(
        "```table\ndata:\n  inline: [{a: 1}]\n  headers: {x: y}\n```\n",
        &options(),
    );
    let all: Vec<Diagnostic> = match render_d {
        Ok(report) => report.diagnostics,
        Err(diagnostics) => diagnostics,
    };
    let all = [d, all].concat();
    let headers = all
        .iter()
        .find(|d| d.message.contains("headers"))
        .unwrap_or_else(|| panic!("{all:#?}"));
    assert_eq!(headers.level, "error");
    assert!(headers.message.contains("retired"), "{}", headers.message);

    let none = render("```table\ndata: {}\n```\n", &options())
        .map(|r| r.diagnostics)
        .unwrap_or_else(|d| d);
    let none = none
        .iter()
        .find(|d| d.message.contains("exactly one"))
        .unwrap();
    assert_eq!(none.level, "error");
    let two = render(
        "```table\ndata:\n  inline: [{a: 1}]\n  csv: data/a.csv\n```\n",
        &options(),
    )
    .map(|r| r.diagnostics)
    .unwrap_or_else(|d| d);
    assert!(
        two.iter().any(|d| d.message.contains("exactly one")),
        "{two:#?}"
    );
}

#[test]
fn an_inline_row_that_is_not_a_mapping_is_one_finding_at_the_row() {
    let inline = |tag: &str, rows: &str| {
        let body = if tag == "chart" {
            "title: T\ntype: bar\nencoding: {x: a, y: b}\n"
        } else {
            ""
        };
        let d = found(&format!("```{tag}\n{body}data:\n  inline: {rows}\n```\n"));
        let at = format!("{tag}.data.inline");
        d.into_iter()
            .filter(|d| d.path.starts_with(&at))
            .collect::<Vec<_>>()
    };
    for tag in ["table", "chart"] {
        for rows in ["[[1, 2]]", "[1, 2]", "[{a: 1}, 3]"] {
            let d = inline(tag, rows);
            assert_eq!(
                d.len(),
                1,
                "{tag} {rows}: one finding per cause (ARC-08): {d:#?}"
            );
            assert_eq!(d[0].code, "data.inline", "{d:#?}");
            assert!(
                d[0].message.contains("is not a mapping"),
                "{}",
                d[0].message
            );
        }
        let row = &inline(tag, "[{a: 1}, 3]")[0];
        assert_eq!(
            row.path,
            format!("{tag}.data.inline[1]"),
            "at the row as written"
        );
        // A value that is not a list at all is the field's own type finding.
        let d = inline(tag, "\"a,b\"");
        assert_eq!(d.len(), 1, "{d:#?}");
        assert_eq!(d[0].code, "block.schema");
        assert_eq!(
            d[0].message,
            format!("{tag}.data.inline must be a list of mappings, not 'a,b'")
        );
    }
}

#[test]
fn a_table_names_only_columns_the_data_has() {
    let report = render(
        "```table\ndata: {inline: [{a: 1}]}\ncolumns: [{key: nope}]\nsort: {key: other}\ntotal: [missing]\n```\n",
        &options(),
    )
    .unwrap();
    for path in ["table.columns[0].key", "table.sort.key", "table.total[0]"] {
        let d = has(&report.diagnostics, "block.field", path);
        assert_eq!(d.level, "error", "{path}");
        assert!(d.message.contains("Columns: a"), "{path}: {}", d.message);
    }
}

#[test]
fn a_column_digits_finding_says_it_counts_significant_digits() {
    // Fresh agents wrote `digits: 1` for one decimal place (candidate/4); the finding names what
    // digits counts, so the repair keeps the precision the author meant (TYP-23).
    let diagnostics = found(
        "```table\ndata: {inline: [{share: 45.2}]}\ncolumns: [{key: share, digits: 1}]\n```\n",
    );
    let d = has(&diagnostics, "block.schema", "table.columns[0].digits");
    assert_eq!(
        d.message,
        "table.columns[0].digits must be a whole number of significant digits from 2 to 6, not 1"
    );
    assert_eq!(
        d.fix.as_deref(),
        Some("Set `table.columns[0].digits` to a whole number of significant digits from 2 to 6.")
    );
}

#[test]
fn a_chart_takes_exactly_the_channels_of_its_type() {
    // TYP-14: (type, required channels, other channels it takes). Anything else is refused.
    const ALL: [&str; 8] = [
        "x", "y", "lower", "upper", "color", "size", "label", "facet",
    ];
    let matrix: [(&str, &[&str], &[&str]); 9] = [
        ("line", &["x", "y"], &["color", "facet"]),
        ("area", &["x", "y"], &["color", "facet"]),
        ("bar", &["x", "y"], &["color", "facet"]),
        ("scatter", &["x", "y"], &["color", "size", "label", "facet"]),
        ("pie", &["x", "y"], &["facet"]),
        ("heatmap", &["x", "y", "color"], &["facet"]),
        ("histogram", &["x"], &["color", "facet"]),
        ("boxplot", &["y"], &["x", "color", "facet"]),
        (
            "interval",
            &["x", "lower", "upper"],
            &["y", "color", "facet"],
        ),
    ];
    let chart = |kind: &str, channels: &[&str]| {
        let encoding = channels
            .iter()
            .map(|c| format!("{c}: f_{c}"))
            .collect::<Vec<_>>()
            .join(", ");
        found(&format!(
            "```chart\ntype: {kind}\ntitle: T\ndata: {{inline: [{{f_x: 1}}]}}\nencoding: {{{encoding}}}\n```\n"
        ))
    };
    for (kind, required, optional) in matrix {
        let clean = chart(kind, required);
        assert!(
            clean.iter().all(|d| !d.path.starts_with("chart.encoding")),
            "{kind} with only its required channels: {clean:?}"
        );
        for channel in ALL {
            if required.contains(&channel) {
                let channels: Vec<&str> =
                    required.iter().copied().filter(|c| *c != channel).collect();
                let d = chart(kind, &channels);
                let missing = has(&d, "block.schema", &format!("chart.encoding.{channel}"));
                assert!(
                    missing.message.contains("needs this channel"),
                    "{kind} {channel}"
                );
            } else {
                let channels = [required, &[channel]].concat();
                let d = chart(kind, &channels);
                let found = d.iter().any(|d| {
                    d.code == "block.schema" && d.path == format!("chart.encoding.{channel}")
                });
                assert_eq!(
                    found,
                    !optional.contains(&channel),
                    "{kind} with {channel}: {d:?}"
                );
            }
        }
    }
    let d = chart("interval", &["x", "lower", "y"]);
    let article = has(&d, "block.schema", "chart.encoding.upper");
    assert!(
        article.message.contains("an interval chart needs"),
        "{}",
        article.message
    );
}

#[test]
fn a_refused_channel_is_one_finding_that_names_the_types_reading_it() {
    // The channel matrix and the type's own option check both refused these fields, each in
    // its own words, so an author got two errors for one repair.
    for (kind, encoding, path) in [
        ("boxplot", "{x: g}", "chart.encoding.y"),
        ("histogram", "{x: v, y: v}", "chart.encoding.y"),
        ("histogram", "{color: g}", "chart.encoding.x"),
        ("interval", "{x: g, y: v}", "chart.encoding.lower"),
        ("line", "{x: g, y: v, lower: v}", "chart.encoding.lower"),
    ] {
        let d = found(&format!(
            "```chart\ntype: {kind}\ntitle: T\ndata: {{inline: [{{g: A, v: 1}}]}}\nencoding: {encoding}\n```\n"
        ));
        let at: Vec<&Diagnostic> = d.iter().filter(|d| d.path == path).collect();
        assert_eq!(at.len(), 1, "{kind} {encoding}: {at:#?}");
    }
    // A channel few types read names them, so the author can switch type; one that most types
    // read does not list them.
    let d = found(
        "```chart\ntype: line\ntitle: T\ndata: {inline: [{g: A, v: 1}]}\nencoding: {x: g, y: v, lower: v, label: g}\n```\n",
    );
    assert_eq!(
        has(&d, "block.schema", "chart.encoding.lower").message,
        "chart.encoding.lower: a line chart takes x, y, color, facet, detail; lower is read by interval, dumbbell and band charts"
    );
    assert_eq!(
        has(&d, "block.schema", "chart.encoding.label").message,
        "chart.encoding.label: a line chart takes x, y, color, facet, detail"
    );
}

#[test]
fn a_highlight_that_names_no_point_says_so() {
    let source = "```chart\ntype: scatter\ntitle: Heights\ndata: {inline: [{x: 1, y: 2, n: A}, {x: 2, y: 3, n: B}]}\nencoding: {x: x, y: y, label: n}\nhighlight: {by: n, include: [A, Nowhere]}\n```\n";
    let report = check(source, &options()).unwrap();
    let missing = has(
        &report.diagnostics,
        "chart.unresolved-highlight",
        "chart.highlight.include",
    );
    assert_eq!(missing.level, "warning");
    assert!(missing.message.contains("Nowhere"), "{}", missing.message);
    assert_eq!(
        report
            .diagnostics
            .iter()
            .filter(|d| d.code == "chart.unresolved-highlight")
            .count(),
        1,
        "only the name that matches no row is reported"
    );
}

#[test]
fn an_unknown_key_under_tabs_or_a_scrolly_step_is_named_and_a_wrong_box_is_one_finding() {
    // A typo that the bespoke tabs and scrolly rules never read was silently ignored (TYP-84).
    let d = found(
        "---\ntitle: T\ntabs:\n  defualt: one\n  panels:\n    - {id: one, label: One, from: one, colour: red}\n---\n\n# One {#one}\n",
    );
    let typo = has(&d, "frontmatter.unknown-key", "frontmatter.tabs.defualt");
    assert!(
        typo.message.contains("Did you mean 'default'"),
        "{}",
        typo.message
    );
    has(
        &d,
        "frontmatter.unknown-key",
        "frontmatter.tabs.panels[0].colour",
    );

    let scrolly = |steps: &str| {
        found(&format!(
            "```scrolly\nimage: {{src: map.png, alt: A map}}\nsteps:\n{steps}```\n"
        ))
    };
    let d = scrolly(
        "  - text: One\n    view: {x: 0.5, zom: 2}\n  - text: Two\n    highlight: {x: 0.1, y: 0.1, w: 0.2, h: 0.2, colour: red}\n",
    );
    let zoom = has(&d, "block.unknown-key", "scrolly.steps[0].view.zom");
    assert!(
        zoom.message.contains("Did you mean 'zoom'"),
        "{}",
        zoom.message
    );
    has(&d, "block.unknown-key", "scrolly.steps[1].highlight.colour");

    // A wrong field inside the view or the box is one finding at that field, not a second at
    // the mapping that holds it.
    let d = scrolly(
        "  - text: One\n    highlight: {x: 0.1, y: 0.1, w: 0.2}\n  - text: Two\n    view: {x: a}\n",
    );
    let errors: Vec<&str> = d
        .iter()
        .filter(|d| d.level == "error")
        .map(|d| d.path.as_str())
        .collect();
    assert_eq!(
        errors,
        ["scrolly.steps[0].highlight.h", "scrolly.steps[1].view.x"]
    );
}

#[test]
fn an_x_axis_locale_sets_its_calendar_labels_whatever_the_report_language() {
    // TYP-16: `axes.x.locale` selects the calendar labels; without it they follow `lang`.
    let summary = |lang: &str, axes: &str| -> String {
        let source = format!(
            "---\ntitle: T\nlang: {lang}\n---\n\n```chart\ntype: line\ntitle: Monthly\ndata:\n  inline:\n    - {{d: \"2024-01-01\", v: 1}}\n    - {{d: \"2024-03-01\", v: 3}}\n    - {{d: \"2024-06-01\", v: 6}}\nencoding: {{x: d, y: v}}\n{axes}```\n"
        );
        let report = check(&source, &options()).unwrap();
        report.charts[0]["summary"].as_str().unwrap().to_owned()
    };
    assert!(
        summary("en", "").contains("Jan 1, 2024"),
        "{}",
        summary("en", "")
    );
    assert!(summary("cs-CZ", "").contains("1. led 2024"));
    let czech = summary("en", "axes: {x: {locale: cs-CZ}}\n");
    assert!(
        czech.contains("1. led 2024") && czech.contains("1. čvn 2024"),
        "{czech}"
    );
    let english = summary("cs-CZ", "axes: {x: {locale: en-US}}\n");
    assert!(english.contains("Jan 1, 2024"), "{english}");
    let british = summary("en", "axes: {x: {locale: en-GB}}\n");
    assert!(british.contains("1 Jan 2024"), "{british}");
}

#[test]
fn only_the_x_axis_takes_a_locale_and_values_follow_the_report_language() {
    // TYP-16: `locale` is an x-axis field. A value axis's locale is dropped with a warning; its
    // numbers follow `lang`, as every other number in the report does.
    let report = |lang: &str, axes: &str| {
        let source = format!(
            "---\ntitle: T\nlang: {lang}\n---\n\n```chart\ntype: line\ntitle: Monthly\ndata:\n  inline:\n    - {{d: \"2024-01\", v: 1234.5}}\n    - {{d: \"2024-02\", v: 3456.75}}\nencoding: {{x: d, y: v}}\naxes: {axes}\n```\n"
        );
        check(&source, &options()).unwrap()
    };
    for axis in ["y", "y2"] {
        let english = report("en", &format!("{{{axis}: {{locale: cs-CZ}}}}"));
        let dropped = has(
            &english.diagnostics,
            "block.unknown-key",
            &format!("chart.axes.{axis}.locale"),
        );
        assert_eq!(dropped.level, "warning");
        let summary = english.charts[0]["summary"].as_str().unwrap();
        assert!(summary.contains("3,457"), "{axis}: {summary}");
    }
    let czech = report("cs-CZ", "{y: {label: Prodej}}");
    let summary = czech.charts[0]["summary"].as_str().unwrap();
    // Czech groups thousands with a no-break space.
    assert!(
        summary
            .replace(['\u{a0}', '\u{202f}'], " ")
            .contains("3 457"),
        "{summary}"
    );
}

#[test]
fn chart_diagnostics_sit_on_the_path_grammar_and_keep_the_engines_fix() {
    // TYP-53: a finding about a chart as a whole is at `chart`, never at its generated id, and the
    // engine's one-sentence fix reaches the record.
    let source = "```chart\ntype: scatter\ntitle: Missing field\ndata: {inline: [{a: 1, b: 2}]}\nencoding: {x: a, y: zzz}\n```\n\n```chart\ntype: bar\ntitle: Not numbers\ndata: {inline: [{c: a, v: x}, {c: b, v: 2}]}\nencoding: {x: c, y: v}\n```\n";
    let report = check(source, &options()).unwrap();
    let chart_paths: Vec<&str> = report
        .diagnostics
        .iter()
        .filter(|d| d.code.starts_with("chart."))
        .map(|d| d.path.as_str())
        .collect();
    assert!(!chart_paths.is_empty());
    assert!(
        chart_paths
            .iter()
            .all(|path| !path.starts_with("rv-chart-")),
        "{chart_paths:?}"
    );
    // A missing encoding field now stops the chart before layout; the field diagnostic
    // replaces the engine's misleading missing-observations check for that chart.
    has(&report.diagnostics, "block.field", "chart.encoding.y");
    has(&report.diagnostics, "chart.failed", "chart");
    has(&report.diagnostics, "chart.non-numeric", "rows[0].v");
    for d in report
        .diagnostics
        .iter()
        .filter(|d| d.code == "chart.failed")
    {
        assert!(
            d.details
                .as_ref()
                .is_some_and(|details| details["chartId"].is_string())
        );
    }

    let source = "```chart\ntype: scatter\ntitle: Heights\ndata: {inline: [{x: 1, y: 2, n: A}]}\nencoding: {x: x, y: y, label: n}\nhighlight: {by: n, include: [Nowhere]}\n```\n";
    let report = check(source, &options()).unwrap();
    let missing = has(
        &report.diagnostics,
        "chart.unresolved-highlight",
        "chart.highlight.include",
    );
    assert_eq!(
        missing.fix.as_deref(),
        Some("Check the spelling against the data.")
    );
}

#[test]
fn a_retired_media_name_fails_naming_its_replacement() {
    // TYP-29, TYP-30, TYP-31: image `title` is `alt`; audio and video `alt` is `title`.
    for (source, path, replacement) in [
        (
            "```image\nsrc: photo.png\nalt: A harbour\ntitle: A harbour at dusk\n```\n",
            "image.title",
            "Rename `title` to `alt`",
        ),
        (
            "```audio\nsrc: tone.mp3\ntitle: A tone\nalt: A test tone\n```\n",
            "audio.alt",
            "Rename `alt` to `title`",
        ),
        (
            "```video\nsrc: clip.mp4\ntitle: A clip\nalt: A test pattern\n```\n",
            "video.alt",
            "Rename `alt` to `title`",
        ),
    ] {
        let document = parse(source).unwrap();
        assert!(
            document.blocks.is_empty(),
            "{path}: a retired name fails its block"
        );
        let renamed = has(&document.diagnostics, "block.renamed", path);
        assert_eq!(renamed.level, "error");
        assert!(
            renamed
                .fix
                .as_deref()
                .is_some_and(|fix| fix.starts_with(replacement)),
            "{renamed:?}"
        );
    }
    // Audio sizes are only dropped (TYP-30).
    let d = found("```audio\nsrc: tone.mp3\ntitle: A tone\nwidth: 300\n```\n");
    assert_eq!(has(&d, "block.unknown-key", "audio.width").level, "warning");
}

#[test]
fn a_styling_key_written_outside_style_names_its_place_in_style_unless_style_sets_it() {
    let chart = |extra: &str| {
        format!(
            "```chart\ntype: bar\ntitle: Visits by region\nencoding: {{x: region, y: visits}}\n{extra}\ndata:\n  inline:\n    - {{region: North, visits: 3}}\n```\n"
        )
    };
    // TYP-85: the top-level `ratio` is removed, and its place is `style.ratio`.
    let d = found(&chart("ratio: 1.5"));
    let moved = has(&d, "block.unknown-key", "chart.ratio");
    assert_eq!(moved.level, "warning");
    assert!(
        moved.message.contains("belongs at chart.style.ratio"),
        "{}",
        moved.message
    );
    assert_eq!(
        moved.fix.as_deref(),
        Some("Move `chart.ratio` to `chart.style.ratio`.")
    );
    // The same holds inside another mapping.
    let d = found(&chart("style: {}\naxes: {y: {palette: air}}"));
    let moved = has(&d, "block.unknown-key", "chart.axes.y.palette");
    assert!(
        moved.message.contains("belongs at chart.style.palette"),
        "{}",
        moved.message
    );
    // A repair never overwrites a value the author set.
    let d = found(&chart("ratio: 1.5\nstyle: {ratio: 2}"));
    let kept = has(&d, "block.unknown-key", "chart.ratio");
    assert!(!kept.message.contains("belongs at"), "{}", kept.message);
    // An ordinary word with one home outside `style` is not moved: an attribution is not a
    // flow's source channel.
    let d = found(&chart("source: World Bank"));
    let unknown = has(&d, "block.unknown-key", "chart.source");
    assert!(
        !unknown.message.contains("encoding.source"),
        "{}",
        unknown.message
    );
}

#[test]
fn missing_chart_encoding_column_reports_a_suggestion_before_layout() {
    let source = "```chart\ntype: line\ntitle: Life expectancy\ndata: {inline: [{year: 1950, life: 42.8, region: Africa}, {year: 1960, life: 44, region: Africa}]}\nencoding: {x: year, y: lif, color: region}\naxes: {y: {min: 40}}\n```\n";
    let page =
        render(source, &options()).expect("the invalid chart is a recoverable block failure");
    let diagnostic = has(&page.diagnostics, "block.field", "chart.encoding.y");
    assert_eq!(diagnostic.level, "error");
    assert!(diagnostic.message.contains("'lif'"));
    assert!(diagnostic.message.contains("did you mean 'life'"));
    assert!(page.html.contains("Chart not rendered"));
    assert!(
        page.charts.is_empty(),
        "a chart with an invalid encoding must not reach layout"
    );
    let result = check(source, &options()).expect("check also returns the block diagnostic");
    let diagnostic = has(&result.diagnostics, "block.field", "chart.encoding.y");
    assert!(diagnostic.message.contains("did you mean 'life'"));
}

#[test]
fn missing_scrolly_chart_encoding_column_fails_before_layout() {
    let source = "```scrolly\nchart:\n  type: line\n  title: Life expectancy\n  data: {inline: [{year: 1950, life: 42.8}, {year: 1960, life: 44}]}\n  encoding: {x: year, y: lif}\n  axes: {y: {min: 40}}\nsteps:\n  - text: Start.\n```\n";
    let page =
        render(source, &options()).expect("the invalid story is a recoverable block failure");
    let diagnostic = has(&page.diagnostics, "block.field", "scrolly.chart.encoding.y");
    assert_eq!(diagnostic.level, "error");
    assert!(diagnostic.message.contains("did you mean 'life'"));
    assert!(page.html.contains("Story not rendered"));
    assert!(page.charts.is_empty());
}
