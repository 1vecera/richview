//! Presentation-only syntax highlighting for Rich Markdown, YAML, shell and SQL.
//!
//! The output is escaped HTML whose text content is byte-for-byte the input, so an editor can
//! overlay a transparent text area on it. Classes are stable `hl-*` names; hosts style them.

use crate::RICH_TAGS;
use regex_lite::Regex;
use std::fmt::Write;
use std::sync::LazyLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HighlightLanguage {
    /// Markdown prose, YAML frontmatter and fenced rich blocks.
    RichMarkdown,
    /// A `.rstyle.yaml` file, chart block body or other YAML document.
    Yaml,
    /// Command examples: commands, flags, environment variables and comments.
    Shell,
    /// SQL keywords, literals and comments.
    Sql,
    /// Directory trees and other plain listings.
    Plain,
}

impl HighlightLanguage {
    pub fn from_name(name: &str) -> Self {
        match name.trim().to_ascii_lowercase().as_str() {
            "rmd" | "md" | "markdown" | "richmarkdown" => Self::RichMarkdown,
            "yaml" | "yml" | "rstyle" => Self::Yaml,
            "sh" | "shell" | "bash" | "console" => Self::Shell,
            "sql" => Self::Sql,
            _ => Self::Plain,
        }
    }
}

/// Highlight `source` as escaped HTML spans. Stripping the tags and unescaping the entities
/// returns the original text, including line endings and trailing whitespace.
pub fn highlight_html(source: &str, language: HighlightLanguage) -> String {
    let mut out = Out(String::with_capacity(source.len() * 2));
    match language {
        HighlightLanguage::RichMarkdown => rich_markdown(source, &mut out),
        HighlightLanguage::Yaml => {
            let mut yaml = Yaml::default();
            for line in lines(source) {
                yaml.line(line, &mut out);
            }
        }
        HighlightLanguage::Shell => {
            for line in lines(source) {
                shell(line, &mut out);
            }
        }
        HighlightLanguage::Sql => sql(source, &mut out),
        HighlightLanguage::Plain => out.plain(source),
    }
    out.0
}

struct Out(String);
impl Out {
    fn plain(&mut self, text: &str) {
        escape_into(&mut self.0, text);
    }
    fn span(&mut self, class: &str, text: &str) {
        if text.is_empty() {
            return;
        }
        let _ = write!(self.0, "<span class=\"hl-{class}\">");
        escape_into(&mut self.0, text);
        self.0.push_str("</span>");
    }
}

fn escape_into(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
}

/// Lines including their terminators, so output text equals input text exactly.
fn lines(source: &str) -> impl Iterator<Item = &str> {
    source.split_inclusive('\n')
}

fn split_eol(line: &str) -> (&str, &str) {
    let body = line.trim_end_matches(['\n', '\r']);
    (body, &line[body.len()..])
}

/// Read through a closing quote, including doubled delimiters and optional backslash escapes.
fn quoted_len(text: &str, backslash_escapes: bool) -> usize {
    let quote = text.as_bytes()[0];
    let mut bytes = text.bytes().enumerate().skip(1).peekable();
    while let Some((index, byte)) = bytes.next() {
        if backslash_escapes && byte == b'\\' {
            bytes.next();
        } else if byte == quote {
            if bytes.peek().is_some_and(|(_, next)| *next == quote) {
                bytes.next();
            } else {
                return index + 1;
            }
        }
    }
    text.len()
}

fn rich_markdown(source: &str, out: &mut Out) {
    enum Mode {
        Prose,
        Frontmatter(Yaml),
        Fence { marker: String, yaml: Option<Yaml> },
    }
    let mut mode = Mode::Prose;
    for (index, line) in lines(source).enumerate() {
        let (body, eol) = split_eol(line);
        let trimmed = body.trim_start();
        mode = match mode {
            Mode::Prose if index == 0 && body.trim_end() == "---" => {
                out.span("fm", body);
                out.plain(eol);
                Mode::Frontmatter(Yaml::default())
            }
            Mode::Frontmatter(_) if body.trim_end() == "---" || body.trim_end() == "..." => {
                out.span("fm", body);
                out.plain(eol);
                Mode::Prose
            }
            Mode::Frontmatter(mut yaml) => {
                yaml.line(line, out);
                Mode::Frontmatter(yaml)
            }
            Mode::Prose if trimmed.starts_with("```") || trimmed.starts_with("~~~") => {
                let indent = &body[..body.len() - trimmed.len()];
                let fence_char = trimmed.chars().next().unwrap_or('`');
                let marker_len = trimmed.chars().take_while(|c| *c == fence_char).count();
                let (marker, info) = trimmed.split_at(marker_len);
                let tag = info.trim().to_ascii_lowercase();
                out.plain(indent);
                out.span("fence", marker);
                out.span(
                    if RICH_TAGS.contains(&tag.as_str()) {
                        "tag"
                    } else {
                        "info"
                    },
                    info,
                );
                out.plain(eol);
                let yaml = (RICH_TAGS.contains(&tag.as_str()) || tag == "yaml" || tag == "yml")
                    .then(Yaml::default);
                Mode::Fence {
                    marker: marker.into(),
                    yaml,
                }
            }
            Mode::Fence { marker, .. }
                if trimmed.starts_with(marker.as_str())
                    && trimmed.trim_end().chars().all(|c| marker.starts_with(c)) =>
            {
                out.plain(&body[..body.len() - trimmed.len()]);
                out.span("fence", trimmed);
                out.plain(eol);
                Mode::Prose
            }
            Mode::Fence {
                marker,
                yaml: Some(mut yaml),
            } => {
                yaml.line(line, out);
                Mode::Fence {
                    marker,
                    yaml: Some(yaml),
                }
            }
            Mode::Fence { marker, yaml: None } => {
                out.span("code", body);
                out.plain(eol);
                Mode::Fence { marker, yaml: None }
            }
            Mode::Prose => {
                prose(body, out);
                out.plain(eol);
                Mode::Prose
            }
        };
    }
}

fn prose(body: &str, out: &mut Out) {
    let trimmed = body.trim_start();
    let indent = &body[..body.len() - trimmed.len()];
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
        out.plain(indent);
        out.span("mark", &trimmed[..hashes]);
        out.span("heading", &trimmed[hashes..]);
        return;
    }
    out.plain(indent);
    let mut rest = trimmed;
    for marker in ["- ", "* ", "+ ", "> "] {
        if let Some(after) = rest.strip_prefix(marker) {
            out.span("mark", marker);
            rest = after;
            break;
        }
    }
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && rest[digits..].starts_with(". ") {
        out.span("mark", &rest[..digits + 2]);
        rest = &rest[digits + 2..];
    }
    inline(rest, out);
}

/// Inline Markdown: code spans, strong/emphasis, links and escapes.
fn inline(text: &str, out: &mut Out) {
    let mut plain_start = 0;
    let mut i = 0;
    let bytes = text.as_bytes();
    let flush = |out: &mut Out, from: usize, to: usize| out.plain(&text[from..to]);
    while i < bytes.len() {
        let rest = &text[i..];
        let matched = if rest.starts_with('\\') && rest.len() > 1 {
            let len = 1 + rest[1..].chars().next().map_or(0, char::len_utf8);
            Some(("esc", len))
        } else if let Some(after) = rest.strip_prefix('`') {
            after.find('`').map(|end| ("inline", end + 2))
        } else if rest.starts_with("**") || rest.starts_with("__") {
            let marker = &rest[..2];
            rest[2..]
                .find(marker)
                .filter(|end| *end > 0)
                .map(|end| ("strong", end + 4))
        } else if (rest.starts_with('*') || rest.starts_with('_'))
            && !rest[1..].starts_with(' ')
            && (i == 0 || !text[..i].ends_with(|c: char| c.is_alphanumeric()))
        {
            let marker = &rest[..1];
            rest[1..]
                .find(marker)
                .filter(|end| *end > 0 && !rest[..end + 1].ends_with(' '))
                .map(|end| ("em", end + 2))
        } else if rest.starts_with('[') {
            rest.find("](").and_then(|mid| {
                rest[mid..]
                    .find(')')
                    .map(|end| ("link", mid + end + 1))
                    .filter(|(_, len)| !rest[..*len].contains('\n'))
            })
        } else {
            None
        };
        match matched {
            Some((class, len)) => {
                flush(out, plain_start, i);
                if class == "link" {
                    let mid = rest.find("](").unwrap_or(0);
                    out.span("link", &rest[..mid + 1]);
                    out.span("url", &rest[mid + 1..len]);
                } else {
                    out.span(class, &rest[..len]);
                }
                i += len;
                plain_start = i;
            }
            None => i += rest.chars().next().map_or(1, char::len_utf8),
        }
    }
    flush(out, plain_start, text.len());
}

/// Line-oriented YAML highlighting with block-scalar tracking.
#[derive(Default)]
struct Yaml {
    /// Indentation that starts a literal/folded block scalar's parent key.
    block_parent: Option<usize>,
}

impl Yaml {
    fn line(&mut self, line: &str, out: &mut Out) {
        let (body, eol) = split_eol(line);
        let trimmed = body.trim_start();
        let indent = body.len() - trimmed.len();
        if let Some(parent) = self.block_parent {
            if trimmed.is_empty() || indent > parent {
                out.plain(&body[..indent]);
                out.span("str", trimmed);
                out.plain(eol);
                return;
            }
            self.block_parent = None;
        }
        out.plain(&body[..indent]);
        if trimmed.starts_with('#') {
            out.span("comment", trimmed);
            out.plain(eol);
            return;
        }
        let mut rest = trimmed;
        while let Some(after) = rest
            .strip_prefix("- ")
            .or_else(|| (rest == "-").then_some(""))
        {
            out.span("punc", "-");
            out.plain(&rest[1..rest.len() - after.len()]);
            rest = after;
        }
        if let Some(key_len) = block_key(rest) {
            out.span("key", &rest[..key_len]);
            out.span("punc", ":");
            rest = &rest[key_len + 1..];
        }
        let (value, comment) = split_comment(rest);
        let value_trimmed = value.trim();
        if value_trimmed.starts_with('|') || value_trimmed.starts_with('>') {
            self.block_parent = Some(indent);
            out.plain(&value[..value.len() - value.trim_start().len()]);
            out.span("punc", value.trim_start());
        } else {
            scalar_or_flow(value, out);
        }
        out.span("comment", comment);
        out.plain(eol);
    }
}

/// Length of a plain or quoted mapping key followed by `:` and a space/end, if any.
fn block_key(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let end = match bytes.first()? {
        b'"' | b'\'' => quoted_len(text, bytes[0] == b'"'),
        _ => text
            .char_indices()
            .find(|(_, c)| matches!(c, ':' | '#' | '{' | '}' | '[' | ']' | ','))
            .map(|(i, _)| i)?,
    };
    let after = text.get(end..)?;
    (after.starts_with(':')
        && (after.len() == 1 || after[1..].starts_with([' ', '\t']))
        && end > 0
        && !text[..end].trim().is_empty())
    .then_some(end)
}

/// Split a value from a trailing ` #comment`, ignoring `#` inside quotes.
fn split_comment(text: &str) -> (&str, &str) {
    let mut rest = text;
    let mut previous = ' ';
    while let Some(c) = rest.chars().next() {
        let space = previous.is_whitespace();
        if c == '#' && space {
            let value_end = text[..text.len() - rest.len()].trim_end().len();
            return (&text[..value_end], &text[value_end..]);
        }
        let len = if matches!(c, '"' | '\'') && (space || matches!(previous, '[' | '{' | ',' | ':'))
        {
            quoted_len(rest, c == '"')
        } else {
            c.len_utf8()
        };
        rest = &rest[len..];
        previous = c;
    }
    (text, "")
}

/// Flow collections and scalars: `{ key: value, n: 3 }`, `[a, "b"]`, `plain words`.
fn scalar_or_flow(value: &str, out: &mut Out) {
    let mut i = 0;
    while i < value.len() {
        let rest = &value[i..];
        let c = rest.chars().next().unwrap_or(' ');
        if c.is_whitespace() {
            let len = rest.len() - rest.trim_start().len();
            out.plain(&rest[..len]);
            i += len;
            continue;
        }
        if matches!(c, '{' | '}' | '[' | ']' | ',') {
            out.span("punc", &rest[..1]);
            i += 1;
            continue;
        }
        if c == '"' || c == '\'' {
            let len = quoted_len(rest, c == '"');
            let after = rest[len..].trim_start();
            out.span(
                if after.starts_with(':') { "key" } else { "str" },
                &rest[..len],
            );
            if let Some(value) = after.strip_prefix(':') {
                out.plain(&rest[len..rest.len() - after.len()]);
                out.span("punc", ":");
                i += rest.len() - value.len();
                continue;
            }
            i += len;
            continue;
        }
        // A plain word ends at flow punctuation or at a `key:` separator.
        let end = rest
            .char_indices()
            .find(|(j, ch)| {
                matches!(ch, ',' | '}' | ']' | '{' | '[')
                    || (*ch == ':'
                        && (rest[j + 1..].is_empty() || rest[j + 1..].starts_with([' ', '\t'])))
            })
            .map_or(rest.len(), |(j, _)| j);
        let word = rest[..end].trim_end();
        if word.is_empty() {
            // A lone ':' separator.
            out.span("punc", &rest[..1]);
            i += 1;
            continue;
        }
        if rest[end..].starts_with(':') {
            out.span("key", word);
            out.plain(&rest[word.len()..end]);
            out.span("punc", ":");
            i += end + 1;
        } else {
            out.span(scalar_class(word), word);
            i += word.len();
        }
    }
}

fn scalar_class(word: &str) -> &'static str {
    match word.to_ascii_lowercase().as_str() {
        "true" | "false" | "yes" | "no" | "on" | "off" => "bool",
        "null" | "~" => "null",
        ".inf" | "-.inf" | "+.inf" | ".nan" => "num",
        _ if is_number(word) => "num",
        _ if crate::compat::date_shape(word) => "date",
        _ => "val",
    }
}

fn is_number(word: &str) -> bool {
    word.parse::<f64>().is_ok_and(f64::is_finite)
}

fn sql(source: &str, out: &mut Out) {
    // At a token boundary, a number may start with a dot and use scientific notation.
    static NUMBER: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?")
            .expect("SQL number pattern")
    });
    let mut rest = source;
    while let Some(first) = rest.chars().next() {
        let (class, len) = if rest.starts_with("--") {
            (
                Some("comment"),
                rest.find(['\r', '\n']).unwrap_or(rest.len()),
            )
        } else if rest.starts_with("/*") {
            (Some("comment"), sql_comment_len(rest))
        } else if first == '\'' {
            (Some("str"), quoted_len(rest, false))
        } else if first == '"' {
            // Standard SQL double quotes delimit identifiers.
            (None, quoted_len(rest, false))
        } else if let Some(number) = (first.is_ascii_digit() || first == '.')
            .then(|| NUMBER.find(rest))
            .flatten()
            .filter(|number| !rest[number.end()..].starts_with(sql_word_char))
        {
            (Some("num"), number.end())
        } else if sql_word_char(first) {
            let len = rest.find(|c| !sql_word_char(c)).unwrap_or(rest.len());
            (sql_word_class(&rest[..len]), len)
        } else {
            (None, first.len_utf8())
        };
        let (token, tail) = rest.split_at(len);
        if let Some(class) = class {
            out.span(class, token);
        } else {
            out.plain(token);
        }
        rest = tail;
    }
}

fn sql_comment_len(text: &str) -> usize {
    let mut bytes = text.as_bytes().iter().enumerate().skip(2).peekable();
    let mut depth = 1;
    while let Some((index, byte)) = bytes.next() {
        match (byte, bytes.peek().map(|(_, next)| **next)) {
            (b'/', Some(b'*')) => depth += 1,
            (b'*', Some(b'/')) => {
                depth -= 1;
                if depth == 0 {
                    return index + 2;
                }
            }
            _ => continue,
        }
        bytes.next();
    }
    text.len()
}

fn sql_word_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '$')
}

fn sql_word_class(word: &str) -> Option<&'static str> {
    match word.to_ascii_uppercase().as_str() {
        "TRUE" | "FALSE" => Some("bool"),
        "NULL" => Some("null"),
        "ALL" | "ALTER" | "AND" | "AS" | "ASC" | "BETWEEN" | "BY" | "CASE" | "CAST" | "CHECK"
        | "CREATE" | "CROSS" | "DATE" | "DEFAULT" | "DELETE" | "DESC" | "DISTINCT" | "DROP"
        | "ELSE" | "END" | "ESCAPE" | "EXCEPT" | "EXISTS" | "FETCH" | "FIRST" | "FOLLOWING"
        | "FOREIGN" | "FROM" | "FULL" | "GROUP" | "HAVING" | "IN" | "INDEX" | "INNER"
        | "INSERT" | "INTERSECT" | "INTERVAL" | "INTO" | "IS" | "JOIN" | "KEY" | "LAST"
        | "LEFT" | "LIKE" | "LIMIT" | "NATURAL" | "NOT" | "NULLS" | "OFFSET" | "ON" | "OR"
        | "ORDER" | "OUTER" | "OVER" | "PARTITION" | "PRECEDING" | "PRIMARY" | "RANGE"
        | "RECURSIVE" | "REFERENCES" | "RIGHT" | "ROWS" | "SELECT" | "SET" | "TABLE" | "THEN"
        | "TIME" | "TIMESTAMP" | "TRUNCATE" | "UNBOUNDED" | "UNION" | "UNIQUE" | "UPDATE"
        | "USING" | "VALUES" | "VIEW" | "WHEN" | "WHERE" | "WINDOW" | "WITH" => Some("key"),
        _ => None,
    }
}

fn shell(line: &str, out: &mut Out) {
    let (body, eol) = split_eol(line);
    let trimmed = body.trim_start();
    out.plain(&body[..body.len() - trimmed.len()]);
    if trimmed.starts_with('#') {
        out.span("comment", trimmed);
        out.plain(eol);
        return;
    }
    let continuation = !trimmed.is_empty() && body.len() != trimmed.len();
    let mut first = !continuation;
    let mut rest = trimmed;
    while !rest.is_empty() {
        let space = rest.len() - rest.trim_start().len();
        out.plain(&rest[..space]);
        rest = &rest[space..];
        if rest.is_empty() {
            break;
        }
        let quote = rest.starts_with(['\'', '"']);
        let len = if quote {
            quoted_len(rest, rest.starts_with('"'))
        } else {
            rest.find(char::is_whitespace).unwrap_or(rest.len())
        };
        let word = &rest[..len];
        let class = if word == "\\" {
            "punc"
        } else if word.starts_with('#') {
            out.span("comment", rest);
            break;
        } else if first {
            first = false;
            "cmd"
        } else if quote {
            "str"
        } else if is_number(word) {
            "num"
        } else if word.starts_with('-') {
            "flag"
        } else if word.starts_with('$')
            || (word.len() > 2 && word.chars().all(|c| c.is_ascii_uppercase() || c == '_'))
        {
            "var"
        } else if word.starts_with("http://") || word.starts_with("https://") {
            "url"
        } else {
            "val"
        };
        out.span(class, word);
        rest = &rest[len..];
    }
    out.plain(eol);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_of(html: &str) -> String {
        let mut out = String::new();
        let mut in_tag = false;
        for c in html.chars() {
            match c {
                '<' => in_tag = true,
                '>' => in_tag = false,
                _ if !in_tag => out.push(c),
                _ => {}
            }
        }
        out.replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&amp;", "&")
    }

    const DEMO: &str = include_str!("../../../assets/landing/anchor/report.rmd");
    const FLAGSHIP: &str = include_str!("../../../examples/openai-vs-anthropic/report.rmd");

    #[test]
    fn text_round_trips_exactly() {
        for (source, language) in [
            (DEMO, HighlightLanguage::RichMarkdown),
            (FLAGSHIP, HighlightLanguage::RichMarkdown),
            (
                "a: 'x # y' # c\n  - { k: \"v\", n: 3 }\r\n",
                HighlightLanguage::Yaml,
            ),
            (
                "body: |\n  <b>&</b>\n  more\nnext: 1",
                HighlightLanguage::Yaml,
            ),
            (
                "richview publish r.rmd \\\n  --server https://x --format json\n",
                HighlightLanguage::Shell,
            ),
            ("my-report/\n  report.rmd\n", HighlightLanguage::Plain),
            ("```chart\ntype: bar\n", HighlightLanguage::RichMarkdown),
            (
                "Ünïcode *ok* `x` [a](b) \\* **é**\n",
                HighlightLanguage::RichMarkdown,
            ),
            ("", HighlightLanguage::RichMarkdown),
        ] {
            assert_eq!(text_of(&highlight_html(source, language)), source);
        }
    }

    #[test]
    fn rich_blocks_get_yaml_tokens() {
        // The opening of the home page's former iris example, so these tokens stay put when the
        // landing's example changes (S27-931).
        let source = "---\ntitle: Three iris species, 150 flowers\n---\n\n```chart\ntype: bar\ndata:\n  inline:\n    - { species: Setosa, petal: 1.46 }\n```\n\nEach species has **50 flowers**.\n";
        let html = highlight_html(source, HighlightLanguage::RichMarkdown);
        assert!(html.contains("<span class=\"hl-fm\">---</span>"));
        assert!(html.contains("<span class=\"hl-tag\">chart</span>"));
        assert!(html.contains("<span class=\"hl-key\">type</span><span class=\"hl-punc\">:</span> <span class=\"hl-val\">bar</span>"));
        assert!(html.contains("<span class=\"hl-key\">petal</span><span class=\"hl-punc\">:</span> <span class=\"hl-num\">1.46</span>"));
        assert!(html.contains("<span class=\"hl-strong\">**50 flowers**</span>"));
    }

    #[test]
    fn yaml_comments_block_scalars_and_shell() {
        let html = highlight_html(
            "# note\nbody: |\n  a: b\nx: 1 # c\n",
            HighlightLanguage::Yaml,
        );
        assert!(html.contains("<span class=\"hl-comment\"># note</span>"));
        assert!(html.contains("<span class=\"hl-str\">a: b</span>"));
        assert!(html.contains("<span class=\"hl-comment\"> # c</span>"));
        let html = highlight_html(
            "richview check r.rmd --format json\n",
            HighlightLanguage::Shell,
        );
        assert!(html.contains("<span class=\"hl-cmd\">richview</span>"));
        assert!(html.contains("<span class=\"hl-flag\">--format</span>"));
    }

    #[test]
    fn sql_highlights_literals_without_changing_source() {
        let source = "SeLeCt .25, 4.7e-3, 'it''s <ok>', TRUE, null, \"FROM\", café42 -- false\r\n/* outer\n/* nested */ WHERE */ FROM café;";
        let html = highlight_html(source, HighlightLanguage::from_name("SQL"));
        for (class, text) in [
            ("key", "SeLeCt"),
            ("num", ".25"),
            ("num", "4.7e-3"),
            ("str", "'it''s &lt;ok&gt;'"),
            ("bool", "TRUE"),
            ("null", "null"),
            ("comment", "-- false"),
            ("comment", "/* outer\n/* nested */ WHERE */"),
            ("key", "FROM"),
        ] {
            assert!(
                html.contains(&format!("<span class=\"hl-{class}\">{text}</span>")),
                "{html}"
            );
        }
        assert_eq!(html.matches("class=\"hl-key\"").count(), 2, "{html}");
        assert_eq!(text_of(&html), source);
    }

    #[test]
    fn yaml_and_rich_markdown_distinguish_values_and_keys() {
        let yaml = r#"settings: {"date": 2026-11-23, enabled: True, empty: null, count: -3, label: "a\" # b"} # comment"#;
        let rmd = format!("---\n{yaml}\n---\n\n```expand\n{yaml}\n```\n");
        for (source, language, occurrences) in [
            (yaml, HighlightLanguage::Yaml, 1),
            (&rmd, HighlightLanguage::RichMarkdown, 2),
        ] {
            let html = highlight_html(source, language);
            for (class, text) in [
                ("key", "&quot;date&quot;"),
                ("date", "2026-11-23"),
                ("bool", "True"),
                ("null", "null"),
                ("num", "-3"),
                ("str", r#"&quot;a\&quot; # b&quot;"#),
                ("comment", " # comment"),
            ] {
                let token = format!("<span class=\"hl-{class}\">{text}</span>");
                assert_eq!(html.matches(&token).count(), occurrences, "{html}");
            }
            assert_eq!(text_of(&html), source);
        }
    }

    #[test]
    fn shell_highlights_quoted_arguments_and_numbers() {
        let source = "printf '%s %d' -12 # note\n";
        let html = highlight_html(source, HighlightLanguage::Shell);
        assert!(html.contains("<span class=\"hl-str\">'%s %d'</span>"));
        assert!(html.contains("<span class=\"hl-num\">-12</span>"));
        assert_eq!(text_of(&html), source);
    }
}
