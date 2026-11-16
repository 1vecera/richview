//! Source column order is kept end to end (TYP-25). Each source form records it the same way:
//! - a CSV header gives its column order;
//! - JSON rows, from a file (an array of row objects) or a fence written in YAML or JSON, give their first row's key order,
//!   followed by later rows' new keys in first-seen order.
//!
//! The order then drives chart payloads, which feed the Table view and CSV download, and derived
//! table columns.

use richview_core::render_compat;
use serde_json::{Value, json};

fn report(source: &str, files: &[(&str, &str)]) -> Value {
    let mut listed = vec!["report.rmd".to_owned()];
    let mut data = serde_json::Map::new();
    for (path, text) in files {
        listed.push((*path).to_owned());
        data.insert((*path).to_owned(), json!(text));
    }
    render_compat(
        source,
        &json!({
            "report_folder": {"root": "column-order", "source": "report.rmd", "files": listed},
            "data": data,
        }),
    )
    .unwrap()
}

fn recorded(output: &Value, block: usize) -> Vec<String> {
    output
        .pointer(&format!("/parsed/blocks/{block}/value/data/columns"))
        .and_then(Value::as_array)
        .map(|keys| {
            keys.iter()
                .map(|k| k.as_str().unwrap().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// The `columns` of every chart payload in the HTML, in document order.
fn payload_columns(output: &Value) -> Vec<Vec<String>> {
    let html = output["html"].as_str().expect("html");
    html.match_indices("data-rv-payload")
        .map(|(at, _)| {
            let start = at + html[at..].find('>').unwrap() + 1;
            let end = start + html[start..].find("</script>").unwrap();
            let payload: Value = serde_json::from_str(&html[start..end]).unwrap();
            payload["columns"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c.as_str().unwrap().to_owned())
                .collect()
        })
        .collect()
}

const ROWS: &str =
    r#"[{"zeta": "a", "alpha": 1, "mid": 2}, {"zeta": "b", "alpha": 3, "mid": 4, "late": "x"}]"#;

#[test]
fn every_source_form_records_its_column_order() {
    let yaml = "```chart\ntype: bar\ntitle: YAML rows\nencoding: {x: zeta, y: alpha, detail: [late, mid]}\ndata:\n  inline:\n    - {zeta: a, alpha: 1, mid: 2}\n    - {zeta: b, alpha: 3, mid: 4, late: x}\n```\n";
    let json_fence = format!(
        "```chart\n{{\"type\": \"bar\", \"title\": \"JSON rows\", \"encoding\": {{\"x\": \"zeta\", \"y\": \"alpha\", \"detail\": [\"late\", \"mid\"]}}, \"data\": {{\"inline\": {ROWS}}}}}\n```\n"
    );
    let file = "```chart\ntype: bar\ntitle: JSON file\nencoding: {x: zeta, y: alpha, detail: [late, mid]}\ndata: {json: data/rows.json}\n```\n";
    let csv = "```chart\ntype: bar\ntitle: CSV file\nencoding: {x: zeta, y: alpha, detail: [late, mid]}\ndata: {csv: data/rows.csv}\n```\n";
    let source = [yaml, &json_fence, file, csv].join("\n");
    let output = report(
        &source,
        &[
            ("data/rows.json", ROWS),
            (
                "data/rows.csv",
                "zeta,alpha,mid,late\r\na,1,2,\r\nb,3,4,x\r\n",
            ),
        ],
    );
    // The encoding names `late` before `mid`, so a payload that fell back to the encoding's
    // order would differ.
    let order = ["zeta", "alpha", "mid", "late"];
    for block in 0..4 {
        assert_eq!(recorded(&output, block), order, "block {block}");
    }
    let payloads = payload_columns(&output);
    assert_eq!(payloads.len(), 4);
    for (block, columns) in payloads.iter().enumerate() {
        assert_eq!(columns, &order, "payload {block}");
    }
}

#[test]
fn a_chart_keeps_source_order_even_when_its_encoding_names_fields_in_another_order() {
    let source = "```chart\ntype: heatmap\ntitle: Cells\nencoding: {x: c, y: note, color: y}\ndata:\n  inline:\n    - {x: 1, c: P, y: 2, note: first}\n    - {x: 2, c: Q, y: 3, note: second}\n```\n";
    let output = report(source, &[]);
    assert_eq!(payload_columns(&output), [["c", "y", "note"]]);
}

#[test]
fn derived_table_columns_follow_the_source() {
    let source = "```table\ndata:\n  inline:\n    - {zip: '10115', name: Berlin, flag: true, amount: 3}\n    - {zip: '10117', name: Mitte, flag: false, amount: 4, note: new}\n    - {amount: 5, zip: '10119', name: Prenzlauer Berg, flag: true}\n```\n\n```table\ndata: {csv: data/places.csv}\n```\n\n```table\ndata: {json: data/places.json}\n```\n";
    let output = report(
        source,
        &[
            (
                "data/places.csv",
                "zip,name,flag,amount\n10115,Berlin,true,3\n",
            ),
            (
                "data/places.json",
                r#"[{"zip": "10115", "name": "Berlin"}, {"zip": "10117", "amount": 4, "name": "Mitte"}]"#,
            ),
        ],
    );
    let html = output["html"].as_str().unwrap();
    // Each derived table's sort buttons name its columns in order.
    let headers: Vec<Vec<&str>> = html
        .match_indices("<thead>")
        .map(|(at, _)| {
            let head = &html[at..at + html[at..].find("</thead>").unwrap()];
            head.match_indices("data-rv-key=\"")
                .map(|(i, m)| {
                    let key = &head[i + m.len()..];
                    &key[..key.find('"').unwrap()]
                })
                .collect()
        })
        .collect();
    assert_eq!(headers.len(), 3, "{html}");
    // Inline rows, a later one with its keys reordered, then a CSV header and JSON rows.
    assert_eq!(headers[0], ["zip", "name", "flag", "amount", "note"]);
    assert_eq!(headers[1], ["zip", "name", "flag", "amount"]);
    assert_eq!(headers[2], ["zip", "name", "amount"]);
}

#[test]
fn a_column_no_channel_or_table_column_names_never_reaches_the_html() {
    // A chart ships only the fields its encoding names, and a table with `columns` only those
    // columns (TYP-36): the reader never gets source data the author did not show.
    let csv = "region,value,salary\nNorth,1,SECRET-81723\nSouth,2,SECRET-99001\n";
    let source = "```chart\ntype: bar\ntitle: Value by region\nencoding: {x: region, y: value}\ndata: {csv: data/rows.csv}\n```\n\n```chart\ntype: line\ntitle: Inline rows\nencoding: {x: region, y: value}\ndata:\n  inline:\n    - {region: North, value: 1, salary: SECRET-81723}\n    - {region: South, value: 2, salary: SECRET-99001}\n```\n\n```table\ndata: {csv: data/rows.csv}\ncolumns: [{key: region}, {key: value}]\n```\n";
    let output = report(source, &[("data/rows.csv", csv)]);
    let html = output["html"].as_str().unwrap();
    assert_eq!(
        payload_columns(&output),
        [["region", "value"], ["region", "value"]]
    );
    assert!(!html.contains("SECRET"), "an unreferenced value shipped");
    assert!(
        !html.contains("salary"),
        "an unreferenced column name shipped"
    );
    // Control: a table without `columns` shows every column, so the scan sees a shown value.
    let every = report(
        "```table\ndata: {csv: data/rows.csv}\n```\n",
        &[("data/rows.csv", csv)],
    );
    assert!(every["html"].as_str().unwrap().contains("SECRET-81723"));
}
