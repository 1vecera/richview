//! Sign-in with GitHub, Microsoft and Google, end to end. The real hosted router runs over a real store; a
//! test browser keeps cookies the way a browser does (the session cookie is `SameSite=Strict`, so it
//! stays home when the provider's answer arrives); the three providers are loopback stand-ins that
//! check what the server sends them. No test talks to a real provider.
use axum::{
    Form, Json, Router,
    body::{Body, to_bytes},
    extract::{Path, Query, State},
    http::{HeaderMap, Request, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use richview_app::{
    access::{Access, AccessError, LinkPurpose},
    mail::{Mail, Transport},
    server::{
        AppState, build_app,
        hosted::{OperatorAction, operator_action},
    },
    social::{GOOGLE_CONSUMER_DOMAIN, Settings},
    storage::FsStorage,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    future::Future,
    pin::Pin,
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tower::ServiceExt;

/// The server's own address. Nothing listens there: the router is driven directly, and the
/// address only has to be on loopback, which a stand-in requires of it.
const ORIGIN: &str = "http://127.0.0.1:18080";
const HOST: &str = "127.0.0.1:18080";
const REPORTS: &str = "http://localhost:18081";
const PASSWORD: &str = "a-long-password";
const GH_ID: &str = "github-client-id";
const GH_SECRET: &str = "github-secret-canary";
const MS_ID: &str = "microsoft-client-id";
const MS_SECRET: &str = "microsoft-secret-canary";
const GO_ID: &str = "google-client-id";
const GO_SECRET: &str = "google-secret-canary";
const TENANT: &str = "6f1c9f2e-3b0e-4d57-9d1e-0a1b2c3d4e5f";
const OTHER_TENANT: &str = "0a0b0c0d-1111-4222-8333-444455556666";
const CONSUMER_TENANT: &str = "9188040d-6c67-4c5b-b112-36a304b66dad";

const ADA: &str = "ada@example.test";
const STALE: &str = "This sign-in can’t be finished";
const NOT_CONFIRMED: &str = "did not confirm your account";
const NO_ADDRESS: &str = "We couldn’t confirm your address";

// ------------------------------------------------------------------------------ providers

#[derive(Clone, Copy, PartialEq, Debug)]
enum P {
    GitHub,
    Microsoft,
    Google,
}

impl P {
    fn slug(self) -> &'static str {
        match self {
            P::GitHub => "github",
            P::Microsoft => "microsoft",
            P::Google => "google",
        }
    }

    fn name(self) -> &'static str {
        match self {
            P::GitHub => "GitHub",
            P::Microsoft => "Microsoft",
            P::Google => "Google",
        }
    }
}

/// A person at a provider.
#[derive(Clone, Copy)]
struct Who {
    /// GitHub's numeric id; the last part of Microsoft's object id; Google's `sub`, padded.
    n: u32,
    login: &'static str,
    name: &'static str,
    /// The address the provider reports, and whether it vouches for it.
    email: Option<&'static str>,
    vouched: bool,
}

const ADA_AT_PROVIDER: Who = Who {
    n: 4242,
    login: "ada-hub",
    name: "Ada Hub",
    email: Some(ADA),
    vouched: true,
};

fn oid(n: u32) -> String {
    format!("{n:08x}-aaaa-4bbb-8ccc-ddddeeeeffff")
}

fn google_sub(n: u32) -> String {
    format!("10{n:019}")
}

/// What the server keeps as the identity: the provider's own id for the account.
fn subject_of(p: P, who: Who) -> String {
    match p {
        P::GitHub => who.n.to_string(),
        P::Microsoft => format!("{TENANT}:{}", oid(who.n)),
        P::Google => google_sub(who.n),
    }
}

/// What the Account page calls it.
fn label_of(p: P, who: Who) -> String {
    match p {
        P::GitHub => who.login.to_owned(),
        P::Microsoft => who.email.unwrap_or(who.login).to_owned(),
        P::Google => who.email.unwrap_or("Google account").to_owned(),
    }
}

#[derive(Clone)]
struct GitHubUser {
    id: u64,
    login: String,
    name: String,
    emails: Value,
}

#[derive(Clone)]
struct Grant {
    challenge: String,
    nonce: String,
    github: Option<GitHubUser>,
    microsoft: Value,
    google: Value,
    used: bool,
}

#[derive(Default)]
struct Provided {
    github: Option<GitHubUser>,
    /// Claims that replace or add to a Microsoft ID token's; `null` removes one.
    microsoft: Value,
    /// The same for a Google ID token.
    google: Value,
    /// A description the next authorize answers with as an error, instead of a code.
    deny: Option<String>,
    /// The challenge an attacker substituted: the stand-in remembers this one, not the server's.
    swapped_challenge: Option<String>,
    grants: HashMap<String, Grant>,
    minted: u32,
    token_calls: u32,
    refused_tokens: u32,
}

type Shared = Arc<Mutex<Provided>>;

struct Stand {
    base: String,
    shared: Shared,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Stand {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn refuse(provider: &str, error: &str) -> Response {
    // GitHub answers a refused code with 200 and an error; Microsoft with 400.
    let status = if provider == "github" {
        StatusCode::OK
    } else {
        StatusCode::BAD_REQUEST
    };
    (status, Json(json!({ "error": error }))).into_response()
}

/// The provider's sign-in page, as far as the server's request matters: it checks everything the
/// server sends and answers as the person had signed in.
async fn authorize(
    State(shared): State<Shared>,
    Path(provider): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let get = |key: &str| q.get(key).map(String::as_str).unwrap_or_default();
    let (client, scope_ok) = match provider.as_str() {
        "github" => (GH_ID, get("scope") == "user:email"),
        "microsoft" => (
            MS_ID,
            get("scope") == "openid profile email"
                && get("response_type") == "code"
                && get("response_mode") == "query"
                && !get("nonce").is_empty(),
        ),
        "google" => (
            GO_ID,
            get("scope") == "openid email profile"
                && get("response_type") == "code"
                && !get("nonce").is_empty(),
        ),
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    let redirect = format!("{ORIGIN}/sign-in/{provider}/callback");
    if get("client_id") != client
        || !scope_ok
        || get("redirect_uri") != redirect
        || get("code_challenge_method") != "S256"
        || get("code_challenge").is_empty()
        || get("state").is_empty()
    {
        return (
            StatusCode::BAD_REQUEST,
            format!("stand-in: refused the authorize request {q:?}"),
        )
            .into_response();
    }
    let mut provided = shared.lock().unwrap();
    let target = if let Some(description) = provided.deny.clone() {
        format!(
            "{redirect}?error=access_denied&error_description={description}&state={}",
            get("state")
        )
    } else {
        provided.minted += 1;
        let code = format!("code-{}", provided.minted);
        let challenge = provided
            .swapped_challenge
            .take()
            .unwrap_or_else(|| get("code_challenge").to_owned());
        let grant = Grant {
            challenge,
            nonce: get("nonce").to_owned(),
            github: provided.github.clone(),
            microsoft: provided.microsoft.clone(),
            google: provided.google.clone(),
            used: false,
        };
        provided.grants.insert(code.clone(), grant);
        format!("{redirect}?code={code}&state={}", get("state"))
    };
    (StatusCode::FOUND, [(header::LOCATION, target)]).into_response()
}

fn id_token(provider: &str, grant: &Grant) -> String {
    let now = unix_now();
    let (mut claims, changes) = if provider == "google" {
        (
            json!({
                "iss": "https://accounts.google.com",
                "aud": GO_ID,
                "exp": now + 3600,
                "iat": now - 60,
                "nonce": grant.nonce,
            }),
            &grant.google,
        )
    } else {
        (
            json!({
                "iss": format!("https://login.microsoftonline.com/{TENANT}/v2.0"),
                "aud": MS_ID,
                "exp": now + 3600,
                "nbf": now - 60,
                "iat": now - 60,
                "nonce": grant.nonce,
                "tid": TENANT,
            }),
            &grant.microsoft,
        )
    };
    for (key, value) in changes.as_object().into_iter().flatten() {
        if value.is_null() {
            claims.as_object_mut().unwrap().remove(key);
        } else {
            claims[key.as_str()] = value.clone();
        }
    }
    let part = |value: &Value| URL_SAFE_NO_PAD.encode(serde_json::to_vec(value).unwrap());
    format!(
        "{}.{}.{}",
        part(&json!({"typ": "JWT", "alg": "RS256"})),
        part(&claims),
        URL_SAFE_NO_PAD.encode("not-a-signature")
    )
}

/// The token endpoint: client authentication, a code used once, and PKCE.
async fn token(
    State(shared): State<Shared>,
    Path(provider): Path<String>,
    Form(f): Form<HashMap<String, String>>,
) -> Response {
    let get = |key: &str| f.get(key).map(String::as_str).unwrap_or_default();
    let (client, secret) = match provider.as_str() {
        "github" => (GH_ID, GH_SECRET),
        "microsoft" => (MS_ID, MS_SECRET),
        "google" => (GO_ID, GO_SECRET),
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    let mut provided = shared.lock().unwrap();
    provided.token_calls += 1;
    if get("client_id") != client
        || get("client_secret") != secret
        || get("redirect_uri") != format!("{ORIGIN}/sign-in/{provider}/callback")
        || (provider != "github" && get("grant_type") != "authorization_code")
    {
        provided.refused_tokens += 1;
        return refuse(&provider, "invalid_client");
    }
    let code = get("code").to_owned();
    let found = provided
        .grants
        .get_mut(&code)
        .filter(|grant| !grant.used)
        .map(|grant| {
            grant.used = true;
            grant.clone()
        });
    let Some(grant) = found else {
        provided.refused_tokens += 1;
        return refuse(&provider, "invalid_grant");
    };
    let proof = URL_SAFE_NO_PAD.encode(Sha256::digest(get("code_verifier").as_bytes()));
    if proof != grant.challenge {
        provided.refused_tokens += 1;
        return refuse(&provider, "invalid_grant");
    }
    if provider == "github" {
        Json(json!({
            "access_token": format!("gho_stand_in_{code}"),
            "token_type": "bearer",
            "scope": "user:email",
        }))
        .into_response()
    } else {
        Json(json!({
            "id_token": id_token(&provider, &grant),
            "access_token": format!("stand-in-{provider}-access"),
            "token_type": "Bearer",
            "expires_in": 3600,
        }))
        .into_response()
    }
}

/// The GitHub user of the access token the request carries.
fn github_user_of(shared: &Shared, headers: &HeaderMap) -> Option<GitHubUser> {
    let bearer = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let code = bearer.strip_prefix("Bearer gho_stand_in_")?;
    headers.get(header::USER_AGENT)?;
    shared.lock().unwrap().grants.get(code)?.github.clone()
}

async fn github_user(State(shared): State<Shared>, headers: HeaderMap) -> Response {
    match github_user_of(&shared, &headers) {
        Some(user) => {
            Json(json!({"id": user.id, "login": user.login, "name": user.name})).into_response()
        }
        None => StatusCode::UNAUTHORIZED.into_response(),
    }
}

async fn github_emails(State(shared): State<Shared>, headers: HeaderMap) -> Response {
    match github_user_of(&shared, &headers) {
        Some(user) => Json(user.emails).into_response(),
        None => StatusCode::UNAUTHORIZED.into_response(),
    }
}

impl Stand {
    async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let shared = Shared::default();
        let app = Router::new()
            .route("/{provider}/authorize", get(authorize))
            .route("/{provider}/token", post(token))
            .route("/github/user", get(github_user))
            .route("/github/user/emails", get(github_emails))
            .with_state(shared.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self { base, shared, task }
    }

    /// The next person to sign in at `p`.
    fn provide(&self, p: P, who: Who) {
        let mut provided = self.shared.lock().unwrap();
        match p {
            P::GitHub => {
                let emails = who.email.map_or(json!([]), |email| {
                    json!([
                        {"email": "99+noreply@users.noreply.github.com", "primary": false, "verified": true},
                        {"email": email, "primary": true, "verified": who.vouched},
                    ])
                });
                provided.github = Some(GitHubUser {
                    id: u64::from(who.n),
                    login: who.login.to_owned(),
                    name: who.name.to_owned(),
                    emails,
                });
            }
            P::Microsoft => {
                let mut claims = json!({
                    "oid": oid(who.n),
                    "name": who.name,
                    "preferred_username": who.login,
                });
                if let Some(email) = who.email {
                    claims["email"] = json!(email);
                    if who.vouched {
                        claims["xms_edov"] = json!(true);
                    }
                }
                provided.microsoft = claims;
            }
            P::Google => {
                let mut claims = json!({"sub": google_sub(who.n), "name": who.name});
                if let Some(email) = who.email {
                    claims["email"] = json!(email);
                    claims["email_verified"] = json!(who.vouched);
                    // A Workspace account, the kind Google vouches for beside Gmail.
                    if !email.ends_with(&format!("@{GOOGLE_CONSUMER_DOMAIN}")) {
                        claims["hd"] = json!(email.rsplit('@').next().unwrap_or_default());
                    }
                }
                provided.google = claims;
            }
        }
    }

    /// Changes one claim of the next Microsoft ID token; `null` leaves it out.
    fn claim(&self, key: &str, value: Value) {
        self.shared.lock().unwrap().microsoft[key] = value;
    }

    /// Changes one claim of the next Google ID token; `null` leaves it out.
    fn google_claim(&self, key: &str, value: Value) {
        self.shared.lock().unwrap().google[key] = value;
    }

    /// Replaces the GitHub address list of the next sign-in.
    fn github_emails(&self, emails: Value) {
        self.shared
            .lock()
            .unwrap()
            .github
            .as_mut()
            .expect("provide a GitHub person first")
            .emails = emails;
    }

    fn deny(&self, description: &str) {
        self.shared.lock().unwrap().deny = Some(description.to_owned());
    }

    fn swap_challenge(&self, challenge: &str) {
        self.shared.lock().unwrap().swapped_challenge = Some(challenge.to_owned());
    }

    fn token_calls(&self) -> u32 {
        self.shared.lock().unwrap().token_calls
    }

    fn refused_tokens(&self) -> u32 {
        self.shared.lock().unwrap().refused_tokens
    }

    fn minted(&self) -> u32 {
        self.shared.lock().unwrap().minted
    }
}

// ------------------------------------------------------------------------------ the browser

/// A browser: the cookies it holds, by name, with the path each is scoped to, and its address.
#[derive(Clone)]
struct Browser {
    jar: BTreeMap<String, (String, String)>,
    ip: String,
    csrf: String,
}

static BROWSERS: AtomicUsize = AtomicUsize::new(1);

impl Browser {
    fn new() -> Self {
        let n = BROWSERS.fetch_add(1, Ordering::Relaxed);
        Self {
            jar: BTreeMap::new(),
            ip: format!("192.0.2.{}", 1 + n % 250),
            csrf: String::new(),
        }
    }

    fn cookie(&self, name: &str) -> Option<&str> {
        self.jar.get(name).map(|(value, _)| value.as_str())
    }

    /// The `Cookie` header for a request to `path`. A request that follows the provider's answer
    /// comes from another site, so the `SameSite=Strict` session cookie does not go with it.
    fn header_for(&self, path: &str, from_provider: bool) -> Option<String> {
        let path = path.split('?').next().unwrap_or(path);
        let parts: Vec<String> = self
            .jar
            .iter()
            .filter(|(name, (_, scope))| {
                !(from_provider && name.as_str() == "rv_session")
                    && path.starts_with(scope.as_str())
            })
            .map(|(name, (value, _))| format!("{name}={value}"))
            .collect();
        (!parts.is_empty()).then(|| parts.join("; "))
    }

    fn absorb(&mut self, headers: &HeaderMap) {
        for line in headers
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
        {
            let mut parts = line.split(';').map(str::trim);
            let Some((name, value)) = parts.next().and_then(|first| first.split_once('=')) else {
                continue;
            };
            let mut scope = "/".to_owned();
            let mut gone = value.is_empty();
            for attribute in parts {
                if let Some(path) = attribute.strip_prefix("Path=") {
                    scope = path.to_owned();
                }
                if attribute == "Max-Age=0" {
                    gone = true;
                }
            }
            if gone {
                self.jar.remove(name);
            } else {
                self.jar.insert(name.to_owned(), (value.to_owned(), scope));
            }
        }
    }
}

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: String,
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

fn enc(text: &str) -> String {
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

impl Reply {
    fn location(&self) -> Option<String> {
        self.headers
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    }

    /// Where a page that moves on by itself sends the browser.
    fn refresh(&self) -> Option<String> {
        self.headers
            .get("refresh")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("0; url="))
            .map(str::to_owned)
    }

    /// Where the browser goes next, by redirect or by a page that moves on.
    fn next(&self) -> String {
        self.location()
            .or_else(|| self.refresh())
            .unwrap_or_else(|| panic!("goes nowhere: {} {}", self.status, self.body))
    }

    /// The notice a redirect carries in `?m=`, decoded.
    fn notice(&self) -> String {
        let location = self.location().unwrap_or_default();
        let Some((_, query)) = location.split_once('?') else {
            return String::new();
        };
        query
            .split('&')
            .find_map(|pair| pair.strip_prefix("m="))
            .map(decode)
            .unwrap_or_default()
    }

    /// The whole `Set-Cookie` line for a cookie name.
    fn sets(&self, name: &str) -> Option<String> {
        self.headers
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .find(|line| line.starts_with(&format!("{name}=")))
            .map(str::to_owned)
    }

    fn opens_a_session(&self) -> bool {
        self.sets("rv_session")
            .is_some_and(|line| !line.starts_with("rv_session=;"))
    }
}

/// The value of a hidden form field.
fn hidden(body: &str, name: &str) -> String {
    let marker = format!("name=\"{name}\" value=\"");
    let from = body
        .find(&marker)
        .unwrap_or_else(|| panic!("no `{name}` field in {body}"))
        + marker.len();
    let to = from + body[from..].find('"').unwrap();
    body[from..to].to_owned()
}

// ------------------------------------------------------------------------------ mail

#[derive(Default)]
struct FakeMail(Mutex<Vec<Mail>>);

impl Transport for FakeMail {
    fn write_manual(&self, _request: &richview_app::mail::ManualRequest) -> Result<(), String> {
        Ok(())
    }

    fn send<'a>(
        &'a self,
        mail: &'a Mail,
    ) -> Pin<Box<dyn Future<Output = Result<(), ()>> + Send + 'a>> {
        Box::pin(async move {
            self.0.lock().unwrap().push(mail.clone());
            Ok(())
        })
    }
}

/// The path of the one link a mail carries.
fn link_in(mail: &Mail) -> String {
    mail.body
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{ORIGIN}/link/")))
        .map(|token| format!("/link/{token}"))
        .unwrap_or_else(|| panic!("no link in {}", mail.body))
}

fn form_path(link: &str) -> String {
    link.replacen("/link/", "/ui/link/", 1)
}

// ------------------------------------------------------------------------------ the fixture

struct Fixture {
    dir: tempfile::TempDir,
    state: AppState,
    app: Router,
    access: Access,
    mail: Arc<FakeMail>,
    stand: Stand,
    http: reqwest::Client,
}

impl Fixture {
    /// A hosted server with all three providers.
    async fn new() -> Self {
        Self::with(true, true, true).await
    }

    async fn with(github: bool, microsoft: bool, google: bool) -> Self {
        let (stand, dir) = (Stand::start().await, tempfile::tempdir().unwrap());
        Self::build(stand, dir, [github, microsoft, google], Some(1000))
    }

    /// A hosted server with all three providers and `mail_limit` mails a day, or no mail at all.
    async fn with_mail(mail_limit: Option<u64>) -> Self {
        let (stand, dir) = (Stand::start().await, tempfile::tempdir().unwrap());
        Self::build(stand, dir, [true; 3], mail_limit)
    }

    fn build(
        stand: Stand,
        dir: tempfile::TempDir,
        [github, microsoft, google]: [bool; 3],
        mail_limit: Option<u64>,
    ) -> Self {
        let access = Access::open_hosted(dir.path()).unwrap();
        let mail = Arc::new(FakeMail::default());
        if let Some(limit) = mail_limit {
            access.configure_mail(mail.clone(), limit, false).unwrap();
        }
        let social = Settings {
            github_id: github.then(|| GH_ID.to_owned()),
            github_secret: github.then(|| GH_SECRET.to_owned()),
            microsoft_id: microsoft.then(|| MS_ID.to_owned()),
            microsoft_secret: microsoft.then(|| MS_SECRET.to_owned()),
            google_id: google.then(|| GO_ID.to_owned()),
            google_secret: google.then(|| GO_SECRET.to_owned()),
            test_base: (github || microsoft || google).then(|| stand.base.clone()),
        }
        .build(Some(ORIGIN))
        .unwrap();
        if let Some(social) = social {
            access.configure_social(social).unwrap();
        }
        let state = AppState {
            storage: FsStorage::new(dir.path())
                .unwrap()
                .with_hosted(true)
                .with_quota(Some(100_000_000)),
            access: access.clone(),
            preview: None,
            public_origin: Some(ORIGIN.into()),
            report_origin: Some(REPORTS.into()),
        };
        Self {
            dir,
            app: build_app(state.clone()),
            state,
            access,
            mail,
            stand,
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
        }
    }

    // ---- requests

    async fn send(
        &self,
        b: &mut Browser,
        method: &str,
        path: &str,
        form: Option<&str>,
        from_provider: bool,
        origin: &str,
    ) -> Reply {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("host", HOST)
            .header("x-real-ip", &b.ip);
        if form.is_some() {
            request = request
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::ORIGIN, origin);
        }
        if let Some(cookie) = b.header_for(path, from_provider) {
            request = request.header(header::COOKIE, cookie);
        }
        let response = self
            .app
            .clone()
            .oneshot(
                request
                    .body(Body::from(form.unwrap_or_default().to_owned()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let body = String::from_utf8_lossy(
            &to_bytes(response.into_body(), 8 * 1024 * 1024)
                .await
                .unwrap(),
        )
        .into_owned();
        b.absorb(&headers);
        // No page, address or cookie ever carries a secret or a provider's token.
        let everything = format!("{headers:?}{body}");
        for secret in [
            GH_SECRET,
            MS_SECRET,
            GO_SECRET,
            "gho_stand_in",
            "stand-in-microsoft-access",
            "stand-in-google-access",
        ] {
            assert!(!everything.contains(secret), "{secret} leaked from {path}");
        }
        Reply {
            status,
            headers,
            body,
        }
    }

    async fn get(&self, b: &mut Browser, path: &str) -> Reply {
        self.send(b, "GET", path, None, false, ORIGIN).await
    }

    /// A form post; `@csrf` in the form stands for the browser's session's token.
    async fn post(&self, b: &mut Browser, path: &str, form: &str) -> Reply {
        let form = form.replace("@csrf", &enc(&b.csrf));
        self.send(b, "POST", path, Some(&form), false, ORIGIN).await
    }

    /// The provider's answer arriving in the browser: a navigation from another site.
    async fn arrive(&self, b: &mut Browser, path: &str) -> Reply {
        self.send(b, "GET", path, None, true, ORIGIN).await
    }

    // ---- people

    /// An account with a password, and no session.
    async fn person(&self, email: &str, name: &str) -> String {
        let link = self
            .access
            .issue_link(email, LinkPurpose::Signup)
            .unwrap()
            .unwrap();
        // Password hashing takes one lane per test process; on a busy machine with every test
        // of this file hashing at once, the wait can outlast its patience. That refusal is not
        // under test here, so the helper asks again.
        for _ in 0..5 {
            match self
                .access
                .complete_hosted_link(&link, name, PASSWORD)
                .await
            {
                Err(AccessError::Throttled) => continue,
                done => return done.unwrap().id,
            }
        }
        panic!("the hashing lane stayed busy for five waits");
    }

    /// Signs the browser in with the password, as the sign-in form does. Checking the password
    /// hashes too, so like `person` it asks again while the lane is busy.
    fn sign_in(&self, b: &mut Browser, email: &str) {
        for _ in 0..5 {
            match self.access.sign_in(email, PASSWORD) {
                Err(AccessError::Throttled) => continue,
                done => {
                    let (cookie, csrf) = done.unwrap();
                    b.jar.insert("rv_session".into(), (cookie, "/".into()));
                    b.csrf = csrf;
                    return;
                }
            }
        }
        panic!("the hashing lane stayed busy for five waits");
    }

    fn person_with(&self, email: &str) -> Option<richview_app::access::User> {
        self.access.users().into_iter().find(|u| u.email == email)
    }

    // ---- the provider

    /// Takes the browser where the server sent it and returns the path of the provider's answer.
    async fn provider_answer(&self, started: &Reply) -> String {
        let url = started.next();
        assert!(
            url.starts_with(&self.stand.base),
            "not going to the stand-in: {url}"
        );
        let response = self.http.get(&url).send().await.unwrap();
        let status = response.status();
        let location = response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let text = response.text().await.unwrap_or_default();
        assert_eq!(status, 302, "{text}");
        let location = location.unwrap();
        location
            .strip_prefix(ORIGIN)
            .unwrap_or_else(|| panic!("the provider answers elsewhere: {location}"))
            .to_owned()
    }

    /// Starts a plain sign-in at `p` and goes through the provider; the reply is the callback's.
    async fn sign_in_with(&self, b: &mut Browser, p: P, back: Option<&str>) -> (String, Reply) {
        let path = match back {
            Some(back) => format!("/sign-in/{}?back={}", p.slug(), enc(back)),
            None => format!("/sign-in/{}", p.slug()),
        };
        let started = self.get(b, &path).await;
        assert_eq!(started.status, StatusCode::SEE_OTHER, "{}", started.body);
        let answer = self.provider_answer(&started).await;
        let reply = self.arrive(b, &answer).await;
        (answer, reply)
    }

    /// A signed-in person connects `p`; the reply is the last step's.
    async fn connect(&self, b: &mut Browser, p: P) -> Reply {
        let started = self
            .post(b, &format!("/sign-in/{}/connect", p.slug()), "csrf=@csrf")
            .await;
        assert_eq!(started.status, StatusCode::OK, "{}", started.body);
        let answer = self.provider_answer(&started).await;
        let callback = self.arrive(b, &answer).await;
        assert_eq!(callback.status, StatusCode::OK, "{}", callback.body);
        self.get(b, &callback.next()).await
    }

    /// Joins `who` at `p` to the person `email` by the signed-in route, as an Account page does.
    async fn joined(&self, email: &str, p: P, who: Who) {
        self.stand.provide(p, who);
        let mut b = Browser::new();
        self.sign_in(&mut b, email);
        let done = self.connect(&mut b, p).await;
        assert_eq!(done.status, StatusCode::SEE_OTHER, "{}", done.body);
    }

    // ---- mail

    /// The mails to `who`, once at least `count` have arrived.
    async fn mail_to(&self, who: &str, count: usize) -> Vec<Mail> {
        let mails = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let mails: Vec<Mail> = self
                    .mail
                    .0
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|mail| mail.to == who)
                    .cloned()
                    .collect();
                if mails.len() >= count {
                    return mails;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        mails.unwrap_or_else(|_| panic!("fewer than {count} mails to {who}"))
    }

    /// No mail was sent, to anyone.
    async fn no_mail(&self) {
        tokio::time::sleep(Duration::from_millis(300)).await;
        let sent = self.mail.0.lock().unwrap();
        assert!(
            sent.is_empty(),
            "unexpected mail: {:?}",
            sent.iter().map(|m| (&m.to, &m.subject)).collect::<Vec<_>>()
        );
    }

    // ---- what is kept

    fn identities(&self, email: &str) -> Vec<richview_app::access::Identity> {
        let person = self.person_with(email).expect("no such person");
        self.access.identities_of(&person.id)
    }

    fn audit(&self) -> String {
        serde_json::to_string(&self.access.read_audit(200)).unwrap()
    }

    /// The access file as written.
    fn stored(&self) -> String {
        std::fs::read_to_string(self.dir.path().join(".access/state.json")).unwrap_or_default()
    }
}

/// Nothing was joined, created or signed in, and no mail went out.
async fn nothing_happened(fx: &Fixture, reply: &Reply, people: usize) {
    assert!(!reply.opens_a_session(), "a session was opened");
    assert_eq!(fx.access.users().len(), people, "an account was created");
    for user in fx.access.users() {
        assert!(
            fx.access.identities_of(&user.id).is_empty(),
            "an identity was joined"
        );
    }
    fx.no_mail().await;
}

// ------------------------------------------------------------------------------ connect, signed in

async fn connect_joins_whatever_the_address(p: P) {
    let fx = Fixture::new().await;
    let ada = fx.person(ADA, "Ada").await;
    // The provider reports another address entirely: the session is the proof, not the address.
    let who = Who {
        email: Some("someone-else@example.test"),
        ..ADA_AT_PROVIDER
    };
    fx.stand.provide(p, who);
    let mut b = Browser::new();
    fx.sign_in(&mut b, ADA);
    let started = fx
        .post(
            &mut b,
            &format!("/sign-in/{}/connect", p.slug()),
            "csrf=@csrf",
        )
        .await;
    assert_eq!(started.status, StatusCode::OK, "{}", started.body);
    let flow = started.sets("rv_oauth").expect("the flow cookie");
    for part in ["HttpOnly", "SameSite=Lax", "Path=/sign-in", "Max-Age=600"] {
        assert!(flow.contains(part), "{flow}");
    }
    assert!(!flow.contains("Secure"), "plain HTTP on loopback: {flow}");
    // The answer comes from the provider's site: the session cookie does not come with it, and
    // nothing is joined until the browser returns to this site by itself.
    let answer = fx.provider_answer(&started).await;
    let callback = fx.arrive(&mut b, &answer).await;
    assert_eq!(callback.status, StatusCode::OK, "{}", callback.body);
    assert!(fx.access.identities_of(&ada).is_empty());
    let finish = callback.next();
    assert!(
        finish.starts_with(&format!("/sign-in/{}/finish?c=", p.slug())),
        "{finish}"
    );
    let done = fx.get(&mut b, &finish).await;
    assert_eq!(done.status, StatusCode::SEE_OTHER, "{}", done.body);
    assert_eq!(done.location().unwrap().split('?').next(), Some("/account"));
    assert_eq!(done.notice(), format!("connected-{}", p.slug()));
    let have = fx.access.identities_of(&ada);
    assert_eq!(have.len(), 1);
    assert_eq!(have[0].provider, p.slug());
    assert_eq!(have[0].subject, subject_of(p, who));
    assert_eq!(have[0].label, label_of(p, who));
    assert!(!have[0].connected.is_empty());
    // Only the provider's id for the person is kept: no token, and not the address as the key.
    assert!(!fx.stored().contains("gho_"));
    // The Account page lists it, with a way to disconnect it.
    let account = fx.get(&mut b, "/account").await;
    assert!(
        account
            .body
            .contains(&format!("/ui/connections/{}/disconnect", p.slug()))
    );
    assert!(account.body.contains(&label_of(p, who)));
    // The account's address is told, and the audit trail names the provider and no token.
    let notice = fx.mail_to(ADA, 1).await;
    assert_eq!(
        notice[0].subject,
        format!("{} was connected to your RichView account", p.name())
    );
    assert!(notice[0].body.contains(&format!("{ORIGIN}/account")));
    let audit = fx.audit();
    assert!(
        audit.contains("identity.join") && audit.contains(&format!("{ada}/{}", p.slug())),
        "{audit}"
    );
    assert!(!audit.contains("gho_"));
    // The flow is spent and its cookie is gone.
    assert!(b.cookie("rv_oauth").is_none());
}

#[tokio::test]
async fn connecting_github_joins_the_signed_in_person_whatever_the_address() {
    connect_joins_whatever_the_address(P::GitHub).await;
}

#[tokio::test]
async fn connecting_microsoft_joins_the_signed_in_person_whatever_the_address() {
    connect_joins_whatever_the_address(P::Microsoft).await;
}

#[tokio::test]
async fn connecting_google_joins_the_signed_in_person_whatever_the_address() {
    connect_joins_whatever_the_address(P::Google).await;
}

// ------------------------------------------------------------------------------ a known identity

async fn known_identity_signs_in(p: P) {
    let fx = Fixture::new().await;
    fx.person(ADA, "Ada").await;
    fx.joined(ADA, p, ADA_AT_PROVIDER).await;
    // Another browser, with no session, on its way to a page that needs one.
    let mut b = Browser::new();
    let (_, reply) = fx.sign_in_with(&mut b, p, Some("/r/ab123456/q3")).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(reply.refresh().as_deref(), Some("/reports/ab123456/q3"));
    let session = reply.sets("rv_session").expect("a session");
    assert!(
        session.contains("HttpOnly") && session.contains("SameSite=Strict"),
        "{session}"
    );
    assert!(b.cookie("rv_oauth").is_none(), "the flow cookie is spent");
    // The session is real.
    let account = fx.get(&mut b, "/account").await;
    assert_eq!(account.status, StatusCode::OK);
    assert!(account.body.contains(ADA));
    assert!(fx.audit().contains("identity.sign-in"));
}

#[tokio::test]
async fn a_joined_github_identity_signs_its_person_in_and_returns_to_the_report_it_was_going_to() {
    known_identity_signs_in(P::GitHub).await;
}

#[tokio::test]
async fn a_joined_microsoft_identity_signs_its_person_in_and_returns_to_the_report_it_was_going_to()
{
    known_identity_signs_in(P::Microsoft).await;
}

#[tokio::test]
async fn a_joined_google_identity_signs_its_person_in_and_returns_to_the_report_it_was_going_to() {
    known_identity_signs_in(P::Google).await;
}

#[tokio::test]
async fn a_disabled_person_cannot_sign_in_with_a_provider() {
    let fx = Fixture::new().await;
    let ada = fx.person(ADA, "Ada").await;
    fx.joined(ADA, P::GitHub, ADA_AT_PROVIDER).await;
    operator_action(
        &fx.state,
        OperatorAction::DisablePerson {
            person: ada,
            disabled: true,
        },
    )
    .await
    .unwrap();
    let mut b = Browser::new();
    let (_, reply) = fx.sign_in_with(&mut b, P::GitHub, None).await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN, "{}", reply.body);
    assert!(!reply.opens_a_session());
    assert!(!fx.get(&mut b, "/account").await.status.is_success());
}

// ------------------------------------------------------------------------------ a vouched address

async fn vouched_match_joins_at_once(p: P) {
    // Daniel's Q30 answer: an address the provider vouches for joins the account that has it at
    // once, with no mailed link, and signs it in.
    let fx = Fixture::new().await;
    let ada = fx.person(ADA, "Ada").await;
    fx.stand.provide(p, ADA_AT_PROVIDER);
    let mut b = Browser::new();
    let (_, reply) = fx.sign_in_with(&mut b, p, Some("/r/ab123456/q3")).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(reply.refresh().as_deref(), Some("/reports/ab123456/q3"));
    assert!(reply.opens_a_session());
    let have = fx.access.identities_of(&ada);
    assert_eq!(have.len(), 1);
    assert_eq!(have[0].subject, subject_of(p, ADA_AT_PROVIDER));
    assert_eq!(have[0].label, label_of(p, ADA_AT_PROVIDER));
    assert_eq!(fx.get(&mut b, "/account").await.status, StatusCode::OK);
    // The account's address is told that a provider now signs in, and nothing else is mailed.
    let mails = fx.mail_to(ADA, 1).await;
    assert_eq!(mails.len(), 1);
    assert_eq!(
        mails[0].subject,
        format!("{} was connected to your RichView account", p.name())
    );
    assert!(fx.audit().contains("identity.join"));
    // The next sign-in finds the identity joined.
    let mut c = Browser::new();
    let (_, again) = fx.sign_in_with(&mut c, p, None).await;
    assert!(again.opens_a_session(), "{}", again.body);
    assert_eq!(fx.access.identities_of(&ada).len(), 1);
}

#[tokio::test]
async fn a_vouched_github_address_that_matches_an_account_joins_it_at_once() {
    vouched_match_joins_at_once(P::GitHub).await;
}

#[tokio::test]
async fn a_vouched_microsoft_address_that_matches_an_account_joins_it_at_once() {
    vouched_match_joins_at_once(P::Microsoft).await;
}

#[tokio::test]
async fn a_vouched_google_address_that_matches_an_account_joins_it_at_once() {
    vouched_match_joins_at_once(P::Google).await;
}

#[tokio::test]
async fn a_vouched_address_of_a_disabled_account_joins_nothing() {
    let fx = Fixture::new().await;
    let ada = fx.person(ADA, "Ada").await;
    operator_action(
        &fx.state,
        OperatorAction::DisablePerson {
            person: ada.clone(),
            disabled: true,
        },
    )
    .await
    .unwrap();
    fx.stand.provide(P::GitHub, ADA_AT_PROVIDER);
    let mut b = Browser::new();
    let (_, reply) = fx.sign_in_with(&mut b, P::GitHub, None).await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN, "{}", reply.body);
    assert!(!reply.opens_a_session());
    assert!(fx.access.identities_of(&ada).is_empty());
    fx.no_mail().await;
}

const NINA: Who = Who {
    n: 515,
    login: "nina-hub",
    name: "Nina New",
    email: Some("nina@example.test"),
    vouched: true,
};

async fn vouched_stranger_creates_an_account(p: P) {
    let fx = Fixture::new().await;
    fx.stand.provide(p, NINA);
    let mut b = Browser::new();
    let (_, reply) = fx.sign_in_with(&mut b, p, Some("/r/ab123456/q3")).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    // A page with the name, the address and the terms; nothing is created yet.
    assert!(reply.body.contains("Create your account"));
    assert!(reply.body.contains("Nina New") && reply.body.contains("nina@example.test"));
    assert!(reply.body.contains("richview.ai/terms"));
    assert!(!reply.opens_a_session());
    assert!(fx.person_with("nina@example.test").is_none());
    assert!(
        b.cookie("rv_oauth").is_some(),
        "the confirmation is tied to this browser"
    );
    let confirmation = hidden(&reply.body, "confirmation");
    let created = fx
        .post(
            &mut b,
            &format!("/sign-in/{}/confirm", p.slug()),
            &format!("confirmation={}", enc(&confirmation)),
        )
        .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.body);
    assert_eq!(created.refresh().as_deref(), Some("/reports/ab123456/q3"));
    assert!(created.opens_a_session());
    let nina = fx.person_with("nina@example.test").expect("the account");
    assert_eq!(nina.name, "Nina New");
    // No password, one identity, an organization of their own as sign-up makes one.
    assert!(!fx.access.has_password(&nina.id));
    let have = fx.access.identities_of(&nina.id);
    assert_eq!(have.len(), 1);
    assert_eq!(have[0].subject, subject_of(p, NINA));
    assert_eq!(fx.access.organizations_for(&nina.id).len(), 1);
    assert_eq!(fx.get(&mut b, "/account").await.status, StatusCode::OK);
    // The person has just made this account: no notice mail.
    fx.no_mail().await;
    let audit = fx.audit();
    assert!(
        audit.contains("account.create") && audit.contains("identity.join"),
        "{audit}"
    );
    // The confirmation works once.
    let again = fx
        .post(
            &mut b,
            &format!("/sign-in/{}/confirm", p.slug()),
            &format!("confirmation={}", enc(&confirmation)),
        )
        .await;
    assert_eq!(again.status, StatusCode::BAD_REQUEST);
    assert_eq!(fx.access.users().len(), 1);
}

#[tokio::test]
async fn a_vouched_github_address_with_no_account_creates_one_through_the_confirm_page() {
    vouched_stranger_creates_an_account(P::GitHub).await;
}

#[tokio::test]
async fn a_vouched_microsoft_address_with_no_account_creates_one_through_the_confirm_page() {
    vouched_stranger_creates_an_account(P::Microsoft).await;
}

#[tokio::test]
async fn a_vouched_google_address_with_no_account_creates_one_through_the_confirm_page() {
    vouched_stranger_creates_an_account(P::Google).await;
}

#[tokio::test]
async fn a_confirmation_from_another_browser_creates_nothing() {
    let fx = Fixture::new().await;
    fx.stand.provide(P::GitHub, NINA);
    let mut b = Browser::new();
    let (_, reply) = fx.sign_in_with(&mut b, P::GitHub, None).await;
    let confirmation = hidden(&reply.body, "confirmation");
    // Someone who learned the page's address posts it from a browser that never started the flow.
    let mut other = Browser::new();
    let form = format!("confirmation={}", enc(&confirmation));
    let refused = fx.post(&mut other, "/sign-in/github/confirm", &form).await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    assert!(refused.body.contains(STALE));
    assert!(fx.access.users().is_empty());
    // The attempt spent it: the right browser cannot use it afterwards either.
    let late = fx.post(&mut b, "/sign-in/github/confirm", &form).await;
    assert_eq!(late.status, StatusCode::BAD_REQUEST);
    assert!(fx.access.users().is_empty());
}

#[tokio::test]
async fn a_new_account_through_a_provider_follows_sign_up_rules() {
    let fx = Fixture::new().await;
    fx.person(ADA, "Ada").await;
    fx.stand.provide(P::GitHub, NINA);
    // The account limit stops sign-up, so it stops this too.
    fx.access.set_signup_account_limit(1).unwrap();
    let mut b = Browser::new();
    let (_, reply) = fx.sign_in_with(&mut b, P::GitHub, None).await;
    assert_eq!(
        reply.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        reply.body
    );
    assert!(fx.person_with("nina@example.test").is_none());
    // And so does the operator's pause, even for a confirmation already on its page.
    fx.access.set_signup_account_limit(10).unwrap();
    let mut c = Browser::new();
    let (_, page) = fx.sign_in_with(&mut c, P::GitHub, None).await;
    let confirmation = hidden(&page.body, "confirmation");
    operator_action(&fx.state, OperatorAction::SetSignupsPaused { paused: true })
        .await
        .unwrap();
    let paused = fx
        .post(
            &mut c,
            "/sign-in/github/confirm",
            &format!("confirmation={}", enc(&confirmation)),
        )
        .await;
    assert_eq!(
        paused.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        paused.body
    );
    assert!(fx.person_with("nina@example.test").is_none());
}

// ------------------------------------------------------------------------------ an invitation

/// Olivia owns an organization and invites `email`; returns the invitation link's path.
async fn invitation_to(fx: &Fixture, email: &str) -> (String, String) {
    let olivia = fx.person("olivia@example.test", "Olivia").await;
    let org = fx.access.organizations_for(&olivia).remove(0).id;
    let mut owner = Browser::new();
    fx.sign_in(&mut owner, "olivia@example.test");
    let sent = fx
        .post(
            &mut owner,
            &format!("/ui/orgs/{org}/invitations"),
            &format!("csrf=@csrf&email={}&role=author", enc(email)),
        )
        .await;
    assert_eq!(sent.status, StatusCode::SEE_OTHER, "{}", sent.body);
    let mails = fx.mail_to(email, 1).await;
    (link_in(&mails[0]), org)
}

async fn invitation_accepted_with(p: P) {
    let fx = Fixture::new().await;
    let invitee = "ivy@example.test";
    let (link, org) = invitation_to(&fx, invitee).await;
    // The provider reports another address entirely: the link proved the invited one.
    let who = Who {
        n: 5,
        login: "ivy-hub",
        name: "Ivy Hub",
        email: Some("elsewhere@example.test"),
        vouched: true,
    };
    fx.stand.provide(p, who);
    let mut b = Browser::new();
    let page = fx.get(&mut b, &link).await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.body);
    let token = hidden(&page.body, "token");
    assert_eq!(format!("/link/{token}"), link);
    assert!(
        page.body
            .contains(&format!("action=\"/sign-in/{}/invite\"", p.slug()))
    );
    let started = fx
        .post(
            &mut b,
            &format!("/sign-in/{}/invite", p.slug()),
            &format!("token={}", enc(&token)),
        )
        .await;
    assert_eq!(started.status, StatusCode::OK, "{}", started.body);
    assert!(started.sets("rv_oauth").is_some());
    let answer = fx.provider_answer(&started).await;
    let done = fx.arrive(&mut b, &answer).await;
    assert_eq!(done.status, StatusCode::OK, "{}", done.body);
    let ivy = fx.person_with(invitee).expect("the invited account");
    assert!(done.opens_a_session());
    assert_eq!(done.refresh().as_deref(), Some("/"));
    assert!(!fx.access.has_password(&ivy.id));
    assert_eq!(ivy.name, "Ivy Hub");
    assert!(fx.person_with("elsewhere@example.test").is_none());
    let have = fx.access.identities_of(&ivy.id);
    assert_eq!(have.len(), 1);
    assert_eq!(have[0].subject, subject_of(p, who));
    assert!(
        fx.access
            .organizations_for(&ivy.id)
            .iter()
            .any(|o| o.id == org)
    );
    // The link is spent. The person has just made this account: no notice mail follows the
    // invitation itself.
    assert_eq!(
        fx.get(&mut Browser::new(), &link).await.status,
        StatusCode::GONE
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(fx.mail_to(invitee, 1).await.len(), 1);
    assert!(fx.audit().contains("identity.join"));
    // And the provider signs that account in from now on.
    let mut again = Browser::new();
    let (_, signed) = fx.sign_in_with(&mut again, p, None).await;
    assert!(signed.opens_a_session(), "{}", signed.body);
}

#[tokio::test]
async fn an_invitation_is_accepted_with_github_by_a_new_person() {
    invitation_accepted_with(P::GitHub).await;
}

#[tokio::test]
async fn an_invitation_is_accepted_with_microsoft_by_a_new_person() {
    invitation_accepted_with(P::Microsoft).await;
}

#[tokio::test]
async fn an_invitation_is_accepted_with_google_by_a_new_person() {
    invitation_accepted_with(P::Google).await;
}

/// An address that already has an account takes no provider through its invitation: whoever holds
/// the mailed link would otherwise gain a lasting way into the account.
async fn invitation_with_a_provider_is_refused_for_an_existing_account(p: P) {
    let fx = Fixture::new().await;
    let invitee = "ivy@example.test";
    let ivy = fx.person(invitee, "Ivy Invited").await;
    let (link, _) = invitation_to(&fx, invitee).await;
    fx.stand.provide(
        p,
        Who {
            n: 5,
            login: "ivy-hub",
            name: "Ivy Hub",
            email: Some("elsewhere@example.test"),
            vouched: true,
        },
    );
    // The page offers to accept as usual, and no provider.
    let mut b = Browser::new();
    let page = fx.get(&mut b, &link).await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.body);
    assert!(page.body.contains("Accept and sign in"));
    assert!(!page.body.contains("/sign-in/"), "{}", page.body);
    // A post made by hand is refused before anything starts.
    let token = link.trim_start_matches("/link/").to_owned();
    let refused = fx
        .post(
            &mut b,
            &format!("/sign-in/{}/invite", p.slug()),
            &format!("token={}", enc(&token)),
        )
        .await;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.body);
    assert!(refused.body.contains("Accept the invitation as usual"));
    assert!(
        refused
            .body
            .contains("connect the provider on the Account page")
    );
    assert!(refused.sets("rv_oauth").is_none());
    assert_eq!(fx.stand.minted(), 0);
    assert!(fx.access.identities_of(&ivy).is_empty());
    // The invitation was not spent.
    assert_eq!(
        fx.get(&mut Browser::new(), &link).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn an_invitation_with_github_is_refused_for_an_existing_account() {
    invitation_with_a_provider_is_refused_for_an_existing_account(P::GitHub).await;
}

#[tokio::test]
async fn an_invitation_with_microsoft_is_refused_for_an_existing_account() {
    invitation_with_a_provider_is_refused_for_an_existing_account(P::Microsoft).await;
}

#[tokio::test]
async fn an_invitation_with_google_is_refused_for_an_existing_account() {
    invitation_with_a_provider_is_refused_for_an_existing_account(P::Google).await;
}

#[tokio::test]
async fn an_account_made_while_a_provider_flow_is_under_way_takes_no_provider_either() {
    let fx = Fixture::new().await;
    let invitee = "late@example.test";
    let (link, _) = invitation_to(&fx, invitee).await;
    fx.stand.provide(P::GitHub, ADA_AT_PROVIDER);
    let mut b = Browser::new();
    let page = fx.get(&mut b, &link).await;
    let token = hidden(&page.body, "token");
    // The address has no account when the person starts ...
    let started = fx
        .post(
            &mut b,
            "/sign-in/github/invite",
            &format!("token={}", enc(&token)),
        )
        .await;
    assert_eq!(started.status, StatusCode::OK, "{}", started.body);
    let answer = fx.provider_answer(&started).await;
    // ... and has one when the provider answers.
    let late = fx.person(invitee, "Late").await;
    let refused = fx.arrive(&mut b, &answer).await;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.body);
    assert!(!refused.opens_a_session());
    assert!(fx.access.identities_of(&late).is_empty());
    assert_eq!(
        fx.get(&mut Browser::new(), &link).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn the_invitation_page_for_a_new_account_shows_the_terms_beside_the_provider_buttons() {
    let fx = Fixture::new().await;
    let (link, _) = invitation_to(&fx, "ivy@example.test").await;
    let page = fx.get(&mut Browser::new(), &link).await;
    assert!(page.body.contains("action=\"/sign-in/github/invite\""));
    // Under the password form, and again right under the buttons, which sit outside it.
    assert!(
        page.body.contains(
            "</form></div><p class=\"fine\">By creating an account, you agree to the <a href=\"https://richview.ai/terms\">Terms</a>"
        ),
        "{}",
        page.body
    );
    assert!(
        page.body
            .contains("</button></form></div><p class=\"fine\">")
    );
}

#[tokio::test]
async fn an_identity_that_belongs_to_someone_else_cannot_take_an_invitation() {
    let fx = Fixture::new().await;
    fx.person("bob@example.test", "Bob").await;
    fx.joined("bob@example.test", P::GitHub, ADA_AT_PROVIDER)
        .await;
    let (link, _) = invitation_to(&fx, "cara@example.test").await;
    let mut b = Browser::new();
    let page = fx.get(&mut b, &link).await;
    let token = hidden(&page.body, "token");
    let started = fx
        .post(
            &mut b,
            "/sign-in/github/invite",
            &format!("token={}", enc(&token)),
        )
        .await;
    let answer = fx.provider_answer(&started).await;
    let refused = fx.arrive(&mut b, &answer).await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{}", refused.body);
    assert!(
        refused
            .body
            .contains("already connected to another RichView account")
    );
    assert!(!refused.opens_a_session());
    assert!(fx.person_with("cara@example.test").is_none());
    // The invitation was not spent: she can still accept it another way.
    assert_eq!(
        fx.get(&mut Browser::new(), &link).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn an_invitation_post_needs_the_same_origin_and_a_live_link() {
    let fx = Fixture::new().await;
    let (link, _) = invitation_to(&fx, "ivy@example.test").await;
    let token = link.trim_start_matches("/link/").to_owned();
    let mut b = Browser::new();
    let form = format!("token={}", enc(&token));
    let foreign = fx
        .send(
            &mut b,
            "POST",
            "/sign-in/github/invite",
            Some(&form),
            false,
            "http://evil.test",
        )
        .await;
    assert_eq!(foreign.status, StatusCode::FORBIDDEN);
    assert!(foreign.sets("rv_oauth").is_none());
    let unknown = fx
        .post(&mut b, "/sign-in/github/invite", "token=not-a-link")
        .await;
    assert_eq!(unknown.status, StatusCode::GONE);
    assert!(unknown.sets("rv_oauth").is_none());
    assert_eq!(fx.stand.minted(), 0);
    assert_eq!(fx.access.social().unwrap().waiting(), 0);
}

// ------------------------------------------------------------------------------ nothing vouched

#[tokio::test]
async fn github_primary_but_unverified_vouches_for_nothing() {
    let fx = Fixture::new().await;
    let ada = fx.person(ADA, "Ada").await;
    fx.stand.provide(
        P::GitHub,
        Who {
            vouched: false,
            ..ADA_AT_PROVIDER
        },
    );
    let mut b = Browser::new();
    let (_, reply) = fx.sign_in_with(&mut b, P::GitHub, None).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert!(reply.body.contains(NO_ADDRESS), "{}", reply.body);
    nothing_happened(&fx, &reply, 1).await;
    assert!(fx.access.identities_of(&ada).is_empty());
}

#[tokio::test]
async fn github_verified_but_not_primary_vouches_for_nothing() {
    let fx = Fixture::new().await;
    fx.person(ADA, "Ada").await;
    fx.stand.provide(P::GitHub, ADA_AT_PROVIDER);
    fx.stand.github_emails(json!([
        {"email": ADA, "primary": false, "verified": true},
        {"email": "other@example.test", "primary": true, "verified": false},
    ]));
    let mut b = Browser::new();
    let (_, reply) = fx.sign_in_with(&mut b, P::GitHub, None).await;
    assert!(reply.body.contains(NO_ADDRESS), "{}", reply.body);
    nothing_happened(&fx, &reply, 1).await;
}

#[tokio::test]
async fn microsoft_email_without_xms_edov_vouches_for_nothing() {
    // The nOAuth takeover: a tenant can put any address in the `email` claim. The address matches
    // an account here, and still nothing is joined, created or mailed.
    let fx = Fixture::new().await;
    fx.person(ADA, "Ada").await;
    fx.stand.provide(
        P::Microsoft,
        Who {
            vouched: false,
            ..ADA_AT_PROVIDER
        },
    );
    let mut b = Browser::new();
    let (_, reply) = fx.sign_in_with(&mut b, P::Microsoft, None).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert!(reply.body.contains(NO_ADDRESS), "{}", reply.body);
    nothing_happened(&fx, &reply, 1).await;
}

#[tokio::test]
async fn microsoft_email_with_xms_edov_false_vouches_for_nothing() {
    let fx = Fixture::new().await;
    fx.person(ADA, "Ada").await;
    fx.stand.provide(P::Microsoft, ADA_AT_PROVIDER);
    fx.stand.claim("xms_edov", json!(false));
    let mut b = Browser::new();
    let (_, reply) = fx.sign_in_with(&mut b, P::Microsoft, None).await;
    assert!(reply.body.contains(NO_ADDRESS), "{}", reply.body);
    nothing_happened(&fx, &reply, 1).await;
}

#[tokio::test]
async fn microsoft_xms_edov_that_is_not_json_true_vouches_for_nothing() {
    let fx = Fixture::new().await;
    fx.person(ADA, "Ada").await;
    fx.stand.provide(P::Microsoft, ADA_AT_PROVIDER);
    fx.stand.claim("xms_edov", json!("true"));
    let mut b = Browser::new();
    let (_, reply) = fx.sign_in_with(&mut b, P::Microsoft, None).await;
    assert!(reply.body.contains(NO_ADDRESS), "{}", reply.body);
    nothing_happened(&fx, &reply, 1).await;
}

#[tokio::test]
async fn a_personal_microsoft_account_follows_the_same_rule() {
    let fx = Fixture::new().await;
    fx.person(ADA, "Ada").await;
    // The consumer tenant has no special case: without `xms_edov: true` nothing is vouched ...
    fx.stand.provide(
        P::Microsoft,
        Who {
            vouched: false,
            ..ADA_AT_PROVIDER
        },
    );
    fx.stand.claim("tid", json!(CONSUMER_TENANT));
    fx.stand.claim(
        "iss",
        json!(format!(
            "https://login.microsoftonline.com/{CONSUMER_TENANT}/v2.0"
        )),
    );
    let mut b = Browser::new();
    let (_, reply) = fx.sign_in_with(&mut b, P::Microsoft, None).await;
    assert!(reply.body.contains(NO_ADDRESS), "{}", reply.body);
    nothing_happened(&fx, &reply, 1).await;
    // ... and with it the address is vouched for like any other tenant's.
    fx.stand.provide(P::Microsoft, ADA_AT_PROVIDER);
    fx.stand.claim("tid", json!(CONSUMER_TENANT));
    fx.stand.claim(
        "iss",
        json!(format!(
            "https://login.microsoftonline.com/{CONSUMER_TENANT}/v2.0"
        )),
    );
    let mut c = Browser::new();
    let (_, reply) = fx.sign_in_with(&mut c, P::Microsoft, None).await;
    assert!(reply.opens_a_session(), "{}", reply.body);
    let mails = fx.mail_to(ADA, 1).await;
    assert_eq!(
        mails[0].subject,
        "Microsoft was connected to your RichView account"
    );
}

// ------------------------------------------------------------------------------ attacks on the flow

#[tokio::test]
async fn a_replayed_state_is_refused() {
    let fx = Fixture::new().await;
    fx.person(ADA, "Ada").await;
    fx.joined(ADA, P::GitHub, ADA_AT_PROVIDER).await;
    let mut b = Browser::new();
    let started = fx.get(&mut b, "/sign-in/github").await;
    let answer = fx.provider_answer(&started).await;
    // What an eavesdropper has: the address of the answer and the cookie that went with it.
    let copy = b.clone();
    let first = fx.arrive(&mut b, &answer).await;
    assert!(first.opens_a_session(), "{}", first.body);
    let calls = fx.stand.token_calls();
    let mut thief = copy;
    let second = fx.arrive(&mut thief, &answer).await;
    assert_eq!(second.status, StatusCode::BAD_REQUEST);
    assert!(second.body.contains(STALE));
    assert!(!second.opens_a_session());
    assert_eq!(
        fx.stand.token_calls(),
        calls,
        "the replay reached the provider"
    );
}

#[tokio::test]
async fn a_foreign_state_is_refused() {
    let fx = Fixture::new().await;
    fx.person(ADA, "Ada").await;
    fx.joined(ADA, P::GitHub, ADA_AT_PROVIDER).await;
    let mut victim = Browser::new();
    let started = fx.get(&mut victim, "/sign-in/github").await;
    let answer = fx.provider_answer(&started).await;
    // Another browser, with a flow of its own and so a cookie of its own, is made to send the
    // victim's answer: the login-CSRF the cookie is there to stop.
    let mut attacker = Browser::new();
    fx.get(&mut attacker, "/sign-in/github").await;
    let calls = fx.stand.token_calls();
    let refused = fx.arrive(&mut attacker, &answer).await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    assert!(refused.body.contains(STALE));
    assert!(!refused.opens_a_session());
    assert_eq!(fx.stand.token_calls(), calls);
    // A flow is spent by any look at it.
    let late = fx.arrive(&mut victim, &answer).await;
    assert_eq!(late.status, StatusCode::BAD_REQUEST);
    assert!(!late.opens_a_session());
}

#[tokio::test]
async fn an_answer_without_the_flow_cookie_is_refused() {
    let fx = Fixture::new().await;
    fx.person(ADA, "Ada").await;
    fx.joined(ADA, P::Microsoft, ADA_AT_PROVIDER).await;
    let mut b = Browser::new();
    let started = fx.get(&mut b, "/sign-in/microsoft").await;
    let answer = fx.provider_answer(&started).await;
    let calls = fx.stand.token_calls();
    let mut cookieless = Browser::new();
    let refused = fx.arrive(&mut cookieless, &answer).await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    assert!(refused.body.contains(STALE));
    assert!(!refused.opens_a_session());
    assert_eq!(fx.stand.token_calls(), calls);
}

async fn pkce_mismatch(p: P) {
    let fx = Fixture::new().await;
    fx.person(ADA, "Ada").await;
    fx.joined(ADA, p, ADA_AT_PROVIDER).await;
    let before = fx.stand.refused_tokens();
    // The provider holds a challenge that is not the server's verifier's: a code swapped in on
    // the way cannot be redeemed.
    fx.stand.provide(p, ADA_AT_PROVIDER);
    fx.stand
        .swap_challenge("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
    let mut b = Browser::new();
    let (_, reply) = fx.sign_in_with(&mut b, p, None).await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{}", reply.body);
    assert!(reply.body.contains(NOT_CONFIRMED), "{}", reply.body);
    assert!(!reply.opens_a_session());
    assert_eq!(fx.stand.refused_tokens(), before + 1);
}

#[tokio::test]
async fn a_pkce_mismatch_is_refused_for_github() {
    pkce_mismatch(P::GitHub).await;
}

#[tokio::test]
async fn a_pkce_mismatch_is_refused_for_microsoft() {
    pkce_mismatch(P::Microsoft).await;
}

#[tokio::test]
async fn a_pkce_mismatch_is_refused_for_google() {
    pkce_mismatch(P::Google).await;
}

/// A Microsoft token with one claim wrong signs nobody in and creates and joins nothing, though
/// the address in it matches an account.
async fn refused_id_token(key: &str, value: Value) {
    let fx = Fixture::new().await;
    fx.person(ADA, "Ada").await;
    fx.stand.provide(P::Microsoft, ADA_AT_PROVIDER);
    fx.stand.claim(key, value);
    let mut b = Browser::new();
    let (_, reply) = fx.sign_in_with(&mut b, P::Microsoft, None).await;
    assert_eq!(
        reply.status,
        StatusCode::BAD_REQUEST,
        "{key}: {}",
        reply.body
    );
    assert!(reply.body.contains(NOT_CONFIRMED), "{key}: {}", reply.body);
    nothing_happened(&fx, &reply, 1).await;
}

#[tokio::test]
async fn a_wrong_nonce_is_refused() {
    refused_id_token("nonce", json!("not-the-nonce-of-this-flow")).await;
}

#[tokio::test]
async fn a_wrong_audience_is_refused() {
    refused_id_token("aud", json!("someone-elses-client-id")).await;
}

#[tokio::test]
async fn a_wrong_issuer_is_refused() {
    refused_id_token(
        "iss",
        json!(format!(
            "https://login.microsoftonline.com/{OTHER_TENANT}/v2.0"
        )),
    )
    .await;
}

#[tokio::test]
async fn an_issuer_that_is_not_microsoft_is_refused() {
    refused_id_token("iss", json!(format!("https://evil.test/{TENANT}/v2.0"))).await;
}

#[tokio::test]
async fn a_tenant_that_the_issuer_does_not_name_is_refused() {
    refused_id_token("tid", json!(OTHER_TENANT)).await;
}

#[tokio::test]
async fn an_expired_id_token_is_refused() {
    refused_id_token("exp", json!(unix_now() - 10)).await;
}

#[tokio::test]
async fn an_id_token_without_an_object_id_is_refused() {
    refused_id_token("oid", Value::Null).await;
}

/// A Google token with one claim wrong signs nobody in and creates and joins nothing, though the
/// address in it matches an account.
async fn refused_google_token(key: &str, value: Value) {
    let fx = Fixture::new().await;
    fx.person(ADA, "Ada").await;
    fx.stand.provide(P::Google, ADA_AT_PROVIDER);
    fx.stand.google_claim(key, value);
    let mut b = Browser::new();
    let (_, reply) = fx.sign_in_with(&mut b, P::Google, None).await;
    assert_eq!(
        reply.status,
        StatusCode::BAD_REQUEST,
        "{key}: {}",
        reply.body
    );
    assert!(reply.body.contains(NOT_CONFIRMED), "{key}: {}", reply.body);
    nothing_happened(&fx, &reply, 1).await;
}

#[tokio::test]
async fn a_google_token_with_a_wrong_nonce_audience_issuer_or_time_is_refused() {
    refused_google_token("nonce", json!("not-the-nonce-of-this-flow")).await;
    refused_google_token("aud", json!("someone-elses-client-id")).await;
    refused_google_token("iss", json!("https://evil.test")).await;
    refused_google_token(
        "iss",
        json!(format!("https://login.microsoftonline.com/{TENANT}/v2.0")),
    )
    .await;
    refused_google_token("exp", json!(unix_now() - 10)).await;
    refused_google_token("sub", Value::Null).await;
}

#[tokio::test]
async fn a_google_address_counts_only_with_email_verified_true() {
    // The address matches an account each time, and still nothing is joined, created or mailed.
    for verified in [json!(false), json!("true"), json!(1), Value::Null] {
        let fx = Fixture::new().await;
        let ada = fx.person(ADA, "Ada").await;
        fx.stand.provide(P::Google, ADA_AT_PROVIDER);
        fx.stand.google_claim("email_verified", verified.clone());
        let mut b = Browser::new();
        let (_, reply) = fx.sign_in_with(&mut b, P::Google, None).await;
        assert_eq!(reply.status, StatusCode::OK, "{verified}: {}", reply.body);
        assert!(
            reply.body.contains(NO_ADDRESS),
            "{verified}: {}",
            reply.body
        );
        nothing_happened(&fx, &reply, 1).await;
        assert!(fx.access.identities_of(&ada).is_empty(), "{verified}");
    }
}

/// Waits until the signed-in page at `path` tells an unsent join notice for GitHub.
async fn tells_unsent(fx: &Fixture, b: &mut Browser, path: &str) -> Reply {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let page = fx.get(b, path).await;
            if page.body.contains("GitHub was connected on") {
                return page;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{path} tells the unsent notice"))
}

#[tokio::test]
async fn a_join_notice_the_spent_budget_refuses_is_kept_and_shown_at_home_and_on_account() {
    let fx = Fixture::with_mail(Some(2)).await;
    let ada = fx.person(ADA, "Ada").await;
    // The day's mail budget is spent before the join.
    let queue = fx.access.mail_queue().unwrap();
    let mut spent = false;
    for _ in 0..10 {
        let mail = Mail {
            to: "someone@example.test".into(),
            subject: "Spent".into(),
            body: "Spent".into(),
            html: None,
        };
        if queue.enqueue(mail).await.is_err() {
            spent = true;
            break;
        }
    }
    assert!(spent, "a budget of two refuses a mail within ten");
    fx.stand.provide(P::GitHub, ADA_AT_PROVIDER);
    let mut b = Browser::new();
    let (_, reply) = fx.sign_in_with(&mut b, P::GitHub, None).await;
    assert!(reply.opens_a_session(), "{}", reply.body);
    assert_eq!(fx.access.identities_of(&ada).len(), 1);
    // Kept, told on the Account page and on the home page, and the audit says it did not go.
    let account = tells_unsent(&fx, &mut b, "/account").await;
    assert!(
        account.body.contains(
            "We could not mail you about it yet. If it was not you, disconnect it and change your password."
        ),
        "{}",
        account.body
    );
    tells_unsent(&fx, &mut b, "/").await;
    assert!(fx.audit().contains("identity.notice-unsent"));
    assert!(fx.mail.0.lock().unwrap().iter().all(|mail| mail.to != ADA));
}

/// A join notice kept because the server had no mail yet is mailed from the signed-in page at
/// `path` once mail can go, and that page and the others stop telling it.
async fn kept_notice_is_mailed_from(path: &str) {
    let fx = Fixture::with_mail(None).await;
    fx.person(ADA, "Ada").await;
    fx.stand.provide(P::GitHub, ADA_AT_PROVIDER);
    let mut b = Browser::new();
    let (_, reply) = fx.sign_in_with(&mut b, P::GitHub, None).await;
    assert!(reply.opens_a_session(), "{}", reply.body);
    tells_unsent(&fx, &mut b, path).await;
    fx.access
        .configure_mail(fx.mail.clone(), 1000, false)
        .unwrap();
    let page = fx.get(&mut b, path).await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.body);
    assert!(
        !page.body.contains("GitHub was connected on"),
        "{}",
        page.body
    );
    let mails = fx.mail_to(ADA, 1).await;
    assert!(mails[0].subject.contains("GitHub"), "{}", mails[0].subject);
    for other in ["/", "/account"] {
        let page = fx.get(&mut b, other).await;
        assert!(!page.body.contains("We could not mail you"), "{other}");
    }
}

#[tokio::test]
async fn a_kept_join_notice_is_mailed_from_the_home_page_once_mail_can_go() {
    kept_notice_is_mailed_from("/").await;
}

#[tokio::test]
async fn a_kept_join_notice_is_mailed_from_the_account_page_once_mail_can_go() {
    kept_notice_is_mailed_from("/account").await;
}

#[tokio::test]
async fn a_google_address_outside_gmail_and_workspace_never_joins_by_address() {
    // Google checked the address once, when the account was made; it is not Gmail and no
    // Workspace domain (`hd`) stands behind it, so it is treated as unvouched.
    let fx = Fixture::new().await;
    let ada = fx.person(ADA, "Ada").await;
    fx.stand.provide(P::Google, ADA_AT_PROVIDER);
    fx.stand.google_claim("hd", Value::Null);
    let mut b = Browser::new();
    let (_, reply) = fx.sign_in_with(&mut b, P::Google, None).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert!(reply.body.contains(NO_ADDRESS), "{}", reply.body);
    nothing_happened(&fx, &reply, 1).await;
    assert!(fx.access.identities_of(&ada).is_empty());
}

#[tokio::test]
async fn a_gmail_address_google_verified_joins_its_account_at_once() {
    // The secrets scan refuses literal Gmail addresses, so this one is built from the constant.
    let address: &'static str = format!("gail@{GOOGLE_CONSUMER_DOMAIN}").leak();
    let fx = Fixture::new().await;
    let gail = fx.person(address, "Gail").await;
    fx.stand.provide(
        P::Google,
        Who {
            n: 620,
            login: "gail",
            name: "Gail",
            email: Some(address),
            vouched: true,
        },
    );
    let mut b = Browser::new();
    let (_, reply) = fx.sign_in_with(&mut b, P::Google, None).await;
    assert!(reply.opens_a_session(), "{}", reply.body);
    assert_eq!(fx.access.identities_of(&gail).len(), 1);
}

#[tokio::test]
async fn the_google_sign_in_asks_for_openid_email_and_profile_with_pkce_and_a_nonce() {
    let fx = Fixture::new().await;
    let mut b = Browser::new();
    let start = fx.get(&mut b, "/sign-in/google").await;
    assert_eq!(start.status, StatusCode::SEE_OTHER, "{}", start.body);
    let location = start.headers[header::LOCATION].to_str().unwrap().to_owned();
    let url = reqwest::Url::parse(&location).unwrap();
    assert_eq!(url.path(), "/google/authorize");
    let q: HashMap<String, String> = url.query_pairs().into_owned().collect();
    assert_eq!(q["client_id"], GO_ID);
    assert_eq!(q["scope"], "openid email profile");
    assert_eq!(q["response_type"], "code");
    assert_eq!(q["code_challenge_method"], "S256");
    assert_eq!(
        q["redirect_uri"],
        format!("{ORIGIN}/sign-in/google/callback")
    );
    assert!(!q["nonce"].is_empty() && !q["state"].is_empty());
    assert!(b.cookie("rv_oauth").is_some());
}

#[tokio::test]
async fn the_provider_error_redirect_creates_nothing() {
    let fx = Fixture::new().await;
    fx.person(ADA, "Ada").await;
    fx.stand.provide(P::GitHub, ADA_AT_PROVIDER);
    let marker = "%3Cscript%3Ealert(1)%3C%2Fscript%3E";
    fx.stand.deny(marker);
    let mut b = Browser::new();
    let (_, reply) = fx.sign_in_with(&mut b, P::GitHub, None).await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{}", reply.body);
    assert!(
        reply.body.contains("GitHub did not sign you in"),
        "{}",
        reply.body
    );
    // The provider's words are not repeated, and the flow is spent.
    assert!(!reply.body.contains("alert(1)"));
    assert_eq!(fx.access.social().unwrap().waiting(), 0);
    assert!(b.cookie("rv_oauth").is_none());
    assert_eq!(fx.stand.token_calls(), 0);
    nothing_happened(&fx, &reply, 1).await;
}

#[tokio::test]
async fn back_must_be_a_local_path() {
    let fx = Fixture::new().await;
    fx.person(ADA, "Ada").await;
    fx.joined(ADA, P::GitHub, ADA_AT_PROVIDER).await;
    for back in [
        "//evil.test",
        "https://evil.test/steal",
        "/\\evil.test",
        "javascript:alert(1)",
        "evil.test",
    ] {
        fx.stand.provide(P::GitHub, ADA_AT_PROVIDER);
        let mut b = Browser::new();
        let (_, reply) = fx.sign_in_with(&mut b, P::GitHub, Some(back)).await;
        assert!(reply.opens_a_session(), "{back}: {}", reply.body);
        assert_eq!(reply.refresh().as_deref(), Some("/"), "{back}");
        assert!(!reply.body.contains("evil.test"), "{back}: {}", reply.body);
    }
}

// ------------------------------------------------------------------------------ the Account page

#[tokio::test]
async fn connecting_needs_the_csrf_token_a_session_and_the_same_origin() {
    let fx = Fixture::new().await;
    fx.person(ADA, "Ada").await;
    fx.stand.provide(P::GitHub, ADA_AT_PROVIDER);
    let mut b = Browser::new();
    fx.sign_in(&mut b, ADA);
    for form in ["csrf=not-the-token", "csrf=", ""] {
        let refused = fx.post(&mut b, "/sign-in/github/connect", form).await;
        assert!(
            refused.status.is_client_error(),
            "{form}: {} {}",
            refused.status,
            refused.body
        );
        assert!(refused.sets("rv_oauth").is_none());
    }
    // The right token from another site.
    let csrf = b.csrf.clone();
    let foreign = fx
        .send(
            &mut b,
            "POST",
            "/sign-in/github/connect",
            Some(&format!("csrf={}", enc(&csrf))),
            false,
            "http://evil.test",
        )
        .await;
    assert!(foreign.status.is_client_error(), "{}", foreign.status);
    assert!(foreign.sets("rv_oauth").is_none());
    // No session at all: sent back to sign in, with nothing started.
    let mut anonymous = Browser::new();
    let none = fx
        .post(
            &mut anonymous,
            "/sign-in/github/connect",
            &format!("csrf={}", enc(&csrf)),
        )
        .await;
    assert_eq!(none.status, StatusCode::SEE_OTHER);
    assert_eq!(none.notice(), "Sign in first.");
    assert!(none.sets("rv_oauth").is_none());
    // A GET never starts a connection.
    let get = fx.get(&mut b, "/sign-in/github/connect").await;
    assert!(get.status.is_client_error(), "{}", get.status);
    assert_eq!(fx.stand.minted(), 0);
    assert_eq!(fx.access.social().unwrap().waiting(), 0);
    assert!(fx.identities(ADA).is_empty());
}

#[tokio::test]
async fn an_identity_joined_to_someone_else_cannot_be_connected() {
    let fx = Fixture::new().await;
    fx.person("bob@example.test", "Bob").await;
    fx.person(ADA, "Ada").await;
    fx.joined("bob@example.test", P::GitHub, ADA_AT_PROVIDER)
        .await;
    // Ada signs in with her password and connects the same GitHub account.
    let mut b = Browser::new();
    fx.sign_in(&mut b, ADA);
    let done = fx.connect(&mut b, P::GitHub).await;
    assert_eq!(done.status, StatusCode::SEE_OTHER, "{}", done.body);
    assert_eq!(done.notice(), "inuse-github");
    assert!(fx.identities(ADA).is_empty());
    assert_eq!(fx.identities("bob@example.test").len(), 1);
}

/// An account a provider created, signed in.
async fn nina_with(fx: &Fixture, p: P) -> Browser {
    fx.stand.provide(p, NINA);
    let mut b = Browser::new();
    let (_, page) = fx.sign_in_with(&mut b, p, None).await;
    let confirmation = hidden(&page.body, "confirmation");
    let created = fx
        .post(
            &mut b,
            &format!("/sign-in/{}/confirm", p.slug()),
            &format!("confirmation={}", enc(&confirmation)),
        )
        .await;
    assert!(created.opens_a_session(), "{}", created.body);
    // The page that moves on is followed by the browser; it picks the session up from there.
    let account = fx.get(&mut b, "/account").await;
    assert_eq!(account.status, StatusCode::OK);
    b.csrf = hidden(&account.body, "csrf");
    b
}

#[tokio::test]
async fn the_last_way_in_cannot_be_removed() {
    let fx = Fixture::new().await;
    let mut b = nina_with(&fx, P::GitHub).await;
    let refused = fx
        .post(&mut b, "/ui/connections/github/disconnect", "csrf=@csrf")
        .await;
    assert_eq!(refused.status, StatusCode::SEE_OTHER, "{}", refused.body);
    assert_eq!(refused.notice(), "onlyway-github");
    assert_eq!(fx.identities("nina@example.test").len(), 1);
    // And the same without the token.
    let no_csrf = fx
        .post(&mut b, "/ui/connections/github/disconnect", "csrf=wrong")
        .await;
    assert!(no_csrf.status.is_client_error());
    assert_eq!(fx.identities("nina@example.test").len(), 1);
}

#[tokio::test]
async fn a_person_with_a_password_can_disconnect_a_provider() {
    let fx = Fixture::new().await;
    let ada = fx.person(ADA, "Ada").await;
    fx.joined(ADA, P::GitHub, ADA_AT_PROVIDER).await;
    let mut b = Browser::new();
    fx.sign_in(&mut b, ADA);
    let done = fx
        .post(&mut b, "/ui/connections/github/disconnect", "csrf=@csrf")
        .await;
    assert_eq!(done.status, StatusCode::SEE_OTHER, "{}", done.body);
    assert_eq!(done.notice(), "disconnected-github");
    assert!(fx.access.identities_of(&ada).is_empty());
    assert!(fx.audit().contains("identity.disconnect"));
    // GitHub no longer signs that person in, though it vouches for the address.
    fx.stand.provide(P::GitHub, ADA_AT_PROVIDER);
    let mut c = Browser::new();
    let (_, reply) = fx.sign_in_with(&mut c, P::GitHub, None).await;
    assert_eq!(reply.status, StatusCode::CONFLICT, "{}", reply.body);
    assert!(
        reply.body.contains("GitHub was disconnected")
            && reply.body.contains("then choose Connect beside GitHub"),
        "{}",
        reply.body
    );
    assert!(!reply.opens_a_session());
    assert!(fx.access.identities_of(&ada).is_empty());
    // Connecting it again while signed in is the person's own choice, and then it signs in.
    let again = fx.connect(&mut b, P::GitHub).await;
    assert_eq!(again.status, StatusCode::SEE_OTHER, "{}", again.body);
    assert_eq!(fx.access.identities_of(&ada).len(), 1);
    let (_, reply) = fx.sign_in_with(&mut Browser::new(), P::GitHub, None).await;
    assert!(reply.opens_a_session(), "{}", reply.body);
}

#[tokio::test]
async fn an_account_with_two_ways_in_can_drop_one_but_not_the_last() {
    let fx = Fixture::new().await;
    let mut b = nina_with(&fx, P::GitHub).await;
    // Connect Microsoft as well.
    fx.stand.provide(
        P::Microsoft,
        Who {
            n: 516,
            login: "nina-ms",
            email: Some("nina@example.test"),
            ..NINA
        },
    );
    let done = fx.connect(&mut b, P::Microsoft).await;
    assert_eq!(done.status, StatusCode::SEE_OTHER, "{}", done.body);
    assert_eq!(fx.identities("nina@example.test").len(), 2);
    let csrf = enc(&b.csrf.clone());
    let github = fx
        .post(
            &mut b,
            "/ui/connections/github/disconnect",
            &format!("csrf={csrf}"),
        )
        .await;
    assert_eq!(github.notice(), "disconnected-github");
    let microsoft = fx
        .post(
            &mut b,
            "/ui/connections/microsoft/disconnect",
            &format!("csrf={csrf}"),
        )
        .await;
    assert_eq!(microsoft.notice(), "onlyway-microsoft");
    let left = fx.identities("nina@example.test");
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].provider, "microsoft");
}

#[tokio::test]
async fn a_provider_created_account_can_set_a_password_with_the_reset_link() {
    let fx = Fixture::new().await;
    nina_with(&fx, P::GitHub).await;
    let nina = fx.person_with("nina@example.test").unwrap();
    assert!(!fx.access.has_password(&nina.id));
    let mut anonymous = Browser::new();
    let asked = fx
        .post(&mut anonymous, "/ui/forgot", "email=nina%40example.test")
        .await;
    assert_eq!(asked.status, StatusCode::OK, "{}", asked.body);
    // Making the account sent nothing, so the one mail is the reset link.
    let mails = fx.mail_to("nina@example.test", 1).await;
    let link = link_in(&mails[0]);
    // She ticks GitHub to keep it.
    let done = fx
        .post(
            &mut anonymous,
            &form_path(&link),
            "password=a-new-password&password_confirm=a-new-password&keep_github=1",
        )
        .await;
    assert_eq!(done.status, StatusCode::SEE_OTHER, "{}", done.body);
    assert!(fx.access.has_password(&nina.id));
    assert!(
        fx.access
            .sign_in("nina@example.test", "a-new-password")
            .is_ok()
    );
    // The provider still signs her in, and she may now disconnect it.
    assert_eq!(fx.access.identities_of(&nina.id).len(), 1);
    let mut b = Browser::new();
    fx.stand.provide(P::GitHub, NINA);
    let (_, reply) = fx.sign_in_with(&mut b, P::GitHub, None).await;
    assert!(reply.opens_a_session(), "{}", reply.body);
}

const GITHUB_DISCONNECT: &str = "action=\"/ui/connections/github/disconnect\"";
const MICROSOFT_DISCONNECT: &str = "action=\"/ui/connections/microsoft/disconnect\"";
const ONLY_WAY: &str =
    "Your only way to sign in. Set a password or connect another account to disconnect it.";

#[tokio::test]
async fn the_account_page_offers_no_disconnect_for_the_last_way_in() {
    let fx = Fixture::new().await;
    // An account a provider made has no password: GitHub is the only way in.
    let mut b = nina_with(&fx, P::GitHub).await;
    let page = fx.get(&mut b, "/account").await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(!page.body.contains(GITHUB_DISCONNECT), "{}", page.body);
    assert!(!page.body.contains(">Disconnect</button>"), "{}", page.body);
    assert!(page.body.contains(ONLY_WAY), "{}", page.body);
    // A Disconnect sent anyway is refused: the account keeps GitHub, remembers no removal, and
    // GitHub still signs it in.
    let refused = fx
        .post(&mut b, "/ui/connections/github/disconnect", "csrf=@csrf")
        .await;
    assert_eq!(refused.status, StatusCode::SEE_OTHER, "{}", refused.body);
    assert_eq!(refused.notice(), "onlyway-github");
    assert_eq!(fx.identities("nina@example.test").len(), 1);
    fx.stand.provide(P::GitHub, NINA);
    let (_, again) = fx.sign_in_with(&mut Browser::new(), P::GitHub, None).await;
    assert!(again.opens_a_session(), "{}", again.body);
    // The other provider is still offered.
    assert!(page.body.contains("action=\"/sign-in/microsoft/connect\""));
    // With a second identity either can go, and the line is gone.
    fx.stand.provide(
        P::Microsoft,
        Who {
            n: 516,
            login: "nina-ms",
            ..NINA
        },
    );
    let done = fx.connect(&mut b, P::Microsoft).await;
    assert_eq!(done.status, StatusCode::SEE_OTHER, "{}", done.body);
    let page = fx.get(&mut b, "/account").await;
    assert!(page.body.contains(GITHUB_DISCONNECT), "{}", page.body);
    assert!(page.body.contains(MICROSOFT_DISCONNECT), "{}", page.body);
    assert!(!page.body.contains(ONLY_WAY));
}

#[tokio::test]
async fn the_account_page_offers_disconnect_when_a_password_is_the_way_in_too() {
    let fx = Fixture::new().await;
    fx.person(ADA, "Ada").await;
    fx.joined(ADA, P::GitHub, ADA_AT_PROVIDER).await;
    let mut b = Browser::new();
    fx.sign_in(&mut b, ADA);
    let page = fx.get(&mut b, "/account").await;
    assert!(page.body.contains(GITHUB_DISCONNECT), "{}", page.body);
    assert!(!page.body.contains(ONLY_WAY));
}

#[tokio::test]
async fn the_connect_button_has_the_row_action_style_of_the_app() {
    let fx = Fixture::new().await;
    fx.person(ADA, "Ada").await;
    let mut b = Browser::new();
    fx.sign_in(&mut b, ADA);
    let page = fx.get(&mut b, "/account").await;
    for p in [P::GitHub, P::Microsoft, P::Google] {
        assert!(
            page.body.contains(&format!(
                "<button class=\"act\" type=\"submit\" aria-label=\"Connect {}\">Connect</button>",
                p.name()
            )),
            "{}",
            page.body
        );
    }
}

// ------------------------------------------------------------------------------ resetting a password

/// Ada has a password and GitHub; she asks for a reset link, and the mail's link is returned.
async fn reset_link_for_ada(fx: &Fixture) -> String {
    fx.person(ADA, "Ada").await;
    fx.joined(ADA, P::GitHub, ADA_AT_PROVIDER).await;
    let asked = fx
        .post(
            &mut Browser::new(),
            "/ui/forgot",
            "email=ada%40example.test",
        )
        .await;
    assert_eq!(asked.status, StatusCode::OK, "{}", asked.body);
    // The first mail told her GitHub was connected; the second has the reset link.
    let mails = fx.mail_to(ADA, 2).await;
    link_in(
        mails
            .iter()
            .find(|mail| mail.body.contains(&format!("{ORIGIN}/link/")))
            .expect("a mail with a link"),
    )
}

#[tokio::test]
async fn a_reset_removes_a_connected_provider_that_is_not_kept() {
    let fx = Fixture::new().await;
    let link = reset_link_for_ada(&fx).await;
    // The page lists the provider with an unticked box.
    let page = fx.get(&mut Browser::new(), &link).await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.body);
    assert!(
        page.body.contains("Keep GitHub (ada-hub) connected"),
        "{}",
        page.body
    );
    assert!(page.body.contains("name=\"keep_github\""));
    // Unticked: nothing is kept unless she ticks it.
    assert!(
        page.body
            .contains("<input type=\"checkbox\" name=\"keep_github\" value=\"1\">")
    );
    assert!(!page.body.contains("keep_microsoft"));
    // Saving without ticking it disconnects it, and says so.
    let mut b = Browser::new();
    let done = fx
        .post(
            &mut b,
            &form_path(&link),
            "password=a-new-password&password_confirm=a-new-password",
        )
        .await;
    assert_eq!(done.status, StatusCode::SEE_OTHER, "{}", done.body);
    assert_eq!(done.location().as_deref(), Some("/account?m=reset-github"));
    assert!(done.opens_a_session());
    assert!(fx.identities(ADA).is_empty());
    assert!(fx.access.sign_in(ADA, "a-new-password").is_ok());
    let audit = fx.audit();
    assert!(audit.contains("identity.disconnect"), "{audit}");
    let account = fx.get(&mut b, "/account?m=reset-github").await;
    assert!(
        account
            .body
            .contains("Your password was changed. Disconnected: GitHub."),
        "{}",
        account.body
    );
    // GitHub no longer signs her in, though it vouches for her address: the account remembers
    // that the reset took it off, so it does not join again by address.
    fx.stand.provide(P::GitHub, ADA_AT_PROVIDER);
    let (_, reply) = fx.sign_in_with(&mut Browser::new(), P::GitHub, None).await;
    assert_eq!(reply.status, StatusCode::CONFLICT, "{}", reply.body);
    assert!(
        reply.body.contains("GitHub was disconnected")
            && reply.body.contains("then choose Connect beside GitHub"),
        "{}",
        reply.body
    );
    assert!(!reply.opens_a_session());
    assert!(fx.identities(ADA).is_empty());
}

#[tokio::test]
async fn a_reset_keeps_a_connected_provider_that_is_ticked() {
    let fx = Fixture::new().await;
    let link = reset_link_for_ada(&fx).await;
    let done = fx
        .post(
            &mut Browser::new(),
            &form_path(&link),
            "password=a-new-password&password_confirm=a-new-password&keep_github=1",
        )
        .await;
    assert_eq!(done.status, StatusCode::SEE_OTHER, "{}", done.body);
    assert_eq!(done.location().as_deref(), Some("/"));
    assert_eq!(fx.identities(ADA).len(), 1);
    assert!(!fx.audit().contains("identity.disconnect"));
}

// ------------------------------------------------------------------------------ small hardening

#[tokio::test]
async fn a_new_account_made_through_a_provider_gets_no_notice_mail_but_a_join_to_an_existing_one_does()
 {
    let fx = Fixture::new().await;
    // A stranger's new account: the person has just made it.
    nina_with(&fx, P::GitHub).await;
    fx.no_mail().await;
    // A join to an account that already existed: its address is told.
    fx.person(ADA, "Ada").await;
    fx.joined(ADA, P::GitHub, ADA_AT_PROVIDER).await;
    let mails = fx.mail_to(ADA, 1).await;
    assert_eq!(
        mails[0].subject,
        "GitHub was connected to your RichView account"
    );
    assert!(fx.mail.0.lock().unwrap().iter().all(|mail| mail.to == ADA));
}

#[tokio::test]
async fn a_back_address_longer_than_256_bytes_is_ignored() {
    let fx = Fixture::new().await;
    fx.person(ADA, "Ada").await;
    fx.joined(ADA, P::GitHub, ADA_AT_PROVIDER).await;
    let at = |length: usize| {
        let base = "/r/ab123456/q3?x=";
        format!("{base}{}", "a".repeat(length - base.len()))
    };
    // 256 bytes is kept (a hosted report's address becomes its page here) ...
    let mut b = Browser::new();
    let (_, kept) = fx.sign_in_with(&mut b, P::GitHub, Some(&at(256))).await;
    assert_eq!(
        kept.refresh().as_deref(),
        Some("/reports/ab123456/q3"),
        "{}",
        kept.body
    );
    // ... and one byte more is ignored, as if there were none.
    let mut c = Browser::new();
    let (_, long) = fx.sign_in_with(&mut c, P::GitHub, Some(&at(257))).await;
    assert!(long.opens_a_session(), "{}", long.body);
    assert_eq!(long.refresh().as_deref(), Some("/"));
}

#[tokio::test]
async fn the_sign_in_pages_are_not_cached_and_send_no_referrer() {
    let fx = Fixture::new().await;
    fx.person(ADA, "Ada").await;
    fx.joined(ADA, P::GitHub, ADA_AT_PROVIDER).await;
    let mut b = Browser::new();
    let started = fx.get(&mut b, "/sign-in/github").await;
    let answer = fx.provider_answer(&started).await;
    let callback = fx.arrive(&mut b, &answer).await;
    assert!(callback.opens_a_session(), "{}", callback.body);
    for (name, reply) in [("start", &started), ("callback", &callback)] {
        assert_eq!(reply.headers[header::CACHE_CONTROL], "no-store", "{name}");
        assert_eq!(
            reply.headers[header::REFERRER_POLICY],
            "no-referrer",
            "{name}"
        );
    }
    // A refusal on the same routes carries them too.
    let refused = fx
        .arrive(
            &mut Browser::new(),
            "/sign-in/github/callback?code=x&state=y",
        )
        .await;
    assert_eq!(refused.headers[header::CACHE_CONTROL], "no-store");
    assert_eq!(refused.headers[header::REFERRER_POLICY], "no-referrer");
}

#[tokio::test]
async fn a_callback_for_a_state_nobody_started_is_not_audited_but_one_for_a_real_flow_is() {
    let fx = Fixture::new().await;
    fx.person(ADA, "Ada").await;
    fx.joined(ADA, P::GitHub, ADA_AT_PROVIDER).await;
    let before = fx.audit().matches("identity.sign-in").count();
    // Junk: a made-up state, with or without the provider's error.
    for query in ["code=x&state=made-up", "error=access_denied&state=made-up"] {
        let junk = fx
            .arrive(
                &mut Browser::new(),
                &format!("/sign-in/github/callback?{query}"),
            )
            .await;
        assert_eq!(junk.status, StatusCode::BAD_REQUEST);
    }
    assert_eq!(fx.audit().matches("identity.sign-in").count(), before);
    // A real flow answered from the wrong browser is a refused sign-in worth a line.
    let mut victim = Browser::new();
    let started = fx.get(&mut victim, "/sign-in/github").await;
    let answer = fx.provider_answer(&started).await;
    let mut other = Browser::new();
    fx.get(&mut other, "/sign-in/github").await;
    let refused = fx.arrive(&mut other, &answer).await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    assert_eq!(fx.audit().matches("identity.sign-in").count(), before + 1);
}

#[tokio::test]
async fn the_account_page_shows_only_its_own_messages_for_its_codes() {
    let fx = Fixture::new().await;
    fx.person(ADA, "Ada").await;
    let mut b = Browser::new();
    fx.sign_in(&mut b, ADA);
    // Free text in the address is not shown.
    let spoof = fx
        .get(
            &mut b,
            "/account?m=Your%20account%20is%20locked.%20Call%20this%20number",
        )
        .await;
    assert_eq!(spoof.status, StatusCode::OK);
    assert!(!spoof.body.contains("locked"), "{}", spoof.body);
    // An unknown code shows nothing, and a known one its fixed text.
    for code in ["nonsense", "connected-nobody", "connected-github-evil", ""] {
        let page = fx.get(&mut b, &format!("/account?m={code}")).await;
        assert!(!page.body.contains("Connected "), "{code}: {}", page.body);
    }
    let known = fx.get(&mut b, "/account?m=connected-github").await;
    assert!(known.body.contains("Connected GitHub."), "{}", known.body);
}

// ------------------------------------------------------------------------------ what is offered

#[tokio::test]
async fn only_a_configured_provider_is_offered() {
    let fx = Fixture::with(true, false, false).await;
    fx.person(ADA, "Ada").await;
    let mut b = Browser::new();
    let page = fx.get(&mut b, "/").await;
    assert!(
        page.body.contains("href=\"/sign-in/github\""),
        "{}",
        page.body
    );
    assert!(!page.body.contains("/sign-in/microsoft"));
    assert_eq!(
        fx.get(&mut b, "/sign-in/microsoft").await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        fx.get(&mut b, "/sign-in/google").await.status,
        StatusCode::NOT_FOUND
    );
    let refused = fx
        .post(&mut b, "/sign-in/microsoft/invite", "token=whatever")
        .await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_server_with_no_provider_shows_no_button_and_has_no_sign_in_routes() {
    let fx = Fixture::with(false, false, false).await;
    fx.person(ADA, "Ada").await;
    let mut b = Browser::new();
    for path in ["/", "/sign-up"] {
        let page = fx.get(&mut b, path).await;
        assert_eq!(page.status, StatusCode::OK, "{path}");
        assert!(!page.body.contains("/sign-in/"), "{path}: {}", page.body);
        assert!(!page.body.contains("class=\"providers\""), "{path}");
    }
    for slug in ["github", "microsoft", "google"] {
        assert_eq!(
            fx.get(&mut b, &format!("/sign-in/{slug}")).await.status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            fx.get(&mut b, &format!("/sign-in/{slug}/callback?code=x&state=y"))
                .await
                .status,
            StatusCode::NOT_FOUND
        );
    }
    fx.sign_in(&mut b, ADA);
    let account = fx.get(&mut b, "/account").await;
    assert_eq!(account.status, StatusCode::OK);
    assert!(!account.body.contains("/ui/connections/"));
    assert!(!account.body.contains("/sign-in/"));
}

#[tokio::test]
async fn the_buttons_are_on_the_sign_in_sign_up_and_invitation_pages_with_inline_logos() {
    let fx = Fixture::new().await;
    let mut b = Browser::new();
    let sign_in = fx.get(&mut b, "/?next=%2Fr%2Fab123456%2Fq3").await;
    let sign_up = fx.get(&mut b, "/sign-up").await;
    for (page, verb) in [(&sign_in, "Continue"), (&sign_up, "Sign up")] {
        assert_eq!(page.status, StatusCode::OK);
        for p in [P::GitHub, P::Microsoft, P::Google] {
            assert!(
                page.body
                    .contains(&format!("<span>{verb} with {}</span>", p.name())),
                "{verb} {}: {}",
                p.name(),
                page.body
            );
        }
        assert!(page.body.matches("<svg class=\"logo\"").count() >= 3);
        // No external script, image or font: the logos are inline.
        assert!(!page.body.contains("src=\"http"));
        assert!(!page.body.contains("url(http"));
    }
    // The page the person was going to comes back after the provider.
    assert!(
        sign_in
            .body
            .contains("href=\"/sign-in/github?back=/r/ab123456/q3\""),
        "{}",
        sign_in.body
    );
    let (link, _) = invitation_to(&fx, "ivy@example.test").await;
    let invitation = fx.get(&mut b, &link).await;
    for p in [P::GitHub, P::Microsoft, P::Google] {
        assert!(
            invitation
                .body
                .contains(&format!("action=\"/sign-in/{}/invite\"", p.slug())),
            "{}",
            invitation.body
        );
    }
    assert!(invitation.body.contains("<svg class=\"logo\""));
    // One terms line, at the foot under the provider buttons.
    let terms = "By creating an account, you agree to the";
    assert_eq!(
        invitation.body.matches(terms).count(),
        1,
        "{}",
        invitation.body
    );
    assert!(
        invitation.body.find(terms) > invitation.body.rfind("/invite\""),
        "{}",
        invitation.body
    );
    // Signed in, the Account page offers to connect each one.
    fx.person(ADA, "Ada").await;
    fx.sign_in(&mut b, ADA);
    let account = fx.get(&mut b, "/account").await;
    for p in [P::GitHub, P::Microsoft, P::Google] {
        assert!(
            account
                .body
                .contains(&format!("action=\"/sign-in/{}/connect\"", p.slug()))
        );
    }
}

// ------------------------------------------------------------------------------ the test flag

#[test]
fn the_test_flag_is_refused_off_loopback_and_on_a_public_address() {
    let root = std::env::current_dir()
        .unwrap()
        .join("tmp")
        .join(format!("social-flag-{}", std::process::id()));
    let data = root.join("data");
    let outbox = root.join("outbox");
    let canary = "flag-test-secret-canary";
    let run = |public: &str, report: &str, base: &str| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_richview"));
        command
            .args(["server", "--hosted", "--data-dir"])
            .arg(&data)
            .args([
                "--public-url",
                public,
                "--report-origin",
                report,
                "--mail-outbox",
            ])
            .arg(&outbox)
            .args(["--github-client-id", "flag-test-client"])
            .args(["--identity-provider-test-base", base])
            .env("RICHVIEW_GITHUB_CLIENT_SECRET", canary);
        for name in [
            "RICHVIEW_TOKEN",
            "RICHVIEW_ADMIN_EMAIL",
            "RICHVIEW_QUOTA_BYTES",
            "RICHVIEW_ORG_QUOTA_BYTES",
            "RICHVIEW_ORG_REPORT_LIMIT",
            "RICHVIEW_MAIL_DAILY_LIMIT",
            "RICHVIEW_OPERATOR_PORT",
            "RICHVIEW_OPERATOR_TOKEN",
            "RICHVIEW_SMTP_HOST",
            "RICHVIEW_MICROSOFT_CLIENT_ID",
            "RICHVIEW_MICROSOFT_CLIENT_SECRET",
            "RICHVIEW_GITHUB_CLIENT_ID",
        ] {
            command.env_remove(name);
        }
        let output = command.output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert!(!stderr.contains(canary), "the secret was printed: {stderr}");
        assert!(!data.exists(), "the data directory was opened");
        (output.status, stderr)
    };
    // The hosted configuration: a public address that is not loopback, a stand-in that is.
    let (status, stderr) = run(
        "https://app.test",
        "https://reports.test",
        "http://127.0.0.1:9",
    );
    assert!(!status.success(), "{stderr}");
    assert!(stderr.contains("public address on loopback"), "{stderr}");
    // A stand-in that is not on loopback over plain HTTP, whatever the server's own address.
    for base in [
        "http://stand-in.test",
        "https://127.0.0.1:9",
        "http://127.0.0.1:9/extra",
        "http://user@127.0.0.1:9",
        "http://192.0.2.1:9",
    ] {
        let (status, stderr) = run("http://127.0.0.1:18080", "http://localhost:18081", base);
        assert!(!status.success(), "{base}: {stderr}");
        assert!(
            stderr.contains("must be a loopback address"),
            "{base}: {stderr}"
        );
    }
    // Both on loopback, the flag is accepted: the start goes on and stops at the next missing
    // setting, the limits, which this run leaves out.
    let (status, stderr) = run(
        "http://127.0.0.1:18080",
        "http://localhost:18081",
        "http://127.0.0.1:9",
    );
    assert!(!status.success(), "{stderr}");
    assert!(stderr.contains("limits"), "{stderr}");
}
