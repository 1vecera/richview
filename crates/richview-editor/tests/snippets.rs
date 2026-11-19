use richview_editor::snippets;

/// A snippet as the text it inserts: placeholders become their default text, choices their first option, and escapes go.
fn expand(snippet: &str) -> String {
    let mut out = String::new();
    let mut chars = snippet.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.extend(chars.next()),
            '$' if chars.peek() == Some(&'{') => {
                chars.next();
                while chars.peek().is_some_and(char::is_ascii_digit) {
                    chars.next();
                }
                // A choice `|a,b|` expands to its first option; a placeholder `:text` to its text.
                if chars.next() == Some('|') {
                    for c in chars.by_ref() {
                        if c == ',' || c == '|' {
                            break;
                        }
                        out.push(c);
                    }
                    while chars.next_if(|c| *c != '}').is_some() {}
                    chars.next();
                    continue;
                }
                let mut depth = 1;
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => out.extend(chars.next()),
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        c => out.push(c),
                    }
                }
            }
            c => out.push(c),
        }
    }
    out
}

#[test]
fn snippet_catalog_contains_each_block_and_chart_once() {
    let all = snippets();
    let mut ids: Vec<_> = all
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_owned())
        .collect();
    let mut expected: Vec<_> = richview_core::CHART_TYPES
        .iter()
        .copied()
        .chain(
            richview_core::registry::TAGS
                .iter()
                .chain(richview_core::registry::LAYOUTS)
                .map(|tag| tag.name)
                .filter(|name| *name != "chart"),
        )
        .collect();
    ids.sort();
    expected.sort();
    assert_eq!(
        ids, expected,
        "one snippet per block, layout and chart type"
    );
}

#[test]
fn chart_snippets_compile_as_actual_source() {
    let mut failures = Vec::new();
    for item in snippets().as_array().unwrap() {
        let id = item["id"].as_str().unwrap();
        if !richview_core::CHART_TYPES.contains(&id) {
            continue;
        }
        let chart = expand(item["body"].as_str().unwrap());
        for style in richview_core::builtin_style_names() {
            let source = format!("---\ntitle: Snippet\nstyles: [{style}]\n---\n\n{chart}");
            let report = richview_core::document::render(
                &source,
                &richview_core::document::RenderOptions {
                    report_folder: Some(richview_core::document::ReportFolder {
                        root: "snippet-fixture".into(),
                        source: "report.rmd".into(),
                        files: vec!["report.rmd".into()],
                    }),
                    ..Default::default()
                },
            )
            .unwrap_or_else(|e| panic!("{id} in {style}: {e:?}"));
            // The figure carries its payload for the report runtime (ARC-05).
            if report.charts.len() != 1 || !report.html.contains("data-rv-payload") {
                failures.push(format!("{id} in {style}: no chart payload"));
            }
            for d in report
                .diagnostics
                .iter()
                .filter(|d| d.level == "error" || d.level == "warning")
            {
                failures.push(format!("{id} in {style}: {} {}", d.code, d.message));
            }
        }
    }
    failures.dedup();
    assert!(
        failures.is_empty(),
        "{} findings:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn every_block_tag_has_a_snippet_that_renders_clean() {
    let ids: Vec<String> = snippets()
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_owned())
        .collect();
    let files = [
        "report.rmd",
        "media/photo.png",
        "media/interview.mp3",
        "media/demo.mp4",
    ];
    let mut failures = Vec::new();
    for tag in richview_core::registry::TAGS {
        let name = tag.name;
        // A chart's snippets are the chart types, checked above; the others are named by their tag.
        if name == "chart" {
            continue;
        }
        let Some(item) = snippets()
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["id"] == name)
            .cloned()
        else {
            failures.push(format!("no snippet for the {name} block ({ids:?})"));
            continue;
        };
        let block = expand(item["body"].as_str().unwrap());
        for style in richview_core::builtin_style_names() {
            let source = format!("---\ntitle: Snippet\nstyles: [{style}]\n---\n\n{block}");
            let report = richview_core::document::render(
                &source,
                &richview_core::document::RenderOptions {
                    report_folder: Some(richview_core::document::ReportFolder {
                        root: "snippet-fixture".into(),
                        source: "report.rmd".into(),
                        files: files.iter().map(|f| (*f).into()).collect(),
                    }),
                    ..Default::default()
                },
            )
            .unwrap_or_else(|e| panic!("{name} in {style}: {e:?}"));
            for d in report
                .diagnostics
                .iter()
                .filter(|d| d.level == "error" || d.level == "warning")
            {
                failures.push(format!("{name} in {style}: {} {}", d.code, d.message));
            }
        }
    }
    failures.dedup();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The committed VS Code snippet file is what the generator writes. The docs pages under
/// `docs/public/charts` feed it, so a new page makes the file stale; regenerate it with
/// `UPDATE_SNIPPETS=1 cargo test -p richview-editor --test snippets committed_snippets_match_the_generator`.
#[test]
fn committed_snippets_match_the_generator() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/vscode/snippets/rmd.json");
    let generated = richview_editor::vscode_snippet_file();
    if std::env::var_os("UPDATE_SNIPPETS").is_some() {
        std::fs::write(&path, &generated).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        committed.trim_end(),
        generated.trim_end(),
        "packages/vscode/snippets/rmd.json is stale against docs/public/charts; run: UPDATE_SNIPPETS=1 cargo test -p richview-editor --test snippets committed_snippets_match_the_generator"
    );
}
