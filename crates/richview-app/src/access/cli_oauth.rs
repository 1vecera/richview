//! Signing the command line in through a redirect to its own computer (RFC 8252) with PKCE
//! (RFC 7636): the command line listens on a loopback port, sends its person to `/cli/authorize`,
//! and gets a one-time code back on that port. It trades the code and its verifier for a token
//! that lasts an hour and a refresh token that renews it, so the person is asked once.
//!
//! Four rules hold throughout. The code is useless without the verifier, which never leaves the
//! command line before the trade, and it is spent by the first trade whatever the answer. The
//! only place a code can be sent is `http://127.0.0.1:<port>/callback`, so it cannot be sent to
//! another site or another computer. A refresh token works once: the next one replaces it, and any
//! later use of any replaced one, which only a copy of the file could make, ends the sign-in. And
//! nothing here is stored as it was given: codes and tokens are digests.
//!
//! Codes wait in memory like the short-code sign-ins (`_host_lock`): a restart only makes a
//! person start again, a minute at most after they said yes.

use super::cli_sign_in::{address_key, admit, computer_name, token_name};
use super::*;
use base64::Engine as _;
use std::collections::VecDeque;

/// How long an access token works.
pub const ACCESS_TOKEN_SECONDS: u64 = 3600;
/// How long a refresh token works after its last use: a computer nobody used for this long is
/// signed out.
pub(super) const REFRESH_TOKEN_SECONDS: u64 = 90 * 24 * 3600;
/// How long a code can be traded after the person said yes.
pub const CLI_CODE_SECONDS: u64 = 60;
/// How long the access token a refresh replaced keeps working, so that another command on the same
/// computer that read it just before is not refused in the middle of its work.
pub(super) const SUPERSEDED_SECONDS: u64 = 120;
/// Codes waiting to be traded, across everyone.
const MAX_CODES: usize = 1024;
/// Codes one person may have waiting.
const CODES_PER_PERSON: usize = 5;
/// Trades and refreshes one address may make in a minute: several computers behind one address.
const TRADES_PER_ADDRESS: usize = 120;
/// The shortest and longest `state` and PKCE value this accepts.
const STATE_BYTES: std::ops::RangeInclusive<usize> = 16..=128;
const VERIFIER_BYTES: std::ops::RangeInclusive<usize> = 43..=128;
/// The most a token held for the exchange may look like.
const MAX_TOKEN_BYTES: usize = 128;
/// The most the computer's name may take in an address, as written there: the sign-in page can
/// return to the address only when the whole of it fits (`MAX_BACK`).
const MAX_COMPUTER_ADDRESS_BYTES: usize = 120;

/// What a command line's address asks for, checked: where the answer may go, the command line's
/// own `state` to give back, the PKCE challenge, and what it calls its computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliAuthRequest {
    pub redirect_uri: String,
    pub state: String,
    pub challenge: String,
    /// The command line's own claim, shown to the person and put in the token's name. Never proof.
    pub computer: String,
}

/// The port of `http://127.0.0.1:<port>/callback`, the only address a code is sent to. A port
/// below 1024 is a service's, not a command line's.
fn loopback_port(redirect_uri: &str) -> Option<u16> {
    let port = redirect_uri
        .strip_prefix("http://127.0.0.1:")?
        .strip_suffix("/callback")?;
    if port.is_empty() || port.len() > 5 || !port.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    port.parse::<u16>().ok().filter(|port| *port >= 1024)
}

/// Letters, digits, `-` and `_`: what base64url without padding writes.
fn url_safe(text: &str, lengths: &std::ops::RangeInclusive<usize>) -> bool {
    lengths.contains(&text.len())
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// A PKCE verifier as RFC 7636 writes it: 43 to 128 unreserved characters.
fn verifier_ok(verifier: &str) -> bool {
    VERIFIER_BYTES.contains(&verifier.len())
        && verifier
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~'))
}

/// The challenge of a verifier: its SHA-256, in base64url without padding (`S256`).
pub fn cli_challenge(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// The code both the terminal and the page show for one sign-in, so that a person can tell the
/// request they just made from one somebody else's program on the same computer sent them: eight
/// letters, from the three values that make the request what it is. Nobody who is not this
/// request's author can choose a request whose code matches without trying about 2^40 of them.
pub fn cli_check_code(redirect_uri: &str, state: &str, challenge: &str) -> String {
    const LETTERS: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut hasher = Sha256::new();
    for part in ["richview cli sign-in check", redirect_uri, state, challenge] {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    let letters: String = hasher
        .finalize()
        .iter()
        .take(8)
        .map(|byte| char::from(LETTERS[usize::from(byte & 31)]))
        .collect();
    format!("{}-{}", &letters[..4], &letters[4..])
}

/// Percent-encodes everything but the unreserved characters, so an address built here has one
/// spelling.
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// `name` cut after the last whole character that `encode` writes in `MAX_COMPUTER_ADDRESS_BYTES`.
fn short_in_an_address(name: &str) -> String {
    let mut written = 0;
    let mut kept = String::new();
    for c in name.chars() {
        written += encode(c.encode_utf8(&mut [0; 4])).len();
        if written > MAX_COMPUTER_ADDRESS_BYTES {
            break;
        }
        kept.push(c);
    }
    kept.trim_end().to_owned()
}

fn decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let pair = std::str::from_utf8(bytes.get(at + 1..at + 3)?).ok()?;
            out.push(u8::from_str_radix(pair, 16).ok()?);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(out).ok()
}

impl CliAuthRequest {
    /// The request these four values make, or `None` when any of them is not what a command line
    /// of this version sends. The computer's name is made plain and short.
    pub fn new(redirect_uri: &str, state: &str, challenge: &str, computer: &str) -> Option<Self> {
        (loopback_port(redirect_uri).is_some()
            && url_safe(state, &STATE_BYTES)
            && url_safe(challenge, &(43..=43)))
        .then(|| Self {
            redirect_uri: redirect_uri.to_owned(),
            state: state.to_owned(),
            challenge: challenge.to_owned(),
            computer: short_in_an_address(&computer_name(computer)),
        })
    }

    /// The page the command line sends its person to, with one spelling for every part. The
    /// sign-in page can return to this and nothing looser (`from_path`).
    pub fn path(&self) -> String {
        format!(
            "/cli/authorize?redirect_uri={}&state={}&code_challenge={}&code_challenge_method=S256&computer={}",
            encode(&self.redirect_uri),
            self.state,
            self.challenge,
            encode(&self.computer)
        )
    }

    /// The code this request is shown under (`cli_check_code`).
    pub fn check_code(&self) -> String {
        cli_check_code(&self.redirect_uri, &self.state, &self.challenge)
    }

    /// The request a path carries, only when `path` is exactly what `path()` writes.
    pub fn from_path(path: &str) -> Option<Self> {
        let mut parts = path.strip_prefix("/cli/authorize?")?.split('&');
        let mut take = |key: &str| {
            parts
                .next()?
                .strip_prefix(key)?
                .strip_prefix('=')
                .and_then(decode)
        };
        let redirect_uri = take("redirect_uri")?;
        let state = take("state")?;
        let challenge = take("code_challenge")?;
        let method = take("code_challenge_method")?;
        let computer = take("computer")?;
        if parts.next().is_some() || method != "S256" {
            return None;
        }
        let request = Self::new(&redirect_uri, &state, &challenge, &computer)?;
        (request.path() == path).then_some(request)
    }

    /// Where the browser goes next with a code: the command line's own address, never another.
    pub fn answer_with_code(&self, code: &str) -> String {
        format!(
            "{}?code={}&state={}",
            self.redirect_uri,
            encode(code),
            self.state
        )
    }

    /// Where the browser goes when the person said no.
    pub fn answer_denied(&self) -> String {
        format!(
            "{}?error=access_denied&state={}",
            self.redirect_uri, self.state
        )
    }
}

/// A code the person's yes made, waiting to be traded.
struct AuthCode {
    person: String,
    org: String,
    /// The person's credential number when they said yes (`State::credential_epoch`): a reset
    /// since then ends the code.
    approved_under: u64,
    challenge: String,
    redirect_uri: String,
    computer: String,
    until: u64,
}

/// Codes waiting, and the counters that bound the trades.
#[derive(Default)]
pub(super) struct CliCodes {
    /// Digest of the code → what it was made for.
    codes: BTreeMap<String, AuthCode>,
    trades: BTreeMap<String, VecDeque<u64>>,
}

/// A sign-in the command line was just given: shown once, to it, and kept by it.
pub struct CliGrant {
    pub token: String,
    pub token_id: String,
    pub refresh: String,
    /// Seconds the token works.
    pub expires_in: u64,
    pub person: String,
    pub org: String,
}

enum Step {
    Rotated(CliGrant),
    /// No such refresh token, or its sign-in ended: it ran out, or its person lost their place.
    Unknown,
    /// A refresh token that had already been used: the sign-in is ended.
    Reused {
        person: String,
        token_id: String,
    },
}

impl Access {
    fn cli_codes(&self) -> Result<std::sync::MutexGuard<'_, CliCodes>, AccessError> {
        self.cli_codes
            .lock()
            .map_err(|_| AccessError::Storage("Command-line codes poisoned".into()))
    }

    /// Makes the code that takes `request`'s answer to the command line, after `person`, in their
    /// own browser session, said yes for `org`. Only the person's session may ask, never a token.
    pub fn issue_cli_authorization(
        &self,
        caller: impl FnOnce() -> Principal,
        person: &str,
        org: &str,
        request: &CliAuthRequest,
    ) -> Result<String, AccessError> {
        self.issue_cli_authorization_at(caller, person, org, request, now())
    }

    fn issue_cli_authorization_at(
        &self,
        caller: impl FnOnce() -> Principal,
        person: &str,
        org: &str,
        request: &CliAuthRequest,
        at: u64,
    ) -> Result<String, AccessError> {
        if !self.is_hosted() {
            return Err(AccessError::NotFound);
        }
        // Read before the session is checked: a reset that ends the session after this read also
        // changes the number, so the code made under the old one is refused when it is traded.
        let approved_under = self.read(|state| state.credential_epoch(person));
        if !matches!(caller(), Principal::User(user) if user.id == person && user.token_org.is_none())
        {
            return Err(AccessError::Credentials);
        }
        self.may_hold_cli_token(person, org)?;
        let mut codes = self.cli_codes()?;
        codes.codes.retain(|_, code| code.until > at);
        if codes.codes.len() >= MAX_CODES
            || codes
                .codes
                .values()
                .filter(|code| code.person == person)
                .count()
                >= CODES_PER_PERSON
        {
            return Err(AccessError::Throttled);
        }
        let code = random_text(32)?;
        codes.codes.insert(
            digest(&code),
            AuthCode {
                person: person.to_owned(),
                org: org.to_owned(),
                approved_under,
                challenge: request.challenge.clone(),
                redirect_uri: request.redirect_uri.clone(),
                computer: request.computer.clone(),
                until: at + CLI_CODE_SECONDS,
            },
        );
        Ok(code)
    }

    /// Trades a code and its verifier for the command line's sign-in. The code is spent by this
    /// call whatever the answer, so a wrong guess costs the guesser the code. One answer for an
    /// unknown, late, spent or mismatched code.
    pub fn redeem_cli_authorization(
        &self,
        address: Option<std::net::IpAddr>,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
    ) -> Result<CliGrant, AccessError> {
        self.redeem_cli_authorization_at(address, code, verifier, redirect_uri, now())
    }

    fn redeem_cli_authorization_at(
        &self,
        address: Option<std::net::IpAddr>,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
        at: u64,
    ) -> Result<CliGrant, AccessError> {
        let spent = {
            let mut codes = self.cli_codes()?;
            if !admit(
                &mut codes.trades,
                &address_key(address),
                at,
                60,
                TRADES_PER_ADDRESS,
            ) {
                return Err(AccessError::Throttled);
            }
            codes.codes.retain(|_, code| code.until > at);
            codes.codes.remove(&digest(code))
        };
        let Some(spent) = spent else {
            return Err(AccessError::NotFound);
        };
        if !verifier_ok(verifier)
            || !constant_time_eq(spent.redirect_uri.as_bytes(), redirect_uri.as_bytes())
            || !constant_time_eq(
                spent.challenge.as_bytes(),
                cli_challenge(verifier).as_bytes(),
            )
        {
            return Err(AccessError::NotFound);
        }
        // Issued now, with every check a token made on the CLI tokens page gets: the person may
        // have left the organization or been disabled since they said yes.
        let issued = self.issue_token_for(
            || None,
            &spent.person,
            &spent.org,
            &token_name(&spent.computer),
            Some(20),
            Some(CliIssue {
                refreshing: true,
                approved_under: spent.approved_under,
            }),
        )?;
        Ok(CliGrant {
            token: issued.secret,
            token_id: issued.id,
            refresh: issued.refresh.unwrap_or_default(),
            expires_in: issued.expires_in,
            person: spent.person,
            org: spent.org,
        })
    }

    /// Ends the sign-in a token or refresh token belongs to, for a computer that signs itself out:
    /// after an hour only its refresh token is left to say who it is. `None` when nothing knew it.
    pub fn revoke_cli_grant(
        &self,
        address: Option<std::net::IpAddr>,
        token: &str,
    ) -> Result<Option<(String, String)>, AccessError> {
        {
            let mut codes = self.cli_codes()?;
            if !admit(
                &mut codes.trades,
                &address_key(address),
                now(),
                60,
                TRADES_PER_ADDRESS,
            ) {
                return Err(AccessError::Throttled);
            }
        }
        if !self.is_hosted() || token.is_empty() || token.len() > MAX_TOKEN_BYTES {
            return Ok(None);
        }
        match self.revoke_presented_token(token) {
            Ok(found) => Ok(Some(found)),
            Err(AccessError::NotFound) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Trades a refresh token for the next token and the next refresh token. `NotFound` when the
    /// sign-in is over and its person must sign in again; `Credentials` when the refresh token had
    /// already been used, which ends the sign-in too.
    pub fn refresh_cli_grant(
        &self,
        address: Option<std::net::IpAddr>,
        refresh: &str,
    ) -> Result<CliGrant, AccessError> {
        self.refresh_cli_grant_at(address, refresh, now())
    }

    fn refresh_cli_grant_at(
        &self,
        address: Option<std::net::IpAddr>,
        refresh: &str,
        at: u64,
    ) -> Result<CliGrant, AccessError> {
        {
            let mut codes = self.cli_codes()?;
            if !admit(
                &mut codes.trades,
                &address_key(address),
                at,
                60,
                TRADES_PER_ADDRESS,
            ) {
                return Err(AccessError::Throttled);
            }
        }
        if !self.is_hosted() || refresh.is_empty() || refresh.len() > MAX_TOKEN_BYTES {
            return Err(AccessError::NotFound);
        }
        let ask = RefreshAsk::new(refresh);
        // A refresh token nobody issued, which anyone can send, costs a lookup and no write.
        let known = self.read(|state| {
            state
                .users
                .iter()
                .any(|record| record.tokens.iter().any(|t| ask.standing(t).is_some()))
        });
        if !known {
            return Err(AccessError::NotFound);
        }
        let _gate = self.gate()?;
        let step = self.with_gated(|state| {
            let found = state.users.iter().enumerate().find_map(|(user, record)| {
                record
                    .tokens
                    .iter()
                    .position(|t| ask.standing(t).is_some())
                    .map(|token| (user, token))
            });
            // Nothing is written for a refresh token that is no longer here.
            let Some((user, token)) = found else {
                return Err(AccessError::NotFound);
            };
            let record = &state.users[user];
            let held = &record.tokens[token];
            let (person, org, token_id) =
                (record.user.id.clone(), held.org.clone(), held.id.clone());
            let Some(current) = held.refresh.as_ref() else {
                return Err(AccessError::NotFound);
            };
            // A refresh token that was already traded is in the hands of someone the sign-in did
            // not give it to, or of the computer it was given to after a copy of its file took the
            // next one: nobody keeps the sign-in. There is no grace for a lost answer, which would
            // be the same thing to the server.
            if ask.standing(held) != Some(Standing::Current) {
                state.users[user].tokens.remove(token);
                return Ok(Step::Reused { person, token_id });
            }
            let member = state
                .memberships
                .iter()
                .any(|m| m.person == person && m.org == org)
                && state
                    .organizations
                    .iter()
                    .any(|o| o.id == org && !o.suspended);
            if current.expires <= at || !member || record.user.disabled {
                state.users[user].tokens.remove(token);
                return Ok(Step::Unknown);
            }
            let token_secret = format!("rvt_{}", random_text(32)?);
            // The new token names the same sign-in with the same grant; one of the older kind gets
            // a grant now.
            let (refresh_secret, new_grant) = match split_refresh(refresh) {
                Some((id, grant)) => (format!("rvr_{id}.{grant}.{}", random_text(32)?), None),
                None => {
                    let grant = random_text(16)?;
                    let id = &state.users[user].tokens[token].id;
                    (
                        format!("rvr_{id}.{grant}.{}", random_text(32)?),
                        Some(digest(&grant)),
                    )
                }
            };
            let held = &mut state.users[user].tokens[token];
            // The replaced token is accepted a little longer, but never past its own end.
            let until = (at + SUPERSEDED_SECONDS).min(held.expires.unwrap_or(u64::MAX));
            held.superseded = Some(Superseded {
                sha256: std::mem::replace(&mut held.sha256, digest(&token_secret)),
                until,
            });
            held.expires = Some(at + ACCESS_TOKEN_SECONDS);
            if let Some(next) = held.refresh.as_mut() {
                next.rotate(
                    digest(&refresh_secret),
                    at + REFRESH_TOKEN_SECONDS,
                    new_grant,
                );
            }
            Ok(Step::Rotated(CliGrant {
                token: token_secret,
                token_id,
                refresh: refresh_secret,
                expires_in: ACCESS_TOKEN_SECONDS,
                person,
                org,
            }))
        })?;
        match step {
            Step::Rotated(grant) => Ok(grant),
            Step::Unknown => Err(AccessError::NotFound),
            Step::Reused { person, token_id } => {
                let _ = self.audit(
                    &Principal::Anonymous,
                    "token.reuse",
                    &format!("{person}/{token_id}"),
                    false,
                );
                Err(AccessError::Credentials)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hosted() -> (tempfile::TempDir, Access) {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".access")).unwrap();
        let state = serde_json::json!({"schema":2,"tenancy":"hosted",
            "organizations":[{"id":"aaaa0001","name":"A","created":"now","suspended":false,"limits":{}}],
            "memberships":[{"person":"ada","org":"aaaa0001","role":"owner","created":"now"}],
            "users":[{"id":"ada","name":"Ada","email":"ada@example.test","created":"now","verified":"now","tokens":[]}],
            "sessions":[{"sha256":digest("session-ada"),"csrf":"csrf-ada","user":"ada","expires":4102444800u64,"scope":""}],
            "acls":{}});
        fs::write(
            dir.path().join(".access/state.json"),
            serde_json::to_vec(&state).unwrap(),
        )
        .unwrap();
        let access = Access::open_hosted(dir.path()).unwrap();
        (dir, access)
    }

    fn ada(access: &Access) -> impl FnOnce() -> Principal + '_ {
        || access.authenticate(None, Some("session-ada"))
    }

    fn address(last: u8) -> Option<std::net::IpAddr> {
        Some(std::net::IpAddr::from([192, 0, 2, last]))
    }

    const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    const STATE: &str = "state-of-this-login-0123";

    fn request() -> CliAuthRequest {
        CliAuthRequest::new(
            "http://127.0.0.1:49152/callback",
            STATE,
            &cli_challenge(VERIFIER),
            "ada-laptop",
        )
        .unwrap()
    }

    /// A code for `request()`, as a person's yes makes it.
    fn code(access: &Access, at: u64) -> String {
        access
            .issue_cli_authorization_at(ada(access), "ada", "aaaa0001", &request(), at)
            .unwrap()
    }

    #[test]
    fn the_challenge_of_the_rfc_7636_example_verifier_is_the_one_it_gives() {
        assert_eq!(
            cli_challenge(VERIFIER),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn a_code_can_only_be_sent_to_a_loopback_port_of_this_computer() {
        let challenge = cli_challenge(VERIFIER);
        let made = |uri: &str| CliAuthRequest::new(uri, STATE, &challenge, "x");
        assert!(made("http://127.0.0.1:49152/callback").is_some());
        assert!(made("http://127.0.0.1:1024/callback").is_some());
        assert!(made("http://127.0.0.1:65535/callback").is_some());
        for wrong in [
            "",
            "http://127.0.0.1/callback",
            "http://127.0.0.1:80/callback",
            "http://127.0.0.1:1023/callback",
            "http://127.0.0.1:65536/callback",
            "http://127.0.0.1:049152/callback",
            "http://127.0.0.1:+9152/callback",
            "http://127.0.0.1:49152/callback/",
            "http://127.0.0.1:49152/callback?x=1",
            "http://127.0.0.1:49152/other",
            "http://127.0.0.1:49152",
            "https://127.0.0.1:49152/callback",
            "http://localhost:49152/callback",
            "http://[::1]:49152/callback",
            "http://127.0.0.2:49152/callback",
            "http://127.0.0.1.evil.test:49152/callback",
            "http://user@127.0.0.1:49152/callback",
            "http://127.0.0.1:49152@evil.test/callback",
            "//127.0.0.1:49152/callback",
            "http://evil.test/callback",
            "http://127.0.0.1:49152/callback#x",
        ] {
            assert!(made(wrong).is_none(), "{wrong}");
        }
        // The other two values have the shapes this version sends.
        let uri = "http://127.0.0.1:49152/callback";
        assert!(CliAuthRequest::new(uri, "short", &challenge, "").is_none());
        assert!(CliAuthRequest::new(uri, &"s".repeat(129), &challenge, "").is_none());
        assert!(CliAuthRequest::new(uri, "has space in the state 1", &challenge, "").is_none());
        assert!(CliAuthRequest::new(uri, STATE, "too-short", "").is_none());
        assert!(CliAuthRequest::new(uri, STATE, &format!("{challenge}x"), "").is_none());
        assert!(CliAuthRequest::new(uri, STATE, &challenge.replace('-', "+"), "").is_none());
    }

    #[test]
    fn the_authorize_address_has_one_spelling_and_only_it_is_a_place_to_return_to() {
        let request = CliAuthRequest::new(
            "http://127.0.0.1:49152/callback",
            STATE,
            &cli_challenge(VERIFIER),
            "Ada’s Mac mini\u{1b}[31m",
        )
        .unwrap();
        // The computer's name is made plain before it is shown or put in an address.
        assert_eq!(request.computer, "Ada’s Mac mini[31m");
        // However it is written, it takes no more of an address than the page can return to.
        for name in [
            "ž".repeat(40),
            "😀".repeat(40),
            "x".repeat(40),
            "é ".repeat(20),
        ] {
            let long = CliAuthRequest::new(
                "http://127.0.0.1:49152/callback",
                &"s".repeat(22),
                &cli_challenge(VERIFIER),
                &name,
            )
            .unwrap();
            assert!(long.path().len() <= 512, "{}", long.path().len());
            assert_eq!(CliAuthRequest::from_path(&long.path()), Some(long));
        }
        let path = request.path();
        assert!(
            path.starts_with(
                "/cli/authorize?redirect_uri=http%3A%2F%2F127.0.0.1%3A49152%2Fcallback&state=state-of-this-login-0123&code_challenge="
            ) && path.ends_with("&code_challenge_method=S256&computer=Ada%E2%80%99s%20Mac%20mini%5B31m"),
            "{path}"
        );
        assert!(
            path.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/?=&-._~%".contains(&b)),
            "{path}"
        );
        assert_eq!(CliAuthRequest::from_path(&path), Some(request.clone()));
        // Anything else is not one: another order, another spelling, an extra part, another
        // method, a bad value.
        let challenge = cli_challenge(VERIFIER);
        for wrong in [
            path.replace("S256", "plain"),
            path.replace("%3A", ":"),
            format!("{path}&next=/"),
            format!("{path}%"),
            path.replace("&computer=", "&computer=%ZZ"),
            path.replace("127.0.0.1", "evil.test"),
            path.replace("state=", "State="),
            format!(
                "/cli/authorize?state={STATE}&redirect_uri=http%3A%2F%2F127.0.0.1%3A49152%2Fcallback&code_challenge={challenge}&code_challenge_method=S256&computer="
            ),
            "/cli/authorize".to_owned(),
            "/cli/authorize?".to_owned(),
            path.replacen("/cli/authorize", "/cli/authorise", 1),
        ] {
            assert!(CliAuthRequest::from_path(&wrong).is_none(), "{wrong}");
        }
        // The answers go to the command line's own address with its own state, and nowhere else.
        assert_eq!(
            request.answer_with_code("co-de_1"),
            "http://127.0.0.1:49152/callback?code=co-de_1&state=state-of-this-login-0123"
        );
        assert_eq!(
            request.answer_denied(),
            "http://127.0.0.1:49152/callback?error=access_denied&state=state-of-this-login-0123"
        );
        // No computer name is a valid request too.
        let bare = CliAuthRequest::new(&request.redirect_uri, STATE, &challenge, "").unwrap();
        assert_eq!(CliAuthRequest::from_path(&bare.path()), Some(bare));
    }

    #[test]
    fn a_code_is_traded_once_for_a_token_that_refreshes() {
        let (_dir, access) = hosted();
        let code = code(&access, 1000);
        // The code is no token, and a wrong verifier or redirect gives nothing and spends it.
        assert!(matches!(
            access.authenticate(Some(&code), None),
            Principal::Anonymous
        ));
        let grant = access
            .redeem_cli_authorization_at(
                address(1),
                &code,
                VERIFIER,
                "http://127.0.0.1:49152/callback",
                1010,
            )
            .unwrap();
        assert_eq!(
            (grant.person.as_str(), grant.org.as_str()),
            ("ada", "aaaa0001")
        );
        assert_eq!(grant.expires_in, ACCESS_TOKEN_SECONDS);
        assert!(grant.token.starts_with("rvt_") && grant.refresh.starts_with("rvr_"));
        // It is an ordinary CLI token of that person in that organization, named after the computer.
        let Principal::User(user) = access.authenticate(Some(&grant.token), None) else {
            panic!("the token does not sign its person in");
        };
        assert_eq!(
            (user.id.as_str(), user.token_org.as_deref()),
            ("ada", Some("aaaa0001"))
        );
        let listed = access.tokens_for_org("ada", "aaaa0001");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0]["name"], "Command line on ada-laptop");
        // The refresh token is not a token, and the token is not a refresh token.
        assert!(matches!(
            access.authenticate(Some(&grant.refresh), None),
            Principal::Anonymous
        ));
        assert!(matches!(
            access.refresh_cli_grant_at(address(1), &grant.token, 1020),
            Err(AccessError::NotFound)
        ));
        // Spent: the same code trades nothing more.
        assert!(matches!(
            access.redeem_cli_authorization_at(
                address(1),
                &code,
                VERIFIER,
                "http://127.0.0.1:49152/callback",
                1011
            ),
            Err(AccessError::NotFound)
        ));
        assert_eq!(access.tokens_for_org("ada", "aaaa0001").len(), 1);
    }

    #[test]
    fn a_wrong_verifier_redirect_or_clock_spends_the_code_and_gives_nothing() {
        let (_dir, access) = hosted();
        let right = "http://127.0.0.1:49152/callback";
        for (verifier, redirect, at) in [
            ("a-verifier-of-the-right-length-0123456789abc", right, 1010),
            ("short", right, 1010),
            (VERIFIER, "http://127.0.0.1:49153/callback", 1010),
            (VERIFIER, "http://127.0.0.1:49152/callback?x", 1010),
            (VERIFIER, "", 1010),
            // Too late: the code lasts a minute.
            (VERIFIER, right, 1000 + CLI_CODE_SECONDS),
        ] {
            let code = code(&access, 1000);
            assert!(
                matches!(
                    access.redeem_cli_authorization_at(address(1), &code, verifier, redirect, at),
                    Err(AccessError::NotFound)
                ),
                "{verifier} {redirect} {at}"
            );
            // Spent by the wrong answer: the right one comes too late.
            assert!(matches!(
                access.redeem_cli_authorization_at(address(1), &code, VERIFIER, right, 1011),
                Err(AccessError::NotFound)
            ));
        }
        assert!(access.tokens_for_org("ada", "aaaa0001").is_empty());
        // A code that never existed is the same answer.
        assert!(matches!(
            access.redeem_cli_authorization_at(address(1), "no-such-code", VERIFIER, right, 1011),
            Err(AccessError::NotFound)
        ));
    }

    #[test]
    fn only_the_persons_own_session_in_their_own_organization_makes_a_code() {
        let (_dir, access) = hosted();
        let request = request();
        let ask = |caller: Principal, person: &str, org: &str| {
            access.issue_cli_authorization_at(|| caller, person, org, &request, 1000)
        };
        assert!(matches!(
            ask(Principal::Anonymous, "ada", "aaaa0001"),
            Err(AccessError::Credentials)
        ));
        let (_, token) = access
            .create_self_token_for_fixture("ada", "aaaa0001", "old")
            .unwrap();
        // A token cannot say yes for its person.
        assert!(matches!(
            ask(access.authenticate(Some(&token), None), "ada", "aaaa0001"),
            Err(AccessError::Credentials)
        ));
        // Nor can one person say yes for another, or for an organization they are not in.
        assert!(matches!(
            ask(
                access.authenticate(None, Some("session-ada")),
                "bob",
                "aaaa0001"
            ),
            Err(AccessError::Credentials)
        ));
        assert!(matches!(
            ask(
                access.authenticate(None, Some("session-ada")),
                "ada",
                "bbbb0002"
            ),
            Err(AccessError::NotFound)
        ));
        // The session's yes is the only one, and at most a few wait at once.
        for _ in 0..CODES_PER_PERSON {
            ask(
                access.authenticate(None, Some("session-ada")),
                "ada",
                "aaaa0001",
            )
            .unwrap();
        }
        assert!(matches!(
            ask(
                access.authenticate(None, Some("session-ada")),
                "ada",
                "aaaa0001"
            ),
            Err(AccessError::Throttled)
        ));
        // They expire and make room.
        access
            .issue_cli_authorization_at(
                ada(&access),
                "ada",
                "aaaa0001",
                &request,
                1000 + CLI_CODE_SECONDS,
            )
            .unwrap();
    }

    #[test]
    fn trading_is_bounded_per_address_and_minute() {
        let (_dir, access) = hosted();
        let right = "http://127.0.0.1:49152/callback";
        for _ in 0..TRADES_PER_ADDRESS {
            let _ = access.redeem_cli_authorization_at(
                address(7),
                "no-such-code",
                VERIFIER,
                right,
                9000,
            );
        }
        assert!(matches!(
            access.redeem_cli_authorization_at(address(7), "no-such-code", VERIFIER, right, 9001),
            Err(AccessError::Throttled)
        ));
        assert!(matches!(
            access.refresh_cli_grant_at(address(7), "rvr_no-such-token", 9001),
            Err(AccessError::Throttled)
        ));
        // Another address is not held up, and a minute later this one is not either.
        assert!(matches!(
            access.redeem_cli_authorization_at(address(8), "no-such-code", VERIFIER, right, 9001),
            Err(AccessError::NotFound)
        ));
        assert!(matches!(
            access.redeem_cli_authorization_at(address(7), "no-such-code", VERIFIER, right, 9060),
            Err(AccessError::NotFound)
        ));
    }

    /// A signed-in computer: the grant a trade made at time 1000.
    fn signed_in(access: &Access) -> CliGrant {
        let code = code(access, 1000);
        access
            .redeem_cli_authorization_at(
                address(1),
                &code,
                VERIFIER,
                "http://127.0.0.1:49152/callback",
                1001,
            )
            .unwrap()
    }

    #[test]
    fn an_access_token_ends_after_an_hour_and_a_refresh_gives_the_next_without_asking() {
        let (dir, access) = hosted();
        let first = signed_in(&access);
        // The access token works until its time is up. (`authenticate` reads the clock, so the
        // end of the hour is set on the record.)
        access.state.lock().unwrap().users[0].tokens[0].expires = Some(1);
        assert!(matches!(
            access.authenticate(Some(&first.token), None),
            Principal::Anonymous
        ));
        // The refresh token still works, and trades itself for the next pair.
        let second = access
            .refresh_cli_grant_at(address(1), &first.refresh, now())
            .unwrap();
        assert_eq!(second.token_id, first.token_id);
        assert_eq!(second.expires_in, ACCESS_TOKEN_SECONDS);
        assert!(second.token != first.token && second.refresh != first.refresh);
        assert!(matches!(
            access.authenticate(Some(&second.token), None),
            Principal::User(_)
        ));
        // The hour was over for the replaced token: a refresh does not revive it.
        assert!(matches!(
            access.authenticate(Some(&first.token), None),
            Principal::Anonymous
        ));
        // Still one sign-in for this computer, whatever the number of refreshes.
        assert_eq!(access.tokens_for_org("ada", "aaaa0001").len(), 1);
        // The server keeps digests only.
        let kept = std::fs::read_to_string(dir.path().join(".access/state.json")).unwrap();
        assert!(
            !kept.contains(&second.token) && !kept.contains(&second.refresh),
            "the server stored a secret"
        );
        assert!(kept.contains(&digest(&second.refresh)));
    }

    #[test]
    fn the_token_a_refresh_replaced_works_for_two_minutes_and_no_longer() {
        let (_dir, access) = hosted();
        let first = signed_in(&access);
        let second = access
            .refresh_cli_grant_at(address(1), &first.refresh, now())
            .unwrap();
        // Another command that read the first token just before this refresh is not refused...
        assert!(matches!(
            access.authenticate(Some(&first.token), None),
            Principal::User(_)
        ));
        // ...until its two minutes are over.
        access.state.lock().unwrap().users[0].tokens[0]
            .superseded
            .as_mut()
            .unwrap()
            .until = 1;
        assert!(matches!(
            access.authenticate(Some(&first.token), None),
            Principal::Anonymous
        ));
        assert!(matches!(
            access.authenticate(Some(&second.token), None),
            Principal::User(_)
        ));
    }

    #[test]
    fn a_refresh_token_that_was_traded_ends_the_sign_in_at_once_and_whenever_it_is_used() {
        // Asked again at once, with no grace: a lost answer is no different to the server from a
        // copy of the file, so both end the sign-in.
        let (_dir, access) = hosted();
        let first = signed_in(&access);
        let second = access
            .refresh_cli_grant_at(address(1), &first.refresh, now())
            .unwrap();
        assert!(matches!(
            access.refresh_cli_grant_at(address(1), &first.refresh, now()),
            Err(AccessError::Credentials)
        ));
        assert!(access.tokens_for_org("ada", "aaaa0001").is_empty());
        assert!(matches!(
            access.refresh_cli_grant_at(address(1), &second.refresh, now()),
            Err(AccessError::NotFound)
        ));
    }

    #[test]
    fn a_copy_that_renews_twice_cannot_make_the_computers_own_token_unknown() {
        // The copy's two renewals used to push the computer's refresh token out of what the
        // server remembered, so its use was no more than "unknown": the computer was signed out
        // and the copy kept a sign-in nothing revoked.
        let (dir, access) = hosted();
        let first = signed_in(&access);
        let copy_one = access
            .refresh_cli_grant_at(address(2), &first.refresh, now())
            .unwrap();
        let copy_two = access
            .refresh_cli_grant_at(address(2), &copy_one.refresh, now())
            .unwrap();
        assert!(matches!(
            access.authenticate(Some(&copy_two.token), None),
            Principal::User(_)
        ));
        // The computer's turn: its refresh token is two renewals old.
        assert!(matches!(
            access.refresh_cli_grant_at(address(1), &first.refresh, now()),
            Err(AccessError::Credentials)
        ));
        // The sign-in is revoked for the copy too, and the trail says why.
        assert!(access.tokens_for_org("ada", "aaaa0001").is_empty());
        assert!(matches!(
            access.authenticate(Some(&copy_two.token), None),
            Principal::Anonymous
        ));
        assert!(matches!(
            access.refresh_cli_grant_at(address(2), &copy_two.refresh, now()),
            Err(AccessError::NotFound)
        ));
        let mut trail = String::new();
        for entry in std::fs::read_dir(dir.path().join(".access"))
            .unwrap()
            .flatten()
        {
            if entry.file_name().to_string_lossy().starts_with("audit") {
                trail.push_str(&std::fs::read_to_string(entry.path()).unwrap());
            }
        }
        assert!(trail.contains("token.reuse"), "{trail}");
    }

    #[test]
    fn a_traded_refresh_token_ends_the_sign_in_however_many_renewals_came_since() {
        // A thief who renews at the pace the limiter allows (120 a minute) used to push the
        // computer's token out of a list of 128 within about a minute; nothing runs out now.
        let (dir, access) = hosted();
        let mut grant = signed_in(&access);
        let first = grant.refresh.clone();
        let mut early = Vec::new();
        for turn in 0..(2 * 128 + 5) {
            early.push(grant.refresh.clone());
            grant = access
                .refresh_cli_grant_at(address(1), &grant.refresh, now() + turn as u64)
                .unwrap();
        }
        // The record holds the current token's digest and the grant's, and no list of what was
        // traded.
        let kept = std::fs::read_to_string(dir.path().join(".access/state.json")).unwrap();
        let state: serde_json::Value = serde_json::from_str(&kept).unwrap();
        let refresh = state["users"][0]["tokens"][0]["refresh"]
            .as_object()
            .unwrap();
        let mut fields: Vec<_> = refresh.keys().map(String::as_str).collect();
        fields.sort_unstable();
        assert_eq!(fields, ["expires", "grant", "sha256"]);
        // No token is stored, whole or as a digest, and neither is the grant.
        let grant_of = |token: &str| token.split('.').nth(1).unwrap().to_owned();
        assert!(!kept.contains(&grant_of(&grant.refresh)));
        for token in &early {
            assert!(!kept.contains(token) && !kept.contains(&digest(token)));
        }
        // The very first token, a hundred and more renewals old, is still the end of the sign-in.
        assert!(matches!(
            access.refresh_cli_grant_at(address(1), &first, now() + 1000),
            Err(AccessError::Credentials)
        ));
        assert!(access.tokens_for_org("ada", "aaaa0001").is_empty());
        assert!(matches!(
            access.refresh_cli_grant_at(address(1), &grant.refresh, now() + 1000),
            Err(AccessError::NotFound)
        ));
    }

    #[test]
    fn a_refresh_token_that_names_a_sign_in_without_its_grant_ends_nothing() {
        // The id of a sign-in is no secret (a person sees it on the tokens page), so a token that
        // has it and not the grant is no more than a token nobody issued.
        let (dir, access) = hosted();
        let first = signed_in(&access);
        let renewed = access
            .refresh_cli_grant_at(address(1), &first.refresh, now())
            .unwrap();
        let file = dir.path().join(".access/state.json");
        let before = std::fs::read(&file).unwrap();
        for guess in [
            format!("rvr_{}.not-its-grant.{}", first.token_id, "s".repeat(32)),
            format!("rvr_{}..{}", first.token_id, "s".repeat(32)),
            format!("rvr_{}.{}", first.token_id, "s".repeat(32)),
            format!("rvr_{}", first.token_id),
        ] {
            assert!(matches!(
                access.refresh_cli_grant_at(address(9), &guess, now()),
                Err(AccessError::NotFound)
            ));
            assert!(matches!(
                access.revoke_cli_grant(address(9), &guess),
                Ok(None)
            ));
        }
        assert_eq!(std::fs::read(&file).unwrap(), before);
        assert!(
            access
                .refresh_cli_grant_at(address(1), &renewed.refresh, now())
                .is_ok()
        );
    }

    #[test]
    fn a_sign_in_from_before_tokens_named_it_still_catches_what_it_traded_and_gets_a_grant() {
        let (dir, access) = hosted();
        let first = signed_in(&access);
        drop(access);
        // The old store's shape: no grant, the one predecessor in `previous`, no `spent`.
        let file = dir.path().join(".access/state.json");
        let mut state: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        let refresh = &mut state["users"][0]["tokens"][0]["refresh"];
        refresh.as_object_mut().unwrap().remove("grant");
        refresh["sha256"] = serde_json::json!(digest("rvr_old-current"));
        refresh["previous"] = serde_json::json!(digest("rvr_old-first"));
        std::fs::write(&file, serde_json::to_vec(&state).unwrap()).unwrap();
        // Its current token renews, into one that names the sign-in.
        let reopened = Access::open_hosted(dir.path()).unwrap();
        let next = reopened
            .refresh_cli_grant_at(address(1), "rvr_old-current", now())
            .unwrap();
        assert!(
            next.refresh
                .starts_with(&format!("rvr_{}.", first.token_id))
        );
        // The store now holds the grant and both tokens it traded, and no `previous`.
        let state: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        let refresh = &state["users"][0]["tokens"][0]["refresh"];
        assert!(refresh["grant"].is_string() && refresh.get("previous").is_none());
        let mut spent: Vec<_> = refresh["spent"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry.as_str().unwrap())
            .collect();
        spent.sort_unstable();
        let mut both = [
            &digest("rvr_old-first")[..32],
            &digest("rvr_old-current")[..32],
        ];
        both.sort_unstable();
        assert_eq!(spent, both);
        // After a restart, the first of them, traded before anything named a sign-in, ends it.
        drop(reopened);
        let again = Access::open_hosted(dir.path()).unwrap();
        assert!(matches!(
            again.refresh_cli_grant_at(address(1), "rvr_old-first", now()),
            Err(AccessError::Credentials)
        ));
        assert!(again.tokens_for_org("ada", "aaaa0001").is_empty());
    }

    #[test]
    fn a_sign_in_that_takes_its_grant_keeps_every_token_it_had_traded_before() {
        // A store that had already filled the old list of 128: the first renewal under the new
        // rules used to push the oldest out as it added the token it replaced.
        let (dir, access) = hosted();
        let first = signed_in(&access);
        drop(access);
        let file = dir.path().join(".access/state.json");
        let mut state: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        let refresh = &mut state["users"][0]["tokens"][0]["refresh"];
        refresh.as_object_mut().unwrap().remove("grant");
        refresh["sha256"] = serde_json::json!(digest("rvr_old-current"));
        let traded: Vec<String> = (0..128).map(|n| format!("rvr_traded-{n}")).collect();
        refresh["spent"] = traded
            .iter()
            .map(|token| serde_json::json!(&digest(token)[..32]))
            .collect();
        std::fs::write(&file, serde_json::to_vec(&state).unwrap()).unwrap();
        let reopened = Access::open_hosted(dir.path()).unwrap();
        let next = reopened
            .refresh_cli_grant_at(address(1), "rvr_old-current", now())
            .unwrap();
        assert!(
            next.refresh
                .starts_with(&format!("rvr_{}.", first.token_id))
        );
        // All of the 128, and the token just replaced, are held, and the sign-in names its tokens.
        let state: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        let refresh = &state["users"][0]["tokens"][0]["refresh"];
        assert!(refresh["grant"].is_string());
        assert_eq!(refresh["spent"].as_array().unwrap().len(), 129);
        // The oldest one of them, which the rule it replaces would have dropped, still ends it.
        drop(reopened);
        let again = Access::open_hosted(dir.path()).unwrap();
        assert!(matches!(
            again.refresh_cli_grant_at(address(1), &traded[0], now()),
            Err(AccessError::Credentials)
        ));
        assert!(again.tokens_for_org("ada", "aaaa0001").is_empty());
    }

    #[test]
    fn a_predecessor_kept_as_previous_survives_a_save_that_has_nothing_to_do_with_it() {
        // The store is read into the new shape and written back by whatever saves it next, so a
        // person who only signs in elsewhere does not lose what the sign-in remembers.
        let (dir, access) = hosted();
        let first = signed_in(&access);
        drop(access);
        let file = dir.path().join(".access/state.json");
        let mut state: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        let refresh = &mut state["users"][0]["tokens"][0]["refresh"];
        refresh.as_object_mut().unwrap().remove("grant");
        refresh["previous"] = serde_json::json!(digest("rvr_old-first"));
        std::fs::write(&file, serde_json::to_vec(&state).unwrap()).unwrap();
        let reopened = Access::open_hosted(dir.path()).unwrap();
        // Another sign-in saves the store.
        let _other = signed_in(&reopened);
        drop(reopened);
        let state: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        let tokens = state["users"][0]["tokens"].as_array().unwrap();
        let held = tokens.iter().find(|t| t["id"] == first.token_id).unwrap();
        assert!(held["refresh"].get("previous").is_none());
        assert_eq!(
            held["refresh"]["spent"],
            serde_json::json!([&digest("rvr_old-first")[..32]])
        );
        let again = Access::open_hosted(dir.path()).unwrap();
        assert!(matches!(
            again.refresh_cli_grant_at(address(1), "rvr_old-first", now()),
            Err(AccessError::Credentials)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn a_token_nobody_issued_is_refused_without_writing_the_store() {
        use std::os::unix::fs::MetadataExt;
        let (dir, access) = hosted();
        let _first = signed_in(&access);
        let file = dir.path().join(".access/state.json");
        let before = std::fs::metadata(&file).unwrap().ino();
        for guess in 0..5 {
            let guess = format!("rvr_nobody-issued-this-{guess}");
            assert!(matches!(
                access.refresh_cli_grant_at(address(9), &guess, now()),
                Err(AccessError::NotFound)
            ));
            assert!(matches!(
                access.revoke_cli_grant(address(9), &guess),
                Ok(None)
            ));
        }
        // Every save replaces the file, so the same file is no save at all.
        assert_eq!(std::fs::metadata(&file).unwrap().ino(), before);
    }

    #[test]
    fn the_check_code_is_eight_plain_letters_and_follows_every_part_of_the_request() {
        let (redirect, state, challenge) = (
            "http://127.0.0.1:49152/callback",
            "s".repeat(22),
            cli_challenge(VERIFIER),
        );
        let code = cli_check_code(redirect, &state, &challenge);
        assert_eq!(code.len(), 9);
        for (at, letter) in code.chars().enumerate() {
            assert!(
                if at == 4 {
                    letter == '-'
                } else {
                    "ABCDEFGHJKLMNPQRSTUVWXYZ23456789".contains(letter)
                },
                "{code}"
            );
        }
        assert_eq!(code, cli_check_code(redirect, &state, &challenge));
        for other in [
            cli_check_code("http://127.0.0.1:49153/callback", &state, &challenge),
            cli_check_code(redirect, &"t".repeat(22), &challenge),
            cli_check_code(
                redirect,
                &state,
                &cli_challenge("another-verifier-0123456789-0123456789-0123"),
            ),
        ] {
            assert_ne!(other, code);
        }
        let request = CliAuthRequest::new(redirect, &state, &challenge, "Ada").unwrap();
        assert_eq!(request.check_code(), code);
    }

    #[test]
    fn using_a_replaced_refresh_token_ends_the_whole_sign_in() {
        let (dir, access) = hosted();
        let first = signed_in(&access);
        let second = access
            .refresh_cli_grant_at(address(1), &first.refresh, now())
            .unwrap();
        // A copy of the file kept the first refresh token. Using it is not a refresh.
        assert!(matches!(
            access.refresh_cli_grant_at(address(2), &first.refresh, now()),
            Err(AccessError::Credentials)
        ));
        // The sign-in is gone for the computer that holds the second one too.
        assert!(access.tokens_for_org("ada", "aaaa0001").is_empty());
        assert!(matches!(
            access.authenticate(Some(&second.token), None),
            Principal::Anonymous
        ));
        assert!(matches!(
            access.refresh_cli_grant_at(address(1), &second.refresh, now()),
            Err(AccessError::NotFound)
        ));
        // The audit trail names the sign-in, not a secret.
        let mut trail = String::new();
        for entry in std::fs::read_dir(dir.path().join(".access"))
            .unwrap()
            .flatten()
        {
            if entry.file_name().to_string_lossy().starts_with("audit") {
                trail.push_str(&std::fs::read_to_string(entry.path()).unwrap());
            }
        }
        assert!(
            trail.contains("token.reuse") && !trail.contains(&first.refresh),
            "{trail}"
        );
    }

    #[test]
    fn a_sign_in_nobody_used_for_ninety_days_or_that_lost_its_place_is_over() {
        let (_dir, access) = hosted();
        let start = now();
        let late = signed_in(&access);
        assert!(matches!(
            access.refresh_cli_grant_at(
                address(1),
                &late.refresh,
                start + REFRESH_TOKEN_SECONDS + 60
            ),
            Err(AccessError::NotFound)
        ));
        assert!(access.tokens_for_org("ada", "aaaa0001").is_empty());
        // Each use gives it another ninety days.
        let used = signed_in(&access);
        let day = 24 * 3600;
        let near_end = start + REFRESH_TOKEN_SECONDS - day;
        let next = access
            .refresh_cli_grant_at(address(1), &used.refresh, near_end)
            .unwrap();
        let again = access
            .refresh_cli_grant_at(
                address(1),
                &next.refresh,
                near_end + REFRESH_TOKEN_SECONDS - day,
            )
            .unwrap();
        // A disabled person's sign-in does not renew, and is gone.
        access.state.lock().unwrap().users[0].user.disabled = true;
        assert!(matches!(
            access.refresh_cli_grant_at(address(1), &again.refresh, now()),
            Err(AccessError::NotFound)
        ));
        assert!(access.tokens_for_org("ada", "aaaa0001").is_empty());
        // Garbage is the same answer.
        for wrong in ["", "rvr_", &"r".repeat(MAX_TOKEN_BYTES + 1)] {
            assert!(matches!(
                access.refresh_cli_grant_at(address(1), wrong, now()),
                Err(AccessError::NotFound)
            ));
        }
    }

    #[test]
    fn signing_out_ends_a_sign_in_by_its_token_or_by_its_refresh_token() {
        let (_dir, access) = hosted();
        let by_token = signed_in(&access);
        assert_eq!(
            access.revoke_presented_token(&by_token.token).unwrap(),
            ("ada".to_owned(), by_token.token_id.clone())
        );
        assert!(matches!(
            access.refresh_cli_grant_at(address(1), &by_token.refresh, 1500),
            Err(AccessError::NotFound)
        ));
        // After the hour, the refresh token is all the computer has left to sign out with.
        let by_refresh = signed_in(&access);
        access.state.lock().unwrap().users[0].tokens[0].expires = Some(1);
        assert_eq!(
            access.revoke_presented_token(&by_refresh.refresh).unwrap(),
            ("ada".to_owned(), by_refresh.token_id.clone())
        );
        assert!(access.tokens_for_org("ada", "aaaa0001").is_empty());
        assert!(matches!(
            access.revoke_presented_token(&by_refresh.refresh),
            Err(AccessError::NotFound)
        ));
    }

    #[tokio::test]
    async fn a_password_reset_ends_the_codes_and_yeses_made_before_it() {
        let (_dir, access) = hosted();
        let at = now();
        let right = "http://127.0.0.1:49152/callback";
        // A code Ada's yes made, and a short-code sign-in she allowed, both waiting.
        let waiting = code(&access, at);
        let device = access
            .start_cli_sign_in_with(address(3), "ada-laptop", true, at)
            .unwrap();
        access
            .decide_cli_sign_in_at(ada(&access), "ada", "aaaa0001", &device.user_code, true, at)
            .unwrap();
        // She resets her password: someone else may have had the account and made those yeses.
        let link = access
            .issue_link("ada@example.test", LinkPurpose::Reset)
            .unwrap()
            .unwrap();
        access
            .complete_hosted_link_and_sign_in(&link, "", "a long enough password", None)
            .await
            .unwrap();
        // Neither is redeemable now, though both are well inside their minute and ten minutes.
        assert!(matches!(
            access.redeem_cli_authorization_at(address(1), &waiting, VERIFIER, right, at + 1),
            Err(AccessError::NotFound)
        ));
        assert!(matches!(
            access
                .collect_cli_sign_in_at(address(3), &device.code, at + 1)
                .unwrap(),
            CliSignInCollected::Failed(_)
        ));
        assert!(access.tokens_for_org("ada", "aaaa0001").is_empty());
    }

    #[test]
    fn a_new_password_ends_the_yeses_made_before_it_and_not_the_ones_made_after() {
        let (_dir, access) = hosted();
        let at = now();
        let right = "http://127.0.0.1:49152/callback";
        let before = code(&access, at);
        access
            .set_password("ada", "a long enough password")
            .unwrap();
        assert!(matches!(
            access.redeem_cli_authorization_at(address(1), &before, VERIFIER, right, at + 1),
            Err(AccessError::NotFound)
        ));
        // `set_password` ended her session too; a yes needs a new one.
        access.state.lock().unwrap().sessions.push(Session {
            sha256: digest("session-after"),
            csrf: "csrf-after".into(),
            user: "ada".into(),
            expires: 4102444800,
            scope: String::new(),
            org: None,
        });
        let after = access
            .issue_cli_authorization_at(
                || access.authenticate(None, Some("session-after")),
                "ada",
                "aaaa0001",
                &request(),
                at + 2,
            )
            .unwrap();
        assert!(
            access
                .redeem_cli_authorization_at(address(1), &after, VERIFIER, right, at + 3)
                .is_ok()
        );
    }

    #[test]
    fn signing_out_with_a_refresh_token_that_was_just_traded_still_ends_the_sign_in() {
        let (_dir, access) = hosted();
        let first = signed_in(&access);
        // Another command on the computer renewed while this one was signing out.
        let _second = access
            .refresh_cli_grant_at(address(1), &first.refresh, now())
            .unwrap();
        assert_eq!(
            access.revoke_cli_grant(address(1), &first.refresh).unwrap(),
            Some(("ada".to_owned(), first.token_id.clone()))
        );
        assert!(access.tokens_for_org("ada", "aaaa0001").is_empty());
        // The token that renewal replaced is named by the sign-in too.
        let (_dir, access) = hosted();
        let first = signed_in(&access);
        let _second = access
            .refresh_cli_grant_at(address(1), &first.refresh, now())
            .unwrap();
        assert!(access.revoke_presented_token(&first.token).is_ok());
        assert!(access.tokens_for_org("ada", "aaaa0001").is_empty());
    }

    #[test]
    fn a_sign_in_a_person_revokes_on_the_tokens_page_cannot_refresh() {
        let (_dir, access) = hosted();
        let grant = signed_in(&access);
        access
            .revoke_self_token(ada(&access), "ada", &grant.token_id)
            .unwrap();
        assert!(matches!(
            access.authenticate(Some(&grant.token), None),
            Principal::Anonymous
        ));
        assert!(matches!(
            access.refresh_cli_grant_at(address(1), &grant.refresh, 1500),
            Err(AccessError::NotFound)
        ));
    }
}
