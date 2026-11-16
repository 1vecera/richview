use super::spec::{ConnectorError, DatasetSnapshot, Query, QueryBody};
use super::{CachePolicy, SnapshotStore};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

/// A read-only data engine the native host can invoke on behalf of a chart.
pub trait Connector: Send + Sync {
    fn kind(&self) -> &'static str;
    /// Hash the trusted source binding so identical names cannot share another source's rows.
    fn source_identity(&self) -> String;
    fn execute(&self, request: &Query<'_>) -> Result<DatasetSnapshot, ConnectorError>;
}

pub struct Registry {
    connectors: BTreeMap<String, Arc<dyn Connector>>,
    store: SnapshotStore,
}

impl Registry {
    pub fn new(cache_root: PathBuf) -> Self {
        Self {
            connectors: BTreeMap::new(),
            store: SnapshotStore::new(cache_root),
        }
    }

    pub fn register<C: Connector + 'static>(&mut self, name: impl Into<String>, connector: C) {
        self.connectors.insert(name.into(), Arc::new(connector));
    }

    pub fn store(&self) -> &SnapshotStore {
        &self.store
    }

    /// Read a stored snapshot, or materialize and save one from the trusted source.
    /// Named/cache-only references never invoke an adapter. Their stale state remains visible.
    pub fn execute_adhoc(
        &self,
        connector: &str,
        body: QueryBody<'_>,
        params: &BTreeMap<String, Value>,
        cache: Option<&CachePolicy>,
        fingerprint: &str,
    ) -> Result<DatasetSnapshot, ConnectorError> {
        // The policy applies to cached SQL too, before any prior rows can be returned.
        if let QueryBody::Sql(sql) = body {
            super::duckdb_adapter::read_only_guard(sql)?;
        }
        if let Some(policy) = cache {
            policy.validate()?;
        }
        if let QueryBody::Cache = body {
            let snapshot =
                self.store
                    .load_key(fingerprint)?
                    .ok_or_else(|| ConnectorError::CacheMiss {
                        key: fingerprint.to_owned(),
                    })?;
            if let Some(adapter) = self.connectors.get(&snapshot.provenance.connector)
                && let Some(binding) = snapshot
                    .cache
                    .as_ref()
                    .and_then(|record| record.binding.as_deref())
                && binding != self.store.binding_identity(&adapter.source_identity())?
            {
                return Err(ConnectorError::QueryRejected {
                    reason: "snapshot belongs to another source binding; refresh the named query"
                        .into(),
                });
            }
            let mut snapshot = super::cache::annotate(snapshot, cache, fingerprint)?;
            if cache.is_some_and(|p| p.always_refresh) {
                snapshot
                    .cache
                    .as_mut()
                    .expect("cache record inserted")
                    .stale = true;
            }
            return Ok(snapshot);
        }
        let adapter = self
            .connectors
            .get(connector)
            .ok_or_else(|| ConnectorError::UnknownConnector(connector.to_owned()))?;
        let mut hash = Sha256::new();
        hash.update(b"connector-source-bound-v1\0");
        hash.update(fingerprint.as_bytes());
        hash.update(b"\0");
        let binding = self.store.binding_identity(&adapter.source_identity())?;
        hash.update(binding.as_bytes());
        let fingerprint = format!("{:x}", hash.finalize());
        let key = cache.and_then(|p| p.key.as_deref()).unwrap_or(&fingerprint);
        if !cache.is_some_and(|p| p.always_refresh)
            && let Some(snapshot) = self.store.load(&fingerprint)?
        {
            let snapshot = super::cache::annotate(snapshot, cache, key)?;
            if !snapshot
                .cache
                .as_ref()
                .expect("cache record inserted")
                .stale
            {
                if let Some(key) = cache.and_then(|p| p.key.as_deref()) {
                    self.store.save_key(key, &fingerprint, cache)?;
                }
                return Ok(snapshot);
            }
        }
        let request = Query {
            connector,
            body,
            params,
            cache,
            fingerprint: &fingerprint,
        };
        let snapshot = adapter.execute(&request)?;
        let mut snapshot = super::cache::annotate(snapshot, cache, key)?;
        snapshot
            .cache
            .as_mut()
            .expect("cache record inserted")
            .binding = Some(binding);
        self.store.save(&fingerprint, &snapshot)?;
        if let Some(key) = cache.and_then(|p| p.key.as_deref()) {
            self.store.save_key(key, &fingerprint, cache)?;
        }
        Ok(snapshot)
    }
}
