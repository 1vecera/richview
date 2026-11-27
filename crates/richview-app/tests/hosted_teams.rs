//! Teams on hosted RichView: members, invitations, the three visibilities and sharing a report
//! with a person named by email. Every test drives the real routers over a real store; state is
//! seeded through the access file, and mail is read from the outbox directory.
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{HeaderMap, Request, StatusCode, header},
};
use richview_app::{
    access::{Access, OrgRole, Principal, ReportRole, Visibility},
    mail::Outbox,
    server::{
        AppState, build_app, build_report_app,
        hosted::{OperatorAction, operator_action},
    },
    storage::{FsStorage, StoredReport},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tower::ServiceExt;

const A: &str = "aaaa0001";
const B: &str = "bbbb0002";
const C: &str = "cccc0003";
const PASSWORD: &str = "a-long-password";

// ------------------------------------------------------------------------------ seeding

fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

fn email(person: &str) -> String {
    format!("{person}@example.test")
}

fn token_of(person: &str, org: &str) -> String {
    format!("token-{person}-{org}")
}

/// A fixed, increasing timestamp: membership order decides who is the longest-standing owner.
fn moment(order: u32) -> String {
    format!("2026-01-01T{:02}:{:02}:00.000Z", order / 60, order % 60)
}

/// The access file and the stored reports of a hosted server, built up call by call. Every person
/// added gets a session `session-<id>` with CSRF token `csrf-<id>`, so tests pay for no hashing.
#[derive(Default)]
struct Seed {
    orgs: Vec<Value>,
    users: Vec<Value>,
    memberships: Vec<Value>,
    org_creations: BTreeMap<String, usize>,
    sessions: Vec<Value>,
    acls: serde_json::Map<String, Value>,
    invitations: Vec<Value>,
    reports: Vec<String>,
    joined: u32,
}

impl Seed {
    fn org(mut self, id: &str, name: &str) -> Self {
        self.orgs
            .push(json!({"id":id,"name":name,"created":moment(0),"suspended":false,"limits":{}}));
        self
    }

    fn creations(mut self, person: &str, count: usize) -> Self {
        self.org_creations.insert(person.into(), count);
        self
    }

    fn person(mut self, id: &str, name: &str) -> Self {
        self.users.push(json!({"id":id,"name":name,"email":email(id),"created":moment(0),"verified":moment(0),"tokens":[]}));
        self.sessions.push(json!({"sha256":digest(&format!("session-{id}")),"csrf":format!("csrf-{id}"),"user":id,"expires":4_102_444_800u64,"scope":""}));
        self
    }

    fn member(mut self, person: &str, org: &str, role: &str) -> Self {
        self.joined += 1;
        let order = self.joined;
        self.member_at(person, org, role, order)
    }

    fn member_at(mut self, person: &str, org: &str, role: &str, order: u32) -> Self {
        self.memberships
            .push(json!({"person":person,"org":org,"role":role,"created":moment(order)}));
        self
    }

    /// A CLI token for `org`, `token-<person>-<org>`.
    fn token(mut self, person: &str, org: &str) -> Self {
        let user = self
            .users
            .iter_mut()
            .find(|u| u["id"] == person)
            .expect("token for a person that was not added");
        user["tokens"].as_array_mut().unwrap().push(json!({
            "id": format!("t-{org}"), "name": "CLI", "created": moment(0),
            "sha256": digest(&token_of(person, org)), "org": org
        }));
        self
    }

    /// A stored report with an access record: `key` is `<org>/<slug>`.
    fn report(mut self, key: &str, owner: &str, visibility: &str) -> Self {
        let org = key.split_once('/').unwrap().0;
        self.acls.insert(
            key.to_owned(),
            json!({"org":org,"owner":owner,"visibility":visibility,"grants":[]}),
        );
        self.reports.push(key.to_owned());
        self
    }

    fn grant(mut self, key: &str, person: &str, role: &str) -> Self {
        self.acls[key]["grants"]
            .as_array_mut()
            .unwrap()
            .push(json!({"user":person,"role":role}));
        self
    }

    fn invitation(mut self, invitation: Value) -> Self {
        self.invitations.push(invitation);
        self
    }

    /// Write the access file and the reports into `data`.
    fn persist(&self, data: &Path) {
        fs::create_dir_all(data.join(".access")).unwrap();
        let state = json!({"schema":2,"tenancy":"hosted","organizations":self.orgs,
            "memberships":self.memberships,"org_creations":self.org_creations,"users":self.users,"sessions":self.sessions,
            "acls":self.acls,"invitations":self.invitations});
        fs::write(
            data.join(".access/state.json"),
            serde_json::to_vec(&state).unwrap(),
        )
        .unwrap();
        let storage = FsStorage::new(data).unwrap().with_hosted(true);
        for key in &self.reports {
            let slug = key.split_once('/').unwrap().1;
            storage
                .put_version(StoredReport {
                    slug: key.clone(),
                    version: "1.0.0".into(),
                    source: format!("# Report {slug}"),
                    report_folder: None,
                    html: format!(
                        "<!doctype html><html><head><title>{slug}</title></head><body><h1>Report {slug}</h1></body></html>"
                    ),
                    frontmatter: json!({"title": format!("Report {slug}")}),
                    created_at: "2026-11-11T10:00:00Z".into(),
                    styles: vec![],
                    data: BTreeMap::new(),
                    files: BTreeMap::new(),
                    content_sha256: String::new(),
                })
                .unwrap();
        }
    }

    fn start(self, mail: Mail) -> Host {
        let dir = scratch();
        self.persist(dir.path());
        let data = dir.path().to_path_buf();
        Host::open(dir, data, mail)
    }
}

fn scratch() -> tempfile::TempDir {
    let tmp = std::env::current_dir().unwrap().join("tmp");
    fs::create_dir_all(&tmp).unwrap();
    tempfile::Builder::new()
        .prefix("hosted-teams-")
        .tempdir_in(tmp)
        .unwrap()
}

/// Organization A with its four roles and one person of organization B.
fn team() -> Seed {
    Seed::default()
        .org(A, "Acme")
        .org(B, "Bravo Co")
        .person("owner", "Olivia Owner")
        .person("admin", "Adam Admin")
        .person("author", "Aria Author")
        .person("reader", "Rita Reader")
        .person("outsider", "Oscar Outsider")
        .member("owner", A, "owner")
        .member("admin", A, "admin")
        .member("author", A, "author")
        .member("reader", A, "reader")
        .member("outsider", B, "owner")
}

/// The team plus a report of the author's and a person of B who has an account.
fn sharing() -> Seed {
    team()
        .person("known", "Kim Known")
        .member("known", B, "author")
        .token("author", A)
        .token("owner", A)
        .report("aaaa0001/q3", "author", "private")
        .report("aaaa0001/other", "author", "private")
        .report("aaaa0001/orgwide", "owner", "org")
}

// ------------------------------------------------------------------------------ the host

#[derive(Clone, Copy)]
enum Mail {
    /// Plain files with the links in them.
    Files,
    /// Requests the operator turns into mail by hand.
    Manual,
}

struct Host {
    access: Access,
    state: AppState,
    app: Router,
    reports: Router,
    data: PathBuf,
    // Last, so the directory outlives everything that reads it.
    dir: tempfile::TempDir,
}

/// A browser: the session cookie and the CSRF token that go with it.
#[derive(Clone, Default)]
struct Login {
    cookie: String,
    csrf: String,
}

/// A person with a seeded session.
fn me(person: &str) -> Login {
    Login {
        cookie: format!("session-{person}"),
        csrf: format!("csrf-{person}"),
    }
}

fn nobody() -> Login {
    Login::default()
}

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: String,
}

impl Reply {
    fn location(&self) -> String {
        self.headers
            .get(header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned()
    }

    /// The notice a redirect carries in `?m=`, decoded.
    fn notice(&self) -> String {
        let location = self.location();
        let Some((_, query)) = location.split_once('?') else {
            return String::new();
        };
        query
            .split('&')
            .find_map(|pair| pair.strip_prefix("m="))
            .map(decode)
            .unwrap_or_default()
    }

    fn cookie(&self) -> Option<String> {
        self.headers
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .find_map(|v| v.strip_prefix("rv_session="))
            .map(|v| v.split(';').next().unwrap_or_default().to_owned())
    }

    fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or_else(|_| panic!("not JSON: {}", self.body))
    }
}

fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.') {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (bytes[i], hex) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The text between two markers; panics when either is missing, which is the failure to show.
fn between<'a>(text: &'a str, start: &str, end: &str) -> &'a str {
    let from = text
        .find(start)
        .unwrap_or_else(|| panic!("`{start}` not found"))
        + start.len();
    let to = from
        + text[from..]
            .find(end)
            .unwrap_or_else(|| panic!("`{end}` not found after `{start}`"));
    &text[from..to]
}

async fn call(router: &Router, request: Request<Body>) -> Reply {
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    Reply {
        status,
        headers,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

struct Letter {
    to: String,
    subject: String,
    body: String,
}

impl Letter {
    /// The path of the one link the mail carries.
    fn link(&self) -> String {
        assert_eq!(
            self.body.matches("https://").count() + self.body.matches("http://").count(),
            1,
            "a mail carries exactly one link: {}",
            self.body
        );
        let link = self
            .body
            .lines()
            .find(|line| line.starts_with("https://app.test/link/"))
            .unwrap_or_else(|| panic!("no app link in {}", self.body));
        reqwest::Url::parse(link).unwrap().path().to_owned()
    }
}

fn form_path(link: &str) -> String {
    link.replacen("/link/", "/ui/link/", 1)
}

impl Host {
    fn open(dir: tempfile::TempDir, data: PathBuf, mail: Mail) -> Self {
        let access = Access::open_hosted(&data).unwrap();
        access
            .configure_mail(
                Arc::new(Outbox::open(&dir.path().join("outbox")).unwrap()),
                1000,
                matches!(mail, Mail::Manual),
            )
            .unwrap();
        let storage = FsStorage::new(&data)
            .unwrap()
            .with_hosted(true)
            .with_quota(Some(100_000_000));
        let state = AppState {
            storage,
            access: access.clone(),
            preview: None,
            public_origin: Some("https://app.test".into()),
            report_origin: Some("https://reports.test".into()),
        };
        Self {
            access,
            app: build_app(state.clone()),
            reports: build_report_app(state.clone()),
            state,
            data,
            dir,
        }
    }

    fn outbox(&self) -> PathBuf {
        self.dir.path().join("outbox")
    }

    async fn get(&self, who: &Login, path: &str) -> Reply {
        let mut request = Request::builder()
            .uri(path)
            .header(header::HOST, "app.test");
        if !who.cookie.is_empty() {
            request = request.header(header::COOKIE, format!("rv_session={}", who.cookie));
        }
        call(&self.app, request.body(Body::empty()).unwrap()).await
    }

    /// A form post: the CSRF token of `who` goes in first, then the fields.
    async fn post(&self, who: &Login, path: &str, fields: &[(&str, &str)]) -> Reply {
        let mut pairs = Vec::new();
        if !who.csrf.is_empty() {
            pairs.push(format!("csrf={}", encode(&who.csrf)));
        }
        pairs.extend(fields.iter().map(|(k, v)| format!("{k}={}", encode(v))));
        let mut request = Request::builder()
            .method("POST")
            .uri(path)
            .header(header::HOST, "app.test")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
        if !who.cookie.is_empty() {
            request = request.header(header::COOKIE, format!("rv_session={}", who.cookie));
        }
        call(
            &self.app,
            request.body(Body::from(pairs.join("&"))).unwrap(),
        )
        .await
    }

    async fn api(&self, token: &str, method: &str, path: &str, body: Option<Value>) -> Reply {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, "app.test")
            .header(header::AUTHORIZATION, format!("Bearer {token}"));
        let body = match body {
            Some(body) => {
                request = request.header(header::CONTENT_TYPE, "application/json");
                Body::from(body.to_string())
            }
            None => Body::empty(),
        };
        call(&self.app, request.body(body).unwrap()).await
    }

    /// A request to the report listener with no sign-in at all.
    async fn report_get(&self, path: &str) -> Reply {
        let request = Request::builder()
            .uri(path)
            .header(header::HOST, "reports.test")
            .body(Body::empty())
            .unwrap();
        call(&self.reports, request).await
    }

    /// The browser a reply signed in.
    fn login_from(&self, reply: &Reply) -> Login {
        let cookie = reply.cookie().expect("the reply set no session cookie");
        let csrf = self
            .access
            .csrf_for(&cookie)
            .expect("the session is not in the store");
        Login { cookie, csrf }
    }

    fn state_json(&self) -> Value {
        serde_json::from_slice(&fs::read(self.data.join(".access/state.json")).unwrap()).unwrap()
    }

    fn invitations(&self) -> Vec<Value> {
        self.state_json()["invitations"].as_array().unwrap().clone()
    }

    fn id_of(&self, address: &str) -> String {
        self.access
            .users()
            .into_iter()
            .find(|u| u.email == address)
            .unwrap_or_else(|| panic!("no account for {address}"))
            .id
    }

    fn has_account(&self, address: &str) -> bool {
        self.access.users().iter().any(|u| u.email == address)
    }

    /// Mail written for `address`, oldest first.
    fn letters(&self, address: &str) -> Vec<Letter> {
        let mut found = Vec::new();
        for entry in fs::read_dir(self.outbox()).unwrap().flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "txt") {
                continue;
            }
            let text = fs::read_to_string(&path).unwrap();
            let (head, body) = text.split_once("\n\n").unwrap();
            let to = head.lines().next().unwrap().strip_prefix("To: ").unwrap();
            let subject = head
                .lines()
                .nth(1)
                .unwrap()
                .strip_prefix("Subject: ")
                .unwrap();
            if to == address {
                let at = entry.metadata().unwrap().modified().unwrap();
                found.push((
                    at,
                    Letter {
                        to: to.into(),
                        subject: subject.into(),
                        body: body.into(),
                    },
                ));
            }
        }
        found.sort_by_key(|(at, _)| *at);
        found.into_iter().map(|(_, letter)| letter).collect()
    }

    /// Wait for the asynchronous queue to write at least `count` mails for `address`.
    async fn wait_letters(&self, address: &str, count: usize) -> Vec<Letter> {
        let mut waited = 0;
        loop {
            let letters = self.letters(address);
            if letters.len() >= count {
                return letters;
            }
            assert!(
                waited < 500,
                "only {} of {count} mails for {address}",
                letters.len()
            );
            waited += 1;
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Invite a throw-away address and wait for its mail: the queue is in order, so anything
    /// queued before it has been written by the time this returns.
    async fn drain_mail(&self, by: &Login, org: &str) {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let address = format!("drain{n}@example.test");
        let drain = self
            .post(
                by,
                &format!("/ui/orgs/{org}/invitations"),
                &[("email", &address), ("role", "reader")],
            )
            .await;
        assert_eq!(drain.status, StatusCode::SEE_OTHER);
        self.wait_letters(&address, 1).await;
    }
}

// ------------------------------------------------------------------------------ members

#[tokio::test]
async fn an_owner_invites_a_new_person_who_joins_from_the_mail_link_and_is_signed_in() {
    let host = team().start(Mail::Files);
    let invited = host
        .post(
            &me("owner"),
            &format!("/ui/orgs/{A}/invitations"),
            &[("email", "nina@example.test"), ("role", "author")],
        )
        .await;
    assert_eq!(invited.status, StatusCode::SEE_OTHER);
    assert!(invited.location().starts_with("/members?m="));
    assert!(
        invited.notice().contains("Invited nina@example.test."),
        "{}",
        invited.notice()
    );

    let letters = host.wait_letters("nina@example.test", 1).await;
    assert_eq!(letters.len(), 1);
    let mail = &letters[0];
    assert_eq!(mail.subject, "Olivia Owner invited you to Acme on RichView");
    assert!(mail.body.contains(
        "Olivia Owner (owner@example.test) invited you to join Acme as an author, who can publish reports."
    ));
    let link = mail.link();

    // Looking at the link spends nothing, however often a mail scanner does it.
    for _ in 0..2 {
        let page = host.get(&nobody(), &link).await;
        assert_eq!(page.status, StatusCode::OK);
        assert!(page.body.contains("Join Acme"));
        assert!(page.body.contains("Create account"));
    }
    assert_eq!(host.invitations().len(), 1);
    let links: Value =
        serde_json::from_slice(&fs::read(host.data.join(".access/links.json")).unwrap()).unwrap();
    assert_eq!(links.as_array().unwrap().len(), 1);
    assert_eq!(links[0]["used"], false);

    let rejected = host
        .post(
            &nobody(),
            &form_path(&link),
            &[
                ("name", "Nina <Newbie>"),
                ("password", PASSWORD),
                ("password_confirm", "different password"),
            ],
        )
        .await;
    assert_eq!(rejected.status, StatusCode::BAD_REQUEST);
    assert!(
        rejected
            .body
            .contains("The two passwords did not match. Enter them again.")
    );
    assert!(rejected.body.contains("value=\"Nina &lt;Newbie&gt;\""));
    assert_eq!(host.get(&nobody(), &link).await.status, StatusCode::OK);
    let invalid = host
        .post(
            &nobody(),
            &form_path(&link),
            &[
                ("name", "Nina <Newbie>"),
                ("password", "short"),
                ("password_confirm", "short"),
            ],
        )
        .await;
    assert_eq!(invalid.status, StatusCode::BAD_REQUEST);
    assert!(
        invalid
            .body
            .contains("A password needs at least 12 characters.")
    );
    assert!(invalid.body.contains("value=\"Nina &lt;Newbie&gt;\""));
    assert_eq!(host.get(&nobody(), &link).await.status, StatusCode::OK);

    let joined = host
        .post(
            &nobody(),
            &form_path(&link),
            &[
                ("name", "Nina Newbie"),
                ("password", PASSWORD),
                ("password_confirm", PASSWORD),
            ],
        )
        .await;
    assert_eq!(joined.status, StatusCode::SEE_OTHER);
    assert_eq!(joined.location(), "/");
    let nina = host.login_from(&joined);
    let id = host.id_of("nina@example.test");
    assert_eq!(host.access.role_in(&id, A), Some(OrgRole::Author));
    let members = host.get(&nina, "/members").await;
    assert_eq!(members.status, StatusCode::OK);
    assert!(
        members
            .body
            .contains("Nina Newbie</strong> <span class=\"tag\">You</span>")
    );
    assert!(
        !host
            .get(&nina, "/")
            .await
            .body
            .contains("Create organization")
    );

    assert_eq!(host.get(&nobody(), &link).await.status, StatusCode::GONE);
    assert!(host.invitations().is_empty());
}

#[tokio::test]
async fn an_existing_account_accepting_an_invitation_link_joins_but_is_not_signed_in() {
    let host = team()
        .person("eve", "Eve Existing")
        .member("eve", B, "reader")
        .start(Mail::Files);
    host.post(
        &me("owner"),
        &format!("/ui/orgs/{A}/invitations"),
        &[("email", "eve@example.test"), ("role", "reader")],
    )
    .await;
    let letters = host.wait_letters("eve@example.test", 1).await;
    let link = letters[0].link();
    let sessions_before = host.state_json()["sessions"].as_array().unwrap().len();

    let page = host.get(&nobody(), &link).await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(page.body.contains("Accept and sign in"));
    assert!(!page.body.contains("Create account"));

    let accepted = host.post(&nobody(), &form_path(&link), &[]).await;
    assert_eq!(accepted.status, StatusCode::SEE_OTHER);
    assert!(
        accepted.headers.get(header::SET_COOKIE).is_none(),
        "a mail link must never open a session on an existing account"
    );
    assert!(accepted.location().starts_with("/?m=Accepted"));
    assert_eq!(host.access.role_in("eve", A), Some(OrgRole::Reader));
    assert_eq!(
        host.state_json()["sessions"].as_array().unwrap().len(),
        sessions_before
    );
    assert_eq!(host.get(&nobody(), &link).await.status, StatusCode::GONE);
}

#[tokio::test]
async fn an_invitee_who_is_signed_in_joins_from_the_reports_page_and_nobody_else_can_use_the_invitation()
 {
    let host = team().person("eve", "Eve Existing").start(Mail::Files);
    host.post(
        &me("owner"),
        &format!("/ui/orgs/{A}/invitations"),
        &[("email", "eve@example.test"), ("role", "author")],
    )
    .await;

    let list = host.get(&me("eve"), "/").await;
    assert_eq!(list.status, StatusCode::OK);
    assert!(list.body.contains("Olivia Owner invited you to join"));
    let id = between(&list.body, "/ui/invitations/", "/accept").to_owned();

    // Somebody else, signed in, holding the identifier, gets what a missing invitation gets.
    let stolen = host
        .post(
            &me("outsider"),
            &format!("/ui/invitations/{id}/accept"),
            &[],
        )
        .await;
    assert_eq!(stolen.status, StatusCode::NOT_FOUND);
    assert_eq!(host.access.role_in("outsider", A), None);
    assert_eq!(host.invitations().len(), 1);

    let accepted = host
        .post(&me("eve"), &format!("/ui/invitations/{id}/accept"), &[])
        .await;
    assert_eq!(accepted.status, StatusCode::SEE_OTHER);
    assert_eq!(accepted.notice(), "You joined Acme.");
    assert_eq!(host.access.role_in("eve", A), Some(OrgRole::Author));
    assert!(host.invitations().is_empty());
    assert!(
        host.get(&me("eve"), "/")
            .await
            .body
            .contains("app-org__name\"><strong>Acme")
    );
}

fn assert_refused(reply: &Reply, fragment: &str) {
    assert_eq!(reply.status, StatusCode::SEE_OTHER, "{}", reply.body);
    assert!(
        reply.location().starts_with("/members?m="),
        "{}",
        reply.location()
    );
    assert!(
        reply.notice().contains(fragment),
        "expected `{fragment}` in `{}`",
        reply.notice()
    );
}

#[tokio::test]
async fn a_reader_and_an_author_cannot_invite_and_an_admin_cannot_invite_an_owner_or_touch_one() {
    let host = team()
        .person("owner2", "Owen Second")
        .member("owner2", A, "owner")
        .start(Mail::Files);
    let invite = |who: &str, role: &'static str| {
        let who = me(who);
        let host = &host;
        async move {
            host.post(
                &who,
                &format!("/ui/orgs/{A}/invitations"),
                &[("email", "boss@example.test"), ("role", role)],
            )
            .await
        }
    };

    for who in ["reader", "author"] {
        assert_refused(&invite(who, "reader").await, "Only an owner or admin");
        let role = host
            .post(
                &me(who),
                &format!("/ui/orgs/{A}/members/reader/role"),
                &[("role", "admin")],
            )
            .await;
        assert_refused(&role, "Only an owner or admin");
    }
    assert!(host.invitations().is_empty());
    assert_eq!(host.access.role_in("reader", A), Some(OrgRole::Reader));

    // An admin goes up to admin and no further, and leaves owners alone.
    assert_refused(&invite("admin", "owner").await, "above your own");
    let demote_owner = host
        .post(
            &me("admin"),
            &format!("/ui/orgs/{A}/members/owner2/role"),
            &[("role", "reader")],
        )
        .await;
    assert_refused(&demote_owner, "above your own");
    let promote = host
        .post(
            &me("admin"),
            &format!("/ui/orgs/{A}/members/author/role"),
            &[("role", "owner")],
        )
        .await;
    assert_refused(&promote, "above your own");
    let remove_owner = host
        .post(
            &me("admin"),
            &format!("/ui/orgs/{A}/members/owner2/remove"),
            &[],
        )
        .await;
    assert_refused(&remove_owner, "never one above");
    let rename = host
        .post(
            &me("admin"),
            &format!("/ui/orgs/{A}/name"),
            &[("name", "Mine now")],
        )
        .await;
    assert_refused(&rename, "Only an owner can rename");
    assert!(host.invitations().is_empty());
    assert_eq!(host.access.role_in("owner2", A), Some(OrgRole::Owner));
    assert_eq!(host.access.role_in("author", A), Some(OrgRole::Author));
    assert_eq!(host.access.organization(A).unwrap().name, "Acme");

    // What an admin may do still works.
    let admin_invite = invite("admin", "admin").await;
    assert_eq!(admin_invite.status, StatusCode::SEE_OTHER);
    assert!(
        admin_invite
            .notice()
            .starts_with("Invited boss@example.test.")
    );
    assert_eq!(host.invitations().len(), 1);

    // An owner may do all of it.
    let owner_invite = host
        .post(
            &me("owner"),
            &format!("/ui/orgs/{A}/invitations"),
            &[("email", "chief@example.test"), ("role", "owner")],
        )
        .await;
    assert!(
        owner_invite
            .notice()
            .starts_with("Invited chief@example.test.")
    );
    assert!(
        host.invitations()
            .iter()
            .any(|i| i["email"] == "chief@example.test" && i["role"] == "owner")
    );
    let demote = host
        .post(
            &me("owner"),
            &format!("/ui/orgs/{A}/members/owner2/role"),
            &[("role", "admin")],
        )
        .await;
    assert_eq!(demote.notice(), "Role saved.");
    assert_eq!(host.access.role_in("owner2", A), Some(OrgRole::Admin));
    let remove = host
        .post(
            &me("owner"),
            &format!("/ui/orgs/{A}/members/admin/remove"),
            &[],
        )
        .await;
    assert_eq!(remove.notice(), "Member removed.");
    assert_eq!(host.access.role_in("admin", A), None);
    let renamed = host
        .post(
            &me("owner"),
            &format!("/ui/orgs/{A}/name"),
            &[("name", "Acme Group")],
        )
        .await;
    assert_eq!(renamed.notice(), "Name saved.");
}

#[tokio::test]
async fn a_person_from_another_organization_gets_the_same_404_as_for_an_organization_that_does_not_exist()
 {
    let host = team().start(Mail::Files);
    host.post(
        &me("owner"),
        &format!("/ui/orgs/{A}/invitations"),
        &[("email", "open@example.test"), ("role", "reader")],
    )
    .await;
    let id = host.invitations()[0]["id"].as_str().unwrap().to_owned();
    let before = {
        let state = host.state_json();
        (
            state["memberships"].clone(),
            state["invitations"].clone(),
            state["organizations"].clone(),
        )
    };

    type Route<'a> = (&'a str, String, Vec<(&'a str, &'a str)>);
    let routes: Vec<Route> = vec![
        (
            "invite",
            "/ui/orgs/ORG/invitations".into(),
            vec![("email", "x@example.test"), ("role", "reader")],
        ),
        (
            "resend",
            format!("/ui/orgs/ORG/invitations/{id}/resend"),
            vec![],
        ),
        (
            "cancel",
            format!("/ui/orgs/ORG/invitations/{id}/cancel"),
            vec![],
        ),
        (
            "role",
            "/ui/orgs/ORG/members/reader/role".into(),
            vec![("role", "owner")],
        ),
        (
            "remove",
            "/ui/orgs/ORG/members/reader/remove".into(),
            vec![],
        ),
        (
            "rename",
            "/ui/orgs/ORG/name".into(),
            vec![("name", "Hijacked")],
        ),
    ];
    for (name, path, fields) in routes {
        let real = host
            .post(&me("outsider"), &path.replace("ORG", A), &fields)
            .await;
        let ghost = host
            .post(&me("outsider"), &path.replace("ORG", "zzzz9999"), &fields)
            .await;
        assert_eq!(real.status, StatusCode::NOT_FOUND, "{name}");
        assert_eq!(
            (real.status, real.location(), real.body),
            (ghost.status, ghost.location(), ghost.body),
            "{name} tells a real organization from a missing one"
        );
    }
    let after = {
        let state = host.state_json();
        (
            state["memberships"].clone(),
            state["invitations"].clone(),
            state["organizations"].clone(),
        )
    };
    assert_eq!(before, after);

    // The page of members shows the outsider only their own organization.
    let page = host.get(&me("outsider"), "/members").await;
    assert!(page.body.contains("Bravo Co"));
    assert!(!page.body.contains("Olivia Owner"));
    assert!(!page.body.contains("open@example.test"));
}

#[tokio::test]
async fn the_last_owner_cannot_be_demoted_removed_or_leave_until_a_second_owner_exists() {
    let host = team()
        .report("aaaa0001/q3", "owner", "private")
        .start(Mail::Files);
    let demote = host
        .post(
            &me("owner"),
            &format!("/ui/orgs/{A}/members/owner/role"),
            &[("role", "admin")],
        )
        .await;
    assert_refused(&demote, "needs an owner");
    let leave = host
        .post(
            &me("owner"),
            &format!("/ui/orgs/{A}/members/owner/remove"),
            &[],
        )
        .await;
    assert_refused(&leave, "needs an owner");
    assert_eq!(host.access.role_in("owner", A), Some(OrgRole::Owner));

    let promote = host
        .post(
            &me("owner"),
            &format!("/ui/orgs/{A}/members/admin/role"),
            &[("role", "owner")],
        )
        .await;
    assert_eq!(promote.notice(), "Role saved.");
    let left = host
        .post(
            &me("owner"),
            &format!("/ui/orgs/{A}/members/owner/remove"),
            &[],
        )
        .await;
    assert_eq!(left.status, StatusCode::SEE_OTHER);
    assert!(left.location().starts_with("/?m="));
    assert_eq!(left.notice(), "You left the organization.");
    assert_eq!(host.access.role_in("owner", A), None);
    assert_eq!(host.access.acl("aaaa0001/q3").owner, "admin");
    assert!(
        host.get(&me("owner"), "/")
            .await
            .body
            .contains("Create organization")
    );

    // The one who is left is the last owner now.
    let again = host
        .post(
            &me("admin"),
            &format!("/ui/orgs/{A}/members/admin/remove"),
            &[],
        )
        .await;
    assert_refused(&again, "needs an owner");
    assert_eq!(host.access.role_in("admin", A), Some(OrgRole::Owner));
}

#[tokio::test]
async fn removing_a_member_ends_their_tokens_and_grants_and_hands_their_reports_to_the_remover() {
    let host = team()
        .person("mia", "Mia Member")
        .member("mia", A, "author")
        .member("mia", B, "author")
        .token("mia", A)
        .token("mia", B)
        .report("aaaa0001/secret", "owner", "private")
        .grant("aaaa0001/secret", "mia", "viewer")
        .report("aaaa0001/shared", "owner", "org")
        .report("aaaa0001/mias", "mia", "private")
        .report("bbbb0002/bshared", "outsider", "org")
        .start(Mail::Files);
    let token = token_of("mia", A);
    for slug in ["secret", "shared"] {
        let before = host
            .api(
                &token,
                "GET",
                &format!("/api/reports/aaaa0001/{slug}"),
                None,
            )
            .await;
        assert_eq!(before.status, StatusCode::OK, "{slug} before");
    }

    // An admin removes her, so her reports pass to the admin, not to the longest-standing owner.
    let removed = host
        .post(
            &me("admin"),
            &format!("/ui/orgs/{A}/members/mia/remove"),
            &[],
        )
        .await;
    assert_eq!(removed.notice(), "Member removed.");

    for slug in ["secret", "shared"] {
        let after = host
            .api(
                &token,
                "GET",
                &format!("/api/reports/aaaa0001/{slug}"),
                None,
            )
            .await;
        assert_eq!(after.status, StatusCode::NOT_FOUND, "{slug} after");
    }
    assert!(matches!(
        host.access.authenticate(Some(&token), None),
        Principal::Anonymous
    ));
    assert_eq!(
        host.api(&token, "GET", "/api/reports", None).await.json(),
        json!([])
    );
    assert!(host.access.tokens_for_org("mia", A).is_empty());
    // Her token for the other organization is a different token and keeps working.
    assert_eq!(host.access.tokens_for_org("mia", B).len(), 1);
    assert_eq!(
        host.api(
            &token_of("mia", B),
            "GET",
            "/api/reports/bbbb0002/bshared",
            None
        )
        .await
        .status,
        StatusCode::OK
    );

    assert!(
        host.access
            .acl("aaaa0001/secret")
            .grants
            .iter()
            .all(|g| g.user != "mia")
    );
    assert_eq!(host.access.acl("aaaa0001/mias").owner, "admin");
}

async fn invite_as(host: &Host, who: &str, address: &str, role: &str) -> Letter {
    let reply = host
        .post(
            &me(who),
            &format!("/ui/orgs/{A}/invitations"),
            &[("email", address), ("role", role)],
        )
        .await;
    assert!(reply.notice().starts_with("Invited"), "{}", reply.notice());
    host.wait_letters(address, 1).await.remove(0)
}

fn assert_gone(reply: &Reply) {
    assert_eq!(reply.status, StatusCode::GONE, "{}", reply.body);
}

#[tokio::test]
async fn an_invitation_dies_when_its_sender_is_demoted_below_what_it_offers() {
    let host = team().start(Mail::Files);
    let letter = invite_as(&host, "admin", "late@example.test", "admin").await;
    let link = letter.link();
    assert_eq!(host.get(&nobody(), &link).await.status, StatusCode::OK);

    let demoted = host
        .post(
            &me("owner"),
            &format!("/ui/orgs/{A}/members/admin/role"),
            &[("role", "reader")],
        )
        .await;
    assert_eq!(demoted.notice(), "Role saved.");

    assert_gone(&host.get(&nobody(), &link).await);
    let post = host
        .post(
            &nobody(),
            &form_path(&link),
            &[
                ("name", "Late Comer"),
                ("password", PASSWORD),
                ("password_confirm", PASSWORD),
            ],
        )
        .await;
    assert_gone(&post);
    assert!(!host.has_account("late@example.test"));
}

#[tokio::test]
async fn an_invitation_dies_when_its_sender_is_removed() {
    let host = team().start(Mail::Files);
    let letter = invite_as(&host, "admin", "late@example.test", "author").await;
    let link = letter.link();
    host.post(
        &me("owner"),
        &format!("/ui/orgs/{A}/members/admin/remove"),
        &[],
    )
    .await;

    assert_gone(&host.get(&nobody(), &link).await);
    let post = host
        .post(
            &nobody(),
            &form_path(&link),
            &[
                ("name", "Late"),
                ("password", PASSWORD),
                ("password_confirm", PASSWORD),
            ],
        )
        .await;
    assert_gone(&post);
    assert!(!host.has_account("late@example.test"));
    assert!(host.invitations().is_empty());
    assert!(
        !host
            .get(&me("owner"), "/members")
            .await
            .body
            .contains("late@example.test")
    );
}

#[tokio::test]
async fn an_owner_invitation_dies_when_its_sender_is_demoted_to_admin() {
    let host = team()
        .person("owner2", "Owen Second")
        .member("owner2", A, "owner")
        .start(Mail::Files);
    let letter = invite_as(&host, "owner", "late@example.test", "owner").await;
    let link = letter.link();
    assert_eq!(host.get(&nobody(), &link).await.status, StatusCode::OK);
    host.post(
        &me("owner2"),
        &format!("/ui/orgs/{A}/members/owner/role"),
        &[("role", "admin")],
    )
    .await;
    assert_eq!(host.access.role_in("owner", A), Some(OrgRole::Admin));
    assert_gone(&host.get(&nobody(), &link).await);
    assert!(!host.has_account("late@example.test"));
}

#[tokio::test]
async fn a_person_in_two_organizations_switches_between_them_and_tokens_follow_the_choice() {
    let host = Seed::default()
        .org(A, "Acme")
        .org(B, "Bravo Co")
        .org(C, "Cobalt")
        .person("dual", "Dana Dual")
        .person("owner", "Olivia Owner")
        .person("bowner", "Bruno Owner")
        .person("cowner", "Carla Owner")
        .member("dual", A, "reader")
        .member("dual", B, "author")
        .member("owner", A, "owner")
        .member("bowner", B, "owner")
        .member("cowner", C, "owner")
        .report("aaaa0001/areport", "owner", "org")
        .report("bbbb0002/breport", "bowner", "org")
        .start(Mail::Files);
    let dual = me("dual");

    // The first organization joined is the one a fresh browser works in.
    let first = host.get(&dual, "/").await;
    assert!(first.body.contains("href=\"/reports/aaaa0001/areport\""));
    assert!(!first.body.contains("breport"));
    assert!(
        first
            .body
            .contains("value=\"aaaa0001\" aria-current=\"true\"")
    );

    let switched = host.post(&dual, "/ui/organization", &[("org", B)]).await;
    assert_eq!(switched.status, StatusCode::SEE_OTHER);
    assert_eq!(switched.location(), "/");
    let second = host.get(&dual, "/").await;
    assert!(second.body.contains("href=\"/reports/bbbb0002/breport\""));
    assert!(!second.body.contains("areport"));
    assert!(
        second
            .body
            .contains("value=\"bbbb0002\" aria-current=\"true\"")
    );

    // A token made now is for the organization in view, and the page lists only that one's.
    let made = host.post(&dual, "/ui/tokens", &[("name", "laptop")]).await;
    assert_eq!(made.status, StatusCode::OK);
    assert_eq!(host.access.tokens_for_org("dual", B).len(), 1);
    assert_eq!(host.access.tokens_for_org("dual", A).len(), 0);
    assert!(
        host.get(&dual, "/tokens")
            .await
            .body
            .contains("<strong>laptop</strong>")
    );

    // An organization they are not in cannot be selected, and looks like one that is not there.
    let foreign = host.post(&dual, "/ui/organization", &[("org", C)]).await;
    let ghost = host
        .post(&dual, "/ui/organization", &[("org", "zzzz9999")])
        .await;
    assert_eq!(foreign.status, StatusCode::NOT_FOUND);
    assert_eq!((foreign.status, foreign.body), (ghost.status, ghost.body));
    assert!(
        host.get(&dual, "/")
            .await
            .body
            .contains("value=\"bbbb0002\" aria-current=\"true\"")
    );

    host.post(&dual, "/ui/organization", &[("org", A)]).await;
    assert!(
        !host
            .get(&dual, "/tokens")
            .await
            .body
            .contains("<strong>laptop</strong>")
    );
}

#[tokio::test]
async fn a_person_in_no_organization_can_create_one_and_a_person_who_owns_one_cannot_create_another()
 {
    let host = team()
        .person("lone", "Lena Lone")
        .creations("lone", 0)
        .start(Mail::Files);
    let lone = me("lone");
    for page in ["/", "/members"] {
        let reply = host.get(&lone, page).await;
        assert_eq!(reply.status, StatusCode::OK, "{page}");
        assert!(reply.body.contains("Create organization"), "{page}");
        assert!(reply.body.contains("action=\"/ui/orgs\""), "{page}");
    }

    let created = host
        .post(&lone, "/ui/orgs", &[("name", "Lone Studio")])
        .await;
    assert_eq!(created.status, StatusCode::SEE_OTHER);
    assert_eq!(created.notice(), "Lone Studio is ready.");
    let memberships = host.access.memberships_for("lone");
    assert_eq!(memberships.len(), 1);
    assert_eq!(memberships[0].role, OrgRole::Owner);
    assert_eq!(
        host.access.organization(&memberships[0].org).unwrap().name,
        "Lone Studio"
    );
    let list = host.get(&lone, "/").await;
    assert!(list.body.contains("app-org__name\"><strong>Lone Studio"));
    assert!(!list.body.contains("Create organization"));

    // One organization of their own is the limit, whether they made it or were given it.
    for who in [&lone, &me("owner")] {
        let second = host.post(who, "/ui/orgs", &[("name", "Another")]).await;
        assert_eq!(second.status, StatusCode::SEE_OTHER);
        assert_eq!(second.notice(), "You already own an organization.");
    }
    assert_eq!(host.access.memberships_for("lone").len(), 1);
    assert_eq!(host.access.memberships_for("owner").len(), 1);
    assert_eq!(host.access.organizations_for("lone").len(), 1);
}

// ------------------------------------------------------------------------------ visibility

#[tokio::test]
async fn a_private_report_is_hidden_from_a_colleague_until_shared_with_the_organization_and_a_public_one_is_not_listed_for_outsiders()
 {
    let host = team()
        .report("aaaa0001/draft", "author", "private")
        .start(Mail::Files);
    let (rita, oscar) = (me("reader"), me("outsider"));
    let page = "/reports/aaaa0001/draft";

    assert_eq!(host.get(&rita, page).await.status, StatusCode::NOT_FOUND);
    assert!(!host.get(&rita, "/").await.body.contains(page));

    let shared = host
        .post(
            &me("author"),
            "/ui/share/aaaa0001/draft",
            &[("visibility", "org")],
        )
        .await;
    assert_eq!(shared.status, StatusCode::SEE_OTHER);
    assert!(shared.location().starts_with(page));
    assert_eq!(
        host.access.acl("aaaa0001/draft").visibility,
        Visibility::Org
    );
    assert!(host.get(&rita, "/").await.body.contains(page));
    assert_eq!(host.get(&rita, page).await.status, StatusCode::OK);
    assert_eq!(host.get(&oscar, page).await.status, StatusCode::NOT_FOUND);
    assert!(!host.get(&oscar, "/").await.body.contains(page));

    // A public link opens for anyone without signing in, and still lists for nobody outside.
    host.post(
        &me("author"),
        "/ui/share/aaaa0001/draft",
        &[("visibility", "public")],
    )
    .await;
    let anonymous = host.report_get("/r/aaaa0001/draft").await;
    assert_eq!(anonymous.status, StatusCode::OK);
    assert!(anonymous.body.contains("Report draft"));
    // This version has no publisher credit. Everyone gets the original public bar.
    let bar = anonymous
        .body
        .find("class=\"rv-host-bar\"")
        .expect("the report host's bar");
    assert!(bar < anonymous.body.find("<h1>Report draft</h1>").unwrap());
    assert!(anonymous.body.contains("anyone with the link can open it"));
    assert!(!anonymous.body.contains("Your report"));
    assert!(!host.get(&oscar, "/").await.body.contains(page));
    assert_eq!(
        host.api(&token_of_outsider(&host), "GET", "/api/reports", None)
            .await
            .json(),
        json!([])
    );
}

/// A token for the outsider of B, made through the page like a person would.
fn token_of_outsider(host: &Host) -> String {
    let (_, secret) = host
        .access
        .create_self_token_for_fixture("outsider", B, "probe")
        .unwrap();
    secret
}

fn publish_body(slug: &str, version: &str, visibility: Option<&str>, new: bool) -> Value {
    let mut body = json!({
        "slug": slug, "new": new,
        "source": format!("---\ntitle: Q3\ndescription: D\nversion: {version}\n---\n\n# Q3\n"),
        "reportFolder": {"root": "t", "source": "report.rmd", "files": ["report.rmd"]}
    });
    if let Some(visibility) = visibility {
        body["visibility"] = json!(visibility);
    }
    body
}

#[cfg(feature = "embedded-runtime")]
#[tokio::test]
async fn a_version_keeps_its_style_credit_across_readers_and_sharing_changes() {
    const KEY: &str = "aaaa0001/q3";
    async fn signed_in(host: &Host, person: &str, path: &str) -> Reply {
        let login = me(person);
        let principal = host.access.authenticate(None, Some(&login.cookie));
        let ticket = host
            .access
            .issue_grant(&principal, KEY, Some(&login.cookie))
            .unwrap();
        let cookie = host.access.redeem_grant(&ticket, KEY).unwrap();
        let request = Request::builder()
            .uri(path)
            .header(header::HOST, "reports.test")
            .header(header::COOKIE, format!("rv_report={cookie}"))
            .body(Body::empty())
            .unwrap();
        call(&host.reports, request).await
    }
    let host = team()
        .token("author", A)
        .report(KEY, "author", "public")
        .grant(KEY, "outsider", "viewer")
        .start(Mail::Files);
    let latest = "/r/aaaa0001/q3";
    let old = "/r/aaaa0001/q3/v/1.0.0";
    let first = "/r/aaaa0001/q3/v/1.0.1";
    let original = host.report_get(old).await;
    assert_eq!(original.status, StatusCode::OK);
    assert!(original.body.contains("Published with"));
    let author = token_of("author", A);
    for (version, name) in [("1.0.1", "Studio North"), ("1.0.2", "Studio South")] {
        let mut body = publish_body("q3", version, None, false);
        body["source"] = json!(format!(
            "---\ntitle: Q3\ndescription: D\nversion: {version}\nstyles: [publisher.rstyle.yaml]\n---\n\n# Q3\n"
        ));
        body["reportFolder"]["files"] = json!(["report.rmd", "publisher.rstyle.yaml"]);
        body["data"] = json!({"publisher.rstyle.yaml": format!("company:\n  name: {name}\n")});
        for created in [true, false] {
            let response = host
                .api(&author, "POST", "/api/reports", Some(body.clone()))
                .await;
            assert_eq!(response.status, StatusCode::OK, "{}", response.body);
            assert_eq!(response.json()["created"], created);
        }
    }
    assert_eq!(host.report_get(old).await.body, original.body);
    // The hosted app answers every report path with `private, no-store` (f5192261), so no shared
    // cache keeps a credit; what this test pins is that all readers get the same bytes.
    let cache = "private, no-store";
    for (path, name) in [(latest, "Studio South"), (first, "Studio North")] {
        let anonymous = host.report_get(path).await;
        assert_eq!(anonymous.status, StatusCode::OK);
        assert!(anonymous.body.contains(&format!("<strong>{name}</strong>")));
        assert!(anonymous.body.contains("anyone with the link can open it"));
        assert!(anonymous.body.contains(">Sign in</a>"));
        assert!(!anonymous.body.contains("Aria Author"));
        assert_eq!(anonymous.headers[header::CACHE_CONTROL], cache);
        for person in ["author", "reader", "outsider"] {
            let page = signed_in(&host, person, path).await;
            assert_eq!(page.status, StatusCode::OK);
            assert_eq!(page.headers[header::CACHE_CONTROL], cache);
            assert_eq!(anonymous.body, page.body);
        }
    }
    host.post(
        &me("author"),
        "/ui/share/aaaa0001/q3",
        &[("visibility", "org")],
    )
    .await;
    assert_eq!(host.access.acl(KEY).visibility, Visibility::Org);
    for (person, message, action) in [
        ("author", "Your report", ">Share</a>"),
        ("outsider", "Shared with you", "Find it again"),
        ("reader", "every member can open it", ">All reports</a>"),
    ] {
        let page = signed_in(&host, person, first).await;
        assert_eq!(page.status, StatusCode::OK);
        assert_eq!(page.headers[header::CACHE_CONTROL], "private, no-store");
        assert!(page.body.contains("<strong>Studio North</strong>"));
        assert!(page.body.contains(message));
        assert!(page.body.contains(action));
    }
    assert!(!host.report_get(first).await.body.contains("Studio North"));
}

#[cfg(feature = "embedded-runtime")]
#[tokio::test]
async fn a_marks_shape_is_decided_when_the_version_is_published() {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let host = team()
        .token("author", A)
        .report("aaaa0001/q3", "author", "public")
        .start(Mail::Files);
    let author = token_of("author", A);
    // A 1 x 1 mark is squarer than a lockup; the example company's mark is 872 x 144.
    let square = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVQI12P4DwQACfsD/WMmxY8AAAAASUVORK5CYII=".to_owned();
    let wide = format!(
        "data:image/png;base64,{}",
        STANDARD.encode(include_bytes!("../../../examples/company-style/logo.png"))
    );
    for (version, name, logo) in [
        ("1.0.1", "Studio North", square),
        ("1.0.2", "Studio South", wide),
    ] {
        let mut body = publish_body("q3", version, None, false);
        body["source"] = json!(format!(
            "---\ntitle: Q3\ndescription: D\nversion: {version}\nstyles: [publisher.rstyle.yaml]\n---\n\n# Q3\n"
        ));
        body["reportFolder"]["files"] = json!(["report.rmd", "publisher.rstyle.yaml", "logo.png"]);
        body["data"] = json!({
            "publisher.rstyle.yaml": format!("company:\n  name: {name}\n  logo: logo.png\n"),
            "logo.png": logo,
        });
        let response = host.api(&author, "POST", "/api/reports", Some(body)).await;
        assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    }
    // The squarer mark keeps the name beside it.
    let square_page = host.report_get("/r/aaaa0001/q3/v/1.0.1").await;
    assert_eq!(square_page.status, StatusCode::OK);
    assert!(
        square_page
            .body
            .contains("<span class=\"rv-publisher\" title=\"Studio North\"><img ")
    );
    assert!(square_page.body.contains("<strong>Studio North</strong>"));
    assert!(
        !square_page
            .body
            .contains("<span class=\"rv-publisher rv-publisher--lockup\"")
    );
    // The wide lockup stands alone; the name stays as its alt text.
    let wide_page = host.report_get("/r/aaaa0001/q3/v/1.0.2").await;
    assert_eq!(wide_page.status, StatusCode::OK);
    assert!(wide_page.body.contains(
        "<span class=\"rv-publisher rv-publisher--lockup\" title=\"Studio South\"><img "
    ));
    assert!(!wide_page.body.contains("<strong>Studio South</strong>"));
    let credit = wide_page
        .body
        .split_once("<span class=\"rv-publisher")
        .unwrap()
        .1
        .split_once("</span>")
        .unwrap()
        .0;
    assert!(credit.contains("alt=\"Studio South\""));
}

#[tokio::test]
async fn the_hosted_version_list_places_a_release_before_its_prerelease() {
    let key = "aaaa0001/q3";
    let host = team().report(key, "author", "private").start(Mail::Files);
    let storage = &host.state.storage;
    let seed = storage.get(key, None).unwrap().unwrap();
    for version in ["3.0.0", "3.0.0-alpha.4"] {
        storage
            .put_version(StoredReport {
                version: version.into(),
                ..seed.clone()
            })
            .unwrap();
    }
    let page = host.get(&me("author"), "/reports/aaaa0001/q3").await;
    assert_eq!(page.status, StatusCode::OK);
    let release = page.body.find("href=\"/r/aaaa0001/q3/v/3.0.0\"").unwrap();
    let prerelease = page
        .body
        .find("href=\"/r/aaaa0001/q3/v/3.0.0-alpha.4\"")
        .unwrap();
    assert!(release < prerelease);
}

#[tokio::test]
async fn an_editor_cannot_change_who_can_open_a_report_by_republishing_it() {
    let host = team()
        .person("ed", "Edith Editor")
        .member("ed", A, "reader")
        .token("ed", A)
        .report("aaaa0001/q3", "author", "private")
        .grant("aaaa0001/q3", "ed", "editor")
        .start(Mail::Files);
    let token = token_of("ed", A);

    // Refused before the report is rendered, so this holds with or without the runtime.
    let refused = host
        .api(
            &token,
            "POST",
            "/api/reports",
            Some(publish_body("q3", "1.0.1", Some("org"), false)),
        )
        .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.body);
    assert_eq!(
        host.access.acl("aaaa0001/q3").visibility,
        Visibility::Private
    );

    // The same publish naming the visibility it already has is not turned away for that reason.
    let unchanged = host
        .api(
            &token,
            "POST",
            "/api/reports",
            Some(publish_body("q3", "1.0.1", Some("private"), false)),
        )
        .await;
    assert_ne!(
        unchanged.status,
        StatusCode::FORBIDDEN,
        "{}",
        unchanged.body
    );
    assert_ne!(
        unchanged.status,
        StatusCode::NOT_FOUND,
        "{}",
        unchanged.body
    );
    assert_eq!(
        host.access.acl("aaaa0001/q3").visibility,
        Visibility::Private
    );
}

// Publishing renders the report, which needs the embedded runtime (as in hosted_scope.rs).
#[cfg(feature = "embedded-runtime")]
#[tokio::test]
async fn publishing_sets_the_visibility_of_a_new_report_and_only_the_share_right_changes_it_later()
{
    let host = team()
        .person("ed", "Edith Editor")
        .member("ed", A, "reader")
        .token("ed", A)
        .token("author", A)
        .start(Mail::Files);
    let author = token_of("author", A);

    let created = host
        .api(
            &author,
            "POST",
            "/api/reports",
            Some(publish_body("q3", "1.0.0", Some("org"), true)),
        )
        .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.body);
    assert_eq!(created.json()["visibility"], "org");
    assert_eq!(host.access.acl("aaaa0001/q3").visibility, Visibility::Org);

    // Without a visibility a new report starts private.
    let plain = host
        .api(
            &author,
            "POST",
            "/api/reports",
            Some(publish_body("plain", "1.0.0", None, true)),
        )
        .await;
    assert_eq!(plain.status, StatusCode::OK, "{}", plain.body);
    assert_eq!(
        host.access.acl("aaaa0001/plain").visibility,
        Visibility::Private
    );

    // Make the reader an editor of the report: they publish versions but cannot change who reads.
    host.access
        .set_acl_for_fixture(
            "aaaa0001/q3",
            Visibility::Org,
            vec![richview_app::access::Grant {
                user: "ed".into(),
                role: ReportRole::Editor,
            }],
        )
        .unwrap();
    let editor = token_of("ed", A);
    let refused = host
        .api(
            &editor,
            "POST",
            "/api/reports",
            Some(publish_body("q3", "1.0.1", Some("private"), false)),
        )
        .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.body);
    assert_eq!(host.access.acl("aaaa0001/q3").visibility, Visibility::Org);
    let version = host
        .api(
            &editor,
            "POST",
            "/api/reports",
            Some(publish_body("q3", "1.0.1", None, false)),
        )
        .await;
    assert_eq!(version.status, StatusCode::OK, "{}", version.body);
    assert_eq!(host.access.acl("aaaa0001/q3").visibility, Visibility::Org);

    let changed = host
        .api(
            &author,
            "POST",
            "/api/reports",
            Some(publish_body("q3", "1.0.2", Some("private"), false)),
        )
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.body);
    assert_eq!(
        host.access.acl("aaaa0001/q3").visibility,
        Visibility::Private
    );
}

// ------------------------------------------------------------------------------ naming people

/// Everything in a reply that may differ between two addresses, with the address blanked out.
fn blank(text: &str, address: &str) -> String {
    text.replace(address, "ADDRESS")
}

/// The table row of the person named `address` on a report page, address blanked out.
fn row_for(body: &str, address: &str) -> String {
    let at = body
        .find(&format!("<strong>{address}</strong>"))
        .unwrap_or_else(|| panic!("no row for {address}"));
    let start = body[..at].rfind("<li").unwrap();
    let end = at + body[at..].find("</li>").unwrap() + "</li>".len();
    blank(&body[start..end], address)
}

#[tokio::test]
async fn sharing_with_an_address_that_has_an_account_looks_exactly_like_sharing_with_one_that_has_none()
 {
    let host = sharing().start(Mail::Files);
    let (known, unknown) = ("known@example.test", "unknown@example.test");
    assert!(host.has_account(known) && !host.has_account(unknown));
    let author = me("author");

    let mut replies = Vec::new();
    for address in [known, unknown] {
        let reply = host
            .post(
                &author,
                "/ui/reports/aaaa0001/q3/people",
                &[("email", address), ("role", "viewer")],
            )
            .await;
        replies.push((reply, address));
    }
    let [(first, _), (second, _)] = &replies[..] else {
        unreachable!()
    };
    assert_eq!(first.status, StatusCode::SEE_OTHER);
    assert_eq!(first.status, second.status);
    assert_eq!(
        blank(&first.notice(), known),
        blank(&second.notice(), unknown),
        "the notice tells the two apart"
    );
    assert_eq!(
        first.location().split('?').next().unwrap(),
        second.location().split('?').next().unwrap()
    );

    // Both got the same mail, and neither was turned into a grant.
    let (mail_known, mail_unknown) = (
        host.wait_letters(known, 1).await.remove(0),
        host.wait_letters(unknown, 1).await.remove(0),
    );
    assert_eq!(mail_known.subject, mail_unknown.subject);
    assert_eq!(
        mail_known.body.replace(&mail_known.link(), "LINK"),
        mail_unknown.body.replace(&mail_unknown.link(), "LINK")
    );

    let page = host.get(&author, "/reports/aaaa0001/q3").await;
    assert_eq!(page.status, StatusCode::OK);
    assert_eq!(page.body.matches("Invited, not opened yet").count(), 2);
    assert_eq!(row_for(&page.body, known), row_for(&page.body, unknown));
    // A name is shown only for members of the report's organization.
    assert!(!page.body.contains("Kim Known"));

    let access = host
        .api(
            &token_of("author", A),
            "GET",
            "/api/reports/aaaa0001/q3/access",
            None,
        )
        .await;
    assert_eq!(access.status, StatusCode::OK);
    assert_eq!(access.json()["grants"], json!([]));
    assert!(host.access.acl("aaaa0001/q3").grants.is_empty());
    assert_eq!(host.invitations().len(), 2);
}

#[tokio::test]
async fn inviting_an_address_that_has_an_account_looks_exactly_like_inviting_one_that_has_none() {
    let host = sharing().start(Mail::Files);
    let (known, unknown) = ("known@example.test", "unknown@example.test");
    let mut notices = Vec::new();
    for address in [known, unknown] {
        let reply = host
            .post(
                &me("owner"),
                &format!("/ui/orgs/{A}/invitations"),
                &[("email", address), ("role", "reader")],
            )
            .await;
        assert_eq!(reply.status, StatusCode::SEE_OTHER);
        notices.push(blank(&reply.notice(), address));
        assert_eq!(reply.location().split('?').next().unwrap(), "/members");
    }
    assert_eq!(notices[0], notices[1]);
    let first = host.wait_letters(known, 1).await.remove(0);
    let second = host.wait_letters(unknown, 1).await.remove(0);
    assert_eq!(first.subject, second.subject);
    assert_eq!(
        first.body.replace(&first.link(), "LINK"),
        second.body.replace(&second.link(), "LINK")
    );
    // Nothing joined yet, for either.
    assert_eq!(host.access.role_in("known", A), None);
    let page = host.get(&me("owner"), "/members").await;
    assert_eq!(
        row_for_invitation(&page.body, known),
        row_for_invitation(&page.body, unknown)
    );
}

/// The open-invitation row of `address` on the members page, address blanked out.
fn row_for_invitation(body: &str, address: &str) -> String {
    let at = body
        .find(&format!("<strong>{address}</strong>"))
        .unwrap_or_else(|| panic!("no invitation row for {address}"));
    let start = body[..at].rfind("<tr").unwrap();
    let end = at + body[at..].find("</tr>").unwrap() + "</tr>".len();
    // The expiry and the identifier differ by nature; the wording must not.
    let row = blank(&body[start..end], address);
    let id = between(&row, "/invitations/", "/resend").to_owned();
    row.replace(&id, "ID")
}

#[tokio::test]
async fn an_existing_account_sees_a_shared_report_in_its_list_and_nothing_else_of_the_organization()
{
    let host = sharing().start(Mail::Files);
    host.post(
        &me("author"),
        "/ui/reports/aaaa0001/q3/people",
        &[("email", "known@example.test"), ("role", "viewer")],
    )
    .await;
    let known = me("known");

    // Signing in and opening the list is the proof; the report turns into a grant.
    let all = host.get(&known, "/").await;
    assert_eq!(all.status, StatusCode::OK);
    assert!(all.body.contains("href=\"/reports/aaaa0001/q3\""));
    let shared = host.get(&known, "/?show=shared").await;
    assert!(shared.body.contains("href=\"/reports/aaaa0001/q3\""));
    assert!(!shared.body.contains("Nobody has shared"));
    // The row says it was shared with them and by whom; they may read it, so it leads to the
    // report's page and never to the editor, and its menu offers details, not sharing.
    assert!(shared.body.contains("chip chip--you"));
    assert!(!shared.body.contains("chip chip--org") && !shared.body.contains("chip chip--public"));
    assert!(shared.body.contains("Shared by Aria Author"));
    assert!(shared.body.contains("</svg>Details and versions</a>"));
    assert!(!shared.body.contains("aaaa0001/q3/edit"));
    assert!(!shared.body.contains("aaaa0001/q3/delete"));
    let owner_list = host.get(&me("author"), "/").await;
    assert!(owner_list.body.contains("Shared with known@example.test"));
    assert!(owner_list.body.contains("</svg>Sharing and versions</a>"));
    assert!(
        owner_list
            .body
            .contains("href=\"/reports/aaaa0001/q3/edit\"")
    );
    for hidden in ["other", "orgwide"] {
        assert!(!all.body.contains(&format!("aaaa0001/{hidden}")));
        assert_eq!(
            host.get(&known, &format!("/reports/aaaa0001/{hidden}"))
                .await
                .status,
            StatusCode::NOT_FOUND,
            "{hidden}"
        );
    }
    assert_eq!(
        host.get(&known, "/reports/aaaa0001/q3").await.status,
        StatusCode::OK
    );
    let grants = host.access.acl("aaaa0001/q3").grants;
    assert_eq!(grants.len(), 1);
    assert_eq!(
        (grants[0].user.as_str(), grants[0].role),
        ("known", ReportRole::Viewer)
    );
    assert!(host.invitations().is_empty());

    // The members page is their own organization's, not the report's.
    let members = host.get(&known, "/members").await;
    assert_eq!(members.status, StatusCode::OK);
    assert!(members.body.contains("Bravo Co"));
    for secret in ["Olivia Owner", "owner@example.test", "Aria Author", "Acme"] {
        assert!(!members.body.contains(secret), "{secret}");
    }
}

#[tokio::test]
async fn an_existing_account_opening_a_share_link_gets_the_grant_without_a_session() {
    let host = sharing().start(Mail::Files);
    host.post(
        &me("author"),
        "/ui/reports/aaaa0001/q3/people",
        &[("email", "known@example.test"), ("role", "editor")],
    )
    .await;
    let link = host
        .wait_letters("known@example.test", 1)
        .await
        .remove(0)
        .link();
    let page = host.get(&nobody(), &link).await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(page.body.contains("Report q3"));
    assert!(page.body.contains("Accept and sign in"));

    let accepted = host.post(&nobody(), &form_path(&link), &[]).await;
    assert_eq!(accepted.status, StatusCode::SEE_OTHER);
    // Not signed in: sent to sign in, and back to the report from there.
    assert!(accepted.location().starts_with("/?m=Accepted"));
    assert!(accepted.location().ends_with("next=/r/aaaa0001/q3"));
    assert!(accepted.headers.get(header::SET_COOKIE).is_none());
    let grants = host.access.acl("aaaa0001/q3").grants;
    assert_eq!(
        (grants[0].user.as_str(), grants[0].role),
        ("known", ReportRole::Editor)
    );
    assert_gone(&host.get(&nobody(), &link).await);
}

#[tokio::test]
async fn a_new_person_accepting_a_share_link_is_signed_in_with_that_one_report_and_no_organization()
{
    let host = sharing().start(Mail::Files);
    host.post(
        &me("author"),
        "/ui/reports/aaaa0001/q3/people",
        &[("email", "fresh@example.test"), ("role", "viewer")],
    )
    .await;
    let letter = host.wait_letters("fresh@example.test", 1).await.remove(0);
    assert_eq!(letter.subject, "Aria Author shared “Report q3” with you");
    assert!(letter.body.contains("Aria Author (author@example.test) shared “Report q3” from Acme with you, so you can read it."));
    let link = letter.link();
    let page = host.get(&nobody(), &link).await;
    assert!(page.body.contains("Open the shared report"));
    assert!(page.body.contains("Report q3"));

    let joined = host
        .post(
            &nobody(),
            &form_path(&link),
            &[
                ("name", "Fiona Fresh"),
                ("password", PASSWORD),
                ("password_confirm", PASSWORD),
            ],
        )
        .await;
    assert_eq!(joined.status, StatusCode::SEE_OTHER);
    // The report itself lives on another origin, so they land on its page here.
    assert!(
        joined.location().starts_with("/reports/aaaa0001/q3?m="),
        "{}",
        joined.location()
    );
    assert_eq!(joined.notice(), "This report was shared with you.");
    let fiona = host.login_from(&joined);
    // That page sends the browser on to the report in a navigation of its own; the page reached
    // any other way stays put.
    let landed = host.get(&fiona, &joined.location()).await;
    assert_eq!(landed.status, StatusCode::OK);
    assert_eq!(landed.headers["refresh"], "0; url=/r/aaaa0001/q3");
    assert!(
        !host
            .get(&fiona, "/reports/aaaa0001/q3")
            .await
            .headers
            .contains_key("refresh")
    );
    let id = host.id_of("fresh@example.test");
    assert!(host.access.memberships_for(&id).is_empty());
    assert_eq!(host.access.acl("aaaa0001/q3").grants[0].user, id);

    let list = host.get(&fiona, "/").await;
    assert_eq!(list.status, StatusCode::OK);
    assert!(list.body.contains("href=\"/reports/aaaa0001/q3\""));
    assert!(!list.body.contains("aaaa0001/other"));
    assert_eq!(
        host.get(&fiona, "/reports/aaaa0001/q3").await.status,
        StatusCode::OK
    );
    assert_eq!(
        host.get(&fiona, "/reports/aaaa0001/other").await.status,
        StatusCode::NOT_FOUND
    );
    assert!(
        host.get(&fiona, "/members")
            .await
            .body
            .contains("Create organization")
    );
    assert_gone(&host.get(&nobody(), &link).await);
    // Last, so a failure here does not hide the checks above: the list offers the way to start
    // an organization alongside the shared report, not only when it is empty.
    assert!(
        list.body.contains("Create organization"),
        "a person in no organization is not offered one next to the report shared with them"
    );
}

#[tokio::test]
async fn removing_a_person_from_a_report_ends_a_pending_invitation_and_an_accepted_grant_alike() {
    let host = sharing()
        .person("granted", "Gail Granted")
        .member("granted", B, "author")
        .grant("aaaa0001/q3", "granted", "viewer")
        .start(Mail::Files);
    let author = me("author");
    host.post(
        &author,
        "/ui/reports/aaaa0001/q3/people",
        &[("email", "known@example.test"), ("role", "viewer")],
    )
    .await;
    let link = host
        .wait_letters("known@example.test", 1)
        .await
        .remove(0)
        .link();
    assert_eq!(
        host.get(&me("granted"), "/reports/aaaa0001/q3")
            .await
            .status,
        StatusCode::OK
    );

    for address in ["known@example.test", "granted@example.test"] {
        let removed = host
            .post(
                &author,
                "/ui/reports/aaaa0001/q3/people/remove",
                &[("email", address)],
            )
            .await;
        assert_eq!(removed.status, StatusCode::SEE_OTHER, "{address}");
        assert_eq!(removed.notice(), "Removed.");
    }
    assert!(host.invitations().is_empty());
    assert!(host.access.acl("aaaa0001/q3").grants.is_empty());
    assert_gone(&host.get(&nobody(), &link).await);
    for who in ["known", "granted"] {
        let list = host.get(&me(who), "/").await;
        assert!(!list.body.contains("aaaa0001/q3"), "{who}");
        assert_eq!(
            host.get(&me(who), "/reports/aaaa0001/q3").await.status,
            StatusCode::NOT_FOUND,
            "{who}"
        );
    }
    let page = host.get(&author, "/reports/aaaa0001/q3").await;
    assert!(!page.body.contains("known@example.test"));
    assert!(!page.body.contains("granted@example.test"));
}

#[tokio::test]
async fn a_row_opens_the_editor_for_those_who_may_edit_and_offers_delete_to_those_who_may_delete() {
    let host = team()
        .person("ed", "Edith Editor")
        .member("ed", A, "reader")
        .report("aaaa0001/q3", "author", "org")
        .grant("aaaa0001/q3", "ed", "editor")
        .start(Mail::Files);
    let edit = "href=\"/reports/aaaa0001/q3/edit\"";
    let delete = "href=\"/reports/aaaa0001/q3/delete\"";
    // Its owner and the administrators of the organization hold every right.
    for who in ["author", "admin", "owner"] {
        let list = host.get(&me(who), "/").await;
        assert!(list.body.contains(edit), "{who}");
        assert!(list.body.contains(delete), "{who}");
    }
    // An editor named on it may edit but not delete.
    let editor = host.get(&me("ed"), "/").await;
    assert!(editor.body.contains(edit));
    assert!(!editor.body.contains(delete));
    // A reader sees the organization's report and may do neither: the row leads to its page.
    let reader = host.get(&me("reader"), "/").await;
    assert!(reader.body.contains("href=\"/reports/aaaa0001/q3\""));
    assert!(!reader.body.contains(edit) && !reader.body.contains(delete));
}

#[tokio::test]
async fn the_editor_opens_for_those_who_may_edit_and_for_nobody_else() {
    let host = team()
        .person("ed", "Edith Editor")
        .member("ed", A, "reader")
        .person("viewer", "Vera Viewer")
        .member("viewer", A, "reader")
        .report("aaaa0001/q3", "author", "private")
        .report("aaaa0001/orgwide", "owner", "org")
        .grant("aaaa0001/q3", "ed", "editor")
        .grant("aaaa0001/q3", "viewer", "viewer")
        .start(Mail::Files);
    // Its owner, an editor named on it and the administrators open it on the stored report.
    for who in ["author", "ed", "admin", "owner"] {
        let page = host.get(&me(who), "/reports/aaaa0001/q3/edit").await;
        assert_eq!(page.status, StatusCode::OK, "{who}");
        assert!(page.body.contains("data-richview-page=\"editor\""), "{who}");
        assert!(page.body.contains("data-org=\"aaaa0001\""), "{who}");
        assert!(page.body.contains("data-slug=\"q3\""), "{who}");
        assert!(page.body.contains("data-version=\"1.0.0\""), "{who}");
        assert!(page.body.contains("data-new=\"false\""), "{who}");
        assert!(
            page.body.contains(&format!("data-csrf=\"csrf-{who}\"")),
            "{who}"
        );
        assert!(
            page.body
                .contains("<strong id=\"ed-title\">Report q3</strong>")
        );
        let policy = page.headers[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap();
        assert!(policy.contains("worker-src 'self' blob:"), "{policy}");
        assert!(policy.contains("frame-ancestors 'none'"), "{policy}");
        assert!(!policy.contains("http"), "{policy}");
        assert_eq!(page.headers[header::CACHE_CONTROL], "no-store");
        assert_eq!(page.headers[header::REFERRER_POLICY], "no-referrer");
    }
    // Someone who can read it but not edit it is taken to its page, which says so.
    for (who, path) in [
        ("viewer", "/reports/aaaa0001/q3"),
        ("reader", "/reports/aaaa0001/orgwide"),
    ] {
        let reply = host.get(&me(who), &format!("{path}/edit")).await;
        assert_eq!(reply.status, StatusCode::SEE_OTHER, "{who}");
        assert!(
            reply.location().starts_with(path),
            "{who}: {}",
            reply.location()
        );
        assert_eq!(
            reply.notice(),
            "You can read this report but not edit it.",
            "{who}"
        );
    }
    // Anyone else gets the answer a missing report gets, and a signed-out visitor is asked to
    // sign in first.
    let missing = host
        .get(&me("outsider"), "/reports/aaaa0001/nothing/edit")
        .await;
    for who in ["outsider", "reader"] {
        let hidden = host.get(&me(who), "/reports/aaaa0001/q3/edit").await;
        assert_eq!(hidden.status, StatusCode::NOT_FOUND, "{who}");
        assert_eq!(
            hidden.body, missing.body,
            "{who} learns that the report exists"
        );
    }
    let signed_out = host.get(&nobody(), "/reports/aaaa0001/q3/edit").await;
    assert_eq!(signed_out.status, StatusCode::SEE_OTHER);
    assert_eq!(signed_out.notice(), "Sign in first.");
}

#[tokio::test]
async fn a_new_report_opens_the_editor_for_a_person_who_may_publish_and_for_nobody_else() {
    let host = team().person("lone", "Lena Lone").start(Mail::Files);
    for who in ["author", "admin", "owner"] {
        let page = host.get(&me(who), "/reports/new").await;
        assert_eq!(page.status, StatusCode::OK, "{who}");
        assert!(page.body.contains("data-richview-page=\"editor\""), "{who}");
        assert!(page.body.contains("data-org=\"aaaa0001\""), "{who}");
        assert!(page.body.contains("data-slug=\"\""), "{who}");
        assert!(page.body.contains("data-new=\"true\""), "{who}");
        assert!(page.body.contains("New report"), "{who}");
        assert!(
            page.body
                .contains("id=\"ed-history\" href=\"/\" aria-label=\"History\" hidden>")
        );
    }
    let reader = host.get(&me("reader"), "/reports/new").await;
    assert_eq!(reader.status, StatusCode::SEE_OTHER);
    assert_eq!(
        reader.notice(),
        "Readers open reports; authors create them."
    );
    let lone = host.get(&me("lone"), "/reports/new").await;
    assert_eq!(lone.status, StatusCode::SEE_OTHER);
    assert_eq!(lone.notice(), "Create or join an organization first.");
    let signed_out = host.get(&nobody(), "/reports/new").await;
    assert_eq!(signed_out.status, StatusCode::SEE_OTHER);
    assert_eq!(signed_out.notice(), "Sign in first.");
}

#[tokio::test]
async fn the_editors_files_are_served_by_the_app_origin_and_not_by_the_report_origin() {
    let host = team().start(Mail::Files);
    for path in [
        "/richview_site.js",
        "/richview_site_bg.wasm",
        "/worker.js",
        "/richview-fonts.css",
        "/fonts-math.css",
        "/richview_runtime.js",
        "/richview_runtime_bg.wasm",
    ] {
        // A build without the staged runtime answers 503; one with it answers 200. Neither
        // answers 404, which is what a missing route says.
        let reply = host.get(&nobody(), path).await;
        assert!(
            matches!(
                reply.status,
                StatusCode::OK | StatusCode::SERVICE_UNAVAILABLE
            ),
            "{path}: {}",
            reply.status
        );
        let there = host.report_get(path).await;
        assert_eq!(
            there.status,
            StatusCode::NOT_FOUND,
            "{path} on the report origin"
        );
    }
}

#[tokio::test]
async fn only_someone_with_the_share_right_can_share_a_report() {
    let host = team()
        .person("ed", "Edith Editor")
        .person("coowner", "Cory Coowner")
        .member("ed", A, "author")
        .member("coowner", A, "reader")
        .report("aaaa0001/q3", "author", "org")
        .grant("aaaa0001/q3", "ed", "editor")
        .grant("aaaa0001/q3", "coowner", "owner")
        .start(Mail::Files);
    let share = |who: &str, address: &'static str, role: &'static str| {
        let who = me(who);
        let host = &host;
        async move {
            host.post(
                &who,
                "/ui/reports/aaaa0001/q3/people",
                &[("email", address), ("role", role)],
            )
            .await
        }
    };
    let denied = |reply: &Reply, who: &str| {
        assert!(
            matches!(reply.status, StatusCode::FORBIDDEN | StatusCode::NOT_FOUND),
            "{who} got {}",
            reply.status
        );
    };

    for who in ["ed", "reader"] {
        denied(&share(who, "x@example.test", "viewer").await, who);
        let unshare = host
            .post(
                &me(who),
                "/ui/reports/aaaa0001/q3/people/remove",
                &[("email", "coowner@example.test")],
            )
            .await;
        denied(&unshare, who);
        let visibility = host
            .post(
                &me(who),
                "/ui/share/aaaa0001/q3",
                &[("visibility", "public")],
            )
            .await;
        denied(&visibility, who);
    }
    // Somebody from another organization cannot even tell the report is there.
    assert_eq!(
        share("outsider", "x@example.test", "viewer").await.status,
        StatusCode::NOT_FOUND
    );
    assert!(host.invitations().is_empty());
    assert_eq!(host.access.acl("aaaa0001/q3").visibility, Visibility::Org);
    assert_eq!(host.access.acl("aaaa0001/q3").grants.len(), 2);

    // The owner role is not one a form can hand out.
    let owner = share("author", "x@example.test", "owner").await;
    assert_eq!(owner.status, StatusCode::SEE_OTHER);
    assert_eq!(owner.notice(), "Choose what they can do.");
    assert!(host.invitations().is_empty());

    // Those who do hold the right can: an admin, and a person holding an owner grant.
    for who in ["admin", "coowner"] {
        let ok = share(who, "y@example.test", "viewer").await;
        assert!(
            ok.notice().starts_with("Shared with y@example.test."),
            "{who}: {}",
            ok.notice()
        );
    }
}

// ------------------------------------------------------------------------------ limits

#[tokio::test]
async fn the_thirty_third_named_person_on_a_report_is_refused() {
    // 20 grants and 11 open invitations: 31 names, so exactly one more fits.
    let mut seed = team()
        .token("author", A)
        .report("aaaa0001/q3", "author", "private");
    for i in 1..=20 {
        let id = format!("g{i:02}");
        seed = seed.person(&id, &id).grant("aaaa0001/q3", &id, "viewer");
    }
    for i in 1..=11 {
        seed = seed.invitation(json!({
            "id": format!("i-seed{i:02}"), "org": A, "email": format!("pending{i:02}@example.test"),
            "report": "aaaa0001/q3", "report_role": "viewer", "inviter": "author",
            "created": 1, "expires": 4_102_444_800u64
        }));
    }
    let host = seed.start(Mail::Files);
    let author = me("author");
    let share = |address: &'static str| {
        let host = &host;
        let author = author.clone();
        async move {
            host.post(
                &author,
                "/ui/reports/aaaa0001/q3/people",
                &[("email", address), ("role", "viewer")],
            )
            .await
        }
    };

    let last = share("thirty2@example.test").await;
    assert!(
        last.notice()
            .starts_with("Shared with thirty2@example.test."),
        "{}",
        last.notice()
    );
    host.wait_letters("thirty2@example.test", 1).await;
    assert_eq!(host.invitations().len(), 12);

    let over = share("thirty3@example.test").await;
    assert_eq!(over.status, StatusCode::SEE_OTHER);
    assert!(
        over.notice().contains("at most 32 named people"),
        "{}",
        over.notice()
    );
    assert_eq!(host.invitations().len(), 12);
    assert!(
        !host
            .invitations()
            .iter()
            .any(|i| i["email"] == "thirty3@example.test")
    );
    assert!(host.letters("thirty3@example.test").is_empty());
    assert_eq!(host.access.acl("aaaa0001/q3").grants.len(), 20);
}

#[tokio::test]
async fn a_fourth_invitation_mail_to_one_address_in_an_hour_is_not_written_but_the_invitation_stays_pending()
 {
    let host = team().start(Mail::Files);
    let owner = me("owner");
    let address = "repeat@example.test";
    let invited = host
        .post(
            &owner,
            &format!("/ui/orgs/{A}/invitations"),
            &[("email", address), ("role", "reader")],
        )
        .await;
    assert!(invited.notice().starts_with("Invited repeat@example.test."));
    let id = host.invitations()[0]["id"].as_str().unwrap().to_owned();

    let mut notices = Vec::new();
    for _ in 0..3 {
        let again = host
            .post(
                &owner,
                &format!("/ui/orgs/{A}/invitations/{id}/resend"),
                &[],
            )
            .await;
        notices.push(again.notice());
    }
    assert!(notices[0].starts_with("Invited repeat@example.test again."));
    assert!(notices[1].starts_with("Invited repeat@example.test again."));
    assert!(
        notices[2].contains("could not be sent"),
        "the fourth mail was queued: {}",
        notices[2]
    );

    host.wait_letters(address, 3).await;
    // Anything queued before this invitation's mail has been written once it appears.
    host.drain_mail(&owner, A).await;
    assert_eq!(host.letters(address).len(), 3);
    assert_eq!(
        host.invitations()
            .iter()
            .filter(|i| i["email"] == address)
            .count(),
        1
    );
    let members = host.get(&owner, "/members").await;
    assert!(members.body.contains(address));
    assert!(members.body.contains("Send again"));
}

// ------------------------------------------------------------------------------ text safety

fn assert_escaped(body: &str, page: &str) {
    assert!(
        !body.contains("<script>alert(1)"),
        "{page}: a raw script tag"
    );
    assert!(!body.contains("<img src=x"), "{page}: a raw image tag");
    assert!(
        body.contains("&lt;script&gt;alert(1)&lt;/script&gt;"),
        "{page}: the organization name is missing"
    );
    assert!(
        body.contains("&lt;img src=x&gt;"),
        "{page}: the person's name is missing"
    );
}

#[tokio::test]
async fn organization_and_person_names_are_escaped_on_every_page() {
    let host = team()
        .person("mallory", "<img src=x>")
        .member("mallory", A, "author")
        .report("aaaa0001/q3", "mallory", "org")
        .grant("aaaa0001/q3", "outsider", "viewer")
        .start(Mail::Files);
    let renamed = host
        .post(
            &me("owner"),
            &format!("/ui/orgs/{A}/name"),
            &[("name", "<script>alert(1)</script>")],
        )
        .await;
    assert_eq!(renamed.notice(), "Name saved.");
    for page in ["/members", "/reports/aaaa0001/q3"] {
        let reply = host.get(&me("owner"), page).await;
        assert_eq!(reply.status, StatusCode::OK, "{page}");
        assert_escaped(&reply.body, page);
    }
    // The list names the organization in its rail and a person only where a row says who shared
    // the report with someone, so each is checked where it appears.
    for who in ["owner", "reader"] {
        let list = host.get(&me(who), "/").await;
        assert!(!list.body.contains("<script>alert(1)"), "{who}");
        assert!(
            list.body.contains("&lt;script&gt;alert(1)&lt;/script&gt;"),
            "{who}"
        );
    }
    let outside = host.get(&me("outsider"), "/").await;
    assert!(outside.body.contains("Shared by &lt;img src=x&gt;"));
    assert!(!outside.body.contains("<img src=x"));
    // A colleague who only reads sees them escaped too.
    let reply = host.get(&me("reader"), "/members").await;
    assert_escaped(&reply.body, "/members");
}

#[tokio::test]
async fn an_organization_named_like_a_link_adds_no_second_link_to_the_invitation_mail() {
    let host = team().start(Mail::Files);
    let renamed = host
        .post(
            &me("owner"),
            &format!("/ui/orgs/{A}/name"),
            &[("name", "Visit https://evil.test")],
        )
        .await;
    assert_eq!(renamed.notice(), "Name saved.");
    let letter = invite_as(&host, "owner", "guest@example.test", "reader").await;
    // `link()` has already counted exactly one link in the body.
    let link = letter.link();
    assert!(link.starts_with("/link/"));
    assert!(!letter.body.contains("evil.test"));
    assert!(!letter.subject.contains("evil.test"));
    assert!(!letter.subject.contains(['\r', '\n']));
    assert_eq!(letter.to, "guest@example.test");
    assert_eq!(
        letter.subject,
        "Olivia Owner invited you to their organization on RichView"
    );
}

// ------------------------------------------------------------------------------ by hand

#[tokio::test]
async fn an_invitation_in_the_manual_outbox_is_dispatched_by_hand_with_a_fresh_link() {
    let host = team().start(Mail::Manual);
    let invited = host
        .post(
            &me("owner"),
            &format!("/ui/orgs/{A}/invitations"),
            &[("email", "late@example.test"), ("role", "reader")],
        )
        .await;
    assert!(
        invited.notice().contains("send mails by hand"),
        "{}",
        invited.notice()
    );
    host.post(
        &me("owner"),
        &format!("/ui/orgs/{A}/invitations"),
        &[("email", "cancelled@example.test"), ("role", "reader")],
    )
    .await;
    let cancelled = host
        .invitations()
        .into_iter()
        .find(|i| i["email"] == "cancelled@example.test")
        .unwrap();
    host.post(
        &me("owner"),
        &format!(
            "/ui/orgs/{A}/invitations/{}/cancel",
            cancelled["id"].as_str().unwrap()
        ),
        &[],
    )
    .await;

    // Two requests are waiting, and neither holds a link; no link has been issued yet.
    let mut requests = Vec::new();
    for _ in 0..500 {
        requests = fs::read_dir(host.outbox())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".request.json"))
            .map(|e| {
                (
                    e.path(),
                    serde_json::from_slice::<Value>(&fs::read(e.path()).unwrap()).unwrap(),
                )
            })
            .collect();
        if requests.len() == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(requests.len(), 2);
    for (path, request) in &requests {
        assert_eq!(request["kind"], "invite");
        assert!(!fs::read_to_string(path).unwrap().contains("/link/"));
    }
    assert!(!host.data.join(".access/links.json").exists());
    let late_id = host
        .invitations()
        .into_iter()
        .find(|i| i["email"] == "late@example.test")
        .unwrap()["id"]
        .clone();
    let find = |to: &str| {
        requests
            .iter()
            .map(|(_, r)| r)
            .find(|r| r["to"] == to)
            .unwrap_or_else(|| panic!("no request for {to}"))
    };
    assert_eq!(find("late@example.test")["invitation"], late_id);
    let pending = operator_action(&host.state, OperatorAction::PendingLinks)
        .await
        .unwrap();
    // The operator's list holds only the mail there is to send: the cancelled invitation's
    // request is left out, as is one for an invitation that a second share replaced.
    let waiting = pending["manualRequests"].as_array().unwrap();
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0]["to"], "late@example.test");

    // An invitation cancelled since is not delivered.
    let stale = find("cancelled@example.test")["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let error = operator_action(&host.state, OperatorAction::DispatchManual { id: stale })
        .await
        .unwrap_err();
    assert!(error.contains("no longer open"), "{error}");

    let id = find("late@example.test")["id"].as_str().unwrap().to_owned();
    let mail = operator_action(
        &host.state,
        OperatorAction::DispatchManual { id: id.clone() },
    )
    .await
    .unwrap();
    assert_eq!(mail["to"], "late@example.test");
    assert_eq!(
        mail["subject"],
        "Olivia Owner invited you to Acme on RichView"
    );
    let letter = Letter {
        to: "late@example.test".into(),
        subject: mail["subject"].as_str().unwrap().into(),
        body: mail["body"].as_str().unwrap().into(),
    };
    let link = letter.link();
    let links: Value =
        serde_json::from_slice(&fs::read(host.data.join(".access/links.json")).unwrap()).unwrap();
    assert_eq!(links.as_array().unwrap().len(), 1);
    assert_eq!(
        links[0]["expires"].as_u64().unwrap() - links[0]["created"].as_u64().unwrap(),
        24 * 3600
    );
    assert_eq!(host.get(&nobody(), &link).await.status, StatusCode::OK);
    let joined = host
        .post(
            &nobody(),
            &form_path(&link),
            &[
                ("name", "Late"),
                ("password", PASSWORD),
                ("password_confirm", PASSWORD),
            ],
        )
        .await;
    assert_eq!(joined.status, StatusCode::SEE_OTHER);
    assert_eq!(
        host.access.role_in(&host.id_of("late@example.test"), A),
        Some(OrgRole::Reader)
    );
    // A request is delivered once.
    assert!(
        operator_action(&host.state, OperatorAction::DispatchManual { id })
            .await
            .is_err()
    );
}

// ------------------------------------------------------------------------------ single mode

#[tokio::test]
async fn single_mode_still_opens_has_no_members_route_and_loads_state_without_invitations() {
    let dir = scratch();
    let access = Access::open(dir.path(), Visibility::Private, None).unwrap();
    assert!(!access.is_hosted());
    access
        .create_user(
            "Ann Admin",
            "ann@example.test",
            OrgRole::Admin,
            Some(PASSWORD),
        )
        .unwrap();
    drop(access);

    let path = dir.path().join(".access/state.json");
    let saved: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["invitations"], json!([]));

    // A file written before invitations existed has no such field.
    let mut older = saved.clone();
    older.as_object_mut().unwrap().remove("invitations");
    fs::write(&path, serde_json::to_vec(&older).unwrap()).unwrap();
    let access = Access::open(dir.path(), Visibility::Private, None).unwrap();
    assert_eq!(access.users().len(), 1);
    assert_eq!(access.users()[0].email, "ann@example.test");
    // Signed in, so a missing route answers 404 and not the sign-in redirect an anonymous
    // form post gets.
    let (cookie, csrf) = access.sign_in("ann@example.test", PASSWORD).unwrap();

    let app = build_app(AppState {
        storage: FsStorage::new(dir.path()).unwrap(),
        access,
        preview: None,
        public_origin: None,
        report_origin: None,
    });
    let get = |path: &str| {
        Request::builder()
            .uri(path.to_owned())
            .header(header::HOST, "app.test")
            .body(Body::empty())
            .unwrap()
    };
    assert_eq!(
        call(&app, get("/members")).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(call(&app, get("/")).await.status, StatusCode::OK);
    for path in ["/ui/orgs", "/ui/organization", "/ui/invitations/i-x/accept"] {
        let post = Request::builder()
            .method("POST")
            .uri(path)
            .header(header::HOST, "app.test")
            .header(header::COOKIE, format!("rv_session={cookie}"))
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(format!("csrf={csrf}&name=Y&org=z")))
            .unwrap();
        assert_eq!(
            call(&app, post).await.status,
            StatusCode::NOT_FOUND,
            "{path}"
        );
    }
}

// ------------------------------------------------------------------------------ deleting a person

/// The operator API only exists inside the server binary, so these tests run it as a child
/// process. The binary refuses to start without the staged docs runtime, hence the feature.
#[cfg(feature = "embedded-runtime")]
mod operator {
    use super::*;
    use std::process::{Child, Command, Stdio};

    const TOKEN: &str = "teams-operator-token-0123456789abcdef";

    struct Server {
        child: Child,
        operator: String,
        client: reqwest::Client,
    }

    impl Server {
        /// Start the server over `data` and wait until the operator API answers.
        async fn start(data: &Path, outbox: &Path) -> Self {
            let ports: Vec<_> = (0..3)
                .map(|_| std::net::TcpListener::bind("127.0.0.1:0").unwrap())
                .collect();
            let [app, report, operator] =
                [0, 1, 2].map(|i| ports[i].local_addr().unwrap().port().to_string());
            drop(ports);
            let child = Command::new(env!("CARGO_BIN_EXE_richview"))
                .args(["server", "--hosted", "--bind", "127.0.0.1"])
                .args(["--port", app.as_str(), "--report-port", report.as_str()])
                .args(["--operator-port", operator.as_str()])
                .args(["--public-url", "https://app.test"])
                .args(["--report-origin", "https://reports.test"])
                .args(["--quota-bytes", "104857600", "--data-dir"])
                .arg(data)
                .arg("--mail-outbox")
                .arg(outbox)
                .env("RICHVIEW_OPERATOR_TOKEN", TOKEN)
                .env_remove("RICHVIEW_TOKEN")
                .env_remove("RICHVIEW_ADMIN_EMAIL")
                .env_remove("RICHVIEW_SMTP_HOST")
                .env_remove("RICHVIEW_DEFAULT_VISIBILITY")
                .env_remove("RICHVIEW_MAIL_DAILY_LIMIT")
                .env_remove("RICHVIEW_SIGNUP_ACCOUNT_LIMIT")
                .env_remove("RICHVIEW_ORG_QUOTA_BYTES")
                .env_remove("RICHVIEW_ORG_REPORT_LIMIT")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let server = Self {
                child,
                operator: format!("http://127.0.0.1:{operator}"),
                client: reqwest::Client::builder()
                    .no_proxy()
                    .timeout(Duration::from_secs(60))
                    .build()
                    .unwrap(),
            };
            for _ in 0..300 {
                let ready = server
                    .client
                    .get(format!("{}/operator/organizations", server.operator))
                    .bearer_auth(TOKEN)
                    .send()
                    .await;
                if ready.is_ok_and(|r| r.status() == StatusCode::OK) {
                    return server;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            panic!("the server did not start");
        }

        async fn delete(&self, person: &str) -> (StatusCode, Value) {
            let reply = self
                .client
                .delete(format!("{}/operator/people/{person}", self.operator))
                .bearer_auth(TOKEN)
                .send()
                .await
                .unwrap();
            let status = reply.status();
            (status, reply.json().await.unwrap_or(Value::Null))
        }

        /// Stop the process we started, by its handle, and release the data directory.
        fn stop(mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    impl Drop for Server {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    fn state_of(data: &Path) -> Value {
        serde_json::from_slice(&fs::read(data.join(".access/state.json")).unwrap()).unwrap()
    }

    /// Seed a data directory, run the server over it, and hand back what a restart sees.
    async fn seeded(seed: Seed) -> (tempfile::TempDir, PathBuf, Server) {
        let dir = scratch();
        let data = dir.path().join("data");
        seed.persist(&data);
        let server = Server::start(&data, &dir.path().join("outbox")).await;
        (dir, data, server)
    }

    #[tokio::test]
    async fn deleting_a_member_of_a_shared_organization_removes_their_ties_and_hands_their_reports_to_the_longest_standing_owner()
     {
        let invitation = |id: &str, to: &str, by: &str, key: &str| {
            json!({"id": id, "org": A, "email": to, "report": key, "report_role": "viewer",
                "inviter": by, "created": 1, "expires": 4_102_444_800u64})
        };
        let seed = Seed::default()
            .org(A, "Acme")
            .person("own", "Olivia Owner")
            .person("own2", "Owen Later")
            .person("victim", "Vic Victim")
            .person("rdr", "Rita Reader")
            // The earlier owner is listed second, so the order of the list cannot decide.
            .member_at("own2", A, "owner", 9)
            .member_at("own", A, "owner", 1)
            .member_at("rdr", A, "reader", 3)
            .member_at("victim", A, "author", 4)
            .token("victim", A)
            .report("aaaa0001/vr", "victim", "org")
            .report("aaaa0001/shared", "own", "private")
            .grant("aaaa0001/shared", "victim", "viewer")
            .report("aaaa0001/kept", "own2", "org")
            .invitation(invitation(
                "i-by-victim",
                "friend@example.test",
                "victim",
                "aaaa0001/vr",
            ))
            .invitation(invitation(
                "i-to-victim",
                "victim@example.test",
                "own2",
                "aaaa0001/kept",
            ))
            .invitation(invitation(
                "i-other",
                "bystander@example.test",
                "own2",
                "aaaa0001/kept",
            ));
        let (dir, data, server) = seeded(seed).await;

        let (status, body) = server.delete("victim").await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["deleted"], true);
        assert_eq!(body["orgs"], json!([]));
        server.stop();

        let host = Host::open(dir, data, Mail::Files);
        assert!(host.access.user("victim").is_none());
        assert_eq!(host.access.role_in("victim", A), None);
        assert!(matches!(
            host.access.authenticate(Some(&token_of("victim", A)), None),
            Principal::Anonymous
        ));
        assert!(matches!(
            host.access.authenticate(None, Some("session-victim")),
            Principal::Anonymous
        ));
        let ids: Vec<_> = host
            .invitations()
            .iter()
            .map(|i| i["id"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(ids, ["i-other"], "sent by them or addressed to them");
        assert!(host.access.acl("aaaa0001/shared").grants.is_empty());
        assert_eq!(host.access.acl("aaaa0001/vr").owner, "own");
        assert!(host.access.organization(A).is_some());

        let page = host.get(&me("rdr"), "/reports/aaaa0001/vr").await;
        assert_eq!(page.status, StatusCode::OK);
        assert!(page.body.contains("Olivia Owner"));
        assert!(!page.body.contains("A former member"));
        assert!(
            host.get(&me("rdr"), "/")
                .await
                .body
                .contains("href=\"/reports/aaaa0001/vr\"")
        );
    }

    #[tokio::test]
    async fn deleting_the_only_owner_of_an_organization_with_other_members_is_refused_and_changes_nothing()
     {
        let seed = Seed::default()
            .org(A, "Acme")
            .person("solo", "Sol Owner")
            .person("mate", "Mat Mate")
            .member("solo", A, "owner")
            .member("mate", A, "author")
            .token("solo", A)
            .report("aaaa0001/s", "solo", "org")
            .report("aaaa0001/m", "mate", "org");
        let (dir, data, server) = seeded(seed).await;
        let before = state_of(&data);

        let (status, body) = server.delete("solo").await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(
            body["error"].as_str().unwrap().contains("last owner"),
            "{body}"
        );
        assert_eq!(state_of(&data), before);
        server.stop();

        let host = Host::open(dir, data, Mail::Files);
        for who in ["solo", "mate"] {
            assert!(!host.access.user(who).unwrap().disabled, "{who}");
            let list = host.get(&me(who), "/").await;
            assert_eq!(list.status, StatusCode::OK, "{who}");
            assert!(list.body.contains("href=\"/reports/aaaa0001/s\""), "{who}");
            assert!(list.body.contains("href=\"/reports/aaaa0001/m\""), "{who}");
        }
        assert!(matches!(
            host.access.authenticate(Some(&token_of("solo", A)), None),
            Principal::User(_)
        ));
    }

    #[tokio::test]
    async fn deleting_the_sole_member_of_an_organization_removes_it_from_disk_and_leaves_the_other_one()
     {
        const V: &str = "vvvv0004";
        let seed = Seed::default()
            .org(A, "Acme")
            .org(V, "Vic Studio")
            .person("vic", "Vic Victim")
            .person("own", "Olivia Owner")
            .person("rdr", "Rita Reader")
            .member("own", A, "owner")
            .member("rdr", A, "reader")
            .member("vic", A, "author")
            .member("vic", V, "owner")
            .report("vvvv0004/mine", "vic", "private")
            .report("aaaa0001/theirs", "vic", "org")
            .report("aaaa0001/base", "own", "org");
        let (dir, data, server) = seeded(seed).await;
        assert!(data.join("orgs/vvvv0004").exists());

        let (status, body) = server.delete("vic").await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["orgs"], json!([V]));
        server.stop();

        assert!(!data.join("orgs/vvvv0004").exists());
        assert!(data.join("orgs/aaaa0001/theirs").exists());
        assert!(data.join("orgs/aaaa0001/base").exists());
        let host = Host::open(dir, data, Mail::Files);
        assert!(host.access.organization(V).is_none());
        assert!(!host.access.has_record("vvvv0004/mine"));
        assert!(host.access.organization(A).is_some());
        let mut members: Vec<_> = host.state_json()["memberships"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| {
                (
                    m["person"].as_str().unwrap().to_owned(),
                    m["org"].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        members.sort();
        assert_eq!(
            members,
            [
                ("own".to_owned(), A.to_owned()),
                ("rdr".to_owned(), A.to_owned())
            ]
        );
        assert_eq!(host.access.acl("aaaa0001/theirs").owner, "own");
        let page = host.get(&me("rdr"), "/reports/aaaa0001/theirs").await;
        assert_eq!(page.status, StatusCode::OK);
        assert!(page.body.contains("Olivia Owner"));
        let list = host.get(&me("rdr"), "/").await;
        assert!(list.body.contains("href=\"/reports/aaaa0001/theirs\""));
        assert!(list.body.contains("href=\"/reports/aaaa0001/base\""));
    }
}

// ------------------------------------------------------------------------------ review of 12 November
// Each test below is one of the security reviewer's sequences, step for step.

#[tokio::test]
async fn a_removed_member_does_not_get_a_report_back_through_a_share_that_was_still_open() {
    let host = sharing().start(Mail::Files);
    // The author shares a private report with a colleague's address.
    let shared = host
        .post(
            &me("author"),
            "/ui/reports/aaaa0001/q3/people",
            &[("email", "reader@example.test"), ("role", "viewer")],
        )
        .await;
    assert_eq!(shared.status, StatusCode::SEE_OTHER);
    assert_eq!(host.invitations().len(), 1);
    let link = host
        .wait_letters("reader@example.test", 1)
        .await
        .remove(0)
        .link();

    // Before the colleague opens their list, an admin removes them from the organization.
    let removed = host
        .post(&me("admin"), "/ui/orgs/aaaa0001/members/reader/remove", &[])
        .await;
    assert_eq!(removed.status, StatusCode::SEE_OTHER);
    assert!(host.access.role_in("reader", A).is_none());
    assert!(host.invitations().is_empty());

    // Their list, the report page and the mail link from before give them nothing.
    let reader = me("reader");
    let list = host.get(&reader, "/").await;
    assert_eq!(list.status, StatusCode::OK);
    assert!(!list.body.contains("aaaa0001/q3"));
    assert_eq!(
        host.get(&reader, "/reports/aaaa0001/q3").await.status,
        StatusCode::NOT_FOUND
    );
    assert_gone(&host.get(&nobody(), &link).await);
    assert!(host.access.acl("aaaa0001/q3").grants.is_empty());
}

#[tokio::test]
async fn replacing_access_through_the_api_cancels_the_open_shares_it_does_not_name() {
    let host = sharing().start(Mail::Files);
    let author = token_of("author", A);
    for (address, role) in [
        ("known@example.test", "editor"),
        ("kept@example.test", "viewer"),
    ] {
        host.post(
            &me("author"),
            "/ui/reports/aaaa0001/q3/people",
            &[("email", address), ("role", role)],
        )
        .await;
    }
    let link = host
        .wait_letters("known@example.test", 1)
        .await
        .remove(0)
        .link();

    // The open shares are part of who can open the report, so the API shows them.
    let before = host
        .api(&author, "GET", "/api/reports/aaaa0001/q3/access", None)
        .await;
    assert_eq!(before.status, StatusCode::OK, "{}", before.body);
    assert_eq!(
        before.json()["pending"],
        json!([
            {"email": "kept@example.test", "role": "viewer"},
            {"email": "known@example.test", "role": "editor"}
        ])
    );

    // A full replacement that names one of them: the other is cancelled.
    let replaced = host
        .api(
            &author,
            "PUT",
            "/api/reports/aaaa0001/q3/access",
            Some(json!({"visibility": "private", "grants": [],
                "pending": [{"email": "kept@example.test"}]})),
        )
        .await;
    assert_eq!(replaced.status, StatusCode::OK, "{}", replaced.body);
    let after = host
        .api(&author, "GET", "/api/reports/aaaa0001/q3/access", None)
        .await
        .json();
    assert_eq!(after["grants"], json!([]));
    assert_eq!(
        after["pending"],
        json!([{"email": "kept@example.test", "role": "viewer"}])
    );

    // The recipient's next visit and the mail link from before recreate nothing.
    let known = me("known");
    assert!(!host.get(&known, "/").await.body.contains("aaaa0001/q3"));
    assert_gone(&host.get(&nobody(), &link).await);
    let spent = host.post(&nobody(), &form_path(&link), &[]).await;
    assert_ne!(spent.status, StatusCode::SEE_OTHER);
    assert_eq!(
        host.get(&known, "/reports/aaaa0001/q3").await.status,
        StatusCode::NOT_FOUND
    );
    assert!(host.access.acl("aaaa0001/q3").grants.is_empty());

    // The reviewer's exact body, naming nothing: no share is left.
    let emptied = host
        .api(
            &author,
            "PUT",
            "/api/reports/aaaa0001/q3/access",
            Some(json!({"visibility": "private", "grants": []})),
        )
        .await;
    assert_eq!(emptied.status, StatusCode::OK, "{}", emptied.body);
    assert_eq!(emptied.json()["pending"], json!([]));
    assert!(host.invitations().is_empty());
}

#[tokio::test]
async fn an_open_share_never_raises_a_grant_that_was_given_directly() {
    let host = sharing().start(Mail::Files);
    let author = token_of("author", A);
    host.post(
        &me("author"),
        "/ui/reports/aaaa0001/q3/people",
        &[("email", "known@example.test"), ("role", "editor")],
    )
    .await;
    // The owner then names the person as a viewer and leaves the open share in place.
    let granted = host
        .api(
            &author,
            "PUT",
            "/api/reports/aaaa0001/q3/access",
            Some(json!({"visibility": "private",
                "grants": [{"user": "known", "role": "viewer"}],
                "pending": [{"email": "known@example.test"}]})),
        )
        .await;
    assert_eq!(granted.status, StatusCode::OK, "{}", granted.body);

    let list = host.get(&me("known"), "/").await;
    assert!(list.body.contains("aaaa0001/q3"));
    let grants = host.access.acl("aaaa0001/q3").grants;
    assert_eq!(grants.len(), 1);
    assert_eq!(
        (grants[0].user.as_str(), grants[0].role),
        ("known", ReportRole::Viewer)
    );
    assert!(host.invitations().is_empty());
}

/// An open share of `key` for `address`, seeded straight into the store.
fn open_share(id: &str, key: &str, address: &str) -> Value {
    json!({"id": id, "org": A, "email": address, "report": key, "report_role": "viewer",
        "inviter": "author", "created": 1, "expires": 4_102_444_800u64})
}

#[tokio::test]
async fn open_shares_count_against_the_limits_of_named_people() {
    // 256 open shares across eight reports fill the organization; a ninth report is empty.
    let mut seed = team().token("author", A);
    for report in 1..=9 {
        seed = seed.report(&format!("aaaa0001/r{report}"), "author", "private");
    }
    for person in 1..=32 {
        let id = format!("g{person:02}");
        seed = seed.person(&id, &id);
    }
    for report in 1..=8 {
        for share in 1..=32 {
            // One of them is addressed to a person with an account.
            let address = if (report, share) == (1, 1) {
                "outsider@example.test".to_owned()
            } else {
                format!("p{report}x{share:02}@example.test")
            };
            seed = seed.invitation(open_share(
                &format!("i-r{report}s{share:02}"),
                &format!("aaaa0001/r{report}"),
                &address,
            ));
        }
    }
    let host = seed.start(Mail::Files);
    let author = token_of("author", A);
    let direct = |count: usize| -> Vec<Value> {
        (1..=count)
            .map(|person| json!({"user": format!("g{person:02}"), "role": "viewer"}))
            .collect()
    };
    let put = |key: &'static str, body: Value| {
        let host = &host;
        let author = author.clone();
        async move {
            host.api(
                &author,
                "PUT",
                &format!("/api/reports/aaaa0001/{key}/access"),
                Some(body),
            )
            .await
        }
    };

    // The reviewer's first sequence: 32 direct grants on the ninth report, then even one.
    for count in [32, 1] {
        let refused = put(
            "r9",
            json!({"visibility": "private", "grants": direct(count)}),
        )
        .await;
        assert_eq!(
            refused.status,
            StatusCode::BAD_REQUEST,
            "{count}: {}",
            refused.body
        );
        assert!(refused.body.contains("limit"), "{}", refused.body);
    }
    assert!(host.access.acl("aaaa0001/r9").grants.is_empty());

    // The second: 32 direct grants on a report whose 32 open shares stay.
    let keep: Vec<Value> = (2..=32)
        .map(|share| json!({"email": format!("p2x{share:02}@example.test")}))
        .chain([json!({"email": "p2x01@example.test"})])
        .collect();
    let refused = put(
        "r2",
        json!({"visibility": "private", "grants": direct(32), "pending": keep}),
    )
    .await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{}", refused.body);
    assert!(host.access.acl("aaaa0001/r2").grants.is_empty());
    assert_eq!(host.invitations().len(), 256);

    // Accepting an offer moves it from open to granted and counts it once, so it fits even
    // with the organization full.
    let list = host.get(&me("outsider"), "/").await;
    assert!(list.body.contains("aaaa0001/r1"), "{}", list.body);
    assert_eq!(host.access.acl("aaaa0001/r1").grants.len(), 1);
    assert_eq!(host.invitations().len(), 255);

    // Replacing the shares of a report with grants frees their room: 32 for 32.
    let swapped = put("r2", json!({"visibility": "private", "grants": direct(32)})).await;
    assert_eq!(swapped.status, StatusCode::OK, "{}", swapped.body);
    assert_eq!(host.access.acl("aaaa0001/r2").grants.len(), 32);
    assert_eq!(host.invitations().len(), 223);
}

#[tokio::test]
async fn an_expired_invitation_or_share_leaves_the_store_with_the_next_write() {
    let expired = |id: &str, address: &str, report: Option<&str>| {
        let mut invitation = json!({"id": id, "org": A, "email": address, "inviter": "owner",
            "created": 1, "expires": 2});
        match report {
            Some(key) => {
                invitation["report"] = json!(key);
                invitation["report_role"] = json!("viewer");
            }
            None => invitation["role"] = json!("reader"),
        }
        invitation
    };
    let host = sharing()
        .invitation(expired("i-old-member", "lapsed-member@example.test", None))
        .invitation(expired(
            "i-old-share",
            "lapsed-share@example.test",
            Some("aaaa0001/orgwide"),
        ))
        .invitation(open_share(
            "i-open",
            "aaaa0001/q3",
            "still-open@example.test",
        ))
        .start(Mail::Files);
    let stored = || fs::read_to_string(host.data.join(".access/state.json")).unwrap();

    // Any write will do: here a colleague's role changes.
    let changed = host
        .post(
            &me("owner"),
            "/ui/orgs/aaaa0001/members/reader/role",
            &[("role", "author")],
        )
        .await;
    assert_eq!(changed.status, StatusCode::SEE_OTHER);

    let after = stored();
    for address in ["lapsed-member@example.test", "lapsed-share@example.test"] {
        assert!(!after.contains(address), "{address} is still stored");
    }
    assert!(after.contains("still-open@example.test"));
    assert_eq!(host.invitations().len(), 1);
}

#[tokio::test]
async fn expired_invitations_leave_the_store_at_start_and_when_the_sweep_runs_without_any_write() {
    let soon = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 4;
    let host = sharing()
        .invitation(
            json!({"id": "i-lapsed", "org": A, "email": "lapsed@example.test",
            "role": "reader", "inviter": "owner", "created": 1, "expires": 2}),
        )
        .invitation(
            json!({"id": "i-soon", "org": A, "email": "about-to-lapse@example.test",
            "report": "aaaa0001/q3", "report_role": "viewer", "inviter": "author",
            "created": 1, "expires": soon}),
        )
        .start(Mail::Files);
    let stored = || fs::read_to_string(host.data.join(".access/state.json")).unwrap();

    // Opening the store removed what had expired while the server was down.
    assert!(!stored().contains("lapsed@example.test"));
    assert!(stored().contains("about-to-lapse@example.test"));
    assert_eq!(host.access.purge_expired_invitations().unwrap(), 0);

    // The clock passes the other one's expiry; nobody writes anything; the sweep's call
    // removes it.
    while std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        <= soon
    {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(host.access.purge_expired_invitations().unwrap(), 1);
    assert!(!stored().contains("about-to-lapse@example.test"));
    assert!(host.invitations().is_empty());
}

#[tokio::test]
async fn the_sweep_writes_nothing_when_no_invitation_has_expired() {
    let host = sharing()
        .invitation(open_share(
            "i-open",
            "aaaa0001/q3",
            "still-open@example.test",
        ))
        .start(Mail::Files);
    let file = host.data.join(".access/state.json");
    let written = || fs::metadata(&file).unwrap().modified().unwrap();
    let before = (written(), fs::read(&file).unwrap());
    for _ in 0..3 {
        assert_eq!(host.access.purge_expired_invitations().unwrap(), 0);
    }
    assert_eq!((written(), fs::read(&file).unwrap()), before);
}
