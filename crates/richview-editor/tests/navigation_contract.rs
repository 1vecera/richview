use richview_editor::authoring::outline;
use serde_json::{Value, json};

fn nodes(value: &Value) -> Vec<&Value> {
    fn walk<'a>(items: &'a Value, found: &mut Vec<&'a Value>) {
        for item in items.as_array().unwrap() {
            found.push(item);
            walk(&item["children"], found);
        }
    }
    let mut found = vec![];
    walk(&value["symbols"], &mut found);
    found
}

fn valid_ranges(source: &str, result: &Value) {
    let lines: Vec<_> = source.split('\n').collect();
    let position = |value: &Value| {
        let line = value["line"].as_u64().unwrap() as usize;
        let character = value["character"].as_u64().unwrap() as usize;
        assert!(line < lines.len(), "{value}");
        assert!(
            character <= lines[line].trim_end_matches('\r').encode_utf16().count(),
            "{value}"
        );
        (line, character)
    };
    for node in nodes(result) {
        let start = position(&node["range"]["start"]);
        let end = position(&node["range"]["end"]);
        let selected_start = position(&node["selectionRange"]["start"]);
        let selected_end = position(&node["selectionRange"]["end"]);
        assert!(
            start <= selected_start && selected_start <= selected_end && selected_end <= end,
            "{node}"
        );
        for child in node["children"].as_array().unwrap() {
            assert!(
                start <= position(&child["range"]["start"])
                    && position(&child["range"]["end"]) <= end
            );
        }
    }
    for fold in result["folds"].as_array().unwrap() {
        let start = fold["start"].as_u64().unwrap() as usize;
        let end = fold["end"].as_u64().unwrap() as usize;
        assert!(start < end && end < lines.len(), "{fold}");
    }
}

#[test]
fn report_navigation_nests_sections_and_named_blocks_with_exact_unicode_ranges() {
    let source = "\u{feff}---\r\ntitle: Metadata\r\n---\r\n# 🦀 **Revenue**\r\nIntro\r\n## Distribution\r\n```chart\r\ntitle: 'Wait 🦀'\r\ndata: [unfinished\r\n```\r\n\r\n# Next\r\nLast\r\n";
    let result = outline(source);
    valid_ranges(source, &result);
    assert_eq!(result["truncated"], false);
    let roots = result["symbols"].as_array().unwrap();
    assert_eq!(roots.len(), 2);
    assert_eq!(roots[0]["name"], "🦀 Revenue");
    assert_eq!(
        roots[0]["selectionRange"],
        json!({"start":{"line":3,"character":2},"end":{"line":3,"character":14}})
    );
    assert_eq!(roots[0]["range"]["end"], json!({"line":9,"character":3}));
    assert_eq!(roots[0]["children"][0]["name"], "Distribution");
    assert_eq!(roots[0]["children"][0]["children"][0]["name"], "Wait 🦀");
    assert_eq!(roots[0]["children"][0]["children"][0]["detail"], "chart");
    assert!(
        result["folds"]
            .as_array()
            .unwrap()
            .contains(&json!({"start":0,"end":2}))
    );
    assert!(
        result["folds"]
            .as_array()
            .unwrap()
            .contains(&json!({"start":6,"end":9}))
    );
}

#[test]
fn a_heading_anchor_is_not_part_of_the_section_name() {
    // The outline reads Markdown as the renderer does: `{#sales}` is the heading's id (TYP-34).
    let source = "## Sales {#sales}\n\nDetail.\n";
    let result = outline(source);
    valid_ranges(source, &result);
    assert_eq!(result["symbols"][0]["name"], "Sales");
}

#[test]
fn setext_and_nested_fences_keep_names_but_code_and_html_do_not_create_headings() {
    let source = "A `code` [link](https://example.test)\n==============================\n\n> ### Quoted\n>\n> ```metric\n> label: Accepted observations\n> value: 4\n> ```\n\n~~~rust\n# Not a heading\n~~~\n\n<div>\n# Also not a heading\n</div>\n\n    # Indented code\n    still code\n";
    let result = outline(source);
    valid_ranges(source, &result);
    assert_eq!(
        nodes(&result)
            .iter()
            .map(|node| node["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["A code link", "Quoted", "Accepted observations"]
    );
    assert!(
        result["folds"]
            .as_array()
            .unwrap()
            .contains(&json!({"start":10,"end":12}))
    );
    assert!(
        result["folds"]
            .as_array()
            .unwrap()
            .contains(&json!({"start":18,"end":19}))
    );
}

#[test]
fn closed_metadata_and_partial_blocks_follow_source_navigation_rules() {
    for source in [
        "",
        "#",
        "#\r\n",
        "\u{feff}# 🦀",
        "---\ntitle: unfinished\n# Still metadata\n",
        "---\r\ntitle: T\r\n...\r\n## Visible\r\n",
    ] {
        valid_ranges(source, &outline(source));
    }
    assert_eq!(
        outline("---\ntitle: unfinished\n# Visible fallback\n")["symbols"][0]["name"],
        "Visible fallback"
    );
    assert_eq!(outline("#")["symbols"][0]["name"], "Untitled section");
    let source = "```table\ndata: {inline: []}\n```\n\n```chart\ndata:\n  inline:\n    - title: An observation is not the chart name\ntitle: Chart name\n";
    let result = outline(source);
    valid_ranges(source, &result);
    assert_eq!(
        nodes(&result)
            .iter()
            .map(|node| node["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["Table", "Chart name"]
    );
    let invalid = outline("```chart\n- title\n- Not a mapping\n```\n");
    assert_eq!(invalid["symbols"][0]["name"], "Chart");
    for source in [
        "```chart\n? [title, nested]\n: Not a scalar key\n```\n",
        "```chart\nother: value\n---\ntitle: Second document\n```\n",
    ] {
        assert_eq!(outline(source)["symbols"][0]["name"], "Chart");
    }
}

#[test]
fn authored_labels_name_metrics_callouts_and_media_without_nested_value_confusion() {
    let source = concat!(
        "```metric\nlabel: Revenue\nvalue: 123\n```\n\n",
        "```callout\ntitle: Data coverage\nbody: Observations retained\n```\n\n",
        "```image\nalt: Accessible description\ncaption: Figure caption\n```\n\n",
        "```audio\ncaption: Recording caption\n```\n\n",
        "```video\ncaption: Alternate caption\ntitle: Video title\n```\n\n",
        "```embed\nurl: https://example.test/demo\n```\n"
    );
    let result = outline(source);
    valid_ranges(source, &result);
    assert_eq!(
        nodes(&result)
            .iter()
            .map(|node| node["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "Revenue",
            "Data coverage",
            "Figure caption",
            "Recording caption",
            "Video title",
            "https://example.test/demo"
        ]
    );
}

#[test]
fn navigation_bounds_very_long_names_and_discloses_large_document_truncation() {
    let source = format!("# {}\n", "🦀".repeat(250));
    let result = outline(&source);
    valid_ranges(&source, &result);
    assert_eq!(
        result["symbols"][0]["name"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        201
    );
    assert_eq!(outline(&"# Section\n".repeat(10_000))["truncated"], false);
    let source = "# Section\n".repeat(10_001);
    let result = outline(&source);
    valid_ranges(&source, &result);
    assert_eq!(result["truncated"], true);
    assert_eq!(nodes(&result).len(), 10_000);
}
