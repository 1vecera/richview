use crate::{Diagnostic, RenderOptions, escape};
use pulldown_cmark::{
    Alignment, CodeBlockKind, CowStr, Event, HeadingLevel, LinkType, Options, Parser, Tag, TagEnd,
    html,
};
use regex_lite::Regex;
use std::{borrow::Cow, collections::HashSet, sync::OnceLock};

/// Where a Markdown string sits in the report, so diagnostics can name its line and field.
pub(crate) struct Context<'a> {
    /// Explicit heading ids that tab panels name; every other authored id is ignored.
    pub heading_ids: &'a [String],
    /// Mark top-level headings for section folding after tab panels are assembled.
    pub section_markers: bool,
    /// The source line of the string's first line, when it is known.
    pub first_line: Option<usize>,
    /// Whether the string is the report's own text, line for line, so a finding also gets its
    /// column (TYP-56). A YAML field's text is placed at its field instead.
    pub columns: bool,
    /// The diagnostic path of the field the Markdown came from.
    pub path: &'a str,
    /// Makes the generated footnote ids unique: empty for the report body, a short word such as
    /// `c12` for a callout or a transcript, so two Markdown strings never share an id.
    pub scope: &'a str,
    /// The generated rich-block placeholder prefix; the only HTML comment that survives.
    pub placeholder: &'a str,
    /// The report language, which picks the quotation marks (FEA-RP-08).
    pub lang: &'a str,
}

/// The renderer's CommonMark extensions, [`crate::document::MARKDOWN_OPTIONS`]; every scan of
/// report Markdown in core reads with these.
pub(crate) const FLAGS: Options = crate::document::MARKDOWN_OPTIONS;

/// The finding for a Markdown image or file link whose file the report folder cannot give,
/// from `asset_problem`'s code and message: the folder check and the renderer give the same one
/// (FEA-RP-30).
pub(crate) fn file_finding(code: &str, message: &str, image: bool, path: &str) -> Diagnostic {
    if image {
        Diagnostic::new(
            "error",
            code,
            format!("{message} The alt text is shown instead."),
            path,
        )
        .fix("Add the file inside the report folder and reference it by relative path")
    } else {
        Diagnostic::new(
            "error",
            code,
            format!("{message} The text stays unlinked."),
            path,
        )
        .fix("Add the file inside the report folder, or link to an http(s) address")
    }
}

/// The finding for a Markdown image at the remote address `src` (TYP-40).
pub(crate) fn remote_image_finding(src: &str, path: &str) -> Diagnostic {
    Diagnostic::new(
        "error",
        "report.asset.remote",
        format!(
            "A Markdown image cannot load from the remote address '{src}'; its alt text is shown instead"
        ),
        path,
    )
    .fix("Download the file into the report folder and reference it by relative path")
}

/// Id and class prefixes that belong to the report runtime and its generated markup (`rv-`,
/// `rvc0` and `rvp0` in the chart engine's SVG, `richview-`, `tab-`, `panel-`). Source text never
/// produces a name with one of them.
const RUNTIME_PREFIXES: [&str; 4] = ["rv", "richview", "tab-", "panel-"];

/// The heading being read.
struct OpenHeading {
    /// Where its start tag sits in `events`.
    index: usize,
    /// Its level after the body's shift.
    level: usize,
    /// A kept `{#id}`.
    explicit: Option<String>,
    /// A bare brace phrase such as ` {USD}`, given back as the heading's last text.
    literal: Option<String>,
    /// Whether it stands at the top level, outside every list, quote and table.
    top_level: bool,
}

/// Encode a heading ID in an internal comment without allowing it to end the comment.
pub(crate) fn heading_marker_id(id: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(id.len() * 2);
    for byte in id.bytes() {
        encoded.push(HEX[usize::from(byte >> 4)] as char);
        encoded.push(HEX[usize::from(byte & 15)] as char);
    }
    encoded
}

fn runtime_name(name: &str) -> bool {
    let lower = name.trim().to_ascii_lowercase();
    RUNTIME_PREFIXES
        .iter()
        .any(|prefix| lower.starts_with(prefix))
}

/// Classes the Markdown renderer itself generates. The sanitizer keeps no others.
const GENERATED_CLASSES: &[&str] = &[
    "num",
    "rv-code-line",
    // The highlighter's spans in a code fence of a language it knows (FEA-RP-17).
    "hl-code",
    "hl-comment",
    "hl-fence",
    "hl-fm",
    "hl-heading",
    "hl-key",
    "hl-link",
    "hl-mark",
    "hl-punc",
    "hl-str",
    "hl-url",
    "hl-cmd",
    "hl-flag",
    "hl-num",
    "hl-bool",
    "hl-null",
    "hl-date",
    "hl-strong",
    "hl-tag",
    "hl-val",
    "center",
    "rv-source",
    "rv-source__label",
    "rv-math-error",
    "rv-math",
    "footnote-reference",
    "rv-table-scroll",
    "rv-notes",
    "rv-fn-back",
];

/// The ids the footnote machinery generates; no other id in the runtime's namespace survives.
fn generated_id(value: &str) -> bool {
    let digits = |rest: &str| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit());
    // `rv-fn-<scope>-<n>` and `rv-fnref-<scope>-<n>[-<count>]`, the scope being lowercase letters
    // and digits; tab copies add `-<k>`.
    let scoped = |rest: &str| {
        rest.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_digit() || b.is_ascii_lowercase())
        }) && rest.split('-').count() <= 3
            && rest.rsplit('-').next().is_some_and(digits)
    };
    // The heading of a scoped notes section is the scope alone (`c12`, `s12n3`).
    let token = |rest: &str| {
        !rest.is_empty()
            && rest
                .bytes()
                .all(|b| b.is_ascii_digit() || b.is_ascii_lowercase())
    };
    value == "rv-notes-title"
        || value.strip_prefix("rv-notes-title-").is_some_and(token)
        || value.strip_prefix("rv-fn-").is_some_and(scoped)
        || value.strip_prefix("rv-fnref-").is_some_and(scoped)
}

/// Render Markdown to sanitized HTML. Raw HTML never renders (ARC-13, FEA-RP-14): each tag is
/// removed with a `security.html` warning that names the Markdown alternative, its text stays
/// as text and comments are dropped. A link whose target is not allowed is not linked, a remote
/// image is not loaded, and a heading or footnote id in the runtime's namespace is renamed or
/// dropped. The allowlist sanitizer at the end is a second layer over the generated HTML.
pub(crate) fn markdown_html(
    source: &str,
    options: &RenderOptions,
    context: &Context,
    diagnostics: &mut Vec<Diagnostic>,
) -> String {
    let heading_ids = context.heading_ids;
    let (source, escapes) = disambiguate_currency(source);
    let mut events = Vec::new();
    let mut code_block = false;
    let mut code_text = String::new();
    let mut code_language: Option<String> = None;
    let mut link_depth = 0usize;
    let mut verbatim_link = false;
    let mut pending_text = String::new();
    let mut pending_start = 0usize;
    let mut pending_end = 0usize;
    let mut heading: Option<OpenHeading> = None;
    let mut container_depth = 0usize;
    let mut in_columns = false;
    // Generated slugs must not take a later panel heading's explicit anchor.
    let mut used_ids: HashSet<String> = authored_heading_ids(&source)
        .into_iter()
        .filter(|id| heading_ids.contains(id) && !runtime_name(id))
        .collect();
    // A body that uses `#` moves down a level, so the title is the page's only h1 (FEA-RP-18).
    let shift = usize::from(Parser::new_ext(&source, FLAGS).any(|event| {
        matches!(
            event,
            Event::Start(Tag::Heading {
                level: HeadingLevel::H1,
                ..
            })
        )
    }));
    let mut resolved_image_bytes = 0usize;
    // The character before the current text, for choosing opening or closing quotes.
    let mut before: Option<char> = None;
    let mut table = Table::default();
    let newlines: Vec<usize> = source.match_indices('\n').map(|(i, _)| i).collect();
    // The folder's file names, built when the first link or image asks and then looked up.
    let inventory = std::cell::OnceCell::new();
    // Where the last column was counted to: its line's start, the byte reached and the characters
    // from the start to it. Findings come in source order, so a line with thousands of them is
    // counted once and not once for each.
    let counted = std::cell::Cell::new((0usize, 0usize, 0usize));
    // The diagnostic for a position `offset` bytes into the string. Its column counts the
    // characters before it on its line, less the backslashes the currency escape added.
    let at = |diagnostic: Diagnostic, offset: usize| {
        let mut diagnostic = diagnostic;
        if let Some(first) = context.first_line {
            let index = newlines.partition_point(|&n| n < offset);
            diagnostic.line = Some(first + index);
            diagnostic.tag = Some("markdown".into());
            if context.columns {
                let start = index.checked_sub(1).map_or(0, |i| newlines[i] + 1);
                // The escapes are in rising order.
                let added = escapes.partition_point(|&e| e < offset)
                    - escapes.partition_point(|&e| e < start);
                let (from_line, from, chars) = counted.get();
                let (from, chars) = if from_line == start && from <= offset {
                    (from, chars)
                } else {
                    (start, 0)
                };
                let before = chars
                    + source[from..]
                        .char_indices()
                        .take_while(|&(i, _)| from + i < offset)
                        .count();
                counted.set((start, offset, before));
                diagnostic.column = Some(before - added + 1);
            }
        }
        diagnostic
    };
    let mut kept_links: Vec<bool> = vec![];
    let mut kept_images: Vec<bool> = vec![];
    // The raw HTML block being read, and where each of its lines starts in it and in the source.
    let mut html_block: Option<(String, Vec<(usize, usize)>)> = None;
    for (event, range) in Parser::new_ext(&source, FLAGS).into_offset_iter() {
        match &event {
            Event::Start(
                Tag::BlockQuote(_) | Tag::List(_) | Tag::Item | Tag::FootnoteDefinition(_),
            ) => {
                container_depth += 1;
            }
            Event::End(
                TagEnd::BlockQuote(_) | TagEnd::List(_) | TagEnd::Item | TagEnd::FootnoteDefinition,
            ) => {
                container_depth = container_depth.saturating_sub(1);
            }
            _ => {}
        }
        if table.head {
            match &event {
                Event::Text(text) | Event::Code(text) => {
                    table.label.push_str(text);
                    table.label_key.push_str(text);
                }
                Event::InlineMath(math) | Event::DisplayMath(math) => {
                    table.label.push_str(&format!("${math}$"));
                    table.label_key.push_str(&format!("\0math:{math}\0"));
                }
                Event::FootnoteReference(label) => {
                    table.label.push_str(&format!("[^{label}]"));
                    table.label_key.push_str(&format!("\0footnote:{label}\0"));
                }
                Event::SoftBreak | Event::HardBreak => {
                    table.label.push(' ');
                    table.label_key.push(' ');
                }
                _ => {}
            }
        }
        // A task-list box has no label of its own, so it names its state (WCAG 4.1.2).
        if let Event::TaskListMarker(checked) = event {
            events.push(Event::Html(CowStr::from(format!(
                "<input type=\"checkbox\" disabled aria-label=\"{}\"{}> ",
                if checked { "Done" } else { "To do" },
                if checked { " checked" } else { "" }
            ))));
            continue;
        }
        // CommonMark may split unmatched brackets into adjacent Text events. Coalesce those
        // events before interpreting wikilinks, while keeping code and explicit links intact.
        if let Event::Text(ref text) = event
            && !code_block
            && link_depth == 0
        {
            if pending_text.is_empty() {
                pending_start = range.start;
            }
            pending_end = range.end;
            pending_text.push_str(text);
            continue;
        }
        if !pending_text.is_empty() {
            let text = std::mem::take(&mut pending_text);
            text_events(
                CowStr::from(smart(&text, &mut before, context.lang)),
                (pending_start, &source[pending_start..pending_end]),
                &mut events,
                diagnostics,
                context,
                &at,
            );
        }
        match &event {
            Event::Start(tag) if !inline(tag) => before = None,
            Event::End(tag) if !inline_end(tag) => before = None,
            Event::SoftBreak | Event::HardBreak => before = Some(' '),
            Event::Code(_) | Event::InlineMath(_) | Event::FootnoteReference(_) => {
                before = Some('0')
            }
            _ => {}
        }
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                code_block = true;
                code_text.clear();
                code_language = match &kind {
                    CodeBlockKind::Fenced(info) => {
                        info.split_whitespace().next().map(str::to_owned)
                    }
                    CodeBlockKind::Indented => None,
                };
                events.push(Event::Start(Tag::CodeBlock(kind)));
            }
            Event::Text(text) if code_block => code_text.push_str(&text),
            Event::End(TagEnd::CodeBlock) => {
                code_block = false;
                events.push(Event::Html(CowStr::from(code_lines_in(
                    &code_text,
                    code_language.as_deref(),
                    &[],
                ))));
                events.push(Event::End(TagEnd::CodeBlock));
            }
            // Link text is prose, except an autolink or email address shown as itself.
            Event::Text(text) if link_depth > 0 && !code_block => {
                let text = if verbatim_link {
                    before = text.chars().last();
                    text
                } else {
                    CowStr::from(smart(&text, &mut before, context.lang))
                };
                events.push(Event::Text(text));
            }
            Event::Start(Tag::Table(alignments)) => {
                table = Table {
                    alignments: alignments.clone(),
                    ..Table::default()
                };
                events.push(Event::Html(CowStr::from("<div class=\"rv-table-scroll\">")));
                events.push(Event::Start(Tag::Table(alignments)));
            }
            Event::End(TagEnd::Table) => {
                events.push(Event::End(TagEnd::Table));
                events.push(Event::Html(CowStr::from("</div>\n")));
            }
            Event::Start(Tag::TableHead) => {
                table.head = true;
                table.cell = 0;
                events.push(Event::Start(Tag::TableHead));
            }
            Event::End(TagEnd::TableHead) => {
                table.head = false;
                events.push(Event::End(TagEnd::TableHead));
            }
            Event::Start(Tag::TableRow) => {
                table.cell = 0;
                events.push(Event::Start(Tag::TableRow));
            }
            // Authored column alignment survives sanitising as a class, not an inline style.
            Event::Start(Tag::TableCell) => {
                table.label.clear();
                table.label_key.clear();
                table.label_offset = range.start;
                let class = match table.alignments.get(table.cell) {
                    Some(Alignment::Right) => " class=\"num\"",
                    Some(Alignment::Center) => " class=\"center\"",
                    _ => "",
                };
                let cell = if table.head { "th" } else { "td" };
                // A header cell names its column for a screen reader (WCAG 1.3.1).
                let scope = if table.head { " scope=\"col\"" } else { "" };
                table.cell += 1;
                events.push(Event::Html(CowStr::from(format!("<{cell}{scope}{class}>"))));
            }
            Event::End(TagEnd::TableCell) => {
                if table.head {
                    let label = table.label.split_whitespace().collect::<Vec<_>>().join(" ");
                    let key = table
                        .label_key
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ");
                    if !label.is_empty() && !table.labels.insert(key) {
                        diagnostics.push(at(
                            Diagnostic::new(
                                "warning",
                                "markdown.table-header",
                                format!(
                                    "Markdown table column {} repeats the header {label:?}.",
                                    table.cell
                                ),
                                context.path,
                            ),
                            table.label_offset,
                        ));
                    }
                }
                events.push(Event::End(TagEnd::TableCell));
            }
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                link_depth += 1;
                verbatim_link = matches!(link_type, LinkType::Autolink | LinkType::Email);
                // A `//host` link is an address, refused below as `security.link`.
                let file_problem = crate::folder::link_file(link_type, &dest_url)
                    .filter(|_| !dest_url.starts_with("//"))
                    .and_then(|path| {
                        crate::folder::asset_problem_in(
                            options.report_folder.as_ref(),
                            &inventory,
                            path,
                        )
                    });
                if let Some((code, message)) = file_problem {
                    // FEA-RP-30 and FEA-RP-15: a link to a file the folder lacks, or outside it,
                    // keeps its text and loses the link.
                    kept_links.push(false);
                    diagnostics.push(at(
                        file_finding(code, &message, false, context.path),
                        range.start,
                    ));
                } else if safe_link(&dest_url) {
                    kept_links.push(true);
                    if external(&dest_url) {
                        events.push(Event::Html(CowStr::from(external_anchor(&dest_url))));
                    } else {
                        events.push(Event::Start(Tag::Link {
                            link_type,
                            dest_url,
                            title,
                            id,
                        }));
                    }
                } else {
                    // The text stays; the link does not.
                    kept_links.push(false);
                    diagnostics.push(at(
                        Diagnostic::new(
                            "warning",
                            "security.link",
                            "Blocked a Markdown link whose target is not http, https, mailto, tel, a # anchor or a report file; the text stays unlinked",
                            context.path,
                        )
                        .fix("Link to an http(s), mailto or tel address, or to a file in the report folder"),
                        range.start,
                    ));
                }
            }
            Event::End(TagEnd::Link) => {
                link_depth = link_depth.saturating_sub(1);
                verbatim_link = false;
                if kept_links.pop().unwrap_or(true) {
                    events.push(Event::End(TagEnd::Link));
                }
            }
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                let src = options
                    .data
                    .get(dest_url.as_ref())
                    .map(String::as_str)
                    .unwrap_or(dest_url.as_ref());
                if remote_media(src) {
                    // TYP-40: a plain remote image is an error at its line; the alt text stays.
                    kept_images.push(false);
                    diagnostics.push(at(
                        remote_image_finding(&dest_url, context.path),
                        range.start,
                    ));
                    continue;
                }
                if !safe_media(src, "image") {
                    kept_images.push(false);
                    diagnostics.push(at(
                        Diagnostic::new(
                            "warning",
                            "security.image",
                            "Blocked a Markdown image whose source is not a report file or a base64 data:image URI; its alt text is shown instead",
                            context.path,
                        ),
                        range.start,
                    ));
                    continue;
                }
                if let Some((code, message)) = crate::folder::asset_problem_in(
                    options.report_folder.as_ref(),
                    &inventory,
                    &dest_url,
                ) {
                    // FEA-RP-30: a file the folder lacks or that lies outside it fails only this
                    // image; the alt text stays.
                    kept_images.push(false);
                    diagnostics.push(at(
                        file_finding(code, &message, true, context.path),
                        range.start,
                    ));
                    continue;
                }
                kept_images.push(true);
                // A short repeated image reference can otherwise clone a large caller-
                // supplied data URI thousands of times before report assembly sees it.
                let escaped_bytes = src.bytes().fold(0usize, |bytes, byte| {
                    bytes.saturating_add(match byte {
                        b'&' => 5,
                        b'<' | b'>' => 4,
                        b'"' => 6,
                        b'\'' => 5,
                        _ => 1,
                    })
                });
                if escaped_bytes > crate::MAX_REPORT_BYTES - resolved_image_bytes {
                    diagnostics.extend(crate::render::output_limit(None));
                    return String::new();
                }
                resolved_image_bytes += escaped_bytes;
                if !src.starts_with("data:") {
                    diagnostics.push(Diagnostic::new(
                        "info",
                        "assets.external-reference",
                        "Markdown image is not embedded; the report requires the external asset",
                        "markdown.image",
                    ));
                }
                events.push(Event::Start(Tag::Image {
                    link_type,
                    dest_url: CowStr::from(src.to_owned()),
                    title,
                    id,
                }));
            }
            Event::End(TagEnd::Image) => {
                if kept_images.pop().unwrap_or(true) {
                    events.push(Event::End(TagEnd::Image));
                }
            }
            Event::Start(Tag::Heading {
                level,
                id,
                classes,
                attrs,
            }) => {
                // A bare brace phrase such as `Price {USD}` is text, not an attribute request.
                let literal = (id.is_none()
                    && classes.is_empty()
                    && !attrs.is_empty()
                    && attrs.iter().all(|(_, value)| value.is_none()))
                .then(|| {
                    format!(
                        " {{{}}}",
                        attrs
                            .iter()
                            .map(|(key, _)| key.as_ref())
                            .collect::<Vec<_>>()
                            .join(" ")
                    )
                });
                let id = id.filter(|id| {
                    let keep = !runtime_name(id);
                    if !keep {
                        diagnostics.push(at(
                            Diagnostic::new(
                                "warning",
                                "security.id",
                                format!("Heading id \"{id}\" is reserved for the report runtime and was dropped"),
                                context.path,
                            )
                            .fix("Choose an id that does not start with rv, richview, tab- or panel-"),
                            range.start,
                        ));
                    }
                    if !keep { return false; }
                    if !heading_ids.iter().any(|allowed| allowed.as_str() == id.as_ref()) {
                        diagnostics.push(at(Diagnostic::new(
                            "warning", "markdown.heading-id",
                            "An explicit heading id is allowed only when a tab panel names it",
                            context.path,
                        ).fix("Name the id in tabs.panels[].from, or remove it and use the heading's generated anchor"), range.start));
                        return false;
                    }
                    true
                });
                if !classes.is_empty() || (!attrs.is_empty() && literal.is_none()) {
                    diagnostics.push(at(
                        Diagnostic::new(
                            "warning",
                            "security.class",
                            "Heading classes and attributes from source were dropped; only {#id} is allowed",
                            context.path,
                        ),
                        range.start,
                    ));
                }
                let mut level = level as usize;
                if shift == 1 && level == 6 {
                    diagnostics.push(at(
                        Diagnostic::new(
                            "warning",
                            "markdown.heading-depth",
                            "A sixth-level heading cannot move below the page title and stays at level 6",
                            context.path,
                        )
                        .fix("Use at most five heading levels, with # for the top level"),
                        range.start,
                    ));
                } else {
                    level += shift;
                }
                heading = Some(OpenHeading {
                    index: events.len(),
                    level,
                    explicit: id.map(|id| id.to_string()),
                    literal,
                    top_level: container_depth == 0 && !in_columns,
                });
                events.push(Event::Html(CowStr::from("")));
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some(OpenHeading {
                    index,
                    level,
                    explicit,
                    literal,
                    top_level,
                }) = heading.take()
                {
                    if let Some(literal) = literal {
                        events.push(Event::Text(CowStr::from(literal)));
                    }
                    let text: String = events[index + 1..]
                        .iter()
                        .filter_map(|event| match event {
                            Event::Text(text) | Event::Code(text) => Some(text.as_ref()),
                            _ => None,
                        })
                        .collect();
                    let id = match explicit {
                        Some(id) => id,
                        None => unique_slug(&text, &used_ids),
                    };
                    used_ids.insert(id.clone());
                    events[index] =
                        Event::Html(CowStr::from(format!("<h{level} id=\"{}\">", escape(&id))));
                    events.push(Event::Html(CowStr::from(format!("</h{level}>\n"))));
                    if context.section_markers && top_level {
                        let prefix = context.placeholder.replacen("block:", "heading:", 1);
                        events.push(Event::Html(CowStr::from(format!(
                            "{prefix}{level}:{}-->",
                            heading_marker_id(&id)
                        ))));
                    }
                }
            }
            Event::Start(Tag::FootnoteDefinition(label)) => {
                events.push(Event::Start(Tag::FootnoteDefinition(CowStr::from(
                    format!("{}\0{label}", range.start),
                ))));
            }
            Event::FootnoteReference(label) => {
                events.push(Event::FootnoteReference(CowStr::from(format!(
                    "{}\0{label}",
                    range.start
                ))));
            }
            Event::InlineMath(math) => events.push(Event::Html(CowStr::from(render_math(
                &math,
                false,
                range.start,
                diagnostics,
                &at,
            )))),
            // The scroll box carries the region role when a wide formula overflows, so the
            // `<math>` keeps its own role for assistive technology.
            Event::DisplayMath(math) => events.push(Event::Html(CowStr::from(format!(
                "<span class=\"rv-math\">{}</span>",
                render_math(&math, true, range.start, diagnostics, &at)
            )))),
            Event::Start(Tag::HtmlBlock) => {
                html_block = Some((String::new(), vec![]));
                events.push(Event::Start(Tag::HtmlBlock));
            }
            // A block of raw HTML arrives line by line. Judge it whole, so a tag or comment
            // that spans lines is one tag or comment. Each line keeps its own source offset: in
            // a list or quote, every line loses its indent or `>`.
            Event::Html(raw) if html_block.is_some() => {
                if let Some((buffer, starts)) = html_block.as_mut() {
                    starts.push((buffer.len(), range.start));
                    buffer.push_str(&raw);
                }
            }
            Event::End(TagEnd::HtmlBlock) => {
                if let Some((buffer, starts)) = html_block.take() {
                    if !context.placeholder.is_empty()
                        && is_placeholder(&buffer, context.placeholder)
                    {
                        // A heading in a column never starts or ends a folded section, so a
                        // fold always holds whole columns (FEA-RP-67, FEA-RP-68).
                        match crate::columns::mark_of(
                            &buffer,
                            &crate::columns::prefix(context.placeholder),
                        ) {
                            Some(crate::columns::Mark::Open(_)) => in_columns = true,
                            Some(crate::columns::Mark::Close) => in_columns = false,
                            _ => {}
                        }
                        events.push(Event::Html(CowStr::from(buffer)));
                    } else {
                        let at_line = |diagnostic: Diagnostic, here: usize| {
                            let line = starts.partition_point(|&(from, _)| from <= here);
                            let (from, start) = starts[line.saturating_sub(1)];
                            at(diagnostic, start + here - from)
                        };
                        let text = strip_html(&buffer, 0, context, &at_line, diagnostics);
                        if !text.trim().is_empty() {
                            events.push(Event::Html(CowStr::from(format!(
                                "<p>{}</p>\n",
                                text.trim()
                            ))));
                        }
                    }
                }
                events.push(Event::End(TagEnd::HtmlBlock));
            }
            Event::Html(raw) | Event::InlineHtml(raw) => {
                let text = strip_html(&raw, range.start, context, &at, diagnostics);
                if !text.is_empty() {
                    events.push(Event::Html(CowStr::from(text)));
                }
            }
            other => events.push(other),
        }
    }
    if !pending_text.is_empty() {
        text_events(
            CowStr::from(smart(&pending_text, &mut before, context.lang)),
            (pending_start, &source[pending_start..pending_end]),
            &mut events,
            diagnostics,
            context,
            &at,
        );
    }
    let _ = source_notes(&mut events, context.placeholder);
    let events = footnotes(events, diagnostics, context, &at);
    let mut result = String::new();
    html::push_html(&mut result, events.into_iter());
    let mut cleaner = ammonia::Builder::default();
    cleaner
        .strip_comments(false)
        .link_rel(None)
        .add_generic_attributes(&["class", "id"])
        .add_tags(&[
            "section",
            "input",
            "math",
            "mrow",
            "mn",
            "mi",
            "mo",
            "mtext",
            "mspace",
            "msup",
            "msub",
            "msubsup",
            "mfrac",
            "msqrt",
            "mroot",
            "munder",
            "mover",
            "munderover",
            "mtable",
            "mtr",
            "mtd",
            "menclose",
            "mstyle",
            "mpadded",
            "mphantom",
            "semantics",
            "annotation",
            "dl",
            "dt",
            "dd",
        ])
        .add_tag_attributes("input", &["type", "checked", "disabled", "aria-label"])
        .add_tag_attributes("div", &["tabindex", "role", "aria-label"])
        .add_tag_attributes("section", &["aria-labelledby"])
        .add_tag_attributes("th", &["scope"])
        .add_tag_attributes("a", &["rel", "aria-label"])
        .add_tag_attributes("math", &["xmlns", "display"])
        .add_tag_attributes("annotation", &["encoding"])
        .add_tag_attributes("mi", &["mathvariant"])
        .add_tag_attributes(
            "mo",
            &["stretchy", "fence", "separator", "lspace", "rspace"],
        )
        .add_tag_attributes("mspace", &["width", "height", "depth"])
        .add_tag_attributes("mstyle", &["mathvariant", "displaystyle", "scriptlevel"])
        .add_tag_attributes("mtd", &["columnalign"])
        .add_tag_attributes("mtable", &["columnalign", "rowspacing", "columnspacing"])
        .url_schemes(
            ["http", "https", "mailto", "tel", "data"]
                .into_iter()
                .collect::<HashSet<_>>(),
        )
        .attribute_filter(|element, attribute, value| {
            if (attribute == "href" && !safe_link(value))
                || (element == "img"
                    && attribute == "src"
                    && (remote_media(value) || !safe_media(value, "image")))
                || (element == "input" && attribute == "type" && value != "checkbox")
                || (attribute == "id" && runtime_name(value) && !generated_id(value))
                || (element == "a" && attribute == "rel" && value != "noopener noreferrer")
                || (element == "th" && attribute == "scope" && value != "col")
                || (element == "div"
                    && matches!(attribute, "tabindex" | "role" | "aria-label")
                    && !matches!(
                        (attribute, value),
                        ("tabindex", "0") | ("role", "region") | ("aria-label", "Table")
                    ))
            {
                None
            } else if attribute == "class" {
                // Only classes the renderer generates, or a code fence's `language-` class.
                let kept: Vec<&str> = value
                    .split_whitespace()
                    .filter(|class| {
                        GENERATED_CLASSES.contains(class)
                            || code_indent_class(class)
                            || class.starts_with("language-")
                    })
                    .collect();
                (!kept.is_empty()).then(|| Cow::Owned(kept.join(" ")))
            } else {
                Some(std::borrow::Cow::Borrowed(value))
            }
        });
    cleaner.clean(&result).to_string()
}

/// A code block's text as one `rv-code-line` span per source line, its newline inside the span,
/// and an `rv-code-line--i<n>` class for a line indented by `n` columns: print hangs each line's
/// continuations from its own indent (FEA-RP-17), and the text a reader copies is unchanged.
pub(crate) fn code_lines(text: &str) -> String {
    code_lines_in(text, None, &[])
}

/// [`code_lines`] in `language`: a language the highlighter knows (Rich Markdown, YAML, shell, SQL)
/// is set in its `hl-` spans, which a line's end closes and the next line opens again, and any
/// other is plain. A line whose number (from 1) is in `marked` also has `rv-code-line--mark`.
pub(crate) fn code_lines_in(text: &str, language: Option<&str>, marked: &[usize]) -> String {
    use crate::highlight::{HighlightLanguage, highlight_html};
    let language = language.map_or(HighlightLanguage::Plain, HighlightLanguage::from_name);
    let plain = || {
        text.split_inclusive('\n')
            .map(escape)
            .collect::<Vec<String>>()
    };
    let lines = match language {
        HighlightLanguage::Plain => plain(),
        known => {
            // The highlighter keeps every character, so its lines are the text's lines; were
            // they ever not, the code is shown plain rather than cut.
            let lines = split_spans(&highlight_html(text, known));
            if lines.len() == text.split_inclusive('\n').count() {
                lines
            } else {
                plain()
            }
        }
    };
    let mut html = String::with_capacity(text.len() * 2 + 48 * lines.len());
    for (n, (line, inner)) in text.split_inclusive('\n').zip(&lines).enumerate() {
        html.push_str("<span class=\"rv-code-line");
        match code_indent(line) {
            0 => {}
            columns => html.push_str(&format!(" rv-code-line--i{columns}")),
        }
        if marked.contains(&(n + 1)) {
            html.push_str(" rv-code-line--mark");
        }
        html.push_str("\">");
        html.push_str(inner);
        html.push_str("</span>");
    }
    html
}

/// Highlighted HTML split after each newline, every line closing the spans still open at its
/// end and the next opening them again, so each line nests on its own.
fn split_spans(html: &str) -> Vec<String> {
    let mut lines = vec![];
    let mut open: Vec<&str> = vec![];
    let mut line = String::new();
    // Whether the line holds text, not only the spans the line before left open.
    let mut text = false;
    let mut rest = html;
    while !rest.is_empty() {
        if let Some(tail) = rest.strip_prefix("</span>") {
            open.pop();
            line.push_str("</span>");
            rest = tail;
        } else if rest.starts_with("<span") {
            let end = rest.find('>').map_or(rest.len(), |i| i + 1);
            open.push(&rest[..end]);
            line.push_str(&rest[..end]);
            rest = &rest[end..];
        } else {
            let c = rest.chars().next().unwrap_or_default();
            line.push(c);
            text = true;
            rest = &rest[c.len_utf8()..];
            if c == '\n' {
                line.push_str(&"</span>".repeat(open.len()));
                lines.push(std::mem::take(&mut line));
                line.extend(open.iter().copied());
                text = false;
            }
        }
    }
    if text {
        lines.push(line);
    }
    lines
}

/// The tab width of printed code: the browser's default `tab-size`.
const CODE_TAB: usize = 8;
/// The deepest indent print hangs from; a deeper line hangs from this column.
const MAX_CODE_INDENT: usize = 24;

/// The columns of `line`'s leading whitespace, a tab reaching the next multiple of [`CODE_TAB`],
/// capped at [`MAX_CODE_INDENT`].
fn code_indent(line: &str) -> usize {
    let mut columns = 0;
    for c in line.chars() {
        match c {
            ' ' => columns += 1,
            '\t' => columns += CODE_TAB - columns % CODE_TAB,
            _ => break,
        }
    }
    columns.min(MAX_CODE_INDENT)
}

/// Whether `class` is a code line's indent class, `rv-code-line--i1` to `rv-code-line--i24`.
fn code_indent_class(class: &str) -> bool {
    class.strip_prefix("rv-code-line--i").is_some_and(|n| {
        !n.starts_with('0')
            && n.bytes().all(|b| b.is_ascii_digit())
            && n.parse::<usize>()
                .is_ok_and(|n| (1..=MAX_CODE_INDENT).contains(&n))
    })
}

/// Whether `html` is exactly one generated rich-block placeholder comment.
fn is_placeholder(html: &str, prefix: &str) -> bool {
    // The marker of a block and the marker of a fence that failed (FEA-RP-29).
    let failed = crate::parser::failed_prefix(prefix);
    // A column marker (FEA-RP-68) only in its own grammar.
    if crate::columns::mark_of(html, &crate::columns::prefix(prefix)).is_some() {
        return true;
    }
    [prefix, failed.as_str()].into_iter().any(|prefix| {
        html.trim()
            .strip_prefix(prefix)
            .and_then(|rest| rest.strip_suffix("-->"))
            .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
    })
}

/// A footnote label in the runtime's namespace is renamed, so the id it becomes cannot be one
/// the runtime looks up. The reference and the definition are renamed alike.
fn footnote_label<'a>(
    label: CowStr<'a>,
    renamed: &mut HashSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
    context: &Context,
    at: &dyn Fn(Diagnostic, usize) -> Diagnostic,
    offset: usize,
) -> CowStr<'a> {
    if !runtime_name(&label) {
        return label;
    }
    if renamed.insert(label.to_string()) {
        diagnostics.push(at(
            Diagnostic::new(
                "warning",
                "security.id",
                format!(
                    "Footnote label \"{label}\" is reserved for the report runtime and was renamed"
                ),
                context.path,
            )
            .fix("Choose a label that does not start with rv, richview, tab- or panel-"),
            offset,
        ));
    }
    CowStr::from(format!("fn-{label}"))
}

/// An address a browser would fetch from the network: `http:`, `https:` or protocol-relative.
pub(crate) fn remote_media(src: &str) -> bool {
    let src = src.trim_start().to_ascii_lowercase();
    src.starts_with("http:") || src.starts_with("https:") || src.starts_with("//")
}

/// The Markdown that replaces a raw HTML tag.
fn markdown_alternative(tag: &str) -> &'static str {
    match tag {
        "b" | "strong" => "write **bold** text with asterisks",
        "i" | "em" | "cite" => "write *italic* text with asterisks",
        "s" | "del" | "strike" => "write ~~struck~~ text with tildes",
        "a" => "write a link as [text](https://example.org)",
        "img" | "picture" | "source" => {
            "write an image as ![alt](images/name.png) or use an image block"
        }
        "br" => "end the line with two spaces or start a new paragraph",
        "hr" => "write a rule as ---",
        "p" | "div" | "span" | "section" | "article" | "center" | "font" => {
            "separate paragraphs with a blank line"
        }
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => "write a heading with # characters",
        "ul" | "ol" | "li" => "write a list with - or 1. markers",
        "dl" | "dt" | "dd" => "write a definition list as a term line followed by `: definition`",
        "table" | "thead" | "tbody" | "tfoot" | "tr" | "td" | "th" => {
            "write a table with | pipes or use a table block"
        }
        "code" | "pre" | "kbd" | "samp" | "tt" => "write code with `backticks` or a fenced block",
        "blockquote" | "q" => "write a quotation with a > prefix",
        "sub" | "sup" => "use Unicode characters such as m² or CO₂",
        "details" | "summary" => "write the content as ordinary paragraphs",
        "script" | "style" | "iframe" | "object" | "embed" | "form" | "input" | "button"
        | "svg" | "math" | "link" | "meta" | "base" | "template" | "canvas" | "video" | "audio" => {
            "use a rich block (chart, table, image, audio, video, embed) or Markdown"
        }
        _ => "use Markdown syntax or a rich block",
    }
}

/// Remove every tag, doctype, processing instruction and comment from `raw` and return what
/// is left as escaped text. Comments vanish silently; every other tag adds a `security.html`
/// warning at its line naming the Markdown alternative. Entities such as `&nbsp;` stay.
fn strip_html(
    raw: &str,
    offset: usize,
    context: &Context,
    at: &dyn Fn(Diagnostic, usize) -> Diagnostic,
    diagnostics: &mut Vec<Diagnostic>,
) -> String {
    let mut text = String::new();
    let mut rest = raw;
    while let Some(open) = rest.find('<') {
        text.push_str(&escape_text(&rest[..open]));
        let here = offset + (raw.len() - rest.len()) + open;
        let tail = &rest[open..];
        if let Some(comment) = tail.strip_prefix("<!--") {
            // An unclosed comment runs to the end, as in browsers.
            rest = comment.find("-->").map_or("", |end| &comment[end + 3..]);
            continue;
        }
        let name_start = tail[1..].strip_prefix('/').unwrap_or(&tail[1..]);
        let is_tag = name_start.starts_with(|c: char| c.is_ascii_alphabetic())
            || tail.starts_with("<!")
            || tail.starts_with("<?");
        if !is_tag {
            text.push_str("&lt;");
            rest = &tail[1..];
            continue;
        }
        // The tag ends at the first `>` outside a quoted attribute value.
        let mut quote = None;
        let mut end = tail.len();
        for (i, c) in tail.char_indices().skip(1) {
            match (quote, c) {
                (None, '"' | '\'') => quote = Some(c),
                (Some(q), c) if c == q => quote = None,
                (None, '>') => {
                    end = i + 1;
                    break;
                }
                _ => {}
            }
        }
        let tag = &tail[..end];
        let closing = tag.starts_with("</");
        let name: String = name_start
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        if !closing {
            let label = if name.is_empty() {
                "markup".to_owned()
            } else {
                format!("<{name}>")
            };
            diagnostics.push(at(
                Diagnostic::new(
                    "warning",
                    "security.html",
                    format!(
                        "Raw HTML {label} was removed and any text inside it stays. Instead, {}",
                        markdown_alternative(&name)
                    ),
                    context.path,
                )
                .fix(markdown_alternative(&name)),
                here,
            ));
        }
        rest = &tail[end..];
    }
    text.push_str(&escape_text(rest));
    text
}

/// Escape text for HTML but keep well-formed character references, so `&nbsp;` still works.
fn escape_text(text: &str) -> String {
    static ENTITY: OnceLock<Regex> = OnceLock::new();
    let entity = ENTITY.get_or_init(|| {
        Regex::new(r"^&(?:#[0-9]{1,7}|#[xX][0-9a-fA-F]{1,6}|[a-zA-Z][a-zA-Z0-9]{1,31});").unwrap()
    });
    let mut out = String::with_capacity(text.len());
    for (i, c) in text.char_indices() {
        match c {
            '&' if entity.is_match(&text[i..]) => out.push('&'),
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

#[derive(Default)]
struct Table {
    labels: HashSet<String>,
    label: String,
    // Keep formulas and note references distinct from literal prose with the same source text.
    label_key: String,
    label_offset: usize,
    alignments: Vec<Alignment>,
    head: bool,
    cell: usize,
}

/// Inline containers keep the quote context of the text around them; blocks reset it.
fn inline(tag: &Tag) -> bool {
    matches!(
        tag,
        Tag::Emphasis
            | Tag::Strong
            | Tag::Strikethrough
            | Tag::Superscript
            | Tag::Subscript
            | Tag::Link { .. }
            | Tag::Image { .. }
    )
}

fn inline_end(tag: &TagEnd) -> bool {
    matches!(
        tag,
        TagEnd::Emphasis
            | TagEnd::Strong
            | TagEnd::Strikethrough
            | TagEnd::Superscript
            | TagEnd::Subscript
            | TagEnd::Link
            | TagEnd::Image
    )
}

/// Typographic punctuation for prose: curly quotes and apostrophes, en and em dashes and the
/// ellipsis. URLs and command-line flags keep their ASCII characters. Code, math and raw HTML
/// are separate events and never reach this function, and currency amounts pass through
/// unchanged.
fn smart(text: &str, before: &mut Option<char>, lang: &str) -> String {
    static URL: OnceLock<Regex> = OnceLock::new();
    let url = URL.get_or_init(|| {
        Regex::new(r#"(?i)\b(?:[a-z][a-z0-9+.-]*://|www\.)[^\s<>]*[^\s<>.,;:!?'")\]]"#).unwrap()
    });
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for found in url.find_iter(text) {
        typeset(&text[cursor..found.start()], before, lang, &mut out);
        out.push_str(found.as_str());
        *before = found.as_str().chars().last();
        cursor = found.end();
    }
    typeset(&text[cursor..], before, lang, &mut out);
    out
}

/// The opening and closing double and single quotation marks for a report language.
fn quote_marks(lang: &str) -> [char; 4] {
    let primary = lang.split(['-', '_']).next().unwrap_or("");
    match primary.to_ascii_lowercase().as_str() {
        "cs" | "de" | "sk" => ['„', '“', '‚', '‘'],
        "ja" => ['「', '」', '『', '』'],
        _ => ['“', '”', '‘', '’'],
    }
}

fn typeset(text: &str, before: &mut Option<char>, lang: &str, out: &mut String) {
    let [open_double, close_double, open_single, close_single] = quote_marks(lang);
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let next = chars.get(i + 1).copied();
        // A quote opens at the start of a block, after a space or after opening punctuation.
        let opens = before.is_none_or(|b| {
            b.is_whitespace() || "([{—–-/".contains(b) || b == open_double || b == open_single
        });
        let (glyph, width) = match chars[i] {
            '.' if next == Some('.') && chars.get(i + 2) == Some(&'.') => ('…', 3),
            '-' if next == Some('-') => {
                let em = chars.get(i + 2) == Some(&'-');
                let after = chars.get(i + if em { 3 } else { 2 }).copied();
                // Command-line flags (`--style`) and ASCII arrows (`-->`, `<--`) stay as written.
                let ascii = !em
                    && (after == Some('>')
                        || *before == Some('<')
                        || opens && after.is_some_and(|a| a.is_ascii_alphabetic()));
                if ascii {
                    out.push_str("--");
                    *before = Some('-');
                    i += 2;
                    continue;
                }
                if em { ('—', 3) } else { ('–', 2) }
            }
            '"' => (if opens { open_double } else { close_double }, 1),
            // An apostrophe before digits elides them ('90s) and one between letters is a
            // contraction; otherwise a quote opens only where a quote can open.
            '\'' if opens && !next.is_some_and(|n| n.is_ascii_digit()) => (open_single, 1),
            '\'' if before.is_some_and(char::is_alphanumeric)
                && next.is_some_and(char::is_alphanumeric) =>
            {
                ('’', 1)
            }
            '\'' if next.is_some_and(|n| n.is_ascii_digit()) => ('’', 1),
            '\'' => (close_single, 1),
            c => (c, 1),
        };
        out.push(glyph);
        *before = Some(glyph);
        i += width;
    }
}

/// A paragraph directly after a rich block that is wholly emphasised and starts with
/// "Source:", "Sources:", "Note:" or "Notes:" is that block's source note. It is set as a
/// roman `rv-source` paragraph whose label is a `rv-source__label` span, so a style can letter
/// it; every other paragraph is unchanged.
fn source_notes(events: &mut Vec<Event<'_>>, prefix: &str) -> Vec<usize> {
    const LABELS: [&str; 4] = ["Sources:", "Source:", "Notes:", "Note:"];
    static PLACEHOLDER: OnceLock<Regex> = OnceLock::new();
    let placeholder = PLACEHOLDER
        .get_or_init(|| Regex::new(r"^<!--richview:(?:generated:\d+:)?block:\d+-->\s*$").unwrap());
    let mut noted = Vec::new();
    let mut i = 1;
    while i + 3 < events.len() {
        let note = matches!(&events[i - 1], Event::Html(html) if html.starts_with(prefix) && placeholder.is_match(html))
            && matches!(events[i], Event::End(TagEnd::HtmlBlock))
            && matches!(events[i + 1], Event::Start(Tag::Paragraph))
            && matches!(events[i + 2], Event::Start(Tag::Emphasis))
            && matches!(&events[i + 3], Event::Text(text)
                if LABELS.iter().any(|label| text.starts_with(label)));
        if note {
            let mut depth = 0usize;
            let close = events[i + 2..]
                .iter()
                .position(|event| {
                    match event {
                        Event::Start(Tag::Emphasis) => depth += 1,
                        Event::End(TagEnd::Emphasis) => depth -= 1,
                        _ => {}
                    }
                    depth == 0
                })
                .map(|offset| i + 2 + offset);
            if let Some(close) = close
                && matches!(events.get(close + 1), Some(Event::End(TagEnd::Paragraph)))
            {
                if let Event::Html(html) = &events[i - 1]
                    && let Some(index) = html
                        .trim()
                        .strip_prefix(prefix)
                        .and_then(|s| s.strip_suffix("-->"))
                        .and_then(|s| s.parse::<usize>().ok())
                {
                    noted.push(index);
                }
                events[close + 1] = Event::Html(CowStr::Borrowed("</p>\n"));
                events.remove(close);
                events.remove(i + 2);
                events[i + 1] = Event::Html(CowStr::Borrowed("<p class=\"rv-source\">"));
                if let Event::Text(text) = &events[i + 2]
                    && let Some(label) = LABELS.iter().find(|label| text.starts_with(*label))
                {
                    let rest = text[label.len()..].to_owned();
                    events[i + 2] = Event::Html(CowStr::from(format!(
                        "<span class=\"rv-source__label\">{label}</span>"
                    )));
                    if !rest.is_empty() {
                        events.insert(i + 3, Event::Text(CowStr::from(rest)));
                    }
                }
            }
        }
        i += 1;
    }
    noted
}

/// The blocks whose following paragraph is a source note under the renderer's own rule.
pub(crate) fn noted_blocks(body: &str, prefix: &str) -> Vec<usize> {
    let mut events = Parser::new_ext(body, FLAGS).collect();
    source_notes(&mut events, prefix)
}

/// Authored heading anchors, so a generated slug alone cannot start a tab panel.
pub(crate) fn authored_heading_ids(body: &str) -> Vec<String> {
    Parser::new_ext(body, FLAGS)
        .filter_map(|event| match event {
            Event::Start(Tag::Heading { id: Some(id), .. }) => Some(id.to_string()),
            _ => None,
        })
        .collect()
}

/// The text with each currency `$` pair escaped, and where each added backslash sits in it.
fn disambiguate_currency(source: &str) -> (Cow<'_, str>, Vec<usize>) {
    if !source.contains('$') {
        return (Cow::Borrowed(source), vec![]);
    }
    let mut escapes = vec![];
    for (event, range) in Parser::new_ext(source, FLAGS).into_offset_iter() {
        if let Event::InlineMath(content) = event
            && content.chars().next().is_some_and(|c| c.is_ascii_digit())
            && source[range.end..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit())
        {
            // Financial prose such as "$13.07bn vs ~$4.6bn" contains two currency
            // openings. CommonMark's math extension can mistake the second for a closer.
            // Escape the delimiters and reparse so links/emphasis inside remain Markdown.
            escapes.push(range.start);
            escapes.push(range.end - 1);
        }
    }
    if escapes.is_empty() {
        return (Cow::Borrowed(source), vec![]);
    }
    let mut result = String::with_capacity(source.len() + escapes.len());
    let mut added = Vec::with_capacity(escapes.len());
    let mut cursor = 0;
    for position in escapes {
        result.push_str(&source[cursor..position]);
        added.push(result.len());
        result.push('\\');
        cursor = position;
    }
    result.push_str(&source[cursor..]);
    (Cow::Owned(result), added)
}

fn render_math(
    input: &str,
    display: bool,
    offset: usize,
    diagnostics: &mut Vec<Diagnostic>,
    at: &dyn Fn(Diagnostic, usize) -> Diagnostic,
) -> String {
    // latex2mathml reports unsupported TeX (`\href`, `\style`) as `Ok` with the error written into an
    // `<mtext>`, which a reader would see; it is an error like any other.
    let result = latex2mathml::latex_to_mathml(
        input,
        if display {
            latex2mathml::DisplayStyle::Block
        } else {
            latex2mathml::DisplayStyle::Inline
        },
    )
    .map_err(|err| err.to_string())
    .and_then(|math| {
        if math.contains("[PARSE ERROR") {
            Err("unsupported TeX".to_owned())
        } else {
            Ok(math)
        }
    });
    match result {
        Ok(math) => math,
        Err(err) => {
            diagnostics.push(at(
                Diagnostic::new(
                    "warning",
                    "markdown.math",
                    format!("Math could not be rendered and is shown as code: {err}"),
                    "markdown.math",
                )
                .fix("Fix the TeX, or escape a literal dollar sign as \\$"),
                offset,
            ));
            format!("<code class=\"rv-math-error\">{}</code>", escape(input))
        }
    }
}

/// Whether a link target leaves the report: `http` and `https`.
fn external(href: &str) -> bool {
    let lower = href.trim_start().to_ascii_lowercase();
    lower.starts_with("http:") || lower.starts_with("https:")
}

/// The opening tag of an external link, which opens in the same tab and withholds the opener.
fn external_anchor(href: &str) -> String {
    format!(
        "<a href=\"{}\" rel=\"noopener noreferrer\">",
        escape(href.trim())
    )
}

/// Turn text into events: wikilinks and the `[!quote]` marker are dropped to their text with a
/// warning, a bare http(s) address becomes a link, and a stray `$` span that is not math shows
/// as code with a warning. `place` is the text's source offset and its source as written, where
/// each warning finds its construct.
fn text_events<'a>(
    text: CowStr<'a>,
    place: (usize, &str),
    events: &mut Vec<Event<'a>>,
    diagnostics: &mut Vec<Diagnostic>,
    context: &Context,
    at: &dyn Fn(Diagnostic, usize) -> Diagnostic,
) {
    let (offset, written) = place;
    let escaped_dollar = written.contains("\\$");
    if text.starts_with("[!quote]") {
        diagnostics.push(at(
            Diagnostic::new(
                "warning",
                "markdown.quote",
                "The [!quote] pull-quote marker is not supported and shows as text",
                context.path,
            )
            .fix("Write the quotation as a block quote and end it with — Name"),
            offset,
        ));
    }
    static SPECIAL: OnceLock<Regex> = OnceLock::new();
    let special = SPECIAL.get_or_init(|| {
        Regex::new(r"\[\[([^\]\n]+)\]\]|\[\^[^\]\s]+\]|(?i:https?://)[^\s<>]+|\$[\\A-Za-z][^\s]*")
            .unwrap()
    });
    let mut cursor = 0;
    // How far the source is searched. A construct that smart quotes changed is not found, and
    // its warning stays at the text's start.
    let mut searched = 0;
    for found in special.find_iter(&text) {
        let matched = found.as_str();
        let (start, end) = (found.start(), found.end());
        let here = match written[searched..].find(matched) {
            Some(index) => {
                searched += index + matched.len();
                offset + searched - matched.len()
            }
            None => offset,
        };
        let replacement = if let Some(inner) = matched
            .strip_prefix("[[")
            .and_then(|m| m.strip_suffix("]]"))
        {
            let (target, label) = inner.split_once('|').unwrap_or((inner, inner));
            diagnostics.push(at(
                Diagnostic::new(
                    "warning",
                    "markdown.wikilink",
                    format!("The wikilink [[{inner}]] to \"{}\" is not supported; its label is shown as text", target.trim()),
                    context.path,
                )
                .fix("Link to a published URL with [text](https://…)"),
                here,
            ));
            Event::Text(CowStr::from(label.to_owned()))
        } else if let Some(label) = matched.strip_prefix("[^").and_then(|m| m.strip_suffix(']')) {
            diagnostics.push(at(
                Diagnostic::new(
                    "warning",
                    "markdown.footnote",
                    format!("Footnote reference [^{label}] has no definition and shows as text"),
                    context.path,
                )
                .fix(format!("Add a line [^{label}]: followed by the note text")),
                here,
            ));
            continue;
        } else if matched.starts_with('$') {
            if escaped_dollar {
                continue;
            }
            // Only the part before trailing punctuation is the span.
            diagnostics.push(at(
                Diagnostic::new(
                    "warning",
                    "markdown.math",
                    format!("The dollar span \"{matched}\" has no closing $ and is shown as code"),
                    context.path,
                )
                .fix("Close the math with a second $, or escape a literal dollar sign as \\$"),
                here,
            ));
            Event::Html(CowStr::from(format!(
                "<code class=\"rv-math-error\">{}</code>",
                escape(matched)
            )))
        } else {
            // A bare address links as in GFM, without its trailing punctuation.
            let mut url = matched;
            loop {
                let last = url.chars().last().unwrap_or(' ');
                let trim = match last {
                    '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '"' | '”' | '’' | '*' | '_' => {
                        true
                    }
                    ')' => url.matches(')').count() > url.matches('(').count(),
                    _ => false,
                };
                if !trim {
                    break;
                }
                url = &url[..url.len() - last.len_utf8()];
            }
            if url.len() <= "https://".len() || !safe_link(url) {
                continue;
            }
            if start > cursor {
                events.push(Event::Text(CowStr::from(text[cursor..start].to_owned())));
            }
            events.push(Event::Html(CowStr::from(external_anchor(url))));
            events.push(Event::Text(CowStr::from(url.to_owned())));
            events.push(Event::Html(CowStr::from("</a>")));
            cursor = start + url.len();
            continue;
        };
        if start > cursor {
            events.push(Event::Text(CowStr::from(text[cursor..start].to_owned())));
        }
        events.push(replacement);
        cursor = end;
    }
    if cursor == 0 {
        events.push(Event::Text(text));
    } else if cursor < text.len() {
        events.push(Event::Text(CowStr::from(text[cursor..].to_owned())));
    }
}

/// The ASCII letters a character stands for in a heading id, if any.
fn ascii_fold(c: char) -> &'static str {
    match c {
        'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ā' | 'ą' | 'ă' => "a",
        'ç' | 'ć' | 'č' | 'ĉ' => "c",
        'ď' | 'đ' => "d",
        'é' | 'è' | 'ê' | 'ë' | 'ě' | 'ē' | 'ę' | 'ė' => "e",
        'ğ' | 'ĝ' => "g",
        'í' | 'ì' | 'î' | 'ï' | 'ī' | 'į' | 'ı' => "i",
        'ĺ' | 'ľ' | 'ł' => "l",
        'ń' | 'ň' | 'ñ' | 'ņ' => "n",
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ø' | 'ō' | 'ő' => "o",
        'ŕ' | 'ř' => "r",
        'ś' | 'š' | 'ş' | 'ș' => "s",
        'ť' | 'ţ' | 'ț' => "t",
        'ú' | 'ù' | 'û' | 'ü' | 'ů' | 'ū' | 'ű' | 'ų' => "u",
        'ý' | 'ÿ' => "y",
        'ź' | 'ž' | 'ż' => "z",
        'ß' => "ss",
        'æ' => "ae",
        'œ' => "oe",
        'þ' => "th",
        _ => "",
    }
}

/// A heading id for `text`: lowercase ASCII letters and digits joined by hyphens, `section`
/// when nothing is left, and prefixed `s-` where it would fall in the runtime's namespace.
fn heading_slug(text: &str) -> String {
    let mut out = String::new();
    let mut separator = false;
    for c in text.chars().flat_map(char::to_lowercase) {
        let folded = ascii_fold(c);
        let piece: &str = if c.is_ascii_alphanumeric() {
            ""
        } else if folded.is_empty() {
            separator = true;
            continue;
        } else {
            folded
        };
        if separator && !out.is_empty() {
            out.push('-');
        }
        separator = false;
        if piece.is_empty() {
            out.push(c);
        } else {
            out.push_str(piece);
        }
    }
    if out.is_empty() {
        out.push_str("section");
    }
    if runtime_name(&out) {
        out.insert_str(0, "s-");
    }
    out
}

/// A heading id that no earlier heading on the page has: `base`, then `base-2`, `base-3`.
fn unique_slug(text: &str, used: &HashSet<String>) -> String {
    let base = heading_slug(text);
    if !used.contains(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|candidate| !used.contains(candidate))
        .expect("an unused suffix exists")
}

/// Number the footnotes by first reference and append the Notes section (FEA-RP-10). A
/// reference without a definition stays literal text and a definition nobody references is
/// dropped; both warn. Labels never become ids, so source cannot reach the runtime's namespace.
fn footnotes<'a>(
    events: Vec<Event<'a>>,
    diagnostics: &mut Vec<Diagnostic>,
    context: &Context,
    at: &dyn Fn(Diagnostic, usize) -> Diagnostic,
) -> Vec<Event<'a>> {
    if !events.iter().any(|event| {
        matches!(
            event,
            Event::FootnoteReference(_) | Event::Start(Tag::FootnoteDefinition(_))
        )
    }) {
        return events;
    }
    let scope = context.scope;
    let split = |label: &str| -> (usize, String) {
        let (offset, label) = label.split_once('\0').unwrap_or(("0", label));
        (offset.parse().unwrap_or(0), label.to_owned())
    };
    let mut renamed: HashSet<String> = HashSet::new();
    let mut main = Vec::new();
    // Definitions by label: source offset and body events.
    let mut definitions: Vec<(String, usize)> = Vec::new();
    let mut taken: Vec<Option<Vec<Event<'a>>>> = Vec::new();
    let mut open: Option<(String, usize, Vec<Event<'a>>)> = None;
    for event in events {
        match event {
            Event::Start(Tag::FootnoteDefinition(label)) => {
                let (offset, label) = split(&label);
                let label = footnote_label(
                    CowStr::from(label),
                    &mut renamed,
                    diagnostics,
                    context,
                    at,
                    offset,
                );
                open = Some((label.to_string(), offset, Vec::new()));
            }
            Event::End(TagEnd::FootnoteDefinition) => {
                if let Some((label, offset, body)) = open.take() {
                    if definitions.iter().any(|(known, _)| *known == label) {
                        diagnostics.push(at(
                            Diagnostic::new(
                                "warning",
                                "markdown.footnote",
                                format!("Footnote [^{label}] is defined more than once; the first definition is used"),
                                context.path,
                            )
                            .fix("Give each footnote its own label"),
                            offset,
                        ));
                    } else {
                        definitions.push((label, offset));
                        taken.push(Some(body));
                    }
                }
            }
            other => match open.as_mut() {
                Some((_, _, body)) => body.push(other),
                None => main.push(other),
            },
        }
    }
    // Labels in order of first reference, and how often each is referenced.
    let mut order: Vec<usize> = Vec::new();
    let mut counts: Vec<usize> = Vec::new();
    let mut number_events = |input: Vec<Event<'a>>,
                             diagnostics: &mut Vec<Diagnostic>,
                             order: &mut Vec<usize>,
                             counts: &mut Vec<usize>|
     -> Vec<Event<'a>> {
        input
            .into_iter()
            .map(|event| {
                let Event::FootnoteReference(label) = event else {
                    return event;
                };
                let (offset, original) = split(&label);
                let label = footnote_label(
                    CowStr::from(original.clone()),
                    &mut renamed,
                    diagnostics,
                    context,
                    at,
                    offset,
                )
                .to_string();
                let Some(found) = definitions.iter().position(|(known, _)| *known == label)
                else {
                    diagnostics.push(at(
                        Diagnostic::new(
                            "warning",
                            "markdown.footnote",
                            format!("Footnote reference [^{original}] has no definition and shows as text"),
                            context.path,
                        )
                        .fix(format!("Add a line [^{original}]: followed by the note text")),
                        offset,
                    ));
                    return Event::Text(CowStr::from(format!("[^{original}]")));
                };
                let slot = order.iter().position(|&known| known == found).unwrap_or_else(|| {
                    order.push(found);
                    counts.push(0);
                    order.len() - 1
                });
                counts[slot] += 1;
                let number = slot + 1;
                let id = if counts[slot] == 1 {
                    format!("rv-fnref-{scope}{number}")
                } else {
                    format!("rv-fnref-{scope}{number}-{}", counts[slot])
                };
                Event::Html(CowStr::from(format!(
                    "<sup class=\"footnote-reference\"><a id=\"{id}\" href=\"#rv-fn-{scope}{number}\">{number}</a></sup>"
                )))
            })
            .collect()
    };
    let mut result = number_events(main, diagnostics, &mut order, &mut counts);
    // A note can cite another, so number the notes' own references in order too.
    let mut bodies: Vec<Vec<Event<'a>>> = Vec::new();
    let mut next = 0;
    while next < order.len() {
        let body = taken[order[next]].take().unwrap_or_default();
        bodies.push(number_events(body, diagnostics, &mut order, &mut counts));
        next += 1;
    }
    for (index, (label, offset)) in definitions.iter().enumerate() {
        if !order.contains(&index) {
            diagnostics.push(at(
                Diagnostic::new(
                    "warning",
                    "markdown.footnote",
                    format!("Footnote [^{label}] is never referenced and was dropped"),
                    context.path,
                )
                .fix(format!("Cite it with [^{label}] in the text, or remove it")),
                *offset,
            ));
        }
    }
    if order.is_empty() {
        return result;
    }
    let title = if scope.is_empty() {
        "rv-notes-title".to_owned()
    } else {
        format!("rv-notes-title-{}", scope.trim_end_matches('-'))
    };
    result.push(Event::Html(CowStr::from(format!(
        "<section class=\"rv-notes\" aria-labelledby=\"{title}\"><h2 id=\"{title}\">Notes</h2><ol>"
    ))));
    for (slot, mut body) in bodies.into_iter().enumerate() {
        let number = slot + 1;
        let back = Event::Html(CowStr::from(format!(
            " <a class=\"rv-fn-back\" href=\"#rv-fnref-{scope}{number}\" aria-label=\"Back to reference {number}\"></a>"
        )));
        result.push(Event::Html(CowStr::from(format!(
            "<li id=\"rv-fn-{scope}{number}\">"
        ))));
        // A note of one paragraph reads as plain text with the way back at its end.
        let paragraphs = body
            .iter()
            .filter(|e| matches!(e, Event::Start(Tag::Paragraph)))
            .count();
        if paragraphs == 1
            && matches!(body.first(), Some(Event::Start(Tag::Paragraph)))
            && matches!(body.last(), Some(Event::End(TagEnd::Paragraph)))
        {
            body.remove(0);
            body.pop();
        }
        match body
            .iter()
            .rposition(|e| matches!(e, Event::End(TagEnd::Paragraph)))
        {
            Some(last) if last + 1 == body.len() => body.insert(last, back),
            _ => body.push(back),
        }
        result.extend(body);
        result.push(Event::Html(CowStr::from("</li>")));
    }
    result.push(Event::Html(CowStr::from("</ol></section>\n")));
    result
}

pub(crate) fn safe_link(input: &str) -> bool {
    let input = input.trim();
    if input.chars().any(|c| c.is_control()) {
        return false;
    }
    // Browsers read `\` as `/`, so `/\host` is a protocol-relative address too.
    if input.starts_with("//") || input.contains('\\') {
        return false;
    }
    match url::Url::parse(input) {
        Ok(url) => matches!(url.scheme(), "http" | "https" | "mailto" | "tel"),
        Err(_) => !input.split('/').next().unwrap_or("").contains(':'),
    }
}

pub(crate) fn safe_media(input: &str, kind: &str) -> bool {
    if input.is_empty() || input.chars().any(|c| c.is_control()) || input.starts_with("//") {
        return false;
    }
    if input.starts_with("data:") {
        let expected = match kind {
            "audio" => "data:audio/",
            "video" => "data:video/",
            _ => "data:image/",
        };
        return input.starts_with(expected)
            && input.contains(";base64,")
            && !input.contains(['<', '>', '"', '\'']);
    }
    match url::Url::parse(input) {
        Ok(url) => matches!(url.scheme(), "http" | "https"),
        Err(_) => !input.split('/').next().unwrap_or("").contains(':'),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn html(source: &str, diagnostics: &mut Vec<Diagnostic>) -> String {
        markdown_html(
            source,
            &RenderOptions::default(),
            &Context {
                heading_ids: &["mine".into()],
                section_markers: false,
                first_line: Some(1),
                columns: true,
                path: "markdown",
                scope: "",
                placeholder: "<!--richview:block:",
                lang: "en",
            },
            diagnostics,
        )
    }

    #[test]
    fn a_remote_image_is_an_error_and_keeps_its_alt_text() {
        let mut diagnostics = vec![];
        let out = html("![the alt](https://evil.example/a.png)\n", &mut diagnostics);
        assert!(
            !out.contains("<img") && !out.contains("evil.example"),
            "{out}"
        );
        assert!(out.contains("the alt"));
        let remote = diagnostics
            .iter()
            .find(|d| d.code == "report.asset.remote")
            .expect("report.asset.remote");
        assert_eq!((remote.level.as_str(), remote.line), ("error", Some(1)));
        for src in ["//evil.example/a.png", "HTTP://evil.example/a.png"] {
            let out = html(&format!("![x]({src})"), &mut vec![]);
            assert!(!out.contains("<img"), "{src}: {out}");
        }
    }

    #[test]
    fn a_data_image_is_kept() {
        let out = html("![x](data:image/png;base64,AAAA)", &mut vec![]);
        assert!(
            out.contains("<img src=\"data:image/png;base64,AAAA\""),
            "{out}"
        );
    }

    #[test]
    fn only_the_generated_placeholder_comment_survives() {
        let mut diagnostics = vec![];
        let out = html(
            "<!-- private -->\n\n<!--richview:block:3-->\n\nText.\n",
            &mut diagnostics,
        );
        assert!(out.contains("<!--richview:block:3-->"), "{out}");
        assert!(!out.contains("private"));
        assert!(diagnostics.is_empty());
        // A placeholder typed into prose is not a marker.
        let out = html("a <!--richview:block:3--> b", &mut vec![]);
        assert!(!out.contains("richview:block"), "{out}");
    }

    #[test]
    fn a_tag_that_spans_lines_is_one_tag() {
        let mut diagnostics = vec![];
        let out = html("<div\n  onclick=\"x()\"\n>text</div>\n", &mut diagnostics);
        assert!(
            out.contains("text") && !out.contains("onclick") && !out.contains("<div"),
            "{out}"
        );
        assert_eq!(
            diagnostics
                .iter()
                .filter(|d| d.code == "security.html")
                .count(),
            1
        );
    }

    #[test]
    fn heading_classes_attributes_and_reserved_ids_are_dropped() {
        let mut diagnostics = vec![];
        let out = html(
            "# One {#mine .rv-chart onclick=x}\n\n# Two {#rv-field}\n",
            &mut diagnostics,
        );
        assert!(
            !out.contains("rv-chart") && !out.contains("onclick") && !out.contains("rv-field"),
            "{out}"
        );
        assert!(out.contains("id=\"mine\""), "{out}");
        let codes: Vec<_> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"security.class") && codes.contains(&"security.id"),
            "{codes:?}"
        );
    }

    #[test]
    fn unsafe_links_lose_the_link_but_keep_nested_markup() {
        let mut diagnostics = vec![];
        let out = html(
            "[**bold** text](javascript:x) [ok](#a) [no](\\\\host)",
            &mut diagnostics,
        );
        assert!(
            !out.contains("javascript") && !out.contains("host"),
            "{out}"
        );
        assert!(out.contains("<strong>bold</strong> text"));
        assert!(out.contains("href=\"#a\""));
        assert_eq!(
            diagnostics
                .iter()
                .filter(|d| d.code == "security.link")
                .count(),
            2
        );
    }
}
