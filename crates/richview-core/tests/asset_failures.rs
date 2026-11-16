//! A missing or escaping input fails only its element (FEA-RP-30), and each panel of a tabbed
//! report ends with its own Notes (FEA-BR-40).

use richview_core::document::{Diagnostic, RenderOptions, RenderedReport, ReportFolder};

fn options(files: &[&str]) -> RenderOptions {
    RenderOptions {
        report_folder: Some(ReportFolder {
            root: "fixture".into(),
            source: "report.rmd".into(),
            files: std::iter::once("report.rmd")
                .chain(files.iter().copied())
                .map(str::to_owned)
                .collect(),
        }),
        ..Default::default()
    }
}

fn rendered(source: &str, files: &[&str]) -> RenderedReport {
    richview_core::document::render(source, &options(files)).expect("the draft renders")
}

fn issue<'a>(report: &'a RenderedReport, code: &str) -> &'a Diagnostic {
    report
        .diagnostics
        .iter()
        .find(|d| d.code == code)
        .unwrap_or_else(|| panic!("{code} in {:?}", report.diagnostics))
}

#[test]
fn a_missing_or_escaping_media_block_shows_a_notice_and_the_rest_renders() {
    for (src, code) in [
        ("gone.png", "report.asset.missing"),
        ("../secret.png", "report.asset.escape"),
    ] {
        let source = format!(
            "---\ntitle: T\n---\n\nBefore.\n\n```image\nsrc: {src}\nalt: A\n```\n\nAfter.\n"
        );
        let report = rendered(&source, &[]);
        assert!(report.html.contains("Before.") && report.html.contains("After."));
        assert!(report.html.contains("rv-notice"), "{src}");
        assert!(!report.html.contains("<img"), "{src}");
        let found = issue(&report, code);
        assert_eq!(found.level, "error");
        // The author fixes the src field, on the line after the opening fence (TYP-56).
        assert_eq!(found.line, Some(8));
        assert_eq!(found.column, Some(1));
        assert_eq!(found.tag.as_deref(), Some("image"));
        assert_eq!(found.path, "image.src");
    }
}

#[test]
fn audio_and_video_fail_alone_and_a_present_file_still_renders() {
    let report = rendered(
        "```audio\nsrc: voice.mp3\ntitle: V\n```\n\n```video\nsrc: gone.mp4\ntitle: G\n```\n",
        &["voice.mp3"],
    );
    assert!(report.html.contains("<audio"));
    assert_eq!(report.html.matches("rv-notice\"").count(), 1);
    assert_eq!(
        issue(&report, "report.asset.missing").tag.as_deref(),
        Some("video")
    );
}

const ACCESSIBLE_MEDIA: &str = "```audio\nsrc: data:audio/mpeg;base64,AA==\ntitle: Audio\ntranscript: transcript.md\n```\n\n```video\nsrc: data:video/mp4;base64,AA==\ntitle: Video\ncaptions: captions.vtt\ntranscript: transcript.md\n```\n";

#[test]
fn media_accessibility_files_are_report_resources() {
    let document = richview_core::document::parse(ACCESSIBLE_MEDIA).unwrap();
    let resources: Vec<_> = richview_core::document::report_resources(&document)
        .into_iter()
        .map(|r| (r.path, r.kind))
        .collect();
    assert_eq!(
        resources,
        [
            ("captions.vtt".into(), "media"),
            ("transcript.md".into(), "media")
        ]
    );
}

#[test]
fn media_accessibility_files_are_validated_before_resolution() {
    let document = richview_core::document::parse(ACCESSIBLE_MEDIA).unwrap();
    let absent = options(&[]);
    let found =
        richview_core::document::validate_report_document(&document, absent.report_folder.as_ref())
            .expect_err("media accessibility files must belong to the supplied report folder");
    let mut paths: Vec<_> = found
        .iter()
        .filter(|d| d.code == "report.asset.missing")
        .map(|d| d.path.as_str())
        .collect();
    paths.sort_unstable();
    assert_eq!(
        paths,
        ["audio.transcript", "video.captions", "video.transcript"]
    );
    let complete = options(&["captions.vtt", "transcript.md"]);
    richview_core::document::validate_report_document(&document, complete.report_folder.as_ref())
        .unwrap();
    let escaping = richview_core::document::parse(
        &ACCESSIBLE_MEDIA.replace("transcript: transcript.md", "transcript: ../outside.md"),
    )
    .unwrap();
    let found = richview_core::document::validate_report_document(
        &escaping,
        complete.report_folder.as_ref(),
    )
    .unwrap_err();
    assert_eq!(found.len(), 2);
    assert!(
        found
            .iter()
            .all(|d| d.code == "report.asset.escape" && d.path.ends_with(".transcript"))
    );
}

#[test]
fn a_missing_video_poster_is_left_out_and_the_video_stays() {
    let report = rendered(
        "```video\nsrc: clip.mp4\ntitle: C\nposter: gone.png\n```\n",
        &["clip.mp4"],
    );
    assert!(report.html.contains("<video") && !report.html.contains("gone.png"));
    assert_eq!(issue(&report, "report.asset.missing").path, "video.poster");
}

#[test]
fn a_missing_markdown_image_shows_its_alt_text_at_its_line() {
    let report = rendered(
        "---\ntitle: T\n---\n\nText.\n\n![the alt](missing.png) and ![ok](here.png)\n",
        &["here.png"],
    );
    assert!(report.html.contains("the alt") && !report.html.contains("missing.png"));
    assert!(report.html.contains("src=\"here.png\""));
    let found = issue(&report, "report.asset.missing");
    assert_eq!(found.line, Some(7));
}

#[test]
fn a_link_to_a_missing_or_outside_file_keeps_its_text_and_loses_the_link() {
    let report = rendered(
        "[gone](notes/gone.txt) [out](../x.csv) [here](notes/here.txt#top) [anchor](#a)\n",
        &["notes/here.txt"],
    );
    assert!(report.html.contains("gone") && !report.html.contains("notes/gone.txt"));
    assert!(report.html.contains("out") && !report.html.contains("../x.csv"));
    assert!(report.html.contains("href=\"notes/here.txt#top\""));
    issue(&report, "report.asset.missing");
    issue(&report, "report.asset.escape");
}

#[test]
fn a_missing_style_file_is_skipped_with_an_error() {
    let report = rendered(
        "---\ntitle: T\nstyles: [gone.rstyle.yaml]\n---\n\nBody text.\n",
        &[],
    );
    assert!(report.html.contains("Body text."));
    let found = issue(&report, "report.asset.missing");
    // At the list item that names it (TYP-56).
    assert_eq!(found.path, "frontmatter.styles[0]");
    assert_eq!((found.line, found.column), (Some(3), Some(10)));
}

#[test]
fn a_missing_header_image_shows_its_alt_text_and_the_title() {
    let report = rendered(
        "---\ntitle: The title\ntitle_image: {src: gone.png, alt: Cover photo}\n---\n\nBody.\n",
        &[],
    );
    assert!(report.html.contains("The title") && report.html.contains("Cover photo"));
    assert!(!report.html.contains("gone.png"));
    issue(&report, "report.asset.missing");
}

#[test]
fn a_missing_table_file_fails_only_its_table() {
    let report = rendered(
        "```table\ndata: {csv: gone.csv}\n```\n\nAfter the table.\n",
        &[],
    );
    assert!(report.html.contains("After the table.") && report.html.contains("rv-notice"));
    issue(&report, "data.unresolved");
}

#[test]
fn a_wrong_data_definition_fails_only_its_block_beside_every_other_finding() {
    // FEA-RP-30 (lead, 3 Nov): two sources, retired request headers or a retired header dataset fail
    // only their block. Before, any of them stopped the report: check and generate returned those
    // findings alone and wrote no draft.
    let source = "---\ntitle: T\ndata: {sales: {inline: [{x: 1}]}}\n---\n\n```big\nvalue: 12\n```\n\n```table\ndata:\n  inline: [{a: 1}]\n  headers: {Authorization: Bearer HEADER-SECRET}\n```\n\n```table\ndata: {csv: data/sales.csv, inline: [{a: 1}]}\n```\n\n```chart\ntype: bar\ntitle: Sales\nencoding: {x: x, y: y}\ndata: {id: sales}\n```\n\n```image\nsrc: photo.png\n```\n\nStill here.\n";
    let options = options(&["data/sales.csv", "photo.png"]);
    let mut document = richview_core::document::parse(source).unwrap();
    richview_core::document::attach_asset_findings(&mut document, options.report_folder.as_ref())
        .unwrap();
    let report = richview_core::document::render_document(document, &options).unwrap();
    let errors: Vec<(&str, &str, Option<usize>)> = report
        .diagnostics
        .iter()
        .filter(|d| d.level == "error")
        .map(|d| (d.code.as_str(), d.path.as_str(), d.line))
        .collect();
    assert_eq!(
        errors,
        [
            ("frontmatter.data", "frontmatter.data", Some(3)),
            ("block.renamed", "big", Some(6)),
            ("data.local", "table.data.headers", Some(13)),
            ("data.local", "table.data", Some(17)),
            ("data.local", "chart.data.id", Some(24)),
            ("block.schema", "image.alt", Some(27)),
        ],
        "{:?}",
        report.diagnostics
    );
    for notice in [
        "Big not rendered",
        "Table not rendered",
        "Chart not rendered",
        "Image not rendered",
    ] {
        assert!(report.html.contains(notice), "{notice}");
    }
    assert!(report.html.contains("Still here."));
    // The fence that held request headers is not shown in its notice (ARC-14).
    assert!(!report.html.contains("HEADER-SECRET"));
}

#[test]
fn no_finding_or_notice_repeats_a_credential_written_into_an_address() {
    // ARC-14 applied to data addresses (lead, 3 Nov): a data url with a user name or password fails
    // its block, and no message, fix or notice prints the user name and password.
    let source = "```table\ndata: {url: 'https://reader:URL-SECRET@example.com/sales.csv'}\n```\n\n```image\nsrc: 'https://viewer:IMG-SECRET@example.com/a.png'\nalt: A\n```\n\n```embed\nurl: 'https://watcher:EMBED-SECRET@www.youtube.com/embed/x'\n```\n\nStill here.\n";
    let report = rendered(source, &[]);
    let url = issue(&report, "block.schema");
    assert_eq!((url.path.as_str(), url.line), ("table.data.url", Some(2)));
    assert!(
        url.message.contains("user name or password"),
        "{}",
        url.message
    );
    for secret in [
        "URL-SECRET",
        "IMG-SECRET",
        "EMBED-SECRET",
        "reader:",
        "viewer:",
        "watcher:",
    ] {
        assert!(
            !format!("{:?}", report.diagnostics).contains(secret),
            "{secret}: {:?}",
            report.diagnostics
        );
        assert!(!report.html.contains(secret), "{secret}");
    }
    assert!(report.html.contains("Table not rendered") && report.html.contains("Still here."));
}

#[test]
fn a_missing_data_file_is_one_finding_on_every_surface_and_fails_only_its_chart() {
    // Surfaces attach the folder's findings before rendering (ARC-08): the missing file is named
    // once, by the folder check, and the chart still shows its notice.
    let source = "```chart\ntype: bar\ntitle: Sales\nencoding: {x: name, y: n}\ndata: {csv: data/gone.csv}\n```\n\nAfter the chart.\n";
    let options = options(&[]);
    let mut document = richview_core::document::parse(source).unwrap();
    richview_core::document::attach_asset_findings(&mut document, options.report_folder.as_ref())
        .unwrap();
    let report = richview_core::document::render_document(document, &options).unwrap();
    let errors: Vec<&Diagnostic> = report
        .diagnostics
        .iter()
        .filter(|d| d.level == "error")
        .collect();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(
        (errors[0].code.as_str(), errors[0].path.as_str()),
        ("report.asset.missing", "chart.data.csv")
    );
    assert!(report.html.contains("After the chart.") && report.html.contains("rv-notice"));
}

#[test]
fn an_unloaded_data_url_is_one_finding_at_the_field_that_names_the_sources_to_use() {
    let report = rendered(
        "```chart\ntype: bar\ntitle: Sales\nencoding: {x: name, y: n}\ndata: {url: \"https://example.com/d.csv\"}\n```\n\nAfter the chart.\n",
        &[],
    );
    let errors: Vec<&Diagnostic> = report
        .diagnostics
        .iter()
        .filter(|d| d.level == "error")
        .collect();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(
        (errors[0].code.as_str(), errors[0].path.as_str()),
        ("data.unresolved", "chart.data.url")
    );
    assert!(
        !errors[0].message.contains("data.inline"),
        "{}",
        errors[0].message
    );
    let fix = errors[0].fix.as_deref().unwrap();
    assert!(
        fix.contains("data.inline") && fix.contains("data.csv") && fix.contains("data.json"),
        "{fix}"
    );
    assert!(report.html.contains("After the chart.") && report.html.contains("rv-notice"));
}

#[test]
fn an_unusable_folder_still_stops_the_render() {
    assert!(richview_core::document::render("# T\n", &RenderOptions::default()).is_err());
}

const TABBED: &str = "---\ntitle: T\ntabs:\n  panels:\n    - {id: one, label: One, from: one}\n    - {id: two, label: Two, from: two}\n---\n\n## One {#one}\n\nA[^a] and B[^b].\n\n## Two {#two}\n\nC[^c] and A again[^a].\n\n[^a]: Note A.\n\n[^b]: Note B.\n\n[^c]: Note C.\n";

fn panels(html: &str) -> Vec<&str> {
    html.split("<section class=\"rv-tabpanel\"")
        .skip(1)
        .collect()
}

#[test]
fn each_panel_ends_with_the_notes_it_cites_numbered_across_the_report() {
    let report = rendered(TABBED, &[]);
    let panels = panels(&report.html);
    assert_eq!(panels.len(), 2);
    let (one, two) = (panels[0], panels[1]);
    assert!(one.contains("<li id=\"rv-fn-1\" value=\"1\">Note A."));
    assert!(one.contains("<li id=\"rv-fn-2\" value=\"2\">Note B."));
    assert!(!one.contains("Note C."));
    // Panel two cites A again and C: A gets its own copy, C keeps its report number.
    assert!(two.contains("<li id=\"rv-fn-3\" value=\"3\">Note C."));
    assert!(two.contains("<li id=\"rv-fn-1-2\" value=\"1\">Note A."));
    assert!(two.contains("href=\"#rv-fn-1-2\""));
    assert!(!two.contains("Note B."));
    // Every panel has exactly one Notes section, and no report-wide one remains outside.
    assert_eq!(report.html.matches("class=\"rv-notes\"").count(), 2);
    assert!(!report.html.contains("id=\"rv-notes-title\""));
}

#[test]
fn footnote_links_and_back_links_stay_inside_their_own_panel() {
    let report = rendered(TABBED, &[]);
    let panels = panels(&report.html);
    for panel in &panels {
        for target in panel
            .split("href=\"#")
            .skip(1)
            .filter_map(|r| r.split_once('"'))
        {
            let id = format!("id=\"{}\"", target.0);
            assert!(panel.contains(&id), "{} is not in its panel", target.0);
        }
    }
    // The copy's way back leads to the first reference in that panel.
    assert!(panels[1].contains("class=\"rv-fn-back\" href=\"#rv-fnref-1-2\""));
    let mut ids: Vec<&str> = report
        .html
        .split(" id=\"")
        .skip(1)
        .filter_map(|r| r.split_once('"').map(|p| p.0))
        .collect();
    let all = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), all, "ids stay unique");
}

#[test]
fn panel_anchors_keep_their_deep_links() {
    let report = rendered(TABBED, &[]);
    assert!(report.html.contains("<h2 id=\"one\">") && report.html.contains("id=\"panel-one\""));
    assert!(report.html.contains("<h2 id=\"two\">") && report.html.contains("id=\"panel-two\""));
}

#[test]
fn a_report_without_tabs_keeps_one_notes_section() {
    let report = rendered("A[^a].\n\n[^a]: Note A.\n", &[]);
    assert_eq!(report.html.matches("class=\"rv-notes\"").count(), 1);
    assert!(report.html.contains("id=\"rv-notes-title\""));
}

/// The source of `every_file_is_one_finding_at_its_place`: each line names one file or more.
const NAMED_FILES: &str = "---\ntitle: T\nstyles: [sleek, gone.rstyle.yaml]\ntitle_image: {src: cover.png, alt: C, caption: Cap}\n---\n\nText ![a](x.png) ![b](x.png), [notes](notes/gone.md) and <someone@example.com>.\n\n![r](https://example.com/r.png)\n\n```image\nsrc: gone-block.png\nalt: A\n```\n\n```callout\nkind: neutral\nbody: \"See ![c](gone2.png)\"\n```\n\n```table\ndata: {csv: data/gone.csv}\n```\n";

/// The asset findings, as (code, path, line, column).
fn placed(found: &[Diagnostic]) -> Vec<(String, String, Option<usize>, Option<usize>)> {
    found
        .iter()
        .filter(|d| richview_core::ASSET_FINDINGS.contains(&d.code.as_str()))
        .map(|d| (d.code.clone(), d.path.clone(), d.line, d.column))
        .collect()
}

#[test]
fn every_file_is_one_finding_at_its_place_from_validate_and_from_check() {
    // TYP-56 and FEA-RP-30: the folder check names each file reference once, where it is
    // written, and the render that follows does not name it again. An email link names no file.
    let options = options(&[]);
    let mut document = richview_core::document::parse(NAMED_FILES).unwrap();
    let found = richview_core::document::validate_report_document(
        &document,
        options.report_folder.as_ref(),
    )
    .unwrap_err();
    let at = |code: &str, path: &str, line: usize, column: usize| {
        (code.to_owned(), path.to_owned(), Some(line), Some(column))
    };
    let expected = vec![
        at("report.asset.missing", "frontmatter.styles[1]", 3, 17),
        at("report.asset.missing", "frontmatter.title_image", 4, 1),
        at("report.asset.missing", "markdown", 7, 6),
        at("report.asset.missing", "markdown", 7, 18),
        at("report.asset.missing", "markdown", 7, 31),
        at("report.asset.remote", "markdown", 9, 1),
        at("report.asset.missing", "image.src", 12, 1),
        at("report.asset.missing", "callout.body", 18, 1),
        at("report.asset.missing", "table.data.csv", 22, 8),
    ];
    assert_eq!(placed(&found), expected);
    assert!(
        found.iter().all(|d| !d.message.contains("someone@")),
        "{found:?}"
    );
    richview_core::document::attach_asset_findings(&mut document, options.report_folder.as_ref())
        .unwrap();
    let checked = richview_core::document::check_document(document, &options).unwrap();
    assert_eq!(placed(&checked.diagnostics), expected);
    // Without the folder check, the render names each file it draws itself.
    let rendered = rendered(NAMED_FILES, &[]);
    assert!(
        rendered
            .diagnostics
            .iter()
            .any(|d| d.code == "report.asset.missing" && d.path == "image.src")
    );
}
