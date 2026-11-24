//! Table, metric, callout, media and embed markup (FEA-RP-19..28, 55, 56).

use richview_core::document::{Diagnostic, RenderOptions, RenderedReport, ReportFolder};

fn render_with(source: &str, options: &RenderOptions) -> RenderedReport {
    let mut options = options.clone();
    options.report_folder = Some(ReportFolder {
        root: "blocks-fixture".into(),
        source: "report.rmd".into(),
        files: std::iter::once("report.rmd".into())
            .chain(options.data.keys().cloned())
            .collect(),
    });
    richview_core::document::render(source, &options).unwrap()
}

fn render(source: &str) -> RenderedReport {
    render_with(source, &RenderOptions::default())
}

fn body(report: &RenderedReport) -> &str {
    let html = &report.html;
    let start = html.find("<main").unwrap();
    &html[start..html.find("</main>").unwrap()]
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<&str> {
    diagnostics.iter().map(|d| d.code.as_str()).collect()
}

const ROWS: &str = "data:\n  inline:\n    - {name: a, units: 1200, rate: 0.5}\n    - {name: b, units: 30, rate: 0.25}\n    - {name: c, units: 4, rate: 0.1}\n";

#[test]
fn a_table_is_one_semantic_table_with_a_caption_and_plain_headers() {
    let r = render(&format!(
        "```table\ntitle: Units sold\n{ROWS}columns:\n  - {{key: name}}\n  - {{key: units, format: number}}\n```\n"
    ));
    let b = body(&r);
    assert!(
        b.contains("<caption class=\"rv-table__title\">Units sold</caption>"),
        "{b}"
    );
    assert!(b.contains("<th scope=\"col\" class=\"\" data-rv-key=\"name\">name</th>"));
    assert!(b.contains("<th scope=\"col\" class=\"num\" data-rv-key=\"units\">units</th>"));
    assert!(b.contains("<td class=\"num\""), "numbers align right");
    // The runtime adds the buttons; the static page has none (FEA-RP-22).
    assert!(!b.contains("<button") && !b.contains("data-rv-table"));
}

#[test]
fn a_short_label_of_several_words_stays_on_one_line() {
    // `Store 40` broke before its number in a tight table while the narrower `Store 19` did
    // not: a label of at most 12 characters is kept whole, longer text still wraps.
    let r = render(
        "```table\ndata:\n  inline:\n    - {store: Store 40, city: Karlovy Vary, region: West, size: Rhône-Alpes}\n    - {store: Store 19, city: České Budějovice, region: South Bohemia, size: large}\n```\n",
    );
    let b = body(&r);
    for cell in [
        "<td class=\"label\">Store 40</td>",
        "<td class=\"label\">Store 19</td>",
        "<td class=\"label\">Karlovy Vary</td>",
        "<td class=\"label\">Rhône-Alpes</td>",
        // Longer text may wrap, and a single word has nowhere to break.
        "<td class=\"\">České Budějovice</td>",
        "<td class=\"\">South Bohemia</td>",
        "<td class=\"\">West</td>",
        "<td class=\"\">large</td>",
    ] {
        assert!(b.contains(cell), "{cell} in {b}");
    }
    // The runtime decides per table (`data-rv-labels`), on screen only. A page that runs
    // scripts keeps labels whole on a wide screen until then; a phone, a page without scripts
    // and paper wrap them, so a table that fits wrapped never scrolls sideways.
    assert!(
        r.html
            .contains("@media screen{.rv-table[data-rv-labels=line] td.label{white-space:nowrap}}")
    );
    assert!(r.html.contains(
        "@media screen and (scripting:enabled) and (min-width:641px){.rv-table:not([data-rv-labels]) td.label{white-space:nowrap}}"
    ));
    // A report without such a label carries none of it.
    let plain = render("```table\ndata:\n  inline:\n    - {a: West, b: 1}\n```\n");
    assert!(!plain.html.contains("td.label"));
}

#[test]
fn align_wins_over_the_numeric_default() {
    let r = render(&format!(
        "```table\n{ROWS}columns:\n  - {{key: name}}\n  - {{key: units, align: left}}\n  - {{key: rate, align: center}}\n```\n"
    ));
    let b = body(&r);
    assert!(b.contains("<th scope=\"col\" class=\"\" data-rv-key=\"units\">"));
    assert!(b.contains("<th scope=\"col\" class=\"center\" data-rv-key=\"rate\">"));
}

#[test]
fn rows_past_max_rows_fold_under_a_labelled_disclosure_and_the_total_follows_all_rows() {
    let r = render(&format!(
        "```table\nmax_rows: 1\ntotal: true\n{ROWS}columns:\n  - {{key: name}}\n  - {{key: units, format: number}}\n```\n"
    ));
    let b = body(&r);
    assert!(b.contains("<summary>Showing 1 of 3 rows</summary>"), "{b}");
    let details = b.find("<details").unwrap();
    let total = b.find("<td class=\"\">Total</td>").unwrap();
    assert!(total > details, "the total comes after the folded rows");
    assert!(
        b.contains("1,234"),
        "1200 + 30 + 4 summed over hidden rows too: {b}"
    );
    // Parts of one table share column widths.
    assert_eq!(b.matches("<colgroup>").count(), 3);
}

#[test]
fn total_true_skips_percent_columns_and_says_so() {
    let r = render(&format!(
        "```table\ntotal: true\n{ROWS}columns:\n  - {{key: name}}\n  - {{key: units, format: number}}\n  - {{key: rate, format: percent}}\n```\n"
    ));
    let d = r
        .diagnostics
        .iter()
        .find(|d| d.code == "table.total")
        .expect("table.total");
    assert!(d.message.contains("rate"), "{}", d.message);
    assert!(body(&r).contains("<tfoot>"));
}

#[test]
fn a_year_column_or_metric_prints_years_without_grouping_and_total_true_skips_it() {
    let source = "---\ntitle: T\nlang: cs-CZ\n---\n\n```table\ntotal: true\ndata:\n  inline:\n    - {year: 2024, units: 1200}\n    - {year: 2025, units: 30}\ncolumns:\n  - {key: year, format: year}\n  - {key: units}\n```\n\n```table\ndata:\n  inline:\n    - {year: 2024}\n```\n\n```metric\nvalue: 2025\nformat: year\nlabel: Launch\n```\n";
    let r = render(source);
    assert!(
        r.diagnostics.iter().all(|d| d.code != "block.schema"),
        "a column and a metric take format: year (TYP-23): {:?}",
        r.diagnostics
    );
    let b = body(&r);
    for year in ["2024", "2025"] {
        assert!(b.contains(&format!(">{year}</td>")), "{year}: {b}");
    }
    assert!(
        b.contains(">2\u{a0}024</td>"),
        "without a format a year is a number below 10,000, grouped by lang: {b}"
    );
    assert!(b.contains(">2025</div>"), "the metric: {b}");
    // The years are not summed; the first column reads Total and units add up.
    let total = &b[b.find("<tfoot>").expect("a total row")..];
    let total = &total[..total.find("</tfoot>").unwrap()];
    assert!(total.contains(">Total</td>"), "{total}");
    assert!(
        !total.contains("4049") && !total.contains("4\u{a0}049"),
        "{total}"
    );
    assert!(total.contains("1\u{a0}230"), "{total}");
    let skipped = r
        .diagnostics
        .iter()
        .find(|d| d.code == "table.total")
        .expect("table.total");
    assert_eq!(
        skipped.message, "total: true skips year columns (year)",
        "{skipped:?}"
    );
}

#[test]
fn wide_blocks_take_the_full_width_class() {
    let r = render(&format!("```table\nwide: true\n{ROWS}```\n"));
    assert!(body(&r).contains("class=\"rv-table rv-table--full\""));
}

#[test]
fn a_metric_prints_its_sign_and_takes_its_trend_from_it() {
    let r = render(
        "```metric\nvalue: 4200\nlabel: Revenue\nformat: number\ndelta: 0.04\ndelta_label: vs last year\n```\n\n```metric\nvalue: 10\ndelta: -0.02\n```\n\n```metric\nvalue: 10\ndelta: 0\n```\n\n```metric\nvalue: 10\ndelta: -0.05\ntrend: up\n```\n",
    );
    let b = body(&r);
    assert!(
        b.contains("rv-metric__delta--up\">+4.0% vs last year</span>"),
        "{b}"
    );
    assert!(b.contains("rv-metric__delta--down\">\u{2212}2.0%</span>"));
    assert!(b.contains("rv-metric__delta--flat\">0.0%</span>"));
    assert!(
        b.contains("rv-metric__delta--up\">\u{2212}5.0%</span>"),
        "an explicit trend wins"
    );
}

#[test]
fn a_metric_delta_follows_the_metrics_digits_and_the_reports_decimal_mark() {
    let r = render(
        "---\ntitle: T\nlang: cs-CZ\n---\n\n```metric\nvalue: 10\ndelta: 0.1234\ndigits: 4\n```\n\n```metric\nvalue: 10\ndelta: 0.1234\n```\n",
    );
    let b = body(&r);
    assert!(b.contains("rv-metric__delta--up\">+12,34%</span>"), "{b}");
    assert!(
        b.contains("rv-metric__delta--up\">+12,3%</span>"),
        "three digits by default: {b}"
    );
}

#[test]
fn a_table_total_may_list_the_columns_to_sum() {
    let r = render(&format!(
        "```table\n{ROWS}columns:\n  - {{key: name}}\n  - {{key: units, format: number}}\n  - {{key: rate}}\ntotal: [units]\n```\n"
    ));
    assert!(
        r.diagnostics.iter().all(|d| d.level != "error"),
        "{:?}",
        r.diagnostics
    );
    let b = body(&r);
    assert!(b.contains("<tfoot"), "{b}");
    assert!(b.contains("1,234"), "the listed column is summed: {b}");
    let refused = render(&format!("```table\n{ROWS}total: [units, 3]\n```\n"));
    assert!(
        refused
            .diagnostics
            .iter()
            .any(|d| d.code == "block.schema" && d.path.contains("total")),
        "{:?}",
        refused.diagnostics
    );
}

#[test]
fn a_text_metric_reads_as_written_and_percent_prints_once() {
    let r = render(
        "```metric\nvalue: n/a\nunit: kg\n```\n\n```metric\nvalue: 0.5\nformat: percent\nunit: \"%\"\n```\n",
    );
    let b = body(&r);
    assert!(b.contains(">n/a</div>"), "{b}");
    assert!(
        b.contains(">50%</div>") || b.contains(">50.0%</div>"),
        "{b}"
    );
    assert!(!b.contains("%%"));
}

#[test]
fn a_non_neutral_callout_without_a_title_warns() {
    let r = render(
        "```callout\nkind: up\nbody: Revenue rose.\n```\n\n```callout\nkind: neutral\nbody: Fine.\n```\n\n```callout\nkind: down\ntitle: Costs rose\nbody: Fine.\n```\n",
    );
    assert_eq!(
        codes(&r.diagnostics)
            .iter()
            .filter(|c| **c == "callout.title")
            .count(),
        1
    );
}

#[test]
fn media_is_embedded_once_with_no_fallback_link() {
    let mut options = RenderOptions::default();
    for (k, v) in [
        ("a.mp3", "data:audio/mpeg;base64,AAAA"),
        ("v.mp4", "data:video/mp4;base64,BBBB"),
        ("p.png", "data:image/png;base64,CCCC"),
        ("c.vtt", "WEBVTT\n\n00:00.000 --> 00:01.000\nHello\n"),
        ("t.md", "Spoken **words**."),
    ] {
        options.data.insert(k.into(), v.into());
    }
    let r = render_with(
        "```audio\nsrc: a.mp3\ntitle: Interview\ntranscript: t.md\n```\n\n```video\nsrc: v.mp4\ntitle: Demo\ncaption: A demo\nposter: p.png\ncaptions: c.vtt\nwide: true\n```\n",
        &options,
    );
    let b = body(&r);
    assert_eq!(b.matches("data:audio/mpeg;base64,AAAA").count(), 1);
    assert_eq!(b.matches("data:video/mp4;base64,BBBB").count(), 1);
    assert!(!b.contains("Download"));
    assert!(b.contains("preload=\"metadata\"") && !b.contains("autoplay") && !b.contains("loop"));
    assert!(b.contains("aria-label=\"Interview\"") && b.contains("aria-label=\"Demo\""));
    assert!(b.contains("<track kind=\"captions\" src=\"data:text/vtt;base64,"));
    assert!(b.contains("srclang=\"en\""));
    assert!(b.contains("<details class=\"rv-transcript\"><summary>Transcript</summary>"));
    assert!(b.contains("<strong>words</strong>"));
    assert!(b.contains("class=\"rv-video rv-video--full\""));
}

#[test]
fn an_image_keeps_its_alt_and_reserves_its_space() {
    let mut options = RenderOptions::default();
    options
        .data
        .insert("p.png".into(), "data:image/png;base64,CCCC".into());
    let r = render_with(
        "```image\nsrc: p.png\nalt: \"\"\nwidth: 300\nheight: 200\nfit: cover\nwide: true\ncaption: Seen\n```\n",
        &options,
    );
    let b = body(&r);
    assert!(b.contains("alt=\"\"") && b.contains("width=\"300\" height=\"200\""));
    assert!(b.contains("aspect-ratio:300/200") && b.contains("object-fit:cover"));
    assert!(b.contains("class=\"rv-image rv-image--full\""));
    assert!(b.contains("<figcaption>Seen</figcaption>"));
}

#[test]
fn an_allowed_embed_is_a_card_until_the_reader_asks() {
    let r = render(
        "```embed\nurl: https://www.youtube.com/embed/abc\ncaption: A talk\nwide: true\n```\n",
    );
    let b = body(&r);
    assert!(
        !b.contains("<iframe"),
        "no frame before the reader asks: {b}"
    );
    assert!(b.contains("data-rv-embed=\"https://www.youtube.com/embed/abc\""));
    assert!(b.contains(
        "<button type=\"button\" class=\"rv-embed__load\" hidden>Load from www.youtube.com</button>"
    ));
    assert!(b.contains("<a class=\"rv-embed__link\" href=\"https://www.youtube.com/embed/abc\" rel=\"noopener noreferrer\">"));
    assert!(b.contains("rv-embed--full"));
    let policy = r
        .html
        .split("Content-Security-Policy\" content=\"")
        .nth(1)
        .unwrap();
    assert!(
        policy.contains("frame-src https://www.youtube.com;"),
        "{policy:.300}"
    );
}

#[test]
fn a_blocked_embed_is_an_error_with_a_notice_naming_the_host() {
    let r = render("```embed\nurl: https://evil.example/x\n```\n");
    assert!(
        body(&r).contains("Embed not rendered: host &#39;evil.example&#39; is not allowed"),
        "{}",
        body(&r)
    );
    let d = r
        .diagnostics
        .iter()
        .find(|d| d.code == "security.embed")
        .unwrap();
    assert_eq!(d.level, "error");
}

#[test]
fn a_total_shows_no_more_decimals_than_its_cells_and_a_currency_unit_names_the_currency() {
    let r = render_with(
        "```table\ndata:\n  inline:\n    - {name: a, price: 10.1}\n    - {name: b, price: 20.2}\n    - {name: c, price: 30.3}\ntotal: true\ncolumns:\n  - {key: name}\n  - {key: price, format: number, unit: \"€\"}\n```\n",
        &RenderOptions::default(),
    );
    assert!(r.html.contains("€60.6"), "currency unit leads");
    let r = render_with(
        "```table\ndata:\n  inline:\n    - {name: a, units: 10.1}\n    - {name: b, units: 20.2}\n    - {name: c, units: 30.3}\ntotal: true\ncolumns:\n  - {key: name}\n  - {key: units}\n```\n",
        &RenderOptions::default(),
    );
    assert!(r.html.contains(">60.6<"));
    assert!(!r.html.contains("60.599999"));
}

#[test]
fn a_missing_header_image_names_its_frontmatter_line() {
    let r = render_with(
        "---\ntitle: T\ntitle_image: {src: gone.png, alt: Gone}\n---\n\nText.\n",
        &RenderOptions::default(),
    );
    let d = r
        .diagnostics
        .iter()
        .find(|d| d.code == "report.asset.missing")
        .expect("missing asset");
    assert_eq!(d.line, Some(3));
    assert_eq!(d.tag.as_deref(), Some("frontmatter"));
}

#[test]
fn a_numeric_cell_sorts_by_what_it_displays_not_by_its_source_precision() {
    let r = render_with(
        "```table\ndata:\n  inline:\n    - {name: a, v: 1234567.891, p: 0.123456789, c: 12.3456789, u: 1.23456789, d: 9.87654321, w: 98765432.1}\ncolumns:\n  - {key: name}\n  - {key: v, format: number, unit: \"$\"}\n  - {key: p, format: percent}\n  - {key: c, format: number}\n  - {key: u}\n  - {key: d, digits: 2}\n  - {key: w}\n```\n",
        &RenderOptions::default(),
    );
    let sorts: Vec<&str> = r
        .html
        .split("data-rv-sort-value=\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .collect();
    // The cells show $1.23 million, 12.3%, 12.3, 1.23, 9.9 and 98.8 million, with or without a
    // format: the sort values are as exact, and no finer.
    assert_eq!(
        sorts,
        ["1230000", "0.123", "12.3", "1.23", "9.9", "98800000"],
        "{:?} {}",
        r.diagnostics,
        r.html.split("<main").nth(1).unwrap_or("")
    );
    for source in [
        "1234567.891",
        "0.123456789",
        "12.3456789",
        "1.23456789",
        "9.87654321",
        "98765432.1",
    ] {
        assert!(!r.html.contains(source), "{source} never reaches the page");
    }
}
