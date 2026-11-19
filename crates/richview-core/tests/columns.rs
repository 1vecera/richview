//! Columns: two to four columns side by side, written as `::: columns` and `::: column` fenced
//! divs (FEA-RP-68, assumed). Malformed columns warn and render as before, in one column.

use richview_core::document::{Diagnostic, RenderOptions, RenderedReport, ReportFolder};

fn render(source: &str) -> RenderedReport {
    let options = RenderOptions {
        report_folder: Some(ReportFolder {
            root: "columns-fixture".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }),
        ..RenderOptions::default()
    };
    richview_core::document::render(source, &options).unwrap()
}

fn body(report: &RenderedReport) -> &str {
    let html = &report.html;
    let start = html.find("<main").unwrap();
    &html[start..html.find("</main>").unwrap()]
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<&str> {
    diagnostics
        .iter()
        .map(|d| d.code.as_str())
        .filter(|code| code.starts_with("columns.") || code.starts_with("block."))
        .collect()
}

fn finding<'a>(report: &'a RenderedReport, code: &str) -> &'a Diagnostic {
    report
        .diagnostics
        .iter()
        .find(|d| d.code == code)
        .unwrap_or_else(|| panic!("no {code} in {:?}", codes(&report.diagnostics)))
}

const CHART: &str = "```chart\ntype: bar\ntitle: Orders by month\ndata:\n  inline:\n    - {month: Jan, orders: 10}\n    - {month: Feb, orders: 14}\nencoding: {x: month, y: orders}\n```\n";

const METRIC: &str = "```metric\nvalue: 1240\nlabel: Orders in launch week\n```\n";

fn two(options: &str, first: &str, second: &str) -> String {
    format!(
        "---\ntitle: Columns\n---\n\nBefore.\n\n::: columns{options}\n\n::: column\n{first}\n:::\n\n::: column\n{second}\n:::\n\n:::\n\nAfter.\n"
    )
}

#[test]
fn two_columns_hold_prose_and_blocks_in_source_order() {
    let report = render(&two(
        "",
        CHART,
        "The launch week alone brought **1,240** orders.",
    ));
    let b = body(&report);
    assert_eq!(codes(&report.diagnostics), Vec::<&str>::new());
    let open = b
        .find("<div class=\"rv-columns rv-columns--wide\"")
        .expect(b);
    let chart = b.find("<figure class=\"rv-chart\"").unwrap();
    let prose = b.find("<strong>1,240</strong>").unwrap();
    let after = b.find("<p>After.</p>").unwrap();
    // Source order is reading order: the columns' boxes reorder nothing.
    assert!(
        b.find("<p>Before.</p>").unwrap() < open && open < chart && chart < prose && prose < after
    );
    assert_eq!(b.matches("<div class=\"rv-column\">").count(), 2);
    // A chart's floor, twice over for its equal share: the columns stack under two phone
    // columns and a gap.
    assert!(
        b.contains("style=\"--rv-columns-at:44.75rem;--rv-columns-n:2\""),
        "{b}"
    );
    // No marker line reaches the page as text, and no placeholder as a comment.
    assert!(!b.contains(":::") && !b.contains("richview:columns"), "{b}");
}

#[test]
fn options_set_closed_classes_and_weights() {
    let report = render(&two(
        " {widths: 2-1, gap: large, align: center, wide: false}",
        CHART,
        "A note.",
    ));
    let b = body(&report);
    assert_eq!(codes(&report.diagnostics), Vec::<&str>::new());
    assert!(
        b.contains("<div class=\"rv-columns rv-columns--measure rv-columns--gap-large rv-columns--center\" style=\"--rv-columns-at:33.57rem;--rv-columns-n:2\">"),
        "{b}"
    );
    assert!(b.contains("<div class=\"rv-column rv-column--2\">"));
}

#[test]
fn a_chart_is_laid_out_at_its_columns_width() {
    let alone = render(&format!("---\ntitle: One\n---\n\n{CHART}"));
    let paired = render(&two("", CHART, CHART));
    let ratio = |report: &RenderedReport| {
        let b = body(report).to_owned();
        let at = b.find("--rv-ratio:").unwrap() + "--rv-ratio:".len();
        b[at..at + b[at..].find(';').unwrap()]
            .parse::<f64>()
            .unwrap()
    };
    // A narrower desktop box is a taller one: the payload was laid out for the column.
    assert!(
        ratio(&paired) < ratio(&alone),
        "{} {}",
        ratio(&paired),
        ratio(&alone)
    );
}

#[test]
fn malformed_columns_warn_and_render_exactly_as_before() {
    // The same source with every marker renamed to a div RichView never reads, so its lines
    // render as they did before columns existed (a `:::` line after a paragraph is a definition).
    let plain = |source: &str| {
        let report = render(&source.replace("column", "colxmn"));
        body(&report).replace("colxmn", "column")
    };
    for (source, code) in [
        // One column.
        (
            "::: columns\n::: column\nOnly.\n:::\n:::\n".to_owned(),
            "columns.count",
        ),
        // Five columns.
        (
            format!("::: columns\n{}:::\n", "::: column\nText.\n:::\n".repeat(5)),
            "columns.count",
        ),
        // An empty column.
        (
            "::: columns\n::: column\nA.\n:::\n::: column\n\n:::\n:::\n".to_owned(),
            "columns.empty",
        ),
        // Columns in a column.
        (
            "::: columns\n::: column\n::: columns\n::: column\nA\n:::\n::: column\nB\n:::\n:::\n:::\n::: column\nC\n:::\n:::\n".to_owned(),
            "columns.nested",
        ),
        // A section heading in a column.
        (
            "::: columns\n::: column\n## Section\n:::\n::: column\nB\n:::\n:::\n".to_owned(),
            "columns.heading",
        ),
        // No closing marker.
        (
            "::: columns\n::: column\nA\n:::\n::: column\nB\n:::\n".to_owned(),
            "columns.unclosed",
        ),
        // Text between the columns.
        (
            "::: columns\nLoose.\n::: column\nA\n:::\n::: column\nB\n:::\n:::\n".to_owned(),
            "columns.stray",
        ),
        // A column without columns.
        ("::: column\nA\n:::\n".to_owned(), "columns.stray"),
    ] {
        let source = format!("---\ntitle: Bad\n---\n\n{source}");
        let report = render(&source);
        let found = finding(&report, code);
        assert_eq!(found.level, "warning", "{code}");
        assert!(found.line.is_some() && found.fix.is_some(), "{code}");
        let b = body(&report);
        assert!(!b.contains("rv-column"), "{code}: {b}");
        // The page is the one the source made before columns existed: the markers are text.
        assert!(b.contains(":::"), "{code}: {b}");
        assert_eq!(b, plain(&source), "{code}");
    }
}

#[test]
fn other_fenced_divs_and_stray_closers_stay_text_without_a_warning() {
    let report = render("---\ntitle: Divs\n---\n\n::: callout\nNote\n:::\n\n:::\n");
    assert_eq!(codes(&report.diagnostics), Vec::<&str>::new());
    let b = body(&report);
    assert!(b.contains("::: callout") && !b.contains("rv-column"), "{b}");
}

#[test]
fn markers_in_code_lists_and_quotes_are_not_markers() {
    let report = render(
        "---\ntitle: Code\n---\n\n```\n::: columns\n::: column\nA\n:::\n::: column\nB\n:::\n:::\n```\n\n- ::: columns\n\n> ::: column\n",
    );
    assert_eq!(codes(&report.diagnostics), Vec::<&str>::new());
    assert!(!body(&report).contains("rv-column"));
}

#[test]
fn widths_that_do_not_match_warn_and_go_equal() {
    let report = render(&two(" {widths: 2-1-1}", "A.", "B."));
    assert_eq!(finding(&report, "columns.widths").level, "warning");
    let b = body(&report);
    assert_eq!(b.matches("<div class=\"rv-column\">").count(), 2, "{b}");
}

#[test]
fn unknown_and_refused_options_follow_the_block_rules() {
    let report = render(&two(" {widths: 2-1, colour: red}", "A.", "B."));
    let unknown = finding(&report, "block.unknown-key");
    assert_eq!(unknown.level, "warning");
    assert!(
        body(&report).contains("rv-column--2"),
        "the rest still applies"
    );

    // A word outside the closed vocabulary is refused, as a block's is: the columns stay text.
    let report = render(&two(" {widths: 70%}", "A.", "B."));
    assert_eq!(finding(&report, "block.schema").level, "error");
    assert!(!body(&report).contains("rv-column"));

    let report = render(&two(" not a mapping", "A.", "B."));
    assert_eq!(finding(&report, "columns.options").level, "warning");
    assert!(body(&report).contains("rv-column"), "defaults apply");
}

#[test]
fn a_scrolly_block_cannot_sit_in_a_column() {
    let scrolly = "```scrolly\nimage: {src: \"data:image/png;base64,iVBORw0KGgo=\", alt: x}\nsteps:\n  - {text: One}\n```\n";
    let report = render(&two("", scrolly, "B."));
    assert_eq!(finding(&report, "columns.block").level, "warning");
    assert!(!body(&report).contains("rv-column"));
}

#[test]
fn metrics_in_columns_share_one_top() {
    let report = render(&format!(
        "---\ntitle: Metrics\n---\n\n::: columns {{wide: false}}\n::: column\n{METRIC}:::\n::: column\n{METRIC}:::\n::: column\n{METRIC}:::\n:::\n"
    ));
    assert_eq!(codes(&report.diagnostics), Vec::<&str>::new());
    let b = body(&report);
    assert_eq!(b.matches("rv-metric__value").count(), 3, "{b}");
    // Three prose floors, no chart: they stand side by side on the measure.
    assert!(
        b.contains("--rv-columns-at:33.00rem;--rv-columns-n:3"),
        "{b}"
    );
}

#[test]
fn a_typed_placeholder_is_text_not_a_column() {
    let report = render(
        "---\ntitle: Hostile\n---\n\n<!--richview:columns:s-->\n\nText <!--richview:columns:o:1-1:medium:top:wide:100-->\n",
    );
    let b = body(&report);
    assert!(
        !b.contains("rv-column") && !b.contains("richview:columns"),
        "{b}"
    );
}

#[test]
fn a_source_note_stays_with_its_block_in_a_column() {
    let report = render(&two(
        "",
        &format!("{CHART}\n*Source: order system.*\n"),
        "B.",
    ));
    let b = body(&report);
    let column = b.find("<div class=\"rv-column\">").unwrap();
    let note = b.find("<p class=\"rv-source\">").expect(b);
    let next = b[column + 1..].find("<div class=\"rv-column\">").unwrap() + column + 1;
    assert!(column < note && note < next, "{b}");
}

#[test]
fn a_closing_marker_under_a_quote_or_a_list_closes_the_column() {
    let report = render(&two("", "> A quote.\n> — Its author", "- one\n- two"));
    assert_eq!(codes(&report.diagnostics), Vec::<&str>::new());
    let b = body(&report);
    assert_eq!(b.matches("<div class=\"rv-column\">").count(), 2, "{b}");
    assert!(!b.contains(":::"), "{b}");
}

#[test]
fn a_stray_marker_after_a_paragraph_stays_the_definition_it_always_was() {
    let source = "---\ntitle: Old\n---\n\nTerm\n::: callout\n\nText\n:::\n";
    let report = render(source);
    assert_eq!(codes(&report.diagnostics), Vec::<&str>::new());
    assert!(body(&report).contains("<dd>"), "{}", body(&report));
}

#[test]
fn expands_and_code_blocks_sit_in_columns() {
    let expand = "```expand\ntitle: How the sample was drawn\nbody: Twelve weeks of orders.\n```\n";
    let code = "```code\nlang: sql\ntext: |\n  select 1\n```\n";
    let report = render(&two("", expand, code));
    assert_eq!(codes(&report.diagnostics), Vec::<&str>::new());
    let b = body(&report);
    let first = b.find("<div class=\"rv-column\">").unwrap();
    assert!(
        b[first..].contains("<details") && b[first..].contains("select"),
        "{b}"
    );
    // Code needs a chart's width before the columns stack.
    assert!(b.contains("--rv-columns-at:44.75rem"), "{b}");
}

#[test]
fn a_fold_holds_whole_columns_and_a_heading_in_a_column_never_starts_one() {
    let source = "---\ntitle: Folds\ncollapsed_sections: [details, in-a-column]\n---\n\n## Details\n\n::: columns\n::: column\n### In a column\n\nA.\n:::\n::: column\nB.\n:::\n:::\n\n## Next\n\nC.\n";
    let report = render(source);
    let b = body(&report);
    let fold = b.find("<details").expect(b);
    let columns = b.find("<div class=\"rv-columns").unwrap();
    let end = b.find("</details>").unwrap();
    let close = b.rfind("</div></div>").unwrap_or(0);
    assert!(fold < columns && columns < end, "{b}");
    assert!(
        b[columns..end].matches("<div").count() == b[columns..end].matches("</div>").count(),
        "{b}"
    );
    let _ = close;
    // The heading in a column names no section, so its entry warns and stays open.
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "section.anchor-missing" && d.message.contains("in-a-column")),
        "{:?}",
        report.diagnostics
    );
}

#[test]
fn columns_in_a_folded_last_section_close_with_the_fold_before_the_notes() {
    let source = "---\ntitle: Folds\ncollapsed_sections: [last]\n---\n\n## First\n\nA.\n\n## Last\n\n::: columns\n::: column\nLeft, with a note.[^1]\n:::\n::: column\nRight.\n:::\n:::\n\nAfter the columns.\n\n[^1]: The note.\n";
    let report = render(source);
    let b = body(&report);
    let fold = b.find("<details class=\"rv-section\"").expect(b);
    let columns = b.find("<div class=\"rv-columns").expect(b);
    let close = b[columns..].find("</details>").expect(b) + columns;
    let notes = b.rfind("<section class=\"rv-notes\"").expect(b);
    // The fold holds the whole columns box and closes before the notes.
    assert!(fold < columns && columns < close && close < notes, "{b}");
    assert_eq!(
        b[columns..close].matches("<div").count(),
        b[columns..close].matches("</div>").count(),
        "{b}"
    );
    assert!(b[columns..close].contains("After the columns."), "{b}");
    assert!(b[notes..].contains("The note."), "{b}");
}

#[test]
fn a_chart_in_columns_reserves_its_stacked_box_between_a_phone_and_the_stack_width() {
    let report = render(&two("", CHART, CHART));
    let b = body(&report);
    assert_eq!(b.matches(";--rv-ratio-stacked:").count(), 2, "{b}");
    // The two charts' window: from the phone layout up to where the columns stand side by side.
    let rules = report
        .html
        .matches("@media screen and (min-width:601px) and (max-width:")
        .count();
    assert_eq!(rules, 2);
    assert!(
        report
            .html
            .contains("#rv-chart-0 .rv-chart__frame{aspect-ratio:var(--rv-ratio-stacked")
    );

    // A chart outside columns keeps its two boxes and the report no rule.
    let alone = render(&format!("---\ntitle: One\n---\n\n{CHART}"));
    assert!(!alone.html.contains("--rv-ratio-stacked") && !alone.html.contains("rv-columns"));
}
