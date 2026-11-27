//! Heat in table cells, the filter box's marker and `sortable` (FEA-RP-69, FEA-RP-70, FEA-RP-22).

use richview_core::document::{Output, RenderOptions, RenderedReport, ReportFolder};

fn render_with(source: &str, options: &RenderOptions) -> RenderedReport {
    let mut options = options.clone();
    options.report_folder = Some(ReportFolder {
        root: "tables-fixture".into(),
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

/// The cells of the body's rows, as `(class, style, text)`.
fn cells(body: &str) -> Vec<(String, String, String)> {
    let tbody = &body[body.find("<tbody>").unwrap()..body.find("</tbody>").unwrap()];
    tbody
        .split("<td")
        .skip(1)
        .map(|cell| {
            let attr = |name: &str| {
                cell[..cell.find('>').unwrap()]
                    .split_once(&format!("{name}=\""))
                    .map(|(_, rest)| rest[..rest.find('"').unwrap()].to_owned())
                    .unwrap_or_default()
            };
            let text = &cell[cell.find('>').unwrap() + 1..cell.find("</td>").unwrap()];
            (attr("class"), attr("style"), text.to_owned())
        })
        .collect()
}

const ROWS: &str = "data:\n  inline:\n    - {name: a, units: 1200, change: -0.2}\n    - {name: b, units: 30, change: 0.1}\n    - {name: c, units: 615, change: 0}\n    - {name: d, change: 0.05}\n";

#[test]
fn a_heat_column_shades_its_cells_from_the_lowest_to_the_highest_value() {
    let r = render(&format!(
        "```table\n{ROWS}columns:\n  - {{key: name}}\n  - {{key: units, heat: true}}\n```\n"
    ));
    let cells = cells(body(&r));
    let units: Vec<_> = cells.iter().skip(1).step_by(2).collect();
    // The highest value takes the whole shade, the lowest and the empty cell none.
    assert_eq!(units[0].0, "num rv-heat");
    assert_eq!(units[0].1, "--rv-heat:100%");
    assert_eq!((units[1].0.as_str(), units[1].1.as_str()), ("num", ""));
    assert_eq!(units[2].1, "--rv-heat:50%");
    assert_eq!((units[3].0.as_str(), units[3].1.as_str()), ("num", ""));
    // Every cell still prints its number: the shade is never the only carrier.
    assert_eq!(units[0].2, "1,200");
    // The rules and the style's two shades come with the report.
    assert!(r.html.contains("--rv-heat-up:"));
    assert!(r.html.contains("print-color-adjust:exact"));
    assert!(r.diagnostics.iter().all(|d| d.code != "table.heat"));
}

#[test]
fn a_diverging_column_shades_both_sides_of_zero_on_one_scale() {
    let r = render(&format!(
        "```table\n{ROWS}columns:\n  - {{key: name}}\n  - {{key: change, format: percent, heat: diverging}}\n```\n"
    ));
    let cells = cells(body(&r));
    let change: Vec<_> = cells.iter().skip(1).step_by(2).collect();
    assert_eq!(change[0].0, "num rv-heat rv-heat--neg");
    assert_eq!(change[0].1, "--rv-heat:100%");
    assert_eq!(change[1].0, "num rv-heat");
    assert_eq!(change[1].1, "--rv-heat:50%");
    assert_eq!(change[2].1, "", "zero is the page");
    assert_eq!(change[3].1, "--rv-heat:25%");
}

#[test]
fn a_shade_comes_from_the_value_the_cell_shows() {
    // 1.204 and 1.196 both show 1.2 under the three-digit rule, so they share a shade and the
    // page holds no finer number than the printed one (FEA-CH-37).
    let r = render(
        "```table\ndata:\n  inline:\n    - {name: a, v: 1.204}\n    - {name: b, v: 1.196}\n    - {name: c, v: 1.1}\n    - {name: d, v: 1.3}\ncolumns:\n  - {key: name}\n  - {key: v, digits: 2, heat: true}\n```\n",
    );
    let cells = cells(body(&r));
    assert_eq!(cells[1].2, "1.2");
    assert_eq!(cells[1].1, cells[3].1);
    assert_eq!(cells[1].1, "--rv-heat:50%");
    assert!(!body(&r).contains("1.204") && !body(&r).contains("1.196"));
}

#[test]
fn cells_that_show_the_same_text_share_a_shade_and_a_sort_value_past_what_a_number_holds() {
    // Three finite fractions whose percent no number holds: every cell prints `inf%`, so none
    // may be told apart by a shade or a sort value.
    let r = render(
        "```table\ndata:\n  inline:\n    - {name: a, share: 2e306}\n    - {name: b, share: 3e306}\n    - {name: c, share: 4e306}\n    - {name: d, share: 0.5}\ncolumns:\n  - {key: name}\n  - {key: share, format: percent, heat: true}\n```\n",
    );
    let cells = cells(body(&r));
    let shares: Vec<_> = cells
        .iter()
        .filter(|(_, _, text)| text.ends_with('%'))
        .collect();
    assert_eq!(shares.len(), 4);
    for (class, style, text) in &shares[..3] {
        assert_eq!(text, "inf%");
        assert!(!class.contains("rv-heat"), "{class}");
        assert!(style.is_empty(), "{style}");
    }
    assert_eq!(shares[3].2, "50%");
    let tbody = &body(&r)[body(&r).find("<tbody>").unwrap()..];
    assert_eq!(tbody.matches("data-rv-sort-value=").count(), 1, "{tbody}");
    assert!(tbody.contains("data-rv-sort-value=\"0.5\""));
}

#[test]
fn heat_scale_table_puts_the_heat_columns_on_one_scale() {
    let source = |scale: &str| {
        format!(
            "```table\n{scale}data:\n  inline:\n    - {{name: a, q1: 10, q2: 40}}\n    - {{name: b, q1: 20, q2: 30}}\ncolumns:\n  - {{key: name}}\n  - {{key: q1, heat: true}}\n  - {{key: q2, heat: true}}\n```\n"
        )
    };
    let own = render(&source(""));
    let own = cells(body(&own));
    assert_eq!(own[4].1, "--rv-heat:100%", "20 is its column's highest");
    let shared = render(&source("heat_scale: table\n"));
    let shared = cells(body(&shared));
    assert_eq!(shared[4].1, "--rv-heat:33%", "20 of 10 to 40");
    assert_eq!(shared[2].1, "--rv-heat:100%");
}

#[test]
fn heat_on_a_text_column_warns_and_the_total_row_is_never_shaded() {
    let r = render(&format!(
        "```table\ntotal: true\n{ROWS}columns:\n  - {{key: name, heat: true}}\n  - {{key: units, heat: true}}\n```\n"
    ));
    let warning = r
        .diagnostics
        .iter()
        .find(|d| d.code == "table.heat")
        .expect("table.heat");
    assert!(warning.message.contains("name"));
    let b = body(&r);
    let foot = &b[b.find("<tfoot>").unwrap()..b.find("</tfoot>").unwrap()];
    assert!(!foot.contains("rv-heat"));
    assert!(cells(b)[0].0.is_empty() && cells(b)[0].1.is_empty());
}

#[test]
fn a_refused_heat_value_is_a_schema_error() {
    let r = render(&format!(
        "```table\n{ROWS}columns:\n  - {{key: units, heat: \"#ff0000\"}}\n```\n"
    ));
    assert!(r.diagnostics.iter().any(|d| d.code == "block.schema"));
    assert!(!r.html.contains("rv-heat"));
    let r = render(&format!("```table\n{ROWS}heat_scale: rows\n```\n"));
    assert!(r.diagnostics.iter().any(|d| d.code == "block.schema"));
}

#[test]
fn paper_keeps_the_shades_and_a_report_without_heat_carries_no_heat_rules() {
    let source = format!(
        "```table\n{ROWS}columns:\n  - {{key: name}}\n  - {{key: units, heat: true}}\n```\n"
    );
    let paper = render_with(
        &source,
        &RenderOptions {
            output: Output::Pdf,
            ..Default::default()
        },
    );
    assert!(body(&paper).contains("class=\"num rv-heat\" style=\"--rv-heat:100%\""));
    assert!(paper.html.contains("--rv-heat-up:"));
    let plain = render(&format!("```table\n{ROWS}```\n"));
    assert!(!plain.html.contains("rv-heat"));
    assert!(!plain.html.contains("rv-table__filter"));
}

#[test]
fn a_table_of_more_than_ten_rows_is_marked_for_the_filter_box() {
    let rows = |n: usize| {
        (0..n)
            .map(|i| format!("    - {{name: r{i}, units: {i}}}\n"))
            .collect::<String>()
    };
    let table = |n: usize, extra: &str| {
        render(&format!(
            "```table\n{extra}data:\n  inline:\n{}```\n",
            rows(n)
        ))
    };
    let long = table(11, "");
    assert!(body(&long).contains("<div class=\"rv-table\" data-rv-filter><table"));
    assert!(long.html.contains(".rv-table__filter"), "the box's rules");
    // The static page holds no field: the runtime adds it, so no control is dead without it.
    assert!(!body(&long).contains("<input"));
    let short = table(10, "");
    assert!(!body(&short).contains("data-rv-filter"));
    assert!(!short.html.contains(".rv-table__filter"));
    assert!(body(&table(3, "filter: true\n")).contains("data-rv-filter"));
    assert!(!body(&table(30, "filter: false\n")).contains("data-rv-filter"));
    // A table that is both wide and long carries the marker and brings the box's rules.
    let wide = table(30, "filter: auto\nwide: true\n");
    assert!(body(&wide).contains("<div class=\"rv-table rv-table--full\" data-rv-filter>"));
    assert!(wide.html.contains(".rv-table__filter"));
    // The rules keep the box's room under the title until the runtime starts, and give it back.
    assert!(wide.html.contains("rv-filter-room-out"));
    // Paper has no reader to type.
    let paper = render_with(
        &format!("```table\ndata:\n  inline:\n{}```\n", rows(30)),
        &RenderOptions {
            output: Output::Pdf,
            ..Default::default()
        },
    );
    assert!(!body(&paper).contains("data-rv-filter"));
    // A slide shows the table without the report's tools, whatever `filter` says.
    for extra in ["", "filter: true\n"] {
        let piece = format!("{extra}data:\n  inline:\n{}", rows(12));
        let indented: String = piece.lines().map(|line| format!("  {line}\n")).collect();
        let slides = render(&format!(
            "format: rslide/1\ntitle: Deck\n---\ntitle: One\ntable:\n{indented}"
        ));
        assert!(slides.html.contains("<div class=\"rv-table\"><table"));
        assert!(!slides.html.contains(" data-rv-filter>"));
        assert!(!slides.html.contains(".rv-table__filter"));
    }
    let refused = table(3, "filter: search\n");
    assert!(refused.diagnostics.iter().any(|d| d.code == "block.schema"));
}

#[test]
fn sortable_false_keeps_the_headings_plain() {
    let r = render(&format!("```table\n{ROWS}```\n"));
    assert!(body(&r).contains("data-rv-key=\"units\""));
    let r = render(&format!("```table\nsortable: false\n{ROWS}```\n"));
    let b = body(&r);
    assert!(!b.contains("data-rv-key") && !b.contains("aria-sort"));
    assert!(b.contains("<th scope=\"col\" class=\"num\">units</th>"));
}

#[test]
fn an_undeclared_column_prints_each_cell_on_its_own_and_decimals_set_one_scale() {
    // No `decimals` and no DECIMAL type: each cell keeps the precision of its own value, as it
    // would alone (FEA-CH-37), so 9 does not become 9.00 beside 9.26.
    let r = render(
        "```table\ndata: {inline: [{price: 9.26, qty: 2, rate: 0.061}, {price: -9, qty: 3, rate: 0.07}, {price: 8.98, qty: 4, rate: 0}]}\ncolumns: [{key: price, unit: $, heat: true}, {key: qty}, {key: rate, format: percent}]\n```",
    );
    let undeclared = cells(body(&r));
    assert_eq!(
        undeclared.iter().map(|c| c.2.as_str()).collect::<Vec<_>>(),
        ["$9.26", "2", "6.1%", "−$9", "3", "7%", "$8.98", "4", "0%"]
    );
    // The author's `decimals` is a declared scale: one for every cell of the column.
    let r = render(
        "```table\ndata: {inline: [{price: 9.26, rate: 0.061}, {price: -9, rate: 0.07}, {price: 8.98, rate: 0}]}\ncolumns: [{key: price, unit: $, decimals: 2}, {key: rate, format: percent, decimals: 1}]\n```",
    );
    let declared = cells(body(&r));
    assert_eq!(
        declared.iter().map(|c| c.2.as_str()).collect::<Vec<_>>(),
        ["$9.26", "6.1%", "−$9.00", "7.0%", "$8.98", "0.0%"]
    );
}

#[test]
fn a_declared_scale_survives_source_parsing_and_percent_conversion() {
    for (kind, file, data) in [
        ("inline", "", "[{v: 0.0700}, {v: 0}]"),
        ("csv", "rates.csv", "v\n0.0700\n0\n"),
        ("json", "rates.json", r#"[{"v":7.00e-2},{"v":0}]"#),
        (
            "parquet",
            "rates.parquet",
            r#"{"columns":["v"],"fields":["v"],"scales":{"v":4},"rows":[[0.07],[0]]}"#,
        ),
    ] {
        let mut options = RenderOptions::default();
        let source = if file.is_empty() {
            data
        } else {
            options.data.insert(file.into(), data.into());
            file
        };
        let shown = |columns: &str| {
            let report = render_with(
                &format!("```table\ndata: {{{kind}: {source}}}\ncolumns: [{columns}]\n```"),
                &options,
            );
            let cells = cells(body(&report));
            [cells[0].2.clone(), cells[1].2.clone()]
        };
        // The author's places decide the scale whatever the source wrote.
        assert_eq!(
            shown("{key: v, format: percent, decimals: 2}"),
            ["7.00%", "0.00%"],
            "{kind}"
        );
        // The source's declared scale decides it when the author says nothing; a file that only
        // wrote its zeros declares nothing, and each cell prints as it would alone.
        let declared = if kind == "parquet" {
            ["7.00%", "0.00%"]
        } else {
            ["7%", "0%"]
        };
        assert_eq!(shown("{key: v, format: percent}"), declared, "{kind}");
    }
}

#[test]
fn authored_decimals_round_sort_keys_heat_and_the_total_at_the_printed_scale() {
    let r = render(
        "```table\ndata: {inline: [{v: 9.255}, {v: 9.256}, {v: -1.225}, {v: 123.45}]}\ncolumns: [{key: v, decimals: 2, unit: €, heat: true}]\ntotal: true\n```",
    );
    let page = body(&r);
    let cells = cells(page);
    assert_eq!(
        cells.iter().map(|c| c.2.as_str()).collect::<Vec<_>>(),
        ["€9.26", "€9.26", "−€1.23", "€123.45"]
    );
    assert_eq!(cells[0].1, cells[1].1);
    assert_eq!(page.matches("data-rv-sort-value=\"9.26\"").count(), 2);
    assert!(page.contains("€140.74</td>"));
    assert!(!page.contains("9.255") && !page.contains("9.256"));
}

#[test]
fn explicit_places_respect_significant_digits_and_override_source_scale() {
    let r = render(
        "```table\ndata: {inline: [{v: 2.45678}, {v: 2.40}]}\ncolumns: [{key: v, decimals: 2, digits: 2}]\n```",
    );
    let values = cells(body(&r));
    assert_eq!([&values[0].2, &values[1].2], ["2.50", "2.40"]);
    let r = render(
        "```table\ndata: {inline: [{v: 1.249}]}\ncolumns: [{key: v, digits: 3, decimals: 1}]\n```",
    );
    assert_eq!(cells(body(&r))[0].2, "1.2");
    let r = render(
        "```table\ndata: {inline: [{v: 1234.56}]}\ncolumns: [{key: v, digits: 2, decimals: 2, format: number}]\n```",
    );
    assert_eq!(cells(body(&r))[0].2, "1,200.00");
    assert!(body(&r).contains("data-rv-sort-value=\"1200\""));
    // With no declared scale the zeros written in the source are not shown (FEA-CH-37).
    let r = render("```table\ndata: {inline: [{v: 9.2600}, {v: 9}]}\n```");
    let values = cells(body(&r));
    assert_eq!([&values[0].2, &values[1].2], ["9.26", "9"]);
    let mut options = RenderOptions::default();
    options.data.insert(
        "x.parquet".into(),
        r#"{"columns":["v"],"fields":["v"],"scales":{"v":4},"rows":[[9.256],[9]]}"#.into(),
    );
    let r = render_with(
        "```table\ndata: {parquet: x.parquet}\ncolumns: [{key: v, decimals: 1}]\n```",
        &options,
    );
    let values = cells(body(&r));
    assert_eq!([&values[0].2, &values[1].2], ["9.3", "9.0"]);
    let r = render(
        "```table\ndata: {inline: [{v: 3.14159}, {v: withheld}]}\ncolumns: [{key: v, decimals: 1}]\n```",
    );
    let values = cells(body(&r));
    assert_eq!([&values[0].2, &values[1].2], ["3.1", "withheld"]);
    assert!(!body(&r).contains("3.14159"));
    for invalid in ["-1", "7", "1.5", "'2'"] {
        let r = render(&format!(
            "```table\ndata: {{inline: [{{v: 1}}]}}\ncolumns: [{{key: v, decimals: {invalid}}}]\n```"
        ));
        assert!(
            r.diagnostics.iter().any(|d| d.code == "block.schema"),
            "{invalid}"
        );
    }
}

/// The shown text and sort key of every cell of a one-column table.
fn column(source: &str, options: &RenderOptions) -> Vec<(String, String)> {
    let r = render_with(source, options);
    assert!(
        !r.diagnostics.iter().any(|d| d.level == "error"),
        "{:?}",
        r.diagnostics
    );
    cells(body(&r))
        .into_iter()
        .map(|(_, _, text)| text)
        .zip(
            body(&r)
                .split("data-rv-sort-value=\"")
                .skip(1)
                .map(|rest| rest[..rest.find('"').unwrap()].to_owned()),
        )
        .collect()
}

fn pair(shown: &str, sort: &str) -> (String, String) {
    (shown.into(), sort.into())
}

#[test]
fn an_undeclared_column_prints_each_cell_as_it_would_alone() {
    use richview_chart::numbers::{DEFAULT_DIGITS, Format, Kind, Locale, value};
    // The precision fixture's numbers and the edges of each notation: no cell takes a scale or a
    // word from its neighbours, whatever the format, unit and digits (FEA-CH-37).
    let values = [
        98765.4321,
        4321.98765,
        12.3456789,
        98.7654321,
        167543.217,
        1234567.891,
        999.95,
        1234.56789,
        0.123456789,
        0.045678912,
        0.5,
        0.00049,
        1.5e-9,
        12345678901234567.0,
        0.0,
        -8.14159265,
    ];
    let rows = values
        .iter()
        .map(|v| format!("{{v: {v:?}}}"))
        .collect::<Vec<_>>()
        .join(", ");
    for (name, kind) in [
        ("", Kind::Auto),
        ("format: number", Kind::Number),
        ("format: number_compact", Kind::NumberCompact),
        ("format: percent", Kind::Percent),
    ] {
        for unit in [None, Some("$")] {
            for digits in [None, Some(4)] {
                let spec = [
                    Some("key: v".to_owned()),
                    (!name.is_empty()).then(|| name.to_owned()),
                    unit.map(|u| format!("unit: {u:?}")),
                    digits.map(|d| format!("digits: {d}")),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(", ");
                let shown = column(
                    &format!("```table\ndata: {{inline: [{rows}]}}\ncolumns: [{{{spec}}}]\n```"),
                    &RenderOptions::default(),
                );
                let format = Format {
                    kind,
                    unit: unit.map(str::to_owned),
                    digits: digits.unwrap_or(DEFAULT_DIGITS),
                    locale: Locale::default(),
                };
                let expected: Vec<_> = values.iter().map(|v| value(*v, &format)).collect();
                let texts: Vec<_> = shown.iter().map(|(text, _)| text.clone()).collect();
                assert_eq!(texts, expected, "{spec}");
            }
        }
    }
    // The precision fixture's column, in words and in dollars.
    assert_eq!(
        column(
            "```table\ndata: {inline: [{v: 1234567.891}, {v: 167543.217}, {v: 98.7654321}]}\ncolumns: [{key: v, format: number_compact}]\n```",
            &RenderOptions::default()
        ),
        [
            pair("1.23 million", "1230000"),
            pair("168 thousand", "168000"),
            pair("98.8", "98.8")
        ]
    );
    assert_eq!(
        column(
            "```table\ndata: {inline: [{v: 98765.4321}, {v: 4321.98765}, {v: 12.3456789}]}\ncolumns: [{key: v, format: number, unit: $}]\n```",
            &RenderOptions::default()
        ),
        [
            pair("$98,800", "98800"),
            pair("$4,322", "4322"),
            pair("$12.3", "12.3")
        ]
    );
    // Authored `digits` is honored on a four-digit number too, in a table (FEA-CH-37).
    assert_eq!(
        column(
            "```table\ndata: {inline: [{v: 1234.56}, {v: 12.3456}]}\ncolumns: [{key: v, digits: 3}]\n```",
            &RenderOptions::default()
        ),
        [pair("1,230", "1230"), pair("12.3", "12.3")]
    );
}

#[test]
fn a_declared_column_is_plain_at_its_scale_and_an_undeclared_one_keeps_each_cell_in_its_own_notation()
 {
    let table = |rows: &str, extra: &str| {
        format!("```table\ndata: {{inline: {rows}}}\ncolumns: [{{key: v{extra}}}]\n```")
    };
    let plain = RenderOptions::default();
    // With no declared scale each cell is compact or plain on its own: 12.3 thousand beside 988.
    assert_eq!(
        column(&table("[{v: 12345.67}, {v: 987.5}]", ""), &plain),
        [pair("12.3 thousand", "12300"), pair("988", "988")]
    );
    assert_eq!(
        column(
            &table("[{v: 1234567}, {v: 250000}, {v: 98000}]", ""),
            &plain
        ),
        [
            pair("1.23 million", "1230000"),
            pair("250 thousand", "250000"),
            pair("98 thousand", "98000")
        ]
    );
    assert_eq!(
        column(
            &table("[{v: 1234567.89}, {v: 500.5}, {v: 1.25}]", ""),
            &plain
        ),
        [
            pair("1.23 million", "1230000"),
            pair("501", "501"),
            pair("1.25", "1.25")
        ]
    );
    // `number_compact` is the author's own choice of words, for each cell that has one.
    assert_eq!(
        column(
            &table(
                "[{v: 1500}, {v: 2500}, {v: 98.7654321}]",
                ", format: number_compact"
            ),
            &plain
        ),
        [
            pair("1.5 thousand", "1500"),
            pair("2.5 thousand", "2500"),
            pair("98.8", "98.8")
        ]
    );
    // A declared scale, the author's `decimals` or the source's DECIMAL scale, makes every cell
    // a plain number at that scale with its separators, never compacted.
    assert_eq!(
        column(
            &table(
                "[{v: 12345.67}, {v: 987.5}, {v: 1234567.89}]",
                ", decimals: 2"
            ),
            &plain
        ),
        [
            pair("12,345.67", "12345.67"),
            pair("987.50", "987.5"),
            pair("1,234,567.89", "1234567.89")
        ]
    );
    let declared = |rows: &str, extra: &str| {
        let mut options = RenderOptions::default();
        options.data.insert(
            "x.parquet".into(),
            format!(r#"{{"columns":["v"],"fields":["v"],"scales":{{"v":2}},"rows":{rows}}}"#),
        );
        column(
            &format!("```table\ndata: {{parquet: x.parquet}}\ncolumns: [{{key: v{extra}}}]\n```"),
            &options,
        )
    };
    assert_eq!(
        declared("[[12345.67],[987.5]]", ""),
        [pair("12,345.67", "12345.67"), pair("987.50", "987.5")]
    );
    assert_eq!(
        declared("[[1234567.89],[500.5]]", ""),
        [pair("1,234,567.89", "1234567.89"), pair("500.50", "500.5")]
    );
    assert_eq!(
        declared("[[12345.67],[9999.4],[987.5]]", ""),
        [
            pair("12,345.67", "12345.67"),
            pair("9,999.40", "9999.4"),
            pair("987.50", "987.5")
        ]
    );
    // Words the author asked for outright are not a declared scale: each cell has its own,
    // whatever the source declared.
    assert_eq!(
        declared("[[1234567.89],[250000.5]]", ", format: number_compact"),
        [
            pair("1.23 million", "1230000"),
            pair("250 thousand", "250000")
        ]
    );
}

#[test]
fn a_number_is_never_printed_as_zero() {
    let table = |rows: &str, extra: &str| {
        format!("```table\ndata: {{inline: {rows}}}\ncolumns: [{{key: v{extra}}}]\n```")
    };
    let plain = RenderOptions::default();
    // A cell the author's places would round to zero keeps its own digits, in the sort key too.
    assert_eq!(
        column(
            &table("[{v: 0.04}, {v: 12345.67}]", ", decimals: 1"),
            &plain
        ),
        [pair("0.04", "0.04"), pair("12,345.7", "12345.7")]
    );
    assert_eq!(
        column(
            &table("[{v: 0.0004}, {v: 0.5}]", ", format: percent, decimals: 1"),
            &plain
        ),
        [pair("0.04%", "0.0004"), pair("50.0%", "0.5")]
    );
    // The same for words the author asked for: 98.77 beside millions is not 0.00 million, and it
    // keeps the default three digits.
    assert_eq!(
        column(
            &table(
                "[{v: 1234567.891}, {v: 98.7654321}]",
                ", format: number_compact, decimals: 2"
            ),
            &plain
        ),
        [pair("1.23 million", "1230000"), pair("98.8", "98.8")]
    );
    // A cell that is zero is still zero.
    assert_eq!(
        column(&table("[{v: 0}, {v: 1.25}]", ", decimals: 1"), &plain),
        [pair("0.0", "0"), pair("1.3", "1.3")]
    );
}

#[test]
fn decimals_on_a_year_column_do_not_change_the_year() {
    let r = render(
        "```table\ndata: {inline: [{y: 2024}, {y: 1999}]}\ncolumns: [{key: y, format: year, decimals: 2}]\n```",
    );
    let values = cells(body(&r));
    assert_eq!([&values[0].2, &values[1].2], ["2024", "1999"]);
}
