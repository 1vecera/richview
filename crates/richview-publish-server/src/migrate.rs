//! Moves the ledger PR #70's JavaScript Worker stored into this server's format, in place and deterministically.
//!
//! PR #70 kept one `state` record (camel-cased audit counters), one `activated:<release>` record per release that
//! was ever live and one `runtime:<id>` record per runtime file map. The same Durable Object serves both Workers,
//! so the new adapter converts on first load. Releases and objects in R2 are unchanged: the builder that wrote them
//! is the same. Everything that was public stays public; OPN-54's private-until-public applies only to editor
//! publishes made after the move, so `private` starts empty.
use crate::ledger::State;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// True for a record written by PR #70: the new ledger always carries a `revision`.
pub fn is_pr70(state: &Value) -> bool {
    state.is_object() && state.get("revision").is_none()
}

/// Converts PR #70's `state` record plus its `activated:` and `runtime:` records into a ledger.
///
/// `activated` and `runtimes` are keyed by release id and runtime id (the part after the prefix).
pub fn from_pr70(
    old: &Value,
    activated: &BTreeMap<String, Value>,
    runtimes: &BTreeMap<String, Value>,
) -> Result<State, String> {
    let Some(old) = old.as_object() else {
        return Err("The stored ledger is not an object".into());
    };
    let mut next = Map::new();
    for (key, value) in old {
        let renamed = match key.as_str() {
            "auditSequence" => "audit_sequence",
            "accessRevision" => "access_revision",
            other => other,
        };
        next.insert(renamed.into(), value.clone());
    }
    // PR #70's first activation stored {actor: "bootstrap", head: commit}, without a pull request.
    // Use the same sentinel as this server's bootstrap path, only for that exact legacy shape.
    if let Some(Value::Object(authorized)) = next.get_mut("authorized") {
        for (commit, record) in authorized {
            if let Some(approval) = record.as_object_mut()
                && approval.len() == 2
                && approval.get("actor").and_then(Value::as_str) == Some("bootstrap")
                && approval.get("head").and_then(Value::as_str) == Some(commit.as_str())
            {
                approval.insert("pull".into(), Value::from(0));
            }
        }
    }
    // PR #70's first version (9ad16b97) stored a hide as {actor, at, reason, reconciliation[, pull]} without a
    // revision. Only that exact shape gets one, derived from the item so converting twice gives the same bytes.
    if let Some(Value::Object(hidden)) = next.get_mut("hidden") {
        const FIRST: [&str; 4] = ["actor", "at", "reason", "reconciliation"];
        for (id, record) in hidden {
            if let Some(hide) = record.as_object_mut()
                && !hide.contains_key("revision")
                && FIRST.iter().all(|key| hide.contains_key(*key))
                && hide
                    .keys()
                    .all(|key| key == "pull" || FIRST.contains(&key.as_str()))
            {
                hide.insert("revision".into(), Value::from(format!("legacy:{id}")));
            }
        }
    }
    next.insert("revision".into(), Value::from(0));
    next.insert(
        "activated".into(),
        Value::Object(
            activated
                .iter()
                .map(|(id, record)| {
                    let commit = record.get("commit").and_then(Value::as_str).unwrap_or("");
                    (id.clone(), Value::from(commit))
                })
                .collect(),
        ),
    );
    next.insert(
        "runtimes".into(),
        Value::Object(
            runtimes
                .iter()
                .map(|(id, map)| (id.clone(), map.clone()))
                .collect(),
        ),
    );
    // Absent on purpose: nothing live is private.
    next.remove("private");
    serde_json::from_value(Value::Object(next))
        .map_err(|e| format!("The stored ledger does not fit: {e}"))
}
