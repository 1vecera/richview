/// The source operations live in their own pure crate, which the site's Wasm shares (S27-931);
/// the editor's paths to them stay as they were.
pub use richview_authoring as authoring;
pub mod lsp;

use serde_json::{Value, json};

include!(concat!(env!("OUT_DIR"), "/doc_examples.rs"));

/// A chart type's snippet: the worked example of its documentation page, in VS Code snippet
/// syntax with the title as the first tab stop.
fn chart_snippet(kind: &str) -> Option<String> {
    let example = DOC_EXAMPLES
        .iter()
        .find_map(|(name, body)| (*name == kind).then_some(*body))?;
    let escape = |text: &str| {
        text.replace('\\', "\\\\")
            .replace('$', "\\$")
            .replace('}', "\\}")
    };
    Some(
        example
            .lines()
            .map(|line| match line.strip_prefix("title: ") {
                Some(title) => format!("title: ${{1:{}}}", escape(title)),
                None => escape(line),
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n",
    )
}

/// The VS Code snippet file the extension ships (`packages/vscode/snippets/rmd.json`): every
/// snippet of [`snippets`] under its label, plus the frontmatter. The build writes it and a test
/// compares it with the committed file.
pub fn vscode_snippet_file() -> String {
    let mut catalog = serde_json::Map::new();
    for snippet in snippets().as_array().into_iter().flatten() {
        let (Some(id), Some(label), Some(body)) = (
            snippet["id"].as_str(),
            snippet["label"].as_str(),
            snippet["body"].as_str(),
        ) else {
            continue;
        };
        let prefix = if richview_core::CHART_TYPES.contains(&id) {
            format!("rv-chart-{id}")
        } else {
            format!("rv-{id}")
        };
        catalog.insert(
            label.into(),
            json!({"prefix":prefix,"body":body.lines().collect::<Vec<_>>(),"description":label}),
        );
    }
    catalog.insert("Frontmatter".into(), json!({"prefix":"rv-frontmatter","body":["---","title: ${1:My report}","description: ${2:What the evidence shows}","version: 0.0.1","styles: [scientific]","---","","$0"],"description":"RichView report metadata"}));
    serde_json::to_string_pretty(&catalog).expect("snippets serialize")
}

pub fn snippets() -> Value {
    let mut items = Vec::new();
    for &kind in richview_core::CHART_TYPES {
        // A type without a documentation example has no snippet: the docs gate requires one.
        let Some(body) = chart_snippet(kind) else {
            continue;
        };
        items.push(
            json!({"id":kind,"label":kind,"description":format!("{kind} chart"),"body":body}),
        );
    }
    for (id, label, body) in [
        (
            "table",
            "Table",
            "```table\ndata:\n  inline:\n    - month: Jan\n      value: 100\n    - month: Feb\n      value: 140\ncolumns:\n  - key: month\n    label: Month\n  - key: value\n    label: Value\n    align: right\n```\n",
        ),
        (
            "metric",
            "Metric",
            "```metric\nvalue: ${1:1234}\nlabel: ${2:Visits today}\ndelta: ${3:0.12}\ntrend: ${4|up,down,flat|}\n```\n",
        ),
        (
            "callout",
            "Callout",
            "```callout\nkind: ${1|up,down,neutral,hold|}\ntitle: ${2:Title}\nbody: |\n  ${3:Body}\n```\n",
        ),
        (
            "expand",
            "Expand",
            "```expand\ntitle: ${1:How the sample was drawn}\nbody: |\n  ${2:The detail a reader opens when they want it.}\n```\n",
        ),
        (
            "code",
            "Code",
            "```code\ntitle: ${1:query.sql}\nlang: ${2:sql}\ntext: |\n  ${3:select 1}\n```\n",
        ),
        (
            "image",
            "Image",
            "```image\nsrc: ${1:media/photo.png}\nalt: ${2:What the picture shows}\ncaption: ${3:Caption}\n```\n",
        ),
        (
            "audio",
            "Audio",
            "```audio\nsrc: ${1:media/interview.mp3}\ntitle: ${2:Interview with the maker}\ncaption: ${3:Caption}\n```\n",
        ),
        (
            "video",
            "Video",
            "```video\nsrc: ${1:media/demo.mp4}\ntitle: ${2:Product demo}\ncaption: ${3:Caption}\n```\n",
        ),
        (
            "embed",
            "Embed",
            "```embed\nurl: ${1:https://www.youtube.com/embed/dQw4w9WgXcQ}\ncaption: ${2:Caption}\n```\n",
        ),
        (
            "columns",
            "Columns",
            "::: columns {widths: ${1|equal,2-1,1-2,1-1-1|}}\n\n::: column\n${2:The first column: prose or a block.}\n:::\n\n::: column\n${3:The second column.}\n:::\n\n:::\n",
        ),
        (
            "scrolly",
            "Scrollytelling",
            "```scrolly\ntitle: ${1:How it changed}\nimage:\n  src: ${2:media/photo.png}\n  alt: ${3:What the picture shows}\nsteps:\n  - text: ${4:The first thing to notice.}\n  - text: ${5:Then the second.}\n```\n",
        ),
    ] {
        items.push(json!({"id":id,"label":label,"body":body}));
    }
    Value::Array(items)
}
