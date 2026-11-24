//! Load a connector [`Registry`] from a report's `report.connectors.yaml` and
//! a trusted host-side `connectors.toml`.
//!
//! Split by trust boundary:
//! * `report.connectors.yaml` sits next to the report and only names runtime
//!   handles. It cannot carry credentials, URIs or command lines.
//! * `connectors.toml` is picked by the operator (CLI flag or env var) and
//!   binds each named handle to a concrete DuckDB file path or MetricFlow
//!   runtime (launcher command + dbt project directory).

use super::Registry;
use super::duckdb_adapter::DuckDbConnector;
use super::metricflow_adapter::{MetricFlowConnector, MetricFlowRuntime};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Debug, Deserialize)]
struct ReportConnectors {
    connectors: BTreeMap<String, ReportConnectorEntry>,
}

#[derive(Debug, Deserialize)]
struct ReportConnectorEntry {
    kind: String,
    /// For `duckdb`: a report-relative database path. For `metricflow`: must be absent.
    #[serde(default)]
    database: Option<String>,
    /// For `parquet`: a report-relative Parquet file, which report SQL reads as the table named
    /// after the connector.
    #[serde(default)]
    file: Option<String>,
    /// For `metricflow`: a trusted runtime handle resolved via `connectors.toml`.
    #[serde(default)]
    runtime: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct TrustedConnectors {
    /// Absolute worker path supplied by the operator; reports cannot select executables.
    #[serde(default)]
    sql_worker: Option<std::path::PathBuf>,
    #[serde(default)]
    cache_root: Option<String>,
    #[serde(default)]
    metricflow: BTreeMap<String, TrustedMetricFlowRuntime>,
}

#[derive(Debug, Deserialize)]
struct TrustedMetricFlowRuntime {
    /// Command used to launch `mf`, e.g. `["uv", "run", "--", "mf"]`.
    launcher: Vec<String>,
    /// Absolute path to the dbt project directory.
    project_dir: String,
    /// Environment variables merged into the `mf` subprocess, declared as
    /// `[metricflow.<handle>.env]` key = "value" entries in `connectors.toml`.
    #[serde(default)]
    env: BTreeMap<String, String>,
}

/// Build a [`Registry`] by reading the report-local `report.connectors.yaml`
/// (if present), binding each entry against the trusted `connectors.toml`
/// (if supplied). Snapshot-only reports can use a trusted store without an active connector.
/// With neither a report declaration nor trusted configuration, returns `None`.
pub fn load_registry(
    report_root: &Path,
    trusted_toml: Option<&Path>,
) -> Result<Option<Registry>, String> {
    let report_yaml = report_root.join("report.connectors.yaml");
    let report = if report_yaml.exists() {
        let text = fs::read_to_string(&report_yaml)
            .map_err(|err| format!("read {}: {err}", report_yaml.display()))?;
        let report: ReportConnectors = richview_core::yaml::from_str(&text)
            .map_err(|err| err.to_string())
            .and_then(|value| serde_json::from_value(value).map_err(|err| err.to_string()))
            .map_err(|err| format!("parse {}: {err}", report_yaml.display()))?;
        report
    } else if trusted_toml.is_some() {
        ReportConnectors {
            connectors: BTreeMap::new(),
        }
    } else {
        return Ok(None);
    };

    let trusted = read_trusted(trusted_toml)?;
    let canonical_root = report_root
        .canonicalize()
        .map_err(|_| "cannot resolve report root".to_owned())?;
    let cache_root = snapshot_root(&canonical_root, trusted_toml, &trusted)?;

    let mut registry = Registry::new(cache_root);

    for (name, entry) in report.connectors {
        match entry.kind.as_str() {
            kind @ ("duckdb" | "parquet") => {
                let parquet = kind == "parquet";
                // What the source is called in a message, and the key that names it.
                let (noun, key) = if parquet {
                    ("file", "file")
                } else {
                    ("database", "database")
                };
                let rel = if parquet { entry.file } else { entry.database }.ok_or_else(|| {
                    format!("connector '{name}' of kind '{kind}' needs a `{key}` path")
                })?;
                if !richview_core::document::report_relative_path(&rel) {
                    return Err(format!(
                        "connector {noun} must be a report-relative file path"
                    ));
                }
                let path = canonical_root.join(&rel);
                // A stored snapshot remains readable when its source is offline. No adapter
                // is registered for a missing source, so cache misses still fail closed.
                if !path.exists() && path.symlink_metadata().is_err() {
                    continue;
                }
                let canonical = path
                    .canonicalize()
                    .map_err(|err| format!("resolve {kind} {noun} '{rel}': {err}"))?;
                if !canonical.starts_with(&canonical_root) {
                    return Err(format!(
                        "connector '{name}' {noun} '{rel}' escapes the report folder"
                    ));
                }
                // The worker opens the file at this path; on Windows it opens a copy under a
                // name of the host's own.
                if parquet && cfg!(not(windows)) && super::parquet_source::pattern_path(&canonical)
                {
                    return Err(format!(
                        "connector '{name}' file '{rel}' {}. {}.",
                        super::parquet_source::PATTERN_PATH,
                        super::parquet_source::PATTERN_PATH_FIX
                    ));
                }
                if let Some(worker) = &trusted.sql_worker
                    && !worker.is_absolute()
                {
                    return Err(
                        "connectors.toml sql_worker must be an absolute trusted executable path"
                            .into(),
                    );
                }
                let connector = match (&trusted.sql_worker, parquet) {
                    (worker, true) => DuckDbConnector::parquet(canonical, worker.clone()),
                    (Some(worker), false) => {
                        DuckDbConnector::with_worker(canonical, worker.clone())
                    }
                    (None, false) => DuckDbConnector::new(canonical),
                };
                registry.register(name, connector);
            }
            "metricflow" => {
                let handle = entry.runtime.ok_or_else(|| {
                    format!("connector '{name}' of kind 'metricflow' needs a `runtime` handle")
                })?;
                let runtime_cfg = trusted.metricflow.get(&handle).ok_or_else(|| {
                    format!(
                        "connectors.toml does not define metricflow runtime '{handle}' for connector '{name}'"
                    )
                })?;
                let runtime = MetricFlowRuntime {
                    launcher: runtime_cfg.launcher.clone(),
                    project_dir: Path::new(&runtime_cfg.project_dir).to_path_buf(),
                    env: runtime_cfg.env.clone(),
                };
                registry.register(name, MetricFlowConnector::new(runtime));
            }
            other => {
                return Err(format!(
                    "connector '{name}' has unknown kind '{other}' (expected duckdb, parquet or metricflow)"
                ));
            }
        }
    }
    Ok(Some(registry))
}

/// Open only the server snapshot store; no adapter or executable is registered.
pub fn load_snapshot_registry(
    report_root: &Path,
    trusted_toml: Option<&Path>,
) -> Result<Registry, String> {
    let trusted = read_trusted(trusted_toml)?;
    let canonical_root = report_root
        .canonicalize()
        .map_err(|_| "cannot resolve report root".to_owned())?;
    Ok(Registry::new(snapshot_root(
        &canonical_root,
        trusted_toml,
        &trusted,
    )?))
}

fn read_trusted(trusted_toml: Option<&Path>) -> Result<TrustedConnectors, String> {
    Ok(if let Some(path) = trusted_toml {
        let text = fs::read_to_string(path)
            .map_err(|_| "cannot read trusted connectors.toml".to_owned())?;
        toml_lite::decode::<TrustedConnectors>(&text).map_err(|_| {
            "connectors.toml is invalid; check its syntax and field types".to_owned()
        })?
    } else {
        TrustedConnectors::default()
    })
}

fn snapshot_root(
    canonical_root: &Path,
    trusted_toml: Option<&Path>,
    trusted: &TrustedConnectors,
) -> Result<std::path::PathBuf, String> {
    let report_root = canonical_root;
    let cache_root = trusted
        .cache_root
        .as_deref()
        .map(|r| {
            if Path::new(r).is_absolute() {
                Path::new(r).to_path_buf()
            } else {
                trusted_toml
                    .and_then(Path::parent)
                    .unwrap_or(report_root)
                    .join(r)
            }
        })
        .map(Ok)
        .unwrap_or_else(|| {
            std::env::var_os("XDG_STATE_HOME")
                .map(std::path::PathBuf::from)
                .filter(|path| path.is_absolute())
                .or_else(|| {
                    std::env::var_os("HOME")
                        .map(std::path::PathBuf::from)
                        .filter(|path| path.is_absolute())
                        .map(|home| home.join(".local/state"))
                })
                // Windows usually sets neither; its per-user state folder stands in.
                .or_else(|| {
                    std::env::var_os("LOCALAPPDATA")
                        .map(std::path::PathBuf::from)
                        .filter(|path| path.is_absolute())
                })
                .map(|root| root.join("richview/snapshots"))
                .ok_or_else(|| {
                    "set a trusted cache_root, XDG_STATE_HOME or HOME for server snapshots"
                        .to_owned()
                })
        })?;
    Ok(cache_root.join(format!(
        "{:x}",
        Sha256::digest(canonical_root.as_os_str().as_encoded_bytes())
    )))
}

/// Tiny TOML subset decoder: only what `connectors.toml` needs today.
/// Supports top-level string values, nested tables (`[section]`,
/// `[section.sub]`) and string arrays. Avoids pulling the full `toml` crate
/// into the host just for this trusted file.
mod toml_lite {
    use serde::Deserialize;
    use serde_json::{Map, Value};

    pub fn decode<T: for<'de> Deserialize<'de>>(input: &str) -> Result<T, String> {
        let value = parse(input)?;
        serde_json::from_value(value).map_err(|err| err.to_string())
    }

    fn parse(input: &str) -> Result<Value, String> {
        let mut root = Map::new();
        let mut current_path: Vec<String> = Vec::new();
        for (lineno, raw) in input.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if let Some(stripped) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                current_path = stripped.split('.').map(str::to_owned).collect();
                ensure_table(&mut root, &current_path);
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| format!("line {}: expected key = value", lineno + 1))?;
            let value =
                parse_value(value.trim()).map_err(|err| format!("line {}: {err}", lineno + 1))?;
            insert(&mut root, &current_path, key.trim(), value);
        }
        Ok(Value::Object(root))
    }

    fn ensure_table(root: &mut Map<String, Value>, path: &[String]) {
        let mut cursor = root;
        for segment in path {
            let entry = cursor
                .entry(segment.clone())
                .or_insert_with(|| Value::Object(Map::new()));
            cursor = entry.as_object_mut().expect("table");
        }
    }

    fn insert(root: &mut Map<String, Value>, path: &[String], key: &str, value: Value) {
        let mut cursor = root;
        for segment in path {
            cursor = cursor
                .get_mut(segment)
                .and_then(Value::as_object_mut)
                .expect("ensure_table");
        }
        cursor.insert(key.to_owned(), value);
    }

    fn parse_value(input: &str) -> Result<Value, String> {
        if let Some(rest) = input.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
            return Ok(Value::String(rest.to_owned()));
        }
        if let Some(rest) = input.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            let mut items = Vec::new();
            for part in split_top_commas(rest) {
                let trimmed = part.trim();
                if trimmed.is_empty() {
                    continue;
                }
                items.push(parse_value(trimmed)?);
            }
            return Ok(Value::Array(items));
        }
        if let Ok(n) = input.parse::<i64>() {
            return Ok(Value::from(n));
        }
        if let Ok(b) = input.parse::<bool>() {
            return Ok(Value::from(b));
        }
        Err(format!("unsupported TOML value '{input}'"))
    }

    fn split_top_commas(input: &str) -> Vec<&str> {
        let mut depth = 0i32;
        let mut start = 0usize;
        let mut parts = Vec::new();
        let bytes = input.as_bytes();
        for (index, byte) in bytes.iter().enumerate() {
            match byte {
                b'[' => depth += 1,
                b']' => depth -= 1,
                b',' if depth == 0 => {
                    parts.push(&input[start..index]);
                    start = index + 1;
                }
                _ => {}
            }
        }
        if start < input.len() {
            parts.push(&input[start..]);
        }
        parts
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[derive(Deserialize)]
        struct T {
            cache_root: Option<String>,
            metricflow: std::collections::BTreeMap<String, Mf>,
        }

        #[derive(Deserialize)]
        struct Mf {
            launcher: Vec<String>,
            project_dir: String,
        }

        #[test]
        fn parses_the_connectors_toml_subset() {
            let input = r#"
cache_root = ".richview-cache"

[metricflow.okurka-local]
launcher = ["uv", "run", "--", "mf"]
project_dir = "/tmp/okurka"
"#;
            let decoded: T = decode(input).unwrap();
            assert_eq!(decoded.cache_root.as_deref(), Some(".richview-cache"));
            let mf = decoded.metricflow.get("okurka-local").unwrap();
            assert_eq!(mf.launcher, vec!["uv", "run", "--", "mf"]);
            assert_eq!(mf.project_dir, "/tmp/okurka");
        }
    }
}
