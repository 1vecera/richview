//! Shared authoring vocabulary. Open metadata and row/coordinate keys remain extensible.
use crate::{RenderOptions, RichBlock};
use serde_json::Value;

use crate::registry::{self, Field, Ty};
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

const CHART_TYPE_COUNT: usize = registry::CHART_TYPES.len();

/// The names of the registry's chart types, in its order.
const CHART_TYPE_NAMES: [&str; CHART_TYPE_COUNT] = {
    let mut names = [""; CHART_TYPE_COUNT];
    let mut i = 0;
    while i < CHART_TYPE_COUNT {
        names[i] = registry::CHART_TYPES[i].name;
        i += 1;
    }
    names
};

/// Source chart types shared by completion, generated metadata and editor insertion: the
/// registry's `CHART_TYPES`, which is where a new type is added.
pub const CHART_TYPES: &[&str] = &CHART_TYPE_NAMES;

/// The registry's fields for `tag`: `frontmatter`, a rich tag, or a deck's `deck-setup` and
/// `deck-slide` (FEA-RP-71), which stand where a rich tag does for the keys of a `.rslide`.
fn fields_of(tag: &str) -> Option<&'static [Field]> {
    match tag {
        "frontmatter" => Some(registry::FRONTMATTER),
        "deck-setup" => Some(registry::DECK_SETUP),
        "deck-slide" => Some(registry::DECK_SLIDE),
        _ => registry::tag(tag).map(|tag| tag.fields),
    }
}

/// A registry path without its list markers: `columns[n].key` is `columns.key`, the way the
/// editor names the path of a key inside a list item.
fn plain(path: &str) -> String {
    path.replace("[n]", "")
}

/// The keys found so far, by `tag:path`.
type KeyCache = BTreeMap<String, Option<&'static [&'static str]>>;

/// The keys an author may write at `path` below `tag`, read from the registry (ARC-09): the
/// fields directly under it, or under each of its list items. `None` where the registry lists
/// none, which is where the author's own names go (rows, anchors).
pub fn authoring_keys(tag: &str, path: &[&str]) -> Option<&'static [&'static str]> {
    static CACHE: OnceLock<Mutex<KeyCache>> = OnceLock::new();
    let path: Vec<_> = path
        .iter()
        .copied()
        .filter(|s| s.parse::<usize>().is_err())
        .collect();
    let at = path.join(".");
    let key = format!("{tag}:{at}");
    let mut cache = CACHE.get_or_init(Default::default).lock().ok()?;
    if let Some(found) = cache.get(&key) {
        return *found;
    }
    let fields = fields_of(tag)?;
    let prefix = if at.is_empty() {
        String::new()
    } else {
        format!("{at}.")
    };
    let keys: Vec<&'static str> = fields
        .iter()
        .filter_map(|field| {
            let rest = plain(field.path);
            let rest = rest.strip_prefix(&prefix)?.to_owned();
            if rest.contains('.') {
                return None;
            }
            // The key's name is the tail of the registry path itself, which lives forever.
            field.path.rsplit('.').next()
        })
        .collect();
    let found: Option<&'static [&'static str]> =
        (!keys.is_empty()).then(|| &*Box::leak(keys.into_boxed_slice()));
    cache.insert(key, found);
    found
}

/// The words an author may write for the field at `path` below `tag`, read from the registry.
pub fn authoring_values(tag: &str, path: &[&str]) -> &'static [&'static str] {
    let path: Vec<_> = path
        .iter()
        .copied()
        .filter(|s| s.parse::<usize>().is_err())
        .collect();
    let at = path.join(".");
    if matches!(tag, "frontmatter" | "deck-setup") && at == "styles" {
        return registry::BUILTIN_STYLES;
    }
    let Some(field) =
        fields_of(tag).and_then(|fields| fields.iter().find(|field| plain(field.path) == at))
    else {
        return &[];
    };
    words(&field.ty)
}

fn words(ty: &'static Ty) -> &'static [&'static str] {
    match ty {
        Ty::OneOf(words) => words,
        Ty::Bool => &["true", "false"],
        Ty::ChartType => CHART_TYPES,
        Ty::List(item) => words(item),
        Ty::Either(a, b) => {
            let first = words(a);
            if first.is_empty() { words(b) } else { first }
        }
        _ => &[],
    }
}

/// Use the same inline/resolved CSV/JSON operation as report rendering.
pub fn authoring_fields(
    block: &RichBlock,
    frontmatter: &Value,
    options: &RenderOptions,
) -> Vec<String> {
    let mut block = block.clone();
    crate::blocks::resolve_data(&mut block, frontmatter, options, &mut vec![]);
    let mut fields = std::collections::BTreeSet::new();
    if let Some(rows) = block
        .value
        .pointer("/data/inline")
        .and_then(Value::as_array)
    {
        for row in rows {
            if let Some(row) = row.as_object() {
                fields.extend(row.keys().cloned());
            }
        }
    }
    fields.into_iter().collect()
}

/// Make `style` the first (base) entry of the frontmatter `styles` list, preserving every
/// other byte. Later entries, such as a local `.rstyle.yaml`, keep their order and still
/// override the base. Inserts `styles: [style]` when frontmatter has no list. Returns `None`
/// for sources without frontmatter, where a host must pass the style as an explicit option.
pub fn with_primary_style(source: &str, style: &str) -> Option<String> {
    let mut lines: Vec<&str> = source.split_inclusive('\n').collect();
    let is_fence = |line: &str| line.trim_end_matches(['\n', '\r']) == "---";
    if !lines.first().is_some_and(|line| is_fence(line)) {
        return None;
    }
    let close = (1..lines.len()).find(|&i| is_fence(lines[i]) || lines[i].trim_end() == "...")?;
    let eol = if lines[0].ends_with("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let replaced: String;
    let Some(at) =
        (1..close).find(|&i| lines[i].starts_with("styles:") || lines[i].starts_with("styles :"))
    else {
        let inserted = format!("styles: [{style}]{eol}");
        let mut out: String = lines[..close].concat();
        out.push_str(&inserted);
        out.push_str(&lines[close..].concat());
        return Some(out);
    };
    let line = lines[at];
    let body = line.trim_end_matches(['\n', '\r']);
    let ending = &line[body.len()..];
    let colon = body.find(':')? + 1;
    let value = &body[colon..];
    let trimmed = value.trim();
    if let Some(inner) = trimmed.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
        let lead = &value[..value.len() - value.trim_start().len()];
        let mut items: Vec<String> = if inner.trim().is_empty() {
            vec![]
        } else {
            inner.split(',').map(str::to_string).collect()
        };
        match items.first_mut() {
            Some(first) => {
                let pad_left = &first[..first.len() - first.trim_start().len()];
                let pad_right = &first[first.trim_end().len()..];
                *first = format!("{pad_left}{style}{pad_right}");
            }
            None => items.push(style.into()),
        }
        replaced = format!("{}{lead}[{}]{ending}", &body[..colon], items.join(","));
        lines[at] = &replaced;
    } else if !trimmed.is_empty() && !trimmed.starts_with('#') {
        replaced = format!("{}: {style}{ending}", &body[..colon - 1]);
        lines[at] = &replaced;
    } else {
        let item = (at + 1..close).find(|&i| lines[i].trim_start().starts_with("- "))?;
        let entry = lines[item];
        let entry_body = entry.trim_end_matches(['\n', '\r']);
        let dash = entry_body.find("- ")? + 2;
        let rest = &entry_body[dash..];
        let comment = rest.find(" #").map_or("", |p| &rest[p..]);
        replaced = format!(
            "{}{style}{comment}{}",
            &entry_body[..dash],
            &entry[entry_body.len()..]
        );
        lines[item] = &replaced;
    }
    Some(lines.concat())
}

/// The first (base) entry of the frontmatter `styles` list, if the source declares one.
pub fn primary_style(source: &str) -> Option<String> {
    let mut lines = source.lines();
    if lines.next()?.trim_end() != "---" {
        return None;
    }
    let mut block = false;
    for line in lines {
        let trimmed = line.trim_end();
        if trimmed == "---" || trimmed == "..." {
            return None;
        }
        if block {
            if let Some(item) = trimmed.trim_start().strip_prefix("- ") {
                return Some(clean_style(item));
            }
            if !line.starts_with(' ') && !line.starts_with('-') {
                block = false;
            }
            continue;
        }
        let Some(value) = trimmed.strip_prefix("styles:") else {
            continue;
        };
        let value = value.split(" #").next().unwrap_or("").trim();
        if value.is_empty() {
            block = true;
            continue;
        }
        let first = value
            .trim_start_matches('[')
            .split([',', ']'])
            .next()
            .unwrap_or("");
        return Some(clean_style(first)).filter(|s| !s.is_empty());
    }
    None
}

fn clean_style(value: &str) -> String {
    value
        .split(" #")
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches(['"', '\''])
        .to_string()
}

#[cfg(test)]
mod primary_style_tests {
    use super::with_primary_style;

    #[test]
    fn replaces_or_inserts_the_base_style_only() {
        let flow = "---\ntitle: T\nstyles: [sleek, report.rstyle.yaml]\n---\n\nBody\n";
        assert_eq!(
            with_primary_style(flow, "scientific").unwrap(),
            "---\ntitle: T\nstyles: [scientific, report.rstyle.yaml]\n---\n\nBody\n"
        );
        let block = "---\nstyles:\n- scientific\n- report.rstyle.yaml\n---\n";
        assert_eq!(
            with_primary_style(block, "brutalist").unwrap(),
            "---\nstyles:\n- brutalist\n- report.rstyle.yaml\n---\n"
        );
        assert_eq!(
            with_primary_style("---\nstyles: sleek\n---\n", "scientific").unwrap(),
            "---\nstyles: scientific\n---\n"
        );
        assert_eq!(
            with_primary_style("---\ntitle: T\n---\nx", "sleek").unwrap(),
            "---\ntitle: T\nstyles: [sleek]\n---\nx"
        );
        assert_eq!(
            with_primary_style("---\nstyles: []\n---\n", "sleek").unwrap(),
            "---\nstyles: [sleek]\n---\n"
        );
        assert!(with_primary_style("# No frontmatter\n", "sleek").is_none());
        assert_eq!(super::primary_style(flow).as_deref(), Some("sleek"));
        assert_eq!(super::primary_style(block).as_deref(), Some("scientific"));
        assert_eq!(
            super::primary_style("---\nstyles: 'brutalist' # x\n---\n").as_deref(),
            Some("brutalist")
        );
        assert_eq!(super::primary_style("---\ntitle: T\n---\n"), None);
        assert!(with_primary_style("---\ntitle: unterminated\n", "sleek").is_none());
    }
}
