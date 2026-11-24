//! Slides (FEA-RP-65): `slides: true` renders the same source as one screen for the
//! title and one for each top-level heading or thematic break, and a source without the field
//! renders as the report it always was.

use richview_core::document::{Diagnostic, Output, RenderOptions, RenderedReport, ReportFolder};

const DECK: &str = "---\ntitle: September review\ndescription: Synthetic rows.\nslides: true\n---\n\n```metric\nvalue: 630000\nlabel: MRR\nformat: number_compact\nunit: $\ndelta: 0.26\ndelta_label: vs August\n```\n\n# At a glance\n\n```metric\nvalue: 130000\nlabel: Net new MRR\nformat: number_compact\nunit: $\n```\n\n```chart\ntype: bar\ntitle: MRR grew each month\ndata:\n  inline:\n    - { month: Jul, mrr: 390000 }\n    - { month: Aug, mrr: 500000 }\n    - { month: Sep, mrr: 630000 }\nencoding: { x: month, y: mrr }\n```\n\n*Source: synthetic rows.*\n\n# Next month\n\n---\n\nOne closing line.\n";

fn render(source: &str, output: Output) -> RenderedReport {
    richview_core::document::render(
        source,
        &RenderOptions {
            output,
            report_folder: Some(ReportFolder {
                root: "slides-fixture".into(),
                source: "report.rmd".into(),
                files: vec!["report.rmd".into()],
            }),
            ..RenderOptions::default()
        },
    )
    .unwrap_or_else(|errors| panic!("the source renders: {errors:?}"))
}

/// The source rendered in one style, in the same fixture folder as `render`.
fn render_in(source: &str, style: &str) -> RenderedReport {
    richview_core::document::render(
        source,
        &RenderOptions {
            styles: vec![style.into()],
            report_folder: Some(ReportFolder {
                root: "slides-fixture".into(),
                source: "report.rmd".into(),
                files: vec!["report.rmd".into()],
            }),
            ..RenderOptions::default()
        },
    )
    .unwrap_or_else(|errors| panic!("the source renders in {style}: {errors:?}"))
}

fn codes(report: &RenderedReport) -> Vec<&str> {
    report.diagnostics.iter().map(|d| d.code.as_str()).collect()
}

fn found<'a>(report: &'a RenderedReport, code: &str) -> &'a Diagnostic {
    report
        .diagnostics
        .iter()
        .find(|d| d.code == code)
        .unwrap_or_else(|| panic!("no {code} in {:?}", codes(report)))
}

/// The slides in order: each one's class list.
fn slides(html: &str) -> Vec<&str> {
    html.split("<section class=\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .filter(|classes| classes.starts_with("rv-slide"))
        .collect()
}

#[test]
fn a_source_without_the_field_is_the_report_it_was() {
    let report = render(&DECK.replace("slides: true\n", ""), Output::Report);
    for absent in ["rv-slide", "data-rv-slides", "rv-report--slides"] {
        assert!(!report.html.contains(absent), "{absent}");
    }
    assert!(report.metadata["_richview"].get("slides").is_none());
    assert!(
        !codes(&report)
            .iter()
            .any(|code| code.starts_with("slides."))
    );
}

#[test]
fn the_title_and_each_heading_or_break_make_a_slide() {
    let report = render(DECK, Output::Report);
    let html = &report.html;
    assert!(html.contains(" data-rv-slides"), "the document is marked");
    assert_eq!(
        slides(html),
        [
            "rv-slide rv-slide--title",
            // Text before the first heading is a slide of its own: here one number, the hero.
            "rv-slide rv-slide--numbers rv-slide--hero rv-slide--untitled",
            "rv-slide rv-slide--figures",
            // A heading with nothing under it is a section slide.
            "rv-slide rv-slide--section",
            "rv-slide rv-slide--text rv-slide--untitled",
        ]
    );
    for n in 1..=5 {
        assert!(
            html.contains(&format!(
                "data-rv-slide=\"{n}\" aria-roledescription=\"slide\" aria-label=\"Slide {n} of 5\""
            )),
            "slide {n} announces its place"
        );
    }
    assert_eq!(report.metadata["_richview"]["slides"], 5);
    // The title slide keeps the report's header, and the colophon closes the last slide.
    let title = html.split("data-rv-slide=\"1\"").nth(1).unwrap();
    let title = title.split("data-rv-slide=\"2\"").next().unwrap();
    assert!(title.contains("<header class=\"rv-report__header\">"));
    assert!(title.contains("September review"));
    let last = html.split("data-rv-slide=\"5\"").nth(1).unwrap();
    assert!(last.contains("One closing line."));
    assert_eq!(
        html.matches("<footer class=\"rv-report__footer\">").count(),
        1
    );
    assert!(last.contains("<footer class=\"rv-report__footer\">"));
    // A slide's regions come in one order, and a source note stays under its figure.
    let glance = html.split("data-rv-slide=\"3\"").nth(1).unwrap();
    let glance = glance.split("data-rv-slide=\"4\"").next().unwrap();
    let at = |needle: &str| {
        glance
            .find(needle)
            .unwrap_or_else(|| panic!("{needle} is on the slide"))
    };
    assert!(at("At a glance</h2>") < at("rv-slide__numbers"));
    assert!(at("rv-slide__numbers") < at("rv-slide__figures"));
    assert!(at("rv-slide__figure rv-slide__figure--chart") < at("data-rv-chart"));
    assert!(at("data-rv-chart") < at("rv-source"));
    // The contents list has nothing to list: every heading is a slide.
    assert!(!html.contains("class=\"rv-toc"));
}

#[test]
fn a_chart_on_a_slide_prints_its_values_unless_its_author_turns_them_off() {
    // On a stage the number is the message: the chart keeps the engine's default, as in a report.
    let deck = render(DECK, Output::Report);
    assert!(!deck.html.contains("\"auto_annotations\""));
    // Larger and in the axis's compact form on the stage; a report's payload leaves it out.
    assert!(deck.html.contains("\"stageValues\":true"));
    let report = render(&DECK.replace("slides: true\n", ""), Output::Report);
    assert!(!report.html.contains("stageValues"));
    let off = render(
        &DECK.replace("type: bar\n", "type: bar\nauto_annotations: false\n"),
        Output::Report,
    );
    assert!(off.html.contains("\"auto_annotations\":false"));
}

#[test]
fn a_deck_within_the_rules_renders_without_a_slides_diagnostic() {
    // Slides are no longer experimental (Daniel, 19 Nov 2026), so a clean deck says nothing.
    let report = render(DECK, Output::Report);
    assert!(
        !codes(&report)
            .iter()
            .any(|code| code.starts_with("slides.")),
        "{:?}",
        codes(&report)
    );
}

#[test]
fn tabs_have_no_effect_and_say_so() {
    let source = "---\ntitle: T\nslides: true\ntabs:\n  panels:\n    - { id: one, label: One, from: one }\n    - { id: two, label: Two, from: two }\n---\n\n# One {#one}\n\nFirst.\n\n# Two {#two}\n\nSecond.\n";
    let report = render(source, Output::Report);
    assert_eq!(found(&report, "slides.tabs").level, "warning");
    assert_eq!(found(&report, "slides.tabs").path, "frontmatter.tabs");
    assert!(!report.html.contains("rv-tabs__tab"));
    assert!(!report.html.contains("rv-tabpanel"));
    assert_eq!(slides(&report.html).len(), 3);
}

#[test]
fn collapsed_sections_fold_nothing_in_slides() {
    // A slide shows everything it holds: the field has no effect and leaves no marker behind.
    let source = "---\ntitle: T\nslides: true\ncollapsed_sections: [two]\n---\n\n# One\n\nFirst.\n\n# Two\n\nSecond.\n";
    let report = render(source, Output::Report);
    let hex: String = "two".bytes().map(|byte| format!("{byte:02x}")).collect();
    assert!(!report.html.contains("<details class=\"rv-section\""));
    assert!(!report.html.contains(&format!("1:{hex}-->")));
    assert!(report.html.contains("Second."));
    assert_eq!(slides(&report.html).len(), 3);
}

#[test]
fn columns_lay_out_nothing_in_slides() {
    // A slide places its blocks in its own regions: the columns' markers go, and a number in a
    // column joins the slide's numbers while the other column's text joins its text.
    let source = "---\ntitle: T\nslides: true\n---\n\n# Accounts\n\n::: columns\n::: column\n```metric\nvalue: 42\nlabel: Accounts\n```\n:::\n::: column\nEvery region grew.\n:::\n:::\n";
    let report = render(source, Output::Report);
    let html = &report.html;
    assert!(!html.contains("rv-columns"), "no columns on a slide");
    assert!(!html.contains("richview:columns:"), "no marker left behind");
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.code.starts_with("columns."))
    );
    let deck = slides(html);
    assert_eq!(deck.len(), 2, "{deck:?}");
    assert!(
        deck[1].starts_with("rv-slide rv-slide--numbers"),
        "{deck:?}"
    );
    // The stylesheet names the regions too, so they are found on the slide itself.
    let slide = html.split("data-rv-slide=\"2\"").nth(1).unwrap();
    let at = |needle: &str| {
        slide
            .find(needle)
            .unwrap_or_else(|| panic!("{needle} is on the slide"))
    };
    assert!(at("Accounts</h2>") < at("rv-slide__numbers"));
    assert!(at("rv-slide__numbers") < at("rv-slide__text"));
    assert!(at("rv-slide__text") < at("Every region grew."));
    // The same source as a report keeps its columns, so the markers above were well formed.
    let report = render(&source.replace("slides: true\n", ""), Output::Report);
    assert!(report.html.contains("<div class=\"rv-columns"));
}

#[test]
fn a_picture_behind_the_header_marks_the_title_slide_and_nothing_else() {
    // slides.css fills the title slide's stage with the picture and sets the title on a panel
    // under this mark (STY-74). A report, paper, a texture and a picture elsewhere carry none.
    const PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABAAAAAJCAIAAAC0SDtlAAAAF0lEQVR42mOs6FnAQApgYiARjGqgiQYA4WwBtjxJERoAAAAASUVORK5CYII=";
    let rendered = |style: &str, slides: bool, output: Output| {
        let files: std::collections::BTreeMap<String, String> = [
            ("hero.png".to_owned(), PNG.to_owned()),
            ("look.rstyle.yaml".to_owned(), style.to_owned()),
        ]
        .into();
        let source = format!(
            "---\ntitle: T\nstyles: [look.rstyle.yaml]\n{}---\n\n# One\n\nText.\n",
            if slides { "slides: true\n" } else { "" }
        );
        richview_core::document::render(
            &source,
            &RenderOptions {
                output,
                report_folder: Some(ReportFolder {
                    root: "slides-fixture".into(),
                    source: "report.rmd".into(),
                    files: std::iter::once("report.rmd".to_owned())
                        .chain(files.keys().cloned())
                        .collect(),
                }),
                data: files,
                ..RenderOptions::default()
            },
        )
        .unwrap_or_else(|errors| panic!("the source renders: {errors:?}"))
    };
    let marked = |report: &RenderedReport| {
        let head = report.html.split("<head>").next().unwrap();
        head.contains(" data-rv-title-picture")
    };
    let header = "preset: air\nbackground:\n  picture: hero.png\n";
    let deck = rendered(header, true, Output::Report);
    assert!(
        deck.diagnostics.iter().all(|d| d.level != "error"),
        "{:?}",
        codes(&deck)
    );
    assert!(marked(&deck), "a header picture marks the slides");
    assert!(
        deck.html
            .contains("html[data-rv-title-picture] .rv-slide--title")
    );
    assert!(
        !marked(&rendered(header, false, Output::Report)),
        "a report"
    );
    assert!(!marked(&rendered(header, true, Output::Pdf)), "paper");
    for other in [
        "preset: air\nbackground:\n  picture: hero.png\n  place: page\n",
        "preset: air\nbackground:\n  texture: dots\n  place: header\n",
        "preset: air\n",
    ] {
        assert!(!marked(&rendered(other, true, Output::Report)), "{other}");
    }
}

#[test]
fn a_slide_that_holds_too_much_is_named_with_its_line() {
    let chart = |title: &str| {
        format!(
            "```chart\ntype: bar\ntitle: {title}\ndata:\n  inline:\n    - {{ a: x, b: 1 }}\n    - {{ a: y, b: 2 }}\nencoding: {{ x: a, y: b }}\n```\n\n"
        )
    };
    let source = format!(
        "---\ntitle: T\nslides: true\n---\n\n# Fine\n\nShort.\n\n# Too much\n\n{}{}{}",
        chart("One"),
        chart("Two"),
        chart("Three")
    );
    let report = render(&source, Output::Report);
    let density = found(&report, "slides.density");
    assert_eq!(density.level, "warning");
    assert_eq!(density.line, Some(10), "the line of the slide's heading");
    assert!(
        density.message.contains("Slide 3 (\"Too much\")") && density.message.contains("3 figures"),
        "{}",
        density.message
    );
    assert!(density.fix.as_deref().unwrap().contains("new slide"));
    // Nothing is dropped: the slide still renders all three.
    assert_eq!(
        report
            .html
            .matches("class=\"rv-slide__figure rv-slide__figure--chart\"")
            .count(),
        3
    );
}

#[test]
fn paper_is_one_slide_a_page_at_sixteen_to_nine() {
    let screen = render(DECK, Output::Report);
    let paper = render(DECK, Output::Pdf);
    for report in [&screen, &paper] {
        assert!(
            report
                .html
                .contains("@page { size: 13.333in 7.5in; margin: 0; }"),
            "browser print and the PDF share the slide page"
        );
    }
    // The report's portrait page box, with its running title, belongs to the report rendition.
    let report = render(&DECK.replace("slides: true\n", ""), Output::Pdf);
    assert!(
        report.html.contains(" portrait;margin:"),
        "a report has its page"
    );
    assert!(
        !paper.html.contains(" portrait;margin:"),
        "slides bring their own"
    );
    assert_eq!(slides(&paper.html).len(), 5);
    // On paper a chart is a drawing made for its slot, not a live frame.
    assert!(paper.html.contains("rv-chart--print"));
}

#[test]
fn a_stage_quiets_a_charts_other_series_within_the_palette_rule() {
    let source = "---\ntitle: Segments\nslides: true\n---\n\n# Three segments\n\n```chart\ntype: bar\ntitle: Enterprise leads\ndata:\n  inline:\n    - { month: Aug, mrr: 340, segment: Enterprise }\n    - { month: Aug, mrr: 160, segment: Mid-market }\n    - { month: Aug, mrr: 90, segment: Self-serve }\n    - { month: Sep, mrr: 420, segment: Enterprise }\n    - { month: Sep, mrr: 210, segment: Mid-market }\n    - { month: Sep, mrr: 95, segment: Self-serve }\nencoding: { x: month, y: mrr, color: segment }\n```\n";
    for style in ["sleek", "air", "brutalist", "hand-drawn", "scientific"] {
        let report = render_in(source, style);
        assert!(
            !codes(&report).contains(&"chart.palette-rule"),
            "{style}: {:?}",
            codes(&report)
        );
    }
}

#[test]
fn the_sketchbook_stage_draws_its_marks_in_slides_only() {
    let hand_drawn = |source: &str| render_in(source, "hand-drawn").html;
    let deck = hand_drawn(DECK);
    let report = hand_drawn(&DECK.replace("slides: true\n", ""));
    assert!(deck.contains("data-rv-stage=\"sketchbook\""));
    for mark in [
        "--rv-pencil-ring:",
        "--rv-pencil-ring-small:",
        "--rv-pencil-arrow:",
        "--rv-pencil-total:",
        "--rv-pencil-swash:",
    ] {
        assert!(deck.contains(mark), "the deck draws {mark}");
        assert!(!report.contains(mark), "a report carries no {mark}");
    }
    assert_eq!(
        deck.matches("data:image/svg+xml,").count(),
        report.matches("data:image/svg+xml,").count() + 5
    );
}

#[test]
fn a_stage_letters_its_charts_in_a_face_the_engine_measures() {
    // The runtime clips each chart label to the box the engine measured, so a face the engine
    // does not measure, such as an interface face, would be cut where its glyphs run wider than
    // the estimate. Every stage keeps the style's data face.
    for (style, first) in [
        ("sleek", "Geist Mono,"),
        ("air", "Geist Mono,"),
        ("scientific", "RichView Code,"),
        ("brutalist", "Space Mono,"),
        ("hand-drawn", "Patrick Hand,"),
    ] {
        let deck = render_in(DECK, style).html;
        let faces: Vec<String> = deck
            .match_indices("\"dataFont\":")
            .map(|(at, key)| {
                serde_json::Deserializer::from_str(&deck[at + key.len()..])
                    .into_iter::<String>()
                    .next()
                    .unwrap()
                    .unwrap()
            })
            .collect();
        assert!(
            !faces.is_empty(),
            "{style}: the deck carries its chart theme"
        );
        for face in faces {
            assert!(
                face.starts_with(first),
                "{style}: charts are lettered in {first}, got {face}"
            );
            assert!(
                richview_chart::text::Font::new(&face).is_measured(),
                "{style}: the engine measures {face}"
            );
        }
    }
}
