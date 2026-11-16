//! The chart's rows for the Table view and the CSV download (FEA-CH-45, FEA-CH-48): every
//! resolved row the payload embeds, in source column order, numbers at the readout precision the
//! payload already rounded to and never more.

use crate::payload::Chart;
use richview_chart::block::Axis;
use richview_chart::numbers::{self, Format, Kind, Locale};
use serde_json::Value;

/// One CSV field with standard quoting.
fn csv_field(text: &str) -> String {
    if text.contains([',', '"', '\n', '\r']) || text.starts_with(' ') || text.ends_with(' ') {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_owned()
    }
}

fn csv_value(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n
            .as_f64()
            .map(numbers::csv_number)
            .unwrap_or_else(|| n.to_string()),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// The CSV file: UTF-8, a header row, one line per resolved row (FEA-CH-48).
pub fn csv(chart: &Chart) -> String {
    let shown = chart.payload.shown_rows();
    let mut out = String::new();
    out.push_str(
        &chart
            .payload
            .columns
            .iter()
            .map(|c| csv_field(c))
            .collect::<Vec<_>>()
            .join(","),
    );
    out.push_str("\r\n");
    for row in &shown {
        out.push_str(
            &row.iter()
                .map(|v| csv_field(&csv_value(v)))
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push_str("\r\n");
    }
    out
}

/// The format a column's numbers use in the Table: the channel's readout format, years plain.
fn column_format(chart: &Chart, column: &str, locale: Locale) -> Format {
    let shown = chart.payload.shown_rows();
    let b = &chart.block;
    let e = &b.encoding;
    let axis: Option<&Axis> = if e.x.as_deref() == Some(column) {
        Some(&b.x)
    } else if [&e.y, &e.lower, &e.upper]
        .iter()
        .any(|f| f.as_deref() == Some(column))
    {
        Some(&b.y)
    } else if e.color.as_deref() == Some(column) {
        Some(&b.color)
    } else {
        None
    };
    let mut format = match axis {
        Some(a) => b.readout_format(a, locale),
        None => Format {
            kind: Kind::Number,
            unit: None,
            digits: b.readout_digits,
            locale,
        },
    };
    let values: Vec<f64> = shown
        .iter()
        .filter_map(|r| {
            chart
                .payload
                .columns
                .iter()
                .position(|c| c == column)
                .and_then(|i| r.get(i))
                .and_then(Value::as_f64)
        })
        .collect();
    if axis.and_then(|a| a.format).is_none()
        && !values.is_empty()
        && numbers::looks_like_years(&values)
        && column.to_ascii_lowercase().contains("year")
    {
        format.kind = Kind::Year;
    }
    format
}

/// The Table view's cells: header and display text per row, numbers right-aligned by the caller.
pub struct Table {
    pub header: Vec<String>,
    pub numeric: Vec<bool>,
    pub rows: Vec<Vec<String>>,
}

pub fn table(chart: &Chart) -> Table {
    let shown = chart.payload.shown_rows();
    let locale = Locale::from_tag(if chart.payload.lang.is_empty() {
        "en"
    } else {
        &chart.payload.lang
    });
    let formats: Vec<Format> = chart
        .payload
        .columns
        .iter()
        .map(|c| column_format(chart, c, locale))
        .collect();
    let numeric: Vec<bool> = (0..chart.payload.columns.len())
        .map(|i| {
            shown
                .iter()
                .all(|r| matches!(r.get(i), Some(Value::Number(_)) | Some(Value::Null) | None))
                && shown
                    .iter()
                    .any(|r| matches!(r.get(i), Some(Value::Number(_))))
        })
        .collect();
    let rows = shown
        .iter()
        .map(|r| {
            r.iter()
                .zip(&formats)
                .map(|(v, f)| match v {
                    Value::Null => String::new(),
                    Value::Number(n) => {
                        n.as_f64().map(|x| numbers::value(x, f)).unwrap_or_default()
                    }
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .collect()
        })
        .collect();
    Table {
        header: chart.payload.columns.clone(),
        numeric,
        rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payload::Payload;
    use serde_json::json;

    fn chart() -> Chart {
        Chart::from_payload(Payload {
            block: json!({"type": "line", "title": "T", "encoding": {"x": "year", "y": "sales"}, "axes": {"y": {"unit": "$"}}}),
            columns: vec!["year".into(), "sales".into(), "note".into()],
            rows: vec![
                vec![json!(2024), json!(1234567.0), json!("first, \"big\"")],
                vec![json!(2025), Value::Null, json!({"a": 1})],
            ],
            lang: "en".into(),
            ..Payload::default()
        })
        .unwrap()
    }

    #[test]
    fn csv_quotes_text_and_writes_plain_numbers() {
        let text = csv(&chart());
        let lines: Vec<&str> = text.split("\r\n").collect();
        assert_eq!(lines[0], "year,sales,note");
        assert_eq!(lines[1], "2024,1234567,\"first, \"\"big\"\"\"");
        assert_eq!(lines[2], "2025,,\"{\"\"a\"\":1}\"");
    }

    #[test]
    fn the_table_reads_like_the_readout() {
        let t = table(&chart());
        assert_eq!(t.rows[0][0], "2024", "years stay plain");
        assert!(t.rows[0][1].starts_with('$'), "{}", t.rows[0][1]);
        assert_eq!(t.rows[1][1], "", "missing stays empty");
        assert_eq!(t.numeric, vec![true, true, false]);
    }

    #[test]
    fn quantized_columns_read_at_the_authors_precision_in_the_table_and_csv() {
        let chart = Chart::from_payload(Payload {
            block: json!({"type": "bar", "title": "T", "encoding": {"x": "k", "y": "v"}}),
            columns: vec!["k".into(), "v".into()],
            rows: vec![vec![json!("a"), json!(100.01611721611722)]],
            shown: [("v".to_owned(), vec![json!(100)])].into(),
            ..Payload::default()
        })
        .unwrap();
        assert_eq!(csv(&chart).split("\r\n").nth(1), Some("a,100"));
        assert_eq!(table(&chart).rows[0][1], "100");
    }
}
