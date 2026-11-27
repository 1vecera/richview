//! The RichView YAML profile (TYP-02): every frontmatter, rich fence and style file is read by
//! this one loader, the YAML 1.2 core schema on `yaml-rust2` with RichView's limits.
//!
//! - A duplicate key is `yaml.duplicate-key` at the second key; anchors, aliases, merge keys,
//!   explicit tags and a second document are `yaml.unsupported`.
//! - Numbers are finite: `.inf`, `.nan` and overflowing literals are `yaml.unsupported`.
//! - Plain scalars resolve by the core schema: `null`, `~` and empty are null; `true` and `false`
//!   in their three spellings are booleans; decimal, `0x` and `0o` integers and decimal floats
//!   are numbers. Everything else is a string, so `yes`, `on`, unquoted dates, `1_000` and `0b11`
//!   stay text. An integer with a leading zero (`007`) also stays text, as it always has.
//! - Quoted and block scalars are always strings.
//!
//! The loader also records where each field starts, by the diagnostic path grammar
//! (`encoding.y`, `data.inline[2].lo`), so diagnostics can point at a field's own line (TYP-56).

use serde_json::{Map, Number, Value};
use std::collections::BTreeMap;
use yaml_rust2::parser::{Event, Parser};
use yaml_rust2::scanner::{Marker, TScalarStyle};

/// A 1-based line and column in the YAML text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pos {
    pub line: usize,
    pub column: usize,
}

impl Pos {
    fn of(mark: Marker) -> Pos {
        Pos {
            line: mark.line(),
            column: mark.col() + 1,
        }
    }
}

/// Why the text is not RichView YAML. `code` is `yaml.syntax` for text that is not YAML at all,
/// which each caller reports in its own family (`frontmatter.yaml`, `block.yaml`, `style.yaml`),
/// or `yaml.duplicate-key` or `yaml.unsupported` for YAML outside the profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YamlError {
    pub code: &'static str,
    pub message: String,
    pub pos: Pos,
}

impl std::fmt::Display for YamlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} (line {}, column {})",
            self.message, self.pos.line, self.pos.column
        )
    }
}

/// A loaded document: its value and where each field starts. Mapping keys are recorded at the
/// key, sequence items at the item; the root is `""`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Loaded {
    pub value: Value,
    pub positions: BTreeMap<String, Pos>,
}

/// Load `text` under the profile. An empty document is null.
pub(crate) fn load(text: &str) -> Result<Loaded, YamlError> {
    Loader::default().run(text)
}

/// Load `text` under the profile and keep only the value.
pub fn from_str(text: &str) -> Result<Value, YamlError> {
    load(text).map(|l| l.value)
}

/// How deep lists and mappings may nest. RichView sources need a handful of levels; the cap
/// keeps hostile input from growing the stack of values that later code walks recursively.
const MAX_DEPTH: usize = 64;

/// How many fields a document may have, counting every mapping key and every list item. Loading
/// records where each of them starts, under its whole path, so the cost of a document is not its
/// size: a list of a million items under keys that are ten kilobytes each keeps a million paths of
/// hundreds of kilobytes. Nothing a report says needs this many fields; a table that large is a
/// data file.
const MAX_FIELDS: usize = 250_000;

/// How many bytes of field paths loading may build in all, those it keeps and those of the
/// containers it is inside of. The count is made before each path is built, so a document that is
/// over it is refused having allocated at most this much, and takes a moment to be refused.
const MAX_PATH_BYTES: usize = 16 * 1024 * 1024;

enum Frame {
    Seq {
        path: String,
        items: Vec<Value>,
    },
    Map {
        path: String,
        entries: Map<String, Value>,
        key: Option<String>,
    },
}

#[derive(Default)]
struct Loader {
    stack: Vec<Frame>,
    root: Option<Value>,
    positions: BTreeMap<String, Pos>,
    documents: usize,
    /// The bytes of field paths built so far (`MAX_PATH_BYTES`).
    path_bytes: usize,
}

fn too_many_fields(mark: Marker) -> YamlError {
    YamlError {
        code: "yaml.unsupported",
        message: format!(
            "This YAML has more fields than RichView reads from one document (at most {MAX_FIELDS} mapping keys and list items, and {} MiB of field names); put a large table in a CSV file",
            MAX_PATH_BYTES >> 20
        ),
        pos: Pos::of(mark),
    }
}

fn unsupported(what: &str, mark: Marker) -> YamlError {
    YamlError {
        code: "yaml.unsupported",
        message: format!("{what} are not supported in RichView YAML; write the value out in full"),
        pos: Pos::of(mark),
    }
}

impl Loader {
    fn run(mut self, text: &str) -> Result<Loaded, YamlError> {
        let mut parser = Parser::new_from_str(text);
        loop {
            let (event, mark) = parser.next_token().map_err(|e| YamlError {
                code: "yaml.syntax",
                message: e.info().to_owned(),
                pos: Pos::of(*e.marker()),
            })?;
            match event {
                Event::StreamEnd => break,
                Event::DocumentStart => {
                    self.documents += 1;
                    if self.documents > 1 {
                        return Err(YamlError {
                            code: "yaml.unsupported",
                            message: "Only one YAML document is allowed here".into(),
                            pos: Pos::of(mark),
                        });
                    }
                }
                Event::Alias(_) => return Err(unsupported("Aliases", mark)),
                Event::Scalar(text, style, anchor, tag) => {
                    if anchor > 0 {
                        return Err(unsupported("Anchors", mark));
                    }
                    if tag.is_some() {
                        return Err(unsupported("Explicit tags", mark));
                    }
                    if self.expects_key() {
                        if style == TScalarStyle::Plain && text == "<<" {
                            return Err(unsupported("Merge keys", mark));
                        }
                        self.key(text, mark)?;
                    } else {
                        let value = scalar(&text, style).map_err(|message| YamlError {
                            code: "yaml.unsupported",
                            message: message.into(),
                            pos: Pos::of(mark),
                        })?;
                        self.open_item(mark)?;
                        self.push(value);
                    }
                }
                Event::SequenceStart(anchor, ref tag) | Event::MappingStart(anchor, ref tag) => {
                    if anchor > 0 {
                        return Err(unsupported("Anchors", mark));
                    }
                    if tag.is_some() {
                        return Err(unsupported("Explicit tags", mark));
                    }
                    if self.expects_key() {
                        return Err(unsupported("Keys that are lists or mappings", mark));
                    }
                    if self.stack.len() >= MAX_DEPTH {
                        return Err(YamlError {
                            code: "yaml.unsupported",
                            message: format!(
                                "Lists and mappings nest at most {MAX_DEPTH} levels deep in RichView YAML"
                            ),
                            pos: Pos::of(mark),
                        });
                    }
                    self.open_item(mark)?;
                    // The container keeps its own copy of its path while it is open.
                    self.charge(self.child_path_len(), mark)?;
                    let path = self.child_path();
                    let sequence = matches!(event, Event::SequenceStart(..));
                    self.stack.push(if sequence {
                        Frame::Seq {
                            path,
                            items: Vec::new(),
                        }
                    } else {
                        Frame::Map {
                            path,
                            entries: Map::new(),
                            key: None,
                        }
                    });
                }
                Event::SequenceEnd => {
                    if let Some(Frame::Seq { items, .. }) = self.stack.pop() {
                        self.push(Value::Array(items));
                    }
                }
                Event::MappingEnd => {
                    if let Some(Frame::Map { entries, .. }) = self.stack.pop() {
                        self.push(Value::Object(entries));
                    }
                }
                Event::Nothing | Event::StreamStart | Event::DocumentEnd => {}
            }
        }
        Ok(Loaded {
            value: self.root.unwrap_or(Value::Null),
            positions: self.positions,
        })
    }

    fn expects_key(&self) -> bool {
        matches!(self.stack.last(), Some(Frame::Map { key: None, .. }))
    }

    fn key(&mut self, name: String, mark: Marker) -> Result<(), YamlError> {
        let Some(Frame::Map { path, .. }) = self.stack.last() else {
            return Ok(());
        };
        let cost = join_len(path, &name);
        if self.positions.len() >= MAX_FIELDS {
            return Err(too_many_fields(mark));
        }
        self.charge(cost, mark)?;
        let Some(Frame::Map { path, entries, key }) = self.stack.last_mut() else {
            return Ok(());
        };
        if entries.contains_key(&name) {
            return Err(YamlError {
                code: "yaml.duplicate-key",
                message: format!("`{name}` appears twice in one mapping; keep one"),
                pos: Pos::of(mark),
            });
        }
        let here = Pos::of(mark);
        // A block mapping's start event comes after its first key; the mapping starts there.
        if entries.is_empty()
            && let Some(start) = self.positions.get_mut(path.as_str())
            && (here.line, here.column) < (start.line, start.column)
        {
            *start = here;
        }
        let at = join(path, &name);
        self.positions.insert(at, here);
        *key = Some(name);
        Ok(())
    }

    /// The path the next value takes in its parent.
    fn child_path(&self) -> String {
        match self.stack.last() {
            None => String::new(),
            Some(Frame::Seq { path, items }) => format!("{path}[{}]", items.len()),
            Some(Frame::Map { path, key, .. }) => join(path, key.as_deref().unwrap_or("")),
        }
    }

    /// How long `child_path` is, without building it.
    fn child_path_len(&self) -> usize {
        match self.stack.last() {
            None => 0,
            Some(Frame::Seq { path, items }) => {
                path.len() + 2 + items.len().checked_ilog10().map_or(1, |d| d as usize + 1)
            }
            Some(Frame::Map { path, key, .. }) => join_len(path, key.as_deref().unwrap_or("")),
        }
    }

    /// Counts `bytes` of a field path that is about to be built, and refuses the document that
    /// would build more than `MAX_PATH_BYTES` of them.
    fn charge(&mut self, bytes: usize, mark: Marker) -> Result<(), YamlError> {
        self.path_bytes = self.path_bytes.saturating_add(bytes);
        if self.path_bytes > MAX_PATH_BYTES {
            return Err(too_many_fields(mark));
        }
        Ok(())
    }

    /// A sequence item starts here; mapping values are positioned by their key.
    fn open_item(&mut self, mark: Marker) -> Result<(), YamlError> {
        match self.stack.last() {
            Some(Frame::Seq { .. }) => {
                if self.positions.len() >= MAX_FIELDS {
                    return Err(too_many_fields(mark));
                }
                self.charge(self.child_path_len(), mark)?;
                let at = self.child_path();
                self.positions.insert(at, Pos::of(mark));
            }
            None => {
                self.positions.insert(String::new(), Pos::of(mark));
            }
            Some(Frame::Map { .. }) => {}
        }
        Ok(())
    }

    fn push(&mut self, value: Value) {
        match self.stack.last_mut() {
            None => self.root = Some(value),
            Some(Frame::Seq { items, .. }) => items.push(value),
            Some(Frame::Map { entries, key, .. }) => {
                if let Some(k) = key.take() {
                    entries.insert(k, value);
                }
            }
        }
    }
}

/// The keys of the rows at `path` in first-seen order: the first row's keys, then each later
/// row's new keys (TYP-25). `path` leads through mappings from the top-level mapping to a
/// sequence of row mappings. Values are skipped without being built; text that is not rows of
/// mappings there gives no order, and callers fall back to their own.
pub(crate) fn key_order(text: &str, path: &[&str]) -> Vec<String> {
    let mut parser = Parser::new_from_str(text);
    let mut next = move || parser.next_token().ok().map(|(event, _)| event);
    rows_at(&mut next, path).unwrap_or_default()
}

fn rows_at(next: &mut impl FnMut() -> Option<Event>, path: &[&str]) -> Option<Vec<String>> {
    let mut event = next()?;
    while matches!(
        event,
        Event::StreamStart | Event::DocumentStart | Event::Nothing
    ) {
        event = next()?;
    }
    for step in path {
        if !matches!(event, Event::MappingStart(..)) {
            return None;
        }
        loop {
            match next()? {
                Event::Scalar(key, ..) if key == *step => break,
                Event::Scalar(..) => skip(next)?,
                _ => return None,
            }
        }
        event = next()?;
    }
    if !matches!(event, Event::SequenceStart(..)) {
        return None;
    }
    let mut keys: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    loop {
        match next()? {
            Event::SequenceEnd => return Some(keys),
            Event::MappingStart(..) => loop {
                match next()? {
                    Event::MappingEnd => break,
                    Event::Scalar(key, ..) => {
                        if seen.insert(key.clone()) {
                            keys.push(key);
                        }
                        skip(next)?;
                    }
                    _ => return None,
                }
            },
            _ => return None,
        }
    }
}

/// Skip one node, however deeply nested.
fn skip(next: &mut impl FnMut() -> Option<Event>) -> Option<()> {
    let mut depth = 0usize;
    loop {
        match next()? {
            Event::SequenceStart(..) | Event::MappingStart(..) => depth += 1,
            Event::SequenceEnd | Event::MappingEnd => depth = depth.checked_sub(1)?,
            Event::Scalar(..) | Event::Alias(_) => {}
            _ => return None,
        }
        if depth == 0 {
            return Some(());
        }
    }
}

/// How long `join(path, key)` is.
fn join_len(path: &str, key: &str) -> usize {
    if path.is_empty() {
        key.len()
    } else {
        path.len() + 1 + key.len()
    }
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_owned()
    } else {
        format!("{path}.{key}")
    }
}

/// Resolve one scalar by the profile.
fn scalar(text: &str, style: TScalarStyle) -> Result<Value, &'static str> {
    if style != TScalarStyle::Plain {
        return Ok(Value::String(text.to_owned()));
    }
    match text {
        "" | "~" | "null" | "Null" | "NULL" => return Ok(Value::Null),
        "true" | "True" | "TRUE" => return Ok(Value::Bool(true)),
        "false" | "False" | "FALSE" => return Ok(Value::Bool(false)),
        _ => {}
    }
    let unsigned = text.trim_start_matches(['-', '+']);
    if matches!(
        unsigned,
        ".inf" | ".Inf" | ".INF" | ".nan" | ".NaN" | ".NAN"
    ) {
        return Err("Numbers must be finite: write a finite number, or quote the text");
    }
    if let Some(hex) = text.strip_prefix("0x") {
        if !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return radix(hex, 16);
        }
        return Ok(Value::String(text.to_owned()));
    }
    if let Some(octal) = text.strip_prefix("0o") {
        if !octal.is_empty() && octal.bytes().all(|b| (b'0'..=b'7').contains(&b)) {
            return radix(octal, 8);
        }
        return Ok(Value::String(text.to_owned()));
    }
    if !decimal(text) {
        return Ok(Value::String(text.to_owned()));
    }
    // An integer part with a leading zero (`007`, `00.5`) stays text, as it always has.
    let whole = unsigned.split(['.', 'e', 'E']).next().unwrap_or("");
    if whole.len() > 1 && whole.starts_with('0') {
        return Ok(Value::String(text.to_owned()));
    }
    let integer = !text.contains(['.', 'e', 'E']);
    if integer {
        if let Ok(n) = text.parse::<i64>() {
            return Ok(Value::Number(n.into()));
        }
        if let Ok(n) = text.trim_start_matches('+').parse::<u64>() {
            return Ok(Value::Number(n.into()));
        }
        // A float would keep only its leading digits.
        return Err("This number is too large; quote it to keep it as text");
    }
    let n: f64 = text
        .parse()
        .map_err(|_| "Numbers must be finite: write a finite number, or quote the text")?;
    Number::from_f64(n)
        .map(Value::Number)
        .ok_or("Numbers must be finite: write a finite number, or quote the text")
}

fn radix(digits: &str, base: u32) -> Result<Value, &'static str> {
    match u64::from_str_radix(digits, base) {
        Ok(n) => Ok(Value::Number(n.into())),
        Err(_) => Err("This number is too large; quote it to keep it as text"),
    }
}

/// The YAML 1.2 core schema's decimal integer and float forms:
/// `[-+]?(\.[0-9]+|[0-9]+(\.[0-9]*)?)([eE][-+]?[0-9]+)?`.
fn decimal(text: &str) -> bool {
    let b = text.as_bytes();
    let mut i = 0;
    if matches!(b.first(), Some(b'-' | b'+')) {
        i += 1;
    }
    let digits = |i: &mut usize| {
        let start = *i;
        while *i < b.len() && b[*i].is_ascii_digit() {
            *i += 1;
        }
        *i - start
    };
    let whole = digits(&mut i);
    let mut fraction = 0;
    let mut dot = false;
    if i < b.len() && b[i] == b'.' {
        dot = true;
        i += 1;
        fraction = digits(&mut i);
    }
    if whole == 0 && (!dot || fraction == 0) {
        return false;
    }
    if i < b.len() && matches!(b[i], b'e' | b'E') {
        i += 1;
        if i < b.len() && matches!(b[i], b'-' | b'+') {
            i += 1;
        }
        if digits(&mut i) == 0 {
            return false;
        }
    }
    i == b.len()
}

/// A YAML node to write, with mappings in the order given. `From<&Value>` keeps a JSON
/// object's key order.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Scalar(Value),
    Map(Vec<(String, Node)>),
    Seq(Vec<Node>),
}

impl From<&Value> for Node {
    fn from(value: &Value) -> Node {
        match value {
            Value::Object(map) => {
                Node::Map(map.iter().map(|(k, v)| (k.clone(), v.into())).collect())
            }
            Value::Array(items) => Node::Seq(items.iter().map(Node::from).collect()),
            other => Node::Scalar(other.clone()),
        }
    }
}

/// Write `node` as block YAML that this profile reads back to the same value. A string is
/// plain only when it reads back as the same string; otherwise it is double-quoted.
pub fn to_string(node: &Node) -> String {
    let mut out = String::new();
    match node {
        Node::Map(entries) if !entries.is_empty() => write_map(entries, 0, &mut out),
        Node::Seq(items) if !items.is_empty() => write_seq(items, 0, &mut out),
        other => {
            out.push_str(&inline(other));
            out.push('\n');
        }
    }
    out
}

fn write_map(entries: &[(String, Node)], indent: usize, out: &mut String) {
    for (i, (key, value)) in entries.iter().enumerate() {
        if i > 0 {
            out.push_str(&" ".repeat(indent));
        }
        out.push_str(&string(key));
        out.push(':');
        write_value(value, indent + 2, out);
    }
}

fn write_seq(items: &[Node], indent: usize, out: &mut String) {
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(&" ".repeat(indent));
        }
        out.push_str("- ");
        match item {
            Node::Map(entries) if !entries.is_empty() => write_map(entries, indent + 2, out),
            Node::Seq(inner) if !inner.is_empty() => write_seq(inner, indent + 2, out),
            other => {
                out.push_str(&inline(other));
                out.push('\n');
            }
        }
    }
}

/// A mapping value after its key's colon: nested blocks on the next lines, scalars inline.
fn write_value(value: &Node, indent: usize, out: &mut String) {
    match value {
        Node::Map(entries) if !entries.is_empty() => {
            out.push('\n');
            out.push_str(&" ".repeat(indent));
            write_map(entries, indent, out);
        }
        Node::Seq(items) if !items.is_empty() => {
            out.push('\n');
            out.push_str(&" ".repeat(indent));
            write_seq(items, indent, out);
        }
        other => {
            out.push(' ');
            out.push_str(&inline(other));
            out.push('\n');
        }
    }
}

fn inline(node: &Node) -> String {
    match node {
        Node::Map(_) => "{}".into(),
        Node::Seq(_) => "[]".into(),
        Node::Scalar(Value::String(text)) => string(text),
        Node::Scalar(other) => other.to_string(),
    }
}

/// Text other YAML readers take for a boolean, null, number or date (`yes`, `off`, `007`,
/// `2026-10-27`, `1_000`) is quoted even though the profile reads it as text.
fn looks_typed(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "y" | "n" | "yes" | "no" | "on" | "off" | "true" | "false" | "null" | "~"
    ) || text
        .trim_start_matches(['-', '+', '.'])
        .starts_with(|c: char| c.is_ascii_digit())
}

/// A string scalar: plain when that reads back as the same string, else double-quoted with
/// JSON escapes, which are valid YAML escapes.
fn string(text: &str) -> String {
    let plain = !text.is_empty()
        && text.trim() == text
        && !text.contains(['\n', '\r', '\t', '"', '\'', '\\', '`'])
        && !text.contains(": ")
        && !text.contains(" #")
        && !text.ends_with(':')
        && !text.starts_with('#')
        && !text.starts_with("...")
        && !text.starts_with([
            '-', '?', ',', '[', ']', '{', '}', '&', '*', '!', '|', '>', '%', '@', '<', '=',
        ])
        && !text.chars().any(char::is_control)
        && !looks_typed(text)
        && matches!(scalar(text, TScalarStyle::Plain), Ok(Value::String(ref s)) if s == text);
    if plain {
        text.to_owned()
    } else {
        Value::String(text.to_owned()).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn value(text: &str) -> Value {
        from_str(text).unwrap_or_else(|e| panic!("{text}: {e}"))
    }

    fn error(text: &str) -> YamlError {
        from_str(text).expect_err(text)
    }

    #[test]
    fn key_order_reads_rows_in_first_seen_order_without_building_them() {
        let yaml = "type: bar\ndata:\n  note: {skip: [1, {deep: 2}]}\n  inline:\n    - {zeta: a, alpha: {x: [1]}}\n    - alpha: 2\n      mid: 3\n      zeta: b\n";
        assert_eq!(
            key_order(yaml, &["data", "inline"]),
            ["zeta", "alpha", "mid"]
        );
        let json = r#"{"type": "bar", "data": {"inline": [{"b": 1, "a": null}, {"c": 2}]}}"#;
        assert_eq!(key_order(json, &["data", "inline"]), ["b", "a", "c"]);
        for odd in [
            "data: {inline: [1, 2]}",
            "data: {rows: [{a: 1}]}",
            "data: {inline: {a: 1}}",
            "- {a: 1}",
            "data: [",
        ] {
            assert!(key_order(odd, &["data", "inline"]).is_empty(), "{odd}");
        }
    }

    #[test]
    fn plain_scalars_resolve_by_the_core_schema_with_the_profile_s_strings() {
        let v = value(
            "a: ~\nb: null\nc:\nd: true\ne: False\nf: 12\ng: -3\nh: +4\ni: 1.5\nj: 1e3\nk: .5\nl: 0x10\nm: 0o17\nn: 0\no: 0.25\n",
        );
        assert_eq!(
            v,
            json!({"a": null, "b": null, "c": null, "d": true, "e": false, "f": 12, "g": -3, "h": 4,
                "i": 1.5, "j": 1000.0, "k": 0.5, "l": 16, "m": 15, "n": 0, "o": 0.25})
        );
        // Text under the profile, whatever YAML 1.1 or another schema would make of it.
        let v = value(
            "a: yes\nb: no\nc: on\nd: off\ne: 2024-03-01\nf: 1_000\ng: 0b11\nh: 007\ni: -012\nj: 00.5\nk: 1,234\nl: 0x\nm: 0o9\n",
        );
        for (k, text) in [
            ("a", "yes"),
            ("b", "no"),
            ("c", "on"),
            ("d", "off"),
            ("e", "2024-03-01"),
            ("f", "1_000"),
            ("g", "0b11"),
            ("h", "007"),
            ("i", "-012"),
            ("j", "00.5"),
            ("k", "1,234"),
            ("l", "0x"),
            ("m", "0o9"),
        ] {
            assert_eq!(v[k], json!(text), "{k}");
        }
    }

    #[test]
    fn quoted_and_block_scalars_are_always_text() {
        let v = value("a: '12'\nb: \"true\"\nc: |\n  7\nd: >\n  null\ne: '.inf'\n");
        assert_eq!(
            v,
            json!({"a": "12", "b": "true", "c": "7\n", "d": "null\n", "e": ".inf"})
        );
    }

    #[test]
    fn large_integers_keep_their_value_and_floats_stay_finite() {
        assert_eq!(value("n: 18446744073709551615")["n"], json!(u64::MAX));
        assert_eq!(value("n: -9223372036854775808")["n"], json!(i64::MIN));
        assert_eq!(value("n: 1.5e300")["n"], json!(1.5e300));
        for text in [
            "n: 123456789012345678901234567890",
            "n: -9223372036854775809",
            "n: .inf",
            "n: -.Inf",
            "n: +.INF",
            "n: .nan",
            "n: .NaN",
            "n: 1e400",
            "n: 0xFFFFFFFFFFFFFFFFFF",
        ] {
            let e = error(text);
            assert_eq!(e.code, "yaml.unsupported", "{text}");
            assert_eq!(e.pos, Pos { line: 1, column: 4 }, "{text}");
        }
    }

    #[test]
    fn a_duplicate_key_is_an_error_at_the_second_key() {
        let e = error("title: A\nencoding:\n  x: a\n  y: b\n  x: c\n");
        assert_eq!(e.code, "yaml.duplicate-key");
        assert_eq!(e.pos, Pos { line: 5, column: 3 });
        assert!(e.message.contains("`x`"), "{}", e.message);
        // The same key in two mappings is fine.
        value("a: {x: 1}\nb: {x: 2}\n");
        // Spellings that resolve to one key collide.
        assert_eq!(error("a: 1\n'a': 2\n").code, "yaml.duplicate-key");
    }

    #[test]
    fn anchors_aliases_merge_keys_tags_and_extra_documents_are_unsupported() {
        for (text, line) in [
            ("rows: &r [1, 2]\nmore: *r\n", 1),
            ("a: 1\nb: *x\n", 2),
            ("base: {x: 1}\nchart:\n  <<: {y: 2}\n", 3),
            ("a: !!str 12\n", 1),
            ("a: !custom {b: 1}\n", 1),
            ("? [a, b]\n: 1\n", 1),
            ("a: 1\n---\nb: 2\n", 2),
        ] {
            let e = from_str(text).expect_err(text);
            // An alias to an unknown anchor fails in the parser itself; both refuse the text.
            assert!(
                matches!(e.code, "yaml.unsupported" | "yaml.syntax"),
                "{text}: {e:?}"
            );
            assert_eq!(e.pos.line, line, "{text}: {e:?}");
        }
        assert_eq!(error("rows: &r [1]\n").code, "yaml.unsupported");
        // A quoted `<<` is an ordinary key.
        assert_eq!(value("'<<': 1\n"), json!({"<<": 1}));
    }

    #[test]
    fn invalid_yaml_is_a_syntax_error_with_its_position() {
        let e = error("a: [1, 2\nb: 3\n");
        assert_eq!(e.code, "yaml.syntax");
        assert!(e.pos.line >= 1);
        let e = error("a: 1\n  b: 2\n");
        assert_eq!(e.code, "yaml.syntax");
        assert_eq!(e.pos.line, 2);
    }

    #[test]
    fn empty_documents_are_null_and_roots_may_be_any_value() {
        assert_eq!(value(""), Value::Null);
        assert_eq!(value("# only a comment\n"), Value::Null);
        assert_eq!(value("- 1\n- two\n"), json!([1, "two"]));
        assert_eq!(value("just text"), json!("just text"));
    }

    #[test]
    fn a_list_under_long_keys_is_refused_before_its_paths_are_built() {
        // Sixty keys of a thousand characters (the longest a simple key may be), each inside the
        // last, and a list at the bottom: every item is a field whose path is the sixty kilobytes
        // above it. A million of them are two megabytes of text and sixty gigabytes of paths.
        let mut text = String::new();
        for level in 0..60 {
            text.push_str(&" ".repeat(level));
            text.push_str(&"k".repeat(1_000));
            text.push_str(":\n");
        }
        text.push_str(&" ".repeat(60));
        text.push_str(&format!("[{}]\n", "1,".repeat(1_000_000)));
        assert!(text.len() < 5_000_000, "{}", text.len());
        let started = std::time::Instant::now();
        let error = load(&text).unwrap_err();
        assert_eq!(error.code, "yaml.unsupported");
        assert!(error.message.contains("more fields"), "{}", error.message);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_document_with_more_fields_than_a_report_has_is_refused_and_one_with_a_great_many_is_read()
    {
        let flow = |items: usize| format!("[{}]", vec!["1"; items].join(","));
        let many = load(&flow(MAX_FIELDS - 1)).unwrap();
        assert_eq!(many.value.as_array().unwrap().len(), MAX_FIELDS - 1);
        let error = load(&flow(MAX_FIELDS + 1)).unwrap_err();
        assert_eq!(error.code, "yaml.unsupported");
        // A table of twenty thousand rows of three fields, as inline data may be, is read.
        let rows: String = (0..20_000)
            .map(|n| format!("  - {{a: {n}, b: 2, c: x}}\n"))
            .collect();
        let table = load(&format!("data:\n  inline:\n{rows}")).unwrap();
        assert_eq!(
            table.value["data"]["inline"].as_array().unwrap().len(),
            20_000
        );
        assert!(table.positions.contains_key("data.inline[19999].c"));
        // Mapping keys count as fields as list items do.
        let keys: String = (0..=MAX_FIELDS).map(|n| format!("k{n}: 1\n")).collect();
        assert_eq!(load(&keys).unwrap_err().code, "yaml.unsupported");
    }

    #[test]
    fn positions_name_each_field_by_the_diagnostic_path_grammar() {
        let text = "type: bar\nencoding:\n  x: region\n  y: sales\ndata:\n  inline:\n    - {region: A, sales: 1}\n    - region: B\n      sales: 2\n";
        let l = load(text).unwrap();
        let at = |p: &str| l.positions.get(p).copied().unwrap_or_else(|| panic!("{p}"));
        assert_eq!(at("type"), Pos { line: 1, column: 1 });
        assert_eq!(at("encoding.y"), Pos { line: 4, column: 3 });
        assert_eq!(at("data.inline[0]"), Pos { line: 7, column: 7 });
        assert_eq!(
            at("data.inline[0].sales"),
            Pos {
                line: 7,
                column: 19
            }
        );
        assert_eq!(at("data.inline[1]"), Pos { line: 8, column: 7 });
        assert_eq!(at("data.inline[1].sales"), Pos { line: 9, column: 7 });
        // Columns count characters: a four-byte character before a field moves it by one.
        let l = load("t: \"🦀\"\nu: 1\nv: {a: 🦀, b: 2}\n").unwrap();
        assert_eq!(
            l.positions["v.b"],
            Pos {
                line: 3,
                column: 11
            }
        );
    }

    #[test]
    fn written_yaml_reads_back_to_the_same_value() {
        let v = json!({
            "title": "Q3: revenue # up",
            "plain": "Enterprise reached $420K",
            "words": ["yes", "on", "007", "0b11", "0x10", "0o17", "1e3", ".inf", ".nan", "1_000", "2026-10-27", "true", "null", "~", "", " padded ", "-dash", "a\nb", "tab\t", "quote\"s", "🦀"],
            "numbers": [0, -3, 1.5, 1e300, 18446744073709551615u64],
            "flags": [true, false, null],
            "empty": {"map": {}, "list": []},
            "rows": [{"region": "A", "sales": 1}, {"region": "B", "nested": [[1, 2], []]}],
            "<<": "merge-looking key",
            "key: colon": 1
        });
        let text = to_string(&Node::from(&v));
        assert_eq!(value(&text), v, "{text}");
        // Order is the caller's; plain text stays plain.
        let ordered = Node::Map(vec![
            ("src".into(), Node::Scalar(json!("photo.jpg"))),
            ("alt".into(), Node::Scalar(json!("A lake at dawn"))),
            ("width".into(), Node::Scalar(json!(1200))),
        ]);
        assert_eq!(
            to_string(&ordered),
            "src: photo.jpg\nalt: A lake at dawn\nwidth: 1200\n"
        );
        assert_eq!(to_string(&Node::Scalar(json!("yes"))), "\"yes\"\n");
        for marker in ["...", "---"] {
            let text = to_string(&Node::Scalar(json!(marker)));
            assert_eq!(value(&text), json!(marker), "{text}");
        }
    }

    #[test]
    fn nesting_stops_at_its_cap_before_hostile_depth_grows_the_value() {
        let at_cap = format!("{}{}", "[".repeat(MAX_DEPTH), "]".repeat(MAX_DEPTH));
        assert!(value(&at_cap).is_array());
        let deeper = format!("{}{}", "[".repeat(MAX_DEPTH + 1), "]".repeat(MAX_DEPTH + 1));
        let e = error(&deeper);
        assert_eq!(e.code, "yaml.unsupported");
        assert_eq!(
            e.pos,
            Pos {
                line: 1,
                column: MAX_DEPTH + 1
            }
        );
        let hostile = "{a: ".repeat(200_000);
        // yaml-rust2's scanner stops a flow this deep itself, before the loader sees it.
        let e = error(&hostile);
        assert!(
            matches!(e.code, "yaml.unsupported" | "yaml.syntax"),
            "{e:?}"
        );
    }
}
