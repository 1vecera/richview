//! The live example's two tools: a picker for the chart's type and buttons that add a block. Both
//! only edit the source, so the visible source always matches the visible report. The picker offers
//! a type only when the core renders the chart with that type and finds nothing to report, so the
//! reader cannot choose a chart the data does not fit. `browser/toolbar.rs` shows the controls;
//! this part is pure, and its tests run natively.
use crate::completion::{offset, position};
use crate::reveal::{Kind, blocks};
use richview_core::document::RenderOptions;
use std::ops::Range;

/// A change to the source: the stretch `start..end`, in UTF-16 units from the start of the source
/// as a text area counts them, becomes `text`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Edit {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// A chart block of the source and where it writes its type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Chart {
    /// The block's lines `first..last`, counted from 0, fences included.
    pub first: usize,
    pub last: usize,
    /// The line that holds `type:`, and the bytes of its value in that line.
    line: usize,
    value: Range<usize>,
    /// The type as written.
    pub kind: String,
}

/// The chart blocks of `source` that write their type on a line of its own.
pub(crate) fn charts(source: &str) -> Vec<Chart> {
    let lines: Vec<&str> = source.split('\n').collect();
    blocks(source)
        .into_iter()
        .filter(|block| block.kind == Kind::Fence)
        .filter_map(|block| {
            let opening = lines[block.start].trim_start().trim_start_matches('`');
            if !opening.trim().eq_ignore_ascii_case("chart") {
                return None;
            }
            // An unclosed fence has no closing line; the lines after the opening one are the body.
            let body = block.start + 1..block.end;
            let (line, value) = body
                .into_iter()
                .find_map(|i| Some((i, type_value(lines[i])?)))?;
            Some(Chart {
                first: block.start,
                last: block.end,
                line,
                kind: lines[line][value.clone()].to_owned(),
                value,
            })
        })
        .collect()
}

/// The bytes of the type a line writes as `type: value`, unquoted and without a comment.
fn type_value(line: &str) -> Option<Range<usize>> {
    let rest = line.strip_prefix("type:")?;
    let skipped = rest.len() - rest.trim_start().len();
    let start = "type:".len() + skipped;
    let value = &line[start..];
    let end = match value.chars().next()? {
        quote @ ('"' | '\'') => {
            let close = value[1..].find(quote)?;
            return Some(start + 1..start + 1 + close).filter(|r| !r.is_empty());
        }
        _ => value.find(" #").unwrap_or(value.len()),
    };
    let text = value[..end].trim_end();
    (!text.is_empty()).then_some(start..start + text.len())
}

/// The chart the tools act on: the one the caret is in, else the first. `line` is the caret's line.
pub(crate) fn chart_at(source: &str, line: Option<usize>) -> Option<Chart> {
    let mut all = charts(source);
    let at = line.and_then(|line| {
        all.iter()
            .position(|chart| (chart.first..chart.last).contains(&line))
    });
    (!all.is_empty()).then(|| all.swap_remove(at.unwrap_or(0)))
}

/// The edit that gives `chart` the type `kind`.
pub(crate) fn retype(source: &str, chart: &Chart, kind: &str) -> Edit {
    let line = source.split('\n').nth(chart.line).unwrap_or_default();
    let column = |bytes: usize| line[..bytes].encode_utf16().count();
    Edit {
        start: offset(source, chart.line, column(chart.value.start)),
        end: offset(source, chart.line, column(chart.value.end)),
        text: kind.to_owned(),
    }
}

/// The byte of `source` at a count of UTF-16 units.
fn byte(source: &str, units: usize) -> usize {
    let mut seen = 0;
    for (at, c) in source.char_indices() {
        if seen >= units {
            return at;
        }
        seen += c.len_utf16();
    }
    source.len()
}

/// What the core is asked about a chart type: the report's front matter, so the styles are the
/// reader's, and the chart alone with that type.
fn alone(source: &str, chart: &Chart, kind: &str) -> String {
    let lines: Vec<&str> = source.split('\n').collect();
    let mut out = String::new();
    if let Some(front) = blocks(source).first().filter(|b| b.kind == Kind::Front) {
        out.push_str(&lines[front.start..front.end].join("\n"));
        out.push_str("\n\n");
    }
    for (i, line) in lines.iter().enumerate().take(chart.last).skip(chart.first) {
        if i == chart.line {
            out.push_str(&line[..chart.value.start]);
            out.push_str(kind);
            out.push_str(&line[chart.value.end..]);
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

/// What the fitting types of a chart depend on: everything it is asked, but its type.
pub(crate) fn key(source: &str, chart: &Chart) -> String {
    alone(source, chart, "")
}

/// Find an encoded column that is absent from its chart's resolved data before drawing it.
pub(crate) fn missing_chart_field(source: &str, options: &RenderOptions) -> Option<String> {
    let document = richview_core::document::parse(source).ok()?;
    for block in document.blocks.iter().filter(|block| block.tag == "chart") {
        let fields = richview_core::authoring_fields(block, &document.frontmatter, options);
        let Some(encoding) = block
            .value
            .get("encoding")
            .and_then(|value| value.as_object())
        else {
            continue;
        };
        for value in encoding.values() {
            let values: Vec<&str> = match value {
                serde_json::Value::String(field) => vec![field],
                serde_json::Value::Array(items) => {
                    items.iter().filter_map(|item| item.as_str()).collect()
                }
                _ => Vec::new(),
            };
            if let Some(missing) = values
                .into_iter()
                .find(|field| !fields.iter().any(|name| name == field))
            {
                return Some(missing.to_owned());
            }
        }
    }
    None
}

/// Whether `chart` as written draws without an error. A chart that does not, such as one that
/// names a column the data lacks, is fixed before its type is changed: the types are not tried
/// against it at all.
pub(crate) fn sound(source: &str, chart: &Chart, options: &RenderOptions) -> bool {
    let source = alone(source, chart, &chart.kind);
    if missing_chart_field(&source, options).is_some() {
        return false;
    }
    richview_core::document::render(&source, options).is_ok_and(|report| {
        report
            .diagnostics
            .iter()
            .all(|d| d.level != "error" || !about_chart(d))
    })
}

/// Whether the core draws `chart` as `kind` and has nothing to say about it. An error means the
/// data or encoding does not suit the type; a warning means the picture would be unreadable, such
/// as a pie of seventy years.
pub(crate) fn fits(source: &str, chart: &Chart, kind: &str, options: &RenderOptions) -> bool {
    let source = alone(source, chart, kind);
    if missing_chart_field(&source, options).is_some() {
        return false;
    }
    richview_core::document::render(&source, options).is_ok_and(|report| {
        report
            .diagnostics
            .iter()
            .all(|d| d.level == "info" || !about_chart(d))
    })
}

/// Whether a finding is about the chart rather than the report around it, such as a style file
/// the reader has broken: that would refuse every type alike.
fn about_chart(finding: &richview_core::document::Diagnostic) -> bool {
    finding.tag.as_deref() == Some("chart")
        || finding.code.starts_with("chart.")
        || finding.path.starts_with("chart")
        || finding.code == "block.schema"
}

/// The types worth asking about `chart`: the registry's, in its order, unless the chart already
/// draws with an error. The host asks each with [`fits`] and keeps the ones that do.
pub(crate) fn candidates(
    source: &str,
    chart: &Chart,
    options: &RenderOptions,
) -> Vec<&'static str> {
    if sound(source, chart, options) {
        richview_core::CHART_TYPES.to_vec()
    } else {
        vec![]
    }
}

/// Where a caret at `at` is after `edit`: before it stays, after it moves with the text, and within
/// the replaced stretch it goes to the end of the new text.
pub(crate) fn remap(at: usize, edit: &Edit) -> usize {
    let length = edit.text.encode_utf16().count();
    if at <= edit.start {
        at
    } else if at >= edit.end {
        at - (edit.end - edit.start) + length
    } else {
        edit.start + length
    }
}

/// A block the reader can add.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Add {
    Chart,
    Table,
    Metric,
    Callout,
}

impl Add {
    pub(crate) fn named(name: &str) -> Option<Self> {
        Some(match name {
            "chart" => Self::Chart,
            "table" => Self::Table,
            "metric" => Self::Metric,
            "callout" => Self::Callout,
            _ => return None,
        })
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Chart => "chart",
            Self::Table => "table",
            Self::Metric => "metric",
            Self::Callout => "callout",
        }
    }

    /// The block, in the example's own words and data: the CSV the page brings and the figures
    /// the report already quotes from it.
    pub(crate) fn block(self) -> &'static str {
        match self {
            Self::Chart => concat!(
                "```chart\n",
                "type: strip\n",
                "title: Asia’s life expectancy ranged the most, 34 years\n",
                "subtitle: Life expectancy at birth in every year from 1950 to 2019, by world region\n",
                "data:\n",
                "  csv: data/life-expectancy-by-region.csv\n",
                "encoding: { x: region, y: life }\n",
                "axes:\n",
                "  y: { label: Years }\n",
                "```\n",
            ),
            Self::Table => concat!(
                "```table\n",
                "title: Life expectancy in the latest two years\n",
                "data:\n",
                "  csv: data/life-expectancy-by-region.csv\n",
                "columns:\n",
                "  - { key: year, label: Year, format: year }\n",
                "  - { key: region, label: Region }\n",
                "  - { key: life, label: Years, align: right }\n",
                "sort: { key: year, dir: desc }\n",
                "max_rows: 8\n",
                "```\n",
            ),
            Self::Metric => concat!(
                "```metric\n",
                "value: 78.8\n",
                "label: Years a newborn in Europe could expect to live, 2019\n",
                "delta: 0.22\n",
                "delta_label: since 1950\n",
                "```\n",
            ),
            Self::Callout => concat!(
                "```callout\n",
                "kind: down\n",
                "title: The dip around 1960\n",
                "body: Asia’s life expectancy fell to 40.7 years in 1960, the lowest of any region in any year, during the Great Chinese Famine of 1959–61.\n",
                "```\n",
            ),
        }
    }
}

/// Where `block` goes in `source`, and the line it begins on, counted from 1. A block stands
/// apart from the text around it: the caret in a paragraph, a fenced block or the front matter
/// puts it after that one, the caret at the start of a paragraph or on a blank line puts it
/// there, and without a caret it goes at the end.
pub(crate) fn insertion(source: &str, caret: Option<usize>, block: &str) -> (Edit, usize) {
    let lines: Vec<&str> = source.split('\n').collect();
    let total = source.encode_utf16().count();
    let line = caret.map_or(lines.len(), |caret| {
        let (line, character) = position(source, caret);
        match blocks(source)
            .into_iter()
            .find(|b| (b.start..b.end).contains(&line))
        {
            Some(b) if b.kind != Kind::Front && line == b.start && character == 0 => b.start,
            Some(b) => b.end,
            None => line,
        }
    });
    // Past the last line is the end of the source.
    let at = if line >= lines.len() {
        total
    } else {
        offset(source, line, 0)
    };
    let (before, after) = source.split_at(byte(source, at));
    let lead = match before {
        "" => "",
        text if text.ends_with("\n\n") => "",
        text if text.ends_with('\n') => "\n",
        _ => "\n\n",
    };
    let trail = if after.is_empty() || after.starts_with('\n') {
        ""
    } else {
        "\n"
    };
    let first = before.matches('\n').count() + lead.matches('\n').count() + 1;
    (
        Edit {
            start: at,
            end: at,
            text: format!("{lead}{block}{trail}"),
        },
        first,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANCHOR: &str = include_str!("../../../assets/landing/anchor/report.rmd");
    const STYLE: &str = include_str!("../../../assets/landing/anchor/report.rstyle.yaml");
    const DATA: &str =
        include_str!("../../../assets/landing/anchor/data/life-expectancy-by-region.csv");

    /// The options the live example renders with: its folder, its style file and the CSV the page
    /// brings.
    fn options() -> RenderOptions {
        let mut options = RenderOptions {
            report_folder: Some(richview_core::document::ReportFolder {
                root: "homepage-demo".into(),
                source: "report.rmd".into(),
                files: vec![
                    "report.rmd".into(),
                    "report.rstyle.yaml".into(),
                    "data/life-expectancy-by-region.csv".into(),
                ],
            }),
            ..Default::default()
        };
        options
            .data
            .insert("report.rstyle.yaml".into(), STYLE.into());
        options
            .data
            .insert("data/life-expectancy-by-region.csv".into(), DATA.into());
        options
    }

    /// The source with `edit` made.
    fn apply(source: &str, edit: &Edit) -> String {
        format!(
            "{}{}{}",
            &source[..byte(source, edit.start)],
            edit.text,
            &source[byte(source, edit.end)..]
        )
    }

    /// The types that fit, as the host asks them.
    fn fitting(source: &str, chart: &Chart, options: &RenderOptions) -> Vec<&'static str> {
        candidates(source, chart, options)
            .into_iter()
            .filter(|kind| fits(source, chart, kind, options))
            .collect()
    }

    #[test]
    fn missing_encoding_columns_are_stopped_before_rendering() {
        let missing = ANCHOR.replace("y: life", "y: lif");
        let chart = chart_at(&missing, None).unwrap();
        assert_eq!(
            missing_chart_field(&missing, &options()).as_deref(),
            Some("lif")
        );
        assert!(!sound(&missing, &chart, &options()));
        assert!(!fits(&missing, &chart, "line", &options()));
        assert!(candidates(&missing, &chart, &options()).is_empty());
        assert_eq!(missing_chart_field(ANCHOR, &options()), None);
    }

    #[test]
    fn column_guard_reads_each_charts_own_inline_csv_and_json_data() {
        let mut options = RenderOptions::default();
        options
            .data
            .insert("quoted.csv".into(), "\"time,year\",value\n2020,3\n".into());
        options
            .data
            .insert("rows.json".into(), r#"[{"year":2020},{"value":3}]"#.into());
        let inline = "```chart\ntype: line\ntitle: T\ndata: {inline: [{year: 2020}, {value: 3}]}\nencoding: {x: year, y: value, detail: [year, value]}\n```\n";
        let csv = "```chart\ntype: line\ntitle: T\ndata: {csv: quoted.csv}\nencoding: {x: 'time,year', y: value}\n```\n";
        let json = "```chart\ntype: line\ntitle: T\ndata: {json: rows.json}\nencoding: {x: year, y: value}\n```\n";
        assert_eq!(
            missing_chart_field(&format!("{inline}{csv}{json}"), &options),
            None
        );
        assert_eq!(
            missing_chart_field(
                &format!("{inline}{}", csv.replace("y: value", "y: missing")),
                &options
            )
            .as_deref(),
            Some("missing")
        );
        assert_eq!(
            missing_chart_field(
                &inline.replace("detail: [year, value]", "detail: [year, nope]"),
                &options
            )
            .as_deref(),
            Some("nope")
        );
        assert_eq!(
            missing_chart_field(&json.replace("y: value", "y: nope"), &options).as_deref(),
            Some("nope")
        );
    }

    #[test]
    fn the_anchor_has_one_chart_and_its_type_is_found() {
        let found = charts(ANCHOR);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].kind, "line");
        assert_eq!(chart_at(ANCHOR, None), chart_at(ANCHOR, Some(40)));
    }

    #[test]
    fn a_type_is_read_whatever_its_quotes_comment_or_spacing() {
        for (line, value) in [
            ("type: line", Some("line")),
            ("type:   bar   ", Some("bar")),
            ("type: \"area\" # a comment", Some("area")),
            ("type: 'pie'", Some("pie")),
            ("type: scatter # a comment", Some("scatter")),
            ("type:", None),
            ("  type: line", None),
            ("title: type: line", None),
        ] {
            let found = type_value(line).map(|r| &line[r]);
            assert_eq!(found, value, "{line:?}");
        }
    }

    #[test]
    fn retyping_changes_the_value_alone_and_keeps_the_rest() {
        let chart = chart_at(ANCHOR, None).unwrap();
        let edit = retype(ANCHOR, &chart, "area");
        let next = apply(ANCHOR, &edit);
        assert_eq!(next, ANCHOR.replacen("type: line", "type: area", 1));
        assert_eq!(chart_at(&next, None).unwrap().kind, "area");
        // A quoted type keeps its quotes and its comment.
        let source = "```chart\ntype: \"line\" # why\ntitle: T\n```\n";
        let chart = chart_at(source, None).unwrap();
        assert_eq!(
            apply(source, &retype(source, &chart, "bar")),
            "```chart\ntype: \"bar\" # why\ntitle: T\n```\n"
        );
    }

    #[test]
    fn the_caret_picks_the_chart_it_is_in_and_the_first_otherwise() {
        let source = format!("{ANCHOR}\n{}", Add::Chart.block());
        let all = charts(&source);
        assert_eq!(all.len(), 2);
        assert_eq!(chart_at(&source, Some(all[1].first + 2)).unwrap(), all[1]);
        assert_eq!(chart_at(&source, Some(0)).unwrap(), all[0]);
        assert_eq!(chart_at(&source, Some(all[0].last + 1)).unwrap(), all[0]);
        assert!(chart_at("No chart here.\n", None).is_none());
    }

    #[test]
    fn the_examples_chart_offers_only_types_that_fit_its_data() {
        let chart = chart_at(ANCHOR, None).unwrap();
        let fitting = fitting(ANCHOR, &chart, &options());
        // Year, life expectancy and region suit these. The rest lack a channel the data has no
        // column for (a pie takes no colour, an interval needs bounds) or draw hundreds of
        // overlapping labels, as a waterfall does with a bar for each of seventy years.
        assert_eq!(
            fitting,
            [
                "line",
                "bar",
                "scatter",
                "area",
                "boxplot",
                "streamgraph",
                "ribbon",
                "lollipop",
                "violin",
                "strip"
            ]
        );
        for refused in [
            "pie",
            "heatmap",
            "waterfall",
            "radar",
            "rose",
            "waffle",
            "bar3d",
        ] {
            assert!(!fitting.contains(&refused), "{refused}");
        }
    }

    #[test]
    fn what_fits_follows_the_charts_encoding() {
        // Region against life holds one value per year: a distribution, not a series.
        let source = format!("{ANCHOR}\n{}", Add::Chart.block());
        let chart = charts(&source).pop().unwrap();
        let fitting = fitting(&source, &chart, &options());
        println!("FITS {fitting:?}");
        assert!(fitting.contains(&"strip") && fitting.contains(&"violin"));
        assert!(!fitting.contains(&"lollipop") && !fitting.contains(&"boxplot"));
    }

    #[test]
    fn a_chart_that_already_breaks_is_not_tried_against_the_types() {
        // A channel the type needs, left out.
        let missing = ANCHOR.replace(
            "encoding: { x: year, y: life, color: region }",
            "encoding: { x: year }",
        );
        let chart = chart_at(&missing, None).unwrap();
        assert!(!sound(&missing, &chart, &options()));
        assert!(fitting(&missing, &chart, &options()).is_empty());
        // A style file the reader has broken is not the chart's fault.
        let mut options = options();
        options
            .data
            .insert("report.rstyle.yaml".into(), "{{{".into());
        let chart = chart_at(ANCHOR, None).unwrap();
        assert!(sound(ANCHOR, &chart, &options));
        assert!(fits(ANCHOR, &chart, "area", &options));
    }

    #[test]
    fn a_caret_follows_the_edit_it_is_around() {
        let source = "type: line # x\n";
        let chart = chart_at("```chart\ntype: line\n```\n", None).unwrap();
        let edit = retype("```chart\ntype: line\n```\n", &chart, "scatter");
        assert_eq!((edit.start, edit.end), (15, 19));
        assert_eq!(remap(3, &edit), 3);
        assert_eq!(remap(17, &edit), 15 + 7);
        assert_eq!(remap(19, &edit), 15 + 7);
        assert_eq!(remap(25, &edit), 25 - 4 + 7);
        assert_eq!(source.len(), 15);
    }

    #[test]
    fn the_key_ignores_the_type_and_follows_the_rest() {
        let chart = chart_at(ANCHOR, None).unwrap();
        let area = apply(ANCHOR, &retype(ANCHOR, &chart, "area"));
        let other = chart_at(&area, None).unwrap();
        assert_eq!(key(ANCHOR, &chart), key(&area, &other));
        let retitled = ANCHOR.replace("Every region", "Each region");
        assert_ne!(
            key(ANCHOR, &chart),
            key(&retitled, &chart_at(&retitled, None).unwrap())
        );
    }

    #[test]
    fn every_block_renders_in_the_example_without_a_finding() {
        for add in [Add::Chart, Add::Table, Add::Metric, Add::Callout] {
            let (edit, _) = insertion(ANCHOR, None, add.block());
            let next = apply(ANCHOR, &edit);
            let report = richview_core::document::render(&next, &options())
                .unwrap_or_else(|d| panic!("{}: {d:?}", add.name()));
            let found: Vec<_> = report
                .diagnostics
                .iter()
                .filter(|d| d.level != "info")
                .map(|d| format!("{}: {}", d.code, d.message))
                .collect();
            assert!(found.is_empty(), "{}: {found:?}", add.name());
            assert!(next.ends_with(add.block()), "{}", add.name());
        }
    }

    /// The CSV's value for a region and year.
    fn life(region: &str, year: u32) -> f64 {
        DATA.lines()
            .find_map(|line| {
                let mut cells = line.split(',');
                let (y, r, v) = (cells.next()?, cells.next()?, cells.next()?);
                (y == year.to_string() && r == region).then(|| v.parse().unwrap())
            })
            .unwrap_or_else(|| panic!("{region} {year}"))
    }

    /// The number a block gives `key`.
    fn number(block: &str, key: &str) -> f64 {
        block
            .lines()
            .find_map(|line| line.strip_prefix(key)?.strip_prefix(':'))
            .unwrap_or_else(|| panic!("{key}"))
            .trim()
            .parse()
            .unwrap()
    }

    #[test]
    fn the_figures_in_the_added_blocks_are_the_csv_s() {
        let metric = Add::Metric.block();
        assert_eq!(number(metric, "value"), life("Europe", 2019));
        // The delta is a share: 14.2 years on 64.6 is 22%.
        let gain = life("Europe", 2019) / life("Europe", 1950) - 1.0;
        assert!((number(metric, "delta") - gain).abs() < 0.005, "{gain}");
        // Asia's 1960 is the lowest value in the file.
        let lowest = DATA
            .lines()
            .skip(1)
            .map(|line| line.split(',').nth(2).unwrap().parse::<f64>().unwrap())
            .fold(f64::MAX, f64::min);
        assert_eq!(life("Asia", 1960), lowest);
        assert!(
            Add::Callout
                .block()
                .contains(&format!("{lowest} years in 1960"))
        );
        // Asia's yearly values span more than any other region's: 34 years.
        let range = |region: &str| {
            let all: Vec<f64> = (1950..=2019).map(|year| life(region, year)).collect();
            all.iter().cloned().fold(f64::MIN, f64::max)
                - all.iter().cloned().fold(f64::MAX, f64::min)
        };
        for other in ["Africa", "Americas", "Europe"] {
            assert!(range(other) < range("Asia"), "{other}");
        }
        assert_eq!(range("Asia").round(), 34.0);
        assert!(Add::Chart.block().contains("ranged the most, 34 years"));
    }

    #[test]
    fn a_block_without_a_caret_goes_at_the_end_after_a_blank_line() {
        let (edit, line) = insertion(ANCHOR, None, "```x\n```\n");
        let next = apply(ANCHOR, &edit);
        assert!(next.starts_with(ANCHOR));
        assert!(next[ANCHOR.len()..].starts_with("\n```x"), "{next:?}");
        assert!(next.ends_with("```x\n```\n"));
        assert_eq!(line, ANCHOR.matches('\n').count() + 2);
        assert_eq!(next.split('\n').nth(line - 1), Some("```x"));
        // No trailing newline, and an empty source.
        let (edit, _) = insertion("Text", None, "B\n");
        assert_eq!(apply("Text", &edit), "Text\n\nB\n");
        let (edit, line) = insertion("", None, "B\n");
        assert_eq!((apply("", &edit), line), ("B\n".to_owned(), 1));
    }

    #[test]
    fn the_caret_decides_where_a_block_goes_without_breaking_what_it_is_in() {
        let source =
            "---\ntitle: T\n---\n\nFirst line\nsecond line\n\n```table\na: 1\n```\n\nLast.\n";
        let at = |needle: &str, shift: usize| {
            source[..source.find(needle).unwrap() + shift]
                .encode_utf16()
                .count()
        };
        let put = |caret: usize| apply(source, &insertion(source, Some(caret), "B\n").0);
        // In a paragraph: after the paragraph, not between its lines.
        assert_eq!(
            put(at("second", 3)),
            source.replace("second line\n\n", "second line\n\nB\n\n")
        );
        // At the start of a paragraph: before it.
        assert_eq!(
            put(at("First", 0)),
            source.replace("First line", "B\n\nFirst line")
        );
        // In a fenced block: after its closing fence.
        assert_eq!(
            put(at("a: 1", 2)),
            source.replace("```\n\nLast", "```\n\nB\n\nLast")
        );
        // In the front matter, and at the very start of the file: after the front matter.
        for caret in [0, at("title", 2)] {
            assert_eq!(
                put(caret),
                source.replace("---\n\nFirst", "---\n\nB\n\nFirst"),
                "{caret}"
            );
        }
        // On a blank line: there.
        assert_eq!(
            put(at("\n\n```table", 1)),
            source.replace("\n\n```table", "\n\nB\n\n```table")
        );
        // At the end of the last paragraph.
        assert_eq!(put(source.encode_utf16().count()), format!("{source}\nB\n"));
    }

    #[test]
    fn a_caret_after_non_ascii_text_is_counted_in_text_area_units() {
        let source = "Čau 📈 světe\n\nKonec.\n";
        let caret = "Čau 📈".encode_utf16().count();
        let (edit, _) = insertion(source, Some(caret), "B\n");
        assert_eq!(apply(source, &edit), "Čau 📈 světe\n\nB\n\nKonec.\n");
        assert_eq!(edit.start, "Čau 📈 světe\n".encode_utf16().count());
    }
}
