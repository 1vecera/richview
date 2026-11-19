//! The reference tables of the spec, generated from the schema registry (ARC-09, TYP-04).
//!
//! A document marks each generated table with a pair of HTML comments,
//! `<!-- registry:begin NAME -->` and `<!-- registry:end -->`. [`splice`] replaces what lies
//! between them with [`fragment`]`(NAME)`, so a reference table can only say what the registry
//! says. Prose around the markers may explain the vocabulary but does not enumerate it.

use crate::registry::{CHART_TYPES, CODES, FRONTMATTER, Field, LAYOUTS, TAGS, Ty};

const BEGIN: &str = "<!-- registry:begin ";
const END: &str = "<!-- registry:end -->";

/// The names of every fragment: `tags`, `frontmatter`, `chart-types`, `styles`, `retired`, `codes`, and
/// `fields/<tag>` for each tag that has its own fields.
pub fn fragment_names() -> Vec<String> {
    let mut names = vec![
        "tags".to_string(),
        "frontmatter".into(),
        "chart-types".into(),
        "styles".into(),
        "retired".into(),
        "codes".into(),
    ];
    names.extend(
        TAGS.iter()
            .chain(LAYOUTS)
            .map(|tag| format!("fields/{}", tag.name)),
    );
    names
}

/// The Markdown table named `name`, ending in a newline, or `None` for an unknown name.
pub fn fragment(name: &str) -> Option<String> {
    match name {
        "tags" => Some(table(
            &["Tag", "Block"],
            TAGS.iter()
                .map(|tag| vec![code(tag.name), tag.summary.to_string()]),
        )),
        "frontmatter" => Some(fields(FRONTMATTER)),
        "styles" => Some(table(
            &["Style", "Default", "Menu"],
            crate::style::presets::preset_names()
                .into_iter()
                .map(|name| {
                    vec![
                        code(name),
                        if name == crate::style::DEFAULT_STYLE {
                            "yes"
                        } else {
                            ""
                        }
                        .into(),
                        if crate::style::builtin_style_names().contains(&name) {
                            "main"
                        } else {
                            "more presets"
                        }
                        .into(),
                    ]
                }),
        )),
        "retired" => {
            use crate::registry::{
                RETIRED, RETIRED_STYLE_FIELDS, RETIRED_STYLES, RETIRED_TAGS, RETIRED_VALUES,
            };
            let mut rows: Vec<Vec<String>> = RETIRED
                .iter()
                .map(|entry| {
                    vec![
                        "field".into(),
                        code(entry.scope),
                        code(entry.path),
                        entry.fix.into(),
                    ]
                })
                .collect();
            rows.extend(
                RETIRED_TAGS
                    .iter()
                    .map(|(old, new)| vec!["tag".into(), "fence".into(), code(old), code(new)]),
            );
            rows.extend(RETIRED_VALUES.iter().map(|entry| {
                vec![
                    "value".into(),
                    code(&format!("{}.{}", entry.scope, entry.path)),
                    code(entry.value),
                    format!("{} with unit {}", code(entry.replacement), code(entry.unit)),
                ]
            }));
            rows.extend(RETIRED_STYLES.iter().map(|(old, new)| {
                vec!["style-name".into(), "styles".into(), code(old), code(new)]
            }));
            rows.extend(RETIRED_STYLE_FIELDS.iter().map(|(old, reason)| {
                vec![
                    "style-field".into(),
                    "style".into(),
                    code(old),
                    (*reason).into(),
                ]
            }));
            Some(table(
                &["Kind", "Scope", "Removed name", "Replacement"],
                rows.into_iter(),
            ))
        }
        "chart-types" => Some(table(
            &["Type", "Required channels", "Optional channels", "Draws"],
            CHART_TYPES.iter().map(|chart| {
                let optional: Vec<&str> = chart
                    .encodings
                    .iter()
                    .copied()
                    .filter(|channel| !chart.required.contains(channel))
                    .collect();
                vec![
                    code(chart.name),
                    listed(chart.required, "and"),
                    listed(&optional, "and"),
                    chart.summary.to_string(),
                ]
            }),
        )),
        "codes" => Some(table(
            &["Code", "Level", "Line", "Meaning", "Fix"],
            CODES.iter().map(|entry| {
                vec![
                    code(entry.code),
                    entry.level.as_str().to_string(),
                    if entry.line { "yes" } else { "" }.to_string(),
                    entry.meaning.to_string(),
                    crate::registry::fix(entry.code)
                        .unwrap_or_default()
                        .to_string(),
                ]
            }),
        )),
        _ => {
            let tag = name.strip_prefix("fields/")?;
            let tag = TAGS.iter().chain(LAYOUTS).find(|t| t.name == tag)?;
            Some(fields(tag.fields))
        }
    }
}

/// `doc` with every marked region refilled from the registry. Fails on an unknown fragment name,
/// a begin marker without its end, or a nested begin.
pub fn splice(doc: &str) -> Result<String, String> {
    let mut out = String::with_capacity(doc.len());
    let mut lines = doc.split_inclusive('\n').enumerate();
    while let Some((index, line)) = lines.next() {
        out.push_str(line);
        let Some(name) = begin_name(line) else {
            continue;
        };
        let body = fragment(name).ok_or_else(|| {
            format!(
                "line {}: `{name}` is not a registry fragment; known: {}",
                index + 1,
                fragment_names().join(", ")
            )
        })?;
        out.push_str(&body);
        loop {
            let Some((inner, next)) = lines.next() else {
                return Err(format!("line {}: `{name}` has no `{END}`", index + 1));
            };
            if begin_name(next).is_some() {
                return Err(format!(
                    "line {}: a registry fragment starts inside `{name}`",
                    inner + 1
                ));
            }
            if next.trim() == END {
                out.push_str(next);
                break;
            }
        }
    }
    Ok(out)
}

/// The fragment names a document marks, in order.
pub fn marked(doc: &str) -> Vec<&str> {
    doc.lines().filter_map(begin_name).collect()
}

fn begin_name(line: &str) -> Option<&str> {
    line.trim()
        .strip_prefix(BEGIN)?
        .strip_suffix("-->")
        .map(str::trim)
}

fn fields(fields: &[Field]) -> String {
    table(
        &["Field", "Type", "Required", "Default", "Meaning"],
        fields.iter().map(|field| {
            vec![
                code(field.path),
                ty(&field.ty),
                if field.required { "yes" } else { "" }.to_string(),
                field.default.map(code).unwrap_or_default(),
                field.summary.to_string(),
            ]
        }),
    )
}

fn ty(value: &Ty) -> String {
    match value {
        Ty::Text => "text".into(),
        Ty::Markdown => "inline Markdown".into(),
        Ty::Number => "number".into(),
        Ty::Integer => "whole number".into(),
        Ty::Digits => "whole number of significant digits from 2 to 6".into(),
        Ty::Bool => "`true` or `false`".into(),
        Ty::OneOf(words) => format!("one of {}", listed(words, "or")),
        Ty::List(item) => format!("list of {}", ty(item)),
        Ty::Map => "mapping".into(),
        Ty::Data => "data source".into(),
        Ty::Column => "column name".into(),
        Ty::Color => "colour".into(),
        Ty::File => "report-folder file".into(),
        Ty::Url => "`http` or `https` address".into(),
        Ty::Date => "date, `YYYY-MM-DD`".into(),
        Ty::Scalar => "text, number or `true`/`false`".into(),
        Ty::NumberOrText => "number or text".into(),
        Ty::Ratio => "positive number or `W:H`".into(),
        Ty::ChartType => "chart type".into(),
        Ty::Any => "any value".into(),
        Ty::Either(a, b) => format!("{} or {}", ty(a), ty(b)),
    }
}

fn code(text: &str) -> String {
    format!("`{text}`")
}

/// `a`, `b` or `c`: each word in a code span, the last joined by `conjunction`.
fn listed(words: &[&str], conjunction: &str) -> String {
    let words: Vec<String> = words.iter().map(|word| code(word)).collect();
    match words.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} {conjunction} {last}", init.join(", ")),
    }
}

fn table(header: &[&str], rows: impl Iterator<Item = Vec<String>>) -> String {
    let cell = |text: &str| text.replace('|', "\\|").replace('\n', " ");
    let mut out = format!("| {} |\n", header.join(" | "));
    out.push_str(&format!("|{}\n", " --- |".repeat(header.len())));
    for row in rows {
        let cells: Vec<String> = row.iter().map(|text| cell(text)).collect();
        out.push_str(&format!("| {} |\n", cells.join(" | ")));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splice_refills_marked_regions_and_keeps_the_rest() {
        let doc = "# Spec\n\nIntro.\n\n<!-- registry:begin chart-types -->\nstale\n<!-- registry:end -->\n\nAfter.\n";
        let out = splice(doc).unwrap();
        assert!(
            out.starts_with("# Spec\n\nIntro.\n\n<!-- registry:begin chart-types -->\n| Type |")
        );
        assert!(out.ends_with("<!-- registry:end -->\n\nAfter.\n"));
        assert!(!out.contains("stale"));
        assert_eq!(out.matches("\n| `").count(), CHART_TYPES.len());
        assert_eq!(splice(&out).unwrap(), out, "splicing is idempotent");
    }

    #[test]
    fn splice_refuses_unknown_names_and_unclosed_or_nested_regions() {
        assert!(splice("<!-- registry:begin nope -->\n<!-- registry:end -->\n").is_err());
        assert!(splice("<!-- registry:begin tags -->\n| x |\n").is_err());
        assert!(
            splice("<!-- registry:begin tags -->\n<!-- registry:begin codes -->\n<!-- registry:end -->\n")
                .is_err()
        );
    }

    #[test]
    fn every_fragment_name_resolves_and_cells_never_break_a_row() {
        for name in fragment_names() {
            let table = fragment(&name).unwrap_or_else(|| panic!("{name}"));
            let columns = table.lines().next().unwrap().matches(" | ").count();
            for line in table.lines() {
                assert_eq!(
                    line.replace("\\|", "").matches(" | ").count(),
                    columns,
                    "{name}: {line}"
                );
            }
        }
        assert!(fragment("fields/big").is_none(), "`big` is retired");
    }
}
