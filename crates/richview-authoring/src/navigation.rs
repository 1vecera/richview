//! Navigation uses source ranges only; it never resolves files or renders a report.
use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};
use serde_json::{Value, json};
use std::ops::Range;

const MAX_ITEMS: usize = 10_000;

struct SourceMap<'a> {
    source: &'a str,
    lines: Vec<usize>,
}

impl<'a> SourceMap<'a> {
    fn new(source: &'a str) -> Self {
        let lines = std::iter::once(0)
            .chain(source.match_indices('\n').map(|(at, _)| at + 1))
            .collect();
        Self { source, lines }
    }

    fn line(&self, offset: usize) -> usize {
        self.lines
            .partition_point(|start| *start <= offset)
            .saturating_sub(1)
    }

    fn position(&self, offset: usize) -> Value {
        let line = self.line(offset);
        let character = self.source[self.lines[line]..offset]
            .trim_end_matches('\r')
            .encode_utf16()
            .count();
        json!({"line":line,"character":character})
    }

    fn range(&self, range: &Range<usize>) -> Value {
        json!({"start":self.position(range.start),"end":self.position(range.end)})
    }

    fn content_end(&self, range: Range<usize>) -> usize {
        range.start + self.source[range].trim_end().len()
    }

    fn fold(&self, range: Range<usize>) -> Option<(usize, usize)> {
        let start = self.line(range.start);
        let end = self.line(self.content_end(range.clone()).max(range.start));
        (start < end).then_some((start, end))
    }
}

struct Node {
    name: String,
    kind: &'static str,
    detail: String,
    range: Range<usize>,
    selection: Range<usize>,
    level: Option<u8>,
    children: Vec<usize>,
}

struct Heading {
    range: Range<usize>,
    level: u8,
    text: String,
    selection: Option<Range<usize>>,
}

struct Code {
    range: Range<usize>,
    tag: String,
    body: String,
}

fn short_name(text: &str, fallback: &str) -> String {
    // Normalize whitespace while consuming only the characters needed for a label.
    let mut chars = text
        .split_whitespace()
        .flat_map(|word| std::iter::once(' ').chain(word.chars()))
        .skip(1);
    let mut name: String = chars.by_ref().take(200).collect();
    if name.is_empty() {
        return fallback.into();
    }
    if chars.next().is_some() {
        name.push('…');
    }
    name
}

/// Stop at a useful root scalar, retaining titles before an unfinished YAML tail.
/// No row values or per-cell paths are retained when a block contains a large dataset.
fn block_name(tag: &str, source: &str) -> String {
    use yaml_rust2::parser::{Event as Yaml, Parser as YamlParser};
    let keys: &[&str] = match tag {
        "chart" | "callout" => &["title"],
        "metric" => &["label"],
        "image" => &["caption", "alt"],
        "audio" | "video" => &["title", "caption"],
        "embed" => &["caption", "url"],
        _ => &[],
    };
    let mut found: Vec<Option<String>> = vec![None; keys.len()];
    let mut parser = YamlParser::new_from_str(source);
    let mut depth = 0usize;
    let mut key = None;
    while !keys.is_empty() {
        let Ok((event, _)) = parser.next_token() else {
            break;
        };
        match event {
            Yaml::StreamEnd | Yaml::DocumentEnd => break,
            Yaml::SequenceStart(..) if depth == 0 => break,
            Yaml::MappingStart(..) | Yaml::SequenceStart(..) => {
                // A report block must be a root mapping with scalar keys.
                if depth == 1 && key.take().is_none() {
                    break;
                }
                depth += 1;
                if depth > 128 {
                    break;
                }
            }
            Yaml::MappingEnd | Yaml::SequenceEnd => {
                depth = depth.saturating_sub(1);
            }
            Yaml::Scalar(value, ..) if depth == 1 => {
                if let Some(name) = key.take() {
                    if let Some(index) = keys.iter().position(|candidate| *candidate == name)
                        && !value.trim().is_empty()
                    {
                        found[index] = Some(value);
                        if index == 0 {
                            break;
                        }
                    }
                } else {
                    key = Some(value);
                }
            }
            Yaml::Alias(_) if depth == 1 && key.take().is_none() => break,
            _ => {}
        }
    }
    let fallback = match tag {
        "chart" => "Chart",
        "table" => "Table",
        "metric" => "Metric",
        "callout" => "Callout",
        "image" => "Image",
        "audio" => "Audio",
        "video" => "Video",
        "embed" => "Embed",
        _ => tag,
    };
    short_name(
        found
            .iter()
            .flatten()
            .next()
            .map(String::as_str)
            .unwrap_or(""),
        fallback,
    )
}

fn metadata(source: &str, start: usize) -> Option<Range<usize>> {
    let mut lines = source[start..].split_inclusive('\n');
    let first = lines.next()?;
    if first.trim_end_matches(['\r', '\n']) != "---" {
        return None;
    }
    let mut end = start + first.len();
    for line in lines {
        end += line.len();
        if matches!(line.trim_end_matches(['\r', '\n']), "---" | "...") {
            return Some(start..end);
        }
    }
    // Match report parsing: an unclosed metadata opener falls back to Markdown.
    None
}

/// Hierarchical headings and named rich blocks, with zero-based UTF-16 positions and folds.
/// Analysis is bounded at 10,000 structural items; the response discloses truncation.
pub fn outline(source: &str) -> Value {
    let map = SourceMap::new(source);
    let start = usize::from(source.starts_with('\u{feff}')) * '\u{feff}'.len_utf8();
    let mut body = start;
    let mut nodes: Vec<Node> = vec![];
    let mut roots = vec![];
    let mut sections: Vec<usize> = vec![];
    let mut folds = vec![];
    if let Some(range) = metadata(source, start) {
        body = range.end;
        if let Some(fold) = map.fold(range) {
            folds.push(fold);
        }
    }
    let mut heading: Option<Heading> = None;
    let mut code: Option<Code> = None;
    let mut truncated = false;
    let mut items = 0usize;
    for (event, range) in
        Parser::new_ext(&source[body..], richview_core::document::MARKDOWN_OPTIONS)
            .into_offset_iter()
    {
        let range = range.start + body..range.end + body;
        if matches!(
            &event,
            Event::Start(Tag::Heading { .. } | Tag::CodeBlock(_))
        ) {
            if items == MAX_ITEMS {
                truncated = true;
                break;
            }
            items += 1;
        }
        let mut node = None;
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                heading = Some(Heading {
                    range,
                    level: level as u8,
                    text: String::new(),
                    selection: None,
                });
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some(heading) = heading.take() {
                    let selection = heading.selection.unwrap_or_else(|| {
                        heading.range.start..map.content_end(heading.range.clone())
                    });
                    node = Some(Node {
                        name: short_name(&heading.text, "Untitled section"),
                        kind: "heading",
                        detail: format!("Heading {}", heading.level),
                        range: heading.range.start..source.len(),
                        selection,
                        level: Some(heading.level),
                        children: vec![],
                    });
                }
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                let tag = match kind {
                    CodeBlockKind::Fenced(info) => info.trim().to_lowercase(),
                    CodeBlockKind::Indented => String::new(),
                };
                code = Some(Code {
                    range,
                    tag,
                    body: String::new(),
                });
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(code) = code.take() {
                    if let Some(fold) = map.fold(code.range.clone()) {
                        folds.push(fold);
                    }
                    if richview_core::RICH_TAGS.contains(&code.tag.as_str()) {
                        let end = map.content_end(code.range.clone());
                        let first_end = source[code.range.clone()]
                            .find('\n')
                            .map_or(end, |at| code.range.start + at);
                        let selection_end = map.content_end(code.range.start..first_end);
                        node = Some(Node {
                            name: block_name(&code.tag, &code.body),
                            kind: "block",
                            detail: code.tag,
                            range: code.range.start..end,
                            selection: code.range.start..selection_end,
                            level: None,
                            children: vec![],
                        });
                    }
                }
            }
            Event::Text(text)
            | Event::Code(text)
            | Event::InlineMath(text)
            | Event::DisplayMath(text) => {
                if let Some(heading) = &mut heading {
                    heading.text.push_str(&text);
                    match &mut heading.selection {
                        Some(selection) => selection.end = range.end,
                        None => heading.selection = Some(range),
                    }
                } else if let Some(code) = &mut code
                    && richview_core::RICH_TAGS.contains(&code.tag.as_str())
                {
                    code.body.push_str(&text);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some(heading) = &mut heading {
                    heading.text.push(' ');
                }
            }
            _ => {}
        }
        if let Some(node) = node {
            if let Some(level) = node.level {
                while sections
                    .last()
                    .is_some_and(|index| nodes[*index].level.unwrap() >= level)
                {
                    let index = sections.pop().unwrap();
                    nodes[index].range.end =
                        map.content_end(nodes[index].range.start..node.range.start);
                }
            }
            let index = nodes.len();
            if let Some(parent) = sections.last() {
                nodes[*parent].children.push(index);
            } else {
                roots.push(index);
            }
            if node.level.is_some() {
                sections.push(index);
            }
            nodes.push(node);
        }
    }
    for index in sections {
        nodes[index].range.end = map.content_end(nodes[index].range.clone());
    }
    for node in nodes.iter().filter(|node| node.level.is_some()) {
        if let Some(fold) = map.fold(node.range.clone()) {
            folds.push(fold);
        }
    }
    folds.sort_unstable();
    folds.dedup();
    fn symbol(index: usize, nodes: &[Node], map: &SourceMap<'_>) -> Value {
        let node = &nodes[index];
        json!({"name":node.name,"kind":node.kind,"detail":node.detail,"range":map.range(&node.range),"selectionRange":map.range(&node.selection),"children":node.children.iter().map(|index|symbol(*index,nodes,map)).collect::<Vec<_>>()})
    }
    json!({"symbols":roots.iter().map(|index|symbol(*index,&nodes,&map)).collect::<Vec<_>>(),"folds":folds.into_iter().map(|(start,end)|json!({"start":start,"end":end})).collect::<Vec<_>>(),"truncated":truncated})
}
