//! Hosted links are private, bounded and remain inspectable across restart without consuming them.
use richview_app::access::{Access, LinkPurpose};

#[test]
fn link_inspection_survives_restart_and_address_rate_limit_is_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let access = Access::open_hosted(dir.path()).unwrap();
    let token = access
        .issue_link("Alice@Example.test", LinkPurpose::Signup)
        .unwrap()
        .unwrap();
    assert_eq!(access.inspect_link(&token), Some(LinkPurpose::Signup));
    assert_eq!(access.inspect_link(&token), Some(LinkPurpose::Signup));
    let contents = std::fs::read_to_string(dir.path().join(".access/links.json")).unwrap();
    assert!(!contents.contains(&token));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(dir.path().join(".access/links.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    for _ in 0..2 {
        assert!(
            access
                .issue_link("alice@example.test", LinkPurpose::Reset)
                .unwrap()
                .is_some()
        );
    }
    assert!(
        access
            .issue_link("alice@example.test", LinkPurpose::Signup)
            .unwrap()
            .is_none()
    );
    assert!(
        access
            .issue_link("bob@example.test", LinkPurpose::Signup)
            .unwrap()
            .is_some()
    );
    drop(access);
    let reopened = Access::open_hosted(dir.path()).unwrap();
    assert_eq!(reopened.inspect_link(&token), Some(LinkPurpose::Signup));
    assert!(
        reopened
            .issue_link("alice@example.test", LinkPurpose::Signup)
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn signup_and_reset_are_single_use_and_reset_ends_existing_sessions() {
    let dir = tempfile::tempdir().unwrap();
    let access = Access::open_hosted(dir.path()).unwrap();
    let link = access
        .issue_link("alice@example.test", LinkPurpose::Signup)
        .unwrap()
        .unwrap();
    assert!(!access.has_users());
    let alice = access
        .complete_hosted_link(&link, "Alice", "first password long")
        .await
        .unwrap();
    assert!(alice.verified.is_some());
    let orgs = access.organizations_for(&alice.id);
    assert_eq!(orgs.len(), 1);
    assert_eq!(orgs[0].id.len(), 8);
    assert_eq!(access.inspect_link(&link), None);
    assert!(
        access
            .complete_hosted_link(&link, "Other", "other password long")
            .await
            .is_err()
    );
    let (old_session, _) = access
        .sign_in("alice@example.test", "first password long")
        .unwrap();
    let (_, old_token) = access
        .create_self_token_for_fixture(&alice.id, &orgs[0].id, "before reset")
        .unwrap();
    let key = format!("{}/report", orgs[0].id);
    let owner = access.authenticate(Some(&old_token), None);
    access.claim(|| owner.clone(), &key).unwrap();
    assert!(access.can(&owner, &key, richview_app::access::Right::Delete));
    let reset = access
        .issue_link("ALICE@example.test", LinkPurpose::Reset)
        .unwrap()
        .unwrap();
    access
        .complete_hosted_link(&reset, "", "second password long")
        .await
        .unwrap();
    assert!(matches!(
        access.authenticate(None, Some(&old_session)),
        richview_app::access::Principal::Anonymous
    ));
    assert!(matches!(
        access.authenticate(Some(&old_token), None),
        richview_app::access::Principal::Anonymous
    ));
    assert!(access.tokens_for_org(&alice.id, &orgs[0].id).is_empty());
    assert!(
        access
            .sign_in("alice@example.test", "second password long")
            .is_ok()
    );
    assert!(
        access
            .complete_hosted_link(&reset, "", "third password long")
            .await
            .is_err()
    );
    drop(access);
    let reopened = Access::open_hosted(dir.path()).unwrap();
    assert!(matches!(
        reopened.authenticate(Some(&old_token), None),
        richview_app::access::Principal::Anonymous
    ));
}

#[test]
fn a_full_link_table_refuses_new_entries_without_growing() {
    let dir = tempfile::tempdir().unwrap();
    let access = Access::open_hosted(dir.path()).unwrap();
    drop(access);
    let path = dir.path().join(".access/links.json");
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let records: Vec<_> = (0..5000).map(|i| serde_json::json!({"sha256": format!("{i:064x}"), "purpose":"signup", "email":format!("{i}@example.test"), "created":at, "expires":at + 3600, "used":false})).collect();
    std::fs::write(&path, serde_json::to_vec(&records).unwrap()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let access = Access::open_hosted(dir.path()).unwrap();
    assert!(
        access
            .issue_link("new@example.test", LinkPurpose::Signup)
            .unwrap()
            .is_none()
    );
    let after: Vec<serde_json::Value> =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(after.len(), 5000);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sign_in_racing_a_reset_never_leaves_an_old_password_session() {
    let dir = tempfile::tempdir().unwrap();
    let access = Access::open_hosted(dir.path()).unwrap();
    let signup = access
        .issue_link("race@example.test", LinkPurpose::Signup)
        .unwrap()
        .unwrap();
    access
        .complete_hosted_link(&signup, "Race", "old password long")
        .await
        .unwrap();
    let reset = access
        .issue_link("race@example.test", LinkPurpose::Reset)
        .unwrap()
        .unwrap();
    let resetting = access.clone();
    let reset_task = tokio::spawn(async move {
        resetting
            .complete_hosted_link(&reset, "", "new password long")
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    let signing_in = access.clone();
    let login = tokio::task::spawn_blocking(move || {
        signing_in.sign_in("race@example.test", "old password long")
    });
    reset_task.await.unwrap().unwrap();
    if let Ok((cookie, _)) = login.await.unwrap() {
        assert!(matches!(
            access.authenticate(None, Some(&cookie)),
            richview_app::access::Principal::Anonymous
        ));
    }
    assert!(
        access
            .sign_in("race@example.test", "new password long")
            .is_ok()
    );
}

#[test]
fn display_names_cannot_bypass_address_limits() {
    let dir = tempfile::tempdir().unwrap();
    let access = Access::open_hosted(dir.path()).unwrap();
    for name in ["a", "b", "c", "d"] {
        let address = format!("{name}<victim@example.test>");
        assert!(
            access
                .issue_link(&address, LinkPurpose::Signup)
                .unwrap()
                .is_none()
        );
        assert!(
            access
                .issue_link(&address, LinkPurpose::Reset)
                .unwrap()
                .is_none()
        );
    }
    for address in [
        "Victim@Example.test",
        "victim@example.test",
        "VICTIM@example.test",
    ] {
        assert!(
            access
                .issue_link(address, LinkPurpose::Signup)
                .unwrap()
                .is_some()
        );
    }
    assert!(
        access
            .issue_link("victim@example.test", LinkPurpose::Reset)
            .unwrap()
            .is_none()
    );
}
