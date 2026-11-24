use richview_publish::build;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
static SERIAL: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tmp/publish-tests")
            .join(format!(
                "{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(root.join("items/example")).unwrap();
        fs::create_dir_all(root.join("runtime")).unwrap();
        fs::write(
            root.join("runtime/richview_runtime.js"),
            "/* fixture runtime */",
        )
        .unwrap();
        fs::write(
            root.join("runtime/richview_runtime_bg.wasm"),
            b"\0asm\x01\0\0\0",
        )
        .unwrap();
        fs::write(root.join("site.yaml"),"title: Example\ndescription: Safe example publication\nbase_url: https://news.example.test\nlayout: broadsheet\nmode: external\nsections: [{id: research, title: Research, description: Evidence}]\nnavigation: [{label: Research, path: /sections/research}]\nlead: [example]\n").unwrap();
        fs::write(root.join("items/example/report.rmd"),"---\ntitle: A useful finding\nversion: 1.0.0\ndescription: Fictional values.\npublish:\n  id: example\n  slug: useful-finding\n  summary: Safe synthetic observations.\n  byline: Example desk\n  section: research\n  tags: [example]\n  published: 2026-10-18T09:00:00Z\n  updated: 2026-10-18T10:00:00Z\n  state: published\n  audience: public\n  kind: report\n  aliases: [former-address]\n---\n\nSynthetic example.\n\n```chart\ntype: bar\ntitle: Observations\ndata:\n  inline: [{label: A, value: 10}, {label: B, value: 14}]\nencoding: {x: label, y: value}\n```\n").unwrap();
        Self(root)
    }
    fn run(&self, name: &str) -> Result<richview_publish::Release, String> {
        build(
            &self.0.join("site.yaml"),
            &self.0.join(name),
            &self.0.join("runtime"),
            &"a".repeat(40),
        )
    }
    fn replace(&self, from: &str, to: &str) {
        let p = self.0.join("items/example/report.rmd");
        fs::write(&p, fs::read_to_string(&p).unwrap().replace(from, to)).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn releases_are_reproducible_and_keep_real_report_runtime() {
    let f = Fixture::new();
    let first = f.run("one").unwrap();
    let second = f.run("two").unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(first.items[0].route, "/stories/useful-finding");
    assert_eq!(first.items[0].meta.aliases, ["former-address"]);
    let html = fs::read_to_string(f.0.join("one/objects").join(&first.items[0].content)).unwrap();
    assert!(html.contains("<svg"));
    assert!(html.contains("src=\"/_runtime/"));
    assert!(html.contains("integrity=\"sha256-nWxQi/myYrLiiBIog8kO1qX1eT8vayKLvcCLV3mqUMQ=\""));
    assert!(
        !html.contains("https://news.example.test"),
        "Report runtime references must not name the site's absolute base_url"
    );
    assert!(!html.contains(env!("CARGO_MANIFEST_DIR")));
    fs::write(
        f.0.join("runtime/richview_runtime.js"),
        "/* changed runtime */",
    )
    .unwrap();
    let changed = f.run("changed").unwrap();
    let changed_html =
        fs::read_to_string(f.0.join("changed/objects").join(&changed.items[0].content)).unwrap();
    assert_ne!(first.id, changed.id);
    assert!(changed_html.contains("integrity=\"sha256-"));
    assert!(!changed_html.contains("sha256-nWxQi/myYrLiiBIog8kO1qX1eT8vayKLvcCLV3mqUMQ="));
}
#[test]
fn failed_update_preserves_the_existing_release() {
    let f = Fixture::new();
    let first = f.run("live").unwrap();
    f.replace("section: research", "section: nonexistent");
    assert!(f.run("candidate").is_err());
    assert!(!f.0.join("candidate").exists());
    let saved: richview_publish::Release =
        serde_json::from_slice(&fs::read(f.0.join("live/release.json")).unwrap()).unwrap();
    assert_eq!(saved.id, first.id);
}
#[test]
fn publication_rejects_empty_runtime_bindings() {
    let f = Fixture::new();
    fs::write(f.0.join("runtime/richview_runtime.js"), "").unwrap();
    assert!(
        f.run("broken")
            .unwrap_err()
            .contains("Runtime bindings are empty")
    );
    assert!(!f.0.join("broken").exists());
}
#[test]
fn new_audience_and_aliases_change_the_immutable_release() {
    let f = Fixture::new();
    let public = f.run("public").unwrap();
    f.replace("audience: public", "audience: members");
    f.replace("slug: useful-finding", "slug: revised-finding");
    let restricted = f.run("members").unwrap();
    assert_ne!(public.id, restricted.id);
    assert_eq!(restricted.items[0].meta.audience, "members");
    assert_eq!(restricted.items[0].meta.id, public.items[0].meta.id);
}
#[test]
fn renderer_errors_fail_the_publication_instead_of_shipping_broken_blocks() {
    let f = Fixture::new();
    f.replace("type: bar", "type: invented");
    assert!(f.run("broken").is_err());
}
#[test]
fn publication_dates_compare_instants_not_string_order() {
    let f = Fixture::new();
    f.replace(
        "updated: 2026-10-18T10:00:00Z",
        "updated: 2026-10-18T10:00:00+02:00",
    );
    assert!(f.run("backdated").is_err());
}
#[cfg(unix)]
#[test]
fn symlinked_content_cannot_import_host_files() {
    let f = Fixture::new();
    let source = f.0.join("items/example/report.rmd");
    fs::remove_file(&source).unwrap();
    std::os::unix::fs::symlink(f.0.join("site.yaml"), source).unwrap();
    assert!(f.run("escaped").unwrap_err().contains("escapes"));
}
fn object(f: &Fixture, release: &str, hash: &str) -> String {
    fs::read_to_string(f.0.join(release).join("objects").join(hash)).unwrap()
}
const FIGURE: &str =
    "  figure: {value: \"84%\", label: Completion rate in the South, as_of: Oct 2026}\n";
#[test]
fn figures_are_optional_bounded_and_rendered_for_the_front_page() {
    let f = Fixture::new();
    assert_eq!(f.run("plain").unwrap().items[0].stat, "");
    f.replace("  aliases:", &format!("{FIGURE}  aliases:"));
    let release = f.run("figure").unwrap();
    let item = &release.items[0];
    assert_eq!(item.meta.figure.as_ref().unwrap().value, "84%");
    assert!(item.stat.contains("84%") && item.stat.contains("Oct 2026"));
    assert!(item.stat.contains("href=\"/stories/useful-finding\""));
    f.replace("value: \"84%\"", "value: \"84.0000000000000001%\"");
    assert!(f.run("long").unwrap_err().contains("figure"));
    f.replace(
        "value: \"84.0000000000000001%\"",
        "value: \"84%\", unit: percent",
    );
    assert!(f.run("unknown").unwrap_err().contains("unknown field"));
}
#[test]
fn the_lead_uses_a_standalone_svg_of_the_first_chart() {
    let f = Fixture::new();
    let release = f.run("lead").unwrap();
    let item = &release.items[0];
    let svg = object(&f, "lead", &item.assets["thumbnail.svg"]);
    assert!(svg.starts_with("<svg") && svg.contains("<style>") && svg.contains("xmlns="));
    assert_eq!(
        release.objects[&item.assets["thumbnail.svg"]].content_type,
        "image/svg+xml"
    );
    assert!(item.lead.contains("/_content/example/thumbnail.svg"));
    assert!(
        item.lead
            .contains("<strong>Observations</strong></figcaption>")
    );
}
#[test]
fn a_dashboard_kind_or_snapshot_is_refused_because_publish_has_reports_and_pages_only() {
    let f = Fixture::new();
    f.replace("kind: report", "kind: dashboard");
    let error = f.run("dashboard").unwrap_err();
    assert!(
        error.contains("dashboard") && error.contains("report") && error.contains("page"),
        "{error}"
    );
    f.replace("kind: dashboard", "kind: report\n  snapshot: snapshot.json");
    assert!(f.run("snapshot").unwrap_err().contains("snapshot"));
}

const PHOTO: &str =
    "https://images.unsplash.com/photo-1559538834-8b012e20a84f?ixid=M3w&w=1600&q=80&fm=jpg&fit=max";
const CREDIT: &str = "  cover_credit:\n    author: Marek Rucinski\n    author_url: https://unsplash.com/@marulla?utm_source=richview&utm_medium=referral\n    source: Unsplash\n    source_url: https://unsplash.com/?utm_source=richview&utm_medium=referral\n";
fn remote(cover: &str, credit: &str) -> String {
    format!("  cover: \"{cover}\"\n  cover_alt: A city skyline at dusk.\n{credit}  aliases:")
}
const AUTHOR_LINK: &str = "<a href=\"https://unsplash.com/@marulla?utm_source=richview&amp;utm_medium=referral\" target=\"_blank\" rel=\"noopener noreferrer\">Marek Rucinski</a>";
const SOURCE_LINK: &str = "<a href=\"https://unsplash.com/?utm_source=richview&amp;utm_medium=referral\" target=\"_blank\" rel=\"noopener noreferrer\">Unsplash</a>";

#[test]
fn a_remote_cover_is_hotlinked_never_read_or_stored() {
    let f = Fixture::new();
    f.replace("  aliases:", &remote(PHOTO, CREDIT));
    let release = f.run("remote").unwrap();
    let item = &release.items[0];
    assert!(
        !item.assets.contains_key("cover"),
        "no photo bytes are embedded"
    );
    assert_eq!(item.meta.cover.as_deref(), Some(PHOTO));
    assert_eq!(
        item.meta.cover_credit.as_ref().unwrap().author,
        "Marek Rucinski"
    );
    assert!(
        release
            .objects
            .values()
            .all(|o| !o.content_type.starts_with("image/jpeg"))
    );
    let article = object(&f, "remote", &item.article);
    assert!(article.contains(&format!(
        "<figcaption><p class=\"credit\">Photo by {AUTHOR_LINK} on {SOURCE_LINK}</p></figcaption>"
    )));
}

#[test]
fn remote_covers_must_be_credited_unsplash_https_urls() {
    for (cover, expected) in [
        (
            "https://example.com/photo.jpg",
            "host must be images.unsplash.com",
        ),
        (
            "http://images.unsplash.com/photo-1?ixid=M3w",
            "http is not allowed",
        ),
        ("https://user:pw@images.unsplash.com/photo-1", "credentials"),
        ("https://images.unsplash.com:444/photo-1", "ports"),
        (
            "https://images.unsplash.com.example.com/photo-1",
            "host must be",
        ),
        (
            "HTTPS://images.unsplash.com/photo-1",
            "use lowercase https://images.unsplash.com",
        ),
        (
            "https://IMAGES.unsplash.com/photo-1",
            "use lowercase https://images.unsplash.com",
        ),
    ] {
        let f = Fixture::new();
        f.replace("  aliases:", &remote(cover, CREDIT));
        let error = f.run("bad").unwrap_err();
        assert!(error.contains(expected), "{cover}: {error}");
        assert!(!f.0.join("bad").exists());
    }
    let f = Fixture::new();
    f.replace("  aliases:", &remote(PHOTO, ""));
    assert!(
        f.run("uncredited")
            .unwrap_err()
            .contains("needs cover_credit")
    );
    f.replace("  cover:", "  # cover:");
    f.replace(
        "  cover_alt: A city skyline at dusk.\n  aliases:",
        &format!("  cover_alt: A.\n{CREDIT}  aliases:"),
    );
    assert!(f.run("orphan").unwrap_err().contains("needs a cover"));
    let f = Fixture::new();
    f.replace(
        "  aliases:",
        &format!("  cover: \"{PHOTO}\"\n{CREDIT}  aliases:"),
    );
    assert!(f.run("no-alt").unwrap_err().contains("cover_alt"));
}

#[test]
fn cover_credits_need_an_author_known_keys_and_web_addresses() {
    for (credit, expected) in [
        (
            "  cover_credit:\n    author_url: https://unsplash.com/@a\n",
            "author",
        ),
        (
            "  cover_credit:\n    author: A\n    author_url: javascript:alert(1)\n",
            "author_url",
        ),
        (
            "  cover_credit:\n    author: A\n    source_url: ftp://unsplash.com\n",
            "source_url",
        ),
        (
            "  cover_credit:\n    author: A\n    licence: CC0\n",
            "unknown field",
        ),
    ] {
        let f = Fixture::new();
        f.replace("  aliases:", &remote(PHOTO, credit));
        let error = f.run("bad").unwrap_err();
        assert!(error.contains(expected), "{credit}: {error}");
    }
}

#[test]
fn story_lead_and_rows_show_the_photo_with_a_credit_that_links_to_unsplash() {
    let f = Fixture::new();
    f.replace("  aliases:", &remote(PHOTO, CREDIT));
    let release = f.run("photo").unwrap();
    let item = &release.items[0];
    let article = object(&f, "photo", &item.article);
    // Story: a figure in the header, before and outside the report frame.
    let figure = article.find("<figure class=\"story-cover\">").unwrap();
    let head = article.find("<header class=\"story-head\">").unwrap();
    assert!(head < figure && figure < head + article[head..].find("</header>").unwrap());
    assert!(figure < article.find("<iframe").unwrap());
    assert!(article.contains("ixid=M3w") && article.contains("&amp;w=1600"));
    assert!(article.contains("width=\"1600\" height=\"900\""));
    assert!(article.contains("alt=\"A city skyline at dusk.\""));
    assert!(article.contains("https://images.unsplash.com/photo-1559538834-8b012e20a84f"));
    // Lead keeps its chart and gains the photo with its credit.
    assert!(item.lead.contains("<figure class=\"lead-photo\">"));
    assert!(
        item.lead
            .contains(&format!("Photo by {AUTHOR_LINK} on {SOURCE_LINK}"))
    );
    assert!(item.lead.contains("class=\"lead-chart\"") && item.lead.contains("lead--photo"));
    // Row: small thumbnail from the remote URL and a compact credit.
    assert!(item.card.contains("w=240\""));
    assert!(item.card.contains(&format!(
        "<p class=\"row-credit\">Photo: {AUTHOR_LINK} / {SOURCE_LINK}</p>"
    )));
    for html in [&article, &item.lead, &item.card] {
        assert!(!html.contains("/_content/example/cover"), "{html}");
        assert!(
            !html.contains("w=1600&w=") && !html.contains("&w=240&amp;w"),
            "{html}"
        );
    }
}

#[test]
fn credit_text_is_escaped_and_a_missing_source_still_names_unsplash() {
    let f = Fixture::new();
    f.replace(
        "  aliases:",
        &remote(
            PHOTO,
            "  cover_credit:\n    author: \"<img src=x onerror=alert(1)> & Co\"\n    author_url: https://unsplash.com/@x\n",
        ),
    );
    let item = &f.run("escaped").unwrap().items[0];
    for html in [&item.lead, &item.card] {
        assert!(
            html.contains("&lt;img src=x onerror=alert(1)&gt; &amp; Co"),
            "{html}"
        );
        assert!(!html.contains("<img src=x"), "{html}");
        assert!(html.contains("utm_source=richview&amp;utm_medium=referral\" target=\"_blank\" rel=\"noopener noreferrer\">Unsplash</a>"), "{html}");
    }
}

#[test]
fn the_report_frame_may_open_credit_links_in_a_new_tab() {
    let f = Fixture::new();
    let release = f.run("frame").unwrap();
    let article = object(&f, "frame", &release.items[0].article);
    assert!(article.contains(
        "sandbox=\"allow-scripts allow-downloads allow-popups allow-popups-to-escape-sandbox\""
    ));
    assert!(!article.contains("allow-same-origin"));
}

#[test]
fn a_remote_photo_must_be_credited_to_unsplash_with_unsplash_links() {
    for (credit, expected) in [
        (
            "  cover_credit:\n    author: A\n    author_url: https://unsplash.com/@a\n    source: Pexels\n",
            "credited to Unsplash",
        ),
        (
            "  cover_credit:\n    author: A\n",
            "needs cover_credit author_url",
        ),
        (
            "  cover_credit:\n    author: A\n    author_url: https://evil.test/@a\n",
            "author_url must be",
        ),
        (
            "  cover_credit:\n    author: A\n    author_url: http://unsplash.com/@a\n",
            "author_url must be",
        ),
        (
            "  cover_credit:\n    author: A\n    author_url: https://unsplash.com/@a\n    source_url: https://evil.test\n",
            "source_url must be",
        ),
    ] {
        let f = Fixture::new();
        f.replace("  aliases:", &remote(PHOTO, credit));
        let error = f.run("bad").unwrap_err();
        assert!(error.contains(expected), "{credit}: {error}");
    }
    let f = Fixture::new();
    let credit = "  cover_credit:\n    author: A\n    author_url: https://unsplash.com/@a\n    source: unsplash\n";
    f.replace("  aliases:", &remote(PHOTO, credit));
    let item = &f.run("ok").unwrap().items[0];
    assert!(item.lead.contains(">Unsplash</a>") && !item.lead.contains(">unsplash</a>"));
}

#[test]
fn a_local_cover_stays_a_list_thumbnail_without_a_hero_treatment() {
    let f = Fixture::new();
    fs::write(
        f.0.join("items/example/cover.svg"),
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 8 5\"/>",
    )
    .unwrap();
    f.replace(
        "  aliases:",
        "  cover: cover.svg\n  cover_alt: A diagram.\n  aliases:",
    );
    let release = f.run("local").unwrap();
    let item = &release.items[0];
    assert!(item.assets.contains_key("cover"));
    let article = object(&f, "local", &item.article);
    assert!(!article.contains("<figure class=\"story-cover") && !article.contains("<img "));
    assert!(!item.lead.contains("lead-photo") && !item.lead.contains("lead--photo"));
    assert!(item.card.contains("/_content/example/cover") && !item.card.contains("row-credit"));
    // A name with a colon is a file, not a URL.
    let f = Fixture::new();
    f.replace(
        "  aliases:",
        "  cover: \"cover:v2.png\"\n  cover_alt: A.\n  aliases:",
    );
    let error = f.run("colon").unwrap_err();
    assert!(!error.contains("Remote"), "{error}");
}

#[test]
fn only_the_first_lead_and_the_story_photo_ask_for_high_fetch_priority() {
    let f = Fixture::new();
    f.replace("  aliases:", &remote(PHOTO, CREDIT));
    let item = &f.run("first").unwrap().items[0];
    assert!(item.lead.contains("fetchpriority=\"high\""));
    assert!(object(&f, "first", &item.article).contains("fetchpriority=\"high\""));
    // With another item leading, this photo is no longer the first lead.
    fs::create_dir_all(f.0.join("items/other")).unwrap();
    let source = fs::read_to_string(f.0.join("items/example/report.rmd")).unwrap();
    fs::write(
        f.0.join("items/other/report.rmd"),
        source
            .replace("id: example", "id: other")
            .replace("slug: useful-finding", "slug: other-finding")
            .replace("former-address", "other-address"),
    )
    .unwrap();
    let site = fs::read_to_string(f.0.join("site.yaml")).unwrap();
    fs::write(
        f.0.join("site.yaml"),
        site.replace("lead: [example]", "lead: [other, example]"),
    )
    .unwrap();
    let release = f.run("second").unwrap();
    let example = release
        .items
        .iter()
        .find(|i| i.meta.id == "example")
        .unwrap();
    assert!(example.lead.contains("lead-photo") && !example.lead.contains("fetchpriority"));
    assert!(object(&f, "second", &example.article).contains("fetchpriority=\"high\""));
}

fn share_png(f: &Fixture, name: &str) -> Vec<u8> {
    let release = f.run(name).unwrap();
    let hash = &release.items[0].assets["og.png"];
    assert_eq!(release.objects[hash].content_type, "image/png");
    let bytes = fs::read(f.0.join(name).join("objects").join(hash)).unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(&bytes[12..16], b"IHDR");
    let n = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
    assert_eq!((n(16), n(20)), (1200, 630));
    assert_eq!(release.objects[hash].bytes, bytes.len());
    bytes
}

#[test]
fn every_published_story_gets_a_reproducible_1200_by_630_share_image() {
    let f = Fixture::new();
    let chart = share_png(&f, "chart-a");
    assert_eq!(chart, share_png(&f, "chart-b"));
    // The stored article holds only the slot: the server decides per reader whether to advertise it.
    let release = f.run("slot").unwrap();
    let article = object(&f, "slot", &release.items[0].article);
    assert!(article.contains(richview_publish::SOCIAL_SLOT) && !article.contains("og:image"));
    // No chart, but a local cover: the cover leads.
    f.replace("```chart", "```text");
    fs::write(
        f.0.join("items/example/cover.svg"),
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 8 5\"><rect width=\"8\" height=\"5\" fill=\"#c0392b\"/></svg>",
    )
    .unwrap();
    f.replace(
        "  aliases:",
        "  cover: cover.svg\n  cover_alt: A diagram.\n  aliases:",
    );
    let cover = share_png(&f, "cover");
    assert_ne!(cover, chart);
    // Neither: a title-only card.
    f.replace("  cover: cover.svg\n  cover_alt: A diagram.\n", "");
    let plain = share_png(&f, "plain");
    assert!(plain != chart && plain != cover);
}

#[test]
fn published_report_and_share_image_include_the_company_logo() {
    let f = Fixture::new();
    f.replace("---\ntitle:", "---\nstyles: [company.rstyle.yaml]\ntitle:");
    fs::write(
        f.0.join("items/example/company.rstyle.yaml"),
        "preset: air\ncolors: {brand: '#0B6E69'}\ncompany: {name: Example Co}\n",
    )
    .unwrap();
    let plain = share_png(&f, "plain-company");
    fs::write(
        f.0.join("items/example/company.rstyle.yaml"),
        "preset: air\ncolors: {brand: '#0B6E69'}\ncompany: {name: Example Co, logo: logo.png}\n",
    )
    .unwrap();
    fs::write(
        f.0.join("items/example/logo.png"),
        include_bytes!("fixtures/company-logo.png"),
    )
    .unwrap();
    let branded = share_png(&f, "branded-company");
    assert!(plain != branded, "the logo must affect the share image");
    fs::write(
        f.0.join("items/example/company.rstyle.yaml"),
        "preset: sleek\ncolors: {brand: '#0B6E69'}\ncompany: {name: Example Co, logo: logo.png}\n",
    )
    .unwrap();
    let dark = share_png(&f, "dark-company");
    assert!(
        branded != dark,
        "the share card follows the company base style"
    );
    let release: richview_publish::Release =
        serde_json::from_slice(&fs::read(f.0.join("branded-company/release.json")).unwrap())
            .unwrap();
    let article = object(&f, "branded-company", &release.items[0].content);
    assert!(article.contains("class=\"rv-report__logo") && article.contains("alt=\"Example Co\""));
}

#[test]
fn company_accent_reaches_the_published_share_card_without_a_logo() {
    let f = Fixture::new();
    f.replace("---\ntitle:", "---\nstyles: [company.rstyle.yaml]\ntitle:");
    f.replace("```chart", "```text");
    let style = f.0.join("items/example/company.rstyle.yaml");
    fs::write(&style, "preset: brutalist\n").unwrap();
    let plain = share_png(&f, "plain-accent");
    fs::write(&style, "preset: brutalist\ncolors: {brand: '#0B6E69'}\n").unwrap();
    let branded = share_png(&f, "brand-accent");
    assert!(
        plain != branded,
        "the derived accent must reach the share card"
    );
}

// FEA-BR-23: layouts, site limits, unknown keys and the embed refusal.
const SITE: &str = "title: Example\ndescription: Safe example publication\nbase_url: https://news.example.test\nmode: external\n";
fn site_with(f: &Fixture, layout: &str, rest: &str) {
    fs::write(
        f.0.join("site.yaml"),
        format!("{SITE}layout: {layout}\n{rest}"),
    )
    .unwrap();
}
fn sections(count: usize) -> String {
    let all = (0..count)
        .map(|n| format!("{{id: s{n}, title: Section {n}, description: d}}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("sections: [{all}]\n")
}
#[test]
fn both_layouts_build_and_each_front_page_carries_its_own_layout() {
    for layout in ["broadsheet", "dispatch"] {
        let f = Fixture::new();
        site_with(
            &f,
            layout,
            "sections: [{id: research, title: Research, description: Evidence}]\nnavigation: []\nlead: [example]\n",
        );
        let release = f.run("out").unwrap();
        let front = fs::read_dir(f.0.join("out/objects"))
            .unwrap()
            .filter_map(|e| fs::read_to_string(e.unwrap().path()).ok())
            .find(|html| html.contains(&format!("<body class=\"{layout}\"")));
        assert!(
            front.is_some(),
            "{layout}: no page carries the layout class; items: {}",
            release.items.len()
        );
    }
    let f = Fixture::new();
    site_with(
        &f,
        "tabloid",
        "sections: [{id: research, title: Research, description: d}]\nnavigation: []\nlead: []\n",
    );
    assert!(f.run("out").unwrap_err().contains("tabloid"));
}
#[test]
fn a_site_has_at_most_20_sections_12_links_and_4_leads() {
    let nav = |count: usize| {
        let all = (0..count)
            .map(|n| format!("{{label: L{n}, path: /sections/s0}}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("navigation: [{all}]\n")
    };
    let f = Fixture::new();
    f.replace("section: research", "section: s0");
    site_with(
        &f,
        "broadsheet",
        &format!("{}{}lead: [example]\n", sections(20), nav(12)),
    );
    f.run("ok")
        .expect("20 sections, 12 links and 1 lead are within the limits");
    site_with(
        &f,
        "broadsheet",
        &format!("{}navigation: []\nlead: []\n", sections(21)),
    );
    assert!(f.run("sections").unwrap_err().contains("20 sections"));
    site_with(
        &f,
        "broadsheet",
        &format!("{}{}lead: []\n", sections(1), nav(13)),
    );
    assert!(f.run("links").unwrap_err().contains("12 navigation links"));
    site_with(
        &f,
        "broadsheet",
        &format!("{}navigation: []\nlead: [a, b, c, d, e]\n", sections(1)),
    );
    assert!(f.run("leads").unwrap_err().contains("4 lead items"));
}
#[test]
fn an_unknown_site_key_fails_the_build() {
    let f = Fixture::new();
    site_with(
        &f,
        "broadsheet",
        "sections: [{id: research, title: Research, description: d}]\nnavigation: []\nlead: []\nbogus_key: 1\n",
    );
    let error = f.run("out").unwrap_err();
    assert!(
        error.contains("bogus_key") || error.contains("unknown"),
        "{error}"
    );
}
#[test]
fn an_item_with_an_embed_block_fails_the_build() {
    let f = Fixture::new();
    f.replace(
        "Synthetic example.",
        "```embed\nurl: https://www.youtube.com/embed/abc\ncaption: A talk\n```",
    );
    let error = f.run("out").unwrap_err();
    assert!(error.contains("embed"), "{error}");
}

// FEA-BR-49: the size limits fail with a named error and the previous release stays live.
fn live_release(f: &Fixture) -> String {
    let saved: richview_publish::Release =
        serde_json::from_slice(&fs::read(f.0.join("live/release.json")).unwrap()).unwrap();
    saved.id
}
#[test]
fn more_than_1000_item_folders_fail_and_the_previous_release_stays_live() {
    let f = Fixture::new();
    let first = f.run("live").unwrap();
    for n in 0..1000 {
        fs::create_dir_all(f.0.join(format!("items/extra-{n}"))).unwrap();
    }
    let error = f.run("candidate").unwrap_err();
    assert!(error.contains("1000 item folders"), "{error}");
    assert!(!f.0.join("candidate").exists());
    assert_eq!(live_release(&f), first.id);
}
#[test]
fn an_item_file_over_20_mib_fails_and_the_previous_release_stays_live() {
    let f = Fixture::new();
    let first = f.run("live").unwrap();
    let path = f.0.join("items/example/report.rmd");
    let mut source = fs::read_to_string(&path).unwrap();
    source.push_str(&"x".repeat(21 * 1024 * 1024));
    fs::write(&path, source).unwrap();
    let error = f.run("candidate").unwrap_err();
    assert!(error.contains("20 MiB"), "{error}");
    assert!(!f.0.join("candidate").exists());
    assert_eq!(live_release(&f), first.id);
}

#[test]
fn cached_data_publishes_reproducibly_without_a_connector_or_live_source() {
    let f = Fixture::new();
    let trusted = f.0.join("host-connectors.toml");
    fs::write(&trusted, "cache_root = \"server-snapshots\"\n").unwrap();
    let registry = richview_app::connectors::config::load_snapshot_registry(
        &f.0.join("items/example"),
        Some(&trusted),
    )
    .unwrap();
    let fingerprint = "a".repeat(64);
    let snapshot = richview_app::connectors::DatasetSnapshot {
        rows: vec![
            serde_json::json!({"label":"A","value":10}),
            serde_json::json!({"label":"B","value":14}),
        ],
        schema: vec![],
        provenance: richview_app::connectors::Provenance {
            connector: "offline-warehouse".into(),
            source: richview_app::connectors::spec::ProvenanceSource::Sql,
            fetched_at: "2026-10-18T09:00:00Z".into(),
            fingerprint: fingerprint.clone(),
            row_count: 2,
        },
        cache: None,
    };
    registry.store().save(&fingerprint, &snapshot).unwrap();
    f.replace(
        "inline: [{label: A, value: 10}, {label: B, value: 14}]",
        &format!("cache: {{key: '{fingerprint}'}}"),
    );
    assert!(!f.0.join("items/example/report.connectors.yaml").exists());
    let build = |output: &str| {
        richview_publish::build_with_connectors(
            &f.0.join("site.yaml"),
            &f.0.join(output),
            &f.0.join("runtime"),
            &"a".repeat(40),
            Some(&trusted),
        )
        .unwrap()
    };
    let first = build("cached-one");
    let second = build("cached-two");
    assert_eq!(first.id, second.id);
    let html =
        fs::read_to_string(f.0.join("cached-one/objects").join(&first.items[0].content)).unwrap();
    assert!(html.contains("<svg") && html.contains("14"));
    assert!(!html.contains("server-snapshots"));
}

#[test]
fn publication_refuses_live_queries_instead_of_materializing_during_the_build() {
    let f = Fixture::new();
    f.replace(
        "inline: [{label: A, value: 10}, {label: B, value: 14}]",
        "sql: {connector: local, inline: 'SELECT 1 AS value'}",
    );
    let error = f.run("live").unwrap_err();
    assert!(
        error.contains("stored data.cache snapshots") && error.contains("materialize"),
        "{error}"
    );
    assert!(!f.0.join("live").exists());
}
