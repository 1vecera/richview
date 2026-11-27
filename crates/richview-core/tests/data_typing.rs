//! One data-typing contract for inline, CSV and JSON rows (TYP-37, TYP-38, TYP-43, TYP-91,
//! TYP-92): the source key picks the parser, rows are flat, one set of markers means missing, and
//! the size limits hold in core for every host.

use richview_core::document::Diagnostic;
use richview_core::{render_compat, render_table};
use serde_json::{Value, json};

fn render(source: &str, files: &[(&str, String)]) -> Result<Value, Vec<Diagnostic>> {
    let mut listed = vec!["report.rmd".to_owned()];
    let mut data = serde_json::Map::new();
    for (path, text) in files {
        listed.push((*path).to_owned());
        data.insert((*path).to_owned(), json!(text));
    }
    render_compat(
        source,
        &json!({
            "report_folder": {"root": "data-typing", "source": "report.rmd", "files": listed},
            "data": data,
        }),
    )
}

fn table(key: &str, path: &str, content: &str) -> Result<Value, Vec<Diagnostic>> {
    render(
        &format!("```table\ndata: {{{key}: {path}}}\n```\n"),
        &[(path, content.to_owned())],
    )
}

fn rows(output: &Value) -> Vec<Value> {
    output["parsed"]["blocks"][0]["value"]["data"]["inline"]
        .as_array()
        .unwrap()
        .clone()
}

fn only_error(result: Result<Value, Vec<Diagnostic>>) -> Diagnostic {
    // A failed block no longer fails the report: its error rides in the output's parse
    // diagnostics and the block shows as source.
    let errors: Vec<Diagnostic> = match result {
        Err(errors) => errors,
        Ok(output) => {
            serde_json::from_value::<Vec<Diagnostic>>(output["diagnostics"]["parse"].clone())
                .expect("parse diagnostics")
                .into_iter()
                .filter(|d| d.level == "error")
                .collect()
        }
    };
    assert_eq!(errors.len(), 1, "{errors:?}");
    errors.into_iter().next().unwrap()
}

#[test]
fn csv_is_comma_separated_trimmed_and_typed() {
    let out = table(
        "csv",
        "data/a.csv",
        "\u{feff} name , n ,flag,id,big,note\r\n\"a, \"\"b\"\"\", 10.50 ,true,00123,9007199254740993,\r\n",
    )
    .unwrap();
    let row = &rows(&out)[0];
    assert_eq!(row["name"], "a, \"b\"");
    assert_eq!(row["n"], json!(10.5));
    assert_eq!(row["flag"], true);
    assert_eq!(row["id"], "00123", "a leading zero is an identifier");
    assert_eq!(row["big"], "9007199254740993", "text keeps precision");
    assert_eq!(row["note"], Value::Null);
    assert_eq!(
        out["parsed"]["blocks"][0]["value"]["data"]["columns"],
        json!(["name", "n", "flag", "id", "big", "note"])
    );
}

/// Where a finding sits: its file, line and column (TYP-56).
fn place(e: &Diagnostic) -> (Option<&str>, Option<usize>, Option<usize>) {
    (e.file.as_deref(), e.line, e.column)
}

#[test]
fn csv_problems_have_their_own_codes_and_lines() {
    for content in ["a,,c\n1,2,3\n", "a,b,a\n1,2,3\n", "a, \n1,2\n"] {
        let e = only_error(table("csv", "data/a.csv", content));
        assert_eq!(e.code, "data.csv.header", "{content:?}");
        assert_eq!(place(&e), (Some("data/a.csv"), Some(1), Some(1)));
        assert_eq!(e.path, "table.data.csv");
    }
    // A quoted cell may span lines: the record's own first line counts, not its row number.
    let e = only_error(table("csv", "data/a.csv", "a,b\n1,\"x\ny\"\n3\n"));
    assert_eq!(e.code, "data.csv");
    assert!(e.message.contains("Line 4"), "{}", e.message);
    assert_eq!(place(&e), (Some("data/a.csv"), Some(4), Some(1)));
    // CRLF line endings count the same lines.
    let e = only_error(table("csv", "data/a.csv", "a,b\r\n1,\"x\r\ny\"\r\n3\r\n"));
    assert!(e.message.contains("Line 4"), "{}", e.message);
    assert_eq!(place(&e), (Some("data/a.csv"), Some(4), Some(1)));
    // A semicolon file is not a CSV with one column of text: its rows are ragged or it has
    // fields that never split.
    let e = only_error(table("csv", "data/a.csv", "a;b\n1;2\n3,4,5\n"));
    assert_eq!(e.code, "data.csv");
    assert!(e.message.contains("commas"), "{}", e.message);
}

#[test]
fn json_is_an_array_of_flat_rows() {
    let out = table(
        "json",
        "data/a.json",
        r#"[{"b": 1, "a": "x", "ok": true, "none": null}]"#,
    )
    .unwrap();
    assert_eq!(rows(&out)[0]["a"], "x");
    assert_eq!(
        out["parsed"]["blocks"][0]["value"]["data"]["columns"],
        json!(["b", "a", "ok", "none"])
    );
    for (content, needle) in [
        (r#"{"inline": [{"a": 1}]}"#, "array of row objects"),
        (r#"[1, 2]"#, "Row 1"),
        (r#"[{"a": 1}, {"a": 2, "b": {"c": 1}}]"#, "Row 2, key 'b'"),
        (r#"[{"a": [1]}]"#, "Row 1, key 'a'"),
        ("[{\"a\": 1}", "not valid JSON"),
        (r#"[{"a": 1e999}]"#, "not valid JSON"),
    ] {
        let e = only_error(table("json", "data/a.json", content));
        assert_eq!(e.code, "data.json", "{content}");
        assert_eq!(e.file.as_deref(), Some("data/a.json"), "{content}");
        assert_eq!(e.line, Some(1), "{content}");
        assert!(e.message.contains(needle), "{content}: {}", e.message);
    }
    // Each problem sits where it is in the file: the row, the outer value, the parse error.
    for (content, line, column) in [
        ("[\n  {\"a\": 1},\n  {\"a\": [2]}\n]", 3, 3),
        ("\n\n  {\"inline\": []}", 3, 3),
        ("[\n  {\"é\": 1,}\n]", 2, 11),
    ] {
        let e = only_error(table("json", "data/a.json", content));
        assert_eq!(
            place(&e),
            (Some("data/a.json"), Some(line), Some(column)),
            "{content}: {}",
            e.message
        );
    }
}

#[test]
fn the_source_key_decides_the_parser() {
    let as_csv = only_error(table("csv", "data/a.csv", "[{\"a\": 1},\n{\"a\": 2}]"));
    assert!(as_csv.code.starts_with("data.csv"), "{}", as_csv.code);
    assert_eq!(as_csv.line, Some(1));
    let as_json = only_error(table("json", "data/a.json", "a,b\n1,2\n"));
    assert_eq!(as_json.code, "data.json");
    assert_eq!(as_json.line, Some(1));
}

#[test]
fn inline_rows_hold_only_flat_values() {
    let source = "```table\ndata:\n  inline:\n    - {a: 1, b: {c: 2}}\n```\n";
    let doc = richview_core::document::parse(source).unwrap();
    let e = doc
        .diagnostics
        .iter()
        .find(|d| d.code == "data.inline")
        .expect("nested inline value is reported");
    assert_eq!(e.level, "error");
    assert!(e.message.contains("Row 1, key 'b'"), "{}", e.message);
    // The diagnostic carries the line of the row it names (TYP-56).
    assert_eq!(e.path, "table.data.inline[0]");
    assert_eq!(e.line, Some(4));
}

fn sorted_ids(content: &Value, dir: &str) -> Vec<String> {
    let table = json!({
        "data": {"inline": content},
        "columns": [{"key": "id"}, {"key": "v"}],
        "sort": {"key": "v", "dir": dir},
    });
    let html = render_table(&table);
    html.split("<tr><td")
        .skip(1)
        .map(|row| {
            let cell = row.split_once('>').unwrap().1;
            cell.split('<').next().unwrap().to_owned()
        })
        .collect()
}

#[test]
fn tables_print_missing_cells_as_written_sort_them_last_and_total_without_them() {
    let rows = json!([
        {"id": "na", "v": "n/a"},
        {"id": "two", "v": 2},
        {"id": "null", "v": null},
        {"id": "one", "v": 1},
        {"id": "dot", "v": "."},
    ]);
    assert_eq!(
        sorted_ids(&rows, "asc"),
        ["one", "two", "na", "null", "dot"]
    );
    assert_eq!(
        sorted_ids(&rows, "desc"),
        ["two", "one", "na", "null", "dot"]
    );
    let html = render_table(&json!({
        "data": {"inline": rows},
        "columns": [{"key": "id"}, {"key": "v"}],
        "total": true,
    }));
    assert!(html.contains(">n/a<"), "printed as written: {html}");
    assert!(
        html.contains("<tfoot><tr><td class=\"\">Total</td><td class=\"num\">3"),
        "{html}"
    );
    let text = render_table(&json!({
        "data": {"inline": [{"id": "a", "v": 1}, {"id": "b", "v": "oops"}]},
        "columns": [{"key": "id"}, {"key": "v"}],
        "total": true,
    }));
    assert!(
        !text.contains("<td class=\"\">1</td></tr></tfoot>"),
        "{text}"
    );
}

fn chart_report(rows: &str) -> Result<Value, Vec<Diagnostic>> {
    render(
        &format!(
            "```chart\ntype: bar\ntitle: T\nencoding: {{x: k, y: v}}\ndata:\n  inline:\n{rows}\n```\n"
        ),
        &[],
    )
}

fn chart_checks(output: &Value) -> Vec<Value> {
    output["diagnostics"]["charts"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|chart| chart["checks"].as_array().unwrap().clone())
        .collect()
}

#[test]
fn charts_count_markers_as_gaps_and_reject_other_text() {
    let ok = chart_report(
        "    - {k: a, v: 1}\n    - {k: b, v: NA}\n    - {k: c, v: ' N/A '}\n    - {k: d, v: null}\n    - {k: e, v: '.'}\n    - {k: f, v: none}\n    - {k: g, v: NaN}\n    - {k: h, v: 2}",
    )
    .unwrap();
    let checks = chart_checks(&ok);
    assert!(
        checks.iter().all(|c| c["severity"] != "error"),
        "{checks:?}"
    );
    let gaps = checks
        .iter()
        .find(|c| c["kind"] == "missing-observations")
        .unwrap_or_else(|| panic!("{checks:?}"));
    assert_eq!(gaps["details"]["count"], 6);
    let bad =
        chart_report("    - {k: a, v: 1}\n    - {k: b, v: '1,234'}\n    - {k: c, v: 2}").unwrap();
    let found = chart_checks(&bad);
    let e = found
        .iter()
        .find(|c| c["kind"] == "non-numeric")
        .unwrap_or_else(|| panic!("{found:?}"));
    assert_eq!(e["severity"], "error");
    assert!(e["message"].as_str().unwrap().contains("1,234"), "{e}");
    assert!(e["message"].as_str().unwrap().contains("Row 2"), "{e}");
}

#[test]
fn the_dataset_and_source_limits_are_core_errors() {
    let big = format!("a\n{}", "1\n".repeat(8 * 1024 * 1024 + 1));
    let e = only_error(table("csv", "data/big.csv", &big));
    assert_eq!(e.code, "data.limit");
    // A file over the limit is not read, so the finding sits at the field that names it.
    assert_eq!(place(&e), (None, Some(2), Some(8)));
    let source = " ".repeat(richview_core::document::MAX_SOURCE_BYTES + 1);
    let e = richview_core::document::parse(&source).expect_err("source over the limit");
    assert_eq!(e[0].code, "source.limit");
}

/// The findings `check` returns for a report whose folder holds `files`.
fn checked(source: &str, files: &[(&str, String)]) -> Vec<Diagnostic> {
    let mut options = richview_core::document::RenderOptions::default();
    let mut listed = vec!["report.rmd".to_owned()];
    for (path, text) in files {
        listed.push((*path).to_owned());
        options.data.insert((*path).to_owned(), text.clone());
    }
    options.report_folder = Some(
        serde_json::from_value(
            json!({"root": "data-typing", "source": "report.rmd", "files": listed}),
        )
        .unwrap(),
    );
    richview_core::document::check(source, &options)
        .unwrap()
        .diagnostics
}

#[test]
fn a_row_check_on_file_data_sits_at_that_row_in_the_file() {
    // TYP-56: a chart finding about one row of a CSV or JSON file names the file and the row's
    // own line in it, even after a quoted cell that spans lines.
    let source = "Intro.\n\n```chart\ntype: bar\ntitle: T\nencoding: {x: k, y: v}\ndata: {FORMAT: data/a.FORMAT}\n```\n";
    for (format, content, line, column) in [
        (
            "csv",
            "k,note,v\na,\"two\nlines\",1\nb,x,\"1,234\"\nc,y,2\n",
            4,
            1,
        ),
        // A file with CRLF line endings, as a spreadsheet or Python's csv module writes it.
        (
            "csv",
            "k,note,v\r\na,\"two\r\nlines\",1\r\nb,x,\"1,234\"\r\nc,y,2\r\n",
            4,
            1,
        ),
        (
            "json",
            "[\n  {\"k\": \"a\", \"v\": 1},\n  {\"k\": \"b\", \"v\": \"1,234\"}\n]",
            3,
            3,
        ),
    ] {
        let file = format!("data/a.{format}");
        let found = checked(
            &source.replace("FORMAT", format),
            &[(&file, content.to_owned())],
        );
        let e = found
            .iter()
            .find(|d| d.code == "chart.non-numeric")
            .unwrap_or_else(|| panic!("{found:?}"));
        assert_eq!(e.path, "rows[1].v", "{format}");
        assert_eq!(
            place(e),
            (Some(file.as_str()), Some(line), Some(column)),
            "{format}"
        );
    }
    // Rows written in the fence keep their place in the fence.
    let found = checked(
        "```chart\ntype: bar\ntitle: T\nencoding: {x: k, y: v}\ndata:\n  inline:\n    - {k: a, v: 1}\n    - {k: b, v: '1,234'}\n```\n",
        &[],
    );
    let e = found
        .iter()
        .find(|d| d.code == "chart.non-numeric")
        .unwrap_or_else(|| panic!("{found:?}"));
    assert_eq!((e.file.as_deref(), e.line), (None, Some(8)));
}

/// A Parquet file as a native host hands it to core (TYP-102): the file's column names, the
/// names of the columns it read, and one list of scalars per row.
fn parquet_table(columns: &[&str], fields: &[&str], rows: Value) -> String {
    json!({"columns": columns, "fields": fields, "rows": rows}).to_string()
}

#[test]
fn a_parquet_file_arrives_as_typed_rows_of_the_columns_read() {
    let content = parquet_table(
        &["id", "day", "amount", "secret_note"],
        &["id", "day", "amount"],
        json!([["00123", "2026-03-02", 10.5], ["123", null, 7]]),
    );
    let out = table("parquet", "data/orders.parquet", &content).unwrap();
    let rows = rows(&out);
    assert_eq!(
        rows,
        vec![
            json!({"id": "00123", "day": "2026-03-02", "amount": 10.5}),
            json!({"id": "123", "day": null, "amount": 7}),
        ],
        "Parquet's types decide: text that looks like a number stays text"
    );
    // The recorded columns are the file's, read or not, in file order (TYP-25).
    assert_eq!(
        out["parsed"]["blocks"][0]["value"]["data"]["columns"],
        json!(["id", "day", "amount", "secret_note"])
    );
}

#[test]
fn a_misspelt_field_of_a_parquet_file_is_answered_with_every_column_of_the_file() {
    let content = parquet_table(
        &["region", "revenue", "units"],
        &["region"],
        json!([["North"]]),
    );
    let found = checked(
        "```chart\ntype: bar\ntitle: T\ndata: {parquet: orders.parquet}\nencoding: {x: region, y: revenu}\n```\n",
        &[("orders.parquet", content)],
    );
    let field: Vec<&Diagnostic> = found.iter().filter(|d| d.code == "block.field").collect();
    assert_eq!(field.len(), 1, "{found:?}");
    assert_eq!(
        field[0].message,
        "chart.encoding.y names 'revenu', which is not a column of the data; did you mean 'revenue'? Columns: region, revenue, units."
    );
}

#[test]
fn a_parquet_table_that_is_not_one_is_a_data_parquet_error() {
    for content in [
        "PAR1 raw bytes are never core's to read".to_owned(),
        "[{\"a\": 1}]".to_owned(),
        // A field the file does not have, a row of the wrong width, a nested cell, an extra key.
        parquet_table(&["a"], &["b"], json!([[1]])),
        parquet_table(&["a", "b"], &["a", "b"], json!([[1]])),
        parquet_table(&["a"], &["a"], json!([[{"x": 1}]])),
        parquet_table(&["a"], &["a"], json!([[[1, 2]]])),
        json!({"columns": ["a"], "fields": ["a"], "scales": {"a": 39}, "rows": [[1]]}).to_string(),
        json!({"columns": ["a"], "fields": ["a"], "scales": {"b": 2}, "rows": [[1]]}).to_string(),
        "{\"columns\":[\"a\"],\"fields\":[\"a\"],\"rows\":[[1]],\"html\":\"<script>\"}".to_owned(),
    ] {
        let e = only_error(table("parquet", "data/x.parquet", &content));
        assert_eq!(e.code, "data.parquet", "{content}");
        assert_eq!(e.path, "table.data.parquet");
        assert!(e.message.contains("'data/x.parquet'"), "{}", e.message);
    }
}

#[test]
fn a_parquet_file_no_host_read_says_which_hosts_read_parquet() {
    // The folder holds the file, and the host, a browser one, passed no rows for it.
    let e = only_error(render_compat(
        "```table\ndata: {parquet: data/orders.parquet}\n```\n",
        &json!({
            "report_folder": {
                "root": "data-typing",
                "source": "report.rmd",
                "files": ["report.rmd", "data/orders.parquet"],
            },
        }),
    ));
    assert_eq!(e.code, "data.unresolved");
    assert_eq!(e.path, "table.data.parquet");
    assert_eq!(
        e.message,
        "Parquet file 'data/orders.parquet' was not read by this host."
    );
    assert!(
        e.fix
            .as_deref()
            .unwrap_or("")
            .starts_with("Render with the `richview` command or a RichView server"),
        "{:?}",
        e.fix
    );
}

#[test]
fn parquet_is_one_source_like_any_other() {
    let e = only_error(render(
        "```table\ndata: {parquet: data/a.parquet, csv: data/a.csv}\n```\n",
        &[("data/a.csv", "a\n1\n".to_owned())],
    ));
    assert_eq!(e.code, "data.local");
    let e = only_error(render(
        "```table\ndata: {parquet: ../a.parquet}\n```\n",
        &[],
    ));
    assert_eq!(e.code, "report.asset.escape");
}
