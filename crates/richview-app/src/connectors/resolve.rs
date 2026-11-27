//! Bridge chart-local `data.sql` / `data.metricflow` / `data.cache` mappings
//! to the inline-rows shape the renderer already consumes.
//!
//! The host inserts captured rows and safe provenance into each resolved block.
//! The pure renderer then reads inline snapshots without invoking a connector.

use super::CachePolicy;
use super::registry::Registry;
use super::spec::{ConnectorError, DatasetSnapshot, MetricFlowSpec, QueryBody};
use richview_core::document::{Diagnostic, Document, RenderOptions};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};

/// Resolve every connector-backed data block in `doc` and insert the result
/// rows and capture metadata into the block's inline data. The options argument remains
/// for host-interface compatibility; version freezing is a separate host responsibility.
pub fn resolve_connector_data(
    doc: &mut Document,
    root: &Path,
    registry: &Registry,
    _options: &mut RenderOptions,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), String> {
    resolve_with_limits(doc, root, registry, &mut Default::default(), diagnostics)
}

pub(crate) fn resolve_with_limits(
    doc: &mut Document,
    root: &Path,
    registry: &Registry,
    inputs: &mut crate::input_limits::InputSizes,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), String> {
    for block in doc.blocks.iter_mut() {
        let Some(data) = block.value.get("data") else {
            continue;
        };
        let kind = if data.get("sql").is_some() {
            "sql"
        } else if data.get("metricflow").is_some() {
            "metricflow"
        } else if data.get("cache").is_some() {
            "cache"
        } else {
            continue;
        };
        let connector = data
            .pointer(&format!("/{kind}/connector"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let cache_policy = match parse_cache_policy(data) {
            Ok(policy) => policy,
            Err(reason) => {
                let mut diagnostic = Diagnostic::new(
                    "error",
                    "data.connector.rejected",
                    reason,
                    format!("{}.data.{kind}", block.tag),
                );
                diagnostic.line = Some(block.line);
                diagnostic.tag = Some(block.tag.clone());
                diagnostics.push(diagnostic);
                continue;
            }
        };
        let params = match parse_params(data) {
            Ok(params) => params,
            Err(err) => {
                let mut d = Diagnostic::new(
                    "error",
                    "data.connector.rejected",
                    err.to_string(),
                    format!("{}.data.sql.params", block.tag),
                );
                d.line = Some(block.line);
                d.tag = Some(block.tag.clone());
                diagnostics.push(d);
                continue;
            }
        };

        let sql_text: String;
        let metricflow_spec: MetricFlowSpec;
        let (query_body, fingerprint) = match kind {
            "sql" => {
                sql_text = load_sql_text(data, root, inputs)?;
                let fp = fingerprint_sql(connector, &sql_text, &params);
                (QueryBody::Sql(sql_text.as_str()), fp)
            }
            "metricflow" => {
                metricflow_spec = parse_metricflow_spec(data)?;
                let fp = fingerprint_metricflow(connector, &metricflow_spec);
                (QueryBody::MetricFlow(&metricflow_spec), fp)
            }
            "cache" => {
                let key = data
                    .pointer("/cache/key")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "data.cache.key must be a string".to_owned())?;
                (QueryBody::Cache, key.to_owned())
            }
            _ => continue,
        };
        match registry.execute_adhoc(
            connector,
            query_body,
            &params,
            cache_policy.as_ref(),
            &fingerprint,
        ) {
            Ok(snapshot) => {
                if kind != "sql" || !inputs.local_connector(connector) {
                    let rows = serde_json::to_vec(&snapshot.rows).map_err(|e| e.to_string())?;
                    let id = format!(
                        "{}:{}",
                        snapshot.provenance.fingerprint,
                        hex_digest(Sha256::new_with_prefix(&rows))
                    );
                    if let Err(mut finding) = inputs.result(
                        id,
                        &format!("{} result", snapshot.provenance.connector),
                        rows.len(),
                    ) {
                        finding.file = None;
                        finding.path = format!("{}.data.{kind}", block.tag);
                        finding.tag = Some(block.tag.clone());
                        finding.line = Some(block.line);
                        return Err(crate::input_limits::encoded(finding));
                    }
                    inputs.check().map_err(crate::input_limits::encoded)?;
                }
                let message = provenance_message(&snapshot, &params);
                let stale = snapshot.cache.as_ref().is_some_and(|cache| cache.stale);
                let tag = block.tag.clone();
                let line = block.line;
                if tag == "table" {
                    for field in &snapshot.schema {
                        if let Some(places) = field.scale {
                            block.value["_decimalScales"][&field.name] = Value::from(places);
                        }
                    }
                }
                if let Some(data_mut) = block.value.get_mut("data").and_then(Value::as_object_mut) {
                    data_mut.insert("inline".into(), Value::Array(snapshot.rows));
                    let mut provenance =
                        serde_json::to_value(&snapshot.provenance).unwrap_or(Value::Null);
                    provenance["stale"] = Value::Bool(stale);
                    data_mut.insert("_provenance".into(), provenance);
                    data_mut.remove("sql");
                    data_mut.remove("metricflow");
                    data_mut.remove("cache");
                }
                let mut d = Diagnostic::new(
                    if stale { "warning" } else { "info" },
                    if stale {
                        "data.cache.stale"
                    } else {
                        "data.provenance"
                    },
                    message,
                    format!("{tag}.data"),
                );
                d.line = Some(line);
                d.tag = Some(tag);
                diagnostics.push(d);
            }
            Err(err) => {
                diagnostics.push(connector_diagnostic(&err, &block.tag, block.line));
            }
        }
    }
    Ok(())
}

fn load_sql_text(
    data: &Value,
    root: &Path,
    inputs: &mut crate::input_limits::InputSizes,
) -> Result<String, String> {
    let sql = data
        .get("sql")
        .and_then(Value::as_object)
        .ok_or_else(|| "data.sql must be a mapping".to_owned())?;
    if let Some(inline) = sql.get("inline").and_then(Value::as_str) {
        return Ok(inline.to_owned());
    }
    let path = sql
        .get("query")
        .and_then(Value::as_str)
        .ok_or_else(|| "data.sql.query must be a report-relative path".to_owned())?;
    let candidate = root.join(path);
    let canonical = candidate
        .canonicalize()
        .map_err(|err| {
            // SQL text given as `query` fails here: say what `query` is before the OS error.
            format!(
                "data.sql.query is the path of a SQL file in the report folder, and '{path}' cannot be read ({err}); give SQL text as data.sql.inline"
            )
        })?;
    let canonical_root = root
        .canonicalize()
        .map_err(|err| format!("canonicalize report root: {err}"))?;
    if !canonical.starts_with(&canonical_root) {
        return Err(format!("sql query path '{path}' escapes the report folder"));
    }
    let bytes = inputs.read(root, path)?;
    String::from_utf8(std::sync::Arc::unwrap_or_clone(bytes))
        .map_err(|err| format!("read sql query '{path}': {err}"))
}

fn parse_cache_policy(data: &Value) -> Result<Option<CachePolicy>, String> {
    let Some(value) = data
        .pointer("/sql/cache")
        .or_else(|| data.pointer("/metricflow/cache"))
        .or_else(|| data.get("cache"))
    else {
        return Ok(None);
    };
    let obj = value.as_object().ok_or("cache policy must be a mapping")?;
    let text = |name: &str| -> Result<Option<String>, String> {
        obj.get(name)
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| format!("cache {name} must be text"))
            })
            .transpose()
    };
    let policy = CachePolicy {
        key: text("key")?,
        ttl_seconds: text("ttl")?
            .map(|ttl| {
                parse_duration(&ttl).ok_or("cache ttl must be a duration such as 30s, 5m or 1h")
            })
            .transpose()?,
        fresh_until: text("freshness")?,
        always_refresh: obj
            .get("refresh")
            .map(|value| value.as_bool().ok_or("cache refresh must be true or false"))
            .transpose()?
            .unwrap_or(false),
    };
    policy.validate().map_err(|error| error.to_string())?;
    Ok(Some(policy))
}

fn parse_duration(input: &str) -> Option<u64> {
    let (num, unit) = input
        .trim()
        .split_at(input.trim().find(|c: char| !c.is_ascii_digit())?);
    let n: u64 = num.parse().ok()?;
    let seconds = match unit.trim() {
        "s" | "sec" | "secs" => 1,
        "m" | "min" | "mins" => 60,
        "h" | "hr" | "hrs" => 3_600,
        "d" | "day" | "days" => 86_400,
        _ => return None,
    };
    n.checked_mul(seconds)
}

fn parse_params(data: &Value) -> Result<BTreeMap<String, Value>, ConnectorError> {
    let Some(params) = data.pointer("/sql/params") else {
        return Ok(BTreeMap::new());
    };
    let Some(params) = params.as_object() else {
        return Err(ConnectorError::QueryRejected {
            reason: "data.sql.params must be a mapping of named scalar values".into(),
        });
    };
    if params
        .values()
        .any(|value| !matches!(value, Value::String(_) | Value::Number(_) | Value::Bool(_)))
    {
        return Err(ConnectorError::QueryRejected {
            reason: "data.sql.params values must be strings, numbers, booleans or date strings"
                .into(),
        });
    }
    Ok(params.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
}

fn parse_metricflow_spec(data: &Value) -> Result<MetricFlowSpec, String> {
    let obj = data
        .get("metricflow")
        .and_then(Value::as_object)
        .ok_or_else(|| "data.metricflow is missing".to_owned())?;
    let metrics = obj
        .get("metrics")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let group_by = obj
        .get("group_by")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let where_clause = obj
        .get("where")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let order_by = obj
        .get("order_by")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let limit = obj
        .get("limit")
        .map(|value| {
            value
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .filter(|n| *n > 0)
                .ok_or_else(|| "data.metricflow.limit must be a positive 32-bit integer".to_owned())
        })
        .transpose()?;
    Ok(MetricFlowSpec {
        metrics,
        group_by,
        where_clause,
        order_by,
        limit,
    })
}

fn fingerprint_sql(connector: &str, sql: &str, params: &BTreeMap<String, Value>) -> String {
    let mut hasher = Sha256::new();
    // Results cached before binding and external-access isolation are not safe to reuse.
    hasher.update(b"sql-worker-os-v2\0");
    hasher.update(connector.as_bytes());
    hasher.update(b"\0");
    hasher.update(sql.trim().as_bytes());
    hasher.update(b"\0");
    let canon = canonical_params(params);
    hasher.update(canon.as_bytes());
    hex_digest(hasher)
}

fn fingerprint_metricflow(connector: &str, spec: &MetricFlowSpec) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"metricflow-query-v2\0");
    hasher.update(connector.as_bytes());
    hasher.update(b"\0");
    let obj = serde_json::to_value(spec).expect("metric query serializes");
    let canon = canonical_json(&obj);
    hasher.update(canon.as_bytes());
    hex_digest(hasher)
}

fn canonical_params(params: &BTreeMap<String, Value>) -> String {
    let ordered: Map<String, Value> = params.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    serde_json::to_string(&ordered).unwrap_or_default()
}

fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let ordered: Map<String, Value> =
                map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            serde_json::to_string(&ordered).unwrap_or_default()
        }
        _ => serde_json::to_string(value).unwrap_or_default(),
    }
}

fn hex_digest(hasher: Sha256) -> String {
    let bytes = hasher.finalize();
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes.iter() {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn provenance_message(snapshot: &DatasetSnapshot, params: &BTreeMap<String, Value>) -> String {
    let mut message = format!(
        "{} from {} via {} (fingerprint {})",
        crate::cli::counted(snapshot.provenance.row_count, "row", "rows"),
        snapshot.provenance.connector,
        match snapshot.provenance.source {
            super::spec::ProvenanceSource::Sql => "sql",
            super::spec::ProvenanceSource::Metricflow => "metricflow",
            super::spec::ProvenanceSource::Cache => "cache",
        },
        snapshot
            .provenance
            .fingerprint
            .chars()
            .take(12)
            .collect::<String>()
    );
    if !params.is_empty() {
        message.push_str(&format!(
            "; parameters {}",
            params
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    message.push_str(&format!("; captured {}", snapshot.provenance.fetched_at));
    if snapshot.cache.as_ref().is_some_and(|cache| cache.stale) {
        message.insert_str(0, "Stale cached snapshot; refresh its source. ");
    }
    message
}

fn connector_diagnostic(err: &ConnectorError, tag: &str, line: usize) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        "error",
        connector_error_code(err),
        err.to_string(),
        format!("{tag}.data"),
    );
    if matches!(err, ConnectorError::SqlIsolationUnavailable) {
        diagnostic.fix = Some("Use Linux kernel 6.2 or newer with Landlock ABI 3 enabled.".into());
    }
    diagnostic.line = Some(line);
    diagnostic.tag = Some(tag.to_owned());
    diagnostic
}

fn connector_error_code(err: &ConnectorError) -> &'static str {
    match err {
        ConnectorError::UnknownConnector(_) => "data.connector.unknown",
        ConnectorError::QueryRejected { .. } => "data.connector.rejected",
        ConnectorError::Execution { .. }
        | ConnectorError::Sql { .. }
        | ConnectorError::SqlIsolationUnavailable => "data.connector.failed",
        ConnectorError::CacheMiss { .. } => "data.cache.miss",
        ConnectorError::Io(_) => "data.connector.io",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sql_isolation_diagnostic_names_the_requirement_without_query_advice() {
        let d = connector_diagnostic(&ConnectorError::SqlIsolationUnavailable, "chart", 12);
        assert_eq!(d.code, "data.connector.failed");
        assert!(d.message.contains("isolation is unavailable"));
        assert!(d.message.contains("Landlock ABI 3"));
        assert!(d.fix.as_deref().unwrap().contains("kernel 6.2"));
        assert!(!d.fix.as_deref().unwrap().contains("query"));
    }

    #[test]
    fn parameters_must_be_a_mapping_of_non_null_scalars() {
        for params in [
            serde_json::json!([]),
            serde_json::json!(null),
            serde_json::json!({"n": []}),
            serde_json::json!({"n": null}),
        ] {
            assert!(parse_params(&serde_json::json!({"sql": {"params": params}})).is_err());
        }
    }

    #[test]
    fn metricflow_query_identity_excludes_cache_controls_and_rejects_wrapped_limits() {
        let first = serde_json::json!({"metricflow":{"connector":"local","metrics":["orders"],"cache":{"key":"monthly","ttl":"60s"}}});
        let mut changed = first.clone();
        changed["metricflow"]["cache"] = serde_json::json!({"key":"other","refresh":true});
        assert_eq!(
            fingerprint_metricflow("local", &parse_metricflow_spec(&first).unwrap()),
            fingerprint_metricflow("local", &parse_metricflow_spec(&changed).unwrap())
        );
        changed["metricflow"]["metrics"] = serde_json::json!(["revenue"]);
        assert_ne!(
            fingerprint_metricflow("local", &parse_metricflow_spec(&first).unwrap()),
            fingerprint_metricflow("local", &parse_metricflow_spec(&changed).unwrap())
        );
        changed["metricflow"]["limit"] = serde_json::json!(4_294_967_296u64);
        assert!(parse_metricflow_spec(&changed).is_err());
    }
}
