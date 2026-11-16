//! Registration verifies control of email before setting a password; the fake sends nothing.
use richview_app::{
    access::{Access, AccessError, Principal, Visibility},
    registration::{Registration, RegistrationError, VerificationSender},
};
use std::{
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct FakeSender(Mutex<Vec<(String, String)>>);
impl VerificationSender for FakeSender {
    fn send<'a>(
        &'a self,
        email: &'a str,
        link: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), ()>> + Send + 'a>> {
        self.0
            .lock()
            .unwrap()
            .push((email.to_owned(), link.to_owned()));
        Box::pin(async { Ok(()) })
    }
}
struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::current_dir().unwrap().join("tmp").join(format!(
            "signup-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        Self(root)
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn email_must_be_verified_before_a_password_or_account_exists() {
    let dir = Dir::new();
    let access = Access::open(&dir.0, Visibility::Private, None).unwrap();
    let fake = Arc::new(FakeSender::default());
    let registration =
        Registration::new(access.clone(), fake.clone(), "https://app.example.test").unwrap();
    registration.request("ALICE@example.test").await.unwrap();
    assert!(access.users().is_empty());
    let sent = fake.0.lock().unwrap()[0].clone();
    assert_eq!(sent.0, "alice@example.test");
    let url = reqwest::Url::parse(&sent.1).unwrap();
    assert_eq!(url.path(), "/signup/verify");
    let token = url
        .query_pairs()
        .find(|(key, _)| key == "token")
        .unwrap()
        .1
        .into_owned();
    let state = std::fs::read_to_string(dir.0.join(".access/state.json")).unwrap();
    assert!(!state.contains(&token), "only the token digest is stored");
    let user = registration
        .complete(&token, "Alice", "correct horse battery")
        .await
        .unwrap();
    assert_eq!(user.email, "alice@example.test");
    assert!(
        access
            .sign_in("alice@example.test", "correct horse battery")
            .is_ok()
    );
    assert!(matches!(
        registration
            .complete(&token, "Alice", "another long password")
            .await,
        Err(RegistrationError::Access(AccessError::Credentials))
    ));
    registration.request("alice@example.test").await.unwrap();
    assert_eq!(
        fake.0.lock().unwrap().len(),
        1,
        "an existing address reveals nothing and gets no new signup link"
    );
    let (_, secret) = access
        .create_self_token_for_fixture(&user.id, "default", "CLI")
        .unwrap();
    assert!(matches!(
        access.authenticate(Some(&secret), None),
        Principal::User(_)
    ));
}

#[tokio::test]
async fn signup_flood_is_bounded_and_does_not_prevent_existing_signin() {
    let dir = Dir::new();
    let access = Access::open(&dir.0, Visibility::Private, None).unwrap();
    access
        .create_user(
            "Existing",
            "existing@example.test",
            richview_app::access::OrgRole::Author,
            Some("correct horse battery"),
        )
        .unwrap();
    let fake = Arc::new(FakeSender::default());
    let registration =
        Registration::new(access.clone(), fake.clone(), "https://app.example.test").unwrap();
    for n in 0..60 {
        registration
            .request(&format!("fresh{n}@example.test"))
            .await
            .unwrap();
    }
    assert!(matches!(
        registration.request("overflow@example.test").await,
        Err(RegistrationError::Access(AccessError::Throttled))
    ));
    assert_eq!(fake.0.lock().unwrap().len(), 60);
    assert!(
        access
            .sign_in("existing@example.test", "correct horse battery")
            .is_ok()
    );
}

struct FailedSender;
impl VerificationSender for FailedSender {
    fn send<'a>(
        &'a self,
        _email: &'a str,
        _link: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), ()>> + Send + 'a>> {
        Box::pin(async { Err(()) })
    }
}

#[tokio::test]
async fn delivery_failure_allows_retry_and_expired_tokens_are_refused() {
    let dir = Dir::new();
    let access = Access::open(&dir.0, Visibility::Private, None).unwrap();
    let failure = Registration::new(
        access.clone(),
        Arc::new(FailedSender),
        "https://app.example.test",
    )
    .unwrap();
    assert!(matches!(
        failure.request("alice@example.test").await,
        Err(RegistrationError::Delivery)
    ));
    let fake = Arc::new(FakeSender::default());
    let retry =
        Registration::new(access.clone(), fake.clone(), "https://app.example.test").unwrap();
    retry.request("alice@example.test").await.unwrap();
    let url = reqwest::Url::parse(&fake.0.lock().unwrap()[0].1).unwrap();
    let token = url
        .query_pairs()
        .find(|(key, _)| key == "token")
        .unwrap()
        .1
        .into_owned();
    let path = dir.0.join(".access/state.json");
    let mut state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    state["registrations"][0]["created"] = 0.into();
    std::fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();
    let reopened = Access::open(&dir.0, Visibility::Private, None).unwrap();
    let expired = Registration::new(reopened, fake, "https://app.example.test").unwrap();
    assert!(matches!(
        expired
            .complete(&token, "Alice", "correct horse battery")
            .await,
        Err(RegistrationError::Access(AccessError::Credentials))
    ));
}

#[test]
fn personal_tokens_are_bounded_and_one_person_cannot_revoke_anothers() {
    let dir = Dir::new();
    let access = Access::open(&dir.0, Visibility::Private, None).unwrap();
    let alice = access
        .create_user(
            "Alice",
            "alice@example.test",
            richview_app::access::OrgRole::Author,
            None,
        )
        .unwrap();
    let bob = access
        .create_user(
            "Bob",
            "bob@example.test",
            richview_app::access::OrgRole::Author,
            None,
        )
        .unwrap();
    let (id, token) = access
        .create_self_token_for_fixture(&alice.id, "default", "CLI")
        .unwrap();
    assert!(matches!(
        access.revoke_token_for_fixture(&bob.id, &id),
        Err(AccessError::NotFound)
    ));
    assert!(matches!(
        access.authenticate(Some(&token), None),
        Principal::User(_)
    ));
    assert!(
        !serde_json::to_string(&access.tokens(&alice.id))
            .unwrap()
            .contains(&token)
    );
    access.revoke_token_for_fixture(&alice.id, &id).unwrap();
    assert!(matches!(
        access.authenticate(Some(&token), None),
        Principal::Anonymous
    ));
    for _ in 0..20 {
        access
            .create_self_token_for_fixture(&alice.id, "default", "CLI")
            .unwrap();
    }
    assert!(matches!(
        access.create_self_token_for_fixture(&alice.id, "default", "overflow"),
        Err(AccessError::Invalid(_))
    ));
}
