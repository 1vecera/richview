//! A host-supplied report-folder inventory; source metadata cannot provide this authority.
use crate::Diagnostic;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportFolder {
    pub root: String,
    pub source: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ReportResource {
    pub path: String,
    pub kind: &'static str,
}

fn media_resource_fields(tag: &str) -> &'static [&'static str] {
    match tag {
        "image" => &["src"],
        "audio" => &["src", "transcript"],
        "video" => &["src", "poster", "captions", "transcript"],
        "code" => &["src"],
        _ => &[],
    }
}

/// Authored local resources, shared by native/browser loaders and portable export.
pub fn report_resources(document: &crate::Document) -> Vec<ReportResource> {
    let mut resources = std::collections::BTreeMap::new();
    let mut add = |path: &str, kind: &'static str| {
        if !path.starts_with("data:") && !(kind == "link" && resources.contains_key(path)) {
            resources.insert(path.to_owned(), kind);
        }
    };
    for path in document.frontmatter["styles"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
    {
        if !crate::style::is_style_name(path) {
            add(path, "style");
        }
    }
    for key in ["title_image", "title-image", "image"] {
        if let Some(path) = document.frontmatter[key]
            .as_str()
            .or_else(|| document.frontmatter[key]["src"].as_str())
            .filter(|path| !unsplash_image_url(path))
        {
            add(path, "media");
        }
    }
    for block in &document.blocks {
        for key in ["csv", "json"] {
            if let Some(path) = block.value["data"][key].as_str() {
                add(path, "data");
            }
        }
        for &key in media_resource_fields(&block.tag) {
            if let Some(path) = block.value[key]
                .as_str()
                .filter(|path| !(block.tag == "image" && unsplash_image_url(path)))
            {
                add(path, "media");
            }
        }
        if block.tag == "scrolly"
            && let Some(path) = block.value["image"]["src"]
                .as_str()
                .filter(|path| !unsplash_image_url(path))
        {
            add(path, "media");
        }
    }
    for markdown in std::iter::once(document.body.as_str())
        .chain(document.frontmatter["prolog"].as_str())
        .chain(
            document
                .blocks
                .iter()
                .filter(|b| b.tag == "callout")
                .filter_map(|b| b.value["body"].as_str()),
        )
    {
        for event in pulldown_cmark::Parser::new_ext(markdown, crate::markdown::FLAGS) {
            match event {
                pulldown_cmark::Event::Start(pulldown_cmark::Tag::Image { dest_url, .. }) => {
                    add(&dest_url, "media")
                }
                pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link {
                    dest_url,
                    link_type,
                    ..
                }) => {
                    if let Some(path) = link_file(link_type, &dest_url) {
                        add(path, "link");
                    }
                }
                _ => {}
            }
        }
    }
    resources
        .into_iter()
        .map(|(path, kind)| ReportResource { path, kind })
        .collect()
}

/// The one remote image origin a report may use. Photos from it are hotlinked, never fetched or
/// embedded by a host, and only with a credit. `https`, the lowercase host and no userinfo or
/// port: `https://images.unsplash.com/...`. Anything else remote is an escaping asset.
pub const UNSPLASH_IMAGE_ORIGIN: &str = "https://images.unsplash.com";

/// Whether `path` is an image address on [`UNSPLASH_IMAGE_ORIGIN`], spelled exactly.
pub fn unsplash_image_url(path: &str) -> bool {
    path.strip_prefix(UNSPLASH_IMAGE_ORIGIN)
        .is_some_and(|rest| rest.starts_with('/'))
        && !path
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '\\')
        && url::Url::parse(path).is_ok_and(|url| {
            url.host_str() == Some("images.unsplash.com")
                && url.port().is_none()
                && url.username().is_empty()
                && url.password().is_none()
        })
}

/// Relative portable asset paths only. Reject URL/drive/parent paths before any host resolution.
pub fn report_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains(['\\', ':', '?', '#', '%', '\0'])
        && path.split('/').all(|part| !part.is_empty() && part != "..")
}

/// The last part of a report root, which a host may give as an absolute path: findings name the
/// folder by it, never by its place on the host (TYP-53).
fn folder_name(root: &str) -> &str {
    root.rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
        .unwrap_or(root)
}

pub fn validate_report_folder(folder: Option<&ReportFolder>) -> Result<(), Vec<Diagnostic>> {
    // Every finding sits at `report_folder`, with the file it concerns in `file`.
    let fail = |code: &str, file: Option<&str>, message: String| {
        let diagnostic = Diagnostic::new("error", code, message, "report_folder");
        Err(vec![match file {
            Some(file) => diagnostic.in_file(file),
            None => diagnostic,
        }])
    };
    let Some(folder) = folder else {
        return fail("report.root.missing", None, "A dedicated report folder is required. Save one .rmd in its own folder with its data/styles/media, or supply an explicit report_folder inventory to this host.".into());
    };
    if folder.root.trim().is_empty()
        || matches!(folder.root.as_str(), "." | "/" | "..")
        || !report_relative_path(&folder.source)
        || folder.source.contains('/')
        || !folder.source.to_lowercase().ends_with(".rmd")
    {
        return fail(
            "report.root.invalid",
            None,
            format!(
                "Invalid report root/source '{} / {}'. Use a named dedicated folder and one .rmd directly inside it.",
                folder_name(&folder.root),
                folder.source
            ),
        );
    }
    let mut files = std::collections::BTreeSet::new();
    for path in &folder.files {
        if !report_relative_path(path) {
            return fail(
                "report.root.escape",
                None,
                format!(
                    "Report inventory path '{path}' escapes or is ambiguous. Move this asset inside '{}' and use a relative path.",
                    folder_name(&folder.root)
                ),
            );
        }
        let canonical = path
            .split('/')
            .filter(|p| *p != ".")
            .collect::<Vec<_>>()
            .join("/");
        if !files.insert(canonical) {
            return fail(
                "report.root.shared",
                Some(path),
                format!(
                    "Duplicate report inventory path '{path}'. Supply one unambiguous source and asset inventory."
                ),
            );
        }
    }
    if !files.contains(&folder.source) {
        return fail(
            "report.root.missing",
            Some(&folder.source),
            format!(
                "Source '{}' is absent from report folder '{}'. Save the source there and include it in the inventory.",
                folder.source,
                folder_name(&folder.root)
            ),
        );
    }
    let sources: Vec<_> = files
        .iter()
        .filter(|p| p.to_lowercase().ends_with(".rmd"))
        .collect();
    if sources.len() != 1 {
        return fail(
            "report.root.shared",
            None,
            format!(
                "Report folder '{}' contains multiple .rmd sources: {}. Move each independent report and its assets into its own folder.",
                folder_name(&folder.root),
                sources
                    .iter()
                    .map(|p| p.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
    }
    Ok(())
}

/// A report-relative path without its `.` segments, as the folder inventory compares paths.
fn canonical(path: &str) -> String {
    path.split('/')
        .filter(|part| *part != ".")
        .collect::<Vec<_>>()
        .join("/")
}

/// How to fix a style file that cannot be read: the report renders without it (FEA-RP-30).
pub(crate) const STYLE_FILE_FIX: &str =
    "Add the file to the report folder, or remove it from `styles`; the report renders without it";

/// The report file a Markdown link targets: its path without a query or fragment. A link to an
/// address, an email, a `#` anchor or a wiki page targets none.
pub(crate) fn link_file(link_type: pulldown_cmark::LinkType, dest_url: &str) -> Option<&str> {
    if matches!(
        link_type,
        pulldown_cmark::LinkType::WikiLink { .. } | pulldown_cmark::LinkType::Email
    ) || dest_url.contains(':')
        || dest_url.starts_with('#')
    {
        return None;
    }
    dest_url
        .split(['?', '#'])
        .next()
        .filter(|path| !path.is_empty())
}

/// The files a Markdown text names, in order: each image source (`true`) and each file a link
/// targets, with its byte offset, read as the renderer reads the text.
fn markdown_files(text: &str) -> Vec<(bool, String, usize)> {
    use pulldown_cmark::{Event, Parser, Tag};
    let mut files = vec![];
    for (event, range) in Parser::new_ext(text, crate::markdown::FLAGS).into_offset_iter() {
        match event {
            Event::Start(Tag::Image { dest_url, .. }) => {
                files.push((true, dest_url.into_string(), range.start));
            }
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                ..
            }) => {
                if let Some(path) = link_file(link_type, &dest_url) {
                    files.push((false, path.to_owned(), range.start));
                }
            }
            _ => {}
        }
    }
    files
}

/// Why `path` cannot be read from the report folder, as a diagnostic code and message: the path
/// leaves the folder (`report.asset.escape`) or the folder's inventory lacks it
/// (`report.asset.missing`). `None` when it is there, a `data:` URI, or no folder is known.
pub(crate) fn asset_problem(
    folder: Option<&ReportFolder>,
    path: &str,
) -> Option<(&'static str, String)> {
    let folder = folder?;
    if path.starts_with("data:") {
        return None;
    }
    if !report_relative_path(path) {
        return Some((
            "report.asset.escape",
            format!(
                "Asset '{path}' escapes the report folder. Move it inside the report folder and use a relative path."
            ),
        ));
    }
    let wanted = canonical(path);
    (!folder.files.iter().any(|f| canonical(f) == wanted))
        .then(|| ("report.asset.missing", missing_message(path)))
}

/// What stops a render before any element is drawn: an unusable folder (FEA-RP-30). A missing or
/// escaping file, a header dataset and a block data definition that is not one local source fail
/// only their element; a header dataset nothing reported yet is reported here.
pub(crate) fn render_blockers(
    document: &mut crate::Document,
    folder: Option<&ReportFolder>,
) -> Result<(), Vec<Diagnostic>> {
    validate_report_folder(folder)?;
    if let Some(finding) = unreported_header_data(document, &document.diagnostics) {
        document.diagnostics.push(finding);
    }
    Ok(())
}

/// The frontmatter.data finding for a header dataset that none of `found` reports. The parser
/// reports a parsed document's and drops the field; a document built by a host may still carry it.
fn unreported_header_data(document: &crate::Document, found: &[Diagnostic]) -> Option<Diagnostic> {
    (document.frontmatter.get("data").is_some()
        && !found.iter().any(|d| d.code == "frontmatter.data"))
    .then(|| Diagnostic::new("error","frontmatter.data","Move header datasets into each consuming block's data.inline or local csv/json definition.","frontmatter.data"))
}

/// Validate authored resources before hosts discover/fetch them or render any output.
pub fn validate_report_document(
    document: &crate::Document,
    folder: Option<&ReportFolder>,
) -> Result<(), Vec<Diagnostic>> {
    validate_report_folder(folder)?;
    let folder = folder.unwrap();
    let mut issues: Vec<_> = document
        .diagnostics
        .iter()
        .filter(|d| {
            matches!(
                d.code.as_str(),
                "frontmatter.data" | "data.local" | "report.asset.escape"
            )
        })
        .cloned()
        .collect();
    issues.extend(unreported_header_data(document, &issues));
    let files: std::collections::BTreeSet<_> = folder.files.iter().map(|p| canonical(p)).collect();
    // Each file the report names is one finding, where it is written (TYP-56). `media` marks a
    // field a remote address cannot fill, as the renderer reports it.
    let problem = |path: &str, media: bool, location: String| -> Option<Diagnostic> {
        if path.starts_with("data:") {
            return None;
        }
        if media && crate::markdown::remote_media(path) {
            return Some(
                Diagnostic::new(
                    "error",
                    "report.asset.remote",
                    format!(
                        "'{path}' is a remote address; the report loads only files from its folder"
                    ),
                    location,
                )
                .fix("Download the file into the report folder and reference it by relative path"),
            );
        }
        let (code, message) = if !report_relative_path(path) {
            (
                "report.asset.escape",
                format!(
                    "Asset '{path}' escapes the report folder. Move it inside the report folder and use a relative path."
                ),
            )
        } else if !files.contains(&canonical(path)) {
            ("report.asset.missing", missing_message(path))
        } else {
            return None;
        };
        Some(Diagnostic::new("error", code, message, location))
    };
    // The findings of a Markdown text's images and file links, each with its byte offset.
    let markdown = |text: &str, location: &str| -> Vec<(Diagnostic, usize)> {
        markdown_files(text)
            .into_iter()
            .filter_map(|(image, path, offset)| {
                let finding = if image && crate::markdown::remote_media(&path) {
                    crate::markdown::remote_image_finding(&path, location)
                } else {
                    let found = problem(&path, false, String::new())?;
                    crate::markdown::file_finding(&found.code, &found.message, image, location)
                };
                Some((finding, offset))
            })
            .collect()
    };
    let mut found = vec![];
    if let Some(styles) = document.frontmatter["styles"].as_array() {
        for (index, style) in styles.iter().enumerate() {
            // A style name is checked by the cascade (`style.unknown`), never as a file.
            if let Some(style) = style.as_str().filter(|s| !crate::style::is_style_name(s))
                && let Some(finding) = problem(style, false, format!("frontmatter.styles[{index}]"))
            {
                found.push(finding.fix(STYLE_FILE_FIX));
            }
        }
    }
    for key in ["title_image", "title-image", "image"] {
        if let Some(path) = document.frontmatter[key]
            .as_str()
            .or_else(|| document.frontmatter[key]["src"].as_str())
            .filter(|path| !unsplash_image_url(path))
            && let Some(finding) = problem(path, true, format!("frontmatter.{key}"))
        {
            found.push(finding);
        }
    }
    if let Some(prolog) = document.frontmatter["prolog"].as_str() {
        found.extend(
            markdown(prolog, "frontmatter.prolog")
                .into_iter()
                .map(|(finding, _)| finding),
        );
    }
    crate::parser::place_frontmatter(document, &mut found);
    // The body keeps every source line, so a body offset's line is a source line: a fence's
    // placeholder keeps the fence's line count.
    let body = document.body.as_str();
    for (mut finding, offset) in markdown(body, "markdown") {
        if document.body_line > 0 {
            let before = &body[..offset];
            let start = before.rfind('\n').map_or(0, |at| at + 1);
            finding.line = Some(document.body_line + before.matches('\n').count());
            finding.column = Some(before[start..].chars().count() + 1);
            finding.tag = Some("markdown".into());
        }
        found.push(finding);
    }
    for block in &document.blocks {
        let mut in_block = vec![];
        for key in ["csv", "json"] {
            if let Some(path) = block.value["data"][key].as_str() {
                in_block.extend(problem(path, false, format!("{}.data.{key}", block.tag)));
            }
        }
        for &key in media_resource_fields(&block.tag) {
            if let Some(path) = block.value[key]
                .as_str()
                .filter(|path| !(block.tag == "image" && unsplash_image_url(path)))
            {
                let media = matches!(key, "src" | "poster");
                in_block.extend(problem(path, media, format!("{}.{key}", block.tag)));
            }
        }
        if block.tag == "scrolly"
            && let Some(path) = block.value["image"]["src"]
                .as_str()
                .filter(|path| !unsplash_image_url(path))
        {
            in_block.extend(problem(path, true, "scrolly.image.src".into()));
        }
        if block.tag == "callout"
            && let Some(text) = block.value["body"].as_str()
        {
            in_block.extend(
                markdown(text, "callout.body")
                    .into_iter()
                    .map(|(finding, _)| finding),
            );
        }
        // At the field that names the file.
        for finding in &mut in_block {
            finding.line = Some(block.line);
            finding.tag = Some(block.tag.clone());
        }
        crate::parser::place_block(&mut in_block, block);
        found.extend(in_block);
    }
    // In source order, as the author reads the report; the parse may have named an escaping
    // data path already.
    found.sort_by_key(|finding| finding.line.unwrap_or(usize::MAX));
    found.retain(|finding| {
        !issues.iter().any(|d| {
            ASSET_FINDINGS.contains(&d.code.as_str())
                && d.path == finding.path
                && d.line == finding.line
        })
    });
    issues.extend(found);
    if issues.is_empty() {
        Ok(())
    } else {
        Err(issues)
    }
}

/// Findings on a file the report names that the folder cannot give (missing, outside it, or a
/// remote address): the renderer shows a notice in place of the element and the report still
/// renders (FEA-RP-30); the file itself is never read.
pub const ASSET_FINDINGS: [&str; 3] = [
    "report.asset.missing",
    "report.asset.escape",
    "report.asset.remote",
];

/// The folder check every surface runs on a parsed report (ARC-08). Only an unusable folder stops
/// the report, and its findings are returned. Every other finding fails only its element and joins
/// the document's own diagnostics once (FEA-RP-30): a missing, escaping or remote file, a block
/// data definition that is not one local source, or a header dataset.
pub fn attach_asset_findings(
    document: &mut crate::Document,
    folder: Option<&ReportFolder>,
) -> Result<(), Vec<Diagnostic>> {
    validate_report_folder(folder)?;
    if let Err(found) = validate_report_document(document, folder) {
        // The document already carries the ones found at parse, such as an escaping path or a
        // block's second data source.
        for finding in found {
            if !document.diagnostics.contains(&finding) {
                document.diagnostics.push(finding);
            }
        }
    }
    Ok(())
}

/// The result object every surface returns for a report operation: `ok` (no errors), `status`
/// (`clean` without errors or warnings, otherwise `issues`) and the full diagnostic list, plus the
/// operation's own fields (TYP-54, TYP-68, ARC-08).
pub fn operation_result(
    diagnostics: Vec<Diagnostic>,
    extra: serde_json::Value,
) -> serde_json::Value {
    let ok = !diagnostics.iter().any(|d| d.level == "error");
    let clean = !diagnostics
        .iter()
        .any(|d| d.level == "error" || d.level == "warning");
    let mut value = serde_json::json!({
        "ok": ok,
        "status": if clean { "clean" } else { "issues" },
        "output": serde_json::Value::Null,
        "diagnostics": diagnostics,
    });
    if let (Some(map), Some(extra)) = (value.as_object_mut(), extra.as_object()) {
        map.extend(extra.clone());
    }
    value
}

/// The finding for a file the report names and the folder lacks. A name that is really pasted file
/// text (a csv written where a path belongs) is shown short, with what to write instead.
fn missing_message(path: &str) -> String {
    if path.contains('\n') || path.chars().count() > 120 {
        let head: String = path
            .lines()
            .next()
            .unwrap_or_default()
            .chars()
            .take(40)
            .collect();
        return format!(
            "'{head}…' is text, not a file name: this key takes the path of a file in the report folder. Save the text as a file there and name it, or write the rows under data.inline."
        );
    }
    format!(
        "Asset '{path}' is absent from the report folder. Add the file there or correct its report-relative path."
    )
}
