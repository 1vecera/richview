use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, fmt};

pub(super) const MAX_ROWS: usize = 100_000;
/// Bump when SQL privacy changes so prior captures and source bindings cannot be reused.
pub(super) const SQL_METADATA_POLICY_VERSION: u8 = 1;

/// A canonical query request handed to a [`crate::connectors::registry::Connector`].
///
/// `fingerprint` includes the connector, canonical query/parameters or MetricFlow spec,
/// and trusted source binding under a private store key. It stays stable within that
/// store and binding, without wall-clock time.
pub struct Query<'a> {
    pub connector: &'a str,
    pub body: QueryBody<'a>,
    pub params: &'a BTreeMap<String, Value>,
    pub cache: Option<&'a super::CachePolicy>,
    pub fingerprint: &'a str,
}

pub enum QueryBody<'a> {
    Sql(&'a str),
    MetricFlow(&'a MetricFlowSpec),
    /// Snapshot lookup only; the registry resolves this against the cache store.
    Cache,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricFlowSpec {
    pub metrics: Vec<String>,
    #[serde(default)]
    pub group_by: Vec<String>,
    #[serde(default)]
    pub where_clause: Vec<String>,
    #[serde(default)]
    pub order_by: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// The materialized result handed back to the renderer. `rows` is the exact
/// JSON array already understood by `crates/richview-core/src/blocks.rs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetSnapshot {
    pub rows: Vec<Value>,
    pub schema: Vec<SchemaField>,
    pub provenance: Provenance,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<CacheRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaField {
    pub name: String,
    pub kind: FieldKind,
    #[serde(default)]
    pub nullable: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FieldKind {
    Integer,
    Number,
    String,
    Boolean,
    Date,
    Timestamp,
    Null,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    pub connector: String,
    pub source: ProvenanceSource,
    pub fetched_at: String,
    pub fingerprint: String,
    pub row_count: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProvenanceSource {
    Sql,
    Metricflow,
    Cache,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheRecord {
    pub key: String,
    pub materialized_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fresh_until: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<String>,
    pub stale: bool,
}

#[derive(Debug)]
pub enum ConnectorError {
    UnknownConnector(String),
    QueryRejected { reason: String },
    SqlIsolationUnavailable,
    Execution { connector: String, message: String },
    Sql { connector: String, error: SqlError },
    CacheMiss { key: String },
    Io(std::io::Error),
}

impl fmt::Display for ConnectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownConnector(name) => write!(f, "no connector named '{name}' is registered"),
            Self::QueryRejected { reason } => write!(f, "query rejected: {reason}"),
            Self::SqlIsolationUnavailable => write!(
                f,
                "SQL worker isolation is unavailable: SQL data sources need Linux kernel 6.2 or newer with Landlock ABI 3 enabled"
            ),
            Self::Execution { connector, message } => {
                write!(f, "connector '{connector}' failed: {message}")
            }
            Self::Sql { connector, error } => write!(f, "connector '{connector}' failed: {error}"),
            Self::CacheMiss { key } => write!(f, "no cached snapshot for '{key}'"),
            Self::Io(err) => write!(f, "cache I/O: {err}"),
        }
    }
}

/// Safe SQL failure data shared by the engine adapter and its isolated worker.
/// Native messages, identifiers, query text and filesystem paths never cross this boundary.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SqlError {
    Syntax { position: Option<SqlPosition> },
    MissingTable,
    MissingColumn,
    Conversion,
    TypeMismatch,
    DatabaseVersion,
    ExternalAccess,
    Database,
    Query,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SqlPosition {
    pub line: u32,
    pub column: u32,
}

impl fmt::Display for SqlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Syntax {
                position: Some(position),
            } => {
                return write!(
                    f,
                    "SQL syntax error at line {}, column {}; correct the SQL expression",
                    position.line, position.column
                );
            }
            Self::Syntax { position: None } => "SQL syntax error; correct the SQL expression",
            Self::MissingTable => {
                "SQL table or view was not found; check its name and declared database"
            }
            Self::MissingColumn => "SQL column was not found; check the field names",
            Self::Conversion => "SQL value could not be converted to the requested type",
            Self::TypeMismatch => {
                "SQL expression types do not match; use compatible types or an explicit cast"
            }
            Self::DatabaseVersion => {
                "SQL database was written by a newer DuckDB; export it with a compatible storage version"
            }
            Self::ExternalAccess => {
                "SQL external access was refused; report SQL cannot read files or use the network"
            }
            Self::Database => "SQL database could not be opened; check the declared source file",
            Self::Query => "SQL query failed; check its expressions and types",
        };
        f.write_str(message)
    }
}

impl std::error::Error for ConnectorError {}

impl From<std::io::Error> for ConnectorError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}
