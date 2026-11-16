//! The hostile-input corpus (DEL-56, ARC-13, FEA-RP-14, OPN-16).
//!
//! Every `.rmd` in `tests/hostile/` is source an attacker could write: raw HTML, script-bearing
//! text in every field, hostile link and media targets, poisoned style files. The corpus index
//! (`tests/hostile/index.json`) names each case, the sink it attacks and the diagnostics it must
//! produce. Each case renders in every builtin style through its screen scheme and
//! the print path, and the output is parsed as HTML: no script but the report's own data
//! elements, no event handler, no script URL, no remote media, no source-chosen runtime id or
//! class, no hostile marker outside escaped text, and every expected diagnostic present.
//!
//! Cases that need a browser (the runtime hydrating, a tab link selecting a panel, a CSP
//! blocking an inline script) are not here; they belong to the browser tier.

use richview_core::builtin_style_names;
use richview_core::document::{Diagnostic, Output, RenderOptions, render};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/hostile")
}

struct Case {
    name: String,
    sink: String,
    source: String,
    /// Sibling files (style YAML, CSV) the host would resolve, keyed by their reference.
    files: BTreeMap<String, String>,
    expect: Vec<String>,
    refused: bool,
    absent: Vec<String>,
    present: Vec<String>,
    forbidden_ids: Vec<String>,
    forbidden_classes: Vec<String>,
    tab_ids: Vec<String>,
}

fn strings(entry: &Value, key: &str) -> Vec<String> {
    entry[key]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn cases() -> Vec<Case> {
    let dir = corpus();
    let index: Value =
        serde_json::from_str(&fs::read_to_string(dir.join("index.json")).expect("index.json"))
            .expect("index.json is JSON");
    let listed: Vec<Case> = index["cases"]
        .as_array()
        .expect("cases array")
        .iter()
        .map(|entry| {
            let name = entry["case"].as_str().expect("case name").to_owned();
            let source = fs::read_to_string(dir.join(format!("{name}.rmd")))
                .unwrap_or_else(|_| panic!("{name}.rmd"));
            let mut files = BTreeMap::new();
            for sibling in fs::read_dir(&dir).unwrap().flatten() {
                let file = sibling.file_name().to_string_lossy().into_owned();
                if file.starts_with(&format!("{name}.")) && !file.ends_with(".rmd") {
                    files.insert(file, fs::read_to_string(sibling.path()).unwrap());
                }
            }
            Case {
                sink: entry["sink"].as_str().expect("sink").to_owned(),
                expect: strings(entry, "expect"),
                refused: entry["outcome"] == "refused",
                absent: strings(entry, "absent"),
                present: strings(entry, "present"),
                forbidden_ids: strings(entry, "forbidden_ids"),
                forbidden_classes: strings(entry, "forbidden_classes"),
                tab_ids: strings(entry, "tab_ids"),
                name,
                source,
                files,
            }
        })
        .collect();
    // The index and the directory list the same cases.
    let on_disk: BTreeSet<String> = fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .filter_map(|entry| {
            let file = entry.file_name().to_string_lossy().into_owned();
            file.strip_suffix(".rmd").map(str::to_owned)
        })
        .collect();
    let indexed: BTreeSet<String> = listed.iter().map(|case| case.name.clone()).collect();
    assert_eq!(
        on_disk, indexed,
        "tests/hostile/index.json lists every case"
    );
    listed
}

/// One way to render: the style's own screen scheme, or the paged print path.
#[derive(Clone, Copy)]
struct Mode {
    label: &'static str,
    output: Output,
}

const MODES: [Mode; 2] = [
    Mode {
        label: "screen",
        output: Output::Report,
    },
    Mode {
        label: "print",
        output: Output::Pdf,
    },
];

fn options(case: &Case, style: &str, mode: Mode) -> RenderOptions {
    in_folder(RenderOptions {
        styles: vec![style.to_owned()],
        data: case.files.clone(),
        output: mode.output,
        source: Some("</script><script>HOSTILE_SOURCE_OPTION()</script>".into()),
        ..RenderOptions::default()
    })
}

// ---- A strict HTML reader. Anything it cannot place is a finding.

#[derive(Debug)]
enum Token {
    Text(String),
    Open {
        name: String,
        attrs: Vec<(String, String)>,
        raw_text: Option<String>,
    },
    Close(String),
    Comment(String),
    Declaration(String),
}

fn read_html(html: &str) -> Result<Vec<Token>, String> {
    let bytes = html.as_bytes();
    let mut tokens = vec![];
    let mut i = 0;
    while i < html.len() {
        let Some(open) = html[i..].find('<').map(|n| n + i) else {
            tokens.push(Token::Text(html[i..].to_owned()));
            break;
        };
        if open > i {
            tokens.push(Token::Text(html[i..open].to_owned()));
        }
        let rest = &html[open..];
        if let Some(comment) = rest.strip_prefix("<!--") {
            let end = comment.find("-->").ok_or("unterminated comment")?;
            tokens.push(Token::Comment(comment[..end].to_owned()));
            i = open + 4 + end + 3;
            continue;
        }
        if rest.starts_with("<!") || rest.starts_with("<?") {
            let end = rest.find('>').ok_or("unterminated declaration")?;
            tokens.push(Token::Declaration(rest[..=end].to_owned()));
            i = open + end + 1;
            continue;
        }
        let closing = rest.starts_with("</");
        let name_start = open + if closing { 2 } else { 1 };
        if !bytes
            .get(name_start)
            .is_some_and(|b| b.is_ascii_alphabetic())
        {
            return Err(format!(
                "a raw '<' in text near {:?}",
                &html[open..(open + 40).min(html.len())]
            ));
        }
        let mut j = name_start;
        while j < html.len() && !matches!(bytes[j], b' ' | b'\t' | b'\n' | b'\r' | b'/' | b'>') {
            j += 1;
        }
        let name = html[name_start..j].to_ascii_lowercase();
        let mut attrs = vec![];
        loop {
            while j < html.len() && matches!(bytes[j], b' ' | b'\t' | b'\n' | b'\r' | b'/') {
                j += 1;
            }
            if j >= html.len() {
                return Err(format!("unterminated <{name}>"));
            }
            if bytes[j] == b'>' {
                j += 1;
                break;
            }
            let start = j;
            while j < html.len()
                && !matches!(bytes[j], b' ' | b'\t' | b'\n' | b'\r' | b'/' | b'>' | b'=')
            {
                j += 1;
            }
            let attribute = html[start..j].to_ascii_lowercase();
            let mut value = String::new();
            if bytes.get(j) == Some(&b'=') {
                j += 1;
                match bytes.get(j) {
                    Some(&quote @ (b'"' | b'\'')) => {
                        let end = html[j + 1..]
                            .find(quote as char)
                            .ok_or("unterminated attribute value")?;
                        value = html[j + 1..j + 1 + end].to_owned();
                        j += end + 2;
                    }
                    _ => {
                        let start = j;
                        while j < html.len() && !matches!(bytes[j], b' ' | b'\t' | b'\n' | b'>') {
                            j += 1;
                        }
                        value = html[start..j].to_owned();
                    }
                }
            }
            if attribute.is_empty() {
                return Err(format!("an empty attribute name in <{name}>"));
            }
            attrs.push((attribute, value));
        }
        i = open + (j - open);
        if closing {
            tokens.push(Token::Close(name));
            continue;
        }
        let raw = matches!(name.as_str(), "script" | "style" | "textarea" | "title")
            .then(|| {
                let lower = html[i..].to_ascii_lowercase();
                lower.find(&format!("</{name}")).map(|end| {
                    let body = html[i..i + end].to_owned();
                    let close = html[i + end..].find('>').unwrap_or(0);
                    i += end + close + 1;
                    body
                })
            })
            .flatten();
        tokens.push(Token::Open {
            name,
            attrs,
            raw_text: raw,
        });
    }
    Ok(tokens)
}

/// What a browser would see in an attribute value: references decoded, whitespace and control
/// characters (which URL parsers skip) removed, lower case.
fn url_form(value: &str) -> String {
    let mut decoded = value
        .replace("&Tab;", "")
        .replace("&NewLine;", "")
        .replace("&colon;", ":")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    let numeric = regex::Regex::new(r"&#(x[0-9a-fA-F]+|[0-9]+);?").unwrap();
    decoded = numeric
        .replace_all(&decoded, |caps: &regex::Captures| {
            let code = &caps[1];
            let n = match code.strip_prefix('x') {
                Some(hex) => u32::from_str_radix(hex, 16),
                None => code.parse(),
            };
            n.ok()
                .and_then(char::from_u32)
                .map(String::from)
                .unwrap_or_default()
        })
        .into_owned();
    decoded
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .collect::<String>()
        .to_ascii_lowercase()
}

const MEDIA: [&str; 8] = [
    "img", "audio", "video", "source", "track", "picture", "embed", "object",
];
const EMBED_HOSTS: [&str; 7] = [
    "youtube.com",
    "www.youtube.com",
    "youtube-nocookie.com",
    "vimeo.com",
    "player.vimeo.com",
    "gist.github.com",
    "codepen.io",
];
const FORBIDDEN_TAGS: [&str; 24] = [
    "object",
    "embed",
    "applet",
    "form",
    "link",
    "base",
    "frame",
    "frameset",
    "template",
    "marquee",
    "portal",
    "bgsound",
    "xmp",
    "plaintext",
    "textarea",
    "select",
    "foreignobject",
    "animate",
    "animatetransform",
    "animatemotion",
    "set",
    "isindex",
    "noembed",
    "dialog",
];

/// Findings in one rendered page. Empty means the page is safe.
fn findings(html: &str, case: &Case) -> Vec<String> {
    let mut found = vec![];
    let tokens = match read_html(html) {
        Ok(tokens) => tokens,
        Err(error) => return vec![format!("the page is not strict HTML: {error}")],
    };
    let mut ids: Vec<String> = vec![];
    for token in &tokens {
        match token {
            Token::Comment(text) => found.push(format!("an HTML comment ships: {text:?}")),
            Token::Declaration(text) if !text.eq_ignore_ascii_case("<!doctype html>") => {
                found.push(format!("a markup declaration: {text:?}"))
            }
            Token::Close(name) if name == "script" || name == "style" => {
                found.push(format!("a stray </{name}>"))
            }
            Token::Text(text) if text.contains("HOSTILE") && text.contains('<') => {
                found.push(format!("a hostile marker beside a raw '<': {text:?}"))
            }
            Token::Open {
                name,
                attrs,
                raw_text,
            } => {
                let attr = |key: &str| {
                    attrs
                        .iter()
                        .find(|(k, _)| k == key)
                        .map(|(_, v)| v.as_str())
                };
                if name.to_ascii_uppercase().contains("HOSTILE") {
                    found.push(format!("a hostile tag name <{name}>"));
                }
                // The page's one empty icon: it names no resource and no request follows.
                let empty_icon =
                    name == "link" && attr("rel") == Some("icon") && attr("href") == Some("data:,");
                if empty_icon {
                    continue;
                }
                if FORBIDDEN_TAGS.contains(&name.as_str()) {
                    found.push(format!("a forbidden element <{name}>"));
                }
                match name.as_str() {
                    "script" => {
                        let own = attr("type") == Some("application/json")
                            && (attr("id") == Some("richview-font-notices")
                                || attr("data-rv-table").is_some()
                                || attr("data-rv-payload").is_some());
                        if !own {
                            found.push(format!("a <script> that is not a data element: {attrs:?}"));
                        }
                        match raw_text {
                            Some(body) if serde_json::from_str::<Value>(body).is_ok() => {}
                            other => {
                                found.push(format!("a data script that is not JSON: {other:?}"))
                            }
                        }
                    }
                    "style" => {
                        let body = raw_text.as_deref().unwrap_or("").to_ascii_lowercase();
                        if body.contains('<')
                            || body.contains("@import")
                            || body.contains("expression(")
                            || body.contains("javascript:")
                            || body.contains("-moz-binding")
                        {
                            let at = body
                                .find("@import")
                                .or(body.find('<'))
                                .or(body.find("expression("))
                                .or(body.find("javascript:"))
                                .or(body.find("-moz-binding"))
                                .unwrap_or(0);
                            found.push(format!(
                                "a <style> carrying markup or an import near {:?}",
                                &body[at.saturating_sub(60)..(at + 60).min(body.len())]
                            ));
                        }
                        let mut rest = body.as_str();
                        while let Some(at) = rest.find("url(") {
                            let after = rest[at + 4..].trim_start_matches(['"', '\'', ' ']);
                            if !(after.starts_with("data:") || after.starts_with('#')) {
                                found.push(format!(
                                    "a CSS url() that is not inline: {:?}",
                                    &after[..after.len().min(40)]
                                ));
                            }
                            rest = &rest[at + 4..];
                        }
                    }
                    "iframe" => {
                        let src = attr("src").unwrap_or("");
                        let host = src
                            .strip_prefix("https://")
                            .map(|rest| rest.split(['/', '?', '#']).next().unwrap_or(""));
                        let allowed = host.is_some_and(|host| {
                            !host.contains(['@', ':'])
                                && EMBED_HOSTS.iter().any(|allowed| {
                                    host == *allowed || host.ends_with(&format!(".{allowed}"))
                                })
                        });
                        if !allowed {
                            found.push(format!("an iframe outside the embed list: {src:?}"));
                        }
                        if attr("srcdoc").is_some() || attr("sandbox").is_none() {
                            found.push("an unsandboxed iframe or one with srcdoc".into());
                        }
                    }
                    "meta" => {
                        let ok = attr("charset").is_some()
                            || matches!(
                                attr("name"),
                                Some(
                                    "viewport"
                                        | "generator"
                                        | "color-scheme"
                                        | "richview-build"
                                        | "description"
                                )
                            )
                            || attr("property").is_some_and(|p| p.starts_with("og:"))
                            || (attr("http-equiv") == Some("Content-Security-Policy")
                                && attr("content").is_some_and(|policy| {
                                    let policy = policy.replace("&#39;", "'");
                                    let policy = policy.as_str();
                                    // The policy never loosens scripts or the network (ARC-15).
                                    policy.contains("default-src 'none'")
                                        && !policy.contains("'unsafe-eval'")
                                        && !policy.split("; ").any(|d| {
                                            d.starts_with("script-src")
                                                && d.contains("unsafe-inline")
                                        })
                                }));
                        if !ok {
                            found.push(format!(
                                "a <meta> outside the known head metadata: {attrs:?}"
                            ));
                        }
                    }
                    "input" if attr("type") != Some("checkbox") || attr("disabled").is_none() => {
                        found.push(format!(
                            "an <input> that is not a disabled checkbox: {attrs:?}"
                        ));
                    }
                    _ => {}
                }
                for (key, value) in attrs {
                    if key.starts_with("on") {
                        found.push(format!("an event handler {key}=<{name}>"));
                    }
                    if key.to_ascii_uppercase().contains("HOSTILE") {
                        found.push(format!("a hostile attribute name {key}"));
                    }
                    let form = url_form(value);
                    if form.starts_with("javascript:")
                        || form.starts_with("vbscript:")
                        || form.starts_with("data:text/html")
                        || form.starts_with("data:application")
                        || form.starts_with("data:image/svg+xml;base64,phn2zy")
                            && !(name == "img" && key == "src")
                    {
                        found.push(format!("a script URL in {name} {key}={value:?}"));
                    }
                    // A generated link to an embedded media file is the one data: link.
                    let media_data = form.starts_with("data:audio/")
                        || form.starts_with("data:video/")
                        || form.starts_with("data:image/") && !form.starts_with("data:image/svg");
                    if matches!(
                        key.as_str(),
                        "href" | "xlink:href" | "action" | "formaction"
                    ) && !(media_data
                        && form.contains(";base64,")
                        && !value.contains(['<', '"']))
                    {
                        let scheme = form.split_once(':').map(|(scheme, _)| scheme);
                        let has_scheme = scheme.is_some_and(|s| {
                            !s.is_empty()
                                && s.chars()
                                    .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
                        });
                        if (has_scheme
                            && !matches!(scheme, Some("http" | "https" | "mailto" | "tel")))
                            || form.starts_with("//")
                            || form.contains('\\')
                        {
                            found.push(format!(
                                "a link target outside the allowed set in {name} {key}={value:?}"
                            ));
                        }
                    }
                    if MEDIA.contains(&name.as_str())
                        && matches!(key.as_str(), "src" | "srcset" | "poster" | "data")
                        && (form.starts_with("http:")
                            || form.starts_with("https:")
                            || form.starts_with("//")
                            || form.contains(",http")
                            || form.contains(",//"))
                    {
                        found.push(format!("a remote media URL in {name} {key}={value:?}"));
                    }
                    if key == "style" {
                        let style = value.to_ascii_lowercase();
                        if style.contains("url(")
                            || style.contains("expression(")
                            || style.contains("javascript:")
                            || style.contains("hostile")
                        {
                            found
                                .push(format!("a style attribute with a URL or marker: {value:?}"));
                        }
                    }
                    if key == "id" {
                        ids.push(value.clone());
                    }
                    if key == "class" {
                        for class in value.split_whitespace() {
                            if class.starts_with("rv-hostile")
                                || case.forbidden_classes.iter().any(|f| f == class)
                            {
                                found.push(format!("a source-chosen class {class:?} on <{name}>"));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    // Ids: unique, none chosen by source, and runtime-prefixed ones only where the renderer
    // generates them.
    let mut seen = BTreeSet::new();
    let chart = regex::Regex::new(r"^rv-chart-\d+$").unwrap();
    // The chart engine numbers the ids inside its own SVG (`rvc0`, `rvp0`); a print page repeats them
    // across charts, which is the engine's, not the source's.
    let engine = regex::Regex::new(r"^rv[a-z]+\d").unwrap();
    // The renderer numbers footnotes itself (`rv-fn-1`, `rv-fnref-1-2`).
    let notes_title = regex::Regex::new(r"^rv-notes-title(-[a-z0-9]+)?$").unwrap();
    let footnote = regex::Regex::new(r"^rv-fn(ref)?-([a-z0-9]+-)?\d+(-\d+)?$").unwrap();
    for id in &ids {
        if engine.is_match(id) {
            continue;
        }
        if !seen.insert(id.as_str()) {
            found.push(format!("a duplicate id {id:?}"));
        }
        if case.forbidden_ids.iter().any(|f| f == id) {
            found.push(format!("a source-chosen id {id:?}"));
        }
        let lower = id.to_ascii_lowercase();
        let generated = chart.is_match(id)
            || id == "richview-font-notices"
            || notes_title.is_match(id)
            || footnote.is_match(id)
            || case
                .tab_ids
                .iter()
                .any(|tab| *id == format!("tab-{tab}") || *id == format!("panel-{tab}"));
        if ["rv", "richview", "tab-", "panel-"]
            .iter()
            .any(|prefix| lower.starts_with(prefix))
            && !generated
        {
            found.push(format!("a source id {id:?} in the runtime namespace"));
        }
    }
    for text in &case.absent {
        if html.contains(text.as_str()) {
            found.push(format!("{text:?} must not appear"));
        }
    }
    for text in &case.present {
        if !html.contains(text.as_str()) {
            found.push(format!("{text:?} must appear"));
        }
    }
    found
}

fn codes(diagnostics: &[Diagnostic]) -> BTreeSet<&str> {
    diagnostics.iter().map(|d| d.code.as_str()).collect()
}

/// The host's view of the report folder: the source and the files beside it. Every render has
/// one; the CLI, the editor and the browser all validate it before they render.
fn in_folder(mut options: RenderOptions) -> RenderOptions {
    options.report_folder = Some(richview_core::document::ReportFolder {
        root: "report".into(),
        source: "report.rmd".into(),
        files: std::iter::once("report.rmd".to_owned())
            .chain(options.data.keys().cloned())
            .collect(),
    });
    options
}

fn check_case(case: &Case) -> Vec<String> {
    let mut problems = vec![];
    for style in builtin_style_names() {
        for mode in MODES {
            let label = format!("{} / {style} / {}", case.name, mode.label);
            let outcome = render(&case.source, &options(case, style, mode));
            let diagnostics = match &outcome {
                Ok(report) => &report.diagnostics,
                Err(diagnostics) => diagnostics,
            };
            if outcome.is_err() != case.refused {
                problems.push(format!(
                    "{label}: expected {}, got {}",
                    if case.refused {
                        "a refusal"
                    } else {
                        "a report"
                    },
                    if outcome.is_err() {
                        "a refusal"
                    } else {
                        "a report"
                    }
                ));
            }
            let produced = codes(diagnostics);
            for code in &case.expect {
                if !produced.contains(code.as_str()) {
                    problems.push(format!(
                        "{label}: expected diagnostic {code}, got {produced:?}"
                    ));
                }
            }
            if let Ok(report) = &outcome {
                for finding in findings(&report.html, case) {
                    problems.push(format!("{label}: {finding}"));
                }
            }
            // No diagnostic text becomes markup either: hosts show it as text, and the report
            // card (rendered above) escapes it.
            for diagnostic in diagnostics {
                if diagnostic.code.contains('<') || diagnostic.level.contains('<') {
                    problems.push(format!("{label}: diagnostic fields carry markup"));
                }
            }
        }
    }
    problems
}

#[test]
fn every_hostile_case_renders_safely_in_every_style_and_mode() {
    let cases = cases();
    assert!(cases.len() >= 40, "the corpus keeps its breadth");
    let workers = std::thread::available_parallelism().map_or(2, |n| n.get().min(8));
    let next = std::sync::atomic::AtomicUsize::new(0);
    let problems = std::sync::Mutex::new(vec![]);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let Some(case) = cases.get(i) else { break };
                    let found = check_case(case);
                    problems.lock().unwrap().extend(found);
                }
            });
        }
    });
    let mut problems = problems.into_inner().unwrap();
    problems.sort();
    assert!(
        problems.is_empty(),
        "{} problems:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

#[test]
fn the_index_names_a_sink_for_every_case() {
    for case in cases() {
        assert!(!case.sink.trim().is_empty(), "{} has a sink", case.name);
    }
}

#[test]
fn the_finder_catches_what_it_is_there_for() {
    // The checks must fail on real injections, or the corpus proves nothing.
    let case = Case {
        name: "probe".into(),
        sink: "probe".into(),
        source: String::new(),
        files: BTreeMap::new(),
        expect: vec![],
        refused: false,
        absent: vec![],
        present: vec![],
        forbidden_ids: vec![],
        forbidden_classes: vec![],
        tab_ids: vec![],
    };
    for (page, needle) in [
        ("<p><script>alert(1)</script></p>", "<script>"),
        ("<img src=x onerror=alert(1)>", "event handler"),
        ("<a href=\"JaVa&#x09;script:alert(1)\">x</a>", "script URL"),
        ("<a href=\"data:text/html;base64,AAAA\">x</a>", "script URL"),
        ("<img src=\"https://evil.example/a.png\">", "remote media"),
        ("<img src=\"//evil.example/a.png\">", "remote media"),
        ("<p id=\"rv-field\">x</p>", "runtime namespace"),
        ("<p id=\"a\">x</p><p id=\"a\">y</p>", "duplicate id"),
        (
            "<p id=\"rv-chart-0\">x</p><p id=\"rv-chart-0\">y</p>",
            "duplicate id",
        ),
        ("<!-- private note -->", "comment"),
        ("<p>a < b</p>", "raw '<'"),
        (
            "<style>p{background:url(https://evil.example/a)}</style>",
            "url()",
        ),
        ("<style>@import 'x';</style>", "import"),
        ("<iframe src=\"https://evil.example/\"></iframe>", "iframe"),
        ("<form action=\"x\"></form>", "forbidden"),
        (
            "<script id=\"richview-font-notices\" type=\"application/json\">{}</script></script>",
            "stray",
        ),
    ] {
        let found = findings(&format!("<!doctype html>{page}"), &case);
        assert!(
            found.iter().any(|f| f.contains(needle)),
            "{page} should be caught as {needle:?}, got {found:?}"
        );
    }
    // And the report's own data element passes.
    let page = "<!doctype html><script id=\"richview-font-notices\" type=\"application/json\">{\"a\":\"\\u003c\"}</script>";
    assert!(findings(page, &case).is_empty());
}

fn rendered(source: &str) -> richview_core::document::RenderedReport {
    render(source, &in_folder(RenderOptions::default())).expect("renders")
}

#[test]
fn raw_html_warns_at_its_line_names_the_markdown_alternative_and_keeps_its_text() {
    let report = rendered(
        "---\ntitle: T\n---\n\nFirst line.\n\nA <b>bold</b> word.\n\n<div>block text</div>\n",
    );
    let html: Vec<&Diagnostic> = report
        .diagnostics
        .iter()
        .filter(|d| d.code == "security.html")
        .collect();
    assert_eq!(html.len(), 2, "{:?}", report.diagnostics);
    assert!(html.iter().all(|d| d.level == "warning"));
    assert_eq!(html[0].line, Some(7), "{:?}", html[0]);
    assert!(html[0].message.contains("<b>") && html[0].message.contains("**bold**"));
    assert_eq!(html[1].line, Some(9), "{:?}", html[1]);
    assert!(html[1].message.contains("<div>"));
    assert!(report.html.contains("A bold word."));
    assert!(report.html.contains("block text"));
    assert!(!report.html.contains("<b>"));
    assert!(!report.html.contains("<div>block"));
}

#[test]
fn a_line_stays_true_after_a_rich_block() {
    let source =
        "---\ntitle: T\n---\n\n```metric\nvalue: 1\nlabel: One\n```\n\nText <i>here</i>.\n";
    let report = rendered(source);
    let line = report
        .diagnostics
        .iter()
        .find(|d| d.code == "security.html")
        .and_then(|d| d.line);
    assert_eq!(line, Some(10), "{:?}", report.diagnostics);
}

#[test]
fn html_comments_are_dropped_without_a_diagnostic() {
    let report =
        rendered("Before <!-- private HOSTILE_COMMENT --> after.\n\n<!-- also private -->\n");
    assert!(!report.html.contains("HOSTILE_COMMENT") && !report.html.contains("also private"));
    assert!(report.html.contains("Before  after."));
    assert!(!report.diagnostics.iter().any(|d| d.code == "security.html"));
}

#[test]
fn entities_still_work() {
    let report = rendered("A&nbsp;B &copy; &lt;b&gt; &#x41;.\n");
    assert!(report.html.contains("A&nbsp;B"), "{}", report.html);
    assert!(report.html.contains("&lt;b&gt;"));
    assert!(!report.html.contains("<b>"));
}

#[test]
fn an_unsafe_link_loses_the_link_and_keeps_the_text() {
    let report = rendered("[js](javascript:alert(1)) and [ok](https://example.org/).\n");
    assert!(!report.html.contains("javascript:"));
    assert!(report.html.contains("js and"), "{}", report.html);
    assert!(report.html.contains("href=\"https://example.org/\""));
    let blocked: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.code == "security.link")
        .collect();
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].level, "warning");
}

#[test]
fn a_remote_markdown_image_fails_alone_and_its_alt_text_stays() {
    let report = rendered("![the alt text](https://evil.example/a.png)\n\nKept paragraph.\n");
    assert!(report.html.contains("the alt text"), "{}", report.html);
    assert!(report.html.contains("Kept paragraph."));
    assert!(!report.html.contains("evil.example"));
    let remote = report
        .diagnostics
        .iter()
        .find(|d| d.code == "report.asset.remote")
        .expect("the remote image is reported");
    assert_eq!(remote.level, "error");
    assert!(remote.line.is_some());
}

#[test]
fn source_ids_in_the_runtime_namespace_are_dropped_or_renamed() {
    let source = "---\ntitle: T\ntabs:\n  panels:\n    - {id: one, label: One, from: one}\n---\n\n## One {#one}\n\n### Mine {#rv-chart-0}\n\nNote[^rv-chart-0].\n\n[^rv-chart-0]: Text.\n";
    let report = rendered(source);
    assert!(
        !report.html.contains("id=\"rv-chart-0\""),
        "{}",
        report.html
    );
    assert!(report.html.contains("id=\"rv-fn-1\""), "{}", report.html);
    assert!(report.html.contains("href=\"#rv-fn-1\""));
    assert_eq!(
        report
            .diagnostics
            .iter()
            .filter(|d| d.code == "security.id")
            .count(),
        2,
        "{:?}",
        report.diagnostics
    );
}

#[test]
fn the_probe_of_opn_16_is_refused_by_the_style_contract() {
    let case = cases()
        .into_iter()
        .find(|case| case.name == "s01-opn16-palette")
        .expect("the OPN-16 probe is the first style case");
    let report = render(&case.source, &options(&case, "sleek", MODES[0])).expect("renders");
    assert!(report.diagnostics.iter().any(|d| d.code == "style.schema"));
    assert!(!report.html.contains("HOSTILE_PALETTE"));
}

/// `HOSTILE_DUMP=1 cargo test -p richview-core --test hostile dump -- --ignored --nocapture`
/// prints each case's findings and diagnostic codes, for writing the index.
#[test]
#[ignore]
fn dump() {
    for case in cases() {
        let mode = MODES[0];
        let outcome = render(&case.source, &options(&case, "sleek", mode));
        let (diagnostics, found) = match &outcome {
            Ok(report) => (report.diagnostics.clone(), findings(&report.html, &case)),
            Err(diagnostics) => (diagnostics.clone(), vec!["REFUSED".into()]),
        };
        println!("{}: {:?} {:?}", case.name, codes(&diagnostics), found);
        if let (Ok(dir), Ok(report)) = (std::env::var("HOSTILE_OUT"), &outcome) {
            fs::write(
                Path::new(&dir).join(format!("{}.html", case.name)),
                &report.html,
            )
            .unwrap();
        }
        if std::env::var("HOSTILE_DUMP").is_ok() {
            for d in &diagnostics {
                println!("    {} {} line={:?} {}", d.level, d.code, d.line, d.message);
            }
        }
    }
}
