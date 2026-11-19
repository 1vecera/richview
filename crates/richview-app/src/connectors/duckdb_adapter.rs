//! Local SQL connector backed by DuckDB opened read-only.
//!
//! The trusted host delegates SQL to its installed, OS-sandboxed query worker.
//! The worker opens only the declared database read-only. Engine settings,
//! bounded results and a watchdog supplement the process boundary.

use super::registry::Connector;
use super::spec::{
    ConnectorError, DatasetSnapshot, FieldKind, Provenance, ProvenanceSource, Query, QueryBody,
    SchemaField, SqlError, SqlPosition,
};
use duckdb::{
    Config, Connection, ToSql,
    arrow::datatypes::DataType,
    types::{TimeUnit, Value as SqlValue, ValueRef},
};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[path = "sql_guard.rs"]
mod sql_guard;
#[cfg(test)]
const QUERY_TIMEOUT: Duration = Duration::from_secs(30);

/// Hard row ceiling. A report that renders a chart from millions of rows is
/// almost certainly a mistake; the connector refuses before the renderer sees
/// the result so authors get a clear failure instead of a frozen tab.
pub(super) use super::spec::MAX_ROWS;
pub(super) const UNSUPPORTED_SCALAR: &str =
    "SQL result is outside supported scalar types or range; cast it to text";
pub(super) use sql_guard::METADATA_REFUSAL;
const QUERY_PREFIX: &str = "SELECT * FROM (\n";

pub struct DuckDbConnector {
    database: PathBuf,
    worker: Option<PathBuf>,
    /// The run's copies of the source, for the Windows worker; they go with the connector.
    copies: super::sql_worker::SourceCopies,
}

impl DuckDbConnector {
    pub fn new(database: PathBuf) -> Self {
        let database = database.canonicalize().unwrap_or(database);
        Self {
            database,
            worker: None,
            copies: Default::default(),
        }
    }

    /// Bind a worker executable chosen by trusted host configuration, never report content.
    pub fn with_worker(database: PathBuf, worker: PathBuf) -> Self {
        let database = database.canonicalize().unwrap_or(database);
        Self {
            database,
            worker: Some(worker),
            copies: Default::default(),
        }
    }
}

impl Connector for DuckDbConnector {
    fn kind(&self) -> &'static str {
        "duckdb"
    }

    fn source_identity(&self) -> String {
        let mut hash = Sha256::new();
        // Cached rows produced before metadata isolation must not survive the policy change.
        hash.update(b"duckdb-file-v1\0");
        hash.update([super::spec::SQL_METADATA_POLICY_VERSION]);
        hash.update(self.database.as_os_str().as_encoded_bytes());
        format!("{:x}", hash.finalize())
    }

    fn execute(&self, request: &Query<'_>) -> Result<DatasetSnapshot, ConnectorError> {
        let QueryBody::Sql(sql) = request.body else {
            return Err(ConnectorError::QueryRejected {
                reason: "duckdb connector accepts only data.sql blocks".into(),
            });
        };
        read_only_guard(sql)?;
        for value in request.params.values() {
            sql_value(value)?;
        }
        if self
            .database
            .canonicalize()
            .is_ok_and(|path| path != self.database)
        {
            return Err(ConnectorError::QueryRejected {
                reason: "SQL source binding changed; reload trusted connector configuration".into(),
            });
        }
        super::sql_worker::execute_in(
            &self.copies,
            &self.database,
            self.worker.as_deref(),
            request,
        )
    }
}

impl DuckDbConnector {
    /// Open the declared source in the restricted worker; no report controls engine settings.
    #[cfg(test)]
    pub(super) fn execute_read_only(
        database: &Path,
        request: &Query<'_>,
        timeout: Duration,
    ) -> Result<DatasetSnapshot, ConnectorError> {
        Self::execute_read_only_with(
            || Self::open_read_only(database, request.connector),
            request,
            timeout,
        )
    }

    /// Run the request on the connection `open` returns, which is called only after the
    /// request's SQL and values have been checked.
    pub(super) fn execute_read_only_with(
        open: impl FnOnce() -> Result<Connection, ConnectorError>,
        request: &Query<'_>,
        timeout: Duration,
    ) -> Result<DatasetSnapshot, ConnectorError> {
        let sql = match request.body {
            QueryBody::Sql(sql) => sql,
            _ => {
                return Err(ConnectorError::QueryRejected {
                    reason: "duckdb connector accepts only data.sql blocks".into(),
                });
            }
        };
        let source_sql = read_only_guard(sql)?;
        // The outer LIMIT bounds what DuckDB materializes, while one extra row proves overflow.
        let sql = format!(
            "{QUERY_PREFIX}{source_sql}\n) AS richview_query LIMIT {}",
            MAX_ROWS + 1
        );
        let values = request
            .params
            .iter()
            .map(|(name, value)| Ok((name.as_str(), sql_value(value)?)))
            .collect::<Result<Vec<_>, ConnectorError>>()?;
        let params: Vec<(&str, &dyn ToSql)> = values
            .iter()
            .map(|(name, value)| (*name, value as &dyn ToSql))
            .collect();

        let connection = open()?;
        with_timeout(&connection, timeout, || {
            Self::check_definitions(&connection, request.connector)?;
            let mut statement = connection
                .prepare(&sql)
                .map_err(duck_err(request.connector, Some(source_sql)))?;

            let mut rows_iter = statement
                .query(params.as_slice())
                .map_err(duck_err(request.connector, Some(source_sql)))?;
            let column_names: Vec<String> = rows_iter
                .as_ref()
                .map(|stmt| {
                    stmt.column_names()
                        .into_iter()
                        .map(|c| c.to_owned())
                        .collect()
                })
                .unwrap_or_default();
            let column_types: Vec<DataType> = rows_iter
                .as_ref()
                .map(|statement| {
                    (0..column_names.len())
                        .map(|index| statement.column_type(index))
                        .collect()
                })
                .unwrap_or_default();
            let mut rows: Vec<Value> = Vec::new();
            let mut schema_kinds: Vec<Option<FieldKind>> = column_types
                .iter()
                .map(|kind| match kind {
                    DataType::Date32 => Some(FieldKind::Date),
                    DataType::Timestamp(_, _) => Some(FieldKind::Timestamp),
                    _ => None,
                })
                .collect();
            let mut nullable = vec![false; column_names.len()];
            while let Some(row) = rows_iter
                .next()
                .map_err(duck_err(request.connector, Some(source_sql)))?
            {
                if rows.len() >= MAX_ROWS {
                    return Err(ConnectorError::QueryRejected {
                        reason: format!("result exceeds {MAX_ROWS} rows; narrow the query"),
                    });
                }
                let mut object = Map::with_capacity(column_names.len());
                for (index, name) in column_names.iter().enumerate() {
                    let (value, kind) = convert_value(
                        row.get_ref(index)
                            .map_err(duck_err(request.connector, Some(source_sql)))?,
                        matches!(column_types[index], DataType::Timestamp(_, Some(_))),
                    )?;
                    if kind == Some(FieldKind::Null) {
                        nullable[index] = true;
                    } else if let Some(kind) = kind {
                        match &mut schema_kinds[index] {
                            slot @ None => *slot = Some(kind),
                            Some(current) if *current != kind => *current = widen(*current, kind),
                            _ => {}
                        }
                    }
                    object.insert(name.clone(), value);
                }
                rows.push(Value::Object(object));
            }

            let schema = column_names
                .into_iter()
                .enumerate()
                .map(|(index, name)| SchemaField {
                    name,
                    kind: schema_kinds[index].unwrap_or(FieldKind::Null),
                    nullable: nullable[index] || schema_kinds[index].is_none(),
                })
                .collect();

            let row_count = rows.len();
            Ok(DatasetSnapshot {
                rows,
                schema,
                provenance: Provenance {
                    connector: request.connector.to_owned(),
                    source: ProvenanceSource::Sql,
                    fetched_at: now_rfc3339(),
                    fingerprint: request.fingerprint.to_owned(),
                    row_count,
                },
                cache: None,
            })
        })
    }
    pub(super) fn open_read_only(
        database: &Path,
        connector: &str,
    ) -> Result<Connection, ConnectorError> {
        let config = Config::default()
            .access_mode(duckdb::AccessMode::ReadOnly)
            .and_then(|config| config.enable_external_access(false))
            .and_then(|config| config.enable_autoload_extension(false))
            .and_then(|config| config.with("memory_limit", "128MB"))
            .and_then(|config| config.with("threads", "2"))
            .map_err(duck_err(connector, None))?;
        let connection =
            Connection::open_with_flags(database, config).map_err(duck_err(connector, None))?;
        // Client settings need an initialized connection. Lock it before preparing report SQL.
        connection
            .execute_batch("SET errors_as_json = true; SET lock_configuration = true")
            .map_err(duck_err(connector, None))?;
        Ok(connection)
    }

    fn check_definitions(connection: &Connection, connector: &str) -> Result<(), ConnectorError> {
        // Qualify built-ins against the system catalog: a report database cannot shadow them.
        let mut statement = connection.prepare(
            "SELECT sql FROM system.main.duckdb_views() WHERE NOT internal
             UNION ALL SELECT sql FROM system.main.duckdb_tables() WHERE NOT internal
             UNION ALL SELECT macro_definition FROM system.main.duckdb_functions() WHERE NOT internal AND macro_definition IS NOT NULL"
        ).map_err(duck_err(connector, Some("")))?;
        let mut rows = statement.query([]).map_err(duck_err(connector, Some("")))?;
        while let Some(row) = rows.next().map_err(duck_err(connector, Some("")))? {
            if let Some(sql) = row
                .get::<_, Option<String>>(0)
                .map_err(duck_err(connector, Some("")))?
            {
                sql_guard::metadata_free(&sql).map_err(|_| ConnectorError::QueryRejected {
                    reason: METADATA_REFUSAL.into(),
                })?;
            }
        }
        Ok(())
    }
}

fn sql_value(value: &Value) -> Result<SqlValue, ConnectorError> {
    Ok(match value {
        Value::String(s) => SqlValue::Text(s.clone()),
        Value::Bool(b) => SqlValue::Boolean(*b),
        Value::Number(n) => {
            if let Some(n) = n.as_i64() {
                SqlValue::BigInt(n)
            } else if let Some(n) = n.as_u64() {
                SqlValue::UBigInt(n)
            } else {
                SqlValue::Double(n.as_f64().expect("finite JSON number"))
            }
        }
        _ => {
            return Err(ConnectorError::QueryRejected {
                reason: "SQL parameters must be named strings, numbers, booleans or date strings"
                    .into(),
            });
        }
    })
}

fn convert_value(
    value: ValueRef<'_>,
    zoned: bool,
) -> Result<(Value, Option<FieldKind>), ConnectorError> {
    use ValueRef::*;
    let unsupported = || ConnectorError::QueryRejected {
        reason: UNSUPPORTED_SCALAR.into(),
    };
    Ok(match value {
        Null => (Value::Null, Some(FieldKind::Null)),
        Boolean(b) => (Value::Bool(b), Some(FieldKind::Boolean)),
        TinyInt(n) => (Value::from(n as i64), Some(FieldKind::Integer)),
        SmallInt(n) => (Value::from(n as i64), Some(FieldKind::Integer)),
        Int(n) => (Value::from(n as i64), Some(FieldKind::Integer)),
        BigInt(n) => (Value::from(n), Some(FieldKind::Integer)),
        HugeInt(n) => (Value::from(n.to_string()), Some(FieldKind::String)),
        UHugeInt(n) => (Value::from(n.to_string()), Some(FieldKind::String)),
        Decimal(n) => (Value::from(n.to_string()), Some(FieldKind::String)),
        UTinyInt(n) => (Value::from(n as u64), Some(FieldKind::Integer)),
        USmallInt(n) => (Value::from(n as u64), Some(FieldKind::Integer)),
        UInt(n) => (Value::from(n as u64), Some(FieldKind::Integer)),
        UBigInt(n) => (Value::from(n), Some(FieldKind::Integer)),
        Float(f) => (
            Value::Number(serde_json::Number::from_f64(f as f64).ok_or_else(unsupported)?),
            Some(FieldKind::Number),
        ),
        Double(f) => (
            Value::Number(serde_json::Number::from_f64(f).ok_or_else(unsupported)?),
            Some(FieldKind::Number),
        ),
        Date32(days) => {
            let date = crate::time::date_after_epoch(i64::from(days)).ok_or_else(unsupported)?;
            (Value::String(date), Some(FieldKind::Date))
        }
        Timestamp(unit, value) => {
            let per_second = match unit {
                TimeUnit::Second => 1,
                TimeUnit::Millisecond => 1_000,
                TimeUnit::Microsecond => 1_000_000,
                TimeUnit::Nanosecond => 1_000_000_000,
            };
            let seconds = value.div_euclid(per_second);
            let nanos = (value.rem_euclid(per_second) * (1_000_000_000 / per_second)) as u32;
            let instant =
                crate::time::Instant::from_unix(seconds, nanos).ok_or_else(unsupported)?;
            let text = if zoned {
                instant.rfc3339_auto()
            } else {
                instant.naive_auto()
            };
            (Value::String(text), Some(FieldKind::Timestamp))
        }
        Text(bytes) => (
            Value::from(
                std::str::from_utf8(bytes)
                    .map_err(|_| unsupported())?
                    .to_owned(),
            ),
            Some(FieldKind::String),
        ),
        Blob(bytes) => (
            Value::from(format!("0x{}", hex(bytes))),
            Some(FieldKind::String),
        ),
        _ => return Err(unsupported()),
    })
}

fn widen(a: FieldKind, b: FieldKind) -> FieldKind {
    match (a, b) {
        (FieldKind::Integer, FieldKind::Number) | (FieldKind::Number, FieldKind::Integer) => {
            FieldKind::Number
        }
        _ => FieldKind::String,
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn now_rfc3339() -> String {
    crate::time::Instant::now().seconds()
}

// Native error text may contain SQL literals, URLs and host paths. Keep it off every report surface.
fn duck_err<'a>(
    connector: &'a str,
    sql: Option<&'a str>,
) -> impl Fn(duckdb::Error) -> ConnectorError + 'a {
    move |err| {
        let native = err.to_string();
        let data = serde_json::from_str::<Value>(&native)
            .or_else(|_| {
                let encoded = native
                    .split_once(" Error: ")
                    .map_or(native.as_str(), |(_, body)| body);
                serde_json::from_str(encoded)
            })
            .unwrap_or(Value::Null);
        let kind = data["exception_type"].as_str().unwrap_or("");
        if kind == "Out of Memory" || native.starts_with("Out of Memory Error:") {
            return ConnectorError::QueryRejected {
                reason: "SQL query exceeded its memory limit".into(),
            };
        }
        if matches!(
            err,
            duckdb::Error::InvalidParameterName(_) | duckdb::Error::InvalidParameterCount(_, _)
        ) {
            return ConnectorError::QueryRejected {
                reason: "SQL parameters must match every named placeholder exactly; positional placeholders are not supported".into(),
            };
        }
        let detail = data["exception_message"].as_str().unwrap_or(&native);
        let error = if sql.is_none() {
            if ((detail.starts_with("Trying to read a database file with version number")
                || detail.starts_with("IO Error: Trying to read a database file with version number"))
                && detail.contains("The database file was created with a newer version of DuckDB"))
                || detail.contains("file was written with a storage version greater than the latest version supported") {
                SqlError::DatabaseVersion
            } else {
                SqlError::Database
            }
        } else {
            match (kind, data["error_subtype"].as_str().unwrap_or("")) {
                ("Parser", _) => SqlError::Syntax {
                    position: sql.and_then(|sql| sql_position(sql, &data["position"])),
                },
                ("Catalog", "MISSING_ENTRY")
                    if matches!(data["type"].as_str(), Some("Table" | "View")) =>
                {
                    SqlError::MissingTable
                }
                ("Binder", "COLUMN_NOT_FOUND") => SqlError::MissingColumn,
                ("Conversion", _) => SqlError::Conversion,
                ("Mismatch Type", _) | ("Binder", "NO_MATCHING_FUNCTION") => SqlError::TypeMismatch,
                ("Permission", _) => SqlError::ExternalAccess,
                _ => SqlError::Query,
            }
        };
        ConnectorError::Sql {
            connector: connector.to_owned(),
            error,
        }
    }
}

fn sql_position(author: &str, position: &Value) -> Option<SqlPosition> {
    let position: usize = position.as_str()?.parse().ok()?;
    let position = position.checked_sub(QUERY_PREFIX.len())?;
    // DuckDB's parser reports characters, not UTF-8 bytes, including the outer query.
    let end = author
        .char_indices()
        .map(|(index, _)| index)
        .chain(std::iter::once(author.len()))
        .nth(position)?;
    let prefix = &author[..end];
    Some(SqlPosition {
        line: u32::try_from(prefix.bytes().filter(|byte| *byte == b'\n').count() + 1).ok()?,
        column: u32::try_from(prefix.rsplit('\n').next()?.chars().count() + 1).ok()?,
    })
}

pub(super) fn read_only_guard(sql: &str) -> Result<&str, ConnectorError> {
    sql_guard::single_query(sql)
}

/// Interrupt preparation, execution and result collection at the deadline, then join the watchdog.
fn with_timeout<T>(
    connection: &Connection,
    timeout: Duration,
    run: impl FnOnce() -> Result<T, ConnectorError>,
) -> Result<T, ConnectorError> {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    };
    let expired = AtomicBool::new(false);
    let interrupt = connection.interrupt_handle();
    std::thread::scope(|scope| {
        let (done, wait) = mpsc::channel();
        let expired = &expired;
        scope.spawn(move || {
            if matches!(
                wait.recv_timeout(timeout),
                Err(mpsc::RecvTimeoutError::Timeout)
            ) {
                expired.store(true, Ordering::Release);
                loop {
                    interrupt.interrupt();
                    if !matches!(
                        wait.recv_timeout(Duration::from_millis(10)),
                        Err(mpsc::RecvTimeoutError::Timeout)
                    ) {
                        break;
                    }
                }
            }
        });
        let result = run();
        let _ = done.send(());
        if expired.load(Ordering::Acquire) {
            Err(ConnectorError::QueryRejected {
                reason: format!("SQL query exceeded the {} ms timeout", timeout.as_millis()),
            })
        } else {
            result
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn read_only_guard_blocks_mutation() {
        assert!(read_only_guard("select * from t").is_ok());
        assert!(read_only_guard("SELECT 1").is_ok());
        assert!(read_only_guard("insert into t values (1)").is_err());
        assert!(read_only_guard("select 1; drop table t").is_err());
        assert!(read_only_guard("update t set x = 1").is_err());
        for sql in [
            "SET enable_external_access=true",
            "-- comment\nDELETE FROM t",
            "SELECT 1; SET enable_external_access=true",
            "SELECT 1;SELECT 2",
            "SELECT 1 -- comment\r) AS escaped; SELECT 2; SELECT * FROM (SELECT 3",
            "/* comment */ COPY t TO 'host.csv'",
        ] {
            assert!(read_only_guard(sql).is_err(), "{sql}");
        }
        for sql in [
            "SELECT 'drop table t; still text' AS label; -- trailing comment",
            "/* a /* nested */ comment */ WITH t AS (SELECT 1 AS n) SELECT n FROM t",
            "SELECT $$quoted; not a statement$$ AS label",
            "SELECT E'escaped\\'; not a statement' AS label",
        ] {
            assert!(read_only_guard(sql).is_ok(), "{sql}");
        }
    }

    #[test]
    fn report_sql_refuses_host_metadata_and_dynamic_query_indirection() {
        for sql in [
            "SELECT * FROM duckdb_settings()",
            "SELECT current_setting('temp_directory')",
            "SELECT * FROM main.\"DUCKDB_DATABASES\"()",
            "SELECT * FROM pragma_database_list()",
            "SELECT * FROM duckdb_views",
            "SELECT * FROM query('SELECT * FROM duckdb_settings()')",
            "SELECT * FROM query_table($table)",
            "SELECT * FROM json_execute_serialized_sql(json_serialize_sql('SEL' || 'ECT value FROM duckdb_settings()'))",
            "SELECT getenv('HOME')",
            "SELECT U&\"current\\005fsetting\"('temp_directory')",
        ] {
            assert!(
                read_only_guard(sql).is_err(),
                "metadata reached report SQL: {sql}"
            );
        }
        for sql in [
            "SELECT 'duckdb_settings()' AS label",
            "SELECT $$current_setting('temp_directory')$$ AS label",
            "SELECT 1 AS n /* duckdb_databases() */",
            "SELECT query, current_setting FROM source",
        ] {
            assert!(
                read_only_guard(sql).is_ok(),
                "literal/comment refused: {sql}"
            );
        }
        assert!(
            sql_guard::metadata_free("CREATE TABLE query(query VARCHAR, current_setting VARCHAR);")
                .is_ok()
        );
    }

    #[test]
    fn sql_failures_name_the_category_without_echoing_native_details() {
        let (_dir, connector) = fixture();
        for (sql, expected) in [
            (
                "SELECT 1 + FROM missing_secret_table",
                "SQL syntax error at line 1, column 12",
            ),
            (
                "SELECT * FROM missing_secret_table",
                "SQL table or view was not found",
            ),
            (
                "SELECT missing_secret_column FROM (SELECT 1 AS n)",
                "SQL column was not found",
            ),
            (
                "SELECT CAST('private-secret-value' AS INTEGER)",
                "SQL value could not be converted to the requested type",
            ),
        ] {
            let error = query(&connector, sql, serde_json::json!({}))
                .unwrap_err()
                .to_string();
            assert!(error.contains(expected), "{sql}: {error}");
            assert!(!error.contains("secret"), "{error}");
            assert!(!error.contains("External access"), "{error}");
        }
    }

    fn fixture() -> (tempfile::TempDir, DuckDbConnector) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("warehouse.duckdb");
        Connection::open(&path).unwrap().close().unwrap();
        (dir, DuckDbConnector::new(path))
    }

    fn query(
        connector: &DuckDbConnector,
        sql: &str,
        params: serde_json::Value,
    ) -> Result<DatasetSnapshot, ConnectorError> {
        DuckDbConnector::execute_read_only(
            &connector.database,
            &Query {
                connector: "local",
                body: QueryBody::Sql(sql),
                params: &serde_json::from_value(params).unwrap(),
                cache: None,
                fingerprint: "0123456789abcdef",
            },
            QUERY_TIMEOUT,
        )
    }

    #[test]
    fn named_parameters_are_typed_values_never_sql_fragments() {
        let (_dir, connector) = fixture();
        let attack = "'); DROP TABLE orders; --";
        let result = query(
            &connector,
            "SELECT $label AS label, $whole AS whole, $fraction AS fraction, $flag AS flag, CAST($day AS DATE)::VARCHAR AS day, $label AS repeated",
            serde_json::json!({"label": attack, "whole": 42, "fraction": 1.25, "flag": true, "day": "2026-10-29"}),
        ).unwrap();
        assert_eq!(
            result.rows,
            [
                serde_json::json!({"label": attack, "whole": 42, "fraction": 1.25, "flag": true, "day": "2026-10-29", "repeated": attack})
            ]
        );
    }

    #[test]
    fn missing_extra_positional_and_non_scalar_parameters_are_rejected() {
        let (_dir, connector) = fixture();
        for (sql, params) in [
            ("SELECT $missing AS n", serde_json::json!({})),
            ("SELECT 1 AS n", serde_json::json!({"unused": 1})),
            ("SELECT ? AS n", serde_json::json!({"1": 1})),
            ("SELECT $n AS n", serde_json::json!({"n": [1, 2]})),
            ("SELECT $n AS n", serde_json::json!({"n": {"value": 1}})),
            ("SELECT $n AS n", serde_json::json!({"n": null})),
        ] {
            assert!(
                matches!(
                    query(&connector, sql, params),
                    Err(ConnectorError::QueryRejected { .. })
                ),
                "{sql}"
            );
        }
    }

    #[test]
    fn sql_cannot_read_host_files_or_enable_external_access() {
        let (dir, connector) = fixture();
        let secret = dir.path().join("credential.csv");
        std::fs::write(&secret, "value\nhost-only-sentinel\n").unwrap();
        for function in ["read_csv_auto", "read_text", "read_blob"] {
            let sql = format!("SELECT * FROM {function}('{}')", secret.display());
            assert!(
                query(&connector, &sql, serde_json::json!({})).is_err(),
                "{function} reached a host file"
            );
        }
        let connection = DuckDbConnector::open_read_only(&connector.database, "local").unwrap();
        let settings: (bool, bool, bool, bool) = connection.query_row(
            "SELECT current_setting('enable_external_access') AS enabled, current_setting('autoload_known_extensions') AS autoload, current_setting('autoinstall_known_extensions') AS install, current_setting('lock_configuration') AS locked",
            [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
        assert_eq!(settings, (false, false, false, true));
        drop(connection);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let sql = format!(
            "SELECT * FROM read_csv_auto('http://{}/credential.csv')",
            listener.local_addr().unwrap()
        );
        assert!(query(&connector, &sql, serde_json::json!({})).is_err());
        assert!(
            matches!(listener.accept(), Err(err) if err.kind() == std::io::ErrorKind::WouldBlock)
        );
    }

    #[test]
    fn driver_failures_never_reveal_sql_values_or_source_paths() {
        let (dir, connector) = fixture();
        let error = query(
            &connector,
            "SELECT CAST('credential-sentinel' AS INTEGER) AS n",
            serde_json::json!({}),
        )
        .unwrap_err()
        .to_string();
        assert!(!error.contains("credential-sentinel"), "{error}");
        let error = query(
            &connector,
            "SELECT CAST('Out of Memory Error: credential-sentinel' AS INTEGER) AS n",
            serde_json::json!({}),
        )
        .unwrap_err();
        assert!(
            matches!(
                error,
                ConnectorError::Sql {
                    error: SqlError::Conversion,
                    ..
                }
            ),
            "{error}"
        );
        assert!(
            !error.to_string().contains("credential-sentinel"),
            "{error}"
        );
        let missing = DuckDbConnector::new(dir.path().join("private-credential-sentinel.duckdb"));
        let error = query(&missing, "SELECT 1 AS n", serde_json::json!({}))
            .unwrap_err()
            .to_string();
        assert!(
            !error.contains("private-credential-sentinel")
                && !error.contains(&dir.path().display().to_string()),
            "{error}"
        );
    }

    #[test]
    fn sql_refuses_more_than_100000_rows_instead_of_truncating() {
        let (_dir, connector) = fixture();
        assert_eq!(
            query(
                &connector,
                "SELECT i FROM range(100000) AS t(i)",
                serde_json::json!({})
            )
            .unwrap()
            .rows
            .len(),
            MAX_ROWS
        );
        for count in [100_001_u64, 1_000_000_000_000] {
            let error = query(
                &connector,
                &format!("SELECT i FROM range({count}) AS t(i)"),
                serde_json::json!({}),
            )
            .unwrap_err()
            .to_string();
            assert!(error.contains("100000"), "{error}");
        }
    }

    #[test]
    fn a_running_query_is_interrupted_and_the_watchdog_is_joined() {
        let (_dir, connector) = fixture();
        let params = BTreeMap::new();
        let request = Query {
            connector: "local",
            body: QueryBody::Sql(
                "WITH RECURSIVE t(n) AS (SELECT 0 UNION ALL SELECT n+1 FROM t WHERE n < 100000) SELECT SUM(n) AS total FROM t",
            ),
            params: &params,
            cache: None,
            fingerprint: "0123456789abcdef",
        };
        let started = std::time::Instant::now();
        let error = DuckDbConnector::execute_read_only(
            &connector.database,
            &request,
            Duration::from_millis(20),
        )
        .unwrap_err();
        assert!(error.to_string().contains("timeout"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(
            query(&connector, "SELECT 7 AS n", serde_json::json!({}))
                .unwrap()
                .rows[0]["n"],
            7
        );
    }

    #[test]
    fn duckdb_round_trips_a_seeded_table() {
        // Removed when the test ends, pass or fail.
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("warehouse.duckdb");
        {
            let connection = Connection::open(&db_path).unwrap();
            connection
                .execute_batch(
                    "create table monthly_orders(month text primary key, orders integer not null);\
                     insert into monthly_orders values ('Jul',120),('Aug',150),('Sep',180);",
                )
                .unwrap();
        }

        let connector = DuckDbConnector::new(db_path);
        let params = BTreeMap::new();
        let request = Query {
            connector: "local-duckdb",
            body: QueryBody::Sql("select month, orders from monthly_orders order by month"),
            params: &params,
            cache: None,
            fingerprint: "test-fp",
        };
        let snapshot =
            DuckDbConnector::execute_read_only(&connector.database, &request, QUERY_TIMEOUT)
                .unwrap();
        assert_eq!(snapshot.rows.len(), 3);
        assert_eq!(snapshot.provenance.row_count, 3);
        assert_eq!(snapshot.schema[0].name, "month");
        assert_eq!(snapshot.schema[0].kind, FieldKind::String);
        assert_eq!(snapshot.schema[1].kind, FieldKind::Integer);
        // `order by month` sorts text alphabetically: Aug, Jul, Sep.
        assert_eq!(snapshot.rows[0]["month"], serde_json::Value::from("Aug"));
        assert_eq!(snapshot.rows[0]["orders"], serde_json::Value::from(150_i64));
        assert_eq!(snapshot.rows[2]["month"], serde_json::Value::from("Sep"));
    }
}
