//! Editor completion offers the schema registry's vocabulary and nothing else (ARC-09, TYP-03).
//!
//! The registry is the one definition of tags, fields and chart types; a hand-kept list in the editor would drift
//! from it the first time a field is added, so these tests compare the completion lists with the registry itself.
use richview_core::document::parse;
use richview_core::registry::{self, FRONTMATTER, TAGS};
use richview_editor::authoring::complete;
use std::collections::BTreeSet;

/// The labels offered at the `|` in `source`.
fn labels(source: &str) -> BTreeSet<String> {
    let (before, after) = source.split_once('|').unwrap();
    let full = format!("{before}{after}");
    let line = before.bytes().filter(|b| *b == b'\n').count();
    let character = before.rsplit('\n').next().unwrap().encode_utf16().count();
    complete(
        &full,
        line,
        character,
        &parse(&full).unwrap(),
        &Default::default(),
    )
    .as_array()
    .unwrap()
    .iter()
    .map(|choice| choice["label"].as_str().unwrap().to_owned())
    .collect()
}

/// The names an author writes directly under the tag (or frontmatter) itself, from the registry.
fn top_level(fields: &[registry::Field]) -> BTreeSet<String> {
    fields
        .iter()
        .map(|field| field.path.replace("[n]", ""))
        .filter(|path| !path.contains('.'))
        .collect()
}

#[test]
fn every_tags_keys_come_from_the_registry() {
    for tag in TAGS {
        let offered = labels(&format!("```{}\n|\n```\n", tag.name));
        let registered = top_level(tag.fields);
        assert_eq!(
            offered, registered,
            "completion inside a `{}` fence",
            tag.name
        );
    }
}

#[test]
fn frontmatter_keys_come_from_the_registry() {
    assert_eq!(labels("---\n|\n---\n"), top_level(FRONTMATTER));
}

#[test]
fn chart_types_come_from_the_registry() {
    let registered: BTreeSet<String> = registry::CHART_TYPES
        .iter()
        .map(|chart| chart.name.to_owned())
        .collect();
    assert_eq!(labels("```chart\ntype: |\n```\n"), registered);
}

#[test]
fn the_channels_of_each_chart_type_are_the_registrys() {
    for chart in registry::CHART_TYPES {
        let offered = labels(&format!(
            "```chart\ntype: {}\nencoding:\n  |\n```\n",
            chart.name
        ));
        let registered: BTreeSet<String> = chart
            .encodings
            .iter()
            .map(|channel| (*channel).to_owned())
            .collect();
        assert_eq!(
            offered, registered,
            "channels under `encoding` of a `{}` chart",
            chart.name
        );
    }
    // Before a type is written, any channel of any type may be offered.
    assert!(labels("```chart\nencoding:\n  |\n```\n").len() > 10);
}
