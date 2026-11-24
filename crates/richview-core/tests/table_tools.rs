//! Heat in table cells, the filter box's marker and `sortable` (FEA-RP-69, FEA-RP-70, FEA-RP-22).

use richview_core::document::{Output, RenderOptions, RenderedReport, ReportFolder};

fn render_with(source: &str, options: &RenderOptions) -> RenderedReport {
    let mut options = options.clone();
    options.report_folder = Some(ReportFolder {
        root: "tables-fixture".into(),
        source: "report.rmd".into(),
        files: vec!["report.rmd".into()],
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
        let slides = render(&format!(
            "---\ntitle: Deck\nslides: true\n---\n\n# One\n\n```table\n{extra}data:\n  inline:\n{}```\n",
            rows(12)
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
