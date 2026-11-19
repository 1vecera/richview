//! Checks a rich block or the frontmatter against the schema registry (ARC-09, TYP-10, TYP-11,
//! TYP-84). One walk applies the same rules everywhere:
//!
//! - an unknown key, at any depth and including a host-internal `_` key, is a warning that names
//!   the nearest known key. In a block it is dropped before anything renders it; in the
//!   frontmatter it stays in the metadata and has no effect;
//! - a known key whose value breaks its type or word list is `block.schema` (error);
//! - a required field that is missing is `block.schema` (error).
//!
//! A mapping field with fields of its own below it in the registry is closed: only those keys are
//! known. One without any, such as a row of `data.inline` or an annotation's `at`, holds the
//! author's own names and is not walked. Cross-field rules (one data source, the channels a chart
//! type reads, field references) stay with each tag's validator.

use crate::Diagnostic;
use crate::registry::{FRONTMATTER, Field, LAYOUTS, TAGS, Ty};
use serde_json::{Map, Value};

/// Where the walk reports: a block's tag (`chart`) or `frontmatter`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Root<'a> {
    Block(&'a str),
    Frontmatter,
}

impl Root<'_> {
    /// The registry scope of the root's retired names.
    fn scope(&self) -> &str {
        self.name()
    }

    fn name(&self) -> &str {
        match self {
            Root::Block(tag) => tag,
            Root::Frontmatter => "frontmatter",
        }
    }
}

/// Check `value` against the registry's fields for `root`, removing unknown block keys. Every
/// diagnostic's path starts with the root's name, so the parser can place it at its field's line.
pub(crate) fn check(root: Root, value: &mut Value) -> Vec<Diagnostic> {
    let fields: &[Field] = match root {
        Root::Block(tag) => match TAGS.iter().chain(LAYOUTS).find(|t| t.name == tag) {
            Some(tag) => tag.fields,
            None => return vec![],
        },
        Root::Frontmatter => FRONTMATTER,
    };
    let mut out = vec![];
    if let Value::Object(map) = value {
        // The keys the author set at the top level and one level below it (`style.ratio`): a
        // repair never moves a key onto one of them.
        let mut top: Vec<String> = map.keys().cloned().collect();
        for (key, value) in map.iter() {
            if let Value::Object(child) = value {
                top.extend(child.keys().map(|name| format!("{key}.{name}")));
            }
        }
        walk_at(root, fields, &top, "", "", map, &mut out);
    }
    out
}

/// The fields directly below `parent` (`""` for the top level), with `[n]` standing for any index.
fn children<'f>(fields: &'f [Field], parent: &str) -> impl Iterator<Item = (&'f str, &'f Field)> {
    fields.iter().filter_map(move |field| {
        let rest = if parent.is_empty() {
            field.path
        } else {
            field.path.strip_prefix(parent)?.strip_prefix('.')?
        };
        (!rest.contains('.') && !rest.contains('[')).then_some((rest, field))
    })
}

/// Whether `pattern` (a registry path) has fields of its own below it.
fn closed(fields: &[Field], pattern: &str) -> bool {
    let prefix = format!("{pattern}.");
    fields.iter().any(|field| field.path.starts_with(&prefix))
}

/// Walk `map`, the value at the concrete path `at` (`columns[2]`), whose registry path is
/// `pattern` (`columns[n]`). `top` holds the keys set at the root's top level.
fn walk_at(
    root: Root,
    fields: &[Field],
    top: &[String],
    pattern: &str,
    at: &str,
    map: &mut Map<String, Value>,
    out: &mut Vec<Diagnostic>,
) {
    let known: Vec<(&str, &Field)> = children(fields, pattern).collect();
    let join = |base: &str, key: &str| -> String {
        if base.is_empty() {
            key.to_owned()
        } else {
            format!("{base}.{key}")
        }
    };
    let mut unknown = vec![];
    let mut dropped = vec![];
    for (key, value) in map.iter_mut() {
        let Some((_, field)) = known.iter().find(|(name, _)| *name == key) else {
            unknown.push(key.clone());
            continue;
        };
        let path = join(at, key);
        let field_pattern = join(pattern, key);
        if !check_value(root, fields, top, field, &field_pattern, &path, value, out) {
            dropped.push(key.clone());
        }
    }
    for key in dropped {
        map.remove(&key);
    }
    for key in unknown {
        let path = join(at, &key);
        // A retired name fails with its own code and names what replaces it (TYP-83); it never
        // reaches the renderers.
        if let Some(entry) = crate::registry::retired(root.scope(), &indexless(&path)) {
            out.push(retired(root, entry, &path));
            map.remove(&key);
            continue;
        }
        let names: Vec<&str> = known.iter().map(|(name, _)| *name).collect();
        let place = belongs(root, pattern, &key, map, top);
        out.push(unknown_key(root, &key, &path, &names, place));
        if matches!(root, Root::Block(_)) {
            map.remove(&key);
        }
    }
    // The frontmatter's required fields are a publishing rule, not a parsing one (TYP-07).
    let required = matches!(root, Root::Block(_));
    for (name, field) in &known {
        // A required title is the caption heading and the accessible name, so blank text is as
        // good as none (TYP-13, TYP-30, TYP-31). An image's `alt` may be `""`: decorative.
        let blank = *name == "title"
            && map
                .get(*name)
                .and_then(Value::as_str)
                .is_some_and(|text| text.trim().is_empty());
        if required && field.required && (blank || !map.contains_key(*name)) {
            let path = join(at, name);
            out.push(
                schema(
                    root,
                    &path,
                    if blank {
                        format!("{} must not be empty", qualified(root, &path))
                    } else {
                        format!("{} is required", qualified(root, &path))
                    },
                    expected(&field.ty),
                )
                .fix(format!(
                    "Add `{}`: {}",
                    qualified(root, &path),
                    field.summary
                )),
            );
        }
    }
}

/// Check one known value, and walk into it when it is a closed mapping or a list of them.
/// Returns false when the field must be dropped: a frontmatter value of the wrong type.
#[allow(clippy::too_many_arguments)]
fn check_value(
    root: Root,
    fields: &[Field],
    top: &[String],
    field: &Field,
    pattern: &str,
    path: &str,
    value: &mut Value,
    out: &mut Vec<Diagnostic>,
) -> bool {
    // One string stands for a list of one, as `tags: draft` and `styles: sleek` read.
    if let (Ty::Either(Ty::Text, Ty::List(_)), Value::String(_)) = (&field.ty, &*value) {
        *value = Value::Array(vec![value.take()]);
    }
    // Inline rows have their own finding at the row, data.inline, which says what a row holds
    // (TYP-38), so the walk checks only that they are a list.
    let rows = pattern == "data.inline" && value.is_array();
    if !rows && !fits(&field.ty, value) {
        if root == Root::Frontmatter {
            let mut diagnostic = Diagnostic::new(
                "warning",
                "frontmatter.type",
                format!(
                    "{} must be {}, not {}; it is ignored",
                    qualified(root, path),
                    expected(&field.ty),
                    shown(value)
                ),
                qualified(root, path),
            )
            .expected(expected(&field.ty));
            // YAML reads an unquoted 1.0.0-like value as a number or a date.
            if matches!(path, "version" | "slug" | "date") {
                diagnostic = diagnostic.fix(format!(
                    "Quote the value so YAML reads it as text, for example `{path}: \"1.0.0\"`"
                ));
            }
            out.push(diagnostic);
            return false;
        }
        if let Some(old) = value
            .as_str()
            .and_then(|v| crate::registry::retired_value(root.scope(), &indexless(path), v))
        {
            out.push(
                Diagnostic::new(
                    "error",
                    "block.renamed",
                    format!(
                        "{} '{}' is retired. Write format: {} with unit: {}.",
                        qualified(root, path),
                        old.value,
                        old.replacement,
                        old.unit
                    ),
                    qualified(root, path),
                )
                .fix(format!(
                    "Use `format: {}` and `unit: {}`.",
                    old.replacement, old.unit
                )),
            );
            return true;
        }
        if field.ty == Ty::ChartType
            && let Some(name) = value.as_str()
        {
            let known: Vec<_> = crate::registry::CHART_TYPES
                .iter()
                .map(|c| c.name)
                .collect();
            let mut message = format!(
                "{} '{name}' is not a known chart type",
                qualified(root, path)
            );
            let fix = if let Some(near) = nearest(name, &known) {
                message.push_str(&format!("; did you mean '{near}'?"));
                format!("Set `{}` to `{near}`.", qualified(root, path))
            } else {
                format!(
                    "Set `{}` to a chart type from `richview learn chart-contract`.",
                    qualified(root, path)
                )
            };
            out.push(schema(root, path, message, expected(&field.ty)).fix(fix));
            return true;
        }
        out.push(schema(
            root,
            path,
            format!(
                "{} must be {}, not {}",
                qualified(root, path),
                expected(&field.ty),
                shown(value)
            ),
            expected(&field.ty),
        ));
        return true;
    }
    // The tabs mapping has its own value rules, which also reject it whole (`validate_tabs`);
    // the walk only names its unknown keys.
    if root == Root::Frontmatter && pattern == "tabs" {
        unknown_below(root, fields, top, pattern, path, value, out);
        return true;
    }
    match (&field.ty, value) {
        (_, Value::Object(map)) if closed(fields, pattern) => {
            walk_at(root, fields, top, pattern, path, map, out);
        }
        (Ty::List(_), Value::Array(items)) => {
            let item = format!("{pattern}[n]");
            if !closed(fields, &item) {
                return true;
            }
            for (index, value) in items.iter_mut().enumerate() {
                if let Value::Object(map) = value {
                    walk_at(
                        root,
                        fields,
                        top,
                        &item,
                        &format!("{path}[{index}]"),
                        map,
                        out,
                    );
                }
            }
        }
        _ => {}
    }
    true
}

/// Name the unknown keys at any depth below `value`, at the concrete path `at` whose registry
/// path is `pattern`, without checking the known values: their own validator does that.
fn unknown_below(
    root: Root,
    fields: &[Field],
    top: &[String],
    pattern: &str,
    at: &str,
    value: &Value,
    out: &mut Vec<Diagnostic>,
) {
    match value {
        Value::Object(map) if closed(fields, pattern) => {
            let known: Vec<&str> = children(fields, pattern).map(|(name, _)| name).collect();
            for (key, child) in map {
                let path = format!("{at}.{key}");
                if known.contains(&key.as_str()) {
                    let below = format!("{pattern}.{key}");
                    unknown_below(root, fields, top, &below, &path, child, out);
                } else if let Some(entry) =
                    crate::registry::retired(root.scope(), &indexless(&path))
                {
                    out.push(retired(root, entry, &path));
                } else {
                    let place = belongs(root, pattern, key, map, top);
                    out.push(unknown_key(root, key, &path, &known, place));
                }
            }
        }
        Value::Array(items) => {
            let item = format!("{pattern}[n]");
            for (index, child) in items.iter().enumerate() {
                unknown_below(
                    root,
                    fields,
                    top,
                    &item,
                    &format!("{at}[{index}]"),
                    child,
                    out,
                );
            }
        }
        _ => {}
    }
}

/// `columns[2].key` as the registry writes it: `columns[n].key`.
fn indexless(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
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

fn retired(root: Root, entry: &crate::registry::Retired, path: &str) -> Diagnostic {
    Diagnostic::new(
        "error",
        entry.code,
        format!("{} is retired. {}", qualified(root, path), entry.fix),
        qualified(root, path),
    )
    .fix(entry.fix)
}

/// Whether `value` has the shape `ty` asks for. Grammars finer than the shape (colours, ratios,
/// URLs) are checked here too; ranges and cross-field rules are the validators'.
fn fits(ty: &Ty, value: &Value) -> bool {
    match ty {
        Ty::Text | Ty::Markdown | Ty::Column | Ty::File => value.is_string(),
        Ty::Number => value.as_f64().is_some_and(f64::is_finite),
        Ty::Integer => value.is_i64() || value.is_u64(),
        Ty::Digits => value.as_u64().is_some_and(|n| (2..=6).contains(&n)),
        Ty::Bool => value.is_boolean(),
        Ty::OneOf(words) => value.as_str().is_some_and(|word| words.contains(&word)),
        Ty::List(item) => value
            .as_array()
            .is_some_and(|items| items.iter().all(|value| fits(item, value))),
        Ty::Map | Ty::Data => value.is_object(),
        Ty::Color => value.as_str().is_some_and(crate::style::valid_color),
        Ty::Url => value
            .as_str()
            .is_some_and(|url| url.starts_with("https://") || url.starts_with("http://")),
        Ty::Date => value.is_string(),
        Ty::Scalar => value.is_string() || value.is_number() || value.is_boolean(),
        Ty::NumberOrText => value.is_string() || value.as_f64().is_some_and(f64::is_finite),
        Ty::Ratio => richview_chart::block::parse_ratio(value).is_some(),
        Ty::ChartType => value
            .as_str()
            .is_some_and(|name| crate::registry::chart_type(name).is_some()),
        Ty::Any => true,
        Ty::Either(a, b) => fits(a, value) || fits(b, value),
    }
}

pub(crate) fn expected(ty: &Ty) -> String {
    match ty {
        Ty::Text | Ty::File | Ty::Date => "text".into(),
        Ty::Column => "a data column name".into(),
        Ty::Markdown => "inline Markdown text".into(),
        Ty::Number => "a number".into(),
        Ty::Integer => "a whole number".into(),
        Ty::Digits => "a whole number of significant digits from 2 to 6".into(),
        Ty::Bool => "true or false".into(),
        Ty::OneOf(words) => {
            let words: Vec<String> = words.iter().map(|w| format!("'{w}'")).collect();
            match words.as_slice() {
                [one] => one.clone(),
                [init @ .., last] => format!("one of {} or {last}", init.join(", ")),
                [] => "nothing".into(),
            }
        }
        Ty::List(item) => match item {
            Ty::Text | Ty::File | Ty::Date => "a list of text".into(),
            Ty::Column => "a list of data column names".into(),
            Ty::Map | Ty::Data => "a list of mappings".into(),
            Ty::Number => "a list of numbers".into(),
            Ty::Integer => "a list of whole numbers".into(),
            item => format!("a list whose entries are each {}", expected(item)),
        },
        Ty::Map | Ty::Data => "a mapping".into(),
        Ty::Color => "a colour (a CSS colour name, #rgb, #rrggbb, rgb(), hsl() or oklch())".into(),
        Ty::Url => "an http or https address".into(),
        Ty::Scalar => "text, a number, true or false".into(),
        Ty::NumberOrText => "a number or text".into(),
        Ty::Ratio => "a number or W:H from 0.5 to 6, such as 16:9".into(),
        Ty::ChartType => {
            let names: Vec<String> = crate::registry::CHART_TYPES
                .iter()
                .map(|chart| format!("'{}'", chart.name))
                .collect();
            match names.split_last() {
                Some((last, init)) => format!("one of {} or {last}", init.join(", ")),
                None => "a chart type".into(),
            }
        }
        Ty::Any => "any value".into(),
        Ty::Either(a, b) => format!("{} or {}", expected(a), expected(b)),
    }
}

fn shown(value: &Value) -> String {
    match value {
        Value::Null => "empty".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) if s.chars().count() <= 40 => format!("'{s}'"),
        Value::String(_) => "longer text".into(),
        Value::Array(_) => "a list".into(),
        Value::Object(_) => "a mapping".into(),
    }
}

/// `chart.encoding.y` or `frontmatter.title`, as the author reads it.
fn qualified(root: Root, path: &str) -> String {
    format!("{}.{path}", root.name())
}

fn schema(root: Root, path: &str, message: String, expected: String) -> Diagnostic {
    let fix = format!("Set `{}` to {expected}.", qualified(root, path));
    Diagnostic::new("error", "block.schema", message, qualified(root, path))
        .fix(fix)
        .expected(expected)
}

/// Words other chart and table grammars use for a key that a RichView mapping names differently,
/// by the mapping's registry path: an axis, column or control `title` is its `label`.
const SYNONYMS: &[(&str, &str, &str)] = &[
    ("axes.x", "title", "label"),
    ("axes.y", "title", "label"),
    ("axes.color", "title", "label"),
    ("columns[n]", "title", "label"),
    ("controls[n]", "title", "label"),
];

/// Where an unknown key belongs, when the registry knows, so that its repair is correct to copy
/// (TYP-55).
#[derive(Clone, Copy)]
enum Belongs {
    /// Nowhere the registry names: the nearest known key, if any, is the hint.
    Unknown,
    /// This mapping's own word for it (`axes.y.title` is `axes.y.label`), and whether the author
    /// set that word too.
    Here(&'static str, bool),
    /// The block's top level, which does not hold it yet (`data.max_rows` is `max_rows`). A key
    /// the top level already holds stays unknown: moving it there would overwrite that value.
    Top,
    /// The block's `style` mapping, the one field of that name, which `style` does not set yet
    /// (`ratio` is `style.ratio`, TYP-85).
    Style(&'static str),
}

fn belongs(
    root: Root,
    pattern: &str,
    key: &str,
    map: &Map<String, Value>,
    top: &[String],
) -> Belongs {
    if let Some((_, _, word)) = SYNONYMS
        .iter()
        .find(|(at, from, _)| *at == pattern && from.eq_ignore_ascii_case(key))
    {
        return Belongs::Here(word, map.contains_key(*word));
    }
    let fields = match root {
        Root::Block(_) => TAGS
            .iter()
            .chain(LAYOUTS)
            .find(|tag| tag.name == root.name())
            .map(|tag| tag.fields),
        Root::Frontmatter => None,
    };
    let top_field = !pattern.is_empty()
        && fields.is_some_and(|fields| fields.iter().any(|field| field.path == key));
    if top_field && !top.iter().any(|set| set == key) {
        return Belongs::Top;
    }
    // Styling words are distinctive, so a key that only `style` has belongs there; other
    // single homes (`encoding.source`, `binning.count`) are ordinary words an author may mean
    // differently, so they keep the nearest-key hint.
    let mut homes = fields.into_iter().flatten().filter(|field| {
        field
            .path
            .rsplit('.')
            .next()
            .map(|last| last.trim_end_matches("[n]"))
            == Some(key)
    });
    match (homes.next(), homes.next()) {
        (Some(home), None)
            if home.path.strip_prefix("style.") == Some(key)
                && !top.iter().any(|set| set == home.path) =>
        {
            Belongs::Style(home.path)
        }
        _ => Belongs::Unknown,
    }
}

fn unknown_key(root: Root, key: &str, path: &str, known: &[&str], belongs: Belongs) -> Diagnostic {
    let (code, effect) = match root {
        Root::Block(_) => ("block.unknown-key", "it is ignored and dropped"),
        Root::Frontmatter => ("frontmatter.unknown-key", "it has no effect"),
    };
    let here = qualified(root, path);
    let near = match belongs {
        Belongs::Unknown => nearest(key, known),
        Belongs::Here(word, false) => Some(word),
        Belongs::Here(_, true) | Belongs::Top | Belongs::Style(_) => None,
    };
    let (message, fix) = match (belongs, near) {
        (Belongs::Top, _) => (
            format!(
                "{here} belongs at {}, not inside this mapping; {effect}.",
                qualified(root, key)
            ),
            format!("Move `{here}` to `{}`.", qualified(root, key)),
        ),
        (Belongs::Style(home), _) => (
            format!("{here} belongs at {}; {effect}.", qualified(root, home)),
            format!("Move `{here}` to `{}`.", qualified(root, home)),
        ),
        (Belongs::Here(word, true), _) => {
            // `path` ends with `key`; the mapping's own word sits beside it.
            let beside = format!("{}{word}", &path[..path.len() - key.len()]);
            (
                format!(
                    "{here} is not a known key; {effect}. Its key here is '{word}', which is already set."
                ),
                format!(
                    "Remove `{here}`, or put its text in `{}`.",
                    qualified(root, &beside)
                ),
            )
        }
        (_, Some(near)) => (
            format!("{here} is not a known key; {effect}. Did you mean '{near}'?"),
            format!("Rename '{key}' to '{near}', or remove it."),
        ),
        (_, None) => (
            format!("{here} is not a known key; {effect}."),
            format!("Remove '{key}'; `richview learn` lists the keys this block reads."),
        ),
    };
    let mut d = Diagnostic::new("warning", code, message, here).fix(fix);
    if !known.is_empty() {
        d = d.expected(known.join(", "));
    }
    d
}

/// The known key closest to `key` by edit distance, ignoring case and `-`/`_`, if at most two
/// edits away.
pub(crate) fn nearest<'k>(key: &str, known: &[&'k str]) -> Option<&'k str> {
    let norm = |s: &str| s.to_lowercase().replace(['-', '_'], "");
    let key = norm(key);
    known
        .iter()
        .map(|name| (distance(&key, &norm(name)), *name))
        .filter(|(d, _)| *d <= 2)
        .min_by_key(|(d, _)| *d)
        .map(|(_, name)| name)
}

fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut last = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let next = (row[j + 1] + 1)
                .min(row[j] + 1)
                .min(last + usize::from(ca != *cb));
            last = row[j + 1];
            row[j + 1] = next;
        }
    }
    row[b.len()]
}

/// The columns a block's resolved data has: the keys of its inline rows, in first-seen order,
/// or the recorded column order. Empty when the data is not known here.
fn data_columns(value: &Value) -> Vec<String> {
    let data = value.get("data");
    let mut columns: Vec<String> = data
        .and_then(|d| d.get("columns"))
        .and_then(Value::as_array)
        .map(|c| {
            c.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    for row in data
        .and_then(|d| d.get("inline"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        for key in row.as_object().into_iter().flat_map(Map::keys) {
            if !columns.contains(key) {
                columns.push(key.clone());
            }
        }
    }
    columns
}

/// Field references that name a column the resolved data lacks (TYP-15): chart channels,
/// `highlight.by`, control fields and annotation anchors, and a table's columns, sort key and
/// total columns. Anchor keys may also be the axis aliases `x` and `y`, a pie's `name` and a
/// histogram's `value`. Nothing is checked while the data has no columns.
pub(crate) fn field_issues(tag: &str, value: &Value, line: usize) -> Vec<Diagnostic> {
    let columns = data_columns(value);
    if columns.is_empty() {
        return vec![];
    }
    let known: Vec<&str> = columns.iter().map(String::as_str).collect();
    let mut issues = vec![];
    let mut check = |path: String, name: &str, allowed: &[&str]| {
        if known.contains(&name) || allowed.contains(&name) {
            return;
        }
        let shown = known.iter().take(8).copied().collect::<Vec<_>>().join(", ");
        let more = if known.len() > 8 { ", ..." } else { "" };
        let mut message = format!("{tag}.{path} names '{name}', which is not a column of the data");
        let near = nearest(name, &known);
        if let Some(near) = near {
            message.push_str(&format!("; did you mean '{near}'?"));
        }
        message.push_str(&format!(
            "{}Columns: {shown}{more}.",
            if near.is_some() { " " } else { ". " }
        ));
        let target = format!("{tag}.{path}");
        let fix = if let Some(near) = near {
            format!("Set `{target}` to `{near}`, or add a `{name}` column to the data.")
        } else {
            format!(
                "Set `{target}` to an existing column ({shown}{more}), or add a `{name}` column to the data."
            )
        };
        issues.push(
            Diagnostic::new("error", "block.field", message, target)
                .fix(fix)
                .at(line, tag),
        );
    };
    match tag {
        "chart" => {
            if let Some(encoding) = value.get("encoding").and_then(Value::as_object) {
                for (channel, name) in encoding {
                    if let Some(name) = name.as_str() {
                        check(format!("encoding.{channel}"), name, &[]);
                    }
                }
            }
            if let Some(by) = value.pointer("/highlight/by").and_then(Value::as_str) {
                check("highlight.by".into(), by, &[]);
            }
            for (index, control) in value
                .get("controls")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .enumerate()
            {
                if let Some(field) = control.get("field").and_then(Value::as_str) {
                    check(format!("controls[{index}].field"), field, &[]);
                }
            }
            for (index, annotation) in value
                .get("annotations")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .enumerate()
            {
                for anchor in ["at", "from", "to"] {
                    for key in annotation
                        .get(anchor)
                        .and_then(Value::as_object)
                        .into_iter()
                        .flat_map(Map::keys)
                    {
                        check(
                            format!("annotations[{index}].{anchor}.{key}"),
                            key,
                            &["x", "y", "name", "value"],
                        );
                    }
                }
            }
        }
        "table" => {
            for (index, column) in value
                .get("columns")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .enumerate()
            {
                if let Some(key) = column.get("key").and_then(Value::as_str) {
                    check(format!("columns[{index}].key"), key, &[]);
                }
            }
            if let Some(key) = value.pointer("/sort/key").and_then(Value::as_str) {
                check("sort.key".into(), key, &[]);
            }
            for (index, name) in value
                .get("total")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .enumerate()
            {
                if let Some(name) = name.as_str() {
                    check(format!("total[{index}]"), name, &[]);
                }
            }
        }
        _ => {}
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn codes(diagnostics: &[Diagnostic]) -> Vec<(&str, &str)> {
        diagnostics
            .iter()
            .map(|d| (d.code.as_str(), d.path.as_str()))
            .collect()
    }

    #[test]
    fn unknown_keys_at_any_depth_warn_name_the_nearest_key_and_are_dropped() {
        let mut chart = json!({
            "type": "bar", "title": "T", "titel": "Revenue", "data": {"inline": [{"m": "Jan", "v": 1}]},
            "encoding": {"x": "m", "y": "v", "colour": "m"},
            "axes": {"y": {"formt": "percent"}},
            "_renderHeight": 900,
            "annotations": [{"kind": "point", "at": {"m": "Jan"}, "text": "Peak", "colr": "red"}],
        });
        let found = check(Root::Block("chart"), &mut chart);
        let mut paths: Vec<_> = codes(&found);
        paths.sort_unstable();
        assert_eq!(
            paths,
            [
                ("block.unknown-key", "chart._renderHeight"),
                ("block.unknown-key", "chart.annotations[0].colr"),
                ("block.unknown-key", "chart.axes.y.formt"),
                ("block.unknown-key", "chart.encoding.colour"),
                ("block.unknown-key", "chart.titel"),
            ]
        );
        let message = |path: &str| {
            found
                .iter()
                .find(|d| d.path == path)
                .map(|d| d.message.clone())
                .unwrap()
        };
        assert!(message("chart.titel").contains("Did you mean 'title'?"));
        assert!(message("chart.encoding.colour").contains("'color'"));
        assert!(message("chart.axes.y.formt").contains("'format'"));
        assert!(!message("chart._renderHeight").contains("Did you mean"));
        assert!(found.iter().all(|d| d.level == "warning"));
        // Dropped before anything renders them; the author's own row and anchor names stay.
        assert!(chart.get("titel").is_none() && chart.get("_renderHeight").is_none());
        assert!(chart["encoding"].get("colour").is_none());
        assert!(chart["axes"]["y"].get("formt").is_none());
        assert!(chart["annotations"][0].get("colr").is_none());
        assert_eq!(chart["annotations"][0]["at"], json!({"m": "Jan"}));
        assert_eq!(chart["data"]["inline"][0], json!({"m": "Jan", "v": 1}));
    }

    #[test]
    fn wrong_types_and_words_are_schema_errors_that_say_what_fits() {
        let mut chart = json!({
            "type": "donut", "title": 2024, "data": {"inline": []},
            "encoding": {"x": "m", "y": ["v"]}, "style": {"ratio": "wide"},
        });
        let found = check(Root::Block("chart"), &mut chart);
        let paths: Vec<_> = found
            .iter()
            .map(|d| (d.code.as_str(), d.path.as_str()))
            .collect();
        for path in [
            "chart.type",
            "chart.title",
            "chart.encoding.y",
            "chart.style.ratio",
        ] {
            assert!(paths.contains(&("block.schema", path)), "{path}: {paths:?}");
        }
        let kind = found.iter().find(|d| d.path == "chart.type").unwrap();
        assert!(
            kind.message.contains("'donut'") && kind.message.contains("not a known chart type"),
            "{}",
            kind.message
        );
        assert!(kind.expected.as_deref().unwrap().contains("'interval'"));
        assert!(found.iter().all(|d| d.level == "error"));
        // Every schema error carries what fits and a one-sentence fix (TYP-53).
        assert!(
            found
                .iter()
                .all(|d| d.expected.is_some() && d.fix.is_some())
        );
        assert!(
            kind.fix
                .as_deref()
                .unwrap()
                .starts_with("Set `chart.type` to")
        );
    }

    #[test]
    fn a_missing_required_field_is_a_schema_error() {
        let mut metric = json!({"label": "Revenue"});
        let found = check(Root::Block("metric"), &mut metric);
        assert_eq!(codes(&found), [("block.schema", "metric.value")]);
    }

    #[test]
    fn a_blank_required_title_is_a_schema_error_but_a_blank_alt_is_decorative() {
        for (tag, mut block) in [
            ("video", json!({"src": "a.mp4", "title": "  "})),
            ("audio", json!({"src": "a.mp3", "title": ""})),
        ] {
            let found = check(Root::Block(tag), &mut block);
            assert_eq!(codes(&found), [("block.schema", &*format!("{tag}.title"))]);
            assert!(found[0].message.contains("must not be empty"));
        }
        let mut image = json!({"src": "a.png", "alt": ""});
        assert!(check(Root::Block("image"), &mut image).is_empty());
    }

    #[test]
    fn unknown_frontmatter_keys_warn_and_stay() {
        let mut front = json!({"title": "Q3", "autor": "Daniel", "_internal": 1});
        let found = check(Root::Frontmatter, &mut front);
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|d| d.code == "frontmatter.unknown-key"));
        assert!(found[0].message.contains("'author'") || found[1].message.contains("'author'"));
        assert_eq!(
            front["autor"], "Daniel",
            "frontmatter keeps unknown keys in its metadata"
        );
    }
}
