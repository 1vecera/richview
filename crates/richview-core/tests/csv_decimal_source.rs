use super::common;

use richview_core::{render_compat, render_table};

use serde_json::{Value, json};

fn report(block: &str, csv: &str) -> Value {
    render_compat(
        block,
        &json!({
            "report_folder":{"root":"csv-decimals","source":"report.rmd","files":["report.rmd","data/readings.csv","data/readings.json"]},
            "data":{"data/readings.csv":csv,"data/readings.json":csv},
        }),
    ).unwrap()
}

#[test]
fn resolved_csv_retains_only_decimal_values_that_number_serialization_would_change() {
    let fixtures: Value =
        serde_json::from_str(include_str!("../../../scripts/fixtures/csv-decimals.json")).unwrap();
    let output = report(
        "```chart\ntype: scatter\ntitle: T\ndata: {csv: data/readings.csv}\nencoding: {x: id, y: amount}\n```\n",
        fixtures["csv"].as_str().unwrap(),
    );
    let rows = output
        .pointer("/parsed/blocks/0/value/data/inline")
        .unwrap()
        .as_array()
        .unwrap();
    for (row, case) in rows.iter().zip(fixtures["cases"].as_array().unwrap()) {
        assert_eq!(row["cell"], case["expected"], "{}", case["name"]);
        assert_eq!(
            row["cell"].is_string(),
            case["kind"] == "string",
            "{}",
            case["name"]
        );
    }
    assert_eq!(rows.len(), fixtures["cases"].as_array().unwrap().len());
}

/// The chart payloads embedded in a report's HTML.
fn payloads(html: &str) -> Vec<Value> {
    html.split("data-rv-payload>")
        .skip(1)
        .map(|rest| serde_json::from_str(rest.split("</script>").next().unwrap()).unwrap())
        .collect()
}

#[test]
fn resolved_csv_reaches_each_temporal_facet_at_the_authors_precision() {
    let source = "```chart\ntype: line\ntitle: T\ndata: {csv: data/readings.csv}\nencoding: {x: date, y: value, color: group, facet: panel}\n```\n";
    let values = [
        ("First", "1.234567890123456789", "1.23"),
        ("Second", "9.876543210987654321", "9.88"),
    ];
    let mut csv = "panel,date,group,value\n".to_owned();
    for panel in ["Left", "Right"] {
        for date in ["2024-03", "2024-01"] {
            for (group, value, _) in values {
                csv.push_str(&format!("{panel},{date},{group},{value}\n"));
            }
        }
    }
    let output = report(source, &csv);
    let html = output["html"].as_str().unwrap();
    // FEA-CH-37: readers never get more precision than the author shows.
    for (_, exact, _) in values {
        assert!(!html.contains(exact), "{exact} never reaches the HTML");
    }
    let payloads = payloads(html);
    assert_eq!(payloads.len(), 1);
    let g = common::geometry(&payloads[0], 720.0);
    assert_eq!(g.panels.len(), 2, "one panel per facet");
    for panel in &g.panels {
        let first = &panel.observations[0].readout;
        for (name, _, shown) in values {
            let line = first.lines.iter().find(|l| l.name == name).unwrap();
            assert!(line.value.starts_with(shown), "{name}: {}", line.value);
        }
    }
}

/// The value each sortable cell carries for the runtime's sort: the one the cell shows, at the
/// author's precision (FEA-CH-37).
fn sort_values(html: &str) -> Vec<String> {
    html.split("<tbody>")
        .nth(1)
        .unwrap()
        .split("</tbody>")
        .next()
        .unwrap()
        .split("data-rv-sort-value=\"")
        .skip(1)
        .map(|rest| rest.split('"').next().unwrap().to_owned())
        .collect()
}

fn cells(html: &str) -> Vec<String> {
    let body = html
        .split("<tbody>")
        .nth(1)
        .unwrap()
        .split("</tbody>")
        .next()
        .unwrap();
    body.split("<td ")
        .skip(1)
        .map(|cell| {
            cell.split_once('>')
                .unwrap()
                .1
                .split("</td>")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect()
}

#[test]
fn csv_table_sorts_near_equal_decimals_and_large_integers_by_exact_value() {
    // The author's `sort:` orders rows by the exact source value. The runtime's sort value is
    // the one each cell shows, so cells that show the same number tie (FEA-CH-37).
    for (values, shown) in [
        (
            [
                "1.234567890123456789",
                "1.234567890123456788",
                "1.234567890123456790",
            ],
            "1.23",
        ),
        (
            ["9007199254740993", "9007199254740992", "9007199254740994"],
            "9010000000000000",
        ),
        (
            [
                "-1.234567890123456789",
                "-1.234567890123456790",
                "-1.234567890123456788",
            ],
            "-1.23",
        ),
    ] {
        let csv = format!(
            "id,value\na,{}\nb,{}\nc,{}\n",
            values[0], values[1], values[2]
        );
        let output = report(
            "```table\ndata: {csv: data/readings.csv}\ncolumns: [{key: id}, {key: value}]\nsort: {key: value, dir: asc}\n```\n",
            &csv,
        );
        let mut table = output["parsed"]["blocks"][0]["value"].clone();
        let ids = |table: &Value| {
            cells(&render_table(table))
                .into_iter()
                .step_by(2)
                .collect::<Vec<_>>()
        };
        // The second observation is the smallest, including negative values.
        assert_eq!(ids(&table), ["b", "a", "c"]);
        assert_eq!(sort_values(&render_table(&table)), [shown; 3]);
        table["sort"]["dir"] = json!("desc");
        assert_eq!(ids(&table), ["c", "a", "b"]);
    }
}

#[test]
fn csv_table_totals_include_retained_numbers_and_identifiers_remain_text() {
    let output = report(
        "```table\ndata: {csv: data/readings.csv}\ncolumns: [{key: id}, {key: value, format: number}]\ntotal: true\n```\n",
        "id,value\n002,0.100000000000000005\n001,1.899999999999999995\n",
    );
    let table = &output["parsed"]["blocks"][0]["value"];
    let html = render_table(table);
    assert!(
        html.contains("<tfoot><tr><td class=\"\">Total</td><td class=\"num\">2</td></tr></tfoot>")
    );
    let mut ids = table.clone();
    ids["sort"] = json!({"key":"id","dir":"asc"});
    assert_eq!(cells(&render_table(&ids))[0], "001");
    // Inline string columns remain textual, even when their cells look numeric.
    let inline = json!({"data":{"inline":[{"value":"2"},{"value":"10"}]},"sort":{"key":"value","dir":"asc"}});
    assert_eq!(cells(&render_table(&inline)), ["10", "2"]);
}

#[test]
fn mixed_table_sorting_has_a_consistent_order_for_null_numeric_and_text_cells() {
    let original = [Value::Null, json!(2), json!(10), json!("11"), json!("2")];
    for offset in 0..original.len() {
        let values: Vec<_> = original
            .iter()
            .cycle()
            .skip(offset)
            .take(original.len())
            .map(|value| json!({"value":value}))
            .collect();
        let table = json!({"data":{"inline":values},"sort":{"key":"value","dir":"asc"}});
        assert_eq!(cells(&render_table(&table)), ["2", "10", "11", "2", ""]);
    }
}

#[test]
fn the_source_key_controls_numeric_text_semantics_and_json_strings_stay_text() {
    // JSON strings stay text, whatever they look like; CSV cells are typed.
    let source = |reference: &str| {
        format!(
            "```table\ndata: {{{reference}: data/readings.{reference}}}\nsort: {{key: value, dir: asc}}\n```\n"
        )
    };
    let output = report(&source("json"), r#"[{"value":"2"},{"value":"10"}]"#);
    let table = &output["parsed"]["blocks"][0]["value"];
    assert_eq!(cells(&render_table(table)), ["10", "2"]);
    let output = report(&source("csv"), "value\n2\n10\n");
    let table = &output["parsed"]["blocks"][0]["value"];
    assert_eq!(cells(&render_table(table)), ["2", "10"]);
    let output = report(
        &source("csv"),
        "value\n9007199254740993\n9007199254740992\n9007199254740994\n",
    );
    let table = &output["parsed"]["blocks"][0]["value"];
    // CSV text past 2^53 is still a number, shown and sorted at three significant digits
    // (TYP-23, FEA-CH-37).
    assert_eq!(sort_values(&render_table(table)), ["9010000000000000"; 3]);
}

#[test]
fn exact_equivalent_csv_cells_keep_stable_rows_and_existing_descending_order() {
    let output = report(
        "```table\ndata: {csv: data/readings.csv}\ncolumns: [{key: id}, {key: value}]\nsort: {key: value, dir: asc}\n```\n",
        "id,value\nwhole,1\nfraction,1.0\nabove,1.000000000000000000001\nbelow,0.999999999999999999999\nalias,10e-1\n",
    );
    let mut table = output["parsed"]["blocks"][0]["value"].clone();
    let ids = |table: &Value| {
        cells(&render_table(table))
            .into_iter()
            .step_by(2)
            .collect::<Vec<_>>()
    };
    let expected = ["below", "whole", "fraction", "alias", "above"];
    assert_eq!(ids(&table), expected);
    table["sort"]["dir"] = json!("desc");
    assert_eq!(ids(&table), expected.into_iter().rev().collect::<Vec<_>>());
}

#[test]
fn mixed_sort_keys_preserve_signed_zero_text_and_structured_value_order() {
    let values = [
        ("true-text", json!("true")),
        ("positive-zero", json!(0.0)),
        ("object", json!({"tag":"x"})),
        ("negative", json!(-2)),
        ("two-text", json!("2")),
        ("missing", Value::Null),
        ("array", json!(["x"])),
        ("negative-zero", json!(-0.0)),
        ("false", json!(false)),
        ("empty", json!("")),
        ("ten", json!(10)),
        ("eleven-text", json!("11")),
        ("true", json!(true)),
    ];
    let rows: Vec<_> = values
        .into_iter()
        .map(|(id, value)| json!({"id":id,"value":value}))
        .collect();
    let mut table = json!({"data":{"inline":rows},"columns":[{"key":"id"},{"key":"value"}],"sort":{"key":"value","dir":"asc"}});
    let ids = |table: &Value| {
        cells(&render_table(table))
            .into_iter()
            .step_by(2)
            .collect::<Vec<_>>()
    };
    // Missing cells (null and the empty string) come after every value in both directions, in
    // source order (TYP-91).
    let present = [
        "negative",
        "negative-zero",
        "positive-zero",
        "ten",
        "eleven-text",
        "two-text",
        "array",
        "false",
        "true-text",
        "true",
        "object",
    ];
    let missing = ["missing", "empty"];
    let expected: Vec<_> = present.iter().chain(&missing).copied().collect();
    assert_eq!(ids(&table), expected);
    table["sort"]["dir"] = json!("desc");
    let expected: Vec<_> = present.iter().rev().chain(&missing).copied().collect();
    assert_eq!(ids(&table), expected);
}
