//! What a reader gets on paper from the browser's Print, the accessibility floor the HTML itself
//! carries, and the scrollytelling element (FEA-RP-34, 45, 48, 49).

use richview_core::document::{Output, RenderOptions};

fn options() -> RenderOptions {
    let mut options = RenderOptions::default();
    for (key, value) in [
        ("cover.png", "data:image/png;base64,AAAA"),
        ("voice.mp3", "data:audio/mpeg;base64,AAAA"),
        ("clip.mp4", "data:video/mp4;base64,AAAA"),
        ("launch.mp4", "data:video/mp4;base64,AAAA"),
        ("map.png", "data:image/png;base64,BBBB"),
    ] {
        options.data.insert(key.into(), value.into());
    }
    options
}

fn with_folder(mut options: RenderOptions) -> RenderOptions {
    options.report_folder = Some(richview_core::document::ReportFolder {
        root: "fixture".into(),
        source: "report.rmd".into(),
        files: std::iter::once("report.rmd".into())
            .chain(options.data.keys().cloned())
            .collect(),
    });
    options
}

fn render(source: &str) -> richview_core::document::RenderedReport {
    richview_core::document::render(source, &with_folder(options())).unwrap()
}

const MEDIA: &str = "---\ntitle: Media\n---\n\n```video\nsrc: clip.mp4\ntitle: Clip\nposter: cover.png\ncaption: Observed interaction\n```\n\n```video\nsrc: launch.mp4\ntitle: Launch\ncaption: Launch\n```\n\n```audio\nsrc: voice.mp3\ntitle: Interview\n```\n\n```embed\nurl: https://www.youtube.com/embed/example\ncaption: Keynote\n```\n";

#[test]
fn browser_print_swaps_players_and_frames_for_the_pdf_stand_ins() {
    let html = render(MEDIA).html;
    // Each player stays for the screen and is hidden on paper...
    for player in ["<video controls", "<audio controls", "data-rv-embed="] {
        assert!(html.contains(player), "{player}");
    }
    assert!(html.contains(
        "figure:has(> :is(audio, video)), .rv-report figure[data-rv-embed] { display: none; }"
    ));
    // ...and a print-only stand-in follows it, naming the file or the address.
    for card in [
        "<figure class=\"rv-video rv-print-only\"><img src=\"data:image/png;base64,AAAA\" alt=\"Clip\" loading=\"eager\"><figcaption>Observed interaction</figcaption><p class=\"rv-media__line\">Video · clip.mp4</p></figure>",
        "<figure class=\"rv-video rv-media-card rv-print-only\"><p class=\"rv-media__line\">Video · Launch · launch.mp4</p></figure>",
        "<figure class=\"rv-audio rv-media-card rv-print-only\"><p class=\"rv-media__line\">Audio · Interview · voice.mp3</p></figure>",
        "<figure class=\"rv-embed rv-media-card rv-print-only\"><p class=\"rv-media__line\">Embedded page · Keynote · <a href=\"https://www.youtube.com/embed/example\">",
    ] {
        assert!(html.contains(card), "{card}\n{html}");
    }
    // On screen the stand-ins stay out of sight.
    assert!(html.contains(".rv-print-only{display:none}"));
}

#[test]
fn the_pdf_carries_only_the_stand_ins_as_before() {
    let report = richview_core::document::render(
        MEDIA,
        &with_folder(RenderOptions {
            output: Output::Pdf,
            ..options()
        }),
    )
    .unwrap();
    for absent in ["<video", "<audio", "data-rv-embed=", " rv-print-only\">"] {
        assert!(!report.html.contains(absent), "{absent}");
    }
    assert_eq!(
        report
            .diagnostics
            .iter()
            .filter(|d| d.code == "export.video-poster")
            .count(),
        1
    );
}

#[test]
fn the_print_stylesheet_keeps_headings_footers_images_and_chrome_in_order() {
    let html = render("---\ntitle: T\n---\n\n## Heading\n\nText.\n").html;
    for rule in [
        ".rv-report :is(h1, h2, h3, h4, h5, h6) { break-after: avoid; }",
        ".rv-report__body tfoot { display: table-row-group; }",
        ".rv-report details::details-content { content-visibility: visible; display: block; }",
        ".rv-report img { width: auto; max-width: 100%; height: auto; max-height: 60vh; }",
        ".rv-scrolly__stage, .rv-report__header) { position: static !important; }",
        ".rv-report dialog",
    ] {
        assert!(html.contains(rule), "{rule}");
    }
}

#[test]
fn the_page_has_landmarks_focus_rings_reduced_motion_and_header_scopes() {
    let html = render("---\ntitle: T\n---\n\n| a | b |\n|---|---|\n| 1 | 2 |\n").html;
    assert!(html.contains("<header class=\"rv-report__header\">"));
    assert!(html.contains("<main class=\"rv-report__body\">"));
    assert!(html.contains("<footer class=\"rv-report__footer\">"));
    assert!(html.contains("<th scope=\"col\">a</th>"), "{html}");
    assert!(html.contains("<th scope=\"col\">b</th>"));
    assert!(
        html.contains(":is(a, button, summary, input, select, textarea, [tabindex]):focus-visible")
    );
    assert!(html.contains("prefers-reduced-motion"));
    assert_eq!(html.matches("<h1").count(), 1);
}

const STORY: &str = "---\ntitle: Story\n---\n\nBefore.\n\n```scrolly\ntitle: The city\nimage:\n  src: map.png\n  alt: A city map\n  caption: The map\nsteps:\n  - title: Overview\n    text: The whole **map**.\n  - text: The harbour.\n    view: {x: 0.5, y: 0.25, zoom: 2}\n    highlight: {x: 0.6, y: 0.3, w: 0.1, h: 0.1}\n```\n\nAfter.\n";

#[test]
fn a_scrolly_block_reads_in_order_without_scripts() {
    let report = render(STORY);
    assert!(
        report.diagnostics.iter().all(|d| d.level != "error"),
        "{:?}",
        report.diagnostics
    );
    let html = report.html;
    let block = &html[html.find("<section class=\"rv-scrolly\"").unwrap()..];
    let block = &block[..block.find("</section>").unwrap()];
    assert!(block.contains("aria-label=\"The city\""));
    // One figure, its caption and alt text, then the steps as an ordered list.
    assert_eq!(block.matches("alt=\"A city map\"").count(), 2, "{block}");
    assert!(block.contains("<figcaption>The map</figcaption>"));
    let first = block.find("Overview").unwrap();
    let second = block.find("The harbour.").unwrap();
    assert!(block.find("<ol").unwrap() < first && first < second);
    assert!(block.contains("<strong>map</strong>"));
    // The figure starts in the first step's state, and the first step is the current one.
    assert!(block.contains("style=\"--rv-vx:0;--rv-vy:0;--rv-vz:1;--rv-mo:0\""));
    assert!(block.contains("aria-current=\"step\""));
    // The second step names its view and its highlight in the view's own percentages.
    assert!(block.contains("data-rv-view=\"0.5 0.25 2\""));
    assert!(block.contains("data-rv-mark=\"20 10 20 20\""), "{block}");
    // Only the step with a state of its own prints a still; nothing needs a script.
    assert_eq!(block.matches("rv-scrolly__still rv-print-only").count(), 1);
    assert!(!block.contains("<script"));
    // The layout rules ride along, and only in a report that has the element.
    assert!(html.contains(".rv-scrolly__stage { position: sticky;"));
    assert!(
        !render("---\ntitle: T\n---\n\nPlain.\n")
            .html
            .contains(".rv-scrolly__stage { position: sticky;")
    );
    assert!(html.find("Before.").unwrap() < html.find("<section class=\"rv-scrolly\"").unwrap());
    assert!(html.find("<section class=\"rv-scrolly\"").unwrap() < html.find("After.").unwrap());
}

#[test]
fn a_scrolly_block_without_steps_or_a_figure_is_reported() {
    let report = render("---\ntitle: T\n---\n\n```scrolly\nsteps: []\n```\n");
    let messages: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.code == "block.schema")
        .map(|d| d.message.as_str())
        .collect();
    assert!(
        messages.iter().any(|m| m.starts_with("scrolly.steps")),
        "{messages:?}"
    );
    assert!(
        messages.iter().any(|m| m.starts_with("scrolly.image")),
        "{messages:?}"
    );
    let report = render(
        "---\ntitle: T\n---\n\n```scrolly\nimage: {src: map.png, alt: x}\nsteps:\n  - text: a\n    view: {zoom: 0.5}\n```\n",
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.starts_with("scrolly.steps[0].view")),
        "{:?}",
        report.diagnostics
    );
}

#[test]
fn a_scrolly_image_is_listed_as_a_report_resource() {
    let source = "```scrolly\nimage: {src: map.png, alt: A map}\nsteps:\n  - text: First\n```\n";
    let doc = richview_core::document::parse(source).expect("parses");
    let found = richview_core::document::report_resources(&doc);
    assert!(found.iter().any(|r| r.path == "map.png"), "{found:?}");
}

#[test]
fn a_markdown_table_scrolls_inside_its_wrapper_on_screen_and_prints_unwrapped() {
    use richview_core::document::{RenderOptions, ReportFolder};
    let options = RenderOptions {
        report_folder: Some(ReportFolder {
            root: "x".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }),
        ..RenderOptions::default()
    };
    let html = richview_core::document::render("| a | b |\n|---|---|\n| 1 | 2 |\n", &options)
        .unwrap()
        .html;
    assert!(html.contains("class=\"rv-table-scroll\""));
    assert!(
        html.contains(".rv-table-scroll { max-width: 100%; overflow-x: auto;"),
        "wrapper styled"
    );
    assert!(
        html.contains(".rv-table-scroll > table { margin: 0; min-width: 100%; width: max-content;")
    );
    assert!(html.contains(":is(.rv-table, .rv-table-scroll) { overflow: visible; }"));
}

#[test]
fn block_entrances_are_opt_in_from_the_runtime_and_settle_for_print_and_reduced_motion() {
    use richview_core::document::{RenderOptions, ReportFolder};
    let options = RenderOptions {
        report_folder: Some(ReportFolder {
            root: "x".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }),
        ..RenderOptions::default()
    };
    let html = richview_core::document::render("# T\n\nText.\n", &options)
        .unwrap()
        .html;
    // The static page never carries the hiding class; only the runtime adds it.
    assert!(!html.contains("class=\"rv-enter"));
    assert!(html.contains(".rv-enter.rv-entered {"));
    assert!(html.contains("@media (prefers-reduced-motion: reduce) { .rv-enter { opacity: 1;"));
    assert!(html.contains(".rv-enter { opacity: 1; transform: none; transition: none; }"));
}

#[test]
fn tabs_anchor_at_any_heading_level_and_footnotes_never_share_ids_across_strings() {
    use richview_core::document::{RenderOptions, ReportFolder};
    let options = RenderOptions {
        report_folder: Some(ReportFolder {
            root: "x".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }),
        ..RenderOptions::default()
    };
    let source = "---\ntitle: T\ntabs:\n  panels:\n    - {id: one, label: One, from: one}\n    - {id: two, label: Two, from: two}\n---\n\n# Title\n\n## One {#one}\n\nBody cites.[^a]\n\n## Two {#two}\n\n```callout\nkind: neutral\nbody: Callout cites.[^b]\n\n  [^b]: Callout note.\n```\n\n[^a]: Body note.\n";
    let report = richview_core::document::render(source, &options).unwrap();
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.code == "tabs.anchor-missing"),
        "{:?}",
        report.diagnostics
    );
    assert!(report.html.contains("id=\"panel-one\""));
    let ids: Vec<&str> = report
        .html
        .split("id=\"")
        .skip(1)
        .filter_map(|r| r.split('"').next())
        .collect();
    let mut seen = std::collections::HashSet::new();
    for id in ids {
        assert!(seen.insert(id), "duplicate id {id}");
    }
}

#[test]
fn task_list_boxes_name_their_state_and_the_page_declares_an_empty_icon() {
    use richview_core::document::{RenderOptions, ReportFolder};
    let options = RenderOptions {
        report_folder: Some(ReportFolder {
            root: "x".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }),
        ..RenderOptions::default()
    };
    let html = richview_core::document::render("- [x] Measured\n- [ ] Next\n", &options)
        .unwrap()
        .html;
    let at = html.find("<li>").unwrap_or(0);
    assert!(
        html.contains("aria-label=\"Done\""),
        "{}",
        &html[at..at + 200]
    );
    assert!(html.contains("aria-label=\"To do\">"));
    assert!(html.contains("<link rel=\"icon\" href=\"data:,\">"));
}

#[test]
fn a_scrolly_over_a_chart_renders_the_chart_and_names_each_steps_control_choices() {
    use richview_core::document::{RenderOptions, ReportFolder};
    let options = RenderOptions {
        report_folder: Some(ReportFolder {
            root: "x".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }),
        ..RenderOptions::default()
    };
    let source = "```scrolly\nchart:\n  type: bar\n  data:\n    inline:\n      - {x: A, y: 3, year: \"2023\"}\n      - {x: B, y: 5, year: \"2023\"}\n      - {x: A, y: 4, year: \"2024\"}\n      - {x: B, y: 9, year: \"2024\"}\n  encoding: {x: x, y: y}\n  controls:\n    - {kind: playback, id: year, field: year, label: Year}\nsteps:\n  - text: Start.\n  - text: Later.\n    state: {year: \"2024\"}\n  - text: Nope.\n    state: {year: \"1999\"}\n```\n";
    let report = richview_core::document::render(source, &options).unwrap();
    assert!(
        report.html.contains("data-rv-scrolly"),
        "{:?}",
        report.diagnostics
    );
    assert!(report.html.contains("data-rv-chart") || report.html.contains("rv-chart"));
    assert!(
        report.html.contains("data-rv-controls=\"0:1\""),
        "{:?} {}",
        report.diagnostics,
        report
            .html
            .split("data-rv-step")
            .skip(1)
            .take(3)
            .collect::<String>()
    );
    assert!(report.diagnostics.iter().any(|d| d.code == "scrolly.state"));
    assert_eq!(report.charts.len(), 1);
}

#[test]
fn a_chart_is_laid_out_at_the_width_its_framed_figure_gives_the_plot() {
    use richview_core::document::{RenderOptions, ReportFolder};
    let render_with_chart_width = |width: f64| {
        let options = RenderOptions {
            report_folder: Some(ReportFolder {
                root: "x".into(),
                source: "report.rmd".into(),
                files: vec!["report.rmd".into()],
            }),
            chart_widths: if width > 0.0 { vec![width] } else { vec![] },
            ..RenderOptions::default()
        };
        richview_core::document::render(
            "```chart\ntype: bar\ntitle: T\ndata:\n  inline:\n    - {x: A, y: 3}\n    - {x: B, y: 5}\nencoding: {x: x, y: y}\n```\n",
            &options,
        )
        .unwrap()
    };
    // The default style frames the figure, so the unforced phone layout is narrower than the
    // 358 px column by the frame's inset: its ratio equals a forced layout at 318 px.
    let natural = render_with_chart_width(0.0);
    let ratio = |html: &str| {
        html.split("--rv-ratio-phone:")
            .nth(1)
            .and_then(|r| r.split(['"', ';']).next())
            .map(str::to_owned)
    };
    let forced = render_with_chart_width(318.0);
    assert!(ratio(&forced.html).is_some(), "{:?}", forced.diagnostics);
    assert_eq!(ratio(&natural.html), ratio(&forced.html));
}

#[test]
fn a_chart_and_its_source_note_share_one_box_for_browser_print() {
    let source = "```chart\ntype: bar\ntitle: T\ndata:\n  inline:\n    - {x: A, y: 3}\n    - {x: B, y: 5}\nencoding: {x: x, y: y}\n```\n\n*Source: the ledger.*\n\n```chart\ntype: bar\ntitle: U\ndata:\n  inline:\n    - {x: A, y: 3}\n    - {x: B, y: 5}\nencoding: {x: x, y: y}\n```\n";
    let html = render(source).html;
    // Only the chart that has a note is boxed; the box generates none on screen.
    assert_eq!(html.matches("<div class=\"rv-keep\">").count(), 1);
    let boxed = html.split("<div class=\"rv-keep\">").nth(1).unwrap();
    let note = boxed.find("<p class=\"rv-source\">").unwrap();
    assert!(boxed.find("rv-chart").unwrap() < note);
    assert!(
        boxed[note..]
            .split("</p>")
            .nth(1)
            .unwrap()
            .trim_start()
            .starts_with("</div>")
    );
    assert!(html.contains(".rv-keep { display: contents; }"));
}

#[test]
fn display_math_scrolls_in_its_own_box_and_keeps_the_browsers_math_layout() {
    // An author `display: block` scatters every MathML child, and `display: block math` is not
    // computed in every engine, so the formula keeps the browser's own and its box scrolls.
    let html = render("---\ntitle: T\n---\n\n$$\n\\frac{a}{b}\n$$\n").html;
    assert!(html.contains("<span class=\"rv-math\"><math"));
    assert!(html.contains(".rv-report__body .rv-math {"));
    assert!(!html.contains("math[display=\"block\"] { display:"));
}
