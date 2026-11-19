//! The author can fold a section without losing its text, rich blocks or anchors.
use richview_core::document::{Output, RenderOptions, ReportFolder, render};

/// A report in its own folder, as every host gives one.
fn options() -> RenderOptions {
    RenderOptions {
        report_folder: Some(ReportFolder {
            root: "report".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }),
        ..Default::default()
    }
}

fn html(source: &str, output: Output) -> String {
    render(
        source,
        &RenderOptions {
            output,
            ..options()
        },
    )
    .expect("report renders")
    .html
}

#[test]
fn sections_start_closed_end_at_the_next_peer_and_keep_rich_blocks() {
    let source = "---\ntitle: Report\ncollapsed_sections: [sources-cited]\n---\n\n## Sources cited\n\nIntro.\n\n```metric\nvalue: 42\nlabel: Sources\n```\n\n### Detail\n\nMore.\n\n## Next\n\nOutside.\n";
    let screen = html(source, Output::Report);
    let start = screen
        .find("<details class=\"rv-section\"><summary><h2 id=\"sources-cited\">")
        .expect("closed section");
    let end = screen[start..].find("</details>").expect("section ends") + start;
    assert!(screen[start..end].contains("Intro."));
    assert!(screen[start..end].contains("rv-metric"));
    assert!(screen[start..end].contains("id=\"detail\""));
    assert!(screen[end..].contains("id=\"next\""));
    // On paper nothing folds: the section is the unfolded report's, with no marker left behind.
    let paper = html(source, Output::Pdf);
    let hex: String = "sources-cited"
        .bytes()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    // The page's CSS names `.rv-section` in print.css; the body has no such element.
    assert!(!paper.contains("<details class=\"rv-section\""));
    assert!(!paper.contains(&format!("2:{hex}-->")));
    assert!(paper.contains("<h2 id=\"sources-cited\">Sources cited</h2>"));
    assert!(paper.contains("class=\"rv-metric"));
}

#[test]
fn a_folded_last_section_leaves_the_notes_in_view() {
    // The Notes follow the last section; a fold closes before them, in a report and in a panel.
    let plain = "---\ntitle: Report\ncollapsed_sections: [sources-cited]\n---\n\n## Findings\n\nA claim.[^1]\n\n## Sources cited\n\nThe export.\n\n[^1]: The note.\n";
    let tabbed = "---\ntitle: Report\ncollapsed_sections: [sources-cited]\ntabs:\n  panels:\n    - {id: one, label: One, from: one}\n    - {id: two, label: Two, from: two}\n---\n\n## One {#one}\n\nFirst.\n\n## Two {#two}\n\nA claim.[^1]\n\n## Sources cited\n\nThe export.\n\n[^1]: The note.\n";
    for (name, source) in [("plain", plain), ("tabbed", tabbed)] {
        let out = html(source, Output::Report);
        let fold = out
            .find("<details class=\"rv-section\"><summary><h2 id=\"sources-cited\">")
            .unwrap_or_else(|| panic!("{name}: the section folds"));
        let closed = out[fold..].find("</details>").expect("the fold closes") + fold;
        let notes = out
            .rfind("<section class=\"rv-notes\"")
            .unwrap_or_else(|| panic!("{name}: notes"));
        assert!(out[fold..closed].contains("The export."), "{name}");
        assert!(closed < notes, "{name}: the notes are inside the fold");
        assert!(out[notes..].contains("The note."), "{name}");
    }
}

#[test]
fn section_folds_do_not_cross_tab_boundaries() {
    let source = "---\ntitle: Report\ncollapsed_sections: [one]\ntabs:\n  panels:\n    - {id: one, label: One, from: one}\n    - {id: two, label: Two, from: two}\n---\n\n## One {#one}\n\nFirst.\n\n## Two {#two}\n\nSecond.\n";
    let out = html(source, Output::Report);
    let first = out.find("id=\"panel-one\"").unwrap();
    let second = out.find("id=\"panel-two\"").unwrap();
    let panel = &out[first..second];
    assert!(panel.contains("<details class=\"rv-section\">"));
    assert!(
        panel.contains("First.</p>\n</details></section>"),
        "{panel}"
    );
    assert!(!out[second..].contains("</details>"));
}

#[test]
fn nested_disclosures_close_in_heading_order() {
    let source = "---\ntitle: Report\ncollapsed_sections: [first, inner]\n---\n\n## First\n\n### Inner\n\nText.\n\n## Last\n\nEnd.\n";
    let out = html(source, Output::Report);
    let start = out.find("<details class=\"rv-section\">").unwrap();
    let end = out.find("<h2 id=\"last\">").unwrap();
    let section = &out[start..end];
    assert_eq!(section.matches("<details class=\"rv-section\">").count(), 2);
    assert_eq!(section.matches("</details>").count(), 2);
    assert!(section.contains("Text.</p>\n</details></details>"));
}

#[test]
fn an_unknown_anchor_warns_and_keeps_the_section_readable() {
    let source =
        "---\ntitle: Report\ncollapsed_sections: [missing]\n---\n\n## Present\n\nStill readable.\n";
    let out = render(source, &options()).unwrap();
    let diagnostic = out
        .diagnostics
        .iter()
        .find(|d| d.code == "section.anchor-missing")
        .expect("missing anchor warning");
    assert_eq!(diagnostic.path, "frontmatter.collapsed_sections[0]");
    assert!(out.html.contains("Still readable."));
    assert!(!out.html.contains("<details class=\"rv-section\">"));
    assert!(!out.html.contains("<!--richview:heading:"));
}

#[test]
fn untrusted_comments_and_nested_headings_do_not_create_disclosures() {
    let source = "---\ntitle: Report\ncollapsed_sections: [nested, real]\n---\n\n<!--richview:heading:2:7265616c-->\n\n> ## Nested\n>\n> Quoted text.\n\n## Real\n\nActual text.\n";
    let out = render(source, &options()).unwrap();
    assert_eq!(
        out.html.matches("<details class=\"rv-section\">").count(),
        1
    );
    assert!(out.diagnostics.iter().any(
        |d| d.code == "section.anchor-missing" && d.path == "frontmatter.collapsed_sections[0]"
    ));
    let start = out.html.find("<details class=\"rv-section\">").unwrap();
    assert!(out.html[..start].contains("Quoted text."));
    assert!(out.html[start..].contains("Actual text."));
}

#[test]
fn missing_anchor_diagnostic_keeps_the_original_list_index() {
    let source =
        "---\ntitle: Report\ncollapsed_sections: [present, missing]\n---\n\n## Present\n\nBody.\n";
    let out = render(source, &options()).unwrap();
    assert!(out.diagnostics.iter().any(
        |d| d.code == "section.anchor-missing" && d.path == "frontmatter.collapsed_sections[1]"
    ));
}

#[test]
fn linked_headings_stay_open_with_the_link_usable() {
    let source = "---\ntitle: Report\ncollapsed_sections: [sources]\n---\n\n## [Sources](https://example.org)\n\nSource list.\n";
    let out = render(source, &options()).unwrap();
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.code == "section.heading-interactive"
                && d.path == "frontmatter.collapsed_sections[0]")
    );
    assert!(out.html.contains("<a href=\"https://example.org\""));
    assert!(!out.html.contains("<details class=\"rv-section\">"));
    assert!(!out.html.contains("<!--richview:heading:"));
}

#[test]
fn repeated_headings_use_the_unique_generated_anchor() {
    let source = "---\ntitle: Report\ncollapsed_sections: [sources-2]\n---\n\n## Sources\n\nFirst list.\n\n## Sources\n\nSecond list.\n\n## After\n\nLast.\n";
    let out = html(source, Output::Report);
    let first = out.find("First list.").unwrap();
    let fold = out.find("<details class=\"rv-section\">").unwrap();
    let second = out.find("Second list.").unwrap();
    let after = out.find("<h2 id=\"after\">").unwrap();
    assert!(first < fold && fold < second && second < after);
    assert!(out[fold..after].contains("id=\"sources-2\""));
    assert_eq!(out.matches("<details class=\"rv-section\">").count(), 1);
}

#[test]
fn malformed_configuration_keeps_all_content_readable() {
    let source = "---\ntitle: Report\ncollapsed_sections: false\n---\n\n## Sources\n\nSource list.\n\n## More\n\nFollowing content.\n";
    let out = render(source, &options()).unwrap();
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.path == "frontmatter.collapsed_sections")
    );
    assert!(out.html.contains("Source list."));
    assert!(out.html.contains("Following content."));
    assert!(!out.html.contains("<details class=\"rv-section\">"));
    assert!(!out.html.contains("<!--richview:heading:"));
}

#[test]
fn a_repeated_section_entry_does_not_nest_duplicate_disclosures() {
    let source = "---\ntitle: Report\ncollapsed_sections: [sources, sources]\n---\n\n## Sources\n\nSource list.\n\n## After\n\nFollowing content.\n";
    let out = html(source, Output::Report);
    assert_eq!(out.matches("<details class=\"rv-section\">").count(), 1);
    assert_eq!(out.matches("</details>").count(), 1);
    assert!(out.find("Source list.") < out.find("</details>"));
    assert!(out.find("</details>") < out.find("Following content."));
}
