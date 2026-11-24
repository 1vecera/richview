//! Parquet data files (TYP-102): a block's `data.parquet`, and report SQL over a `parquet`
//! connector, both read in the restricted worker.
//!
//! The hostile corpus is here too. Its files are made by this test from one valid file, so the
//! repository holds no damaged binary: wrong magic, a truncated footer, corrupt pages, an
//! encrypted footer's magic, a file that expands a thousandfold, column names that try SQL, a
//! path that climbs out and a link that points out. Every one must end as a finding, with the
//! host unharmed and nothing read but the file the report names.

#![cfg(any(target_os = "macos", target_os = "linux", windows))]

use duckdb::Connection;
use richview_app::connectors::parquet_source::{self, ParquetRefusal, ParquetTable};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const WORKER: &str = env!("CARGO_BIN_EXE_richview-sql-worker");
const RICHVIEW: &str = env!("CARGO_BIN_EXE_richview");

/// Write the rows of `select` as the Parquet file `path`, in this process: a test is not a
/// report, and the file under test is what it writes.
fn write_parquet(path: &Path, select: &str) {
    write_parquet_with(path, select, "");
}

/// [`write_parquet`] with more `COPY` options, each written with its leading comma.
fn write_parquet_with(path: &Path, select: &str, options: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let literal = path.to_str().unwrap().replace('\'', "''");
    Connection::open_in_memory()
        .unwrap()
        .execute_batch(&format!(
            "COPY ({select}) TO '{literal}' (FORMAT parquet, COMPRESSION zstd{options})"
        ))
        .unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
}

/// A `report.connectors.yaml` in `folder` that declares `file` as the `parquet` connector `name`.
fn connect(folder: &Path, name: &str, file: &str) {
    fs::create_dir_all(folder).unwrap();
    fs::write(
        folder.join("report.connectors.yaml"),
        format!(
            "connectors:\n  {}:\n    kind: parquet\n    file: {}\n",
            serde_json::to_string(name).unwrap(),
            serde_json::to_string(file).unwrap()
        ),
    )
    .unwrap();
}

/// The columns `names` asks for, read by the worker.
fn read(path: &Path, names: Option<&[&str]>) -> Result<ParquetTable, ParquetRefusal> {
    let names: Option<Vec<String>> =
        names.map(|names| names.iter().map(|name| (*name).to_owned()).collect());
    parquet_source::read_columns(
        path,
        path.file_name().unwrap().to_str().unwrap(),
        Some(Path::new(WORKER)),
        names.as_deref(),
    )
}

fn table(path: &Path, names: Option<&[&str]>) -> Value {
    match read(path, names) {
        Ok(table) => serde_json::from_str(&table.text).expect("the table is JSON"),
        Err(refusal) => panic!("{}: {}", refusal.code, refusal.message),
    }
}

fn refusal(path: &Path, names: Option<&[&str]>) -> ParquetRefusal {
    match read(path, names) {
        Ok(table) => panic!("read as a table: {:.200}", table.text),
        Err(refusal) => refusal,
    }
}

/// A report folder with `report.rmd` holding `body` under a title.
fn report(folder: &Path, body: &str) -> PathBuf {
    fs::create_dir_all(folder).unwrap();
    let path = folder.join("report.rmd");
    fs::write(
        &path,
        format!("---\ntitle: Parquet\n---\n\n# Parquet\n\n{body}\n"),
    )
    .unwrap();
    path
}

/// Run `richview <verb> report.rmd --format json` and return its result.
fn richview(verb: &str, report: &Path) -> Value {
    let output = Command::new(RICHVIEW)
        .args([verb, report.to_str().unwrap(), "--format", "json"])
        .env("XDG_STATE_HOME", report.parent().unwrap().join(".state"))
        .output()
        .unwrap();
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{verb}: {error}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

/// The findings of `result` at `level`, as `code: message`.
fn findings(result: &Value, level: &str) -> Vec<String> {
    result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|finding| finding["level"] == level)
        .map(|finding| {
            format!(
                "{}: {}",
                finding["code"].as_str().unwrap(),
                finding["message"].as_str().unwrap()
            )
        })
        .collect()
}

/// What `check` says of each chart: its geometry, summary and checks. A test build embeds no
/// report runtime, so these tests check a report and never write one.
fn charts(report: &Path) -> Value {
    let result = richview("check", report);
    assert_eq!(findings(&result, "error"), Vec::<String>::new(), "{result}");
    result["charts"].clone()
}

const ORDERS: &str = "SELECT * FROM (VALUES
    (DATE '2026-01-05', 'North', 3, 120.50, 'a'),
    (DATE '2026-01-05', 'South', 1,  40.25, 'b'),
    (DATE '2026-02-09', 'North', 4, 161.00, 'c'),
    (DATE '2026-02-09', 'South', 2,  80.75, 'd'),
    (DATE '2026-03-02', 'North', 5, 200.00, 'e'),
    (DATE '2026-03-02', 'South', 2,  79.50, 'f')
  ) AS t(day, region, units, revenue, note)";

#[test]
fn a_chart_draws_a_parquet_file_as_it_draws_the_same_rows_from_csv() {
    let dir = tempfile::tempdir().unwrap();
    let chart = |source: &str| {
        format!(
            "```chart\ntype: line\ntitle: Revenue by day\ndata: {{{source}}}\nencoding: {{x: day, y: revenue, color: region}}\n```\n\n*Source: synthetic orders.*"
        )
    };
    let from_parquet = report(
        &dir.path().join("p"),
        &chart("parquet: data/orders.parquet"),
    );
    write_parquet(&dir.path().join("p/data/orders.parquet"), ORDERS);
    let from_csv = report(&dir.path().join("c"), &chart("csv: data/orders.csv"));
    fs::create_dir_all(dir.path().join("c/data")).unwrap();
    fs::write(
        dir.path().join("c/data/orders.csv"),
        "day,region,units,revenue,note\n2026-01-05,North,3,120.50,a\n2026-01-05,South,1,40.25,b\n2026-02-09,North,4,161.00,c\n2026-02-09,South,2,80.75,d\n2026-03-02,North,5,200.00,e\n2026-03-02,South,2,79.50,f\n",
    )
    .unwrap();
    let (parquet, csv) = (charts(&from_parquet), charts(&from_csv));
    assert_eq!(parquet.as_array().map(Vec::len), Some(1));
    assert_eq!(parquet, csv, "the same rows give the same chart");
}

#[test]
fn only_the_columns_the_blocks_name_are_read() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("wide.parquet");
    // `tags` is a list, which no block can read: reading it would fail the file.
    write_parquet(
        &file,
        "SELECT i AS id, i * 2 AS amount, 'internal ' || i AS secret_note, [i, i] AS tags FROM range(5) t(i)",
    );
    let read = table(
        &file,
        Some(&["chart", "encoding", "x", "id", "y", "amount", "Amounts"]),
    );
    assert_eq!(
        read["columns"],
        json!(["id", "amount", "secret_note", "tags"])
    );
    assert_eq!(read["fields"], json!(["id", "amount"]));
    assert_eq!(read["rows"][4], json!([4, 8]));
    // A block names a field by its exact name: a text that only holds a name reads nothing,
    // so a list column whose name is part of another's is never touched.
    let read = table(
        &file,
        Some(&["{secret_note} of {amount}", "tags of the id", "amount"]),
    );
    assert_eq!(read["fields"], json!(["amount"]));
    // A block that names no column of the file reads none, and core then names the real ones.
    let read = table(&file, Some(&["revenu"]));
    assert_eq!(read["fields"], json!([]));
    assert_eq!(read["rows"], json!([]));
    // Every column includes the list, and the finding names it.
    let refused = refusal(&file, None);
    assert_eq!(refused.code, "data.parquet");
    assert!(
        refused.message.contains("'tags'") && refused.message.contains("a list"),
        "{}",
        refused.message
    );
}

#[test]
fn parquet_types_decide_each_value() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("types.parquet");
    write_parquet(
        &file,
        "SELECT * FROM (VALUES
            (1::TINYINT, 9007199254740993::BIGINT, 10.50::DECIMAL(18,2), 12345678901234567890.123456::DECIMAL(38,6),
             0.1::FLOAT, 2.5::DOUBLE, true, '00123', DATE '2026-03-02', TIMESTAMP '2026-03-02 10:15:00',
             TIMESTAMPTZ '2026-03-02 10:15:00+00', TIME '10:15:00', 'nan'::DOUBLE, 'a\"b<script>'),
            (NULL, 7, -0.25, 1.5, NULL, 'inf'::DOUBLE, false, '123', NULL, NULL, NULL, NULL, 1.0, NULL)
         ) AS t(tiny, big, money, wide, single, double, flag, code, day, seen, seen_utc, clock, ratio, label)",
    );
    let read = read(&file, None).unwrap_or_else(|r| panic!("{}", r.message));
    let table: Value = serde_json::from_str(&read.text).unwrap();
    let rows = table["rows"].as_array().unwrap();
    assert_eq!(
        rows[0],
        json!([
            1,
            // Beyond 2^53 a number is kept as its digits (TYP-37).
            "9007199254740993",
            10.5,
            "12345678901234567890.123456",
            0.1,
            2.5,
            true,
            "00123",
            "2026-03-02",
            "2026-03-02T10:15:00",
            "2026-03-02T10:15:00Z",
            "10:15:00",
            null,
            "a\"b<script>"
        ])
    );
    assert_eq!(
        rows[1],
        json!([
            null, 7, -0.25, 1.5, null, null, false, "123", null, null, null, null, 1.0, null
        ])
    );
    // The table holds the decimal as written, not as the nearest double's digits.
    assert!(read.text.contains("10.50"), "{:.300}", read.text);
    assert!(read.text.contains(",0.1,"), "{:.300}", read.text);
    assert_eq!(
        read.non_finite,
        vec![("double".to_owned(), 1), ("ratio".to_owned(), 1)]
    );
}

#[test]
fn a_table_shows_typed_cells_and_warns_about_values_read_as_missing() {
    let dir = tempfile::tempdir().unwrap();
    write_parquet(
        &dir.path().join("data/stores.parquet"),
        "SELECT * FROM (VALUES ('00123', 'Brno', 10.50::DECIMAL(9,2), 'nan'::DOUBLE), ('00124', 'Praha', 7.25, 0.5)) AS t(code, city, sales, ratio)",
    );
    let path = report(
        dir.path(),
        "```table\ntitle: Stores\ndata: {parquet: data/stores.parquet}\n```\n\n*Source: synthetic stores.*",
    );
    let result = richview("check", &path);
    assert_eq!(findings(&result, "error"), Vec::<String>::new(), "{result}");
    let warnings = findings(&result, "warning");
    assert!(
        warnings.iter().any(|w| w.starts_with("data.parquet.non-finite: Column 'ratio' of Parquet file 'data/stores.parquet' holds 1 NaN or infinite value, read as missing")),
        "{warnings:?}"
    );
    // The file stays where it is: nothing is copied beside the report.
    assert!(!dir.path().join("Rendered").exists());
}

#[test]
fn a_misspelt_field_is_answered_with_the_files_columns() {
    let dir = tempfile::tempdir().unwrap();
    write_parquet(&dir.path().join("orders.parquet"), ORDERS);
    let path = report(
        dir.path(),
        "```chart\ntype: bar\ntitle: Revenue\ndata: {parquet: orders.parquet}\nencoding: {x: region, y: revenu}\n```\n\n*Source: synthetic orders.*",
    );
    let result = richview("check", &path);
    let errors = findings(&result, "error");
    assert!(
        errors.iter().any(|e| e.starts_with("block.field: chart.encoding.y names 'revenu', which is not a column of the data; did you mean 'revenue'? Columns: day, region, units, revenue, note.")),
        "{errors:?}"
    );
}

#[test]
fn more_than_a_dataset_may_hold_is_a_limit_finding_that_names_sql() {
    let dir = tempfile::tempdir().unwrap();
    let rows = dir.path().join("rows.parquet");
    write_parquet(&rows, "SELECT i AS id FROM range(1000001) t(i)");
    let refused = refusal(&rows, None);
    assert_eq!(refused.code, "data.limit");
    assert_eq!(
        refused.message,
        "Parquet file 'rows.parquet' has 1,000,001 rows, over the 1,000,000 rows one dataset may hold."
    );
    assert!(
        refused.fix.contains("`parquet` connector"),
        "{}",
        refused.fix
    );

    let bytes = dir.path().join("bytes.parquet");
    write_parquet(
        &bytes,
        "SELECT i AS id, repeat('x', 60) || i AS label FROM range(300000) t(i)",
    );
    let refused = refusal(&bytes, None);
    assert_eq!(refused.code, "data.limit");
    assert_eq!(
        refused.message,
        "The columns this report reads from Parquet file 'bytes.parquet' (300,000 rows) hold more than the 16 MiB one dataset may."
    );
    // The same file is within the limit for a block that reads its narrow column.
    let read = table(&bytes, Some(&["id"]));
    let ids = read["rows"].as_array().unwrap();
    assert_eq!(ids.len(), 300_000);
    // The reader asks for the rows in runs, and they come back in the file's order.
    assert!(
        ids.iter()
            .enumerate()
            .all(|(index, row)| row[0] == json!(index)),
        "file order"
    );
}

#[test]
fn report_sql_aggregates_a_parquet_file_through_its_connector() {
    let dir = tempfile::tempdir().unwrap();
    write_parquet(&dir.path().join("data/orders.parquet"), ORDERS);
    connect(dir.path(), "orders", "data/orders.parquet");
    let path = report(
        dir.path(),
        "```chart\ntype: bar\ntitle: Revenue by region\ndata:\n  sql:\n    connector: orders\n    inline: SELECT region, sum(revenue) AS revenue, count(*) AS orders FROM orders GROUP BY region ORDER BY region\nencoding: {x: region, y: revenue}\n```\n\n*Source: synthetic orders.*",
    );
    let result = richview("check", &path);
    assert_eq!(findings(&result, "error"), Vec::<String>::new(), "{result}");
    let info = findings(&result, "info");
    assert!(
        info.iter()
            .any(|i| i.starts_with("data.provenance:") && i.contains("orders")),
        "the rows are labelled as a query's result: {info:?}"
    );
    let summary = result["charts"][0]["summary"].as_str().unwrap_or_default();
    assert!(
        summary.contains("North") && summary.contains("South"),
        "{result}"
    );
}

/// What report SQL over the `parquet` connector `orders` reports for `sql`.
fn sql_errors(folder: &Path, sql: &str) -> Vec<String> {
    sql_errors_of(folder, "orders", sql)
}

/// What report SQL over the connector `connector` reports for `sql`.
fn sql_errors_of(folder: &Path, connector: &str, sql: &str) -> Vec<String> {
    let path = report(
        folder,
        &format!(
            "```table\ntitle: Probe\ndata:\n  sql:\n    connector: {}\n    inline: {}\n```\n\n*Source: probe.*",
            serde_json::to_string(connector).unwrap(),
            serde_json::to_string(sql).unwrap()
        ),
    );
    findings(&richview("check", &path), "error")
}

#[test]
fn report_sql_over_a_parquet_file_reads_nothing_else() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("report");
    write_parquet(&folder.join("data/orders.parquet"), ORDERS);
    connect(&folder, "orders", "data/orders.parquet");
    let outside = dir.path().join("outside.parquet");
    write_parquet(&outside, "SELECT 'secret-outside' AS secret");
    let secret = dir.path().join("secret.csv");
    fs::write(&secret, "secret\nsecret-outside\n").unwrap();
    assert_eq!(
        sql_errors(&folder, "SELECT count(*) AS n FROM orders"),
        Vec::<String>::new()
    );
    for sql in [
        format!("SELECT * FROM read_parquet('{}')", outside.display()),
        format!("SELECT * FROM read_csv('{}')", secret.display()),
        format!("SELECT * FROM read_text('{}')", secret.display()),
        format!("SELECT * FROM '{}'", outside.display()),
        format!("SELECT * FROM glob('{}/*')", dir.path().display()),
        "SELECT * FROM read_csv('/etc/passwd')".to_owned(),
        "SELECT * FROM read_parquet('https://example.com/x.parquet')".to_owned(),
        "SET enable_external_access = true".to_owned(),
        "SELECT current_setting('allowed_paths')".to_owned(),
        format!("COPY orders TO '{}/copy.parquet'", dir.path().display()),
        format!("ATTACH '{}/other.duckdb'", dir.path().display()),
        "INSTALL httpfs".to_owned(),
    ] {
        let errors = sql_errors(&folder, &sql);
        assert!(!errors.is_empty(), "accepted: {sql}");
        let text = errors.join("\n");
        assert!(!text.contains("secret-outside"), "{sql}: {text}");
    }
    assert!(!dir.path().join("copy.parquet").exists());
    assert!(!dir.path().join("other.duckdb").exists());
}

#[test]
fn column_names_that_try_sql_are_only_names() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("report");
    let secret = dir.path().join("secret.csv");
    fs::write(&secret, "secret\nsecret-outside\n").unwrap();
    let names = [
        format!("x\" FROM read_csv('{}') --", secret.display()),
        "'; DROP VIEW data; --".to_owned(),
        "a\"\"b".to_owned(),
        "data".to_owned(),
        "naïve \u{1F600}".to_owned(),
    ];
    let select = names
        .iter()
        .enumerate()
        .map(|(index, name)| format!("{index} AS \"{}\"", name.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(", ");
    let file = folder.join("names.parquet");
    write_parquet(&file, &format!("SELECT {select}"));
    let wanted: Vec<&str> = names.iter().map(String::as_str).collect();
    let read = table(&file, Some(&wanted));
    assert_eq!(read["fields"], json!(names));
    assert_eq!(read["rows"], json!([[0, 1, 2, 3, 4]]));
    assert!(!read.to_string().contains("secret-outside"));
    // A block's texts are matched against the file's names; they are never SQL.
    let read = table(&file, Some(&["* FROM read_csv('/etc/passwd') --", "data"]));
    assert_eq!(read["fields"], json!(["data"]));
}

#[test]
fn a_parquet_path_cannot_leave_the_report_folder() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("report");
    let outside = dir.path().join("outside.parquet");
    write_parquet(&outside, "SELECT 'secret-outside' AS secret");
    fs::create_dir_all(&folder).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, folder.join("link.parquet")).unwrap();
    let mut references = vec![
        "../outside.parquet".to_owned(),
        outside.display().to_string(),
    ];
    if cfg!(unix) {
        references.push("link.parquet".to_owned());
    }
    for reference in references {
        let path = report(
            &folder,
            &format!(
                "```table\ntitle: Outside\ndata: {{parquet: {}}}\n```\n\n*Source: probe.*",
                serde_json::to_string(&reference).unwrap()
            ),
        );
        let result = richview("check", &path);
        let errors = findings(&result, "error");
        assert!(!errors.is_empty(), "{reference} was read");
        assert!(
            !result.to_string().contains("secret-outside"),
            "{reference}"
        );
    }
}

/// One file of the hostile corpus: its name and bytes.
fn hostile_files(valid: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut files = vec![
        ("empty".to_owned(), vec![]),
        ("magic-only".to_owned(), b"PAR1".to_vec()),
        ("two-magics".to_owned(), b"PAR1PAR1".to_vec()),
        ("csv-renamed".to_owned(), b"id,amount\n1,2\n3,4\n".to_vec()),
        ("zip-renamed".to_owned(), b"PK\x03\x04 not parquet".to_vec()),
        (
            "truncated-footer".to_owned(),
            valid[..valid.len() - 12].to_vec(),
        ),
        ("first-half".to_owned(), valid[..valid.len() / 2].to_vec()),
    ];
    let with = |name: &str, change: &dyn Fn(&mut Vec<u8>)| {
        let mut bytes = valid.to_vec();
        change(&mut bytes);
        (name.to_owned(), bytes)
    };
    let end = valid.len();
    files.extend([
        with("wrong-head-magic", &|b| b[..4].copy_from_slice(b"XXXX")),
        with("wrong-tail-magic", &|b| {
            b[end - 4..].copy_from_slice(b"XXXX")
        }),
        // An encrypted footer ends in PARE; RichView has no key and must say so, not guess.
        with("encrypted-footer-magic", &|b| {
            b[..4].copy_from_slice(b"PARE");
            b[end - 4..].copy_from_slice(b"PARE");
        }),
        // The footer's length field claims more bytes than the file has, then none.
        with("footer-longer-than-file", &|b| {
            b[end - 8..end - 4].copy_from_slice(&u32::MAX.to_le_bytes())
        }),
        with("footer-of-no-bytes", &|b| {
            b[end - 8..end - 4].copy_from_slice(&0u32.to_le_bytes())
        }),
        with("footer-of-zeros", &|b| {
            let length = u32::from_le_bytes(b[end - 8..end - 4].try_into().unwrap()) as usize;
            b[end - 8 - length..end - 8].fill(0);
        }),
        with("footer-of-ones", &|b| {
            let length = u32::from_le_bytes(b[end - 8..end - 4].try_into().unwrap()) as usize;
            b[end - 8 - length..end - 8].fill(0xff);
        }),
        with("pages-of-zeros", &|b| b[4..end / 2].fill(0)),
        with("pages-of-ones", &|b| b[4..end / 2].fill(0xff)),
    ]);
    // Deterministic damage across the whole file: one byte, then a run of bytes, at 48 places.
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for index in 0..48 {
        let at = (next() % valid.len() as u64) as usize;
        let value = next() as u8;
        let run = if index % 2 == 0 { 1 } else { 16 };
        files.push(with(&format!("damage-{index:02}-at-{at}"), &|b| {
            let end = (at + run).min(b.len());
            b[at..end].fill(value);
        }));
    }
    files
}

#[test]
fn hostile_parquet_files_end_as_findings() {
    let dir = tempfile::tempdir().unwrap();
    let valid = dir.path().join("valid.parquet");
    write_parquet(
        &valid,
        "SELECT i AS id, i * 1.5 AS amount, 'row ' || i AS label, DATE '2026-01-01' + CAST(i % 300 AS INTEGER) AS day FROM range(2000) t(i)",
    );
    let bytes = fs::read(&valid).unwrap();
    let mut read_whole = 0;
    let mut refused = 0;
    for (name, bytes) in hostile_files(&bytes) {
        let file = dir.path().join(format!("{name}.parquet"));
        fs::write(&file, &bytes).unwrap();
        let started = std::time::Instant::now();
        match read(&file, None) {
            // Damage a reader cannot see leaves a table, which must still be a bounded one.
            Ok(table) => {
                let table: Value = serde_json::from_str(&table.text).expect("a table is JSON");
                assert!(table["rows"].as_array().unwrap().len() <= 2000, "{name}");
                read_whole += 1;
            }
            Err(refusal) => {
                assert!(
                    ["data.parquet", "data.limit"].contains(&refusal.code),
                    "{name}: {}",
                    refusal.code
                );
                // A finding names the file as the report does, and no path or engine text.
                assert!(
                    refusal.message.contains(&format!("{name}.parquet")),
                    "{name}"
                );
                for leak in [
                    dir.path().to_str().unwrap(),
                    "duckdb",
                    "DuckDB",
                    "Error:",
                    "thrift",
                ] {
                    assert!(
                        !refusal.message.contains(leak),
                        "{name}: {}",
                        refusal.message
                    );
                }
                refused += 1;
            }
        }
        assert!(
            started.elapsed().as_secs() < 40,
            "{name} outlasted the worker's limit"
        );
    }
    // The damaged structure is refused; the count guards a corpus that silently stops biting.
    assert!(refused >= 16, "{refused} refused, {read_whole} read");
}

#[test]
fn a_file_that_expands_a_thousandfold_stays_within_the_limits() {
    let dir = tempfile::tempdir().unwrap();
    // A million rows of one repeated 200-character text: about 200 MB as rows, kilobytes on disk.
    let bomb = dir.path().join("bomb.parquet");
    write_parquet(
        &bomb,
        "SELECT repeat('A', 200) AS text, 0 AS zero FROM range(1000000)",
    );
    let size = fs::metadata(&bomb).unwrap().len();
    assert!(size < 2_000_000, "the file is small on disk: {size} bytes");
    let refused = refusal(&bomb, None);
    assert_eq!(refused.code, "data.limit", "{}", refused.message);
    // Its narrow column alone is a million zeros, which a block may read.
    let read = table(&bomb, Some(&["zero"]));
    assert_eq!(read["rows"].as_array().unwrap().len(), 1_000_000);
}

#[test]
fn the_report_names_a_damaged_file_once_per_block_and_still_renders() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("data")).unwrap();
    fs::write(dir.path().join("data/broken.parquet"), b"id,amount\n1,2\n").unwrap();
    let path = report(
        dir.path(),
        "```table\ntitle: Broken\ndata: {parquet: data/broken.parquet}\n```\n\n*Source: probe.*\n\n```chart\ntype: bar\ntitle: Also broken\ndata: {parquet: data/broken.parquet}\nencoding: {x: id, y: amount}\n```\n\n*Source: probe.*",
    );
    let result = richview("check", &path);
    let errors = findings(&result, "error");
    let named: Vec<&String> = errors
        .iter()
        .filter(|e| {
            e.starts_with("data.parquet: 'data/broken.parquet' cannot be read as a Parquet file")
        })
        .collect();
    assert_eq!(named.len(), 2, "{errors:?}");
    assert!(
        !errors.iter().any(|e| e.starts_with("data.unresolved")),
        "one finding per cause (FEA-RP-30): {errors:?}"
    );
}

#[cfg(unix)]
#[test]
fn a_path_the_reader_would_take_for_a_pattern_is_refused_with_its_reason() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("report [draft]/orders.parquet");
    write_parquet(&file, ORDERS);
    let refused = refusal(&file, None);
    assert_eq!(refused.code, "data.parquet");
    assert!(
        refused.message.contains("takes for a pattern"),
        "{}",
        refused.message
    );
}

#[test]
fn rows_come_back_once_and_in_file_order_across_the_readers_runs() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("runs.parquet");
    // 300,000 distinct ids, descending so that file order is not sorted order, in row groups
    // of 2,048: the reader's first run ends inside the file and later runs cross many groups.
    write_parquet_with(
        &file,
        "SELECT 299999 - i AS id, 'row ' || i AS label FROM range(300000) t(i)",
        ", ROW_GROUP_SIZE 2048",
    );
    for names in [Some(&["id"][..]), Some(&["id", "label"][..])] {
        let read = table(&file, names);
        let rows = read["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 300_000, "every row once");
        let misplaced = rows
            .iter()
            .enumerate()
            .find(|(index, row)| row[0] != json!(299_999 - index));
        assert_eq!(
            misplaced, None,
            "a run was skipped, doubled or out of order"
        );
    }
}

#[test]
fn a_folder_named_like_a_partition_adds_no_column() {
    let dir = tempfile::tempdir().unwrap();
    // DuckDB reads `key=value` folders above a file as Hive partitions unless told not to.
    let folder = dir.path().join("tenant=acme/year=2026/report");
    let file = folder.join("data/orders.parquet");
    write_parquet(&file, ORDERS);
    let own = json!(["day", "region", "units", "revenue", "note"]);
    let read = table(&file, None);
    assert_eq!(read["columns"], own);
    assert_eq!(read["fields"], own);
    assert_eq!(read["rows"][0].as_array().unwrap().len(), 5);
    connect(&folder, "orders", "data/orders.parquet");
    assert_eq!(
        sql_errors(
            &folder,
            "SELECT day, region, units, revenue, note FROM orders"
        ),
        Vec::<String>::new()
    );
    for column in ["tenant", "year"] {
        let errors = sql_errors(&folder, &format!("SELECT \"{column}\" FROM orders"));
        assert!(
            !errors.is_empty(),
            "the folder name '{column}' became a column"
        );
        assert!(!errors.join("\n").contains("acme"), "{errors:?}");
    }
}

#[test]
fn a_parquet_connector_cannot_name_a_file_outside_the_report_folder() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("report");
    let outside = dir.path().join("outside.parquet");
    write_parquet(&outside, "SELECT 'secret-outside' AS secret");
    fs::create_dir_all(&folder).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, folder.join("link.parquet")).unwrap();
    let mut files = vec![
        "../outside.parquet".to_owned(),
        outside.display().to_string(),
    ];
    if cfg!(unix) {
        files.push("link.parquet".to_owned());
    }
    for file in files {
        connect(&folder, "orders", &file);
        let path = report(
            &folder,
            "```table\ntitle: Outside\ndata:\n  sql:\n    connector: orders\n    inline: SELECT * FROM orders\n```\n\n*Source: probe.*",
        );
        let result = richview("check", &path);
        assert!(!findings(&result, "error").is_empty(), "{file} was read");
        assert!(!result.to_string().contains("secret-outside"), "{file}");
    }
}

#[test]
fn a_connectors_name_is_only_a_name() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("report");
    write_parquet(&folder.join("data/orders.parquet"), ORDERS);
    // The name is the one author text written into a statement, as the view's quoted name.
    let name = "o\"; CREATE VIEW x AS SELECT 42 AS n; SELECT \"1";
    connect(&folder, name, "data/orders.parquet");
    let quoted = format!("\"{}\"", name.replace('"', "\"\""));
    // The whole name is one identifier: the view exists under it and nothing else ran.
    assert_eq!(
        sql_errors_of(
            &folder,
            name,
            &format!("SELECT count(*) AS n FROM {quoted}")
        ),
        Vec::<String>::new()
    );
    assert!(
        !sql_errors_of(&folder, name, "SELECT * FROM x").is_empty(),
        "a statement inside the name ran"
    );
}

#[cfg(unix)]
#[test]
fn a_parquet_connector_at_a_pattern_path_is_refused_with_its_reason() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("report [draft]");
    write_parquet(&folder.join("data/orders.parquet"), ORDERS);
    connect(&folder, "orders", "data/orders.parquet");
    let errors = sql_errors(&folder, "SELECT count(*) AS n FROM orders");
    assert!(
        errors
            .iter()
            .any(|error| error.contains("takes for a pattern") && error.contains("Rename")),
        "{errors:?}"
    );
}
