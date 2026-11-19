//! Split the source contract into readable topics and index the staged documentation.
use super::*;

pub(super) fn slug(text: &str) -> String {
    text.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn sections(source: &str) -> Vec<(String, String)> {
    let mut headings = Vec::new();
    let mut title = String::new();
    let mut start = None;
    for (event, range) in
        Parser::new_ext(source, richview_core::document::MARKDOWN_OPTIONS).into_offset_iter()
    {
        match event {
            Event::Start(Tag::Heading {
                level: HeadingLevel::H2,
                ..
            }) => {
                start = Some(range.start);
                title.clear();
            }
            Event::Text(text) | Event::Code(text) if start.is_some() => title.push_str(&text),
            Event::End(TagEnd::Heading(HeadingLevel::H2)) => {
                if let Some(offset) = start.take() {
                    headings.push((offset, title.clone()));
                }
            }
            _ => {}
        }
    }
    headings
        .iter()
        .enumerate()
        .map(|(n, (start, title))| {
            let end = headings
                .get(n + 1)
                .map_or(source.len(), |(offset, _)| *offset);
            (title.clone(), source[*start..end].to_owned())
        })
        .collect()
}

/// The reference's topics, the parts of its one page: the contract's second-level sections,
/// with the diagnostic codes as a topic of their own.
fn topics(source: &str) -> Vec<(String, String)> {
    let mut topics = sections(source);
    if let Some((_, diagnostics)) = topics
        .iter_mut()
        .find(|(title, _)| title == "Diagnostics and public interfaces")
        && let Some(at) = diagnostics.find("### Diagnostic codes")
    {
        let codes = diagnostics[at..].replacen("### Diagnostic codes", "## Diagnostic codes", 1);
        diagnostics.truncate(at);
        topics.push(("Diagnostic codes".into(), codes));
    }
    topics
}

/// The topics the sidebar lists under the format reference: anchor and title, in the
/// contract's order. Each is a part of the reference's one page.
pub(super) fn topic_pages() -> &'static [(String, String)] {
    static PAGES: std::sync::OnceLock<Vec<(String, String)>> = std::sync::OnceLock::new();
    PAGES.get_or_init(|| {
        topics(include_str!("../../../../docs/RMD-SPEC.md"))
            .into_iter()
            .map(|(title, _)| (slug(&title), title))
            .collect()
    })
}

pub(super) fn stage(
    root: &Path,
    docs: &Path,
    charts: &[ChartPage],
    flags: BuildFlags,
) -> Result<()> {
    let raw = fs::read_to_string(root.join("docs/RMD-SPEC.md"))?;
    let clean = public::source(&unlink_repository_paths(&raw), flags)?;
    let topics = topics(&clean);
    // The sidebar of every page links these pages by the names it was built with.
    if !topics
        .iter()
        .map(|(title, _)| title)
        .eq(topic_pages().iter().map(|(_, title)| title))
    {
        return Err("docs/RMD-SPEC.md changed its topics: rebuild richview-build".into());
    }
    // One long page holds every topic, in the contract's order (Daniel, 16 Nov 2026): the
    // sidebar's topics move within it. A topic's old address forwards to its part.
    let folder = docs.join("reference");
    fs::create_dir_all(&folder)?;
    let mut index = long_page(&topics);
    for (title, _) in &topics {
        let name = slug(title);
        fs::write(folder.join(format!("{name}.html")), forward(title, &name))?;
    }
    let typed = folder.join("charts");
    fs::create_dir_all(&typed)?;
    index.push_str("\n## Chart-specific fields\n\n");
    for chart in richview_core::registry::CHART_TYPES {
        let title = charts
            .iter()
            .find(|page| page.name == chart.name)
            .map_or(chart.name, |page| page.title.as_str());
        let mut source = format!(
            "---\ntitle: {}\nstyles: [scientific]\n---\n\n{} [Open the live example and source](https://richview.ai/docs/charts/{}).\n\n## Encoding fields\n\n| Field | Requirement |\n| --- | --- |\n",
            serde_json::to_string(&format!("{title}: fields"))?,
            chart.summary,
            chart.name
        );
        for channel in chart.encodings {
            source.push_str(&format!(
                "| `encoding.{channel}` | {} |\n",
                if chart.required.contains(channel) {
                    "Required"
                } else {
                    "Optional"
                }
            ));
        }
        source.push_str(&format!("\nSet `type: {}`. The [shared chart contract](https://richview.ai/docs/reference#charts) defines data sources, field meanings, axes, controls, annotations and styling. Unsupported encoding channels are rejected.\n", chart.name));
        fs::write(
            typed.join(format!("{}.html", chart.name)),
            render_page(
                &source,
                &format!("charts/{}", chart.name),
                Place::Reference,
                charts,
                flags,
            )?,
        )?;
        fs::write(typed.join(format!("{}.md", chart.name)), source)?;
        index.push_str(&format!(
            "- [{title}](https://richview.ai/docs/reference/charts/{})\n",
            chart.name
        ));
    }
    fs::write(
        docs.join("reference.html"),
        render_page(&index, "reference", Place::Reference, charts, flags)?,
    )?;
    fs::write(docs.join("reference.md"), clean)?;
    Ok(())
}

/// The format reference's one page: its introduction, then every topic in the contract's
/// order. A topic's body starts with its own heading, which names its part's anchor.
fn long_page(topics: &[(String, String)]) -> String {
    let mut page = INTRO.to_owned();
    for (_, body) in topics {
        page.push('\n');
        page.push_str(body);
        page.push('\n');
    }
    page
}

const INTRO: &str = "---\ntitle: Format reference\ndescription: Every field and rule of Rich Markdown on one page, by the part of the report you are editing.\nstyles: [scientific]\n---\n\nRich Markdown combines ordinary Markdown, YAML frontmatter and rich blocks. Every topic is on this page: jump to one from the contents, or search for a field or diagnostic code. The [complete Markdown contract](https://richview.ai/docs/reference.md) remains available for agents.\n";

/// A topic's old page: it forwards to the topic's part of the format reference, with a link
/// for a reader whose browser does not follow. It is not a documentation page, so search skips
/// it and no crawler indexes it.
fn forward(title: &str, name: &str) -> String {
    let to = format!("/docs/reference#{name}");
    let title = title
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>{title}: Format reference</title><meta name=\"robots\" content=\"noindex\"><link rel=\"canonical\" href=\"{to}\"><meta http-equiv=\"refresh\" content=\"0; url={to}\"></head><body><p>{title} is part of the <a href=\"{to}\">format reference</a>.</p></body></html>"
    )
}

pub(super) fn field_anchors(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    let mut seen = std::collections::BTreeSet::new();
    while let Some(start) = rest.find("<td><code>") {
        let value = start + "<td><code>".len();
        let Some(end) = rest[value..].find("</code>").map(|n| n + value) else {
            break;
        };
        out.push_str(&rest[..start]);
        let id = format!("field-{}", slug(&rest[value..end]));
        if seen.insert(id.clone()) {
            out.push_str(&format!("<td><code id=\"{id}\">"));
        } else {
            out.push_str("<td><code>");
        }
        rest = &rest[value..];
    }
    out.push_str(rest);
    out
}

pub(super) fn search_index(docs: &Path) -> Result<()> {
    fn visit(root: &Path, dir: &Path, entries: &mut Vec<serde_json::Value>) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() {
                if path.file_name().is_none_or(|name| name != "Rendered") {
                    visit(root, &path, entries)?;
                }
                continue;
            }
            if path.extension().is_none_or(|name| name != "html") {
                continue;
            }
            // Soft line breaks in table codes are not part of a search label.
            let html = fs::read_to_string(&path)?.replace("<wbr>", "");
            if !html.contains("docs-page") {
                continue;
            }
            let relative = path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            let url = format!(
                "/docs/{}",
                relative.strip_suffix(".html").unwrap_or(&relative)
            );
            let title = html
                .split("<title>")
                .nth(1)
                .and_then(|s| s.split("</title>").next())
                .unwrap_or("Documentation");
            entries.push(serde_json::json!({"title": title, "url": url, "text": title}));
            for marker in ["<h2 id=\"", "<h3 id=\"", "<code id=\""] {
                for part in html.split(marker).skip(1) {
                    let Some((id, after)) = part.split_once('"') else {
                        continue;
                    };
                    let label = after
                        .split('>')
                        .nth(1)
                        .and_then(|s| s.split('<').next())
                        .unwrap_or("");
                    if !label.is_empty() {
                        entries.push(serde_json::json!({"title": format!("{title}: {label}"), "url": format!("{url}#{id}"), "text": label}));
                    }
                }
            }
        }
        Ok(())
    }
    let mut entries = Vec::new();
    visit(docs, docs, &mut entries)?;
    fs::write(docs.join("search.json"), serde_json::to_vec(&entries)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_long_page_heads_each_topic_once_in_the_contracts_order() {
        let page = long_page(&topics(include_str!("../../../../docs/RMD-SPEC.md")));
        let mut from = 0;
        for (_, title) in topic_pages() {
            let heading = format!("\n## {title}\n");
            assert_eq!(page.matches(&heading).count(), 1, "{title}");
            let at = page.find(&heading).unwrap();
            assert!(at > from, "{title} out of order");
            from = at;
        }
    }
}
