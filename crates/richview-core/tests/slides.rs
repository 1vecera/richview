//! Rich Slides (FEA-RP-71): a `.rslide` deck renders as one screen for each slide, its slots
//! placed on a 24-column, 24-row stage, and `slides: true` in a report is retired for it.

use richview_core::document::{Diagnostic, Output, RenderOptions, RenderedReport, ReportFolder};

const DECK: &str = "format: rslide/1
title: September review
description: Synthetic rows.
version: 1.0.0
---
title: September review
subtitle: Where the growth came from
---
metric:
  value: 630000
  label: MRR
  format: number_compact
  unit: $
  delta: 0.26
  delta_label: vs August
source: Synthetic rows
---
title: Enterprise carried the quarter
left:
  - **Enterprise** added $80K.
  - **Mid-market** held at $50K.
right:
  chart:
    type: bar
    title: MRR grew each month
    data:
      inline:
        - { month: Jul, mrr: 390000 }
        - { month: Aug, mrr: 500000 }
        - { month: Sep, mrr: 630000 }
    encoding: { x: month, y: mrr }
cols: 9-15
source: Synthetic rows
---
title: Three numbers
left:
  metric:
    value: 42
    label: Accounts
middle:
  metric:
    value: 17
    label: Regions
right:
  metric:
    value: 9
    label: Teams
---
title: Thank you
";

fn folder() -> ReportFolder {
    ReportFolder {
        root: "slides-fixture".into(),
        source: "deck.rslide".into(),
        files: vec!["deck.rslide".into()],
    }
}

fn render(source: &str, output: Output) -> RenderedReport {
    richview_core::document::render(
        source,
        &RenderOptions {
            output,
            report_folder: Some(folder()),
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
            report_folder: Some(folder()),
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

/// One slide's markup, by its number.
fn slide(html: &str, number: usize) -> &str {
    let start = html
        .split(&format!("data-rv-slide=\"{number}\""))
        .nth(1)
        .unwrap_or_else(|| panic!("slide {number}"));
    start
        .split("data-rv-slide=\"")
        .next()
        .unwrap()
        .split("</section>")
        .next()
        .unwrap()
}

#[test]
fn a_deck_is_one_slide_a_screen_in_the_order_written() {
    let report = render(DECK, Output::Report);
    let html = &report.html;
    assert!(html.contains(" data-rv-slides"), "the document is marked");
    assert_eq!(
        slides(html),
        [
            // The first slide with furniture only is the cover.
            "rv-slide rv-slide--cover rv-slide--bare rv-slide--nofoot",
            // One number alone is the hero.
            "rv-slide rv-slide--hero rv-slide--one rv-slide--nohead",
            "rv-slide rv-slide--figures rv-slide--left-right",
            "rv-slide rv-slide--numbers rv-slide--left-middle-right rv-slide--nofoot",
            // Furniture only, after the first slide: a section.
            "rv-slide rv-slide--section rv-slide--bare rv-slide--nofoot rv-slide--last",
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
    assert_eq!(report.metadata["title"], "September review");
    // A deck has no report header, no contents and no report footer: the colophon closes the
    // last slide, and the page keeps its title as a heading the stage hides.
    assert!(!html.contains("<header class=\"rv-report__header"));
    assert!(!html.contains("<nav class=\"rv-toc"));
    assert!(html.contains("<h1 class=\"rv-slides__title\">September review</h1>"));
    assert!(!report.html[..report.html.find("<main").unwrap()].contains("<footer"));
    // A clean deck says nothing.
    assert_eq!(codes(&report), Vec::<&str>::new());
}

#[test]
fn a_slide_places_each_slot_on_the_grid_and_its_regions_come_in_one_order() {
    let report = render(DECK, Output::Report);
    let figures = slide(&report.html, 3);
    let at = |needle: &str| {
        figures
            .find(needle)
            .unwrap_or_else(|| panic!("{needle} is on the slide"))
    };
    // Head, content, foot; the slots in reading order inside the content.
    assert!(at("rv-slide__head") < at("rv-slide__content"));
    assert!(at("rv-slide__content") < at("rv-slide__foot"));
    assert!(at("data-rv-slot=\"left\"") < at("data-rv-slot=\"right\""));
    assert!(at("Enterprise carried the quarter</h2>") < at("rv-slide__content"));
    // The golden split is a track list on the content, the slot's place a pair of properties.
    assert!(figures.contains("--rv-x:9fr 15fr;--rv-y:24fr"), "{figures}");
    assert!(figures.contains("--rv-gc:1 / 2;--rv-gr:1 / 2"));
    assert!(figures.contains("--rv-gc:2 / 3;--rv-gr:1 / 2"));
    // The text is the slot's Markdown, the chart its figure, and the source stands at the foot.
    assert!(figures.contains("<strong>Enterprise</strong> added $80K."));
    assert!(figures.contains("rv-slot--chart rv-slide__figure rv-slide__figure--chart"));
    assert!(figures.contains("data-rv-chart"));
    assert!(figures.contains("<p class=\"rv-slide__source\">Synthetic rows</p>"));
    // A slide with no head or foot gives its slot all 24 rows and has neither element.
    let hero = slide(&report.html, 2);
    assert!(
        !hero.contains("<header") && hero.contains("<footer"),
        "its source is the foot"
    );
    assert!(hero.contains("rv-slot--metric rv-slide__number"));
    let bare = render(
        "format: rslide/1\n---\nmetric:\n  value: 7\n  label: Teams\n---\ntitle: Next\n",
        Output::Report,
    );
    let alone = slide(&bare.html, 1);
    assert!(!alone.contains("<header") && !alone.contains("<footer"));
    assert!(alone.contains("--rv-y:24fr") && alone.contains("--rv-gr:1 / 2"));
}

#[test]
fn a_report_only_layout_is_not_read_in_a_deck() {
    // A text slot is Markdown, not a report's body: it lays out no columns, and the report's tabs
    // and folded sections are not a deck's keys, so a setup that asks for them is told.
    let source = "format: rslide/1\ntabs:\n  panels: []\ncollapsed_sections: [two]\n---\ntitle: T\ntext: |\n  ::: columns\n  ::: column\n  First.\n  :::\n  ::: column\n  Second.\n  :::\n  :::\n";
    let report = render(source, Output::Report);
    assert!(!report.html.contains("rv-columns"), "no columns on a slide");
    assert!(
        !report.html.contains("richview:columns:"),
        "no marker is left behind"
    );
    assert!(report.html.contains("First.") && report.html.contains("Second."));
    assert!(!report.html.contains("<details class=\"rv-section\""));
    assert!(!report.html.contains("rv-tabs__tab"));
    let mut keys: Vec<&str> = report
        .diagnostics
        .iter()
        .filter(|d| d.code == "deck.key")
        .map(|d| d.path.as_str())
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["frontmatter.collapsed_sections", "frontmatter.tabs"]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.code.starts_with("columns."))
    );
}

#[test]
fn a_chart_on_a_slide_prints_its_values_unless_its_author_turns_them_off() {
    // On a stage the number is the message: the chart keeps the engine's default, as in a report.
    let deck = render(DECK, Output::Report);
    assert!(!deck.html.contains("\"auto_annotations\""));
    // Larger and in the axis's compact form on the stage; a report's payload leaves it out.
    assert!(deck.html.contains("\"stageValues\":true"));
    let report = render(
        "title: R\n---\n\n```chart\ntype: bar\ntitle: T\ndata:\n  inline:\n    - { a: x, b: 1 }\nencoding: { x: a, y: b }\n```\n",
        Output::Report,
    );
    assert!(!report.html.contains("stageValues"));
    let off = render(
        &DECK.replace(
            "    type: bar\n",
            "    type: bar\n    auto_annotations: false\n",
        ),
        Output::Report,
    );
    assert!(off.html.contains("\"auto_annotations\":false"));
}

#[test]
fn a_report_that_still_says_slides_true_is_told_what_to_write() {
    let report = render(
        "---\ntitle: T\nslides: true\n---\n\n# One\n\nText.\n",
        Output::Report,
    );
    let retired = found(&report, "frontmatter.slides");
    assert_eq!(retired.level, "error");
    assert_eq!(retired.path, "frontmatter.slides");
    assert!(
        retired.message.contains("`.rslide`")
            && retired.message.contains("format: rslide/1")
            && retired.message.contains("richview learn slides"),
        "{}",
        retired.message
    );
    // The report renders as the report it is: no slide anywhere.
    for absent in ["rv-slide", "data-rv-slides", "rv-report--slides"] {
        assert!(!report.html.contains(absent), "{absent}");
    }
}

#[test]
fn a_deck_and_a_report_never_combine() {
    // A fence and a heading in a deck's text are text; a deck's marker in a report is a mistake.
    let report = render("---\ntitle: T\n---\n\nformat: rslide/1\n", Output::Report);
    assert!(!report.html.contains("rv-slide"));
    // A deck's text is Markdown, not a report: a heading there is a heading.
    let deck = render(
        "format: rslide/1\n---\ntitle: T\ntext: |\n  ## Not a new slide\n\n  Words.\n",
        Output::Report,
    );
    assert_eq!(slides(&deck.html).len(), 1);
    assert!(deck.html.contains("Not a new slide"));
}

#[test]
fn slots_that_are_not_a_layout_are_an_error_that_names_the_nearest() {
    let report = render(
        "format: rslide/1\n---\ntitle: T\nleft: a\nbottom: b\n",
        Output::Report,
    );
    let layout = found(&report, "deck.layout");
    assert_eq!(layout.level, "error");
    assert_eq!(layout.path, "slides[1]");
    assert_eq!(layout.line, Some(2));
    assert!(layout.message.contains("nearest"), "{}", layout.message);
}

#[test]
fn a_span_that_does_not_fit_its_layout_is_named_with_its_line() {
    let report = render(
        "format: rslide/1\n---\nleft: a\nright: b\ncols: 9-14\n---\ntop: a\nbottom: b\ncols: 12-12\n",
        Output::Report,
    );
    let spans: Vec<(&str, Option<usize>)> = report
        .diagnostics
        .iter()
        .filter(|d| d.code == "deck.span")
        .map(|d| (d.path.as_str(), d.line))
        .collect();
    assert_eq!(
        spans,
        [("slides[1].cols", Some(5)), ("slides[2].cols", Some(9))]
    );
}

#[test]
fn a_chart_that_fails_is_found_at_its_line_in_the_deck_and_keeps_its_slot() {
    let report = render(
        "format: rslide/1\n---\ntitle: T\nleft:\n  chart:\n    type: nonsense\nright: ok\n",
        Output::Report,
    );
    let chart = report
        .diagnostics
        .iter()
        .find(|d| d.path.starts_with("slides[1].left.chart"))
        .expect("a chart finding that names its slot");
    assert!(chart.line.is_some_and(|line| (5..=6).contains(&line)));
    let html = slide(&report.html, 1);
    assert!(html.contains("data-rv-slot=\"left\"") && html.contains("data-rv-slot=\"right\""));
}

#[test]
fn a_slide_that_holds_too_much_is_named_with_its_line() {
    let words = "word ".repeat(100);
    let source = format!(
        "format: rslide/1\n---\ntitle: Fine\nleft: Short.\nright: Short.\n---\ntitle: Too much\nleft: {words}\nright: Short.\n"
    );
    let report = render(&source, Output::Report);
    let density = found(&report, "slides.density");
    assert_eq!(density.level, "warning");
    assert_eq!(density.line, Some(8), "the line of the slot's text");
    assert!(
        density.message.contains("Slide 2") && density.message.contains("`left`"),
        "{}",
        density.message
    );
    // Nothing is dropped: the slot still renders its words.
    assert!(slide(&report.html, 2).matches("word").count() >= 100);
}

#[test]
fn a_table_on_a_slide_is_shown_without_the_reports_tools() {
    for extra in ["", "    filter: true\n"] {
        let rows: String = (0..12)
            .map(|i| format!("        - {{ name: r{i}, units: {i} }}\n"))
            .collect();
        let report = render(
            &format!(
                "format: rslide/1\n---\ntitle: T\ntable:\n{extra}    data:\n      inline:\n{rows}"
            ),
            Output::Report,
        );
        assert!(report.html.contains("<div class=\"rv-table\"><table"));
        assert!(!report.html.contains(" data-rv-filter>"));
        assert!(!report.html.contains(".rv-table__filter"));
        // Twelve rows are more than a slide shows.
        assert!(codes(&report).contains(&"slides.density"));
    }
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
    assert!(
        !paper.html.contains(" portrait;margin:"),
        "slides bring their own page"
    );
    assert_eq!(slides(&paper.html).len(), 5);
    // On paper a chart is a drawing made for its slot, not a live frame.
    assert!(paper.html.contains("rv-chart--print"));
}

#[test]
fn a_picture_behind_the_header_marks_a_deck_and_nothing_else() {
    // slides.css fills the cover's stage with the picture and sets the title on a panel under
    // this mark (STY-74). A report, paper, a texture and a picture elsewhere carry none.
    const PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABAAAAAJCAIAAAC0SDtlAAAAF0lEQVR42mOs6FnAQApgYiARjGqgiQYA4WwBtjxJERoAAAAASUVORK5CYII=";
    let rendered = |style: &str, deck: bool, output: Output| {
        let files: std::collections::BTreeMap<String, String> = [
            ("hero.png".to_owned(), PNG.to_owned()),
            ("look.rstyle.yaml".to_owned(), style.to_owned()),
        ]
        .into();
        let (source, name) = if deck {
            (
                "format: rslide/1\ntitle: T\nstyles: [look.rstyle.yaml]\n---\ntitle: One\n"
                    .to_owned(),
                "deck.rslide",
            )
        } else {
            (
                "---\ntitle: T\nstyles: [look.rstyle.yaml]\n---\n\n# One\n\nText.\n".to_owned(),
                "report.rmd",
            )
        };
        richview_core::document::render(
            &source,
            &RenderOptions {
                output,
                report_folder: Some(ReportFolder {
                    root: "slides-fixture".into(),
                    source: name.into(),
                    files: std::iter::once(name.to_owned())
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
    assert!(marked(&deck), "a header picture marks the deck");
    assert!(
        deck.html
            .contains("html[data-rv-title-picture] .rv-slide--cover")
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
fn a_stage_quiets_a_charts_other_series_within_the_palette_rule() {
    let source = "format: rslide/1\n---\ntitle: Three segments\nchart:\n  type: bar\n  title: Enterprise leads\n  data:\n    inline:\n      - { month: Aug, mrr: 340, segment: Enterprise }\n      - { month: Aug, mrr: 160, segment: Mid-market }\n      - { month: Aug, mrr: 90, segment: Self-serve }\n      - { month: Sep, mrr: 420, segment: Enterprise }\n      - { month: Sep, mrr: 210, segment: Mid-market }\n      - { month: Sep, mrr: 95, segment: Self-serve }\n  encoding: { x: month, y: mrr, color: segment }\n";
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
fn every_style_sets_its_own_stage_for_the_same_deck() {
    for (style, stage) in [
        ("sleek", "keynote"),
        ("air", "gallery"),
        ("brutalist", "poster"),
        ("hand-drawn", "sketchbook"),
        ("scientific", "paper"),
    ] {
        let report = render_in(DECK, style);
        assert!(
            report.html.contains(&format!("data-rv-stage=\"{stage}\"")),
            "{style} stands on {stage}"
        );
        assert_eq!(codes(&report), Vec::<&str>::new(), "{style}");
        assert_eq!(slides(&report.html).len(), 5, "{style}");
    }
}

#[test]
fn the_sketchbook_stage_draws_its_marks_in_decks_only() {
    let hand_drawn = |source: &str| render_in(source, "hand-drawn").html;
    let deck = hand_drawn(DECK);
    let report = hand_drawn("title: R\n---\n\n# One\n\nText.\n");
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

#[test]
fn a_deck_publishes_through_the_path_a_report_does() {
    // The publish request is made from the deck's own text: the setup carries the metadata a
    // publication needs, and a folder that holds a `.rslide` source is a valid report folder.
    let parsed = richview_core::document::parse(DECK).unwrap();
    assert_eq!(parsed.frontmatter["title"], "September review");
    assert_eq!(parsed.frontmatter["version"], "1.0.0");
    let errors: Vec<_> = richview_core::document::validate_publish(DECK)
        .into_iter()
        .filter(|d| d.level == "error")
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
    let report = render(DECK, Output::Report);
    assert!(
        report.diagnostics.iter().all(|d| d.code != "folder.source"),
        "{:?}",
        codes(&report)
    );
}

#[test]
fn an_editor_asks_the_document_for_the_slide_under_a_line() {
    // The online editor opens a `.rslide` as it opens an `.rmd`: the same entry points on the
    // text, and from the document the slide each source line belongs to.
    assert!(richview_core::document::is_deck(DECK));
    assert!(!richview_core::document::is_deck(
        "---\ntitle: R\n---\n\nText.\n"
    ));
    let document = richview_core::document::parse(DECK).unwrap();
    let rules: Vec<usize> = DECK
        .lines()
        .enumerate()
        .filter(|(_, line)| *line == "---")
        .map(|(index, _)| index + 1)
        .collect();
    let spans = document.deck_slides();
    assert_eq!(
        spans.iter().map(|span| span.first_line).collect::<Vec<_>>(),
        rules
    );
    assert_eq!(
        spans
            .iter()
            .map(|span| span.title.as_deref())
            .collect::<Vec<_>>(),
        [
            Some("September review"),
            None,
            Some("Enterprise carried the quarter"),
            Some("Three numbers"),
            Some("Thank you")
        ]
    );
    // Each slide runs to the line before the next `---`, the last to the end of the source.
    for pair in spans.windows(2) {
        assert_eq!(pair[0].last_line + 1, pair[1].first_line);
    }
    let last = spans.last().unwrap().last_line;
    assert_eq!(last, DECK.split('\n').count());
    // The setup belongs to no slide; every other line to the slide that holds it.
    assert_eq!(document.slide_at_line(1), None);
    assert_eq!(document.slide_at_line(rules[0] - 1), None);
    for span in &spans {
        for line in [span.first_line, span.last_line] {
            assert_eq!(document.slide_at_line(line), Some(span.number));
        }
    }
    assert_eq!(document.slide_at_line(last + 1), None);
    // The rendition marks each slide with the same number, so the preview can follow the caret.
    let report = render(DECK, Output::Report);
    for span in &spans {
        assert!(
            report
                .html
                .contains(&format!("data-rv-slide=\"{}\"", span.number))
        );
    }
    // A report has no slides.
    let report = richview_core::document::parse("---\ntitle: R\n---\n\nText.\n").unwrap();
    assert!(report.deck_slides().is_empty());
    assert_eq!(report.slide_at_line(3), None);
}

#[test]
fn the_demo_deck_is_made_of_real_data_and_renders_clean_in_every_style() {
    // The deck of the docs and its data file, as the folder holds them.
    let source = include_str!("../../../docs/public/gallery/warmest-years/deck.rslide");
    let csv =
        include_str!("../../../docs/public/gallery/warmest-years/data/global-temperature.csv");
    let folder = ReportFolder {
        root: "warmest-years".into(),
        source: "deck.rslide".into(),
        files: vec!["deck.rslide".into(), "data/global-temperature.csv".into()],
    };
    assert!(richview_core::document::is_deck(source));
    let errors: Vec<_> = richview_core::document::validate_publish(source)
        .into_iter()
        .filter(|d| d.level == "error")
        .collect();
    assert!(errors.is_empty(), "the demo deck publishes: {errors:?}");
    for style in ["sleek", "air", "brutalist", "hand-drawn", "scientific"] {
        let report = richview_core::document::render(
            source,
            &RenderOptions {
                styles: vec![style.into()],
                report_folder: Some(folder.clone()),
                data: [("data/global-temperature.csv".to_owned(), csv.to_owned())].into(),
                ..RenderOptions::default()
            },
        )
        .unwrap_or_else(|errors| panic!("the demo deck renders in {style}: {errors:?}"));
        // Nothing a reader of the source must fix: a chart that leaves a value label out for
        // room says so at `info` level and keeps the value in its readout and table.
        let problems: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.level != "info")
            .map(|d| format!("{} {}", d.code, d.path))
            .collect();
        assert_eq!(problems, Vec::<String>::new(), "{style}");
        assert_eq!(slides(&report.html).len(), 10, "{style}");
        assert!(report.html.matches("data-rv-chart").count() >= 3, "{style}");
    }
    let report = render_in_folder(source, csv, &folder);
    let kinds: Vec<&str> = slides(&report.html)
        .iter()
        .map(|classes| classes.split(' ').nth(1).unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            "rv-slide--cover",
            "rv-slide--hero",
            "rv-slide--figures",
            "rv-slide--figures",
            "rv-slide--figures",
            "rv-slide--section",
            "rv-slide--figures",
            "rv-slide--numbers",
            "rv-slide--text",
            "rv-slide--text"
        ]
    );
    // Figures and tables are counted apart, in the order written, for Paper's captions.
    let counted: Vec<&str> = report
        .html
        .split("--rv-n:")
        .skip(1)
        .filter_map(|rest| rest.split([';', '"']).next())
        .collect();
    assert_eq!(counted, ["1", "2", "1", "3"]);
    // The numbers on the slides are the data's: the record, the last year below the baseline and
    // the three warmest years.
    for shown in ["1976", "1.28", "1.18", "1.17"] {
        assert!(report.html.contains(shown), "{shown}");
    }
    let by_year: std::collections::BTreeMap<u32, f64> = csv
        .lines()
        .skip(1)
        .map(|line| {
            let (year, value) = line.split_once(',').unwrap();
            (year.parse().unwrap(), value.parse().unwrap())
        })
        .collect();
    assert_eq!(by_year.len(), 146);
    assert_eq!(
        by_year
            .iter()
            .filter(|(_, v)| **v < 0.0)
            .map(|(y, _)| *y)
            .max(),
        Some(1976)
    );
    let mut warmest: Vec<_> = by_year.iter().collect();
    warmest.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap());
    assert_eq!(
        warmest.iter().take(3).map(|(y, _)| **y).collect::<Vec<_>>(),
        [2024, 2025, 2023]
    );
    assert!(warmest.iter().take(11).all(|(y, _)| **y >= 2015));
    assert!(by_year.range(2015..=2025).all(|(_, v)| *v >= 0.85));
}

/// The deck rendered with its data file in the folder, in the style its setup names.
fn render_in_folder(source: &str, csv: &str, folder: &ReportFolder) -> RenderedReport {
    richview_core::document::render(
        source,
        &RenderOptions {
            report_folder: Some(folder.clone()),
            data: [("data/global-temperature.csv".to_owned(), csv.to_owned())].into(),
            ..RenderOptions::default()
        },
    )
    .unwrap_or_else(|errors| panic!("the demo deck renders: {errors:?}"))
}

#[test]
fn the_editor_completes_the_keys_of_a_deck_from_the_registry() {
    // The same calls an editor makes for a report's blocks, with the deck's two scopes.
    let slide = richview_core::authoring_keys("deck-slide", &[]).expect("a slide's keys");
    for key in [
        "title",
        "subtitle",
        "source",
        "footer",
        "cols",
        "rows",
        "chart",
        "metric",
        "table",
        "text",
        "left",
        "right",
        "top",
        "middle",
        "bottom",
        "top-left",
        "top-right",
        "bottom-left",
        "bottom-right",
    ] {
        assert!(slide.contains(&key), "{key}");
    }
    let setup = richview_core::authoring_keys("deck-setup", &[]).expect("the setup's keys");
    for key in [
        "format",
        "title",
        "description",
        "version",
        "slug",
        "lang",
        "styles",
    ] {
        assert!(setup.contains(&key), "{key}");
    }
    assert!(richview_core::authoring_values("deck-setup", &["styles"]).contains(&"sleek"));
    // A slot's chart, metric or table is completed as in a report.
    assert!(
        richview_core::authoring_keys("chart", &[])
            .unwrap()
            .contains(&"encoding")
    );
}
