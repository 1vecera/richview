//! The `expand` block: a titled part of the report the reader opens, readable without scripts
//! and open on paper; and the `code` block.
use richview_core::document::{Output, RenderOptions, ReportFolder, render};

fn rendered(source: &str, output: Output) -> richview_core::document::RenderedReport {
    render(
        source,
        &RenderOptions {
            output,
            report_folder: Some(ReportFolder {
                root: "report".into(),
                source: "report.rmd".into(),
                files: vec!["report.rmd".into()],
            }),
            ..Default::default()
        },
    )
    .expect("report renders")
}

fn report(block: &str) -> String {
    format!("---\ntitle: Report\n---\n\n## Part\n\n{block}\n")
}

#[test]
fn an_expand_is_a_native_disclosure_closed_on_screen_and_open_on_paper() {
    let source = report(
        "```expand\ntitle: How the sample was drawn\nbody: |\n  We drew **1,200** orders.\n\n  - one\n  - two\n```",
    );
    let screen = rendered(&source, Output::Report);
    assert!(
        screen.diagnostics.iter().all(|d| d.level != "error"),
        "{:?}",
        screen.diagnostics
    );
    let html = &screen.html;
    let start = html
        .find("<details class=\"rv-expand\"><summary class=\"rv-expand__title\">How the sample was drawn</summary>")
        .expect("a closed expand with its title");
    let end = html[start..].find("</details>").expect("it ends") + start;
    assert!(html[start..end].contains("<strong>1,200</strong>"));
    assert!(html[start..end].contains("<li>"));
    let paper = rendered(&source, Output::Pdf).html;
    assert!(paper.contains("<details class=\"rv-expand\" open>"));
}

#[test]
fn an_expand_may_start_open() {
    let html = rendered(
        &report("```expand\ntitle: Columns\nopen: true\nbody: The columns.\n```"),
        Output::Report,
    )
    .html;
    assert!(html.contains(
        "<details class=\"rv-expand\" open><summary class=\"rv-expand__title\">Columns</summary>"
    ));
}

#[test]
fn an_expand_title_is_text_and_its_body_keeps_the_markdown_rules() {
    let out = rendered(
        &report(
            "```expand\ntitle: \"<b>Bold</b> & <script>x</script>\"\nbody: |\n  <script>alert(1)</script> Safe text.\n```",
        ),
        Output::Report,
    );
    assert!(
        out.html
            .contains("&lt;b&gt;Bold&lt;/b&gt; &amp; &lt;script&gt;x&lt;/script&gt;</summary>")
    );
    assert!(!out.html.contains("<script>alert(1)"));
    assert!(out.html.contains("Safe text."));
}

#[test]
fn an_expand_needs_a_title_and_a_body() {
    for (block, path) in [
        ("```expand\nbody: Text.\n```", "expand.title"),
        ("```expand\ntitle: Empty\nbody: \"  \"\n```", "expand.body"),
        ("```expand\ntitle: \"\"\nbody: Text.\n```", "expand.title"),
    ] {
        let out = rendered(&report(block), Output::Report);
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.level == "error" && d.code == "block.schema" && d.path == path),
            "{block}: {:?}",
            out.diagnostics
        );
    }
}

#[test]
fn a_rich_block_inside_an_expand_shows_a_notice_and_an_error() {
    let source = report(
        "````expand\ntitle: Inside\nbody: |\n  Before.\n\n  ```metric\n  value: 42\n  label: Answer\n  ```\n````",
    );
    let out = rendered(&source, Output::Report);
    let nested = out
        .diagnostics
        .iter()
        .find(|d| d.code == "block.nested")
        .expect("the nested block is refused");
    assert_eq!(nested.path, "expand.body");
    assert!(nested.message.contains("inside an expand"));
    assert!(out.html.contains("Before."));
    assert!(!out.html.contains("class=\"rv-metric__value\""));
}

#[test]
fn a_code_language_names_a_class_only_when_it_is_one_plain_token() {
    // Source text never names a class of the renderer's own: a `lang` with a space gets no
    // class, and its label still shows what the author wrote.
    let out = rendered(
        &report("```code\nlang: sh rv-chart--full\ntext: ls\n```"),
        Output::Report,
    )
    .html;
    assert!(out.contains("<span class=\"rv-code__lang\">sh rv-chart--full</span>"));
    assert!(out.contains("<pre><code>"), "no class on the code");
    assert!(!out.contains("language-sh"));
    let plain = rendered(
        &report("```code\nlang: C++\ntext: int x;\n```"),
        Output::Report,
    )
    .html;
    assert!(plain.contains("<code class=\"language-c++\">"));
}

#[test]
fn a_code_block_has_a_head_numbered_lines_and_marks() {
    let out = rendered(
        &report(
            "```code\ntitle: orders.sql\nlang: sql\nlines: true\nmark: [2, \"4-5\"]\ntext: |\n  select 1\n  from orders\n  where x < 2\n  group by a\n  order by b\n```",
        ),
        Output::Report,
    );
    assert!(
        out.diagnostics.iter().all(|d| d.level != "error"),
        "{:?}",
        out.diagnostics
    );
    let html = &out.html;
    assert!(html.contains("<figure class=\"rv-code rv-code--lines\"><figcaption class=\"rv-code__head\"><span class=\"rv-code__title\">orders.sql</span><span class=\"rv-code__lang\">sql</span></figcaption><pre><code class=\"language-sql\">"));
    assert_eq!(
        html.matches("class=\"rv-code-line rv-code-line--mark\"")
            .count(),
        3
    );
    assert!(html.contains("<span class=\"rv-code-line rv-code-line--mark\">from orders\n</span>"));
    // Code is text: the comparison is escaped, nothing is run.
    assert!(html.contains("where x &lt; 2\n"));
    assert_eq!(html.matches("class=\"rv-code-line").count(), 5);
}

#[test]
fn code_in_a_known_language_is_highlighted_line_by_line() {
    let html = rendered(
        &report("```code\nlang: yaml\ntext: |\n  title: |\n    two\n    lines\n  # note\n```"),
        Output::Report,
    )
    .html;
    assert!(html.contains("<span class=\"hl-key\">title</span>"));
    assert!(html.contains("<span class=\"hl-comment\"># note</span>"));
    // Every line closes what it opened, so a line is a whole box.
    let start = html.find("<pre><code").unwrap();
    let end = html[start..].find("</code></pre>").unwrap() + start;
    for line in html[start..end].split("<span class=\"rv-code-line").skip(1) {
        let line = &line[..line.rfind("</span>").unwrap()];
        assert_eq!(
            line.matches("<span").count(),
            line.matches("</span>").count(),
            "{line}"
        );
    }
}

#[test]
fn an_ordinary_fence_in_a_known_language_is_highlighted_too() {
    let html = rendered(
        &report("```sh\nrichview build --format json # check\n```"),
        Output::Report,
    )
    .html;
    assert!(
        html.contains("<span class=\"hl-cmd\">richview</span>"),
        "{html}"
    );
    let plain = rendered(&report("```sql\nselect 1\n```"), Output::Report).html;
    assert!(plain.contains("<span class=\"rv-code-line\">select 1\n</span>"));
}

#[test]
fn a_code_block_takes_text_or_a_file_and_marks_from_its_lines() {
    for (block, path) in [
        ("```code\nlang: sql\n```", "code.text"),
        ("```code\ntext: a\nsrc: a.sql\n```", "code.text"),
        ("```code\ntext: a\nmark: [0]\n```", "code.mark"),
        ("```code\ntext: a\nmark: [\"3-1\"]\n```", "code.mark"),
    ] {
        let out = rendered(&report(block), Output::Report);
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.level == "error" && d.path == path),
            "{block}: {:?}",
            out.diagnostics
        );
    }
    let out = rendered(
        &report("```code\ntext: |\n  one\n  two\nmark: [3]\n```"),
        Output::Report,
    );
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.level == "warning" && d.path == "code.mark")
    );
}

#[test]
fn highlighted_code_asks_bold_and_italic_only_from_faces_its_style_embeds() {
    let block = "```code\nlang: yaml\ntext: |\n  type: bar  # Q4\n```";
    for style in richview_core::builtin_style_names() {
        let source = format!("---\ntitle: Report\nstyles: [{style}]\n---\n\n## Part\n\n{block}\n");
        let html = rendered(&source, Output::Report).html;
        assert!(
            html.contains("class=\"hl-key\""),
            "{style}: the key is highlighted"
        );
        let mono = html
            .split_once("--rv-font-mono:")
            .and_then(|(_, rest)| rest.split([',', ';', '}']).next())
            .map(|family| family.trim().trim_matches(['\'', '"']).to_owned())
            .expect("the style names a code face");
        let faces: Vec<&str> = html
            .split("@font-face")
            .skip(1)
            .map(|face| face.split('}').next().unwrap_or(""))
            .filter(|face| face.contains(&format!("font-family: \"{mono}\"")))
            .collect();
        assert!(!faces.is_empty(), "{style}: the report embeds {mono}");
        let bold = faces.iter().any(|face| {
            face.split_once("font-weight:")
                .and_then(|(_, rest)| rest.split(';').next())
                .and_then(|weight| weight.trim().parse::<u32>().ok())
                .is_some_and(|weight| weight >= 600)
        });
        let italic = faces.iter().any(|face| face.contains("font-style: italic"));
        assert_eq!(
            html.contains(":is(.hl-key,.hl-cmd,.hl-tag,.hl-heading){font-weight:600}"),
            bold,
            "{style}: {mono} bold"
        );
        assert_eq!(
            html.contains(".hl-comment{font-style:italic}"),
            italic,
            "{style}: {mono} italic"
        );
    }
}
