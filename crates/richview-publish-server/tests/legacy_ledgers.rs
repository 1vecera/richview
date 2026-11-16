//! Every ledger shape PR #70's Worker ever wrote must move over, with every fact kept (S27-655).
//!
//! `fixtures/pr70-legacy-ledgers.json` holds snapshots taken from the real old Worker, editor, repository and
//! policy modules at each version that changed what they store, driven through every writer against a fake
//! GitHub (`cloudflare/publish/test/legacy/generate-ledgers.mjs` regenerates it from Git history). The 30 Oct
//! deploy failed on one hand-missed record, the bootstrap authorization, so no shape here is written by hand.
use richview_publish_server::wire::migrate_json;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/pr70-legacy-ledgers.json")).unwrap()
}

/// True when every key and value of `old` is in `new`: serde may add defaults, but must not drop or alter a fact.
fn kept(old: &Value, new: &Value) -> bool {
    match (old, new) {
        (Value::Object(old), Value::Object(new)) => old
            .iter()
            .all(|(key, value)| new.get(key).is_some_and(|other| kept(value, other))),
        _ => old == new,
    }
}

fn migrate(snapshot: &Value) -> Value {
    let input = json!({
        "state": snapshot["state"], "activated": snapshot["activated"], "runtimes": snapshot["runtimes"],
    });
    serde_json::from_str(&migrate_json(&input.to_string())).unwrap()
}

#[test]
fn every_state_a_pr70_producer_wrote_migrates_with_every_fact_kept() {
    let fixture = fixture();
    let snapshots = fixture["snapshots"].as_array().unwrap();
    assert!(snapshots.len() >= 20, "the fixture lost snapshots");
    for snapshot in snapshots {
        let label = format!("{} {}", snapshot["version"], snapshot["step"]);
        let out = migrate(snapshot);
        assert_eq!(out["kind"], "migrated", "{label}: {out}");
        let (old, new) = (&snapshot["state"], &out["state"]);
        for field in [
            "generation",
            "active",
            "paused",
            "hidden",
            "policies",
            "roles",
            "edits",
            "approvals",
            "authorized",
            "builds",
        ] {
            // `paused` and `accessRevision` are absent from the oldest versions.
            if let Some(value) = old.get(field) {
                assert!(
                    kept(value, &new[field]),
                    "{label}: {field} changed\nold {value}\nnew {}",
                    new[field]
                );
            }
        }
        // The counters keep their values under their new names.
        assert_eq!(old["auditSequence"], new["audit_sequence"], "{label}");
        assert_eq!(
            old.get("accessRevision").unwrap_or(&json!(0)),
            &new["access_revision"],
            "{label}"
        );
        // Releases once live keep their commit; runtime file maps keep their hashes.
        for (id, record) in snapshot["activated"].as_object().unwrap() {
            assert_eq!(
                record["commit"], new["activated"][id],
                "{label}: activated {id}"
            );
        }
        assert!(
            kept(&snapshot["runtimes"], &new["runtimes"]),
            "{label}: runtimes"
        );
        assert_eq!(
            new["private"],
            json!({}),
            "{label}: nothing live becomes private"
        );
        assert_eq!(new["revision"], json!(0), "{label}");
    }
}

#[test]
fn the_snapshots_cover_every_record_kind_and_state_the_writers_can_produce() {
    let fixture = fixture();
    let mut seen = BTreeSet::new();
    for snapshot in fixture["snapshots"].as_array().unwrap() {
        let state = &snapshot["state"];
        for field in [
            "hidden",
            "policies",
            "roles",
            "edits",
            "approvals",
            "authorized",
            "builds",
        ] {
            for record in state[field].as_object().unwrap().values() {
                let mut keys: Vec<String> = record
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(key, value)| match key.as_str() {
                        "state" | "name" => format!("{key}={}", value.as_str().unwrap()),
                        _ => key.clone(),
                    })
                    .collect();
                keys.sort();
                seen.insert(format!("{field}{{{}}}", keys.join(",")));
            }
        }
        if state["paused"] == json!(true) {
            seen.insert("paused=true".into());
        }
        if state["accessRevision"].as_u64().is_some_and(|n| n > 0) {
            seen.insert("accessRevision>0".into());
        }
    }
    // One entry per writer in the PR #70 sources: edits (sync, desk create/edit, hide reconciliation, merge,
    // activation), builds, approvals, authorizations (bootstrap, approval, review), hides, roles, policies.
    for required in [
        "accessRevision>0",
        "paused=true",
        "approvals{actor,at,pull}",
        "authorized{actor,head}",
        "authorized{actor,base,head,pull}",
        "authorized{actor,at,base,head,pull}",
        "builds{release,state=built}",
        "builds{release,state=live}",
        "builds{reason,state=building}",
        "builds{reason,state=failed}",
        "edits{actor,branch,git_author,head,imported,number,state=pending review,url}",
        "edits{actor,branch,git_author,head,imported,number,state=closed,url}",
        "edits{actor,branch,contributors,head,number,path,state=pending review,url}",
        "edits{actor,branch,commit,contributors,head,number,path,state=building,url}",
        "edits{actor,branch,commit,contributors,head,number,path,release,state=live,url}",
        "hidden{actor,at,reason,reconciliation,revision}",
        "hidden{actor,at,pull,reason,reconciliation,revision}",
        "policies{audience,deleted,id,section,state=published}",
        "policies{audience,id,section,state=draft}",
        "roles{audiences,github_login,name=super editor,sections}",
        "roles{audiences,name=editor,sections}",
        "roles{audiences,name=reader,sections}",
    ] {
        assert!(
            seen.contains(required),
            "no snapshot holds {required}\nseen: {seen:#?}"
        );
    }
}

#[test]
fn a_hide_stored_before_hides_had_a_revision_gets_a_stable_one() {
    // 9ad16b97, the first PR #70 version, stored {actor, at, reason, reconciliation} without `revision`.
    let fixture = fixture();
    let old = fixture["snapshots"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| {
            s["state"]["hidden"]
                .as_object()
                .is_some_and(|h| h.values().any(|r| r.get("revision").is_none()))
        })
        .expect("the oldest producer's hide snapshot");
    let first = migrate(old);
    let second = migrate(old);
    assert_eq!(
        first, second,
        "the same ledger must convert to the same bytes"
    );
    for (id, hide) in first["state"]["hidden"].as_object().unwrap() {
        assert!(
            hide["revision"].as_str().is_some_and(|r| !r.is_empty()),
            "hide {id}"
        );
    }
    // A record that is neither the oldest hide shape nor a complete one is still refused, not guessed.
    let mut odd = old.clone();
    let id = odd["state"]["hidden"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    odd["state"]["hidden"][&id]["extra"] = json!(true);
    assert_eq!(migrate(&odd)["kind"], "invalid");
}
