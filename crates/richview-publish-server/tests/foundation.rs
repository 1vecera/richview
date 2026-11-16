use richview_publish_server::{access::*, host::*, ledger::*};
use std::collections::BTreeMap;

fn cfg() -> Config {
    Config {
        publication_host: "news.example".into(),
        repository: "o/r".into(),
        branch: "publish/news".into(),
        content_root: "publications/news".into(),
        publisher_identity: "service:publisher".into(),
        bootstrap_super_editor: "email:owner@example.com".into(),
        ..Config::default()
    }
}
fn who(key: &str) -> Principal {
    Principal {
        key: key.into(),
        name: key.into(),
        email: None,
        service: key.starts_with("service:"),
    }
}

#[test]
fn the_publisher_is_never_an_editorial_role() {
    let mut roles = BTreeMap::new();
    roles.insert(
        "service:publisher".to_string(),
        Access {
            name: RoleName::SuperEditor,
            sections: vec!["*".into()],
            audiences: vec!["*".into()],
            github_login: None,
            byline: None,
        },
    );
    let a = role(Some(&who("service:publisher")), &cfg(), &roles);
    assert_eq!(a.name, RoleName::Publisher);
    assert!(!a.is_editor());
}
#[test]
fn admission_alone_is_a_reader_and_the_bootstrap_owner_is_a_super_editor() {
    let roles = BTreeMap::new();
    assert_eq!(
        role(Some(&who("email:x@y.z")), &cfg(), &roles).name,
        RoleName::Reader
    );
    assert_eq!(
        role(Some(&who("email:owner@example.com")), &cfg(), &roles).name,
        RoleName::SuperEditor
    );
    assert_eq!(role(None, &cfg(), &roles).name, RoleName::Anonymous);
}
#[test]
fn anonymous_reads_public_only_and_never_the_internal_edition() {
    let anon = role(None, &cfg(), &BTreeMap::new());
    assert!(audience(&anon, "public", false));
    assert!(!audience(&anon, "public", true));
    assert!(!audience(&anon, "members", false));
}
#[test]
fn aliases_of_one_github_login_are_one_actor() {
    let mut roles = BTreeMap::new();
    roles.insert(
        "email:a@x".to_string(),
        Access {
            name: RoleName::SuperEditor,
            sections: vec![],
            audiences: vec![],
            github_login: Some("Ada".into()),
            byline: None,
        },
    );
    assert!(same_actor("email:a@x", "github:ada", &cfg(), &roles));
    assert!(!same_actor("email:a@x", "github:bob", &cfg(), &roles));
}
#[test]
fn a_role_change_sends_approved_changes_back_to_review() {
    let mut s = State::default();
    s.approvals.insert(
        "h".into(),
        Approval {
            actor: "email:a@x".into(),
            pull: 3,
            at: None,
            head: None,
            base: None,
            origin: None,
        },
    );
    s.edits.insert(
        3,
        Edit {
            number: 3,
            url: String::new(),
            head: "h".into(),
            branch: "b".into(),
            actor: "email:b@x".into(),
            git_author: None,
            imported: false,
            state: EditState::Approved,
            path: None,
            contributors: vec![],
            commit: None,
            release: None,
            author_is_editor: false,
        },
    );
    s.revoke_approvals_of("email:a@x");
    assert!(s.approvals.is_empty());
    assert_eq!(s.edits[&3].state, EditState::PendingReview);
}
#[test]
fn state_round_trips_through_json_with_old_fields_missing() {
    let s: State = serde_json::from_str(r#"{"generation":4,"active":"abc"}"#).unwrap();
    assert_eq!(s.generation, 4);
    assert!(s.hidden.is_empty());
}

#[test]
fn base64_round_trips_every_length() {
    use richview_publish_server::wire::{decode, encode};
    for n in 0..9u8 {
        let bytes: Vec<u8> = (0..n).map(|i| i.wrapping_mul(37)).collect();
        assert_eq!(decode(&encode(&bytes)).unwrap(), bytes);
    }
    assert_eq!(encode(b"Man"), "TWFu");
    assert_eq!(encode(b"Ma"), "TWE=");
}
#[test]
fn the_json_protocol_answers_a_status_request_for_an_unknown_host_with_a_404_page() {
    let input = serde_json::json!({
        "config": {"publication_host":"news.example","repository":"o/r","branch":"b","content_root":"c","publisher_identity":"","bootstrap_super_editor":""},
        "principal": null, "state": {}, "now": "2026-10-27T00:00:00Z", "nonce": "n",
        "request": {"method":"GET","host":"evil.example","path":"/_status"}
    });
    let out: serde_json::Value = serde_json::from_str(&richview_publish_server::wire::handle_json(
        &input.to_string(),
    ))
    .unwrap();
    assert_eq!(out["kind"], "done");
    assert_eq!(out["response"]["status"], 404);
}
