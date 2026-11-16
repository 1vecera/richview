//! A publication against readers and access changes. Each test decides the order of the two
//! requests with a held point (`crate::test_points`), never with a sleep. Rendering needs the
//! report runtime, so these run with the `embedded-runtime` feature.
use super::*;
use crate::access::{Access, OrgRole, Principal, Visibility};
use crate::storage::FsStorage;
use crate::test_points::{Held, hold};
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

const ORG: &str = "aaaa0001";
/// "Confidential,rows\n1,2\n" as the publish API carries a linked file.
const SECRET_FILE: &str = "Q29uZmlkZW50aWFsLHJvd3MKMSwyCg==";
const PLAIN_FILE: &str = "YSxiCjEsMgo=";

fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

/// Organization A with an owner who owns the public report `slug` (version 1.0.0, with one
/// linked file) and an admin who does not; each has a CLI token `token-<id>`, and the owner a
/// browser session.
struct Host {
    state: AppState,
    key: String,
    slug: &'static str,
    _dir: tempfile::TempDir,
}

async fn host(slug: &'static str) -> Host {
    let dir = tempfile::tempdir().unwrap();
    let key = format!("{ORG}/{slug}");
    std::fs::create_dir_all(dir.path().join(".access")).unwrap();
    let person = |id: &str| {
        json!({"id":id,"name":id,"email":format!("{id}@example.test"),"created":"2026-01-01T00:00:00.000Z",
            "verified":"2026-01-01T00:00:00.000Z",
            "tokens":[{"id":"t-a","name":"CLI","created":"2026-01-01T00:00:00.000Z",
                "sha256":digest(&format!("token-{id}")),"org":ORG}]})
    };
    let invitations = if slug == "renew-ended" {
        json!([{"id":"i-renew", "org":ORG, "email":"new@example.test", "role":"reader", "inviter":"owner", "created":1, "expires":4_102_444_800u64}])
    } else {
        json!([])
    };
    let state = json!({"schema":2,"tenancy":"hosted",
        "organizations":[{"id":ORG,"name":"Acme","created":"2026-01-01T00:00:00.000Z","suspended":false,"limits":{}}],
        "memberships":[
            {"person":"owner","org":ORG,"role":"owner","created":"2026-01-01T00:01:00.000Z"},
            {"person":"admin","org":ORG,"role":"admin","created":"2026-01-01T00:02:00.000Z"}],
        "users":[person("owner"), person("admin")],
        "sessions":[{"sha256":digest("session-owner"),"csrf":"csrf-owner","user":"owner","expires":4_102_444_800u64,"scope":""}],
        "acls":{}, "invitations":invitations});
    std::fs::write(
        dir.path().join(".access/state.json"),
        serde_json::to_vec(&state).unwrap(),
    )
    .unwrap();
    let host = Host {
        state: AppState {
            storage: FsStorage::new(dir.path())
                .unwrap()
                .with_hosted(true)
                .with_quota(Some(100_000_000)),
            access: Access::open_hosted(dir.path()).unwrap(),
            preview: None,
            public_origin: Some("https://app.test".into()),
            report_origin: Some("https://reports.test".into()),
        },
        key,
        slug,
        _dir: dir,
    };
    let first = host
        .publish_as("owner", "1.0.0", "Open", Some("public"), true)
        .await
        .unwrap();
    assert_eq!(first, StatusCode::OK);
    host
}

impl Host {
    fn owner(&self) -> Principal {
        self.state.access.authenticate(None, Some("session-owner"))
    }

    fn visibility(&self) -> Visibility {
        self.state.access.acl(&self.key).visibility
    }

    fn stored(&self, version: &str) -> bool {
        self.state
            .storage
            .summary(&self.key, Some(version))
            .ok()
            .flatten()
            .is_some()
    }

    fn demote_admin(&self) {
        self.state
            .access
            .set_member_role(|| self.owner(), ORG, "admin", OrgRole::Reader)
            .unwrap();
    }

    /// `who` publishes `version`. Its description, text and linked file all carry `word`.
    fn publish_as(
        &self,
        who: &str,
        version: &str,
        word: &str,
        visibility: Option<&str>,
        new: bool,
    ) -> tokio::task::JoinHandle<StatusCode> {
        let source = format!(
            "---\ntitle: Q3\ndescription: {word} summary\nversion: {version}\n---\n\n# Q3\n\n{word} figures, with [the rows](data.csv).\n"
        );
        let file = if word == "Confidential" {
            SECRET_FILE
        } else {
            PLAIN_FILE
        };
        let mut body = json!({"slug":self.slug,"new":new,"source":source,
            "files":{"data.csv":file},
            "reportFolder":{"root":"t","source":"report.rmd","files":["report.rmd","data.csv"]}});
        if let Some(visibility) = visibility {
            body["visibility"] = json!(visibility);
        }
        let request = Request::builder()
            .method("POST")
            .uri("/api/reports")
            .header(header::HOST, "app.test")
            .header(header::AUTHORIZATION, format!("Bearer token-{who}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let app = crate::server::build_app(self.state.clone());
        tokio::spawn(async move { app.oneshot(request).await.unwrap().status() })
    }

    /// The admin publishes confidential `version`, asking for `private`.
    fn publish(&self, version: &str) -> tokio::task::JoinHandle<StatusCode> {
        self.publish_as("admin", version, "Confidential", Some("private"), false)
    }

    fn routers(&self) -> [Router; 2] {
        [
            crate::server::build_app(self.state.clone()),
            crate::server::build_report_app(self.state.clone()),
        ]
    }

    /// The router that answers `request` with 200 today.
    async fn router_for(&self, request: &Ask) -> Router {
        let mut seen = vec![];
        for router in self.routers() {
            let (status, _, _) = request.send(&router).await;
            if status == StatusCode::OK {
                return router;
            }
            seen.push(status);
        }
        panic!(
            "no router serves {} {}: {seen:?}",
            request.method, request.path
        );
    }
}

/// One request: anonymous, or with a bearer token or a cookie.
#[derive(Clone)]
struct Ask {
    method: Method,
    path: String,
    token: Option<&'static str>,
    cookie: Option<String>,
    /// A browser form's fields, already encoded.
    form: Option<String>,
}

impl Ask {
    fn get(path: impl Into<String>) -> Self {
        Self {
            method: Method::GET,
            path: path.into(),
            token: None,
            cookie: None,
            form: None,
        }
    }

    /// The owner's browser posting a form.
    fn form(path: impl Into<String>, fields: &str) -> Self {
        Self {
            method: Method::POST,
            cookie: Some("rv_session=session-owner".into()),
            form: Some(format!("csrf=csrf-owner&{fields}")),
            ..Self::get(path)
        }
    }

    /// The status, the body and the `Set-Cookie` header.
    async fn send(&self, router: &Router) -> (StatusCode, String, Option<String>) {
        let mut request = Request::builder()
            .method(self.method.clone())
            .uri(&self.path)
            .header(header::HOST, "reports.test");
        if let Some(token) = self.token {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        if let Some(cookie) = &self.cookie {
            request = request.header(header::COOKIE, cookie);
        }
        let body = match &self.form {
            Some(form) => {
                request = request.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
                Body::from(form.clone())
            }
            None => Body::empty(),
        };
        let response = router
            .clone()
            .oneshot(request.body(body).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let location = response
            .headers()
            .get(header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let body = to_bytes(response.into_body(), 50_000_000).await.unwrap();
        (
            status,
            format!("{location}\n{}", String::from_utf8_lossy(&body)),
            cookie,
        )
    }
}

/// Wait for a held point without parking the test's own thread on the runtime.
async fn reached(held: &Held) {
    tokio::task::block_in_place(|| held.reached());
}

/// Every way a stored version leaves the server. Each reader passes its rights check for the
/// coming 1.0.1 and is held there; then the report is closed and the confidential 1.0.1 is
/// stored; only then does the reader fetch. The readers that need more than `view` are the
/// admin's, who is demoted while held, and the owner publishes.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_reader_let_in_before_a_report_closed_gets_nothing_of_the_version_stored_after() {
    struct Case {
        slug: &'static str,
        method: Method,
        path: &'static str,
        /// The right the reader's check is for, and whether the reader is the admin.
        right: &'static str,
        admin: bool,
    }
    let anonymous = |slug, method, path| Case {
        slug,
        method,
        path,
        right: "view",
        admin: false,
    };
    let cases = [
        anonymous("read-raw", Method::GET, "/raw/{key}/v/{version}"),
        anonymous("read-latest", Method::GET, "/raw/{key}"),
        anonymous("read-page", Method::GET, "/r/{key}/v/{version}"),
        anonymous("read-head", Method::HEAD, "/r/{key}/v/{version}"),
        anonymous(
            "read-download",
            Method::GET,
            "/r/{key}/v/{version}/download",
        ),
        anonymous(
            "read-file",
            Method::GET,
            "/r/{key}/v/{version}/files/data.csv",
        ),
        anonymous("read-card", Method::GET, "/r/{key}/v/{version}/share.png"),
        anonymous("read-api", Method::GET, "/api/reports/{key}"),
        anonymous("read-versions", Method::GET, "/api/reports/{key}/versions"),
        Case {
            slug: "read-bundle",
            method: Method::GET,
            path: "/api/reports/{key}/v/{version}",
            right: "edit",
            admin: true,
        },
        Case {
            slug: "read-regenerate",
            method: Method::POST,
            path: "/api/reports/{key}/v/{version}/regenerate",
            right: "manage",
            admin: true,
        },
    ];
    let mut leaks = vec![];
    for case in cases {
        let slug = case.slug;
        let host = host(slug).await;
        let ask = |version: &str| Ask {
            method: case.method.clone(),
            path: case
                .path
                .replace("{key}", &host.key)
                .replace("{version}", version),
            token: case.admin.then_some("token-admin"),
            cookie: None,
            form: None,
        };
        let router = host.router_for(&ask("1.0.0")).await;

        let let_in = hold(&format!("authorized.{}", case.right), &host.key);
        let reader = {
            let (router, ask) = (router.clone(), ask("1.0.1"));
            tokio::spawn(async move { ask.send(&router).await })
        };
        reached(&let_in).await;
        // The reader holds a passed check. Everything else happens now.
        let publisher = if case.admin {
            host.demote_admin();
            "owner"
        } else {
            "admin"
        };
        let published = host
            .publish_as(publisher, "1.0.1", "Confidential", Some("private"), false)
            .await
            .unwrap();
        assert_eq!(published, StatusCode::OK, "{slug}");
        assert_eq!(host.visibility(), Visibility::Private);
        assert!(host.stored("1.0.1"));
        let_in.release();

        let (status, body, _) = reader.await.unwrap();
        // An answer about "the latest" must not even name the new version.
        let names_it = !case.path.contains("{version}") && body.contains("1.0.1");
        if status == StatusCode::OK || body.contains("Confidential") || names_it {
            leaks.push(format!("{slug} ({status})"));
        }
    }
    assert!(
        leaks.is_empty(),
        "readers that got the closed version: {leaks:?}"
    );
}

/// A ticket for the report origin is single-use. A publication that overlaps its redemption
/// must not cost the reader the session the ticket bought.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_report_ticket_redeemed_during_a_publication_still_gives_its_session() {
    let host = host("ticket-redeem").await;
    let ticket = host
        .state
        .access
        .issue_grant(&host.owner(), &host.key, Some("session-owner"))
        .unwrap();
    let reports = crate::server::build_report_app(host.state.clone());
    let redeemed = hold("ticket.redeemed", &host.key);
    let reader = {
        let ask = Ask::get(format!("/r/{}?grant={ticket}", host.key));
        tokio::spawn(async move { ask.send(&reports).await })
    };
    reached(&redeemed).await;
    let published = host
        .publish_as("owner", "1.0.1", "Open", None, false)
        .await
        .unwrap();
    assert_eq!(published, StatusCode::OK);
    redeemed.release();

    let (status, _, cookie) = reader.await.unwrap();
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(
        cookie.is_some_and(|cookie| cookie.starts_with("rv_report=")),
        "the spent ticket left no session"
    );
}

/// The app origin hands a signed-in reader one ticket per navigation, also when a
/// publication overlaps it: a person may hold only a few at a time.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn opening_a_report_during_a_publication_issues_one_ticket() {
    let host = host("ticket-issue").await;
    let app = crate::server::build_app(host.state.clone());
    let issued = hold("ticket.issued", &host.key);
    let reader = {
        let ask = Ask {
            cookie: Some("rv_session=session-owner".into()),
            ..Ask::get(format!("/r/{}", host.key))
        };
        tokio::spawn(async move { ask.send(&app).await })
    };
    reached(&issued).await;
    let published = host
        .publish_as("owner", "1.0.1", "Open", None, false)
        .await
        .unwrap();
    assert_eq!(published, StatusCode::OK);
    issued.release();

    let (status, body, _) = reader.await.unwrap();
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(body.contains("grant="), "{body}");
    assert_eq!(host.state.access.outstanding_tickets(), 1);
}

/// The reviewer's refusal: an organization admin who is not the report's owner publishes
/// 1.0.1 asking for `private`; after rendering and before the commit, the owner demotes them.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_publisher_demoted_after_rendering_leaves_no_version() {
    let host = host("demoted").await;
    let raw = |version: &str| Ask::get(format!("/raw/{}/v/{version}", host.key));
    let router = host.router_for(&raw("1.0.0")).await;

    let rendered = hold("publish.rendered", &host.key);
    let publish = host.publish("1.0.1");
    reached(&rendered).await;
    host.demote_admin();
    rendered.release();

    assert_ne!(publish.await.unwrap(), StatusCode::OK);
    assert!(!host.stored("1.0.1"));
    assert_eq!(raw("1.0.1").send(&router).await.0, StatusCode::NOT_FOUND);
    assert_eq!(host.visibility(), Visibility::Public);
    assert_eq!(raw("1.0.0").send(&router).await.0, StatusCode::OK);
}

/// Start an access change on a thread of its own and wait until it stands at the lock the
/// publication commit holds. A change that goes through instead fails the test here, before
/// the publication is released.
async fn change_that_must_wait(
    host: &Host,
    change: impl FnOnce(Access, Principal, String) + Send + 'static,
) -> std::thread::JoinHandle<()> {
    let (access, owner, key) = (host.state.access.clone(), host.owner(), host.key.clone());
    let changing = std::thread::spawn(move || change(access, owner, key));
    let patience = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while host.state.access.waiting_changes() == 0 {
        assert!(
            !changing.is_finished(),
            "the access change went through while the publication was committing"
        );
        assert!(
            std::time::Instant::now() < patience,
            "the change never started"
        );
        tokio::task::yield_now().await;
    }
    changing
}

/// The reviewer's second sequence: once the publication has closed the report and before it
/// stores the version, the owner makes the report public and demotes the publisher. Those
/// changes wait for the commit, so there is no refusal that leaves a version behind: the
/// publication is whole, and the owner's decisions follow it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn access_and_role_changes_wait_for_a_publication_that_has_closed_the_report() {
    let host = host("changes").await;
    let closed = hold("publish.closed", &host.key);
    let publish = host.publish("1.0.1");
    reached(&closed).await;

    let changes = change_that_must_wait(&host, |access, owner, key| {
        access
            .set_visibility_authorized(|| owner.clone(), &key, Visibility::Public)
            .unwrap();
        access
            .set_member_role(|| owner.clone(), ORG, "admin", OrgRole::Reader)
            .unwrap();
    })
    .await;
    // Inside the commit the report is private and 1.0.1 is not stored yet.
    assert_eq!(host.visibility(), Visibility::Private);
    assert!(!host.stored("1.0.1"));
    closed.release();

    let status = publish.await.unwrap();
    changes.join().unwrap();
    // A refusal would have to leave no version; here the publication simply came first.
    assert_eq!(status, StatusCode::OK);
    assert!(host.stored("1.0.1"));
    assert_eq!(host.visibility(), Visibility::Public);
    assert_eq!(
        host.state.access.role_in("admin", ORG),
        Some(OrgRole::Reader)
    );
}

/// The reviewer's third sequence: different bytes for the existing 1.0.0, asking for
/// `private`. After the report is closed the owner sets it private themselves; the store then
/// refuses the version. The owner's decision stands.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_refused_publication_does_not_undo_a_newer_privacy_decision() {
    let host = host("rollback").await;
    let closed = hold("publish.closed", &host.key);
    let publish = host.publish("1.0.0");
    reached(&closed).await;

    let decision = change_that_must_wait(&host, |access, owner, key| {
        access
            .replace_access_authorized(
                || owner.clone(),
                &key,
                None,
                Visibility::Private,
                vec![],
                &[],
            )
            .unwrap();
    })
    .await;
    closed.release();

    assert_eq!(publish.await.unwrap(), StatusCode::CONFLICT);
    decision.join().unwrap();
    assert_eq!(host.visibility(), Visibility::Private);
}

/// A publication the store refuses, with nobody else involved, leaves the report as it was.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_publication_the_store_refuses_leaves_the_visibility_as_it_was() {
    let host = host("refused").await;
    assert_eq!(host.publish("1.0.0").await.unwrap(), StatusCode::CONFLICT);
    assert_eq!(host.visibility(), Visibility::Public);
}

/// The accepted case: the visibility asked for is in force and the version is stored.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn publishing_private_to_a_public_report_closes_it_with_the_new_version() {
    let host = host("accepted").await;
    let raw = |version: &str| Ask::get(format!("/raw/{}/v/{version}", host.key));
    let router = host.router_for(&raw("1.0.0")).await;
    assert_eq!(host.publish("1.0.1").await.unwrap(), StatusCode::OK);
    assert_eq!(host.visibility(), Visibility::Private);
    for version in ["1.0.0", "1.0.1"] {
        assert_eq!(
            raw(version).send(&router).await.0,
            StatusCode::NOT_FOUND,
            "{version}"
        );
    }
}

/// The admin's regenerate of the coming 1.0.1 passes its check and is held; `lose` takes the
/// admin's credentials away without touching their membership; the owner then publishes the
/// confidential, private 1.0.1; the held request resumes. Returns what it answered.
async fn regenerate_after(slug: &'static str, lose: fn(&Host)) -> (StatusCode, String) {
    let host = host(slug).await;
    let ask = |version: &str| Ask {
        method: Method::POST,
        path: format!("/api/reports/{}/v/{version}/regenerate", host.key),
        token: Some("token-admin"),
        cookie: None,
        form: None,
    };
    let router = host.router_for(&ask("1.0.0")).await;
    let let_in = hold("authorized.manage", &host.key);
    let reader = {
        let (router, ask) = (router.clone(), ask("1.0.1"));
        tokio::spawn(async move { ask.send(&router).await })
    };
    reached(&let_in).await;
    lose(&host);
    // The membership is kept: by rights alone the admin could still manage the report.
    assert_eq!(
        host.state.access.role_in("admin", ORG),
        Some(OrgRole::Admin)
    );
    let published = host
        .publish_as("owner", "1.0.1", "Confidential", Some("private"), false)
        .await
        .unwrap();
    assert_eq!(published, StatusCode::OK, "{slug}");
    let_in.release();
    let (status, body, _) = reader.await.unwrap();
    (status, body)
}

/// The reviewer's sequence with the operator disabling the account.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_regenerate_in_flight_gets_nothing_once_its_account_is_disabled() {
    let (status, body) = regenerate_after("regenerate-disabled", |host| {
        host.state
            .access
            .operator_disable_person("admin", true)
            .unwrap();
    })
    .await;
    assert_ne!(status, StatusCode::OK, "{body}");
    assert!(!body.contains("Confidential"), "{body}");
}

/// The same with the bearer token revoked.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_regenerate_in_flight_gets_nothing_once_its_token_is_revoked() {
    let (status, body) = regenerate_after("regenerate-revoked", |host| {
        host.state
            .access
            .revoke_token_for_fixture("admin", "t-a")
            .unwrap();
    })
    .await;
    assert_ne!(status, StatusCode::OK, "{body}");
    assert!(!body.contains("Confidential"), "{body}");
}

/// A publication whose token is revoked while it renders stores nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_publisher_whose_token_is_revoked_after_rendering_leaves_no_version() {
    let host = host("revoked").await;
    let rendered = hold("publish.rendered", &host.key);
    let publish = host.publish("1.0.1");
    reached(&rendered).await;
    host.state
        .access
        .revoke_token_for_fixture("admin", "t-a")
        .unwrap();
    rendered.release();

    assert_ne!(publish.await.unwrap(), StatusCode::OK);
    assert!(!host.stored("1.0.1"));
    assert_eq!(host.visibility(), Visibility::Public);
}

/// A request that waits and is decided afterwards is decided on who the caller is then. The
/// admin's DELETE is held where it starts to wait for the publication lock; the operator
/// disables the account; the request goes on.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_delete_that_waited_deletes_nothing_once_its_account_is_disabled() {
    let host = host("delete-disabled").await;
    let app = crate::server::build_app(host.state.clone());
    let waiting = hold("delete.waiting", &host.key);
    let delete = {
        let ask = Ask {
            method: Method::DELETE,
            token: Some("token-admin"),
            ..Ask::get(format!("/api/reports/{}", host.key))
        };
        tokio::spawn(async move { ask.send(&app).await })
    };
    reached(&waiting).await;
    host.state
        .access
        .operator_disable_person("admin", true)
        .unwrap();
    waiting.release();

    let (status, body, _) = delete.await.unwrap();
    assert_ne!(status, StatusCode::OK, "{body}");
    assert!(host.stored("1.0.0"), "the report was deleted");
}

/// The owner's browser shares the report with an address; the form passes its check and is
/// held before the report is looked up; the owner signs out; the form goes on.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_share_that_waited_changes_nothing_once_its_session_has_ended() {
    let host = host("share-ended").await;
    let app = crate::server::build_app(host.state.clone());
    let checked = hold("authorized.share", &host.key);
    let share = {
        let ask = Ask::form(
            format!("/ui/reports/{}/people", host.key),
            "email=stranger%40example.test&role=editor",
        );
        tokio::spawn(async move { ask.send(&app).await })
    };
    reached(&checked).await;
    host.state
        .access
        .sign_out_for_fixture("session-owner")
        .unwrap();
    checked.release();

    let (status, body, _) = share.await.unwrap();
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let stored = std::fs::read_to_string(host._dir.path().join(".access/state.json")).unwrap();
    assert!(
        !stored.contains("stranger@example.test"),
        "the share was made"
    );
}

/// The same for removing a member: held where it starts to wait for the publication lock,
/// the owner signs out, and the admin stays a member.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_member_removal_that_waited_removes_nobody_once_its_session_has_ended() {
    let host = host("remove-ended").await;
    let app = crate::server::build_app(host.state.clone());
    let waiting = hold("remove.waiting", ORG);
    let removal = {
        let ask = Ask::form(format!("/ui/orgs/{ORG}/members/admin/remove"), "");
        tokio::spawn(async move { ask.send(&app).await })
    };
    reached(&waiting).await;
    host.state
        .access
        .sign_out_for_fixture("session-owner")
        .unwrap();
    waiting.release();

    let (status, body, _) = removal.await.unwrap();
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(
        host.state.access.role_in("admin", ORG),
        Some(OrgRole::Admin)
    );
}

/// The session ends after the form's last check and before its write takes the lock: the
/// write reads the session again under that lock and shares nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_share_whose_session_ends_just_before_the_write_changes_nothing() {
    let host = host("share-gate").await;
    let app = crate::server::build_app(host.state.clone());
    let stored = || std::fs::read_to_string(host._dir.path().join(".access/state.json")).unwrap();
    let before = host.state.access.acl(&host.key);
    let gating = hold("share.gating", &host.key);
    let share = {
        let ask = Ask::form(
            format!("/ui/reports/{}/people", host.key),
            "email=stranger%40example.test&role=editor",
        );
        tokio::spawn(async move { ask.send(&app).await })
    };
    reached(&gating).await;
    host.state
        .access
        .sign_out_for_fixture("session-owner")
        .unwrap();
    gating.release();

    let (status, body, _) = share.await.unwrap();
    assert_ne!(status, StatusCode::OK, "{body}");
    assert!(!body.contains("Shared with"), "{body}");
    assert!(
        !stored().contains("stranger@example.test"),
        "an invitation was stored"
    );
    assert!(
        !stored().contains("\"invitations\":[{"),
        "an invitation was stored"
    );
    let after = host.state.access.acl(&host.key);
    assert_eq!(after.grants.len(), before.grants.len());
    assert_eq!(after.visibility, before.visibility);
}

/// The same for taking a person off a report: the grant given before stays.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_unshare_whose_session_ends_just_before_the_write_changes_nothing() {
    let host = host("unshare-gate").await;
    host.state
        .access
        .set_acl_for_fixture(
            &host.key,
            Visibility::Public,
            vec![crate::access::Grant {
                user: "admin".into(),
                role: crate::access::ReportRole::Viewer,
            }],
        )
        .unwrap();
    let app = crate::server::build_app(host.state.clone());
    let gating = hold("unshare.gating", &host.key);
    let unshare = {
        let ask = Ask::form(
            format!("/ui/reports/{}/people/remove", host.key),
            "email=admin%40example.test",
        );
        tokio::spawn(async move { ask.send(&app).await })
    };
    reached(&gating).await;
    host.state
        .access
        .sign_out_for_fixture("session-owner")
        .unwrap();
    gating.release();

    let (status, body, _) = unshare.await.unwrap();
    assert_ne!(status, StatusCode::OK, "{body}");
    assert!(!body.contains("Removed"), "{body}");
    assert_eq!(host.state.access.acl(&host.key).grants.len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn access_replacement_rejects_a_token_revoked_before_its_write() {
    let host = host("replace-revoked").await;
    host.state
        .access
        .set_acl_for_fixture(&host.key, Visibility::Private, vec![])
        .unwrap();
    let app = crate::server::build_app(host.state.clone());
    let gating = hold("replace.gating", &host.key);
    let path = format!("/api/reports/{}/access", host.key);
    let request = Request::builder()
        .method(Method::PUT)
        .uri(path)
        .header(header::HOST, "app.test")
        .header(header::AUTHORIZATION, "Bearer token-admin")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"visibility":"public"}"#))
        .unwrap();
    let change = tokio::spawn(async move { app.oneshot(request).await.unwrap().status() });
    reached(&gating).await;
    host.state
        .access
        .revoke_token_for_fixture("admin", "t-a")
        .unwrap();
    gating.release();
    assert_ne!(change.await.unwrap(), StatusCode::OK);
    assert_eq!(host.visibility(), Visibility::Private);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn visibility_form_rejects_a_session_ended_before_its_write() {
    let host = host("visibility-ended").await;
    host.state
        .access
        .set_acl_for_fixture(&host.key, Visibility::Private, vec![])
        .unwrap();
    let app = crate::server::build_app(host.state.clone());
    let gating = hold("visibility.gating", &host.key);
    let ask = Ask::form(format!("/ui/share/{}", host.key), "visibility=public");
    let change = tokio::spawn(async move { ask.send(&app).await });
    reached(&gating).await;
    host.state
        .access
        .sign_out_for_fixture("session-owner")
        .unwrap();
    gating.release();
    let (_, body, _) = change.await.unwrap();
    assert!(!body.contains("The report is public"), "{body}");
    assert_eq!(host.visibility(), Visibility::Private);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn invitation_renewal_rejects_a_session_ended_before_its_write() {
    let host = host("renew-ended").await;
    let invitation_id = "i-renew";
    let app = crate::server::build_app(host.state.clone());
    let gating = hold("renew.gating", invitation_id);
    let ask = Ask::form(
        format!("/ui/orgs/{ORG}/invitations/{}/resend", invitation_id),
        "",
    );
    let renewal = tokio::spawn(async move { ask.send(&app).await });
    reached(&gating).await;
    host.state
        .access
        .sign_out_for_fixture("session-owner")
        .unwrap();
    let signed_out = std::fs::read(host._dir.path().join(".access/state.json")).unwrap();
    gating.release();
    let (_, body, _) = renewal.await.unwrap();
    assert!(!body.contains("again"), "{body}");
    assert_eq!(
        std::fs::read(host._dir.path().join(".access/state.json")).unwrap(),
        signed_out
    );
}
