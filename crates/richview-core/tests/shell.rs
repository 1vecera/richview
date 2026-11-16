//! The report document: head, policy, header, byline and colophon (FEA-RP-02..06, 50, 51, 58).

use base64::Engine as _;
use richview_core::document::{RenderOptions, RenderedReport, ReportFolder};

fn render(
    source: &str,
    options: &RenderOptions,
) -> Result<RenderedReport, Vec<richview_core::document::Diagnostic>> {
    let mut options = options.clone();
    options.report_folder = Some(ReportFolder {
        root: "shell-fixture".into(),
        source: "report.rmd".into(),
        files: std::iter::once("report.rmd".into())
            .chain(options.data.keys().cloned())
            .collect(),
    });
    richview_core::document::render(source, &options)
}

fn html(source: &str) -> String {
    render(source, &RenderOptions::default()).unwrap().html
}

#[test]
fn the_head_names_language_scheme_format_and_policy() {
    let page = html("---\ntitle: T\nlang: cs-CZ\n---\n\nBody.\n");
    assert!(page.contains("<html lang=\"cs-CZ\">"), "{page:.200}");
    assert!(page.contains("<meta charset=\"utf-8\"><meta name=\"viewport\""));
    assert!(page.contains("<meta name=\"generator\" content=\"richview rmd/1\">"));
    assert!(page.contains("<meta name=\"color-scheme\" content=\""));
    let policy = page
        .split("http-equiv=\"Content-Security-Policy\" content=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap();
    for part in [
        "default-src &#39;none&#39;",
        "base-uri &#39;none&#39;",
        "frame-src &#39;none&#39;",
    ] {
        assert!(policy.contains(part), "{policy}");
    }
    // The policy is the first thing the browser reads after the encoding and viewport.
    assert!(page.find("Content-Security-Policy").unwrap() < page.find("<style>").unwrap());
}

#[test]
fn the_stored_metadata_records_the_format_the_head_names() {
    let report = render("---\ntitle: T\n---\n\nBody.\n", &RenderOptions::default()).unwrap();
    assert_eq!(report.metadata["_richview"]["format"], "rmd/1");
    assert!(
        report.html.contains(&format!(
            "content=\"richview {}\"",
            report.metadata["_richview"]["format"].as_str().unwrap()
        )),
        "the head and the metadata name one format"
    );
    assert_eq!(
        format!("rmd/{}", richview_core::document::FORMAT_MAJOR),
        report.metadata["_richview"]["format"],
        "the marker follows the supported major version"
    );
}

#[test]
fn an_unusable_language_tag_falls_back_to_english() {
    assert!(html("---\ntitle: T\nlang: \"x\\\" onload=\"\n---\n").contains("<html lang=\"en\">"));
}

#[test]
fn the_runtime_is_allowed_by_hash_only() {
    let report = render(
        "# T",
        &RenderOptions {
            runtime_js: Some("const richview_runtime = async()=>{};".into()),
            runtime_wasm: Some("AA==".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let policy = report
        .html
        .split("Content-Security-Policy\" content=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap();
    assert!(
        policy.contains("script-src &#39;wasm-unsafe-eval&#39; &#39;sha256-"),
        "{policy}"
    );
    let script_src = policy
        .split("; ")
        .find(|d| d.starts_with("script-src"))
        .unwrap();
    assert!(!script_src.contains("unsafe-inline"), "{policy}");
}

#[test]
fn an_external_binding_can_carry_raw_wasm_with_one_constant_initializer() {
    let encoded = "AGFzbQEAAAA=";
    let mut initializer = None;
    for base in [
        "/_runtime/report",
        "https://richview.test/runtime",
        "http://127.0.0.1:4173/runtime",
    ] {
        for bytes in [None, Some(encoded)] {
            let report = render(
                "# Preview",
                &RenderOptions {
                    runtime_url: Some(base.into()),
                    runtime_js: Some("THIS_BINDING_MUST_NOT_BE_INLINED".into()),
                    runtime_wasm: bytes.map(str::to_owned),
                    // Native linked hosts can retain the codec after clearing their embedded bytes.
                    runtime_wasm_codec: if bytes.is_some() { "raw" } else { "gzip" }.into(),
                    ..Default::default()
                },
            )
            .unwrap();
            assert!(report.html.contains(&format!("src=\"{base}.js\"")));
            let integrity = "sha256-kFA8HLto1gTaQzp1uIRsK7mGt5+5bSiI6tf6jo/fW04=";
            assert!(report.html.contains(&format!(
                "integrity=\"{integrity}\" crossorigin=\"anonymous\""
            )));
            assert!(report.html.contains(&format!("&#39;{integrity}&#39;")));
            assert!(
                report
                    .html
                    .contains(&format!("data-rv-wasm=\"{base}_bg.wasm\""))
            );
            assert!(!report.html.contains("THIS_BINDING_MUST_NOT_BE_INLINED"));
            let carried = report
                .html
                .split("data-rv-wasm-bytes=\"")
                .nth(1)
                .map(|rest| rest.split('"').next().unwrap());
            assert_eq!(carried, bytes);
            if let Some(carried) = carried {
                assert_eq!(
                    base64::engine::general_purpose::STANDARD
                        .decode(carried)
                        .unwrap(),
                    b"\0asm\x01\0\0\0"
                );
            }
            let body = report
                .html
                .rsplit_once("<script>")
                .unwrap()
                .1
                .split_once("</script>")
                .unwrap()
                .0;
            assert!(!body.contains(base) && !body.contains(encoded));
            if let Some(expected) = &initializer {
                assert_eq!(body, expected);
            } else {
                initializer = Some(body.to_owned());
            }
            assert_eq!(report.metadata["_richview"]["interactive"], true);
            assert_eq!(report.metadata["_richview"]["offline"], false);
        }
    }
}

#[test]
fn an_external_binding_without_a_trusted_hash_or_bytes_is_not_loaded() {
    for runtime_js in [None, Some(String::new())] {
        let report = render(
            "# Preview",
            &RenderOptions {
                runtime_url: Some("/_runtime/report".into()),
                runtime_js,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!report.html.contains("<script src="));
        assert_eq!(report.metadata["_richview"]["interactive"], false);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "security.runtime-url" && d.path == "options.runtime_js")
        );
    }
}

#[test]
fn an_external_binding_refuses_unsafe_or_non_raw_inline_wasm() {
    for (bytes, codec) in [
        ("<script>", "raw"),
        ("AA\"==", "raw"),
        ("AA==\n", "raw"),
        ("AA==", "gzip"),
    ] {
        let report = render(
            "# Preview",
            &RenderOptions {
                runtime_url: Some("https://richview.test/runtime".into()),
                runtime_js: Some("fixture".into()),
                runtime_wasm: Some(bytes.into()),
                runtime_wasm_codec: codec.into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!report.html.contains("<script src="), "{bytes:?}, {codec}");
        assert_eq!(report.metadata["_richview"]["interactive"], false);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "security.runtime-url")
        );
    }
}

#[test]
fn description_feeds_the_deck_and_the_preview_tags_and_nothing_else_does() {
    let page = html(
        "---\ntitle: T\ndescription: What it shows\nsubtitle: Old\nprolog: Old\n---\n\nBody.\n",
    );
    assert!(page.contains("<p class=\"rv-report__lede\">What it shows</p>"));
    assert!(page.contains("<meta name=\"description\" content=\"What it shows\">"));
    assert!(page.contains("<meta property=\"og:description\" content=\"What it shows\">"));
    assert!(page.contains("<meta property=\"og:title\" content=\"T\">"));
    assert!(page.contains("<meta property=\"og:type\" content=\"article\">"));
    assert!(!page.contains("og:image"));
    for gone in [
        "class=\"rv-report__subtitle\"",
        "class=\"rv-report__prolog\"",
        ">Old<",
    ] {
        assert!(!page.contains(gone), "{gone}");
    }
    let bare = html("---\ntitle: T\n---\n\nBody.\n");
    assert!(!bare.contains("name=\"description\"") && !bare.contains("og:description"));
}

#[test]
fn the_byline_dates_follow_the_report_language() {
    for (lang, shown) in [
        ("en", "21 October 2026"),
        ("en-US", "October 21, 2026"),
        ("cs-CZ", "21. října 2026"),
        ("de-DE", "21. Oktober 2026"),
        ("ja-JP", "2026年10月21日"),
    ] {
        let page = html(&format!(
            "---\ntitle: T\nlang: {lang}\nauthor: Ada\ndate: 2026-10-21\n---\n"
        ));
        assert!(
            page.contains(&format!("<time datetime=\"2026-10-21\">{shown}</time>")),
            "{lang}"
        );
        assert!(page.contains("<span>Ada</span>"));
    }
    assert!(html("---\ntitle: T\ndate: last spring\n---\n").contains("<time>last spring</time>"));
}

#[test]
fn a_report_ends_with_the_colophon_under_the_footer() {
    let page = html("# T");
    assert!(page.contains(
        "<footer class=\"rv-report__footer\"><p class=\"rv-report__colophon\">Made with <a href=\"https://richview.ai\">RichView</a></p></footer>"
    ));
    assert!(page.contains("<h1 class=\"rv-report__title\">Untitled report</h1>"));
}

#[test]
fn a_header_image_without_alt_is_decorative_and_warns() {
    let mut options = RenderOptions::default();
    options
        .data
        .insert("c.png".into(), "data:image/png;base64,AAAA".into());
    let report = render("---\ntitle: Sea\ntitle_image: c.png\n---\n", &options).unwrap();
    assert!(report.html.contains("alt=\"\""));
    assert!(!report.html.contains("alt=\"Sea\""));
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "image.alt-missing")
    );
}

#[test]
fn unreadable_frontmatter_leaves_a_notice_instead_of_the_title_block() {
    let report = render(
        "---\ntitle: [unclosed\n---\n\nBody stays.\n",
        &RenderOptions::default(),
    )
    .unwrap();
    assert!(
        report.html.contains("Frontmatter not read:"),
        "{}",
        report.html
    );
    assert!(report.html.contains("Body stays."));
    assert!(!report.html.contains("<h1"));
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.level == "error" && d.code == "frontmatter.yaml")
    );
}
