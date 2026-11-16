//! End-to-end checks for the SQL connector: seed a DuckDB warehouse from the
//! committed `examples/sql-report/data/seed.sql`, resolve the report through
//! the connector registry, render it, and verify the chart carries the right
//! rows plus a cache snapshot that is reused on a second call.

use duckdb::Connection;
use richview_app::connectors;
use richview_app::resolver;
use serde_json::Value;
use std::{fs, path::PathBuf};

/// A temporary copy of the example: the test seeds a warehouse and a cache, and a test must not
/// write into the checkout. The copy goes when the guard drops.
fn example_copy() -> tempfile::TempDir {
    fn copy(from: &std::path::Path, to: &std::path::Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap().flatten() {
            let name = entry.file_name();
            if name == "warehouse.duckdb" || name == ".richview-cache" {
                continue;
            }
            let target = to.join(&name);
            if entry.file_type().unwrap().is_dir() {
                copy(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), target).unwrap();
            }
        }
    }
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples/sql-report");
    let dir = tempfile::tempdir().unwrap();
    copy(&source, dir.path());
    dir
}

fn seed_warehouse(root: &std::path::Path) {
    let seed = root.join("data/seed.sql");
    let db = root.join("data/warehouse.duckdb");
    let _ = fs::remove_file(&db);
    let script = fs::read_to_string(&seed).expect("seed.sql");
    let connection = Connection::open(&db).expect("open duckdb");
    connection.execute_batch(&script).expect("seed executes");
}

fn clear_cache(root: &std::path::Path) {
    let cache = root.join(".richview-cache");
    if cache.exists() {
        fs::remove_dir_all(&cache).unwrap();
    }
}

#[test]
fn sql_report_renders_through_the_duckdb_connector_and_caches_the_snapshot() {
    let copy = example_copy();
    let root = copy.path().to_path_buf();
    seed_warehouse(&root);
    clear_cache(&root);

    let report = root.join("report.rmd");
    for verb in ["validate", "check"] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_richview"))
            .args([verb, report.to_str().unwrap(), "--format", "json"])
            .env("XDG_STATE_HOME", root.join("state"))
            .output()
            .unwrap();
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(output.status.success(), "{verb}: {result}");
        assert_eq!(result["status"], "clean", "{verb}: {result}");
    }
    let source = fs::read_to_string(&report).unwrap();
    let trusted = root.join("host-connectors.toml");
    fs::write(
        &trusted,
        format!(
            "sql_worker = {}\ncache_root = \".host-cache\"\n",
            serde_json::to_string(env!("CARGO_BIN_EXE_richview-sql-worker")).unwrap()
        ),
    )
    .unwrap();

    let registry = connectors::load_registry(&root, Some(&trusted))
        .expect("registry loads")
        .expect("sql-report declares connectors");

    let (document, options) = resolver::resolve_document_options_full(
        &source,
        &report,
        &[],
        false,
        false,
        Some(&registry),
    )
    .expect("resolve");

    // Chart block carries the resolved rows and a provenance record.
    let chart = document
        .blocks
        .iter()
        .find(|b| b.tag == "chart")
        .expect("chart block");
    let rows = chart
        .value
        .pointer("/data/inline")
        .and_then(Value::as_array)
        .expect("inline rows");
    assert_eq!(rows.len(), 3);
    let months: Vec<&str> = rows
        .iter()
        .filter_map(|r| r.get("month").and_then(Value::as_str))
        .collect();
    assert_eq!(months, vec!["Jul", "Aug", "Sep"]);
    let orders: Vec<i64> = rows
        .iter()
        .filter_map(|r| r.get("orders").and_then(Value::as_i64))
        .collect();
    assert_eq!(orders, vec![120, 150, 180]);
    let provenance = chart
        .value
        .pointer("/data/_provenance")
        .and_then(Value::as_object)
        .expect("provenance recorded");
    assert_eq!(
        provenance.get("source").and_then(Value::as_str),
        Some("sql")
    );
    let fingerprint = provenance
        .get("fingerprint")
        .and_then(Value::as_str)
        .expect("fingerprint")
        .to_owned();

    // A diagnostic was emitted at info level naming the row count.
    assert!(
        document
            .diagnostics
            .iter()
            .any(|d| d.code == "data.provenance" && d.message.contains("3 rows"))
    );

    // The trusted store scopes snapshots to this report, outside the authored cache folder.
    let cache_path = registry.store().root().join(format!("{fingerprint}.json"));
    assert!(!root.join(".richview-cache").exists());
    assert!(
        cache_path.exists(),
        "snapshot not written to {cache_path:?}"
    );

    // The rendered HTML contains the resolved values.
    let rendered =
        richview_core::document::render_document(document, &options).expect("render document");
    assert!(rendered.html.contains("150"), "chart missing August orders");
    assert!(
        rendered.html.contains("180"),
        "chart missing September orders"
    );

    // A second resolve with no cache policy on the block uses the stored snapshot.
    // Prove it by replacing the warehouse with a different value; the cached
    // rows should still be returned because the fingerprint matches.
    {
        let connection = Connection::open(root.join("data/warehouse.duckdb")).unwrap();
        connection
            .execute_batch(
                "delete from monthly_orders; insert into monthly_orders values ('Oct', 999);",
            )
            .unwrap();
    }
    let (document_2, _) = resolver::resolve_document_options_full(
        &source,
        &report,
        &[],
        false,
        false,
        Some(&registry),
    )
    .expect("resolve twice");
    let rows_2 = document_2
        .blocks
        .iter()
        .find(|b| b.tag == "chart")
        .and_then(|b| b.value.pointer("/data/inline").and_then(Value::as_array))
        .unwrap();
    // Cache honoured despite the warehouse change — the block carries no TTL
    // in the example, so the stored snapshot is served indefinitely.
    assert_eq!(rows_2.len(), 3, "cached snapshot was not reused");
    assert!(
        rows_2
            .iter()
            .any(|r| r.get("orders").and_then(Value::as_i64) == Some(150)),
        "cached snapshot content differed from the first render"
    );
}

fn worker_fixture() -> (
    tempfile::TempDir,
    connectors::duckdb_adapter::DuckDbConnector,
) {
    let dir = tempfile::tempdir().unwrap();
    let database = dir.path().join("source.duckdb");
    Connection::open(&database)
        .unwrap()
        .execute_batch("CREATE TABLE source AS SELECT 7 AS n")
        .unwrap();
    let connector = connectors::duckdb_adapter::DuckDbConnector::with_worker(
        database,
        PathBuf::from(env!("CARGO_BIN_EXE_richview-sql-worker")),
    );
    (dir, connector)
}

fn worker_query(
    connector: &connectors::duckdb_adapter::DuckDbConnector,
    sql: &str,
    params: Value,
) -> Result<connectors::DatasetSnapshot, connectors::ConnectorError> {
    use connectors::registry::Connector;
    connector.execute(&connectors::Query {
        connector: "local",
        body: connectors::QueryBody::Sql(sql),
        params: &serde_json::from_value(params).unwrap(),
        cache: None,
        fingerprint: "worker-fingerprint",
    })
}

#[test]
fn hostile_sql_metadata_is_refused_at_the_report_and_worker_boundaries() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let (dir, connector) = worker_fixture();
    let ordinary = worker_query(
        &connector,
        "SELECT 'duckdb_settings()' AS label",
        serde_json::json!({}),
    )
    .unwrap();
    assert_eq!(
        ordinary.rows,
        [serde_json::json!({"label":"duckdb_settings()"})]
    );
    let mut registry = connectors::Registry::new(dir.path().join("cache"));
    registry.register("local", connector);
    let mut document =
        richview_core::document::parse(include_str!("hostile/sql-metadata.rmd")).unwrap();
    let mut diagnostics = Vec::new();
    connectors::resolve::resolve_connector_data(
        &mut document,
        dir.path(),
        &registry,
        &mut richview_core::document::RenderOptions::default(),
        &mut diagnostics,
    )
    .unwrap();
    let error = diagnostics
        .iter()
        .find(|d| d.level == "error")
        .expect("metadata must be refused");
    assert_eq!(
        (&*error.code, &*error.path, error.line, error.tag.as_deref()),
        (
            "data.connector.rejected",
            "table.data",
            Some(document.blocks[0].line),
            Some("table")
        )
    );
    assert!(error.message.contains("host metadata"), "{error:?}");
    assert!(document.blocks[0].value["data"]["inline"].is_null());
    assert!(
        !serde_json::to_string(&diagnostics)
            .unwrap()
            .contains(&dir.path().display().to_string())
    );

    // Raw requests bypass the host preguard: the restricted worker must enforce its own policy.
    let corpus: Vec<Value> =
        serde_json::from_str(include_str!("hostile/sql-metadata.json")).unwrap();
    for case in corpus {
        let mut child = Command::new(env!("CARGO_BIN_EXE_richview-sql-worker"))
            .env_clear()
            .env("SLURM_MEM_PER_NODE", "128M")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let request = serde_json::json!({"version":2,"source":dir.path().join("source.duckdb"),
            "sql":case["sql"],"params":case["params"],"connector":"local","fingerprint":"hostile"});
        child
            .stdin
            .take()
            .unwrap()
            .write_all(serde_json::to_string(&request).unwrap().as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{case}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let reply: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            reply,
            serde_json::json!({"status":"failed","error":"metadata"}),
            "{case}"
        );
    }
}

#[test]
fn the_real_worker_does_not_link_the_json_extension() {
    let (_dir, connector) = worker_fixture();
    assert_eq!(
        worker_query(&connector, "SELECT 1 AS n", serde_json::json!({}))
            .unwrap()
            .rows,
        [serde_json::json!({"n": 1})]
    );
    let error = worker_query(
        &connector,
        "SELECT json_serialize_sql('SELECT 1 AS n') AS plan",
        serde_json::json!({}),
    )
    .unwrap_err();
    assert!(
        matches!(
            error,
            connectors::ConnectorError::Sql {
                error: connectors::spec::SqlError::Query,
                ..
            }
        ),
        "JSON serialization must remain unavailable in the bundled-only worker: {error}"
    );
}

#[test]
fn sql_policy_is_checked_before_a_previously_captured_result_is_reused() {
    let (dir, connector) = worker_fixture();
    let mut registry = connectors::Registry::new(dir.path().join("cache"));
    registry.register("local", connector);
    let params = Default::default();
    // Populate a real stored result, using an identity from before the policy update.
    registry
        .execute_adhoc(
            "local",
            connectors::QueryBody::Sql("SELECT 'captured-host-path' AS path"),
            &params,
            None,
            "legacy-query-identity",
        )
        .unwrap();
    let error = registry
        .execute_adhoc(
            "local",
            connectors::QueryBody::Sql("SELECT * FROM duckdb_settings()"),
            &params,
            None,
            "legacy-query-identity",
        )
        .unwrap_err();
    assert!(
        matches!(error, connectors::ConnectorError::QueryRejected { .. }),
        "{error}"
    );
    assert!(error.to_string().contains("host metadata"));
}

#[test]
fn stored_metadata_definitions_cannot_hide_host_access() {
    for (definition, sql) in [
        (
            "CREATE VIEW saved AS SELECT value FROM duckdb_settings()",
            "SELECT * FROM saved",
        ),
        (
            "CREATE MACRO saved() AS current_setting('temp_directory')",
            "SELECT saved() AS value",
        ),
        (
            "CREATE TABLE saved(n INTEGER, path VARCHAR GENERATED ALWAYS AS (current_setting('temp_directory'))); INSERT INTO saved(n) VALUES (1)",
            "SELECT * FROM saved",
        ),
    ] {
        let (dir, connector) = worker_fixture();
        Connection::open(dir.path().join("source.duckdb"))
            .unwrap()
            .execute_batch(definition)
            .unwrap();
        let error = worker_query(&connector, sql, serde_json::json!({})).unwrap_err();
        assert!(
            matches!(error, connectors::ConnectorError::QueryRejected { .. }),
            "{error}"
        );
        assert!(error.to_string().contains("host metadata"), "{error}");
        assert!(
            !error
                .to_string()
                .contains(&dir.path().display().to_string())
        );
    }
}

#[test]
fn worker_sql_failures_preserve_safe_categories_and_author_positions() {
    use connectors::spec::SqlError;
    let (dir, connector) = worker_fixture();
    for (sql, expected) in [
        (
            "SELECT 1 + FROM source",
            "SQL syntax error at line 1, column 12",
        ),
        (
            "SELECT\n'雪' + FROM source",
            "SQL syntax error at line 2, column 7",
        ),
        (
            "SELECT * FROM missing_secret_table",
            "SQL table or view was not found",
        ),
        (
            "SELECT missing_secret_column FROM source",
            "SQL column was not found",
        ),
        (
            "SELECT CAST('private-secret-value' AS INTEGER)",
            "SQL value could not be converted",
        ),
        (
            "SELECT * FROM read_text('/private-secret-path')",
            "SQL external access was refused",
        ),
    ] {
        let message = worker_query(&connector, sql, serde_json::json!({}))
            .unwrap_err()
            .to_string();
        assert!(message.contains(expected), "{sql}: {message}");
        assert!(!message.contains("secret"), "{message}");
    }
    let path = dir.path().join("source.duckdb");
    let mut bytes = fs::read(&path).unwrap();
    let checksum = |bytes: &[u8]| {
        bytes[8..4096]
            .as_chunks::<8>()
            .0
            .iter()
            .fold(5381_u64, |sum, word| {
                sum ^ u64::from_le_bytes(*word).wrapping_mul(0xbf58476d1ce4e5b9)
            })
    };
    // A genuine database header with a future storage version and valid DuckDB checksum.
    assert_eq!(&bytes[8..12], b"DUCK");
    assert_eq!(
        u64::from_le_bytes(bytes[..8].try_into().unwrap()),
        checksum(&bytes)
    );
    bytes[12..20].copy_from_slice(&u64::MAX.to_le_bytes());
    let sum = checksum(&bytes);
    bytes[..8].copy_from_slice(&sum.to_le_bytes());
    fs::write(&path, &bytes).unwrap();
    let error = worker_query(&connector, "SELECT 1 AS n", serde_json::json!({})).unwrap_err();
    assert!(
        matches!(
            error,
            connectors::ConnectorError::Sql {
                error: SqlError::DatabaseVersion,
                ..
            }
        ),
        "{error}"
    );
    assert!(error.to_string().contains("written by a newer DuckDB"));
    assert!(!error.to_string().contains(&path.display().to_string()));
    bytes[0] ^= 1;
    fs::write(&path, bytes).unwrap();
    let error = worker_query(&connector, "SELECT 1 AS n", serde_json::json!({})).unwrap_err();
    assert!(
        matches!(
            error,
            connectors::ConnectorError::Sql {
                error: SqlError::Database,
                ..
            }
        ),
        "{error}"
    );
}

#[test]
fn the_real_worker_binds_values_and_returns_parent_owned_provenance() {
    let (_dir, connector) = worker_fixture();
    let attack = "'); DROP TABLE source; --";
    let snapshot = worker_query(
        &connector,
        "SELECT n, $label AS label, $count AS count, $flag AS flag FROM source",
        serde_json::json!({"label":attack,"count":42,"flag":true}),
    )
    .unwrap();
    assert_eq!(
        snapshot.rows,
        [serde_json::json!({"n":7,"label":attack,"count":42,"flag":true})]
    );
    assert_eq!(snapshot.provenance.connector, "local");
    assert_eq!(snapshot.provenance.fingerprint, "worker-fingerprint");
    assert_eq!(snapshot.provenance.row_count, 1);
    let error = worker_query(
        &connector,
        "SELECT CAST('private-value-sentinel' AS INTEGER)",
        serde_json::json!({}),
    )
    .unwrap_err()
    .to_string();
    assert!(!error.contains("private-value-sentinel"));
}

#[test]
fn the_real_worker_limits_rows_output_bytes_and_memory() {
    let (_dir, connector) = worker_fixture();
    let snapshot = worker_query(
        &connector,
        "SELECT i FROM range(100000) t(i)",
        serde_json::json!({}),
    )
    .unwrap();
    assert_eq!(snapshot.rows.len(), 100_000);
    let error = worker_query(
        &connector,
        "SELECT i FROM range(100001) t(i)",
        serde_json::json!({}),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("100000"), "{error}");
    let error = worker_query(
        &connector,
        "SELECT repeat('x',40*1024*1024) AS too_large",
        serde_json::json!({}),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("limit"), "{error}");
    let error = worker_query(
        &connector,
        "SELECT repeat('x',512*1024*1024) AS too_large",
        serde_json::json!({}),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("memory"), "{error}");
    assert_eq!(
        worker_query(&connector, "SELECT n FROM source", serde_json::json!({}))
            .unwrap()
            .rows[0]["n"],
        7
    );
}

#[test]
fn the_parent_kills_a_worker_at_the_wall_clock_deadline() {
    let (_dir, connector) = worker_fixture();
    let started = std::time::Instant::now();
    let error = worker_query(
        &connector,
        "SELECT sum(sin(i::DOUBLE)) AS total FROM range(1000000000000) t(i)",
        serde_json::json!({}),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("timeout"), "{error}");
    assert!(started.elapsed() < std::time::Duration::from_secs(35));
}

#[test]
fn a_missing_worker_never_falls_back_to_execution_in_the_host() {
    let (dir, _) = worker_fixture();
    let connector = connectors::duckdb_adapter::DuckDbConnector::with_worker(
        dir.path().join("source.duckdb"),
        dir.path().join("missing-worker"),
    );
    let error = worker_query(&connector, "SELECT n FROM source", serde_json::json!({}))
        .unwrap_err()
        .to_string();
    assert!(error.contains("isolation is unavailable"), "{error}");
    assert!(!error.contains(&dir.path().display().to_string()));
}

#[test]
fn resolver_preserves_the_worker_failure_instead_of_claiming_no_registry_exists() {
    let copy = example_copy();
    let root = copy.path();
    seed_warehouse(root);
    clear_cache(root);
    let trusted = root.join("host-connectors.toml");
    fs::write(
        &trusted,
        format!(
            "sql_worker = {}\n",
            serde_json::to_string(&root.join("missing-worker")).unwrap()
        ),
    )
    .unwrap();
    let registry = connectors::load_registry(root, Some(&trusted))
        .unwrap()
        .unwrap();
    let report = root.join("report.rmd");
    let source = fs::read_to_string(&report).unwrap();
    let error = resolver::resolve_document_options_full(
        &source,
        &report,
        &[],
        false,
        false,
        Some(&registry),
    )
    .expect_err("missing worker rejects SQL");
    let diagnostics: Vec<Value> = serde_json::from_str(&error).unwrap();
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic["code"] == "data.connector.rejected"
                && diagnostic["message"]
                    .as_str()
                    .is_some_and(|text| text.contains("SQL worker isolation is unavailable"))),
        "{error}"
    );
    assert!(!error.contains("has no report.connectors.yaml"), "{error}");
    assert!(!error.contains(&root.display().to_string()), "{error}");
}

#[test]
fn bound_values_drive_rows_cache_identity_and_provenance_at_the_block() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("source.duckdb");
    duckdb::Connection::open(&db).unwrap().close().unwrap();
    let mut registry = connectors::Registry::new(dir.path().join("cache"));
    registry.register(
        "local",
        connectors::duckdb_adapter::DuckDbConnector::with_worker(
            db,
            PathBuf::from(env!("CARGO_BIN_EXE_richview-sql-worker")),
        ),
    );
    let resolve = |value: &str| {
        let source = format!(
            "---\ntitle: Params\n---\n\n```table\ndata:\n  sql:\n    connector: local\n    inline: SELECT $name AS label\n    params: {{name: '{value}'}}\n```\n"
        );
        let mut doc = richview_core::document::parse(&source).unwrap();
        let mut diagnostics = Vec::new();
        connectors::resolve::resolve_connector_data(
            &mut doc,
            dir.path(),
            &registry,
            &mut richview_core::document::RenderOptions::default(),
            &mut diagnostics,
        )
        .unwrap();
        assert!(
            !diagnostics.iter().any(|d| d.level == "error"),
            "{diagnostics:?}"
        );
        assert_eq!(doc.blocks[0].value["data"]["inline"][0]["label"], value);
        assert!(diagnostics.iter().any(|d| d.code == "data.provenance"
            && d.message.contains("parameters name")
            && !d.message.contains(value)));
        doc.blocks[0].value["data"]["_provenance"]["fingerprint"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let first = resolve("alpha");
    assert_eq!(resolve("alpha"), first);
    assert_ne!(resolve("beta"), first);
    let mut doc = richview_core::document::parse(
        "```table\ndata:\n  sql:\n    connector: local\n    inline: SELECT $missing AS n\n```\n",
    )
    .unwrap();
    let line = doc.blocks[0].line;
    let mut diagnostics = Vec::new();
    connectors::resolve::resolve_connector_data(
        &mut doc,
        dir.path(),
        &registry,
        &mut richview_core::document::RenderOptions::default(),
        &mut diagnostics,
    )
    .unwrap();
    let error = diagnostics.iter().find(|d| d.level == "error").unwrap();
    assert_eq!(
        (&*error.code, &*error.path, error.line, error.tag.as_deref()),
        (
            "data.connector.rejected",
            "table.data",
            Some(line),
            Some("table")
        )
    );
}

fn resolve_rows(registry: &connectors::Registry, root: &std::path::Path, data: &str) -> Value {
    let (data, diagnostics) = resolve_snapshot(registry, root, data);
    assert!(
        !diagnostics.iter().any(|d| d.level == "error"),
        "{diagnostics:?}"
    );
    data
}

fn resolve_snapshot(
    registry: &connectors::Registry,
    root: &std::path::Path,
    data: &str,
) -> (Value, Vec<richview_core::document::Diagnostic>) {
    let source = format!("```table\ndata:\n{data}\n```\n");
    let mut document = richview_core::document::parse(&source).unwrap();
    let mut diagnostics = Vec::new();
    connectors::resolve::resolve_connector_data(
        &mut document,
        root,
        registry,
        &mut richview_core::document::RenderOptions::default(),
        &mut diagnostics,
    )
    .unwrap();
    (document.blocks[0].value["data"].clone(), diagnostics)
}

#[test]
fn named_snapshots_remain_readable_when_the_source_is_offline() {
    let (dir, connector) = worker_fixture();
    let mut registry = connectors::Registry::new(dir.path().join("cache"));
    registry.register("local", connector);
    let data = resolve_rows(
        &registry,
        dir.path(),
        "  sql:\n    connector: local\n    inline: SELECT n FROM source\n    cache: {key: monthly}",
    );
    assert_eq!(data["inline"], serde_json::json!([{"n":7}]));
    fs::remove_file(dir.path().join("source.duckdb")).unwrap();
    let cached = resolve_rows(&registry, dir.path(), "  cache: {key: monthly}");
    assert_eq!(cached["inline"], data["inline"]);
    assert_eq!(
        cached["_provenance"]["fingerprint"],
        data["_provenance"]["fingerprint"]
    );
    assert_eq!(
        cached["_provenance"]["fetched_at"],
        data["_provenance"]["fetched_at"]
    );
}

#[test]
fn snapshots_do_not_leak_between_same_named_connectors_bound_to_different_sources() {
    let (first_dir, first) = worker_fixture();
    let (second_dir, second) = worker_fixture();
    Connection::open(second_dir.path().join("source.duckdb"))
        .unwrap()
        .execute_batch("UPDATE source SET n=99")
        .unwrap();
    let cache = first_dir.path().join("cache");
    let mut first_registry = connectors::Registry::new(cache.clone());
    first_registry.register("local", first);
    let mut second_registry = connectors::Registry::new(cache);
    second_registry.register("local", second);
    let data =
        "  sql:\n    connector: local\n    inline: SELECT n FROM source\n    cache: {key: monthly}";
    let first = resolve_rows(&first_registry, first_dir.path(), data);
    let cached = second_registry.execute_adhoc(
        "",
        connectors::QueryBody::Cache,
        &Default::default(),
        None,
        "monthly",
    );
    assert!(matches!(
        cached,
        Err(connectors::ConnectorError::QueryRejected { .. })
    ));
    let second = resolve_rows(&second_registry, second_dir.path(), data);
    assert_eq!(first["inline"], serde_json::json!([{"n":7}]));
    assert_eq!(second["inline"], serde_json::json!([{"n":99}]));
    assert_ne!(
        first["_provenance"]["fingerprint"],
        second["_provenance"]["fingerprint"]
    );
    let cached = first_registry.execute_adhoc(
        "",
        connectors::QueryBody::Cache,
        &Default::default(),
        None,
        "monthly",
    );
    assert!(matches!(
        cached,
        Err(connectors::ConnectorError::QueryRejected { .. })
    ));
}

#[test]
fn snapshots_apply_freshness_warn_on_stale_reads_and_refresh_only_when_queried() {
    let (dir, connector) = worker_fixture();
    let mut registry = connectors::Registry::new(dir.path().join("cache"));
    registry.register("local", connector);
    let initial = resolve_rows(
        &registry,
        dir.path(),
        "  sql:\n    connector: local\n    inline: SELECT n FROM source\n    cache: {key: monthly, ttl: 60s}",
    );
    Connection::open(dir.path().join("source.duckdb"))
        .unwrap()
        .execute_batch("UPDATE source SET n=99")
        .unwrap();
    let (stale, diagnostics) =
        resolve_snapshot(&registry, dir.path(), "  cache: {key: monthly, ttl: 0s}");
    assert_eq!(stale["inline"], initial["inline"]);
    assert_eq!(stale["_provenance"]["stale"], true);
    assert!(diagnostics.iter().any(|d| d.level == "warning"
        && d.code == "data.cache.stale"
        && d.message.contains("Stale")));
    let refreshed = resolve_rows(
        &registry,
        dir.path(),
        "  sql:\n    connector: local\n    inline: SELECT n FROM source\n    cache: {key: monthly, ttl: 60s, refresh: true}",
    );
    assert_eq!(refreshed["inline"], serde_json::json!([{"n":99}]));
    assert_eq!(refreshed["_provenance"]["stale"], false);
    let fingerprint = refreshed["_provenance"]["fingerprint"].as_str().unwrap();
    assert_eq!(
        fingerprint,
        initial["_provenance"]["fingerprint"].as_str().unwrap()
    );
    let direct = resolve_rows(
        &registry,
        dir.path(),
        &format!("  cache: {{key: '{fingerprint}'}}"),
    );
    assert_eq!(direct["inline"], refreshed["inline"]);
    for policy in [
        "{freshness: '2024-02-31T00:00:00Z'}",
        "{ttl: '18446744073709551615d'}",
    ] {
        let (invalid, diagnostics) = resolve_snapshot(
            &registry,
            dir.path(),
            &format!(
                "  sql:\n    connector: local\n    inline: SELECT n FROM source\n    cache: {policy}"
            ),
        );
        assert!(invalid.get("inline").is_none());
        assert!(
            diagnostics
                .iter()
                .any(|d| d.level == "error" && d.code == "data.connector.rejected")
        );
    }
}

#[test]
fn configured_snapshots_are_report_scoped_and_reload_without_the_live_source() {
    let (first_dir, _) = worker_fixture();
    let (second_dir, _) = worker_fixture();
    let host = tempfile::tempdir().unwrap();
    let trusted = host.path().join("connectors.toml");
    fs::write(
        &trusted,
        format!(
            "cache_root = \"snapshots\"\nsql_worker = {}\n",
            serde_json::to_string(env!("CARGO_BIN_EXE_richview-sql-worker")).unwrap()
        ),
    )
    .unwrap();
    for root in [first_dir.path(), second_dir.path()] {
        fs::write(
            root.join("report.connectors.yaml"),
            "connectors:\n  local:\n    kind: duckdb\n    database: source.duckdb\n",
        )
        .unwrap();
    }
    let first = connectors::load_registry(first_dir.path(), Some(&trusted))
        .unwrap()
        .unwrap();
    let second = connectors::load_registry(second_dir.path(), Some(&trusted))
        .unwrap()
        .unwrap();
    assert_ne!(first.store().root(), second.store().root());
    assert!(
        first
            .store()
            .root()
            .starts_with(host.path().join("snapshots"))
    );
    let saved = resolve_rows(
        &first,
        first_dir.path(),
        "  sql:\n    connector: local\n    inline: SELECT n FROM source\n    cache: {key: monthly}",
    );
    let missing = second.execute_adhoc(
        "",
        connectors::QueryBody::Cache,
        &Default::default(),
        None,
        "monthly",
    );
    assert!(matches!(
        missing,
        Err(connectors::ConnectorError::CacheMiss { .. })
    ));
    fs::remove_file(first_dir.path().join("source.duckdb")).unwrap();
    let offline = connectors::load_registry(first_dir.path(), Some(&trusted))
        .unwrap()
        .unwrap();
    let cached = resolve_rows(&offline, first_dir.path(), "  cache: {key: monthly}");
    assert_eq!(cached["inline"], saved["inline"]);
    fs::remove_file(first_dir.path().join("report.connectors.yaml")).unwrap();
    let snapshots_only = connectors::load_registry(first_dir.path(), Some(&trusted))
        .unwrap()
        .unwrap();
    let cached = resolve_rows(&snapshots_only, first_dir.path(), "  cache: {key: monthly}");
    assert_eq!(cached["inline"], saved["inline"]);
    assert!(!first_dir.path().join(".richview-cache").exists());
}

#[test]
fn connector_provenance_is_openable_escaped_and_keeps_private_parameters_out_of_the_report() {
    let (directory, connector) = worker_fixture();
    let name = "warehouse <img src=x onerror=alert(1)>";
    let mut registry = connectors::Registry::new(directory.path().join("cache"));
    registry.register(name, connector);
    let data = format!(
        "data:\n  sql:\n    connector: {}\n    inline: SELECT 'Only' AS label, n AS value FROM source WHERE $needle IS NOT NULL\n    params: {{needle: 'private-parameter-sentinel'}}\n    cache: {{key: provenance, ttl: 60s}}\n",
        serde_json::to_string(name).unwrap()
    );
    let source = format!(
        "---\ntitle: Snapshot provenance\n---\n\n```chart\ntype: bar\ntitle: Observations\n{data}encoding: {{x: label, y: value}}\n```\n\n```table\ndata:\n  cache: {{key: provenance, ttl: 0s}}\n```\n"
    );
    let path = directory.path().join("report.rmd");
    fs::write(&path, &source).unwrap();
    let (document, options) =
        resolver::resolve_document_options_full(&source, &path, &[], false, false, Some(&registry))
            .unwrap();
    assert!(
        !document
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("private-parameter-sentinel"))
    );
    assert!(
        document.diagnostics.iter().any(
            |diagnostic| diagnostic.code == "data.cache.stale" && diagnostic.level == "warning"
        )
    );
    let captured = document.blocks[0].value["data"]["_provenance"]["fetched_at"]
        .as_str()
        .unwrap()
        .to_owned();
    let report = richview_core::document::render_document(document, &options).unwrap();
    let notes = report
        .html
        .split("<details")
        .skip(1)
        .filter_map(|tail| tail.split_once("</details>").map(|(note, _)| note))
        .filter(|note| note.contains("warehouse &lt;img") && note.contains(&captured))
        .collect::<Vec<_>>();
    assert_eq!(
        notes.len(),
        2,
        "both chart and table need an openable provenance note"
    );
    assert!(notes.iter().all(|note| note.contains("<summary")));
    assert!(
        notes
            .iter()
            .any(|note| note.to_ascii_lowercase().contains("stale"))
    );
    assert!(!report.html.contains(name));
    assert!(!report.html.contains("private-parameter-sentinel"));
    assert!(
        !report
            .html
            .contains(&directory.path().to_string_lossy().to_string())
    );
}

#[test]
fn the_real_worker_preserves_dates_timestamp_precision_and_declared_timezones() {
    let (_directory, connector) = worker_fixture();
    let snapshot = worker_query(&connector,
        "SELECT DATE '2024-02-29' AS day, TIMESTAMP_NS '2024-02-29 12:34:56.123456789' AS captured, TIMESTAMPTZ '2024-02-29 13:34:56.123456+01:00' AS absolute, CAST(NULL AS DATE) AS missing",
        serde_json::json!({}),
    ).unwrap();
    assert_eq!(
        snapshot.rows,
        [
            serde_json::json!({"day":"2024-02-29","captured":"2024-02-29T12:34:56.123456789","absolute":"2024-02-29T12:34:56.123456Z","missing":null})
        ]
    );
    use connectors::FieldKind;
    assert_eq!(
        snapshot
            .schema
            .iter()
            .map(|field| field.kind)
            .collect::<Vec<_>>(),
        [
            FieldKind::Date,
            FieldKind::Timestamp,
            FieldKind::Timestamp,
            FieldKind::Date
        ]
    );
    assert!(!snapshot.schema[0].nullable);
    assert!(snapshot.schema[3].nullable);
    let before_epoch = worker_query(
        &connector,
        "SELECT DATE '1969-12-31' AS day, TIMESTAMP '1969-12-31 23:59:59.123456' AS captured",
        serde_json::json!({}),
    )
    .unwrap();
    assert_eq!(
        before_epoch.rows,
        [serde_json::json!({"day":"1969-12-31","captured":"1969-12-31T23:59:59.123456"})]
    );
    assert!(
        worker_query(
            &connector,
            "SELECT DATE 'infinity' AS day",
            serde_json::json!({})
        )
        .is_err()
    );
}

#[test]
fn the_real_worker_preserves_wide_scalars_and_refuses_lossy_or_structural_fallbacks() {
    let (_directory, connector) = worker_fixture();
    let snapshot = worker_query(&connector, "SELECT CAST('340282366920938463463374607431768211455' AS UHUGEINT) AS unsigned, CAST('12345678901234567890.1234567890' AS DECIMAL(38,10)) AS amount", serde_json::json!({})).unwrap();
    assert_eq!(
        snapshot.rows,
        [
            serde_json::json!({"unsigned":"340282366920938463463374607431768211455","amount":"12345678901234567890.1234567890"})
        ]
    );
    assert!(
        snapshot
            .schema
            .iter()
            .all(|field| field.kind == connectors::FieldKind::String)
    );
    for sql in [
        "SELECT CAST('NaN' AS DOUBLE) AS n",
        "SELECT CAST('Infinity' AS DOUBLE) AS n",
        "SELECT [1,2,3] AS items",
    ] {
        let error = worker_query(&connector, sql, serde_json::json!({}))
            .unwrap_err()
            .to_string();
        assert!(error.contains("cast it to text"), "{error}");
        assert!(!error.contains("NaN") && !error.contains("Infinity"));
    }
}
