//! Parquet files as report data, read with DuckDB's Parquet reader in the restricted worker.
//!
//! A block's `data.parquet` names a file in the report folder. The host asks the worker for the
//! columns the report's blocks name, and the worker returns them as one typed table that
//! `richview-core` reads; the host never parses the file. A `parquet` connector shows the same
//! file to report SQL as one view. Either way the worker opens an empty in-memory database
//! that may read that one file and nothing else.

use super::duckdb_adapter::{convert_value, duck_err, with_timeout};
use super::spec::{ConnectorError, SqlError};
use super::sql_worker::{self, ColumnKind, TableFailure, TableRead};
use duckdb::{Config, Connection, arrow::datatypes::DataType, types::ValueRef};
use std::{fmt::Write as _, path::Path, time::Duration};

/// The most rows one Parquet file gives a report. A file with more is aggregated by SQL.
pub const MAX_TABLE_ROWS: u64 = 1_000_000;
/// The view a table read selects from.
pub(super) const TABLE: &str = "data";
/// The rows of a table read's first run, and the most rows of a later one.
const FIRST_RUN: u64 = 2_048;
const LONGEST_RUN: u64 = 262_144;
/// The largest integer every binary64 number below it tells apart (2^53).
const EXACT_INTEGER: u64 = 1 << 53;

/// Whether DuckDB would read `path` as a pattern and not as one file: it holds `*`, `?` or `[`
/// (a brace is not a pattern), or is not text DuckDB can be given at all.
pub(super) fn pattern_path(path: &Path) -> bool {
    path.to_str()
        .is_none_or(|path| path.contains(['*', '?', '[']))
}

/// What a finding says of a file at such a path, and what the author does about it.
pub(super) const PATTERN_PATH: &str =
    "sits at a path with `*`, `?` or `[` in it, which the Parquet reader takes for a pattern";
pub(super) const PATTERN_PATH_FIX: &str =
    "Rename the file or folder so its path has none of those characters";

/// `name` as a quoted SQL identifier.
pub(super) fn quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// An in-memory database whose one view, `view` (already quoted), reads the Parquet file
/// `source`. External access is off except for that path, and the configuration is locked
/// before any report SQL is prepared.
pub(super) fn open(
    source: &Path,
    view: &str,
    connector: &str,
) -> Result<Connection, ConnectorError> {
    let unreadable = || ConnectorError::Sql {
        connector: connector.to_owned(),
        error: SqlError::Database,
    };
    // DuckDB reads `*`, `?` and `[` in a path as a pattern, and matching a pattern lists
    // folders, which the worker may not read. The host refuses such a path with its own finding.
    let path = source
        .to_str()
        .filter(|_| !pattern_path(source))
        .ok_or_else(unreadable)?;
    let literal = path.replace('\'', "''");
    let config = Config::default()
        .enable_autoload_extension(false)
        .and_then(|config| config.with("memory_limit", "128MB"))
        .and_then(|config| config.with("threads", "2"))
        // The worker can write nowhere, so the database has no temporary folder. With one,
        // turning external access off asks for the working directory, which the macOS sandbox
        // does not let the worker read.
        .and_then(|config| config.with("temp_directory", ""))
        .map_err(duck_err(connector, None))?;
    let connection =
        Connection::open_in_memory_with_flags(config).map_err(duck_err(connector, None))?;
    // The view is the file's own columns and rows in the file's order: a folder above it named
    // `key=value` adds no column (DuckDB reads such names as Hive partitions by default), and a
    // table read fetches the rows in runs, which relies on the order being kept.
    connection
        .execute_batch(&format!(
            "SET allowed_paths = ['{literal}']; SET enable_external_access = false; \
             SET preserve_insertion_order = true; \
             CREATE VIEW {view} AS SELECT * FROM \
             read_parquet('{literal}', hive_partitioning = false); \
             SET errors_as_json = true; SET lock_configuration = true"
        ))
        .map_err(duck_err(connector, None))?;
    Ok(connection)
}

/// How a column of a DuckDB type is read.
enum Reading {
    /// As DuckDB returns it.
    Native,
    /// Cast to text. `numeric` text is a decimal or an integer wider than 64 bits.
    Text {
        numeric: bool,
    },
    Unsupported(ColumnKind),
}

fn reading(kind: &str) -> Reading {
    let kind = kind.to_ascii_uppercase();
    if kind.ends_with(']') {
        Reading::Unsupported(ColumnKind::List)
    } else if kind.starts_with("STRUCT") {
        Reading::Unsupported(ColumnKind::Struct)
    } else if kind.starts_with("MAP") {
        Reading::Unsupported(ColumnKind::Map)
    } else if kind.starts_with("UNION") {
        Reading::Unsupported(ColumnKind::Union)
    } else if matches!(kind.as_str(), "BLOB" | "BIT" | "GEOMETRY") {
        Reading::Unsupported(ColumnKind::Binary)
    } else if kind == "INTERVAL" {
        Reading::Unsupported(ColumnKind::Interval)
    } else if matches!(
        kind.as_str(),
        "BOOLEAN"
            | "TINYINT"
            | "SMALLINT"
            | "INTEGER"
            | "BIGINT"
            | "UTINYINT"
            | "USMALLINT"
            | "UINTEGER"
            | "UBIGINT"
            | "FLOAT"
            | "DOUBLE"
            | "DATE"
            | "VARCHAR"
    ) || kind.starts_with("TIMESTAMP")
    {
        Reading::Native
    } else {
        Reading::Text {
            numeric: kind.starts_with("DECIMAL") || kind == "HUGEINT" || kind == "UHUGEINT",
        }
    }
}

/// Whether decimal `text` is a JSON number that binary64 holds exactly as written: at most 15
/// significant digits (TYP-37 keeps longer numbers as text).
fn exact_number(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    let plain = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    !whole.is_empty()
        && plain(whole)
        && plain(fraction)
        && (whole == "0" || !whole.starts_with('0'))
        && !digits.ends_with('.')
        && whole.trim_start_matches('0').len() + fraction.len() <= 15
}

/// Read from the view [`TABLE`] the columns `names` asks for (see the protocol's `TableRequest`)
/// and write them as the table `richview-core` reads: the file's column names in file order,
/// the names of the columns read, and one list of scalars per row.
///
/// Parquet's types decide each value's type. Integers and floats are numbers; a decimal is a
/// number when binary64 holds its digits and text otherwise, as an integer beyond 2^53 is;
/// dates, times and timestamps are ISO 8601 text; NaN and the infinities are missing, counted
/// per column.
pub(super) fn read_table(
    connection: &Connection,
    names: Option<&[String]>,
    timeout: Duration,
) -> Result<TableRead, ConnectorError> {
    const CONNECTOR: &str = "parquet";
    let failed = duck_err(CONNECTOR, Some(""));
    with_timeout(connection, timeout, || {
        let mut columns: Vec<(String, String)> = Vec::new();
        {
            let mut statement = connection
                .prepare(&format!("DESCRIBE {TABLE}"))
                .map_err(&failed)?;
            let mut rows = statement.query([]).map_err(&failed)?;
            while let Some(row) = rows.next().map_err(&failed)? {
                columns.push((row.get(0).map_err(&failed)?, row.get(1).map_err(&failed)?));
            }
        }
        // A block names a field by its exact name; a text that only holds a name reads nothing.
        let wanted = |name: &str| names.is_none_or(|names| names.iter().any(|text| text == name));
        let mut select = Vec::new();
        let mut fields = Vec::new();
        let mut numeric = Vec::new();
        let mut scales = std::collections::BTreeMap::new();
        for (name, kind) in &columns {
            if !wanted(name) {
                continue;
            }
            match reading(kind) {
                Reading::Unsupported(kind) => {
                    return Ok(TableRead::Unsupported {
                        column: name.clone(),
                        kind,
                    });
                }
                Reading::Native => {
                    select.push(quoted(name));
                    numeric.push(false);
                }
                Reading::Text { numeric: is } => {
                    select.push(format!("CAST({} AS VARCHAR)", quoted(name)));
                    numeric.push(is);
                    if let Some(scale) = kind
                        .strip_prefix("DECIMAL(")
                        .and_then(|kind| kind.strip_suffix(')'))
                        .and_then(|args| args.split_once(','))
                        .and_then(|(_, scale)| scale.trim().parse::<u8>().ok())
                    {
                        scales.insert(name, scale);
                    }
                }
            }
            fields.push(name.as_str());
        }
        // Parquet's footer holds the row count, so counting reads no column.
        let total: u64 = connection
            .query_row(&format!("SELECT count(*) FROM {TABLE}"), [], |row| {
                row.get(0)
            })
            .map_err(&failed)?;
        if total > MAX_TABLE_ROWS {
            return Ok(TableRead::TooLarge {
                rows: total,
                by_rows: true,
            });
        }
        let mut text = String::new();
        text.push_str("{\"columns\":");
        text.push_str(&json(
            &columns.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(),
        ));
        text.push_str(",\"fields\":");
        text.push_str(&json(&fields));
        if !scales.is_empty() {
            text.push_str(",\"scales\":");
            text.push_str(&json(&scales));
        }
        text.push_str(",\"rows\":[");
        let mut non_finite = vec![0u64; fields.len()];
        // DuckDB holds a whole result before its first row comes back, so the rows are asked for
        // in runs: a short first run measures a row, and each later run is as many rows as fit
        // the dataset limit. A file that expands far beyond its size is then over that limit,
        // and never a result larger than the reader's memory. The runs follow one another in
        // the file's order because `open` sets `preserve_insertion_order` and locks it.
        let (mut offset, mut take) = (0u64, FIRST_RUN);
        let mut first = true;
        while !fields.is_empty() && offset < total {
            let mut statement = connection
                .prepare(&format!(
                    "SELECT {} FROM {TABLE} LIMIT {take} OFFSET {offset}",
                    select.join(", ")
                ))
                .map_err(&failed)?;
            let mut rows = statement.query([]).map_err(&failed)?;
            let zoned: Vec<bool> = rows
                .as_ref()
                .map(|statement| {
                    (0..fields.len())
                        .map(|index| {
                            matches!(
                                statement.column_type(index),
                                DataType::Timestamp(_, Some(_))
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            let (before, mut read) = (text.len(), 0u64);
            while let Some(row) = rows.next().map_err(&failed)? {
                if !std::mem::take(&mut first) {
                    text.push(',');
                }
                text.push('[');
                for index in 0..fields.len() {
                    if index > 0 {
                        text.push(',');
                    }
                    let value = row.get_ref(index).map_err(&failed)?;
                    match value {
                        ValueRef::Float(number) if !number.is_finite() => {
                            non_finite[index] += 1;
                            text.push_str("null");
                        }
                        ValueRef::Double(number) if !number.is_finite() => {
                            non_finite[index] += 1;
                            text.push_str("null");
                        }
                        // The shortest text that reads back as this single-precision number,
                        // not the digits of its nearest double.
                        ValueRef::Float(number) => {
                            let _ = write!(text, "{number}");
                        }
                        ValueRef::BigInt(number) if number.unsigned_abs() > EXACT_INTEGER => {
                            text.push_str(&json(&number.to_string()));
                        }
                        ValueRef::UBigInt(number) if number > EXACT_INTEGER => {
                            text.push_str(&json(&number.to_string()));
                        }
                        ValueRef::Text(bytes) if numeric[index] => {
                            let digits = std::str::from_utf8(bytes).map_err(|_| unsupported())?;
                            if exact_number(digits) {
                                text.push_str(digits);
                            } else {
                                text.push_str(&json(&digits));
                            }
                        }
                        value => {
                            let (value, _) = convert_value(value, zoned[index])?;
                            text.push_str(&json(&value));
                        }
                    }
                }
                text.push(']');
                read += 1;
                if text.len() > richview_core::document::MAX_DATASET_BYTES {
                    return Ok(TableRead::TooLarge {
                        rows: total,
                        by_rows: false,
                    });
                }
            }
            if read == 0 {
                break;
            }
            offset += read;
            let row = ((text.len() - before) as u64 / read).max(1);
            take = (richview_core::document::MAX_DATASET_BYTES as u64 / row)
                .clamp(FIRST_RUN, LONGEST_RUN);
        }
        text.push_str("]}");
        Ok(TableRead::Table {
            text,
            non_finite: fields
                .iter()
                .zip(non_finite)
                .filter(|(_, count)| *count > 0)
                .map(|(name, count)| ((*name).to_owned(), count))
                .collect(),
        })
    })
}

fn unsupported() -> ConnectorError {
    ConnectorError::QueryRejected {
        reason: super::duckdb_adapter::UNSUPPORTED_SCALAR.into(),
    }
}

/// `value`, a name list or a scalar, as JSON text.
fn json(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).expect("names and scalars serialize")
}

/// A Parquet file's columns as a report reads them.
pub struct ParquetTable {
    /// The table as `richview-core` reads it for a `data.parquet` block.
    pub text: String,
    /// Columns whose NaN and infinite values were read as missing, with how many.
    pub non_finite: Vec<(String, u64)>,
}

/// Why a Parquet file gave a report no rows: the finding's code, message and fix.
pub struct ParquetRefusal {
    pub code: &'static str,
    pub message: String,
    pub fix: String,
}

/// Read from the Parquet file `source`, which the caller has confined to the report folder,
/// the columns `names` asks for: each column whose name is one of those texts, or every column
/// for `None`. `reference` is the file as the report names it. The file is parsed only in the
/// restricted worker; `worker` is the trusted host's choice of executable, by default the one
/// beside this program.
pub fn read_columns(
    source: &Path,
    reference: &str,
    worker: Option<&Path>,
    names: Option<&[String]>,
) -> Result<ParquetTable, ParquetRefusal> {
    let refusal = |code, message: String, fix: &str| ParquetRefusal {
        code,
        message,
        fix: fix.to_owned(),
    };
    let csv = "or save the data as a CSV file and name it in `data.csv`";
    // The worker opens the file at this path, and DuckDB reads these characters as a pattern.
    // On Windows it opens a copy under a name of the host's own.
    if cfg!(not(windows)) && pattern_path(source) {
        return Err(refusal(
            "data.parquet",
            format!("Parquet file '{reference}' {PATTERN_PATH}."),
            &format!("{PATTERN_PATH_FIX}, {csv}."),
        ));
    }
    match sql_worker::read_table(source, worker, names) {
        Ok(TableRead::Table { text, non_finite }) => Ok(ParquetTable { text, non_finite }),
        Ok(TableRead::TooLarge { rows, by_rows }) => {
            let rows = grouped(rows);
            Err(refusal(
                "data.limit",
                if by_rows {
                    format!(
                        "Parquet file '{reference}' has {rows} rows, over the {} rows one dataset may hold.",
                        grouped(MAX_TABLE_ROWS)
                    )
                } else {
                    format!(
                        "The columns this report reads from Parquet file '{reference}' ({rows} rows) hold more than the 16 MiB one dataset may."
                    )
                },
                "Aggregate the rows with SQL: declare the file as a `parquet` connector in report.connectors.yaml and query it in `data.sql`; or write a smaller file.",
            ))
        }
        Ok(TableRead::Unsupported { column, kind }) => Err(refusal(
            "data.parquet",
            format!(
                "Column '{column}' of Parquet file '{reference}' holds {}, which a chart or table cannot read.",
                kind.noun()
            ),
            &format!(
                "Write '{column}' as text, a number, a boolean or a date when you export the file, or leave it unread: a chart reads only the fields it names, and a table only the `columns` it lists."
            ),
        )),
        Err(failure) => Err(match failure {
            TableFailure::Unreadable => refusal(
                "data.parquet",
                format!(
                    "'{reference}' cannot be read as a Parquet file: it is not one, or it is incomplete, damaged or encrypted."
                ),
                &format!("Export the file again as unencrypted Parquet, {csv}."),
            ),
            TableFailure::Stopped => refusal(
                "data.parquet",
                format!(
                    "The Parquet reader stopped while reading '{reference}'; the file is damaged or needs more than the reader's limits."
                ),
                &format!("Export the file again, {csv}."),
            ),
            TableFailure::Time => refusal(
                "data.parquet",
                format!(
                    "Reading Parquet file '{reference}' took longer than the 30 seconds one read may."
                ),
                "Write a smaller file, or aggregate upstream so the file holds the rows the report shows.",
            ),
            TableFailure::Memory => refusal(
                "data.parquet",
                format!(
                    "Reading Parquet file '{reference}' needs more memory than the 256 MiB the reader may use."
                ),
                "Write a smaller file, or aggregate upstream so the file holds the rows the report shows.",
            ),
            TableFailure::KernelTooOld => refusal(
                "data.parquet",
                format!(
                    "Parquet file '{reference}' was not read: RichView reads Parquet in a confined process, which needs Linux kernel 6.2 or newer with Landlock ABI 3 enabled."
                ),
                "Save the data as a CSV file and name it in `data.csv`, or render on a newer kernel.",
            ),
            TableFailure::Isolation => refusal(
                "data.parquet",
                format!(
                    "Parquet file '{reference}' was not read: RichView reads Parquet in a confined process, `richview-sql-worker`, which is missing beside `richview` or could not be confined here."
                ),
                "Reinstall RichView so `richview-sql-worker` sits beside `richview`, or save the data as a CSV file and name it in `data.csv`.",
            ),
            TableFailure::CopyTooLarge => refusal(
                "data.limit",
                format!(
                    "Parquet file '{reference}' is larger than 4 GiB, the most RichView copies into its Windows sandbox."
                ),
                "Write a smaller file, or aggregate upstream so the file holds the rows the report shows.",
            ),
            TableFailure::CopyNoSpace => refusal(
                "data.parquet",
                format!(
                    "The temporary folder lacks the free space to copy Parquet file '{reference}' into the Windows sandbox."
                ),
                "Free space in the temporary folder, then render again.",
            ),
            TableFailure::Protocol => refusal(
                "data.parquet",
                format!(
                    "Parquet file '{reference}' was not read: the reader's answer was not one RichView accepts."
                ),
                &format!("Render again; if it repeats, export the file again, {csv}."),
            ),
        }),
    }
}

/// `number` with a comma between each three digits.
fn grouped(number: u64) -> String {
    let digits = number.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_decimal_is_a_number_only_when_binary64_holds_its_digits() {
        for text in ["0", "10.50", "-0.25", "123456789012345", "1234567.12345678"] {
            assert!(exact_number(text), "{text}");
        }
        for text in [
            "1234567890123456",
            "0.1234567890123456",
            "007",
            "",
            "-",
            "1e5",
            ".5",
            "5.",
        ] {
            assert!(!exact_number(text), "{text}");
        }
    }

    #[test]
    fn columns_that_are_not_scalars_are_named_by_kind() {
        let kind = |text| match reading(text) {
            Reading::Unsupported(kind) => Some(kind),
            _ => None,
        };
        assert_eq!(kind("INTEGER[]"), Some(ColumnKind::List));
        assert_eq!(kind("STRUCT(a INTEGER)[]"), Some(ColumnKind::List));
        assert_eq!(kind("STRUCT(a INTEGER[])"), Some(ColumnKind::Struct));
        assert_eq!(kind("MAP(VARCHAR, INTEGER)"), Some(ColumnKind::Map));
        assert_eq!(kind("BLOB"), Some(ColumnKind::Binary));
        assert_eq!(kind("TIMESTAMP WITH TIME ZONE"), None);
        assert!(matches!(
            reading("DECIMAL(18,2)"),
            Reading::Text { numeric: true }
        ));
        assert!(matches!(reading("UUID"), Reading::Text { numeric: false }));
    }

    #[test]
    fn row_counts_read_in_groups_of_three() {
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1_000_000), "1,000,000");
        assert_eq!(grouped(12_345_678), "12,345,678");
    }
}
