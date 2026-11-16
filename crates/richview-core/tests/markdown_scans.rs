//! Every scan of report Markdown reads it as the renderer does (TYP-34): no scan finds a
//! wikilink, an old-style footnote, smart punctuation or a metadata block the renderer never
//! renders (FEA-RP-10, FEA-RP-12).
use regex::Regex;
use richview_core::document::{RenderOptions, ReportFolder, parse, render, report_resources};
use std::path::{Path, PathBuf};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every `.rs` file under the `src` trees of the workspace's crates and of `richview-chart`.
fn sources() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }
    let mut roots = vec![workspace().join("richview-chart/src")];
    for krate in std::fs::read_dir(workspace().join("crates"))
        .unwrap()
        .flatten()
    {
        roots.push(krate.path().join("src"));
    }
    let mut files = vec![];
    for root in roots.iter().filter(|root| root.is_dir()) {
        walk(root, &mut files);
    }
    files.sort();
    files
}

/// The text from just after an opening parenthesis to its matching close.
fn arguments(text: &str) -> &str {
    let mut depth = 1;
    for (at, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return &text[..at];
                }
            }
            _ => {}
        }
    }
    text
}

#[test]
fn every_markdown_parser_reads_with_the_renderers_options() {
    let parser = Regex::new(r"\bParser::new(_ext)?\(|\bMdParser::new(_ext)?\(").unwrap();
    let shared = [
        "FLAGS",
        "crate::markdown::FLAGS",
        "richview_core::document::MARKDOWN_OPTIONS",
    ];
    let mut wrong = vec![];
    let mut seen = 0;
    for file in sources() {
        let text = std::fs::read_to_string(&file).unwrap();
        // Unit tests sit at the end of each file and may read Markdown however they compare it.
        let live = &text[..text.find("#[cfg(test)]").unwrap_or(text.len())];
        for found in parser.find_iter(live) {
            seen += 1;
            let call = arguments(&live[found.end()..]).trim().trim_end_matches(',');
            let options = call.rsplit_once(',').map(|(_, options)| options.trim());
            let line = live[..found.start()].lines().count();
            let file = file.strip_prefix(workspace()).unwrap_or(&file).display();
            match options {
                Some(options) if found.as_str().ends_with("_ext(") => {
                    if !shared.contains(&options) {
                        wrong.push(format!("{file}:{line} reads with {options}"));
                    }
                }
                _ => wrong.push(format!("{file}:{line} reads without options")),
            }
        }
    }
    assert!(seen >= 10, "the scan found only {seen} Markdown parsers");
    assert!(
        wrong.is_empty(),
        "every Markdown parse uses the renderer's options, `FLAGS` in core and \
         `richview_core::document::MARKDOWN_OPTIONS` elsewhere:\n{}",
        wrong.join("\n")
    );
}

/// A wikilink, an image wikilink, and a footnote whose second paragraph links a file, shows an
/// image and holds a chart fence. The renderer reads the footnote GitHub's way, with indented
/// paragraphs inside the note.
const NOTES: &str = "---\ntitle: Notes\n---\n\nSee [[notes.csv]] and ![[photo.png]].\n\nA claim.[^1]\n\n[^1]: The note.\n\n    It links [the data](data.csv) and shows ![a chart](chart.png).\n\n    ```chart\n    type: bar\n    ```\n";

#[test]
fn a_wikilink_and_a_footnote_are_scanned_as_the_renderer_reads_them() {
    let document = parse(NOTES).expect("the report parses");
    let resources: Vec<(String, &str)> = report_resources(&document)
        .into_iter()
        .map(|resource| (resource.path, resource.kind))
        .collect();
    // The wikilinks render as their text, so they name no file; the footnote's paragraph names
    // the two files the note shows. Resources come sorted by path.
    assert_eq!(
        resources,
        [
            ("chart.png".to_owned(), "media"),
            ("data.csv".to_owned(), "link")
        ]
    );
    // The fence sits inside the note, where a rich block cannot render (FEA-RP-53).
    let nested: Vec<_> = document
        .diagnostics
        .iter()
        .filter(|d| d.code == "block.nested")
        .map(|d| d.line)
        .collect();
    assert_eq!(nested, [Some(13)], "{:?}", document.diagnostics);

    let options = RenderOptions {
        report_folder: Some(ReportFolder {
            root: "notes".into(),
            source: "report.rmd".into(),
            files: [
                "report.rmd",
                "notes.csv",
                "photo.png",
                "data.csv",
                "chart.png",
            ]
            .map(String::from)
            .to_vec(),
        }),
        data: [(
            "chart.png".to_owned(),
            "data:image/png;base64,iVBORw0KGgo=".to_owned(),
        )]
        .into(),
        ..Default::default()
    };
    let report = render(NOTES, &options).expect("the report renders");
    let notes = report
        .html
        .split("<section class=\"rv-notes\"")
        .nth(1)
        .expect("the notes section");
    assert!(
        notes.contains("It links") && notes.contains("href=\"data.csv\""),
        "{notes}"
    );
    let wikilinks = report
        .diagnostics
        .iter()
        .filter(|d| d.code == "markdown.wikilink")
        .count();
    assert_eq!(wikilinks, 2, "{:?}", report.diagnostics);
}
