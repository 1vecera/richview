//! Signing in to a hosted server with an account at GitHub, Microsoft or Google (FEA-SF-19,
//! FEA-SF-20).
//!
//! The server runs the standard sign-in itself: OAuth with PKCE for GitHub, and OpenID Connect
//! with PKCE, a state and a nonce for Microsoft and Google. This module holds the part that does not touch
//! people: the settings, the flows waiting for their provider's answer, the calls to the provider
//! and the checks of what it answers. What an answer means for an account is `access`'s business.
//!
//! Four rules hold throughout.
//!
//! - **A secret never leaves the process.** A client secret comes from the environment only, is
//!   sent to its provider alone and is never printed or logged. A provider's token is used for the
//!   calls that read the profile and is then dropped: it is never stored.
//! - **An identity is the provider and its id for the person, never an address.** GitHub's numeric
//!   user id; Microsoft's tenant id with the object id; Google's `sub`. An address counts as vouched for only when
//!   the provider itself says it checked it (see [`Profile::email`]).
//! - **A flow belongs to one browser.** Starting a flow sets a cookie whose digest the waiting flow
//!   keeps; the answer is accepted from that browser only, once, within ten minutes.
//! - **Tests never reach a real provider.** A hidden setting points every provider at a stand-in
//!   on loopback, and it is refused unless the server itself is on loopback.
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// The cookie that ties a provider's answer to the browser that asked.
pub const COOKIE: &str = "rv_oauth";
/// How long a started flow, and what it leaves waiting for a person's decision, lasts.
pub const FLOW_SECONDS: u64 = 600;
/// Flows waiting at once, across everyone; a new one is refused when the store is full.
const MAX_PENDING: usize = 10_000;
/// Provider answers waiting for the person's confirmation at once.
const MAX_HELD: usize = 2_000;
/// Flows one address may start in ten minutes.
const STARTS_PER_ADDRESS: usize = 30;
/// Addresses counted at once; beyond it, new ones wait.
const MAX_COUNTED: usize = 1024;
/// How long one call to a provider may take, answer included.
const CALL_SECONDS: u64 = 10;
/// The most a provider's token answer may weigh.
const TOKEN_ANSWER_BYTES: usize = 64 * 1024;
/// The most a provider's profile answer may weigh.
const PROFILE_ANSWER_BYTES: usize = 256 * 1024;
/// The most an ID token may weigh.
const ID_TOKEN_BYTES: usize = 16 * 1024;
/// How far a provider's clock may differ from ours.
const SKEW_SECONDS: u64 = 300;

/// A provider a person may sign in with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    GitHub,
    Microsoft,
    Google,
}

impl Provider {
    /// Every provider, in the order the pages offer them.
    pub const ALL: [Self; 3] = [Self::GitHub, Self::Microsoft, Self::Google];

    /// The provider in an address and in the stored identity.
    pub fn slug(self) -> &'static str {
        match self {
            Self::GitHub => "github",
            Self::Microsoft => "microsoft",
            Self::Google => "google",
        }
    }

    /// The provider as a person reads it.
    pub fn name(self) -> &'static str {
        match self {
            Self::GitHub => "GitHub",
            Self::Microsoft => "Microsoft",
            Self::Google => "Google",
        }
    }

    pub fn parse(slug: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|provider| provider.slug() == slug)
    }
}

/// A secret that cannot be printed by accident.
#[derive(Clone)]
struct Secret(String);

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(..)")
    }
}

#[derive(Clone, Debug)]
struct Client {
    id: String,
    secret: Secret,
}

/// What an operator sets: each provider's client, and the hidden stand-in for tests. A secret
/// comes from the environment only; the caller reads it there.
#[derive(Default)]
pub struct Settings {
    pub github_id: Option<String>,
    pub github_secret: Option<String>,
    pub microsoft_id: Option<String>,
    pub microsoft_secret: Option<String>,
    pub google_id: Option<String>,
    pub google_secret: Option<String>,
    /// Where every provider's endpoints are, instead of the real ones. Loopback only.
    pub test_base: Option<String>,
}

const LOOPBACK: [&str; 3] = ["127.0.0.1", "localhost", "[::1]"];

/// What the start limit counts a client by: its IPv4 address, or the /64 of its IPv6 address,
/// since one IPv6 subscriber has a whole /64 to start from.
pub(crate) fn start_key(address: std::net::IpAddr) -> String {
    match address {
        std::net::IpAddr::V4(v4) => v4.to_string(),
        std::net::IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => v4.to_string(),
            None => {
                let [a, b, c, d, ..] = v6.segments();
                format!("{a:x}:{b:x}:{c:x}:{d:x}::/64")
            }
        },
    }
}

/// An address on this machine over plain HTTP, with nothing after the host: the only address a
/// stand-in may have.
fn loopback(url: &str) -> Option<reqwest::Url> {
    let url = reqwest::Url::parse(url).ok()?;
    (url.scheme() == "http"
        && url.host_str().is_some_and(|host| LOOPBACK.contains(&host))
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.path() == "/")
        .then_some(url)
}

/// Whether `origin` is a loopback address, for the server's own public address.
pub fn is_loopback_origin(origin: &str) -> bool {
    loopback(origin).is_some()
}

#[derive(Clone)]
struct Endpoints {
    github_authorize: String,
    github_token: String,
    github_user: String,
    github_emails: String,
    microsoft_authorize: String,
    microsoft_token: String,
    google_authorize: String,
    google_token: String,
}

impl Endpoints {
    fn real() -> Self {
        Self {
            github_authorize: "https://github.com/login/oauth/authorize".into(),
            github_token: "https://github.com/login/oauth/access_token".into(),
            github_user: "https://api.github.com/user".into(),
            github_emails: "https://api.github.com/user/emails".into(),
            microsoft_authorize: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize"
                .into(),
            microsoft_token: "https://login.microsoftonline.com/common/oauth2/v2.0/token".into(),
            google_authorize: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            google_token: "https://oauth2.googleapis.com/token".into(),
        }
    }

    fn stand_in(base: &reqwest::Url) -> Self {
        let at = |path: &str| format!("{}{path}", base.as_str().trim_end_matches('/'));
        Self {
            github_authorize: at("/github/authorize"),
            github_token: at("/github/token"),
            github_user: at("/github/user"),
            github_emails: at("/github/user/emails"),
            microsoft_authorize: at("/microsoft/authorize"),
            microsoft_token: at("/microsoft/token"),
            google_authorize: at("/google/authorize"),
            google_token: at("/google/token"),
        }
    }
}

/// A setting that is set to something.
fn present(text: &Option<String>) -> Option<&str> {
    text.as_deref().filter(|text| !text.is_empty())
}

impl Settings {
    /// The sign-in a server offers, or `None` when no provider has a client. A provider is on when
    /// its id and its secret are both set; one without the other is an error, so a half-made
    /// setting never looks like a working one. `public_origin` is the server's own address, which
    /// the stand-in requires to be on loopback.
    pub fn build(self, public_origin: Option<&str>) -> Result<Option<Social>, String> {
        let mut clients = [None, None, None];
        for (slot, provider, id, secret) in [
            (0, Provider::GitHub, &self.github_id, &self.github_secret),
            (
                1,
                Provider::Microsoft,
                &self.microsoft_id,
                &self.microsoft_secret,
            ),
            (2, Provider::Google, &self.google_id, &self.google_secret),
        ] {
            let name = provider.name();
            let upper = provider.slug().to_ascii_uppercase();
            match (present(id), present(secret)) {
                (Some(id), Some(secret)) => {
                    if id.len() > 256
                        || !id.bytes().all(|b| b.is_ascii_graphic())
                        || secret.len() > 1024
                        || !secret.bytes().all(|b| b.is_ascii_graphic())
                    {
                        return Err(format!(
                            "The {name} client id and secret must be short and without spaces."
                        ));
                    }
                    clients[slot] = Some(Client {
                        id: id.to_owned(),
                        secret: Secret(secret.to_owned()),
                    });
                }
                (None, None) => {}
                (Some(_), None) => {
                    return Err(format!(
                        "Sign-in with {name} needs RICHVIEW_{upper}_CLIENT_SECRET beside its client id."
                    ));
                }
                (None, Some(_)) => {
                    return Err(format!(
                        "Sign-in with {name} needs RICHVIEW_{upper}_CLIENT_ID beside its client secret."
                    ));
                }
            }
        }
        let none = clients.iter().all(Option::is_none);
        let [github, microsoft, google] = clients;
        let endpoints = match present(&self.test_base) {
            None => Endpoints::real(),
            Some(base) => {
                if none {
                    return Err(
                        "--identity-provider-test-base needs a sign-in provider's client".into(),
                    );
                }
                let base = loopback(base).ok_or(
                    "--identity-provider-test-base must be a loopback address over http (127.0.0.1, localhost or [::1])",
                )?;
                // The hosted server at the public address can never be pointed at a stand-in.
                if !public_origin.is_some_and(is_loopback_origin) {
                    return Err(
                        "--identity-provider-test-base needs the server's own public address on loopback"
                            .into(),
                    );
                }
                Endpoints::stand_in(&base)
            }
        };
        if none {
            return Ok(None);
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(CALL_SECONDS))
            .connect_timeout(Duration::from_secs(CALL_SECONDS))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("RichView")
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Some(Social {
            github,
            microsoft,
            google,
            endpoints,
            http,
            store: Mutex::new(Store::default()),
        }))
    }
}

/// Why a person is signing in, and so what the provider's answer is for.
#[derive(Clone)]
pub enum Intent {
    /// A plain sign-in or sign-up.
    SignIn,
    /// A signed-in person connects the provider to their account.
    Connect { person: String },
    /// Someone accepts an invitation with the provider; `token` is the invitation's link token.
    Invite { token: String },
}

/// A flow waiting for its provider's answer.
pub struct Pending {
    pub provider: Provider,
    pub intent: Intent,
    /// Where to go afterwards: a local path the person already chose, or none.
    pub back: Option<String>,
    redirect_uri: String,
    verifier: String,
    nonce: String,
    /// Digest of the browser's cookie.
    browser: String,
    until: u64,
}

/// What starting a flow gives the caller.
pub struct Begun {
    /// Where to send the browser: the provider's page.
    pub location: String,
    /// The cookie's value; only its digest is kept here.
    pub browser: String,
}

/// Why a flow could not start.
#[derive(Debug, PartialEq, Eq)]
pub enum Refused {
    /// The provider has no client here.
    Unavailable,
    /// Too many flows are waiting, or this address started too many.
    Busy,
}

/// What a person's provider account says about them, as far as RichView may rely on it.
pub struct Profile {
    pub provider: Provider,
    /// The provider's own id for the account. Never an address.
    pub subject: String,
    /// What the Account page calls the connection: GitHub's login, or Microsoft's or Google's address. Shown,
    /// never trusted.
    pub label: String,
    /// The name the provider gives, as a person may type it; shown, never trusted.
    pub name: Option<String>,
    /// An address the provider itself checked, normalized. `None` when it vouched for none, and
    /// then no account is made or joined on an address.
    pub email: Option<String>,
}

/// What a provider's answer is kept for until the person decides.
pub enum Held {
    /// A new account the person may create with this address.
    NewAccount {
        profile: Profile,
        email: String,
        back: Option<String>,
    },
    /// A connection waiting for the signed-in person's own browser to come back.
    Connect { person: String, profile: Profile },
}

struct Waiting {
    held: Held,
    provider: Provider,
    browser: String,
    until: u64,
}

#[derive(Default)]
struct Store {
    /// Digest of the state value → the flow.
    pending: BTreeMap<String, Pending>,
    /// Digest of the confirmation's id → what waits.
    held: BTreeMap<String, Waiting>,
    starts: BTreeMap<String, VecDeque<u64>>,
}

/// Why a flow's answer was not accepted. One answer for every reason.
#[derive(Debug, PartialEq, Eq)]
pub struct Stale;

/// Why a provider's answer could not be used.
#[derive(Debug, PartialEq, Eq)]
pub enum Failure {
    /// The provider could not be reached, or answered in a way this server does not understand.
    Unreachable,
    /// The provider refused the code or the token.
    Refused,
    /// The ID token did not pass a check; the reason names the check.
    Token(&'static str),
}

pub struct Social {
    github: Option<Client>,
    microsoft: Option<Client>,
    google: Option<Client>,
    endpoints: Endpoints,
    http: reqwest::Client,
    store: Mutex<Store>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

/// `bytes` random bytes from the operating system, as unpadded URL-safe base64.
fn random_text(bytes: usize) -> Result<String, Refused> {
    let mut buffer = vec![0u8; bytes];
    getrandom::fill(&mut buffer).map_err(|_| Refused::Busy)?;
    Ok(URL_SAFE_NO_PAD.encode(buffer))
}

/// Two digests, compared without an early exit.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}

/// The PKCE challenge of a verifier (RFC 7636, S256).
pub fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// Drops the times older than `window` and says how many are left for `key`.
fn counted(
    counters: &mut BTreeMap<String, VecDeque<u64>>,
    key: &str,
    at: u64,
    window: u64,
) -> usize {
    counters.retain(|_, times| {
        while times.front().is_some_and(|time| time + window <= at) {
            times.pop_front();
        }
        !times.is_empty()
    });
    counters.get(key).map_or(0, VecDeque::len)
}

impl Social {
    /// The providers this server has a client for, in the order pages offer them.
    pub fn providers(&self) -> Vec<Provider> {
        Provider::ALL
            .into_iter()
            .filter(|provider| self.client(*provider).is_some())
            .collect()
    }

    pub fn enabled(&self, provider: Provider) -> bool {
        self.client(provider).is_some()
    }

    fn client(&self, provider: Provider) -> Option<&Client> {
        match provider {
            Provider::GitHub => self.github.as_ref(),
            Provider::Microsoft => self.microsoft.as_ref(),
            Provider::Google => self.google.as_ref(),
        }
    }

    fn store(&self) -> std::sync::MutexGuard<'_, Store> {
        // A poisoned store holds only waiting flows; keep serving rather than refuse everyone.
        self.store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// How many flows and confirmations wait. For tests of the bounds.
    pub fn waiting(&self) -> usize {
        let store = self.store();
        store.pending.len() + store.held.len()
    }

    /// Starts a flow for `provider`: remembers it and says where to send the browser.
    /// `redirect_uri` is this server's callback address for the provider. `address` is the
    /// client's, as the trusted proxy reports it.
    pub fn begin(
        &self,
        provider: Provider,
        intent: Intent,
        back: Option<String>,
        redirect_uri: &str,
        address: Option<std::net::IpAddr>,
    ) -> Result<Begun, Refused> {
        self.begin_at(provider, intent, back, redirect_uri, address, now())
    }

    fn begin_at(
        &self,
        provider: Provider,
        intent: Intent,
        back: Option<String>,
        redirect_uri: &str,
        address: Option<std::net::IpAddr>,
        at: u64,
    ) -> Result<Begun, Refused> {
        let client = self.client(provider).ok_or(Refused::Unavailable)?;
        let state = random_text(32)?;
        let verifier = random_text(32)?;
        let nonce = random_text(24)?;
        let browser = random_text(32)?;
        let challenge = pkce_challenge(&verifier);
        let location = match provider {
            Provider::GitHub => reqwest::Url::parse_with_params(
                &self.endpoints.github_authorize,
                [
                    ("client_id", client.id.as_str()),
                    ("redirect_uri", redirect_uri),
                    ("scope", "user:email"),
                    ("state", state.as_str()),
                    ("code_challenge", challenge.as_str()),
                    ("code_challenge_method", "S256"),
                ],
            ),
            Provider::Microsoft => reqwest::Url::parse_with_params(
                &self.endpoints.microsoft_authorize,
                [
                    ("client_id", client.id.as_str()),
                    ("response_type", "code"),
                    ("redirect_uri", redirect_uri),
                    ("response_mode", "query"),
                    ("scope", "openid profile email"),
                    ("state", state.as_str()),
                    ("nonce", nonce.as_str()),
                    ("code_challenge", challenge.as_str()),
                    ("code_challenge_method", "S256"),
                ],
            ),
            Provider::Google => reqwest::Url::parse_with_params(
                &self.endpoints.google_authorize,
                [
                    ("client_id", client.id.as_str()),
                    ("response_type", "code"),
                    ("redirect_uri", redirect_uri),
                    ("scope", "openid email profile"),
                    ("state", state.as_str()),
                    ("nonce", nonce.as_str()),
                    ("code_challenge", challenge.as_str()),
                    ("code_challenge_method", "S256"),
                ],
            ),
        }
        .map_err(|_| Refused::Unavailable)?;
        let mut store = self.store();
        store.pending.retain(|_, flow| flow.until > at);
        store.held.retain(|_, waiting| waiting.until > at);
        let key = address.map_or_else(|| "unknown".to_owned(), start_key);
        let have = counted(&mut store.starts, &key, at, FLOW_SECONDS);
        if store.pending.len() >= MAX_PENDING
            || have >= STARTS_PER_ADDRESS
            || (have == 0 && store.starts.len() >= MAX_COUNTED)
        {
            return Err(Refused::Busy);
        }
        store.starts.entry(key).or_default().push_back(at);
        store.pending.insert(
            digest(&state),
            Pending {
                provider,
                intent,
                back,
                redirect_uri: redirect_uri.to_owned(),
                verifier,
                nonce,
                browser: digest(&browser),
                until: at + FLOW_SECONDS,
            },
        );
        Ok(Begun {
            location: location.into(),
            browser,
        })
    }

    /// The flow behind a provider's answer. It is removed on the first look, whatever the outcome,
    /// so an answer is never accepted twice; it is returned only for the provider it was started
    /// for, from the browser that started it, within its time.
    pub fn take(
        &self,
        state: &str,
        browser: Option<&str>,
        provider: Provider,
    ) -> Result<Pending, Stale> {
        self.take_at(state, browser, provider, now())
    }

    fn take_at(
        &self,
        state: &str,
        browser: Option<&str>,
        provider: Provider,
        at: u64,
    ) -> Result<Pending, Stale> {
        let flow = self.store().pending.remove(&digest(state)).ok_or(Stale)?;
        let from_that_browser = browser.is_some_and(|value| same(&digest(value), &flow.browser));
        (flow.until > at && flow.provider == provider && from_that_browser)
            .then_some(flow)
            .ok_or(Stale)
    }

    /// Whether a flow with this state is waiting: false for a state nobody here started.
    pub fn knows(&self, state: &str) -> bool {
        self.store().pending.contains_key(&digest(state))
    }

    /// Forgets a flow whose provider answered with an error, so it cannot be used afterwards.
    pub fn discard(&self, state: &str) {
        self.store().pending.remove(&digest(state));
    }

    /// Keeps a provider's answer until the person decides what to do with it. The browser's
    /// cookie must come back with the decision.
    pub fn hold(&self, held: Held, provider: Provider, browser: &str) -> Result<String, Refused> {
        self.hold_at(held, provider, browser, now())
    }

    fn hold_at(
        &self,
        held: Held,
        provider: Provider,
        browser: &str,
        at: u64,
    ) -> Result<String, Refused> {
        let id = random_text(32)?;
        let mut store = self.store();
        store.held.retain(|_, waiting| waiting.until > at);
        if store.held.len() >= MAX_HELD {
            return Err(Refused::Busy);
        }
        store.held.insert(
            digest(&id),
            Waiting {
                held,
                provider,
                browser: digest(browser),
                until: at + FLOW_SECONDS,
            },
        );
        Ok(id)
    }

    /// What a confirmation id waits for. Removed on the first look, whatever the outcome.
    pub fn release(
        &self,
        id: &str,
        browser: Option<&str>,
        provider: Provider,
    ) -> Result<Held, Stale> {
        self.release_at(id, browser, provider, now())
    }

    fn release_at(
        &self,
        id: &str,
        browser: Option<&str>,
        provider: Provider,
        at: u64,
    ) -> Result<Held, Stale> {
        let waiting = self.store().held.remove(&digest(id)).ok_or(Stale)?;
        let from_that_browser = browser.is_some_and(|value| same(&digest(value), &waiting.browser));
        (waiting.until > at && waiting.provider == provider && from_that_browser)
            .then_some(waiting.held)
            .ok_or(Stale)
    }

    /// Exchanges the provider's code for what it says about the person.
    pub async fn profile(&self, flow: &Pending, code: &str) -> Result<Profile, Failure> {
        if code.is_empty() || code.len() > 2048 || !code.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(Failure::Refused);
        }
        match flow.provider {
            Provider::GitHub => self.github_profile(flow, code).await,
            Provider::Microsoft | Provider::Google => self.oidc_profile(flow, code).await,
        }
    }

    /// One call to a provider, its answer read up to `limit` bytes.
    async fn call(
        &self,
        request: reqwest::RequestBuilder,
        limit: usize,
    ) -> Result<(reqwest::StatusCode, Vec<u8>), Failure> {
        let mut response = request.send().await.map_err(|_| Failure::Unreachable)?;
        let status = response.status();
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Failure::Unreachable)? {
            if body.len() + chunk.len() > limit {
                return Err(Failure::Unreachable);
            }
            body.extend_from_slice(&chunk);
        }
        Ok((status, body))
    }

    async fn github_profile(&self, flow: &Pending, code: &str) -> Result<Profile, Failure> {
        let client = self.github.as_ref().ok_or(Failure::Refused)?;
        let (status, body) = self
            .call(
                self.http
                    .post(&self.endpoints.github_token)
                    .header(reqwest::header::ACCEPT, "application/json")
                    .form(&[
                        ("client_id", client.id.as_str()),
                        ("client_secret", client.secret.0.as_str()),
                        ("code", code),
                        ("redirect_uri", flow.redirect_uri.as_str()),
                        ("code_verifier", flow.verifier.as_str()),
                    ]),
                TOKEN_ANSWER_BYTES,
            )
            .await?;
        let answer: Value = serde_json::from_slice(&body).map_err(|_| Failure::Unreachable)?;
        let token = answer["access_token"]
            .as_str()
            .filter(|token| {
                status.is_success()
                    && !token.is_empty()
                    && token.len() <= 4096
                    && token.bytes().all(|b| b.is_ascii_graphic())
            })
            .ok_or(Failure::Refused)?;
        let get = |url: &str| {
            self.http
                .get(url)
                .bearer_auth(token)
                .header(reqwest::header::ACCEPT, "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
        };
        let (status, body) = self
            .call(get(&self.endpoints.github_user), PROFILE_ANSWER_BYTES)
            .await?;
        if !status.is_success() {
            return Err(Failure::Refused);
        }
        let user: Value = serde_json::from_slice(&body).map_err(|_| Failure::Unreachable)?;
        let id = user["id"]
            .as_u64()
            .filter(|id| *id > 0)
            .ok_or(Failure::Unreachable)?;
        let login = user["login"]
            .as_str()
            .filter(|login| {
                !login.is_empty()
                    && login.len() <= 39
                    && login
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            })
            .map_or_else(|| "GitHub account".to_owned(), str::to_owned);
        let emails = match self
            .call(get(&self.endpoints.github_emails), PROFILE_ANSWER_BYTES)
            .await
        {
            Ok((status, body)) if status.is_success() => github_vouched(&body),
            // Without the list nothing is vouched for; the sign-in goes on as a stranger's.
            _ => None,
        };
        Ok(Profile {
            provider: Provider::GitHub,
            subject: id.to_string(),
            label: login,
            name: user["name"].as_str().and_then(person_name),
            email: emails,
        })
    }

    /// Microsoft's and Google's answer: the ID token their token endpoint returns for the code.
    async fn oidc_profile(&self, flow: &Pending, code: &str) -> Result<Profile, Failure> {
        let client = self.client(flow.provider).ok_or(Failure::Refused)?;
        let endpoint = match flow.provider {
            Provider::Google => &self.endpoints.google_token,
            _ => &self.endpoints.microsoft_token,
        };
        let (status, body) = self
            .call(
                self.http
                    .post(endpoint)
                    .header(reqwest::header::ACCEPT, "application/json")
                    .form(&[
                        ("client_id", client.id.as_str()),
                        ("client_secret", client.secret.0.as_str()),
                        ("grant_type", "authorization_code"),
                        ("code", code),
                        ("redirect_uri", flow.redirect_uri.as_str()),
                        ("code_verifier", flow.verifier.as_str()),
                    ]),
                TOKEN_ANSWER_BYTES,
            )
            .await?;
        let answer: Value = serde_json::from_slice(&body).map_err(|_| Failure::Unreachable)?;
        let id_token = answer["id_token"]
            .as_str()
            .filter(|_| status.is_success())
            .ok_or(Failure::Refused)?;
        let expect = Expect {
            client_id: &client.id,
            nonce: &flow.nonce,
            now: now(),
        };
        match flow.provider {
            Provider::Google => google_identity(id_token, &expect),
            _ => microsoft_identity(id_token, &expect),
        }
        .map_err(Failure::Token)
    }
}

/// A name as a person may type it: one line, without control characters, at most 80 characters.
pub fn person_name(text: &str) -> Option<String> {
    let name: String = text
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(80)
        .collect();
    let name = name.trim().to_owned();
    (!name.is_empty()).then_some(name)
}

#[derive(Deserialize)]
struct GitHubEmail {
    email: String,
    #[serde(default)]
    primary: bool,
    #[serde(default)]
    verified: bool,
}

/// The address GitHub vouches for: its primary address, marked verified. Nothing else counts,
/// and an ambiguous list vouches for nothing.
fn github_vouched(body: &[u8]) -> Option<String> {
    let rows: Vec<GitHubEmail> = serde_json::from_slice(body).ok()?;
    let mut vouched = rows
        .iter()
        .filter(|row| row.primary && row.verified)
        .filter_map(|row| crate::email::normalize(&row.email));
    let first = vouched.next()?;
    vouched.next().is_none().then_some(first)
}

/// What an ID token must say to be ours.
pub struct Expect<'a> {
    pub client_id: &'a str,
    pub nonce: &'a str,
    pub now: u64,
}

/// A GUID as Microsoft writes tenant and object ids, lower case; `None` for anything else.
fn guid(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    (bytes.len() == 36
        && bytes.iter().enumerate().all(|(at, b)| {
            if matches!(at, 8 | 13 | 18 | 23) {
                *b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        }))
    .then(|| text.to_ascii_lowercase())
}

/// The claims of an ID token, without its signature (see [`microsoft_identity`]).
fn id_token_claims(id_token: &str) -> Result<Value, &'static str> {
    if id_token.len() > ID_TOKEN_BYTES {
        return Err("malformed");
    }
    let mut parts = id_token.split('.');
    let (Some(_header), Some(payload), Some(_signature), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err("malformed");
    };
    URL_SAFE_NO_PAD
        .decode(payload)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .filter(Value::is_object)
        .ok_or("malformed")
}

/// The checks every ID token passes after its issuer: for this client (`aud`), fresh (`exp`,
/// `nbf`, `iat`) and the answer to this flow (`nonce`).
fn checked_for(claims: &Value, expect: &Expect<'_>) -> Result<(), &'static str> {
    if claims["aud"].as_str() != Some(expect.client_id) {
        return Err("audience");
    }
    let at = expect.now as f64;
    let skew = SKEW_SECONDS as f64;
    match claims["exp"].as_f64() {
        Some(exp) if exp > at => {}
        _ => return Err("expired"),
    }
    for claim in ["nbf", "iat"] {
        if let Some(value) = claims.get(claim)
            && !value.as_f64().is_some_and(|time| time <= at + skew)
        {
            return Err("not yet valid");
        }
    }
    if claims["nonce"]
        .as_str()
        .is_none_or(|nonce| !same(&digest(nonce), &digest(expect.nonce)))
    {
        return Err("nonce");
    }
    Ok(())
}

/// Reads the claims of the ID token Microsoft's token endpoint returned, and checks them.
///
/// The token is not signature-checked, as OpenID Connect Core 3.1.3.7 allows: it comes straight
/// from the token endpoint over TLS, in answer to a request that carried this server's client
/// secret. What is checked instead is that it is for this client (`aud`), from the tenant it
/// names (`iss`), fresh (`exp`, `nbf`, `iat`), and the answer to this flow (`nonce`). The
/// identity is the tenant with the object id. An address is vouched for only when the token has
/// an `email` claim and `xms_edov` is the JSON value `true`: Microsoft's `email` claim alone
/// can be set by a tenant administrator and proves nothing about a mailbox.
pub fn microsoft_identity(id_token: &str, expect: &Expect<'_>) -> Result<Profile, &'static str> {
    let claims = id_token_claims(id_token)?;
    let tid = claims["tid"].as_str().ok_or("tenant")?;
    let tenant = guid(tid).ok_or("tenant")?;
    if claims["iss"].as_str() != Some(&format!("https://login.microsoftonline.com/{tid}/v2.0")) {
        return Err("issuer");
    }
    checked_for(&claims, expect)?;
    let object = claims["oid"].as_str().and_then(guid).ok_or("subject")?;
    let claimed = claims["email"].as_str().and_then(crate::email::normalize);
    let vouched = claims["xms_edov"].as_bool() == Some(true);
    let label = claimed
        .clone()
        .or_else(|| {
            claims["preferred_username"]
                .as_str()
                .and_then(person_name)
                .filter(|name| name.len() <= 120)
        })
        .unwrap_or_else(|| "Microsoft account".to_owned());
    Ok(Profile {
        provider: Provider::Microsoft,
        subject: format!("{tenant}:{object}"),
        label,
        name: claims["name"].as_str().and_then(person_name),
        email: claimed.filter(|_| vouched),
    })
}

/// Google's consumer mail domain, the one domain whose addresses Google answers for without `hd`.
pub const GOOGLE_CONSUMER_DOMAIN: &str = "gmail.com";

/// Reads the claims of the ID token Google's token endpoint returned, and checks them, as
/// [`microsoft_identity`] does and on the same grounds for not checking the signature. The issuer
/// is Google's (`https://accounts.google.com`, or the same without the scheme, both of which
/// Google documents). The identity is `sub`, Google's stable id for the account. An address is
/// vouched for only when `email_verified` is the JSON value `true` and Google answers for it: on
/// [`GOOGLE_CONSUMER_DOMAIN`], or with a Workspace domain in `hd`.
pub fn google_identity(id_token: &str, expect: &Expect<'_>) -> Result<Profile, &'static str> {
    let claims = id_token_claims(id_token)?;
    if !matches!(
        claims["iss"].as_str(),
        Some("https://accounts.google.com" | "accounts.google.com")
    ) {
        return Err("issuer");
    }
    checked_for(&claims, expect)?;
    let subject = claims["sub"]
        .as_str()
        .filter(|sub| {
            !sub.is_empty() && sub.len() <= 128 && sub.bytes().all(|b| b.is_ascii_alphanumeric())
        })
        .ok_or("subject")?;
    let claimed = claims["email"].as_str().and_then(crate::email::normalize);
    // Google is authoritative for an address only on Gmail, or on a Workspace domain (`hd`); any
    // other address it checked once, when the account was made, and that mailbox may have changed
    // hands since (Google's "Verify the Google ID token").
    let authoritative = |email: &str| {
        email
            .rsplit_once('@')
            .is_some_and(|(_, domain)| domain == GOOGLE_CONSUMER_DOMAIN)
            || claims["hd"].as_str().is_some_and(|hd| !hd.is_empty())
    };
    let vouched = claims["email_verified"].as_bool() == Some(true)
        && claimed.as_deref().is_some_and(authoritative);
    Ok(Profile {
        provider: Provider::Google,
        subject: subject.to_owned(),
        label: claimed
            .clone()
            .unwrap_or_else(|| "Google account".to_owned()),
        name: claims["name"].as_str().and_then(person_name),
        email: claimed.filter(|_| vouched),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLIENT: &str = "client-1";
    const NONCE: &str = "nonce-1";
    const TENANT: &str = "9188040d-6c67-4c5b-b112-36a304b66dad";
    const OBJECT: &str = "11111111-2222-3333-4444-555555555555";
    const AT: u64 = 1_800_000_000;

    fn token(claims: Value) -> String {
        let part = |value: &Value| URL_SAFE_NO_PAD.encode(value.to_string());
        format!(
            "{}.{}.sig",
            part(&serde_json::json!({"alg":"none"})),
            part(&claims)
        )
    }

    fn claims() -> Value {
        serde_json::json!({
            "iss": format!("https://login.microsoftonline.com/{TENANT}/v2.0"),
            "aud": CLIENT, "tid": TENANT, "oid": OBJECT, "nonce": NONCE,
            "exp": AT + 3600, "nbf": AT - 5, "iat": AT - 5,
            "name": "Ada Lovelace", "email": "Ada@Example.test", "xms_edov": true,
        })
    }

    fn check(claims: &Value) -> Result<Profile, &'static str> {
        microsoft_identity(
            &token(claims.clone()),
            &Expect {
                client_id: CLIENT,
                nonce: NONCE,
                now: AT,
            },
        )
    }

    fn changed(field: &str, value: Value) -> Value {
        let mut claims = claims();
        claims[field] = value;
        claims
    }

    fn without(field: &str) -> Value {
        let mut claims = claims();
        claims.as_object_mut().unwrap().remove(field);
        claims
    }

    fn google_claims() -> Value {
        serde_json::json!({
            "iss": "https://accounts.google.com",
            "aud": CLIENT, "sub": "110169484474386276334", "nonce": NONCE,
            "exp": AT + 3600, "iat": AT - 5,
            "name": "Grace Hopper", "email": "Grace@Example.test", "email_verified": true,
            "hd": "example.test",
        })
    }

    fn google(claims: &Value) -> Result<Profile, &'static str> {
        google_identity(
            &token(claims.clone()),
            &Expect {
                client_id: CLIENT,
                nonce: NONCE,
                now: AT,
            },
        )
    }

    fn google_with(field: &str, value: Option<Value>) -> Value {
        let mut claims = google_claims();
        match value {
            Some(value) => claims[field] = value,
            None => {
                claims.as_object_mut().unwrap().remove(field);
            }
        }
        claims
    }

    #[test]
    fn a_google_identity_is_sub_and_an_address_counts_only_with_email_verified_true_and_authority()
    {
        let profile = google(&google_claims()).unwrap();
        assert_eq!(profile.provider, Provider::Google);
        assert_eq!(profile.subject, "110169484474386276334");
        assert_eq!(profile.label, "grace@example.test");
        assert_eq!(profile.name.as_deref(), Some("Grace Hopper"));
        assert_eq!(profile.email.as_deref(), Some("grace@example.test"));
        // Both issuer forms Google documents.
        assert!(google(&google_with("iss", Some("accounts.google.com".into()))).is_ok());
        for claims in [
            google_with("email_verified", None),
            google_with("email_verified", Some(serde_json::json!(false))),
            google_with("email_verified", Some(serde_json::json!("true"))),
            google_with("email_verified", Some(serde_json::json!(1))),
        ] {
            let profile = google(&claims).unwrap();
            assert_eq!(profile.email, None, "{claims}");
            assert_eq!(profile.subject, "110169484474386276334");
        }
    }

    #[test]
    fn the_start_limit_counts_an_ipv6_client_by_its_64() {
        let key = |text: &str| start_key(text.parse().unwrap());
        assert_eq!(key("203.0.113.7"), "203.0.113.7");
        assert_eq!(key("::ffff:203.0.113.7"), "203.0.113.7");
        assert_eq!(key("2001:db8:1:2::1"), "2001:db8:1:2::/64");
        assert_eq!(
            key("2001:db8:1:2:ffff:ffff:ffff:ffff"),
            key("2001:db8:1:2::1")
        );
        assert_ne!(key("2001:db8:1:3::1"), key("2001:db8:1:2::1"));
    }

    #[test]
    fn google_vouches_only_for_gmail_and_workspace_addresses() {
        // Every Gmail case below is built from the constant, so it is pinned here.
        assert_eq!(GOOGLE_CONSUMER_DOMAIN, "gmail.com");
        // A Workspace account: `hd` names its domain.
        assert_eq!(
            google(&google_claims()).unwrap().email.as_deref(),
            Some("grace@example.test")
        );
        // Gmail, with no `hd`.
        // The secrets scan refuses literal Gmail addresses, so these are built from the constant.
        let mut gmail = google_with("hd", None);
        gmail["email"] = format!("Grace.Hopper@{GOOGLE_CONSUMER_DOMAIN}").into();
        assert_eq!(
            google(&gmail).unwrap().email,
            Some(format!("grace.hopper@{GOOGLE_CONSUMER_DOMAIN}"))
        );
        // A third-party address on a plain Google account: verified once, vouched for never.
        for hd in [
            None,
            Some(serde_json::json!("")),
            Some(serde_json::json!(true)),
        ] {
            let claims = google_with("hd", hd);
            let profile = google(&claims).unwrap();
            assert_eq!(profile.email, None, "{claims}");
            assert_eq!(profile.label, "grace@example.test");
        }
        // Not Gmail by a suffix that only looks like it.
        let mut lookalike = google_with("hd", None);
        lookalike["email"] = format!("grace@not{GOOGLE_CONSUMER_DOMAIN}").into();
        assert_eq!(google(&lookalike).unwrap().email, None);
    }

    #[test]
    fn a_google_id_token_for_someone_else_or_from_elsewhere_is_refused() {
        for (claims, why) in [
            (
                google_with("iss", Some("https://evil.test".into())),
                "issuer",
            ),
            (google_with("iss", None), "issuer"),
            (
                google_with(
                    "iss",
                    Some(format!("https://login.microsoftonline.com/{TENANT}/v2.0").into()),
                ),
                "issuer",
            ),
            (google_with("aud", Some("other".into())), "audience"),
            (
                google_with("aud", Some(serde_json::json!([CLIENT]))),
                "audience",
            ),
            (google_with("nonce", Some("other".into())), "nonce"),
            (google_with("nonce", None), "nonce"),
            (
                google_with("exp", Some(serde_json::json!(AT - 1))),
                "expired",
            ),
            (
                google_with("iat", Some(serde_json::json!(AT + 3600))),
                "not yet valid",
            ),
            (google_with("sub", None), "subject"),
            (google_with("sub", Some("".into())), "subject"),
            (google_with("sub", Some("a b".into())), "subject"),
            (google_with("sub", Some(serde_json::json!(1101))), "subject"),
        ] {
            assert_eq!(google(&claims).err(), Some(why), "{claims}");
        }
        assert_eq!(
            google_identity(
                "not.a-token",
                &Expect {
                    client_id: CLIENT,
                    nonce: NONCE,
                    now: AT
                }
            )
            .err(),
            Some("malformed")
        );
        // A Microsoft token is not a Google one.
        assert_eq!(google(&claims()).err(), Some("issuer"));
    }

    #[test]
    fn pkce_follows_the_example_of_rfc_7636() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn a_microsoft_identity_is_the_tenant_and_the_object_and_not_the_address() {
        let profile = check(&claims()).unwrap();
        assert_eq!(profile.subject, format!("{TENANT}:{OBJECT}"));
        assert_eq!(profile.label, "ada@example.test");
        assert_eq!(profile.name.as_deref(), Some("Ada Lovelace"));
        assert_eq!(profile.email.as_deref(), Some("ada@example.test"));
    }

    #[test]
    fn an_address_is_vouched_for_only_with_xms_edov_true() {
        // Microsoft's `email` claim alone proves nothing (the nOAuth account takeover).
        for claims in [
            without("xms_edov"),
            changed("xms_edov", serde_json::json!(false)),
            changed("xms_edov", serde_json::json!("true")),
            changed("xms_edov", serde_json::json!(1)),
            changed("xms_edov", Value::Null),
        ] {
            let profile = check(&claims).unwrap();
            assert_eq!(profile.email, None, "{claims}");
            assert_eq!(profile.subject, format!("{TENANT}:{OBJECT}"));
        }
        // Verified but without an address: nothing to vouch for.
        assert_eq!(check(&without("email")).unwrap().email, None);
        assert_eq!(
            check(&changed("email", serde_json::json!("not an address")))
                .unwrap()
                .email,
            None
        );
    }

    #[test]
    fn an_id_token_for_someone_else_or_from_elsewhere_is_refused() {
        assert_eq!(
            check(&changed("aud", "other".into())).err(),
            Some("audience")
        );
        assert_eq!(
            check(&changed("aud", serde_json::json!([CLIENT]))).err(),
            Some("audience")
        );
        assert_eq!(check(&without("aud")).err(), Some("audience"));
        assert_eq!(
            check(&changed("nonce", "other".into())).err(),
            Some("nonce")
        );
        assert_eq!(check(&without("nonce")).err(), Some("nonce"));
        // The issuer must name the token's own tenant, and the tenant must be a GUID.
        assert_eq!(
            check(&changed(
                "iss",
                "https://login.microsoftonline.com/00000000-0000-0000-0000-000000000000/v2.0"
                    .into()
            ))
            .err(),
            Some("issuer")
        );
        assert_eq!(
            check(&changed("iss", "https://evil.test/".into())).err(),
            Some("issuer")
        );
        assert_eq!(
            check(&changed(
                "iss",
                format!("https://login.microsoftonline.com/{TENANT}/v2.0/").into()
            ))
            .err(),
            Some("issuer")
        );
        assert_eq!(check(&without("iss")).err(), Some("issuer"));
        assert_eq!(
            check(&changed("tid", "common".into())).err(),
            Some("tenant")
        );
        assert_eq!(check(&without("tid")).err(), Some("tenant"));
        assert_eq!(check(&without("oid")).err(), Some("subject"));
        assert_eq!(check(&changed("oid", "me".into())).err(), Some("subject"));
    }

    #[test]
    fn an_id_token_is_fresh_within_five_minutes_of_skew() {
        assert_eq!(
            check(&changed("exp", (AT - 1).into())).err(),
            Some("expired")
        );
        assert_eq!(check(&changed("exp", AT.into())).err(), Some("expired"));
        assert_eq!(check(&without("exp")).err(), Some("expired"));
        assert!(check(&changed("exp", (AT + 1).into())).is_ok());
        assert!(check(&changed("nbf", (AT + 300).into())).is_ok());
        assert_eq!(
            check(&changed("nbf", (AT + 301).into())).err(),
            Some("not yet valid")
        );
        assert_eq!(
            check(&changed("iat", (AT + 301).into())).err(),
            Some("not yet valid")
        );
        assert_eq!(
            check(&changed("iat", "now".into())).err(),
            Some("not yet valid")
        );
    }

    #[test]
    fn something_that_is_not_an_id_token_is_refused() {
        let expect = Expect {
            client_id: CLIENT,
            nonce: NONCE,
            now: AT,
        };
        for wrong in [
            "",
            "a.b",
            "a.b.c.d",
            "a.!!!.c",
            &format!("a.{}.c", URL_SAFE_NO_PAD.encode("[1]")),
            &format!("a.{}.c", URL_SAFE_NO_PAD.encode("not json")),
            &"x".repeat(ID_TOKEN_BYTES + 1),
        ] {
            assert_eq!(
                microsoft_identity(wrong, &expect).err(),
                Some("malformed"),
                "{wrong}"
            );
        }
    }

    #[test]
    fn github_vouches_only_for_its_primary_verified_address() {
        let vouched = |body: &str| github_vouched(body.as_bytes());
        assert_eq!(
            vouched(
                r#"[{"email":"Other@example.test","primary":false,"verified":true},
                    {"email":"Ada@Example.test","primary":true,"verified":true}]"#
            )
            .as_deref(),
            Some("ada@example.test")
        );
        // A primary address that is not verified vouches for nothing, and so does a verified
        // address that is not primary.
        assert_eq!(
            vouched(r#"[{"email":"ada@example.test","primary":true,"verified":false}]"#),
            None
        );
        assert_eq!(
            vouched(r#"[{"email":"ada@example.test","primary":false,"verified":true}]"#),
            None
        );
        assert_eq!(vouched(r#"[{"email":"ada@example.test"}]"#), None);
        // Two primaries are ambiguous; garbage is nothing.
        assert_eq!(
            vouched(
                r#"[{"email":"a@example.test","primary":true,"verified":true},
                    {"email":"b@example.test","primary":true,"verified":true}]"#
            ),
            None
        );
        assert_eq!(vouched("{}"), None);
        assert_eq!(vouched("not json"), None);
        assert_eq!(
            vouched(r#"[{"email":"a<b@example.test>","primary":true,"verified":true}]"#),
            None
        );
    }

    fn social(test_base: Option<&str>, public: Option<&str>) -> Result<Option<Social>, String> {
        Settings {
            github_id: Some("github-client".into()),
            github_secret: Some("github-secret".into()),
            microsoft_id: Some(CLIENT.into()),
            microsoft_secret: Some("microsoft-secret".into()),
            google_id: Some("google-client".into()),
            google_secret: Some("google-secret".into()),
            test_base: test_base.map(str::to_owned),
        }
        .build(public)
    }

    #[test]
    fn a_provider_is_on_only_with_both_its_id_and_its_secret() {
        assert!(Settings::default().build(None).unwrap().is_none());
        let github_only = Settings {
            github_id: Some("id".into()),
            github_secret: Some("secret".into()),
            ..Settings::default()
        }
        .build(None)
        .unwrap()
        .unwrap();
        assert_eq!(github_only.providers(), [Provider::GitHub]);
        for (settings, wanted) in [
            (
                Settings {
                    github_id: Some("id".into()),
                    ..Settings::default()
                },
                "RICHVIEW_GITHUB_CLIENT_SECRET",
            ),
            (
                Settings {
                    microsoft_secret: Some("secret".into()),
                    ..Settings::default()
                },
                "RICHVIEW_MICROSOFT_CLIENT_ID",
            ),
            (
                Settings {
                    google_id: Some("id".into()),
                    ..Settings::default()
                },
                "RICHVIEW_GOOGLE_CLIENT_SECRET",
            ),
        ] {
            let error = settings.build(None).err().unwrap();
            assert!(error.contains(wanted), "{error}");
            assert!(!error.contains("secret\""));
        }
        // An empty value is as good as none.
        assert!(
            Settings {
                github_id: Some(String::new()),
                github_secret: Some(String::new()),
                ..Settings::default()
            }
            .build(None)
            .unwrap()
            .is_none()
        );
    }

    #[test]
    fn a_secret_is_never_in_what_debug_prints() {
        let client = Client {
            id: "id".into(),
            secret: Secret("hunter2-secret".into()),
        };
        assert!(!format!("{client:?}").contains("hunter2"));
    }

    #[test]
    fn the_stand_in_needs_loopback_for_itself_and_for_the_server() {
        assert!(
            social(Some("http://127.0.0.1:9999"), Some("http://127.0.0.1:8080"))
                .unwrap()
                .is_some()
        );
        assert!(
            social(Some("http://localhost:9"), Some("http://localhost:8080"))
                .unwrap()
                .is_some()
        );
        assert!(
            social(Some("http://[::1]:9"), Some("http://[::1]:8080"))
                .unwrap()
                .is_some()
        );
        for base in [
            "https://127.0.0.1:9",
            "http://evil.test",
            "http://127.0.0.1.evil.test",
            "http://10.0.0.1:9",
            "http://user@127.0.0.1:9",
            "http://127.0.0.1:9/path",
            "http://127.0.0.1:9/?x=1",
            "not a url",
        ] {
            assert!(
                social(Some(base), Some("http://127.0.0.1:8080")).is_err(),
                "{base}"
            );
        }
        // The hosted server's own address is never loopback.
        for public in [
            Some("https://app.richview.ai"),
            Some("http://app.test"),
            Some("http://127.0.0.1.evil.test"),
            None,
        ] {
            let error = social(Some("http://127.0.0.1:9"), public).err().unwrap();
            assert!(error.contains("public address"), "{error}");
        }
        // And it needs a provider to point at.
        assert!(
            Settings {
                test_base: Some("http://127.0.0.1:9".into()),
                ..Settings::default()
            }
            .build(Some("http://127.0.0.1:8080"))
            .is_err()
        );
    }

    fn started(social: &Social, at: u64) -> (Begun, String) {
        let begun = social
            .begin_at(
                Provider::GitHub,
                Intent::SignIn,
                None,
                "http://127.0.0.1:8080/sign-in/github/callback",
                None,
                at,
            )
            .unwrap();
        let state = reqwest::Url::parse(&begun.location)
            .unwrap()
            .query_pairs()
            .find(|(key, _)| key == "state")
            .map(|(_, value)| value.into_owned())
            .unwrap();
        (begun, state)
    }

    #[test]
    fn a_flow_is_answered_once_by_its_own_browser_for_its_own_provider() {
        let social = social(None, None).unwrap().unwrap();
        let (begun, state) = started(&social, AT);
        assert!(
            begun
                .location
                .starts_with("https://github.com/login/oauth/authorize?")
        );
        assert!(begun.location.contains("scope=user%3Aemail"));
        assert!(begun.location.contains("code_challenge_method=S256"));
        assert!(!begun.location.contains("secret"));
        let flow = social
            .take_at(&state, Some(&begun.browser), Provider::GitHub, AT + 1)
            .unwrap();
        assert!(matches!(flow.intent, Intent::SignIn));
        // A second answer to the same flow is refused.
        assert!(
            social
                .take_at(&state, Some(&begun.browser), Provider::GitHub, AT + 1)
                .is_err()
        );
        // The wrong provider, another browser, no browser and a late answer are refused, and each
        // uses its flow up: the right answer comes too late to be accepted.
        for refusal in 0..4 {
            let (begun, state) = started(&social, AT);
            let (browser, provider, at) = match refusal {
                0 => (Some(begun.browser.as_str()), Provider::Microsoft, AT),
                1 => (Some("someone else"), Provider::GitHub, AT),
                2 => (None, Provider::GitHub, AT),
                _ => (
                    Some(begun.browser.as_str()),
                    Provider::GitHub,
                    AT + FLOW_SECONDS,
                ),
            };
            assert!(social.take_at(&state, browser, provider, at).is_err());
            assert!(
                social
                    .take_at(&state, Some(&begun.browser), Provider::GitHub, AT)
                    .is_err()
            );
        }
        assert_eq!(social.waiting(), 0);
    }

    #[test]
    fn waiting_flows_are_bounded_and_expire() {
        let social = social(None, None).unwrap().unwrap();
        let (_, first) = started(&social, AT);
        // A flow the store forgot after ten minutes frees its room.
        let begun = social
            .begin_at(
                Provider::Microsoft,
                Intent::SignIn,
                None,
                "http://127.0.0.1:8080/sign-in/microsoft/callback",
                None,
                AT + FLOW_SECONDS + 1,
            )
            .unwrap();
        assert!(begun.location.contains("nonce="));
        assert!(begun.location.contains("response_mode=query"));
        assert!(begun.location.contains("scope=openid+profile+email"));
        assert_eq!(social.waiting(), 1);
        assert!(
            social
                .take_at(&first, Some("x"), Provider::GitHub, AT)
                .is_err()
        );
        // One address may start only so many in ten minutes.
        let address = Some("192.0.2.7".parse().unwrap());
        for _ in 0..STARTS_PER_ADDRESS {
            social
                .begin_at(
                    Provider::GitHub,
                    Intent::SignIn,
                    None,
                    "http://127.0.0.1/cb",
                    address,
                    AT,
                )
                .unwrap();
        }
        assert_eq!(
            social
                .begin_at(
                    Provider::GitHub,
                    Intent::SignIn,
                    None,
                    "http://127.0.0.1/cb",
                    address,
                    AT
                )
                .err(),
            Some(Refused::Busy)
        );
        // A provider with no client is not offered.
        let github_only = Settings {
            github_id: Some("id".into()),
            github_secret: Some("secret".into()),
            ..Settings::default()
        }
        .build(None)
        .unwrap()
        .unwrap();
        assert_eq!(
            github_only
                .begin_at(
                    Provider::Microsoft,
                    Intent::SignIn,
                    None,
                    "http://x/cb",
                    None,
                    AT
                )
                .err(),
            Some(Refused::Unavailable)
        );
    }

    #[test]
    fn a_held_answer_is_released_once_to_its_own_browser() {
        let social = social(None, None).unwrap().unwrap();
        let profile = || Profile {
            provider: Provider::GitHub,
            subject: "7".into(),
            label: "ada".into(),
            name: None,
            email: Some("ada@example.test".into()),
        };
        let held = || Held::NewAccount {
            profile: profile(),
            email: "ada@example.test".into(),
            back: None,
        };
        let id = social
            .hold_at(held(), Provider::GitHub, "cookie", AT)
            .unwrap();
        assert!(
            social
                .release_at(&id, Some("other"), Provider::GitHub, AT)
                .is_err()
        );
        assert!(
            social
                .release_at(&id, Some("cookie"), Provider::GitHub, AT)
                .is_err()
        );
        let id = social
            .hold_at(held(), Provider::GitHub, "cookie", AT)
            .unwrap();
        assert!(
            social
                .release_at(&id, Some("cookie"), Provider::Microsoft, AT)
                .is_err()
        );
        let id = social
            .hold_at(held(), Provider::GitHub, "cookie", AT)
            .unwrap();
        assert!(
            social
                .release_at(&id, Some("cookie"), Provider::GitHub, AT + FLOW_SECONDS)
                .is_err()
        );
        let id = social
            .hold_at(held(), Provider::GitHub, "cookie", AT)
            .unwrap();
        assert!(matches!(
            social.release_at(&id, Some("cookie"), Provider::GitHub, AT + 1),
            Ok(Held::NewAccount { .. })
        ));
        assert!(
            social
                .release_at(&id, Some("cookie"), Provider::GitHub, AT + 1)
                .is_err()
        );
        assert_eq!(social.waiting(), 0);
    }
}
