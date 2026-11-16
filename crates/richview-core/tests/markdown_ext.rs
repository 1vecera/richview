//! Core Markdown: smart punctuation, footnotes, math, dropped syntax, links, code, headings
//! and tables (FEA-RP-07, 08, 10..13, 15, 17, 18).

use richview_core::document::{Diagnostic, RenderOptions, ReportFolder};

fn options() -> RenderOptions {
    RenderOptions {
        report_folder: Some(ReportFolder {
            root: "md-fixture".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }),
        ..Default::default()
    }
}

fn render(body: &str) -> (String, Vec<Diagnostic>) {
    let report =
        richview_core::document::render(&format!("---\ntitle: T\n---\n\n{body}\n"), &options())
            .expect("the report renders");
    (report.html, report.diagnostics)
}

fn codes(diagnostics: &[Diagnostic], code: &str) -> Vec<Diagnostic> {
    diagnostics
        .iter()
        .filter(|d| d.code == code)
        .cloned()
        .collect()
}

#[test]
fn punctuation_is_typographic_but_not_in_code_urls_flags_or_currency() {
    let (html, _) = render(
        "He said \"hi\" -- it's 2020--2021 --- and... `a -- \"b\"...` --> $13.07bn vs ~$4.6bn, run --style or see https://example.org/a--b.",
    );
    assert!(
        html.contains("He said “hi” – it’s 2020–2021 — and…"),
        "{html}"
    );
    assert!(html.contains("<code>a -- \"b\"...</code>"));
    assert!(html.contains("--&gt;") && html.contains("run --style"));
    assert!(html.contains("$13.07bn vs ~$4.6bn"));
    assert!(html.contains("href=\"https://example.org/a--b\""), "{html}");
    assert_eq!(html.matches("<math").count(), 0);
}

#[test]
fn a_footnote_is_a_numbered_endnote_with_a_way_back() {
    let (html, diagnostics) =
        render("First[^b] then second[^a] and first again[^b].\n\n[^a]: Note A.\n\n[^b]: Note B.");
    assert!(
        html.contains(
            "<sup class=\"footnote-reference\"><a id=\"rv-fnref-1\" href=\"#rv-fn-1\">1</a></sup>"
        ),
        "{html}"
    );
    assert!(html.contains("id=\"rv-fnref-2\" href=\"#rv-fn-2\">2</a>"));
    assert!(html.contains("id=\"rv-fnref-1-2\" href=\"#rv-fn-1\">1</a>"));
    let notes = html
        .split("<section class=\"rv-notes\"")
        .nth(1)
        .expect("notes");
    assert!(
        notes.contains(
            "aria-labelledby=\"rv-notes-title\"><h2 id=\"rv-notes-title\">Notes</h2><ol>"
        )
    );
    // Numbered by first reference: B is note 1.
    assert!(notes.contains("<li id=\"rv-fn-1\">Note B. <a class=\"rv-fn-back\" href=\"#rv-fnref-1\" aria-label=\"Back to reference 1\"></a>"), "{notes}");
    assert!(notes.contains("<li id=\"rv-fn-2\">Note A."));
    assert!(!html.contains("class=\"footnote-definition\""));
    assert!(codes(&diagnostics, "markdown.footnote").is_empty());
}

#[test]
fn an_unreferenced_definition_is_dropped_and_a_dangling_reference_stays_text() {
    let (html, diagnostics) = render("Text[^missing].\n\n[^unused]: Never cited.");
    assert!(html.contains("Text[^missing]."), "{html}");
    assert!(!html.contains("Never cited") && !html.contains("<section class=\"rv-notes\""));
    let warnings = codes(&diagnostics, "markdown.footnote");
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(warnings.iter().all(|d| d.line.is_some()));
    assert!(warnings.iter().any(|d| d.message.contains("missing")));
    assert!(warnings.iter().any(|d| d.message.contains("unused")));
}

#[test]
fn no_footnotes_means_no_notes_section_and_runtime_labels_never_become_ids() {
    let (html, _) = render("Plain.");
    assert!(!html.contains("<section class=\"rv-notes\""));
    let (html, diagnostics) = render("Cite[^rv-evil].\n\n[^rv-evil]: Body.");
    assert!(!html.contains("rv-evil"), "{html}");
    assert!(html.contains("id=\"rv-fn-1\""));
    assert_eq!(codes(&diagnostics, "security.id").len(), 1);
}

#[test]
fn math_renders_and_unconvertible_tex_shows_as_code_at_its_line() {
    let (html, diagnostics) =
        render("Fine $x^2$.\n\n$$\\sum_{i=1}^n i$$\n\nBroken $\\frac{1$ here.");
    assert_eq!(html.matches("<math").count(), 2, "{html}");
    assert!(html.contains("<code class=\"rv-math-error\">"));
    let warnings = codes(&diagnostics, "markdown.math");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(warnings[0].line, Some(9), "{warnings:?}");
}

#[test]
fn an_unbalanced_dollar_span_shows_as_code_but_currency_and_escapes_stay_text() {
    let (html, diagnostics) = render("Open $x and more.\n\nCosts $5 and \\$y stay.");
    assert!(
        html.contains("<code class=\"rv-math-error\">$x</code>"),
        "{html}"
    );
    assert!(html.contains("Costs $5 and $y stay."), "{html}");
    let warnings = codes(&diagnostics, "markdown.math");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(warnings[0].line, Some(5));
}

#[test]
fn wikilinks_are_dropped_to_their_label_with_a_warning() {
    let (html, diagnostics) = render("See [[Other Report]] and [[Target|the label]].");
    assert!(html.contains("See Other Report and the label."), "{html}");
    assert!(!html.contains("<a class=\"rv-wikilink") && !html.contains("href=\"/r/"));
    let warnings = codes(&diagnostics, "markdown.wikilink");
    assert_eq!(warnings.len(), 2);
    assert!(warnings[0].message.contains("Other Report"));
    assert!(warnings[1].message.contains("Target"));
    assert_eq!(
        warnings[0].fix.as_deref(),
        Some("Link to a published URL with [text](https://…)")
    );
}

#[test]
fn a_quote_marker_is_text_and_a_block_quote_ending_in_a_name_is_the_quotation() {
    let (html, diagnostics) = render("> [!quote] Ada\n> Words.");
    assert!(html.contains("[!quote] Ada"), "{html}");
    assert!(!html.contains("<cite") && !html.contains("<p class=\"rv-pullquote"));
    let warnings = codes(&diagnostics, "markdown.quote");
    assert_eq!(warnings.len(), 1);
    assert_eq!(
        warnings[0].fix.as_deref(),
        Some("Write the quotation as a block quote and end it with — Name")
    );
    let (html, diagnostics) = render("> Words.\n>\n> — Ada");
    assert!(
        html.contains("<blockquote>") && html.contains("— Ada"),
        "{html}"
    );
    assert!(codes(&diagnostics, "markdown.quote").is_empty());
}

#[test]
fn links_open_in_the_same_tab_and_external_ones_withhold_the_opener() {
    let (html, diagnostics) = render(
        "[a](https://example.org/x) [b](mailto:a@example.org) [c](tel:+420123) [d](#top) [e](report.rmd) [f](javascript:alert(1)) [h](ftp://x.example/f)",
    );
    assert!(
        html.contains("<a href=\"https://example.org/x\" rel=\"noopener noreferrer\">a</a>"),
        "{html}"
    );
    for kept in [
        "href=\"mailto:a@example.org\"",
        "href=\"tel:+420123\"",
        "href=\"#top\"",
        "href=\"report.rmd\"",
    ] {
        assert!(html.contains(kept), "{kept}: {html}");
    }
    assert!(!html.contains("target="));
    assert_eq!(
        html.matches("rel=\"noopener noreferrer\"").count(),
        1,
        "{html}"
    );
    for dropped in ["javascript", "ftp:"] {
        assert!(!html.contains(dropped), "{dropped}: {html}");
    }
    assert_eq!(codes(&diagnostics, "security.link").len(), 2);
}

#[test]
fn bare_addresses_link_without_trailing_punctuation_and_never_inside_code_or_links() {
    let (html, _) = render(
        "See https://example.org/a, (https://example.org/b) and https://example.org/c). `https://example.org/code` [t](https://example.org/d) and <https://example.org/e>.",
    );
    assert!(html.contains("<a href=\"https://example.org/a\" rel=\"noopener noreferrer\">https://example.org/a</a>,"), "{html}");
    assert!(html.contains("(<a href=\"https://example.org/b\""));
    assert!(html.contains(">https://example.org/c</a>)."));
    assert!(html.contains("<code>https://example.org/code</code>"));
    assert_eq!(html.matches("<a ").count(), 6, "{html}");
}

#[test]
fn fenced_code_is_plain_preformatted_text_whatever_its_tag() {
    let (html, _) = render("```sql\nSELECT 1; -- \"x\"\n```\n\n```mermaid\ngraph TD\n```");
    // Each source line is one span for print (FEA-RP-17); nothing else marks the text up.
    let plain = html
        .replace("<span class=\"rv-code-line\">", "")
        .replace("</span>", "");
    assert!(
        plain.contains("<pre><code class=\"language-sql\">SELECT 1; -- \"x\"\n</code></pre>"),
        "{html}"
    );
    assert!(plain.contains("<pre><code class=\"language-mermaid\">graph TD\n</code></pre>"));
    assert!(!plain.contains("<span"));
}

#[test]
fn every_source_line_of_code_is_one_span_and_the_text_is_unchanged() {
    let source = "```python\ndef f(x):\n\n    return x  # <tag> & 'quote'\n\t\tif x:\n```\n";
    let (html, _) = render(source);
    let start = html.find("<code class=\"language-python\">").unwrap();
    let end = html[start..].find("</code>").unwrap() + start;
    let code = &html[start..end];
    assert_eq!(
        code.matches("<span class=\"rv-code-line").count(),
        4,
        "{code}"
    );
    // A blank line keeps its own span, and every newline sits inside its line's span.
    assert!(
        code.contains("<span class=\"rv-code-line\">\n</span>"),
        "{code}"
    );
    assert!(!code.contains("</span>\n"), "{code}");
    // An indented line names its indent in columns, a tab reaching the next multiple of 8, so
    // print hangs its continuations from there (FEA-RP-17).
    assert!(
        code.contains("<span class=\"rv-code-line rv-code-line--i4\">    return"),
        "{code}"
    );
    assert!(
        code.contains("<span class=\"rv-code-line rv-code-line--i16\">\t\tif"),
        "{code}"
    );
    let text = code
        .split_once('>')
        .unwrap()
        .1
        .replace("<span class=\"rv-code-line rv-code-line--i4\">", "")
        .replace("<span class=\"rv-code-line rv-code-line--i16\">", "")
        .replace("<span class=\"rv-code-line\">", "")
        .replace("</span>", "")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&amp;", "&");
    assert_eq!(
        text,
        "def f(x):\n\n    return x  # <tag> & 'quote'\n\t\tif x:\n"
    );
}

#[test]
fn a_code_indent_class_is_capped_at_24_columns() {
    let deep = format!("```text\n{}x\n```\n", " ".repeat(40));
    let (html, _) = render(&deep);
    assert!(html.contains("rv-code-line--i24\">"), "{html}");
    assert!(!html.contains("rv-code-line--i40"), "{html}");
}

fn body_headings(html: &str) -> Vec<String> {
    let body = html.split("</header>").last().unwrap_or(html);
    body.split("<h")
        .skip(1)
        .filter_map(|part| {
            let (open, rest) = part.split_once('>')?;
            let text = rest.split("</h").next()?;
            Some(format!("<h{open}>{text}"))
        })
        .collect()
}

#[test]
fn a_body_that_uses_h1_moves_every_level_down_and_h6_warns() {
    let (html, diagnostics) = render("# One\n\n## Two\n\n###### Six");
    let headings = body_headings(&html);
    assert!(
        headings.contains(&"<h2 id=\"one\">One".to_owned()),
        "{headings:?}"
    );
    assert!(headings.contains(&"<h3 id=\"two\">Two".to_owned()));
    assert!(headings.contains(&"<h6 id=\"six\">Six".to_owned()));
    let warnings = codes(&diagnostics, "markdown.heading-depth");
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].line.is_some());
}

#[test]
fn a_body_whose_top_level_is_h2_keeps_its_levels() {
    let (html, diagnostics) = render("## One\n\n### Two\n\n###### Six");
    let headings = body_headings(&html);
    assert!(
        headings.contains(&"<h2 id=\"one\">One".to_owned()),
        "{headings:?}"
    );
    assert!(headings.contains(&"<h3 id=\"two\">Two".to_owned()));
    assert!(headings.contains(&"<h6 id=\"six\">Six".to_owned()));
    assert!(codes(&diagnostics, "markdown.heading-depth").is_empty());
    assert_eq!(html.matches("<h1").count(), 1, "the title is the only h1");
}

#[test]
fn heading_ids_are_ascii_slugs_numbered_when_repeated_and_clear_of_the_runtime() {
    let (html, _) = render(
        "## Průměrná cena\n\n## Same\n\n## Same\n\n## Same\n\n## Rvachev\n\n## Panel-one\n\n## ??\n\n## Straße & Œuvre",
    );
    for id in [
        "id=\"prumerna-cena\"",
        "id=\"same\"",
        "id=\"same-2\"",
        "id=\"same-3\"",
        "id=\"s-rvachev\"",
        "id=\"s-panel-one\"",
        "id=\"section\"",
        "id=\"strasse-oeuvre\"",
    ] {
        assert!(html.contains(id), "{id}: {html}");
    }
    assert!(!html.contains("class=\"anchor\"") && !html.contains('¶'));
}

#[test]
fn tables_scroll_in_their_own_focusable_box_and_keep_alignment_classes() {
    let (html, _) = render(
        "| A | B | C |\n|:--|--:|:-:|\n| x | 1 | y |\n\n```callout\nkind: neutral\nbody: \"| P | Q |\\n|---|---|\\n| 1 | 2 |\"\n```",
    );
    let wrapper = "<div class=\"rv-table-scroll\"><table>";
    assert_eq!(html.matches(wrapper).count(), 2, "{html}");
    assert!(html.contains(
        "<th scope=\"col\" class=\"num\">B</th><th scope=\"col\" class=\"center\">C</th>"
    ));
    assert!(html.contains("</table>\n</div>"));
}

#[test]
fn repeated_markdown_table_headers_warn_without_changing_the_table() {
    for header in [
        "| Category | Gap pp | Gap pp |",
        "| Category | **Gap pp** | [Gap pp](https://example.org/gap) |",
        "| Category | `Gap pp` | Gap   pp |",
    ] {
        let body = format!("{header}\n|---|---:|---:|\n| A | 7 | 9 |\n");
        let (html, diagnostics) = render(&body);
        let warnings = codes(&diagnostics, "markdown.table-header");
        assert_eq!(warnings.len(), 1, "{header}: {diagnostics:?}");
        let warning = &warnings[0];
        assert_eq!(warning.level, "warning");
        assert_eq!(warning.path, "markdown");
        assert_eq!(warning.tag.as_deref(), Some("markdown"));
        assert_eq!(warning.line, Some(5));
        assert!(
            warning.message.contains("Gap pp") && warning.message.contains("column 3"),
            "{warning:?}"
        );
        assert!(
            warning
                .fix
                .as_deref()
                .is_some_and(|fix| fix.contains("distinct"))
        );
        assert_eq!(html.matches("<th scope=\"col\"").count(), 3, "{header}");
        assert!(
            html.contains("<td>A</td><td class=\"num\">7</td><td class=\"num\">9</td>"),
            "{header}"
        );
    }
    let checked = richview_core::document::check(
        "---\ntitle: T\n---\n\n| Gap pp | Gap pp |\n|---|---|\n| 1 | 2 |\n",
        &options(),
    )
    .unwrap();
    let warnings = codes(&checked.diagnostics, "markdown.table-header");
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].line, Some(5));
    for body in [
        "| A | B |\n|---|---|\n| A | A |\n\n| A | B |\n|---|---|\n| B | B |",
        "| | |\n|---|---|\n| 1 | 2 |",
        "| Gap pp | gap pp |\n|---|---|\n| 1 | 2 |",
        "| Score $x$ | Score $y$ |\n|---|---|\n| 1 | 2 |",
        "| Score $x$ | Score `x` |\n|---|---|\n| 1 | 2 |",
        "```text\n| Gap pp | Gap pp |\n|---|---|\n| 1 | 2 |\n```",
    ] {
        let (_, diagnostics) = render(body);
        assert!(
            codes(&diagnostics, "markdown.table-header").is_empty(),
            "{body}: {diagnostics:?}"
        );
    }
    let (_, diagnostics) = render("| Gap pp | Gap pp | Gap pp |\n|---|---|---|\n| 1 | 2 | 3 |");
    assert_eq!(
        codes(&diagnostics, "markdown.table-header").len(),
        2,
        "{diagnostics:?}"
    );
}

#[test]
fn gfm_strikethrough_task_lists_and_definition_lists_render() {
    let (html, _) = render("~~gone~~\n\n- [x] done\n- [ ] open\n\nTerm\n: Meaning");
    assert!(html.contains("<del>gone</del>"));
    assert_eq!(html.matches("<input ").count(), 2);
    assert!(html.contains("disabled"));
    assert!(
        html.contains("<dl>")
            && html.contains("<dt>Term</dt>")
            && html.contains("<dd>Meaning</dd>")
    );
}

#[test]
fn unsupported_tex_commands_show_as_code_with_a_warning_and_never_as_parse_errors() {
    let (html, diagnostics) = render(
        "$$\\href{javascript:alert(1)}{x}$$\n\n$\\style{color:red}{y}$\n\n$$\\sum_{i=1}^n i$$",
    );
    assert!(!html.contains("PARSE ERROR"), "{html}");
    assert_eq!(
        codes(&diagnostics, "markdown.math").len(),
        2,
        "{diagnostics:?}"
    );
    // A good display formula sits in a scroll box, and its `<math>` keeps its role.
    assert!(
        html.contains("<span class=\"rv-math\"><math"),
        "{}",
        html.find("<math")
            .map_or("", |i| &html[i.saturating_sub(80)..i + 40])
    );
}

#[test]
fn every_markdown_finding_sits_at_its_construct_by_line_and_column() {
    // TYP-56: columns count characters, and the currency escape, inline HTML, a quote's `>` and
    // a run of text never move a finding off its construct.
    let (_, diagnostics) = render(concat!(
        "Some <b>bold</b> and a [bad](javascript:alert) link.\n\n",
        "Costs $13.07bn vs ~$4.6bn and <i>x</i> here.\n\n",
        "Café <u>x</u>, see [[Other]] and a note[^gone].\n\n",
        "Broken $\\frac{1$ here.\n\n",
        "> <div>\n> <span>x</span>\n> </div>\n\n",
        "```callout\nkind: neutral\ntitle: Note\nbody: \"Some <b>x</b> here\"\n```",
    ));
    let placed: Vec<(&str, Option<usize>, Option<usize>)> = diagnostics
        .iter()
        .filter(|d| d.code.starts_with("security.") || d.code.starts_with("markdown."))
        .map(|d| (d.code.as_str(), d.line, d.column))
        .collect();
    assert_eq!(
        placed,
        [
            // A YAML field's Markdown is placed at its field; blocks render first.
            ("security.html", Some(20), Some(1)),
            ("security.html", Some(5), Some(6)),
            ("security.link", Some(5), Some(24)),
            ("security.html", Some(7), Some(31)),
            ("security.html", Some(9), Some(6)),
            ("markdown.wikilink", Some(9), Some(20)),
            ("markdown.footnote", Some(9), Some(40)),
            ("markdown.math", Some(11), Some(8)),
            ("security.html", Some(13), Some(3)),
            ("security.html", Some(14), Some(3)),
        ],
        "{diagnostics:#?}"
    );
}
