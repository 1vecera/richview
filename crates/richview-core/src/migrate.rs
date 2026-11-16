//! The one migration (TYP-83, INT-51): rewrites a source written for an older format into the
//! current one. A retired name never renders silently; it fails with its diagnostic, and this
//! rewrites what has a mechanical fix and reports the rest for the author.
//!
//! Edits are textual and by position, so comments, quoting and layout survive. The rules are the
//! registry's `RETIRED` table: each entry names its `Migration`. A source that changed gains
//! `format: rmd/1` (TYP-05).

use crate::registry::{Migration, RETIRED, Retired};
use crate::yaml::{self, Pos};
use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag};
use regex_lite::Regex;
use serde_json::Value;
use std::collections::BTreeMap;

/// What a migration did to one source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Migrated {
    /// The rewritten source; equal to the input when nothing changed.
    pub source: String,
    /// One line per rewrite: `line 3: renamed title-image to title_image`.
    pub changes: Vec<String>,
    /// Retired names with no mechanical fix, as `line 5: frontmatter.data ...`.
    pub manual: Vec<String>,
}

impl Migrated {
    pub fn changed(&self) -> bool {
        !self.changes.is_empty()
    }
}

/// A textual edit at a zero-based line.
enum Edit {
    /// Replace `len` characters at `col` (zero-based) with `text`.
    Replace {
        line: usize,
        col: usize,
        len: usize,
        text: String,
    },
    /// Delete lines `from..to`.
    Delete { from: usize, to: usize },
    /// Insert a line after `line`.
    InsertAfter { line: usize, text: String },
}

impl Edit {
    fn line(&self) -> usize {
        match self {
            Edit::Replace { line, .. } | Edit::InsertAfter { line, .. } => *line,
            Edit::Delete { from, .. } => *from,
        }
    }
    fn col(&self) -> usize {
        match self {
            Edit::Replace { col, .. } => *col,
            _ => 0,
        }
    }
}

/// Migrate `source` to the current format.
pub(crate) fn migrate(source: &str) -> Migrated {
    let bom = source.starts_with('\u{feff}');
    let text = source.strip_prefix('\u{feff}').unwrap_or(source);
    let crlf = text.contains("\r\n");
    let mut lines: Vec<String> = text
        .replace("\r\n", "\n")
        .split('\n')
        .map(str::to_owned)
        .collect();
    let mut changes = vec![];
    let mut manual = vec![];
    let mut edits: Vec<Edit> = vec![];
    let mut body_prefix = String::new();

    // Frontmatter: lines 1..=close, the YAML in between.
    let mut body_start = 0;
    let mut front = None;
    if lines.first().is_some_and(|l| l.trim_end() == "---")
        && let Some(close) = lines[1..]
            .iter()
            .position(|l| matches!(l.trim_end(), "---" | "..."))
            .map(|i| i + 1)
    {
        body_start = close + 1;
        front = Some(close);
        let yaml_text = lines[1..close].join("\n");
        if let Ok(loaded) = yaml::load(&yaml_text) {
            frontmatter_edits(
                &lines,
                close,
                &loaded,
                &mut edits,
                &mut changes,
                &mut manual,
                &mut body_prefix,
            );
        }
    }
    block_edits(&lines, body_start, &mut edits, &mut changes, &mut manual);

    // Apply from the bottom up so earlier line numbers stay valid.
    edits.sort_by_key(|e| std::cmp::Reverse((e.line(), e.col())));
    for edit in edits {
        match edit {
            Edit::Replace {
                line,
                col,
                len,
                text,
            } => {
                let chars: Vec<char> = lines[line].chars().collect();
                let end = (col + len).min(chars.len());
                lines[line] = chars[..col.min(chars.len())].iter().collect::<String>()
                    + &text
                    + &chars[end..].iter().collect::<String>();
            }
            Edit::Delete { from, to } => {
                lines.drain(from..to.min(lines.len()));
            }
            Edit::InsertAfter { line, text } => lines.insert(line + 1, text),
        }
    }
    if !changes.is_empty() {
        if let Some(close) = front {
            // Recompute the closing line: deletions and insertions moved it.
            let close = lines[1..]
                .iter()
                .position(|l| matches!(l.trim_end(), "---" | "..."))
                .map_or(close, |i| i + 1);
            if !lines[1..close]
                .iter()
                .any(|l| l.starts_with("format:") || l.starts_with("format :"))
            {
                lines.insert(close, format!("format: rmd/{}", crate::FORMAT_MAJOR));
                changes.push(format!("added format: rmd/{}", crate::FORMAT_MAJOR));
            }
        }
        if !body_prefix.is_empty() {
            let close = front
                .and_then(|_| {
                    lines[1..]
                        .iter()
                        .position(|l| matches!(l.trim_end(), "---" | "..."))
                        .map(|i| i + 2)
                })
                .unwrap_or(0);
            let prefix: Vec<String> = body_prefix
                .trim_end()
                .split('\n')
                .map(str::to_owned)
                .collect();
            let mut at = close;
            lines.insert(at, String::new());
            at += 1;
            for line in prefix {
                lines.insert(at, line);
                at += 1;
            }
            if lines.get(at).is_some_and(|l| !l.trim().is_empty()) {
                lines.insert(at, String::new());
            }
        }
    }
    let mut out = lines.join(if crlf { "\r\n" } else { "\n" });
    if bom {
        out.insert(0, '\u{feff}');
    }
    Migrated {
        source: out,
        changes,
        manual,
    }
}

/// The zero-based line and character column of a recorded position inside YAML that starts at
/// file line `first` (zero-based).
fn at(first: usize, pos: Pos) -> (usize, usize) {
    (first + pos.line - 1, pos.column - 1)
}

fn frontmatter_edits(
    lines: &[String],
    close: usize,
    loaded: &yaml::Loaded,
    edits: &mut Vec<Edit>,
    changes: &mut Vec<String>,
    manual: &mut Vec<String>,
    body_prefix: &mut String,
) {
    let Some(map) = loaded.value.as_object() else {
        return;
    };
    // Top-level keys by line, to find where each key's lines end.
    let mut tops: Vec<(usize, &str)> = map
        .keys()
        .filter_map(|k| loaded.positions.get(k).map(|p| (at(1, *p).0, k.as_str())))
        .collect();
    tops.sort();
    let extent = |key: &str| -> (usize, usize) {
        let i = tops.iter().position(|(_, k)| *k == key).unwrap();
        let from = tops[i].0;
        let mut to = tops.get(i + 1).map_or(close, |(l, _)| *l);
        // Comments and blank lines before the next key belong to it.
        while to > from + 1
            && (lines[to - 1].trim().is_empty() || lines[to - 1].trim_start().starts_with('#'))
        {
            to -= 1;
        }
        (from, to)
    };
    for entry in RETIRED.iter().filter(|r| r.scope == "frontmatter") {
        let Some(value) = map.get(entry.path) else {
            continue;
        };
        let Some(pos) = loaded.positions.get(entry.path) else {
            continue;
        };
        let (line, col) = at(1, *pos);
        let shown = line + 1;
        match entry.migration {
            Migration::Rename(to) => {
                if map.contains_key(to) {
                    manual.push(format!(
                        "line {shown}: frontmatter.{} duplicates {to}; remove one",
                        entry.path
                    ));
                } else {
                    edits.push(Edit::Replace {
                        line,
                        col,
                        len: entry.path.chars().count(),
                        text: to.to_owned(),
                    });
                    changes.push(format!("line {shown}: renamed {} to {to}", entry.path));
                }
            }
            Migration::RenameElseRemove(to) if !map.contains_key(to) => {
                edits.push(Edit::Replace {
                    line,
                    col,
                    len: entry.path.chars().count(),
                    text: to.to_owned(),
                });
                changes.push(format!("line {shown}: renamed {} to {to}", entry.path));
            }
            Migration::Remove | Migration::RenameElseRemove(_) => {
                let (from, to) = extent(entry.path);
                edits.push(Edit::Delete { from, to });
                changes.push(format!("line {shown}: removed {}: {}", entry.path, value));
                if matches!(value.as_str(), Some("light" | "dark")) {
                    let style = if value == "dark" { "sleek" } else { "air" };
                    manual.push(format!(
                        "line {shown}: theme {value} was removed; add `styles: [{style}]` if the colour scheme matters"
                    ));
                }
            }
            Migration::ToDescription => {
                let empty = map
                    .get("description")
                    .is_none_or(|d| d.as_str().is_none_or(|s| s.trim().is_empty()));
                if empty && !map.contains_key("description") {
                    edits.push(Edit::Replace {
                        line,
                        col,
                        len: entry.path.chars().count(),
                        text: "description".into(),
                    });
                    changes.push(format!(
                        "line {shown}: renamed {} to description",
                        entry.path
                    ));
                } else {
                    manual.push(format!(
                        "line {shown}: frontmatter.subtitle stays until you merge it into description"
                    ));
                }
            }
            Migration::ToBody => {
                if let Some(text) = value.as_str() {
                    let (from, to) = extent(entry.path);
                    edits.push(Edit::Delete { from, to });
                    body_prefix.push_str(text.trim_end());
                    body_prefix.push('\n');
                    changes.push(format!("line {shown}: moved prolog to the top of the body"));
                }
            }
            Migration::Manual => manual.push(format!(
                "line {shown}: frontmatter.{} {}",
                entry.path, entry.fix
            )),
        }
    }
}

/// Fence edits: retired keys inside rich blocks, by the registry's table.
fn block_edits(
    lines: &[String],
    body_start: usize,
    edits: &mut Vec<Edit>,
    changes: &mut Vec<String>,
    manual: &mut Vec<String>,
) {
    let body = lines[body_start..].join("\n");
    let mut fences: Vec<(usize, String)> = vec![];
    for (event, range) in Parser::new_ext(&body, crate::markdown::FLAGS).into_offset_iter() {
        if let Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) = event {
            let line = body[..range.start].bytes().filter(|b| *b == b'\n').count();
            fences.push((body_start + line, info.trim().to_lowercase()));
        }
    }
    for (open, mut tag) in fences {
        if let Some(new) = crate::registry::retired_tag(&tag) {
            let at = lines[open].to_lowercase().find(&tag);
            if let (Some(col), true) = (at, !lines[open].starts_with(' ')) {
                edits.push(Edit::Replace {
                    line: open,
                    col,
                    len: tag.chars().count(),
                    text: new.to_owned(),
                });
                changes.push(format!(
                    "line {}: renamed the {tag} block to {new}",
                    open + 1
                ));
                tag = new.to_owned();
            }
        }
        if crate::registry::tag(&tag).is_none() && !RETIRED.iter().any(|r| r.scope == tag) {
            continue;
        }
        let Some(close) = lines[open + 1..]
            .iter()
            .position(|l| l.trim_start().starts_with("```") || l.trim_start().starts_with("~~~"))
            .map(|i| open + 1 + i)
        else {
            continue;
        };
        if lines[open].starts_with(' ') {
            manual.push(format!(
                "line {}: a {tag} fence inside a list or quote is not migrated",
                open + 1
            ));
            continue;
        }
        let yaml_text = lines[open + 1..close].join("\n");
        let Ok(loaded) = yaml::load(&yaml_text) else {
            continue;
        };
        key_edits(&tag, open + 1, lines, &loaded, edits, changes, manual);
    }
}

/// Rename or remove retired keys of one block, by position.
fn key_edits(
    tag: &str,
    first: usize,
    lines: &[String],
    loaded: &yaml::Loaded,
    edits: &mut Vec<Edit>,
    changes: &mut Vec<String>,
    manual: &mut Vec<String>,
) {
    let positions: &BTreeMap<String, Pos> = &loaded.positions;
    for (path, pos) in positions {
        let pattern = indexless(path);
        let Some(entry): Option<&Retired> = crate::registry::retired(tag, &pattern) else {
            continue;
        };
        let (line, col) = at(first, *pos);
        let shown = line + 1;
        let key = path.rsplit('.').next().unwrap_or(path);
        match entry.migration {
            Migration::Rename(to) => {
                edits.push(Edit::Replace {
                    line,
                    col,
                    len: key.chars().count(),
                    text: to.to_owned(),
                });
                changes.push(format!("line {shown}: renamed {key} to {to} in {tag}"));
            }
            Migration::RenameElseRemove(to)
                if value_at(&loaded.value, path.rsplit_once('.').map_or("", |(p, _)| p))
                    .and_then(|m| m.as_object())
                    .is_none_or(|m| !m.contains_key(to)) =>
            {
                edits.push(Edit::Replace {
                    line,
                    col,
                    len: key.chars().count(),
                    text: to.to_owned(),
                });
                changes.push(format!("line {shown}: renamed {key} to {to} in {tag}"));
            }
            _ => manual.push(format!("line {shown}: {tag}.{pattern} {}", entry.fix)),
        }
    }
    for (path, pos) in positions {
        let Some(Value::String(old)) = value_at(&loaded.value, path) else {
            continue;
        };
        let Some(rule) = crate::registry::retired_value(tag, &indexless(path), old) else {
            continue;
        };
        let (line, col) = at(first, *pos);
        let key = path.rsplit('.').next().unwrap_or(path);
        let text: Vec<char> = lines[line].chars().collect();
        let after: String = text[col.min(text.len())..].iter().collect();
        let pattern = format!(
            r#"^{}\s*:\s*(["']?)({})(["']?)"#,
            regex_lite::escape(key.split('[').next().unwrap_or(key)),
            regex_lite::escape(old)
        );
        let Some(found) = Regex::new(&pattern).ok().and_then(|re| re.captures(&after)) else {
            manual.push(format!(
                "line {}: {tag}.{} '{old}' becomes {} with unit {}",
                line + 1,
                indexless(path),
                rule.replacement,
                rule.unit
            ));
            continue;
        };
        let value = found.get(2).unwrap();
        let start = col + after[..value.start()].chars().count();
        edits.push(Edit::Replace {
            line,
            col: start,
            len: old.chars().count(),
            text: rule.replacement.to_owned(),
        });
        let parent = path.rsplit_once('.').map_or("", |(p, _)| p);
        let has_unit = match value_at(&loaded.value, parent) {
            Some(Value::Object(map)) => map.contains_key("unit") || map.contains_key("suffix"),
            _ if parent.is_empty() => loaded
                .value
                .as_object()
                .is_some_and(|m| m.contains_key("unit")),
            _ => false,
        };
        if !has_unit {
            let rest = &after[found.get(0).unwrap().end()..];
            let flow = rest.trim_start().starts_with(['}', ',']) && rest.contains('}');
            if flow {
                edits.push(Edit::Replace {
                    line,
                    col: col + after[..found.get(0).unwrap().end()].chars().count(),
                    len: 0,
                    text: format!(", unit: {}", rule.unit),
                });
            } else {
                edits.push(Edit::InsertAfter {
                    line,
                    text: format!("{}unit: {}", " ".repeat(col), rule.unit),
                });
            }
        }
        changes.push(format!(
            "line {}: {tag} format {old} became {} with unit {}",
            line + 1,
            rule.replacement,
            rule.unit
        ));
    }
}

/// The value at a diagnostic-grammar path such as `columns[1].format`.
fn value_at<'v>(root: &'v Value, path: &str) -> Option<&'v Value> {
    let mut at = root;
    if path.is_empty() {
        return Some(at);
    }
    for segment in path.split('.') {
        let (name, rest) = segment
            .split_once('[')
            .map_or((segment, ""), |(n, r)| (n, r));
        if !name.is_empty() {
            at = at.get(name)?;
        }
        for index in rest.split('[').filter(|r| !r.is_empty()) {
            at = at.get(index.trim_end_matches(']').parse::<usize>().ok()?)?;
        }
    }
    Some(at)
}

fn indexless(path: &str) -> String {
    let mut out = String::new();
    let mut skipping = false;
    for c in path.chars() {
        match c {
            '[' => {
                out.push_str("[n]");
                skipping = true;
            }
            ']' => skipping = false,
            _ if skipping => {}
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::migrate;

    #[test]
    fn retired_frontmatter_keys_are_rewritten_in_place_and_the_format_is_marked() {
        let source = "---\ntitle: T\nsubtitle: The deck\nimage: cover.png\ntheme: auto\n# keep\nprolog: |\n  Opening text.\n\n  Second paragraph.\nversion: 0.0.1\n---\n\nBody.\n";
        let m = migrate(source);
        assert_eq!(
            m.source,
            "---\ntitle: T\ndescription: The deck\ntitle_image: cover.png\n# keep\nversion: 0.0.1\nformat: rmd/1\n---\n\nOpening text.\n\nSecond paragraph.\n\nBody.\n"
        );
        assert!(m.manual.is_empty(), "{:?}", m.manual);
        assert!(
            crate::validate(&m.source)
                .iter()
                .all(|d| d.level != "error")
        );
    }

    #[test]
    fn money_formats_become_number_formats_with_a_currency_unit() {
        let source = "---\ntitle: T\n---\n\n```metric\nvalue: 1\nformat: currency_compact\n```\n\n```chart\ntype: line\naxes:\n  y: { label: MRR, format: currency }\n```\n\n```table\ndata: {inline: []}\ncolumns:\n  - key: a\n    format: currency\n    unit: €\n```\n";
        let m = migrate(source);
        assert_eq!(
            m.source,
            "---\ntitle: T\nformat: rmd/1\n---\n\n```metric\nvalue: 1\nformat: number_compact\nunit: $\n```\n\n```chart\ntype: line\naxes:\n  y: { label: MRR, format: number, unit: $ }\n```\n\n```table\ndata: {inline: []}\ncolumns:\n  - key: a\n    format: number\n    unit: €\n```\n"
        );
    }

    #[test]
    fn a_subtitle_beside_a_description_waits_for_the_author() {
        let source = "---\ntitle: T\ndescription: D\nsubtitle: S\n---\n";
        let m = migrate(source);
        assert!(!m.changed());
        assert_eq!(m.source, source);
        assert_eq!(m.manual.len(), 1);
    }

    #[test]
    fn a_current_source_is_left_alone_and_line_endings_survive() {
        let source = "---\r\ntitle: T\r\n---\r\n\r\nBody.\r\n";
        assert_eq!(migrate(source).source, source);
        let old = "---\r\ntitle: T\r\ntheme: dark\r\n---\r\n";
        let m = migrate(old);
        assert_eq!(m.source, "---\r\ntitle: T\r\nformat: rmd/1\r\n---\r\n");
        assert_eq!(m.manual.len(), 1, "dark names a style to pick");
    }
}
