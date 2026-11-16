//! dbt MetricFlow connector that bridges to a locally installed `mf` CLI.
//!
//! The adapter runs `mf query --metrics ... --csv <path>` inside the trusted
//! dbt project directory and parses the emitted CSV into typed rows. Metric
//! definitions, joins and dialect stay in the semantic layer; this connector
//! refuses to be used as a thin SQL passthrough.
//!
//! Report source cannot pick the project path, interpreter or environment.
//! The trusted `report.connectors.yaml` names a runtime handle; the host's
//! `connectors.toml` binds the handle to a `MetricFlowRuntime` with the
//! project directory and the command used to launch `mf`.

use super::registry::Connector;
use super::spec::{
    ConnectorError, DatasetSnapshot, FieldKind, MetricFlowSpec, Provenance, ProvenanceSource,
    Query, QueryBody, SchemaField,
};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::{io::Read, path::PathBuf, process::Command};

pub struct MetricFlowConnector {
    runtime: MetricFlowRuntime,
}

impl MetricFlowConnector {
    pub fn new(mut runtime: MetricFlowRuntime) -> Self {
        runtime.project_dir = runtime
            .project_dir
            .canonicalize()
            .unwrap_or(runtime.project_dir);
        Self { runtime }
    }
}

#[derive(Debug, Clone)]
pub struct MetricFlowRuntime {
    /// Command invoked to launch `mf`. Defaults to `["mf"]`; `["uv","run","--","mf"]` is common.
    pub launcher: Vec<String>,
    /// Working directory for the semantic project; trusted-configured.
    pub project_dir: PathBuf,
    /// Extra environment variables (DBT_PROFILES_DIR, warehouse paths, target name)
    /// merged into the subprocess environment. Trusted-configured; never read from
    /// the report source.
    pub env: std::collections::BTreeMap<String, String>,
}

impl Default for MetricFlowRuntime {
    fn default() -> Self {
        Self {
            launcher: vec!["mf".into()],
            project_dir: PathBuf::from("."),
            env: std::collections::BTreeMap::new(),
        }
    }
}

impl Connector for MetricFlowConnector {
    fn kind(&self) -> &'static str {
        "metricflow"
    }

    fn source_identity(&self) -> String {
        let mut hash = Sha256::new();
        hash.update(b"metricflow-runtime-v1\0");
        hash.update(self.runtime.project_dir.as_os_str().as_encoded_bytes());
        hash.update(b"\0");
        hash.update(
            serde_json::to_vec(&(&self.runtime.launcher, &self.runtime.env))
                .expect("trusted runtime serializes"),
        );
        format!("{:x}", hash.finalize())
    }

    fn execute(&self, request: &Query<'_>) -> Result<DatasetSnapshot, ConnectorError> {
        let spec = match request.body {
            QueryBody::MetricFlow(spec) => spec,
            _ => {
                return Err(ConnectorError::QueryRejected {
                    reason: "metricflow connector accepts only data.metricflow blocks".into(),
                });
            }
        };
        validate_spec(spec)?;

        let scratch = tempfile::TempDir::new().map_err(ConnectorError::Io)?;
        let csv_path = scratch.path().join("result.csv");

        let mut command =
            {
                let (first, rest) = self.runtime.launcher.split_first().ok_or_else(|| {
                    ConnectorError::Execution {
                        connector: request.connector.to_owned(),
                        message: "metricflow launcher is empty".into(),
                    }
                })?;
                let mut command = Command::new(first);
                command.args(rest);
                command
            };
        for (key, value) in &self.runtime.env {
            command.env(key, value);
        }
        command
            .current_dir(&self.runtime.project_dir)
            .arg("query")
            .args(["--metrics", &spec.metrics.join(",")])
            .args(["--csv", csv_path.to_str().unwrap()])
            .arg("--quiet");
        if !spec.group_by.is_empty() {
            command.args(["--group-by", &spec.group_by.join(",")]);
        }
        if !spec.order_by.is_empty() {
            command.args(["--order", &spec.order_by.join(",")]);
        }
        for clause in &spec.where_clause {
            command.args(["--where", clause]);
        }
        let ceiling = super::spec::MAX_ROWS as u32 + 1;
        command.args([
            "--limit",
            &spec.limit.unwrap_or(ceiling).min(ceiling).to_string(),
        ]);
        let output = command.output().map_err(ConnectorError::Io)?;
        if !output.status.success() {
            return Err(ConnectorError::Execution {
                connector: request.connector.to_owned(),
                message:
                    "MetricFlow query failed; check trusted runtime configuration and named metrics"
                        .into(),
            });
        }
        let mut csv_bytes = Vec::new();
        std::fs::File::open(&csv_path)?
            .take(crate::MAX_INPUT_BYTES as u64 + 1)
            .read_to_end(&mut csv_bytes)?;
        if csv_bytes.len() > crate::MAX_INPUT_BYTES {
            return Err(ConnectorError::QueryRejected {
                reason: "MetricFlow result exceeds its byte limit".into(),
            });
        }
        let (rows, schema) = parse_csv(&csv_bytes, request.connector)?;
        let row_count = rows.len();
        Ok(DatasetSnapshot {
            rows,
            schema,
            provenance: Provenance {
                connector: request.connector.to_owned(),
                source: ProvenanceSource::Metricflow,
                fetched_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                fingerprint: request.fingerprint.to_owned(),
                row_count,
            },
            cache: None,
        })
    }
}

fn parse_csv(
    bytes: &[u8],
    connector: &str,
) -> Result<(Vec<Value>, Vec<SchemaField>), ConnectorError> {
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(bytes);
    let headers = reader
        .headers()
        .map_err(|err| ConnectorError::Execution {
            connector: connector.to_owned(),
            message: format!("mf CSV header: {err}"),
        })?
        .clone();
    let column_names: Vec<String> = headers.iter().map(|c| c.to_owned()).collect();
    let mut schema_kinds: Vec<Option<FieldKind>> = vec![None; column_names.len()];
    let mut nullable = vec![false; column_names.len()];
    let mut rows = Vec::new();
    for record in reader.records() {
        if rows.len() >= super::spec::MAX_ROWS {
            return Err(ConnectorError::QueryRejected {
                reason: format!(
                    "result exceeds {} rows; narrow the query",
                    super::spec::MAX_ROWS
                ),
            });
        }
        let record = record.map_err(|err| ConnectorError::Execution {
            connector: connector.to_owned(),
            message: format!("mf CSV row: {err}"),
        })?;
        let mut object = Map::with_capacity(column_names.len());
        for (index, name) in column_names.iter().enumerate() {
            let cell = record.get(index).unwrap_or("");
            let (value, kind) = coerce_cell(cell);
            match kind {
                FieldKind::Null => nullable[index] = true,
                kind => match &mut schema_kinds[index] {
                    slot @ None => *slot = Some(kind),
                    Some(current) if *current != kind => *current = FieldKind::String,
                    _ => {}
                },
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
    Ok((rows, schema))
}

fn coerce_cell(cell: &str) -> (Value, FieldKind) {
    if cell.is_empty() {
        return (Value::Null, FieldKind::Null);
    }
    if let Ok(n) = cell.parse::<i64>() {
        return (Value::from(n), FieldKind::Integer);
    }
    if let Ok(f) = cell.parse::<f64>()
        && let Some(number) = serde_json::Number::from_f64(f)
    {
        return (Value::Number(number), FieldKind::Number);
    }
    (Value::from(cell.to_owned()), FieldKind::String)
}

/// Enforce the semantic-API shape: at least one metric, no raw SQL smuggled
/// through filters.
pub(crate) fn validate_spec(spec: &MetricFlowSpec) -> Result<(), ConnectorError> {
    if spec.metrics.is_empty() {
        return Err(ConnectorError::QueryRejected {
            reason: "data.metricflow.metrics must list at least one metric".into(),
        });
    }
    for clause in &spec.where_clause {
        let lowered = clause.to_ascii_lowercase();
        if lowered.contains("select ") || lowered.contains(';') {
            return Err(ConnectorError::QueryRejected {
                reason: "metricflow filters are semantic expressions, not SQL fragments".into(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_requires_at_least_one_metric() {
        let empty = MetricFlowSpec {
            metrics: vec![],
            group_by: vec![],
            where_clause: vec![],
            order_by: vec![],
            limit: None,
        };
        assert!(validate_spec(&empty).is_err());
    }

    #[test]
    fn validate_refuses_sql_injection_in_filters() {
        let spec = MetricFlowSpec {
            metrics: vec!["orders".into()],
            group_by: vec![],
            where_clause: vec!["1=1; select * from users".into()],
            order_by: vec![],
            limit: None,
        };
        assert!(validate_spec(&spec).is_err());
    }

    #[test]
    fn csv_parses_typed_rows_from_mf_output() {
        let csv = b"month,orders\n2026-01,120\n2026-02,150\n";
        let (rows, schema) = parse_csv(csv, "okurka").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(schema[0].kind, FieldKind::String);
        assert_eq!(schema[1].kind, FieldKind::Integer);
    }

    #[test]
    fn csv_refuses_more_than_100000_rows_instead_of_truncating() {
        let mut csv = format!("n\n{}", "1\n".repeat(super::super::spec::MAX_ROWS));
        assert_eq!(
            parse_csv(csv.as_bytes(), "local").unwrap().0.len(),
            super::super::spec::MAX_ROWS
        );
        csv.push_str("1\n");
        assert!(matches!(
            parse_csv(csv.as_bytes(), "local"),
            Err(ConnectorError::QueryRejected { .. })
        ));
    }
}
