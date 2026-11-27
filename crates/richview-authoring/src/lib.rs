//! Source-preserving editor operations. YAML marks locate keys without decoding/re-emitting values.
//! Pure, like core (ARC-02): the editor service and the site's Wasm share these completions,
//! diagnostics and outlines, so the page never offers what the checker rejects (ARC-09).
mod navigation;
pub use navigation::outline;

use pulldown_cmark::{CodeBlockKind, Event as MdEvent, Parser as MdParser, Tag};
use richview_core::document::{Diagnostic, Document, RenderOptions};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use yaml_rust2::{
    parser::{Event, Parser},
    scanner::{Marker, TScalarStyle},
};

struct Region<'a> {
    tag: String,
    start: usize,
    end: usize,
    line: usize,
    raw: &'a str,
    closed: bool,
}

fn regions(source: &str) -> Vec<Region<'_>> {
    let mut regions = vec![];
    let start = usize::from(source.starts_with('\u{feff}')) * '\u{feff}'.len_utf8();
    let mut body = start;
    if source[start..].starts_with("---\n") || source[start..].starts_with("---\r\n") {
        let opener = source[start..].find('\n').unwrap() + start + 1;
        let mut offset = opener;
        for line in source[opener..].split_inclusive('\n') {
            if matches!(line.trim_end_matches(['\r', '\n']), "---" | "...") {
                regions.push(Region {
                    tag: "frontmatter".into(),
                    start: opener,
                    end: offset,
                    line: 1,
                    raw: &source[opener..offset],
                    closed: true,
                });
                body = offset + line.len();
                break;
            }
            offset += line.len();
        }
    }
    for (event, range) in
        MdParser::new_ext(&source[body..], richview_core::document::MARKDOWN_OPTIONS)
            .into_offset_iter()
    {
        if let MdEvent::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) = event {
            let tag = info.trim().to_lowercase();
            if !richview_core::RICH_TAGS.contains(&tag.as_str()) {
                continue;
            }
            let fence_start = body + range.start;
            let fence_end = body + range.end;
            let fence = &source[fence_start..fence_end];
            let Some(newline) = fence.find('\n') else {
                continue;
            };
            // Nested/indented CommonMark containers have transformed text; leave them alone.
            let opener = &fence[..newline].trim_end_matches('\r');
            if !opener.starts_with(['`', '~']) {
                continue;
            }
            let start = fence_start + newline + 1;
            let trimmed = fence.trim_end_matches(['\r', '\n']);
            let last_start = trimmed.rfind('\n').map_or(0, |p| p + 1);
            let last = trimmed[last_start..].trim();
            let marker = opener.chars().next().unwrap();
            let width = opener.chars().take_while(|c| *c == marker).count();
            let closed = last.len() >= width && last.chars().all(|c| c == marker);
            let end = if closed {
                fence_start + last_start
            } else {
                fence_end
            };
            if end < start {
                continue;
            }
            regions.push(Region {
                tag,
                start,
                end,
                line: source[..start].bytes().filter(|c| *c == b'\n').count(),
                raw: &source[start..end],
                closed,
            });
        }
    }
    regions
}

#[derive(Clone)]
struct Entry {
    path: Vec<String>,
    key: Marker,
    value: Marker,
    scalar: Option<(String, TScalarStyle)>,
}
enum Frame {
    Map(Vec<String>, Option<(String, Marker)>),
    Seq(Vec<String>, usize),
}
fn entries(raw: &str) -> Vec<Entry> {
    let mut parser = Parser::new_from_str(raw);
    let mut frames = vec![];
    let mut entries = vec![];
    while let Ok((event, mark)) = parser.next_token() {
        if frames.len() > 128 {
            break;
        }
        if matches!(event, Event::StreamEnd) {
            break;
        }
        if matches!(event, Event::MappingEnd | Event::SequenceEnd) {
            frames.pop();
            continue;
        }
        if !matches!(
            event,
            Event::Scalar(..)
                | Event::Alias(_)
                | Event::MappingStart(..)
                | Event::SequenceStart(..)
        ) {
            continue;
        }
        if let Some(Frame::Map(_, key @ None)) = frames.last_mut() {
            if let Event::Scalar(ref text, ..) = event {
                *key = Some((text.clone(), mark));
                continue;
            }
            // Complex mapping keys are outside the authoring vocabulary.
            break;
        }
        let (path, key) = match frames.last_mut() {
            Some(Frame::Map(parent, key)) => {
                let Some((name, key)) = key.take() else {
                    break;
                };
                let mut path = parent.clone();
                path.push(name);
                (path, Some(key))
            }
            Some(Frame::Seq(parent, index)) => {
                let mut path = parent.clone();
                path.push(index.to_string());
                *index += 1;
                (path, None)
            }
            None => (vec![], None),
        };
        if let Some(key) =
            key.or_else(|| (!path.is_empty() && matches!(event, Event::Scalar(..))).then_some(mark))
        {
            entries.push(Entry {
                path: path.clone(),
                key,
                value: mark,
                scalar: if let Event::Scalar(ref text, style, ..) = event {
                    Some((text.clone(), style))
                } else {
                    None
                },
            });
        }
        match event {
            Event::MappingStart(..) => frames.push(Frame::Map(path, None)),
            Event::SequenceStart(..) => frames.push(Frame::Seq(path, 0)),
            _ => {}
        }
    }
    entries
}
fn position(region: &Region<'_>, mark: Marker, source: &str) -> (usize, usize) {
    let line = region.line + mark.line() - 1;
    let raw_line = source.lines().nth(line).unwrap_or("");
    // yaml-rust2 columns count Unicode scalars; VS Code columns count UTF-16 units.
    (
        line,
        raw_line.chars().take(mark.col()).map(char::len_utf16).sum(),
    )
}
/// The range at core's 1-based line and column: the character there, or the line's start when
/// the column is unknown (TYP-56). Columns count characters; ranges count UTF-16 units.
fn located(source: &str, line: usize, column: Option<usize>) -> Value {
    let line = line.saturating_sub(1);
    let mut chars = source.lines().nth(line).unwrap_or("").chars();
    let (character, width) = column.map_or((0, 0), |column| {
        let character = chars
            .by_ref()
            .take(column.saturating_sub(1))
            .map(char::len_utf16)
            .sum();
        (character, chars.next().map_or(0, char::len_utf16))
    });
    json!({"start":{"line":line,"character":character},"end":{"line":line,"character":character + width}})
}
fn range(region: &Region<'_>, mark: Marker, length: usize, source: &str) -> Value {
    let (line, character) = position(region, mark, source);
    json!({"start":{"line":line,"character":character},"end":{"line":line,"character":character+length.max(1)}})
}
/// YAML markers locate the token; its source spelling determines quoted/escaped width.
fn scalar_range(region: &Region<'_>, mark: Marker, decoded: &str, source: &str) -> Value {
    let (line, _) = position(region, mark, source);
    let raw = source.lines().nth(line).unwrap_or("");
    let byte = raw
        .char_indices()
        .nth(mark.col())
        .map_or(raw.len(), |(byte, _)| byte);
    let tail = &raw[byte..];
    let mut chars = tail.char_indices().peekable();
    let length = if let Some((_, quote @ ('\'' | '"'))) = chars.next() {
        let mut end = tail.len();
        while let Some((index, ch)) = chars.next() {
            if quote == '"' && ch == '\\' {
                chars.next();
            } else if ch == quote {
                if quote == '\'' && chars.peek().is_some_and(|(_, next)| *next == quote) {
                    chars.next();
                } else {
                    end = index + ch.len_utf8();
                    break;
                }
            }
        }
        tail[..end].encode_utf16().count()
    } else {
        decoded
            .encode_utf16()
            .count()
            .min(tail.encode_utf16().count())
    };
    range(region, mark, length, source)
}

fn pointer_segments(pointer: &str) -> Option<Vec<String>> {
    Some(
        pointer
            .strip_prefix('/')?
            .split('/')
            .map(|part| part.replace("~1", "/").replace("~0", "~"))
            .collect(),
    )
}

pub fn diagnostics(
    source: &str,
    original: Vec<Diagnostic>,
    _document: &Document,
    _options: &RenderOptions,
) -> Value {
    let regions = regions(source);
    let mut result = vec![];
    for diagnostic in original {
        let mut value = serde_json::to_value(&diagnostic).unwrap();
        let region = regions.iter().find(|r| {
            if diagnostic.tag.as_deref() == Some("frontmatter")
                || diagnostic.path.starts_with("frontmatter")
            {
                r.tag == "frontmatter"
            } else {
                // Diagnostics point at a field's own line, anywhere from the fence to its close.
                diagnostic.line.is_some_and(|line| {
                    (r.line..=r.line + r.raw.matches('\n').count() + 1).contains(&line)
                })
            }
        });
        if let Some(region) = region {
            let entries = entries(region.raw);
            let path = diagnostic
                .path
                .strip_prefix(&format!("{}.", region.tag))
                .unwrap_or(&diagnostic.path)
                .replace('[', ".")
                .replace(']', "");
            let source_path = diagnostic
                .details
                .as_ref()
                .and_then(|details| details["path"].as_str())
                .and_then(pointer_segments);
            let source_entry = source_path
                .as_ref()
                .and_then(|path| entries.iter().find(|entry| &entry.path == path));
            let reference = source_path
                .as_ref()
                .filter(|path| {
                    path.first().is_some_and(|part| part == "data")
                        && path.get(1).is_some_and(|part| part == "inline")
                        && !entries.iter().any(|entry| entry.path == ["data", "inline"])
                })
                .and_then(|_| {
                    ["csv", "json"].into_iter().find_map(|channel| {
                        entries.iter().find(|entry| entry.path == ["data", channel])
                    })
                })
                .and_then(|entry| entry.scalar.as_ref().map(|(text, _)| (entry, text)));
            if diagnostic.code.ends_with(".yaml") || diagnostic.code.starts_with("yaml.") {
                // The parser reports the 1-based line and column; ranges are 0-based UTF-16.
                if let (Some(line), Some(column)) = (diagnostic.line, diagnostic.column) {
                    let line = line.saturating_sub(1);
                    let character = source
                        .lines()
                        .nth(line)
                        .unwrap_or("")
                        .chars()
                        .take(column.saturating_sub(1))
                        .map(char::len_utf16)
                        .sum::<usize>();
                    value["range"] = json!({"start":{"line":line,"character":character},"end":{"line":line,"character":character+1}});
                }
            } else if let Some(entry) = source_entry {
                let key = entry.path.last().unwrap();
                let text = if entry.key == entry.value {
                    entry.scalar.as_ref().map_or(key, |(text, _)| text)
                } else {
                    key
                };
                value["range"] = scalar_range(region, entry.key, text, source);
            } else if let Some((entry, text)) = reference {
                value["range"] = scalar_range(region, entry.value, text, source);
            } else if let Some(entry) = entries.iter().find(|e| e.path.join(".") == path) {
                value["range"] = match &entry.scalar {
                    // A list entry has no key of its own: a finding at it (an inline row that is
                    // not a mapping) marks the entry's whole token.
                    Some((text, _)) if entry.key == entry.value => {
                        scalar_range(region, entry.value, text, source)
                    }
                    _ => range(
                        region,
                        entry.key,
                        entry.path.last().unwrap().encode_utf16().count(),
                        source,
                    ),
                };
            } else if let Some(line) = diagnostic.line {
                // Core knows the line (a missing key is reported at its mapping); keep it.
                value["range"] = located(source, line, diagnostic.column);
            } else {
                value["range"] = json!({"start":{"line":region.line.saturating_sub(1),"character":0},"end":{"line":region.line.saturating_sub(1),"character":region.tag.len()+3}});
            }
        } else if let Some(line) = diagnostic.line {
            // Outside the frontmatter and every fence, a Markdown finding sits at its construct.
            value["range"] = located(source, line, diagnostic.column);
        }
        if diagnostic.code == "resource.resolve"
            && let Some((region, entry)) = regions.iter().find_map(|region| {
                entries(region.raw)
                    .into_iter()
                    .find(|entry| {
                        entry.scalar.as_ref().is_some_and(|(text, _)| {
                            !text.is_empty() && diagnostic.message.contains(text)
                        })
                    })
                    .map(|entry| (region, entry))
            })
        {
            value["range"] = range(
                region,
                entry.value,
                entry.scalar.as_ref().unwrap().0.encode_utf16().count(),
                source,
            );
        }
        result.push(value);
    }
    // The editor adds no rules (ARC-08): unknown keys and unknown data fields are reported by the
    // core format when it owns them, and relayed here like every other finding.
    Value::Array(result)
}

/// Edit only indentation and spaces following actual YAML key tokens. Never reserialize values.
pub fn format(source: &str) -> String {
    let mut output = source.to_owned();
    for region in regions(source).into_iter().rev().filter(|r| r.closed) {
        let Ok(before) = richview_core::yaml::from_str(region.raw) else {
            continue;
        };
        let entries = entries(region.raw);
        let mut lines: Vec<_> = region.raw.split_inclusive('\n').map(String::from).collect();
        let mut by_line: BTreeMap<usize, Vec<&Entry>> = BTreeMap::new();
        for entry in &entries {
            by_line.entry(entry.key.line() - 1).or_default().push(entry);
        }
        for (number, mut entries) in by_line {
            let Some(line) = lines.get_mut(number) else {
                continue;
            };
            entries.sort_by_key(|e| e.key.col());
            // Multiline quoted keys and values, anchors, aliases and scalar contents stay literal.
            for entry in entries.iter().rev() {
                let key = entry.path.last().unwrap();
                let start = line
                    .char_indices()
                    .nth(entry.key.col())
                    .map(|p| p.0)
                    .unwrap_or(line.len());
                if !line[start..].starts_with(key) {
                    continue;
                }
                let colon = start + key.len();
                if line.as_bytes().get(colon) != Some(&b':') {
                    continue;
                }
                let rest = &line[colon + 1..];
                let spaces = rest.bytes().take_while(|b| *b == b' ').count();
                if spaces > 0
                    && !rest[spaces..].starts_with(['\r', '\n'])
                    && !rest[spaces..].is_empty()
                {
                    line.replace_range(colon + 1..colon + 1 + spaces, " ");
                }
            }
            if let Some(entry) = entries.first() {
                let indent = line.bytes().take_while(|c| *c == b' ').count();
                let content = &line[indent..];
                let dash = content.starts_with("- ");
                if entry.key.col() == indent + usize::from(dash) * 2 {
                    let wanted = (2 * (entry.path.len() - 1)).saturating_sub(usize::from(dash) * 2);
                    line.replace_range(..indent, &" ".repeat(wanted));
                }
            }
        }
        let formatted = lines.concat();
        // Whole-region equality also protects block scalars, unusual indent rules and aliases.
        if richview_core::yaml::from_str(&formatted).is_ok_and(|after| after == before) {
            output.replace_range(region.start..region.end, &formatted);
        }
    }
    output
}

// Check the actual slot when YAML punctuation may change the surrounding structure. Ordinary
// identifier-like names cannot introduce YAML syntax, so parse just the scalar for those; this
// keeps a completion over a large inline dataset from reparsing the whole dataset per column.
fn plain_field(
    field: &str,
    patched: &str,
    start: usize,
    original: &Value,
    path: &[String],
) -> bool {
    if field
        .chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_')
        && field
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-'))
    {
        return richview_core::yaml::from_str(field)
            .is_ok_and(|value| value == Value::String(field.to_owned()));
    }

    let mut expected = original.clone();
    let mut slot = &mut expected;
    for segment in path {
        let next = match slot {
            Value::Object(map) => map.get_mut(segment),
            Value::Array(items) => segment
                .parse::<usize>()
                .ok()
                .and_then(|index| items.get_mut(index)),
            _ => None,
        };
        let Some(next) = next else { return false };
        slot = next;
    }
    if slot.as_str() != Some("__richview_completion__") {
        return false;
    }
    *slot = Value::String(field.to_owned());
    let mut candidate = patched.to_owned();
    candidate.replace_range(start..start + "__richview_completion__".len(), field);
    richview_core::yaml::from_str(&candidate).is_ok_and(|value| value == expected)
}

pub fn complete(
    source: &str,
    line: usize,
    character: usize,
    document: &Document,
    options: &RenderOptions,
) -> Value {
    let Some(current) = source.split('\n').nth(line) else {
        return json!([]);
    };
    let mut units = 0;
    let mut byte = current.len();
    for (index, c) in current.char_indices() {
        if units >= character {
            byte = index;
            break;
        }
        units += c.len_utf16();
    }
    let prefix = &current[..byte];
    let absolute = source
        .split_inclusive('\n')
        .take(line)
        .map(str::len)
        .sum::<usize>()
        + byte;
    let regions = regions(source);
    let Some(region) = regions
        .iter()
        .find(|r| absolute >= r.start && absolute <= r.end)
    else {
        return json!([]);
    };
    // Suppress suggestions in comments/quoted scalar content; YAML remains the authority.
    let entries_before = entries(region.raw);
    if entries_before.iter().any(|e| {
        e.value.line() + region.line - 1 < line
            && e.scalar
                .as_ref()
                .is_some_and(|(_, s)| matches!(s, TScalarStyle::Literal | TScalarStyle::Folded))
            && current.starts_with(' ')
            && entries_before
                .iter()
                .rfind(|e| e.key.line() + region.line - 1 <= line)
                .is_some_and(|last| last.path == e.path)
    }) {
        return json!([]);
    };
    let token_start = prefix
        .char_indices()
        .rfind(|(_, c)| !(c.is_alphanumeric() || matches!(c, '_' | '-')))
        .map_or(0, |(index, c)| index + c.len_utf8());
    let fragment = &prefix[token_start..];
    let token_end = current[byte..]
        .find(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '-')))
        .map_or(current.len(), |p| byte + p);
    // A caret at the end of the region sits before the closing fence, which `---` makes look like
    // a word: the token stops where the region does.
    let token_end = token_end.min(byte + (region.end - absolute));
    let line_start = absolute - byte - region.start;
    let local_start = line_start + token_start;
    let local_end = line_start + token_end;
    let sentinel = "__richview_completion__";
    // Ask the YAML parser first whether this token is a scalar value (including
    // flow/block sequences and quoted enums), then try an unfinished mapping key.
    let mut patched = region.raw.to_owned();
    patched.replace_range(local_start..local_end, sentinel);
    let parsed = entries(&patched);
    let value_entry = parsed.into_iter().find(|e| {
        e.value.line() + region.line - 1 == line
            && e.scalar.as_ref().is_some_and(|(v, _)| v == sentinel)
            // A first unfinished key under `axes:`, `style:`, etc. also parses as
            // a scalar value. The shared vocabulary identifies these mapping slots.
            && !(e.key.line() < e.value.line()
                && richview_core::authoring_keys(
                    &region.tag,
                    &e.path.iter().map(String::as_str).collect::<Vec<_>>(),
                )
                .is_some())
    });
    let is_value = value_entry.is_some();
    let entry = if let Some(entry) = value_entry {
        entry
    } else {
        let mut patched = region.raw.to_owned();
        let replacement = if current[token_end..].trim_start().starts_with(':') {
            sentinel.to_owned()
        } else {
            format!("{sentinel}: null")
        };
        patched.replace_range(local_start..local_end, &replacement);
        let Some(entry) = entries(&patched).into_iter().find(|e| {
            e.key.line() + region.line - 1 == line && e.path.last().is_some_and(|p| p == sentinel)
        }) else {
            return json!([]);
        };
        entry
    };
    // Parser sees a sentinel inside a quoted value/comment as part of another scalar, not a key.
    let path: Vec<_> = entry.path.iter().map(String::as_str).collect();
    let mut choices: Vec<(String, String)> = vec![];
    if !is_value {
        if let Some(keys) = richview_core::authoring_keys(&region.tag, &path[..path.len() - 1]) {
            let colon = current[token_end..].trim_start().starts_with(':');
            // Under `encoding`, a chart reads only the channels its type lists in the registry.
            let channels = (region.tag == "chart" && path[..path.len() - 1] == ["encoding"])
                .then(|| {
                    let chart_type = entries_before
                        .iter()
                        .find(|e| e.path == ["type"])
                        .and_then(|e| e.scalar.as_ref())?;
                    richview_core::registry::CHART_TYPES
                        .iter()
                        .find(|chart| chart.name == chart_type.0)
                        .map(|chart| chart.encodings)
                })
                .flatten();
            for key in keys {
                if channels.is_some_and(|channels| !channels.contains(key)) {
                    continue;
                }
                let mut sibling = entry.path.clone();
                *sibling.last_mut().unwrap() = (*key).into();
                if !entries_before
                    .iter()
                    .any(|e| e.path == sibling && e.key.line() != entry.key.line())
                {
                    choices.push((
                        (*key).into(),
                        if colon {
                            (*key).into()
                        } else {
                            format!("{key}: ")
                        },
                    ));
                }
            }
        }
    } else {
        choices.extend(
            richview_core::authoring_values(&region.tag, &path)
                .iter()
                .map(|s| (s.to_string(), s.to_string())),
        );
        let parent = &path[..path.len() - 1];
        let field = (region.tag == "chart" && parent == ["encoding"])
            || (region.tag == "table"
                && path.last() == Some(&"key")
                && matches!(parent.first(), Some(&"columns" | &"sort")));
        if field && let Ok(value) = richview_core::yaml::from_str(&patched) {
            let block = richview_core::document::RichBlock {
                tag: region.tag.clone(),
                value,
                raw: patched.clone(),
                line: region.line,
            };
            choices.extend(
                richview_core::authoring_fields(&block, &document.frontmatter, options)
                    .into_iter()
                    .filter(|s| s.starts_with(fragment))
                    .map(|s| {
                        let encoded = serde_json::to_string(&s).unwrap();
                        let insert = match entry.scalar.as_ref().map(|(_, style)| style) {
                            Some(TScalarStyle::SingleQuoted) => s.replace("'", "''"),
                            Some(TScalarStyle::DoubleQuoted) => {
                                encoded[1..encoded.len() - 1].to_owned()
                            }
                            _ if plain_field(
                                &s,
                                &patched,
                                local_start,
                                &block.value,
                                &entry.path,
                            ) =>
                            {
                                s.clone()
                            }
                            _ => encoded,
                        };
                        (s, insert)
                    }),
            );
        }
    }
    let start = current[..token_start].encode_utf16().count();
    let end = current[..token_end].encode_utf16().count();
    json!(choices.into_iter().filter(|(label,_)|label.starts_with(fragment)).map(|(label,insert)|json!({"label":label,"insert_text":insert,"kind":if is_value {"value"} else {"key"},"range":{"start":{"line":line,"character":start},"end":{"line":line,"character":end}}})).collect::<Vec<_>>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_caret_before_the_closing_fence_of_the_front_matter_replaces_nothing_after_it() {
        let source = "---\ntitle: T\nversion: 0.0.1\n---\n\nBody\n";
        let document = richview_core::document::parse(source).expect("the source parses");
        let line = source[..source.find("---\n\nBody").unwrap()]
            .matches('\n')
            .count();
        // The fence is no word to complete: whatever is offered replaces an empty stretch at the
        // caret and leaves the fence alone.
        let found = complete(source, line, 0, &document, &RenderOptions::default());
        for item in found.as_array().expect("a list") {
            assert_eq!(
                item["range"]["start"],
                json!({"line": line, "character": 0})
            );
            assert_eq!(item["range"]["end"], json!({"line": line, "character": 0}));
        }
    }
}
