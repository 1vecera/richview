//! Chart-local data connectors: SQL, dbt MetricFlow and cached snapshots.
//!
//! Core stays pure. This module is the native host's bridge from a chart's
//! `data.sql` / `data.metricflow` / `data.cache` block to the inline-rows
//! shape the pure renderer already consumes.
//!
//! Report source names connector handles and queries. Trusted operator configuration
//! supplies runtimes and credentials; rendering receives resolved snapshot rows and
//! capture metadata. The host selects `connectors.toml` explicitly with `--connectors`
//! or `RICHVIEW_CONNECTORS`. Snapshots live in a report-scoped server store.

pub mod cache;
pub mod config;
pub mod duckdb_adapter;
pub mod metricflow_adapter;
pub mod registry;
pub mod resolve;
pub mod spec;
pub mod sql_worker;

pub use cache::{CachePolicy, SnapshotStore};
pub use config::load_registry;
pub use registry::Registry;
pub use spec::{
    ConnectorError, DatasetSnapshot, FieldKind, MetricFlowSpec, Provenance, Query, QueryBody,
    SchemaField,
};
