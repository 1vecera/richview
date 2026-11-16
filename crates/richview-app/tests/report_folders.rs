//! Observable filesystem/CLI contracts, including failures before any successful output.
use richview_app::resolver;
use std::{fs, path::PathBuf, process::Command};
struct Folder(PathBuf);
impl Folder {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::current_dir().unwrap().join("tmp").join(format!(
            "folder-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn cli(&self, verb: &str, path: &std::path::Path) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_richview"))
            .args([verb, path.to_str().unwrap(), "--format", "json"])
            .current_dir(&self.0)
            .output()
            .unwrap()
    }
}
impl Drop for Folder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
#[ignore = "release: needs the real embedded report runtime staged by richview-build (ARC-07)"]
fn pdf_export_keeps_the_output_boundary_and_preserves_previous_output_on_failure() {
    let folder = Folder::new();
    let source = folder.0.join("report.rmd");
    fs::write(&source, "# Local export\n").unwrap();
    let pdf = resolver::pdf_output_path(&source, None).unwrap();
    assert_eq!(pdf, folder.0.join("Rendered/report.pdf"));
    for bad in ["../outside.pdf", "nested/report.pdf", "report.html"] {
        assert!(resolver::pdf_output_path(&source, Some(std::path::Path::new(bad))).is_err());
    }
    fs::write(&pdf, b"previous PDF bytes").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_richview"))
        .arg("generate")
        .arg(&source)
        .args(["--to", "pdf", "--format", "json", "--browser"])
        .arg(folder.0.join("missing-browser"))
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stdout).contains("export.browser-missing"));
    assert_eq!(fs::read(&pdf).unwrap(), b"previous PDF bytes");
    richview_app::export_pdf::write_atomically(&pdf, b"complete replacement").unwrap();
    assert_eq!(fs::read(&pdf).unwrap(), b"complete replacement");
    assert!(!fs::read_dir(pdf.parent().unwrap()).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")
    }));
}

#[test]
#[ignore = "release: needs the real embedded report runtime staged by richview-build (ARC-07)"]
fn cli_refuses_missing_shared_header_and_escaping_reports_with_corrections() {
    let f = Folder::new();
    let source = f.0.join("report.rmd");
    for verb in ["generate", "validate", "check"] {
        let result = f.cli(verb, &source);
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stdout).contains("report.root.missing"));
    }
    fs::write(&source, "# Saved report").unwrap();
    fs::write(f.0.join("second.rmd"), "# Another report").unwrap();
    let error = f.cli("generate", &source);
    assert!(!error.status.success());
    let text = String::from_utf8_lossy(&error.stdout);
    assert!(text.contains("report.root.shared") && text.contains("Move each"));
    assert!(!f.0.join("Rendered/report.html").exists());
    fs::remove_file(f.0.join("second.rmd")).unwrap();
    fs::write(&source, "---\ndata: {old: {inline: [{x: 1}]}}\n---\n").unwrap();
    let error = f.cli("generate", &source);
    assert!(!error.status.success());
    assert!(String::from_utf8_lossy(&error.stdout).contains("frontmatter.data"));
    fs::write(&source, "```table\ndata: {csv: ../outside.csv}\n```\n").unwrap();
    let error = f.cli("generate", &source);
    assert!(!error.status.success());
    assert!(String::from_utf8_lossy(&error.stdout).contains("report.asset.escape"));
    fs::write(
        &source,
        "```table\ndata: {csv: '%2e%2e/outside.csv'}\n```\n",
    )
    .unwrap();
    assert!(!f.cli("generate", &source).status.success());
}

#[test]
fn portable_export_keeps_source_style_and_data_and_places_required_links_inside_rendered() {
    let f = Folder::new();
    fs::create_dir(f.0.join("data")).unwrap();
    let csv = "id,value\n001,100\n002,140\n";
    fs::write(f.0.join("data/sales.csv"), csv).unwrap();
    fs::write(
        f.0.join("report.rstyle.yaml"),
        "tokens: {accent: '#a52a2a'}\n",
    )
    .unwrap();
    let source = "---\nstyles: [scientific, report.rstyle.yaml]\n---\n\n[Original observations](data/sales.csv)\n\n```table\ndata: {csv: data/sales.csv}\n```\n";
    let path = f.0.join("quarter.rmd");
    fs::write(&path, source).unwrap();
    let (document, options) =
        resolver::resolve_document_options_policy(source, &path, &[], false, false).unwrap();
    let report = richview_core::document::render_document(document, &options).unwrap();
    let output = resolver::export_report(&path, None, &report, &options).unwrap();
    assert_eq!(output, f.0.join("Rendered/quarter.html"));
    let html = fs::read_to_string(output).unwrap();
    assert!(html.contains("href=\"data/sales.csv\""));
    assert!(html.contains("#a52a2a"));
    assert!(html.contains("data:font/woff2;base64,"));
    assert!(html.contains("001"));
    assert_eq!(
        fs::read_to_string(f.0.join("Rendered/data/sales.csv")).unwrap(),
        csv
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), source);
    let no_style = source.replace("styles: [scientific, report.rstyle.yaml]\n", "");
    fs::write(&path, &no_style).unwrap();
    fs::remove_file(f.0.join("report.rstyle.yaml")).unwrap();
    let report = {
        let source = fs::read_to_string(&path).unwrap();
        let (document, options) =
            resolver::resolve_document_options_policy(&source, &path, &[], false, false).unwrap();
        richview_core::document::render_document(document, &options).unwrap()
    };
    assert!(report.html.contains("001"));
    assert!(
        resolver::output_path(&path, Some(&f.0.join("outside.html")))
            .unwrap_err()
            .contains("report.output.escape")
    );
    #[cfg(unix)]
    {
        fs::remove_dir_all(f.0.join("Rendered")).unwrap();
        let other = Folder::new();
        std::os::unix::fs::symlink(&other.0, f.0.join("Rendered")).unwrap();
        assert!(
            resolver::output_path(&path, None)
                .unwrap_err()
                .contains("report.output.escape")
        );
        assert!(!other.0.join("quarter.html").exists());
    }
}

#[test]
#[cfg(unix)]
fn source_and_resource_symlinks_cannot_escape_the_dedicated_root() {
    let report = Folder::new();
    let other = Folder::new();
    fs::write(other.0.join("source.rmd"), "# Other report").unwrap();
    let path = report.0.join("source.rmd");
    std::os::unix::fs::symlink(other.0.join("source.rmd"), &path).unwrap();
    assert!(
        resolver::report_folder(&path)
            .unwrap_err()
            .contains("report.root.escape")
    );
    // An escaping report root is refused whole: unlike a draft with errors, nothing is written
    // (FEA-RP-30, S27-578).
    let generated = report.cli("generate", &path);
    assert!(!generated.status.success());
    assert!(String::from_utf8_lossy(&generated.stdout).contains("report.root.escape"));
    assert!(!report.0.join("Rendered").exists() && !other.0.join("Rendered").exists());
    fs::remove_file(&path).unwrap();
    let alias = report.0.join("alias");
    std::os::unix::fs::symlink(&other.0, &alias).unwrap();
    assert!(
        resolver::report_folder(&alias.join("source.rmd"))
            .unwrap_err()
            .contains("report.root.escape")
    );
    fs::remove_file(alias).unwrap();
    fs::write(&path, "```table\ndata: {csv: data.csv}\n```\n").unwrap();
    fs::write(other.0.join("data.csv"), "x,y\nA,2").unwrap();
    std::os::unix::fs::symlink(other.0.join("data.csv"), report.0.join("data.csv")).unwrap();
    // The link is a finding on the report and the outside file is never read.
    let rendered = {
        let source = fs::read_to_string(&path).unwrap();
        let (document, options) =
            resolver::resolve_document_options_policy(&source, &path, &[], false, false).unwrap();
        richview_core::document::render_document(document, &options).unwrap()
    };
    assert!(
        rendered
            .diagnostics
            .iter()
            .any(|d| d.code == "report.asset.escape")
    );
    assert!(!rendered.html.contains("x,y"));
}

#[test]
fn init_says_what_it_wrote_and_never_overwrites() {
    // TYP-54: `init --format json` returns the result object, `output` naming the report it
    // wrote; a second run writes nothing, fails with exit 1 and says why.
    let f = Folder::new();
    let dir = f.0.join("hello");
    let first = f.cli("init", &dir);
    assert_eq!(first.status.code(), Some(0));
    let value: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    let report = dir.join("report.rmd");
    assert_eq!(
        (&value["ok"], &value["status"]),
        (&serde_json::json!(true), &serde_json::json!("clean"))
    );
    assert_eq!(value["diagnostics"], serde_json::json!([]));
    let output = PathBuf::from(value["output"].as_str().unwrap());
    assert_eq!(
        output.canonicalize().unwrap(),
        report.canonicalize().unwrap()
    );
    assert!(dir.join("data/visits.csv").is_file());
    let again = f.cli("init", &dir);
    assert_eq!(again.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&again.stdout).unwrap();
    assert_eq!(value["ok"], serde_json::json!(false), "{value}");
    assert_eq!(value["output"], serde_json::Value::Null, "{value}");
    assert!(
        value["error"].as_str().unwrap().contains("already exists"),
        "{value}"
    );
}

#[cfg(unix)]
#[test]
fn init_next_steps_work_without_path_and_quote_the_report_path() {
    let folder = Folder::new();
    let dir = folder.0.join("a report's folder");
    let output = Command::new(env!("CARGO_BIN_EXE_richview"))
        .arg("init")
        .arg(&dir)
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    let check = text
        .lines()
        .find_map(|line| line.strip_prefix("Try: "))
        .and_then(|line| line.split_once("  ·  ").map(|(_, check)| check))
        .expect("init prints both follow-up commands");
    let result = Command::new("/bin/sh")
        .arg("-c")
        .arg(format!("{check} --format json"))
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["status"], "clean");
}

#[test]
fn initialized_starter_renders_its_title_once() {
    let folder = Folder::new();
    let dir = folder.0.join("starter");
    assert!(folder.cli("init", &dir).status.success());
    let path = dir.join("report.rmd");
    let source = fs::read_to_string(&path).unwrap();
    let (document, options) =
        resolver::resolve_document_options_policy(&source, &path, &[], false, false).unwrap();
    let html = richview_core::document::render_document(document, &options)
        .unwrap()
        .html;
    assert!(html.contains("<h1 class=\"rv-report__title\">Direct visits lead our traffic</h1>"));
    assert_eq!(
        html.matches(">Direct visits lead our traffic</h").count(),
        1
    );
    assert!(html.contains("Visits this week"));
    assert!(html.contains("Direct visits lead, search follows"));
    assert!(html.contains("Make it yours"));
    assert!(dir.join("data/visits.csv").is_file());
}

/// The error findings of `result`'s JSON as (code, path, file, line, column).
fn errors(result: &std::process::Output) -> Vec<(String, String, Option<String>, u64, u64)> {
    let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    value["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["level"] == "error")
        .map(|d| {
            (
                d["code"].as_str().unwrap().to_owned(),
                d["path"].as_str().unwrap().to_owned(),
                d["file"].as_str().map(str::to_owned),
                d["line"].as_u64().unwrap_or(0),
                d["column"].as_u64().unwrap_or(0),
            )
        })
        .collect()
}

#[test]
fn a_missing_or_unreadable_style_file_is_one_finding_beside_every_other_one() {
    // FEA-RP-30: a style file the folder lacks, or one that is not UTF-8 text, is skipped with its
    // finding and the report renders without it. Before, either stopped the resolver with one
    // bare error, so no other finding showed and no draft was written.
    let f = Folder::new();
    let source = f.0.join("report.rmd");
    fs::write(
        f.0.join("latin1.rstyle.yaml"),
        b"colors:\n  accent: \"caf\xe9\"\n",
    )
    .unwrap();
    fs::write(
        &source,
        "---\ntitle: T\nstyles: [gone.rstyle.yaml, latin1.rstyle.yaml]\n---\n\nText ![a](gone.png).\n",
    )
    .unwrap();
    let text = |s: &str| s.to_owned();
    for verb in ["validate", "check"] {
        let result = f.cli(verb, &source);
        assert_eq!(result.status.code(), Some(1), "{verb}");
        let mut found = errors(&result);
        found.sort();
        assert_eq!(
            found,
            [
                (
                    text("report.asset.missing"),
                    text("frontmatter.styles[0]"),
                    None,
                    3,
                    10
                ),
                (text("report.asset.missing"), text("markdown"), None, 6, 6),
                // At the first byte that is not UTF-8, in the style file (TYP-56).
                (
                    text("style.unresolved"),
                    text("frontmatter.styles[1]"),
                    Some(text("latin1.rstyle.yaml")),
                    2,
                    15
                ),
            ],
            "{verb}: {}",
            String::from_utf8_lossy(&result.stdout)
        );
    }
}

#[test]
fn a_wrong_data_definition_is_one_finding_beside_every_other_one() {
    // FEA-RP-30 and ARC-08 (lead, 3 Nov): a table with retired request headers fails alone, and
    // validate and check return every finding once. Before, check returned only the data.local
    // finding and validate listed it twice.
    let f = Folder::new();
    let source = f.0.join("report.rmd");
    fs::write(
        &source,
        "---\ntitle: T\n---\n\n```big\nvalue: 12\n```\n\n```table\ndata:\n  inline: [{a: 1}]\n  headers: {Authorization: Bearer HEADER-SECRET}\n```\n\n```image\nsrc: photo.png\n```\n",
    )
    .unwrap();
    fs::write(f.0.join("photo.png"), b"not read").unwrap();
    let text = |s: &str| s.to_owned();
    for verb in ["validate", "check"] {
        let result = f.cli(verb, &source);
        assert_eq!(result.status.code(), Some(1), "{verb}");
        let mut found: Vec<(String, String)> = errors(&result)
            .into_iter()
            .map(|(code, path, ..)| (code, path))
            .collect();
        found.sort();
        assert_eq!(
            found,
            [
                (text("block.renamed"), text("big")),
                (text("block.schema"), text("image.alt")),
                (text("data.local"), text("table.data.headers")),
            ],
            "{verb}: {}",
            String::from_utf8_lossy(&result.stdout)
        );
        assert!(!String::from_utf8_lossy(&result.stdout).contains("HEADER-SECRET"));
    }
}

#[test]
#[ignore = "release: needs the real embedded report runtime staged by richview-build (ARC-07)"]
fn hostile_style_references_write_a_draft_that_publish_refuses() {
    // Hostile s06 under the m05b conditions (lead, 2 Nov): a script-named style, a path escape
    // and a remote URL each get their finding, the draft is written without them, and publishing
    // it is refused before any request.
    let f = Folder::new();
    let source = f.0.join("report.rmd");
    let corpus = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../richview-core/tests/hostile/s06-style-reference.rmd"
    );
    let hostile = fs::read_to_string(corpus).unwrap();
    let hostile = hostile.replacen("---\n", "---\ndescription: D\nversion: 1.0.0\nslug: t\n", 1);
    fs::write(&source, &hostile).unwrap();
    let result = f.cli("generate", &source);
    assert_eq!(result.status.code(), Some(1));
    let found: Vec<(String, String)> = errors(&result)
        .into_iter()
        .map(|(code, path, ..)| (code, path))
        .collect();
    let at = |code: &str, index: usize| (code.to_owned(), format!("frontmatter.styles[{index}]"));
    assert_eq!(
        found,
        [
            at("report.asset.missing", 0),
            at("report.asset.escape", 1),
            at("report.asset.escape", 2)
        ]
    );
    let draft = fs::read_to_string(f.0.join("Rendered/report.html")).unwrap();
    for text in ["HOSTILE_REF", "etc/passwd", "evil.example"] {
        assert!(!draft.contains(text), "the draft contains {text}");
    }
    let published = Command::new(env!("CARGO_BIN_EXE_richview"))
        .arg("publish")
        .arg(&source)
        .args([
            "--server",
            "http://127.0.0.1:9",
            "--token",
            "publish-token",
            "--new",
            "--format",
            "json",
        ])
        .env_remove("RICHVIEW_TOKEN")
        .env_remove("RICHVIEW_SERVER")
        .output()
        .unwrap();
    assert!(!published.status.success());
    let value: serde_json::Value = serde_json::from_slice(&published.stdout).unwrap();
    assert_eq!(value["ok"], serde_json::json!(false), "{value}");
    assert_eq!(value["output"], serde_json::Value::Null, "{value}");
}

#[test]
fn only_credited_unsplash_photos_may_be_remote_images() {
    let folder = Folder::new();
    let path = folder.0.join("report.rmd");
    let credit = "credit: {author: Ada, author_url: 'https://unsplash.com/@ada'}";
    let unsplash = "https://images.unsplash.com/photo-1?w=1600";
    fs::write(
        &path,
        format!("```image\nsrc: {unsplash}\nalt: A\n{credit}\n```\n"),
    )
    .unwrap();
    let report = {
        let source = fs::read_to_string(&path).unwrap();
        let (document, options) =
            resolver::resolve_document_options_policy(&source, &path, &[], false, false).unwrap();
        richview_core::document::render_document(document, &options).unwrap()
    };
    assert!(report.html.contains(&format!("<img src=\"{unsplash}\"")));
    assert!(!report.html.contains("data:image/"));
    for src in [
        "https://example.org/a.jpg",
        "https://plus.unsplash.com/photo-1",
        "https://Images.Unsplash.com/photo-1",
        "https://images.unsplash.com:8443/photo-1",
        "http://images.unsplash.com/photo-1",
    ] {
        for source in [
            format!("```image\nsrc: {src}\nalt: A\n{credit}\n```\n"),
            format!("---\ntitle: T\ntitle_image: {{src: {src}, {credit}}}\n---\n"),
        ] {
            fs::write(&path, source).unwrap();
            let rendered = {
                let source = fs::read_to_string(&path).unwrap();
                let (document, options) =
                    resolver::resolve_document_options_policy(&source, &path, &[], false, false)
                        .unwrap();
                richview_core::document::render_document(document, &options).unwrap()
            };
            // A remote address is one finding, at its field (TYP-40).
            let remote: Vec<_> = rendered
                .diagnostics
                .iter()
                .filter(|d| d.code.starts_with("report.asset."))
                .map(|d| d.code.as_str())
                .collect();
            assert_eq!(remote, ["report.asset.remote"], "{src}");
            assert!(
                !rendered.html.contains(&format!("<img src=\"{src}\"")),
                "{src}"
            );
        }
    }
}

#[test]
#[ignore = "release: needs the real embedded report runtime staged by richview-build (ARC-07)"]
fn every_cli_result_says_ok_status_and_output_and_exits_by_outcome() {
    use serde_json::{Value, json};
    let f = Folder::new();
    let source = f.0.join("report.rmd");
    let read = |out: &std::process::Output| -> Value {
        serde_json::from_slice(&out.stdout)
            .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stdout)))
    };
    let fields = |v: &Value| (v["ok"].clone(), v["status"].clone());

    // A failure with no output: exit 1, the same shape, and `output` null.
    let missing = f.cli("generate", &source);
    assert_eq!(missing.status.code(), Some(1));
    let value = read(&missing);
    assert_eq!(fields(&value), (json!(false), json!("issues")));
    assert_eq!(value["output"], Value::Null);
    assert!(
        value["diagnostics"]
            .as_array()
            .is_some_and(|d| !d.is_empty())
    );

    // A clean report: exit 0 for `check` and `generate`, which names the file it wrote.
    fs::write(&source, "---\ntitle: Results\n---\n\nBody text.\n").unwrap();
    for verb in ["check", "generate"] {
        let result = f.cli(verb, &source);
        assert_eq!(
            result.status.code(),
            Some(0),
            "{verb}: {}",
            String::from_utf8_lossy(&result.stdout)
        );
        assert_eq!(
            fields(&read(&result)),
            (json!(true), json!("clean")),
            "{verb}"
        );
    }
    let written = read(&f.cli("generate", &source))["output"].clone();
    assert!(
        written
            .as_str()
            .is_some_and(|path| path.ends_with("report.html")),
        "{written}"
    );

    // A block error: the draft is still written, and the command exits 1.
    fs::write(
        &source,
        "---\ntitle: Results\n---\n\n```chart\ntype: donut\ndata: {inline: [{x: a, y: 1}]}\nencoding: {x: x, y: y}\n```\n",
    )
    .unwrap();
    let draft = f.cli("generate", &source);
    assert_eq!(draft.status.code(), Some(1));
    let value = read(&draft);
    assert_eq!(fields(&value), (json!(false), json!("issues")));
    assert!(value["output"].is_string(), "{value}");

    // A command called wrongly exits 2.
    let wrong = Command::new(env!("CARGO_BIN_EXE_richview"))
        .args(["learn", "no-such-section"])
        .output()
        .unwrap();
    assert_eq!(wrong.status.code(), Some(2));
}

#[test]
#[ignore = "release: needs the real embedded report runtime staged by richview-build (ARC-07)"]
fn generate_writes_the_draft_and_reports_every_finding_when_a_linked_file_is_missing() {
    use serde_json::Value;
    let f = Folder::new();
    // One block error and one broken link; no chart, so no runtime is needed.
    fs::write(f.0.join("kept.csv"), "a,b\n1,2\n").unwrap();
    let source = f.0.join("report.rmd");
    fs::write(
        &source,
        "---\ntitle: Results\n---\n\n```callout\nkind: bogus\nbody: x\n```\n\n[gone](missing.csv) and [kept](kept.csv)\n",
    )
    .unwrap();
    let result = f.cli("generate", &source);
    assert_eq!(result.status.code(), Some(1));
    let value: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["ok"], false);
    let codes: Vec<&str> = value["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["code"].as_str().unwrap())
        .collect();
    assert!(
        codes.contains(&"block.schema"),
        "the block error is kept: {codes:?}"
    );
    assert!(codes.contains(&"report.asset.missing"), "{codes:?}");
    // The draft is written, the file that exists travels with it and the missing one is left unlinked.
    let written = value["output"].as_str().expect("the draft is written");
    assert!(fs::read_to_string(written).unwrap().contains("Results"));
    assert!(f.0.join("Rendered/kept.csv").is_file());
    assert!(!f.0.join("Rendered/missing.csv").exists());
}

#[test]
fn asset_messages_name_the_reference_and_never_the_hosts_folder() {
    let f = Folder::new();
    fs::write(f.0.join("outside.csv"), "a\n1\n").unwrap();
    let root = f.0.join("report");
    fs::create_dir_all(&root).unwrap();
    std::os::unix::fs::symlink(f.0.join("outside.csv"), root.join("linked.csv")).unwrap();
    for reference in ["https://example.com/d.csv", "../outside.csv", "linked.csv"] {
        let error = resolver::contained_path(&root, reference).unwrap_err();
        let findings: Vec<richview_core::document::Diagnostic> =
            serde_json::from_str(&error).unwrap();
        assert_eq!(findings.len(), 1, "{reference}: {error}");
        assert!(findings[0].message.contains(reference), "{error}");
        let host = f.0.to_string_lossy();
        assert!(
            !error.contains(host.as_ref()),
            "{reference} leaked the folder: {error}"
        );
    }
}

#[test]
fn a_missing_file_is_a_finding_and_the_rest_of_the_report_still_resolves() {
    let f = Folder::new();
    fs::write(f.0.join("tržby.csv"), "a,b\n1,2\n").unwrap();
    let source = "# R\n\n![gone](missing.png)\n\n[Sales](tržby.csv)\n";
    let path = f.0.join("r.rmd");
    fs::write(&path, source).unwrap();
    let (document, _) =
        resolver::resolve_document_options_policy(source, &path, &[], false, false).unwrap();
    assert!(
        document
            .diagnostics
            .iter()
            .any(|d| d.code == "report.asset.missing")
    );
    assert_eq!(
        resolver::linked_paths(&document)
            .into_iter()
            .collect::<Vec<_>>(),
        ["tržby.csv"]
    );
}

#[test]
fn a_wikilink_names_no_file_and_a_footnote_paragraph_names_its_files() {
    // The renderer shows a wikilink as its text and reads a footnote's indented paragraph as
    // part of the note, so exports and publications carry exactly the files the report shows.
    let f = Folder::new();
    for name in ["notes.csv", "data.csv", "photo.png", "chart.png"] {
        fs::write(f.0.join(name), "bytes").unwrap();
    }
    let source = "# R\n\nSee [[notes.csv]] and ![[photo.png]].\n\nA claim.[^1]\n\n[^1]: The note.\n\n    It links [the data](data.csv) and shows ![a chart](chart.png).\n";
    let path = f.0.join("r.rmd");
    fs::write(&path, source).unwrap();
    let (document, options) =
        resolver::resolve_document_options_policy(source, &path, &[], false, false).unwrap();
    assert_eq!(
        resolver::linked_paths(&document)
            .into_iter()
            .collect::<Vec<_>>(),
        ["data.csv"]
    );
    assert!(options.data.contains_key("chart.png"));
    assert!(!options.data.contains_key("photo.png"));
}

#[test]
fn a_report_folder_stands_for_its_one_rmd_file() {
    let f = Folder::new();
    fs::write(
        f.0.join("report.rmd"),
        "---\ntitle: Results\n---\n\nBody text.\n",
    )
    .unwrap();
    let result = f.cli("check", &f.0);
    assert_eq!(
        result.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
    fs::remove_file(f.0.join("report.rmd")).unwrap();
    fs::create_dir(f.0.join("empty")).unwrap();
    let result = f.cli("check", &f.0.join("empty"));
    assert!(!result.status.success());
    let text = String::from_utf8_lossy(&result.stderr).to_string()
        + &String::from_utf8_lossy(&result.stdout);
    assert!(text.contains("no .rmd file"), "{text}");
}

#[test]
fn publish_refuses_a_draft_with_errors_before_any_request() {
    // `generate` writes this draft and exits 1 (FEA-RP-30); publishing it is refused (TYP-54),
    // before the server is asked: nothing listens on the discard port.
    let f = Folder::new();
    let source = f.0.join("report.rmd");
    fs::write(
        &source,
        "---\ntitle: T\ndescription: D\nversion: 1.0.0\nslug: t\n---\n\n[parent](../../etc/passwd) [proto](//evil.example/HOSTILE) [missing](missing.csv)\n",
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_richview"))
        .arg("publish")
        .arg(&source)
        .args([
            "--server",
            "http://127.0.0.1:9",
            "--token",
            "publish-token",
            "--new",
            "--format",
            "json",
        ])
        .env_remove("RICHVIEW_TOKEN")
        .env_remove("RICHVIEW_SERVER")
        .output()
        .unwrap();
    assert!(!result.status.success());
    let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["ok"], serde_json::json!(false), "{value}");
    assert_eq!(value["output"], serde_json::Value::Null, "{value}");
    let codes: Vec<&str> = value["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["level"] == "error")
        .filter_map(|d| d["code"].as_str())
        .collect();
    assert!(
        codes.contains(&"report.asset.escape") && codes.contains(&"report.asset.missing"),
        "{value}"
    );
}

#[test]
fn the_environment_token_is_sent_only_to_the_environment_server() {
    let f = Folder::new();
    let source = f.0.join("report.rmd");
    fs::write(
        &source,
        "---\ntitle: T\ndescription: D\nversion: 1.0.0\nslug: t\n---\n\n# T\n",
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_richview"))
        .arg("publish")
        .arg(&source)
        .args([
            "--server",
            "https://elsewhere.invalid",
            "--new",
            "--format",
            "json",
        ])
        .env("RICHVIEW_TOKEN", "secret-token")
        .env("RICHVIEW_SERVER", "https://configured.invalid")
        .output()
        .unwrap();
    assert!(!result.status.success());
    let text = String::from_utf8_lossy(&result.stdout);
    assert!(text.contains("only sent to RICHVIEW_SERVER"), "{text}");
    assert!(!text.contains("secret-token"));
}

#[test]
fn unattended_publish_requires_a_choice_and_keeps_environment_tokens_bound() {
    let f = Folder::new();
    let source = f.0.join("report.rmd");
    fs::write(&source, "---\ntitle: T\nversion: 1.0.0\n---\n\n# T\n").unwrap();
    let run = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_richview"));
        command
            .arg("publish")
            .arg(&source)
            .args(["--new", "--format", "json"])
            .env("XDG_CONFIG_HOME", f.0.join("config"))
            .env_remove("RICHVIEW_SERVER")
            .env("RICHVIEW_TOKEN", "secret-token");
        command.output().unwrap()
    };
    let missing = run();
    assert_eq!(missing.status.code(), Some(2));
    let result: serde_json::Value = serde_json::from_slice(&missing.stdout).unwrap();
    assert!(
        result["error"]
            .as_str()
            .unwrap()
            .contains("No publish destination")
    );
    assert!(!f.0.join("config").exists());

    let config = f.0.join("config/richview");
    fs::create_dir_all(&config).unwrap();
    fs::write(config.join("publish-server.json"), "\"http://127.0.0.1:9\"").unwrap();
    let unbound = run();
    assert_eq!(unbound.status.code(), Some(2));
    let text = String::from_utf8_lossy(&unbound.stdout);
    assert!(text.contains("only sent to RICHVIEW_SERVER"), "{text}");
    assert!(!text.contains("secret-token"));
}

#[test]
fn choosing_a_new_destination_never_silently_uses_the_environment_server() {
    let f = Folder::new();
    let source = f.0.join("report.rmd");
    fs::write(&source, "---\ntitle: T\nversion: 1.0.0\n---\n\n# T\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_richview"))
        .arg("publish")
        .arg(source)
        .args(["--choose-server", "--format", "json"])
        .env("XDG_CONFIG_HOME", f.0.join("config"))
        .env("RICHVIEW_SERVER", "http://127.0.0.1:9")
        .env("RICHVIEW_TOKEN", "local-token")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        result["error"]
            .as_str()
            .unwrap()
            .contains("unset RICHVIEW_SERVER"),
        "{result}"
    );
}

#[test]
fn a_brand_font_named_by_the_reports_own_style_file_is_read_from_the_folder() {
    let f = Folder::new();
    // A minimal WOFF2-looking payload: the magic is all the host checks before the render does.
    let mut bytes = b"wOF2".to_vec();
    bytes.extend_from_slice(&[0u8; 64]);
    fs::write(f.0.join("brand.woff2"), &bytes).unwrap();
    fs::write(
        f.0.join("report.rstyle.yaml"),
        "fonts: {display: {family: Brand Sans, file: brand.woff2}}\n",
    )
    .unwrap();
    let source = "---\ntitle: T\nstyles: [report.rstyle.yaml]\n---\n\n# T\n";
    let path = f.0.join("report.rmd");
    fs::write(&path, source).unwrap();
    let (_, options) =
        resolver::resolve_document_options_policy(source, &path, &[], false, false).unwrap();
    assert!(
        options
            .data
            .get("brand.woff2")
            .is_some_and(|uri| uri.starts_with("data:font/woff2;base64,")),
        "the font file was not read"
    );
}

#[test]
fn one_absent_file_is_one_finding_without_the_operating_systems_words() {
    let f = Folder::new();
    let source = "```table\ndata: {csv: data/absent.csv}\n```\n";
    let path = f.0.join("r.rmd");
    fs::write(&path, source).unwrap();
    let (document, _) =
        resolver::resolve_document_options_policy(source, &path, &[], false, false).unwrap();
    let missing: Vec<_> = document
        .diagnostics
        .iter()
        .filter(|d| d.code == "report.asset.missing")
        .collect();
    assert_eq!(missing.len(), 1, "{missing:?}");
    assert!(!missing[0].message.contains("os error"));
}

#[test]
fn a_data_file_that_is_not_utf8_fails_only_its_block_at_its_first_bad_byte() {
    // An Excel export in Windows-1250 ("Plzeň", "město"): each block that reads one is one
    // finding at the bad byte, and the rest of the report renders (FEA-RP-30, TYP-56).
    let f = Folder::new();
    fs::create_dir_all(f.0.join("data")).unwrap();
    fs::write(f.0.join("data/sales.csv"), b"mesto,trzby\nPlze\xf2,90\n").unwrap();
    fs::write(f.0.join("data/rows.json"), b"[{\"m\xecsto\": \"Brno\"}]").unwrap();
    fs::write(f.0.join("data/ok.csv"), "city,visits\nBrno,140\n").unwrap();
    let source = "---\ntitle: R\n---\n\nIntro.\n\n```chart\ntype: bar\ntitle: Sales\ndata:\n  csv: data/sales.csv\nencoding:\n  x: mesto\n  y: trzby\n```\n\n```table\ndata: {json: data/rows.json}\n```\n\n```table\ndata: {csv: data/ok.csv}\n```\n\n```scrolly\nchart:\n  type: bar\n  title: Sales story\n  data: {csv: data/sales.csv}\n  encoding: {x: mesto, y: trzby}\nsteps:\n  - text: Start.\n```\n";
    let path = f.0.join("r.rmd");
    fs::write(&path, source).unwrap();
    let (document, options) =
        resolver::resolve_document_options_policy(source, &path, &[], false, false).unwrap();
    let report = richview_core::document::render_document(document, &options).unwrap();
    let found: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.code.starts_with("data."))
        .map(|d| {
            (
                d.code.as_str(),
                d.path.as_str(),
                d.file.as_deref(),
                d.line,
                d.column,
            )
        })
        .collect();
    assert_eq!(
        found,
        [
            (
                "data.csv",
                "chart.data.csv",
                Some("data/sales.csv"),
                Some(2),
                Some(5)
            ),
            (
                "data.json",
                "table.data.json",
                Some("data/rows.json"),
                Some(1),
                Some(5)
            ),
            (
                "data.csv",
                "scrolly.chart.data.csv",
                Some("data/sales.csv"),
                Some(2),
                Some(5)
            ),
        ],
        "{:#?}",
        report.diagnostics
    );
    assert_eq!(
        report.html.matches("is not UTF-8 text.").count(),
        3,
        "each block's notice names the cause"
    );
    assert!(report.html.contains("Intro.") && report.html.contains(">Brno<"));
}

#[cfg(not(feature = "embedded-runtime"))]
#[test]
fn generate_without_an_embedded_runtime_fails_and_writes_no_report() {
    let folder = Folder::new();
    let path = folder.0.join("report.rmd");
    fs::write(&path, "# A local report\n").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_richview"))
        .args(["generate", path.to_str().unwrap(), "--format", "json"])
        .env("RICHVIEW_PUBLIC_DIR", &folder.0)
        .current_dir(&folder.0)
        .output()
        .unwrap();
    assert!(!result.status.success());
    let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["ok"], false);
    assert_eq!(value["diagnostics"][0]["code"], "report.output.runtime");
    assert!(
        value["diagnostics"][0]["message"]
            .as_str()
            .unwrap()
            .contains("cargo run -p richview-build")
    );
    assert!(!folder.0.join("Rendered/report.html").exists());
}

#[test]
fn folder_and_output_findings_name_no_place_on_this_machine() {
    // TYP-53: a finding on the report folder sits at `report_folder`, one on the output at
    // `output`, each with a report-relative `file`, and its message names a folder or file by
    // name, never by where it is on this machine.
    let f = Folder::new();
    let places = [
        f.0.to_string_lossy().into_owned(),
        fs::canonicalize(&f.0)
            .unwrap()
            .to_string_lossy()
            .into_owned(),
    ];
    let source = f.0.join("report.rmd");
    let missing_source = resolver::report_folder(&source);
    let missing_folder = resolver::report_folder(&f.0.join("absent/report.rmd"));
    fs::create_dir_all(f.0.join("real")).unwrap();
    fs::write(f.0.join("real/report.rmd"), "# Real\n").unwrap();
    std::os::unix::fs::symlink(f.0.join("real"), f.0.join("alias")).unwrap();
    let alias = resolver::report_folder(&f.0.join("alias/report.rmd"));
    fs::write(&source, "# Saved\n").unwrap();
    fs::write(f.0.join("second.rmd"), "# Another\n").unwrap();
    let shared = resolver::report_folder(&source);
    fs::remove_file(f.0.join("second.rmd")).unwrap();
    for name in ["real/report.rmd", "alias"] {
        let _ = fs::remove_file(f.0.join(name));
    }
    fs::remove_dir_all(f.0.join("real")).unwrap();
    let outside = resolver::output_path(&source, Some(&f.0.join("out.html")));
    fs::create_dir_all(f.0.join("Rendered/taken.html")).unwrap();
    let over_folder = resolver::output_path(&source, Some(std::path::Path::new("taken.html")));
    let not_pdf = resolver::pdf_output_path(&source, Some(std::path::Path::new("report.html")));
    for (case, result, code, path, file) in [
        (
            "missing source",
            missing_source.map(|_| ()),
            "report.root.missing",
            "report_folder",
            Some("report.rmd"),
        ),
        (
            "missing folder",
            missing_folder.map(|_| ()),
            "report.root.missing",
            "report_folder",
            None,
        ),
        (
            "symlinked folder",
            alias.map(|_| ()),
            "report.root.escape",
            "report_folder",
            None,
        ),
        (
            "two sources",
            shared.map(|_| ()),
            "report.root.shared",
            "report_folder",
            None,
        ),
        (
            "output outside Rendered",
            outside.map(|_| ()),
            "report.output.escape",
            "output",
            None,
        ),
        (
            "output over a folder",
            over_folder.map(|_| ()),
            "report.output.escape",
            "output",
            Some("Rendered/taken.html"),
        ),
        (
            "PDF named .html",
            not_pdf.map(|_| ()),
            "report.output.invalid",
            "output",
            Some("Rendered/report.html"),
        ),
    ] {
        let error = result.expect_err(case);
        for place in &places {
            assert!(
                !error.contains(place.as_str()),
                "{case} names {place}: {error}"
            );
        }
        let found: Vec<serde_json::Value> =
            serde_json::from_str(&error).unwrap_or_else(|_| panic!("{case}: {error}"));
        assert_eq!(found.len(), 1, "{case}: {error}");
        assert_eq!(found[0]["code"], code, "{case}: {error}");
        assert_eq!(found[0]["path"], path, "{case}: {error}");
        assert_eq!(found[0]["file"].as_str(), file, "{case}: {error}");
    }
}
