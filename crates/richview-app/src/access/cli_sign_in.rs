//! Signing the command line in from the browser. The command line starts a sign-in and gets two
//! codes: a secret it keeps, and a short one it shows. A signed-in person confirms the short code
//! on a page, and the command line collects a CLI token once with its secret.
//!
//! Three rules hold throughout. The short code is no credential: alone it gives nothing, and it
//! is the only part that ever stands in an address. A sign-in is spent by its first collection
//! after a decision and is gone after ten minutes whatever happened. And the token does not exist
//! until it is collected, so it is never stored here, only handed over.
//!
//! Sign-ins wait in memory: the hosted server is one process (`_host_lock`), and a restart only
//! makes a person start again.

use super::*;
use std::collections::VecDeque;

/// How long a started sign-in can be confirmed and collected.
pub const CLI_SIGN_IN_SECONDS: u64 = 600;
/// How often the command line asks whether its person has decided.
pub const CLI_SIGN_IN_POLL_SECONDS: u64 = 3;
/// Sign-ins waiting at once, across everyone.
const MAX_WAITING: usize = 512;
/// Sign-ins one address may start in ten minutes.
const STARTS_PER_ADDRESS: usize = 10;
/// Collection attempts one address may make in a minute: several command lines behind one
/// address, each asking every few seconds.
const COLLECTS_PER_ADDRESS: usize = 120;
/// Codes one person may get wrong in ten minutes before every code is refused for a while.
const WRONG_CODES_PER_PERSON: usize = 10;
/// Addresses and people counted at once; beyond it, new ones wait.
const MAX_COUNTED: usize = 1024;
/// Letters of the short code: no vowels, so no words, and nothing that reads like a digit.
const CODE_LETTERS: &[u8] = b"BCDFGHJKLMNPQRSTVWXZ";
/// A token name is short; so is the computer's name inside it.
const MAX_COMPUTER_CHARS: usize = 40;

enum Decision {
    Waiting,
    Allowed { person: String, org: String },
    Denied,
}

struct SignIn {
    user_code: String,
    /// What the command line called its computer: shown to the person and put in the token's
    /// name. It is the caller's own claim, never proof of anything.
    computer: String,
    until: u64,
    decision: Decision,
}

/// Every command-line sign-in under way, with the counters that bound them.
#[derive(Default)]
pub(super) struct CliSignIns {
    /// Digest of the command line's secret → its sign-in.
    waiting: BTreeMap<String, SignIn>,
    starts: BTreeMap<String, VecDeque<u64>>,
    collects: BTreeMap<String, VecDeque<u64>>,
    wrong_codes: BTreeMap<String, VecDeque<u64>>,
}

/// What the command line gets when it starts a sign-in.
pub struct CliSignInStarted {
    /// The command line's secret: sent back only in a request body, to collect the token.
    pub code: String,
    /// The short code the person compares.
    pub user_code: String,
}

/// A sign-in as the confirm page shows it.
pub struct CliSignInWaiting {
    pub user_code: String,
    pub computer: String,
}

/// The answer to a collection attempt.
pub enum CliSignInCollected {
    /// Nobody has decided yet.
    Waiting,
    /// The person said no; the sign-in is spent.
    Denied,
    /// Unknown, expired or already collected: one answer for all three.
    Gone,
    /// The person said yes: the token, shown this once.
    Allowed {
        token: String,
        token_id: String,
        person: String,
        org: String,
    },
    /// The person said yes, but the token could not be issued; the sign-in is spent.
    Failed(String),
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

/// Counts one more for `key` unless it already has `most` in the window or too many keys are
/// counted; says whether it was admitted.
fn admit(
    counters: &mut BTreeMap<String, VecDeque<u64>>,
    key: &str,
    at: u64,
    window: u64,
    most: usize,
) -> bool {
    let have = counted(counters, key, at, window);
    if have >= most || (have == 0 && counters.len() >= MAX_COUNTED) {
        return false;
    }
    counters.entry(key.to_owned()).or_default().push_back(at);
    true
}

/// What the limits count a client by: as the provider sign-in does, an IPv4 address or the /64
/// of an IPv6 one, since one IPv6 subscriber has a whole /64 to start from.
fn address_key(address: Option<std::net::IpAddr>) -> String {
    address.map_or_else(|| "unknown".to_owned(), crate::social::start_key)
}

/// A fresh short code, `XXXX-XXXX`, each letter drawn evenly.
fn user_code() -> Result<String, String> {
    let mut code = String::with_capacity(9);
    while code.len() < 9 {
        let mut byte = [0u8; 1];
        getrandom::fill(&mut byte).map_err(|e| e.to_string())?;
        // 240 is the largest multiple of the 20 letters a byte holds; the rest would skew.
        if byte[0] < 240 {
            code.push(CODE_LETTERS[usize::from(byte[0]) % CODE_LETTERS.len()] as char);
            if code.len() == 4 {
                code.push('-');
            }
        }
    }
    Ok(code)
}

/// The short code as a person may type it: any case, with or without the hyphen or spaces.
/// `None` when it cannot be a code at all.
pub fn cli_user_code(typed: &str) -> Option<String> {
    let letters: Vec<u8> = typed
        .bytes()
        .filter(|b| !matches!(b, b'-' | b' '))
        .map(|b| b.to_ascii_uppercase())
        .collect();
    (letters.len() == 8 && letters.iter().all(|b| CODE_LETTERS.contains(b))).then(|| {
        format!(
            "{}-{}",
            String::from_utf8_lossy(&letters[..4]),
            String::from_utf8_lossy(&letters[4..])
        )
    })
}

fn computer_name(claimed: &str) -> String {
    claimed
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_COMPUTER_CHARS)
        .collect::<String>()
        .trim()
        .to_owned()
}

impl CliSignIns {
    fn expire(&mut self, at: u64) {
        self.waiting.retain(|_, sign_in| sign_in.until > at);
    }

    /// The undecided sign-in with this short code, or the refusal a wrong code earns: a person
    /// who keeps guessing is refused every code for a while.
    fn undecided(
        &mut self,
        person: &str,
        user_code: &str,
        at: u64,
    ) -> Result<&mut SignIn, AccessError> {
        self.expire(at);
        if counted(&mut self.wrong_codes, person, at, CLI_SIGN_IN_SECONDS) >= WRONG_CODES_PER_PERSON
        {
            return Err(AccessError::Throttled);
        }
        let code = cli_user_code(user_code);
        let found = self.waiting.values_mut().find(|sign_in| {
            matches!(sign_in.decision, Decision::Waiting)
                && code.as_deref() == Some(&sign_in.user_code)
        });
        match found {
            Some(sign_in) => Ok(sign_in),
            None => {
                self.wrong_codes
                    .entry(person.to_owned())
                    .or_default()
                    .push_back(at);
                Err(AccessError::NotFound)
            }
        }
    }
}

impl Access {
    fn cli_sign_ins(&self) -> Result<std::sync::MutexGuard<'_, CliSignIns>, AccessError> {
        self.cli_sign_ins
            .lock()
            .map_err(|_| AccessError::Storage("Command-line sign-ins poisoned".into()))
    }

    /// Starts a sign-in for a command line at `address`. Anyone may ask, so it is bounded per
    /// address and in total, and it changes nothing a person can see.
    pub fn start_cli_sign_in(
        &self,
        address: Option<std::net::IpAddr>,
        computer: &str,
    ) -> Result<CliSignInStarted, AccessError> {
        self.start_cli_sign_in_at(address, computer, now())
    }

    fn start_cli_sign_in_at(
        &self,
        address: Option<std::net::IpAddr>,
        computer: &str,
        at: u64,
    ) -> Result<CliSignInStarted, AccessError> {
        if !self.is_hosted() {
            return Err(AccessError::NotFound);
        }
        let mut sign_ins = self.cli_sign_ins()?;
        sign_ins.expire(at);
        if sign_ins.waiting.len() >= MAX_WAITING
            || !admit(
                &mut sign_ins.starts,
                &address_key(address),
                at,
                CLI_SIGN_IN_SECONDS,
                STARTS_PER_ADDRESS,
            )
        {
            return Err(AccessError::Throttled);
        }
        let code = random_text(32)?;
        let user_code = loop {
            let candidate = user_code()?;
            if !sign_ins.waiting.values().any(|s| s.user_code == candidate) {
                break candidate;
            }
        };
        sign_ins.waiting.insert(
            digest(&code),
            SignIn {
                user_code: user_code.clone(),
                computer: computer_name(computer),
                until: at + CLI_SIGN_IN_SECONDS,
                decision: Decision::Waiting,
            },
        );
        Ok(CliSignInStarted { code, user_code })
    }

    /// The undecided sign-in behind a short code, for the page that asks `person` to confirm it.
    pub fn waiting_cli_sign_in(
        &self,
        person: &str,
        user_code: &str,
    ) -> Result<CliSignInWaiting, AccessError> {
        self.waiting_cli_sign_in_at(person, user_code, now())
    }

    fn waiting_cli_sign_in_at(
        &self,
        person: &str,
        user_code: &str,
        at: u64,
    ) -> Result<CliSignInWaiting, AccessError> {
        let mut sign_ins = self.cli_sign_ins()?;
        let sign_in = sign_ins.undecided(person, user_code, at)?;
        Ok(CliSignInWaiting {
            user_code: sign_in.user_code.clone(),
            computer: sign_in.computer.clone(),
        })
    }

    /// Records a person's answer to a sign-in. Only the person's own browser session decides,
    /// never a token; allowing needs what issuing the token will need, so a yes that cannot be
    /// kept is refused here, where the person reads it.
    pub fn decide_cli_sign_in(
        &self,
        caller: impl FnOnce() -> Principal,
        person: &str,
        org: &str,
        user_code: &str,
        allow: bool,
    ) -> Result<(), AccessError> {
        self.decide_cli_sign_in_at(caller, person, org, user_code, allow, now())
    }

    fn decide_cli_sign_in_at(
        &self,
        caller: impl FnOnce() -> Principal,
        person: &str,
        org: &str,
        user_code: &str,
        allow: bool,
        at: u64,
    ) -> Result<(), AccessError> {
        if !matches!(caller(), Principal::User(user) if user.id == person && user.token_org.is_none())
        {
            return Err(AccessError::Credentials);
        }
        if allow {
            self.read(|state| {
                let member = state
                    .memberships
                    .iter()
                    .any(|m| m.person == person && m.org == org)
                    && state
                        .organizations
                        .iter()
                        .any(|o| o.id == org && !o.suspended);
                let record = state.users.iter().find(|r| r.user.id == person);
                match record {
                    Some(record) if member && !record.user.disabled => {
                        if record.tokens.len() >= 20 {
                            Err(AccessError::Invalid(
                                "You can have at most 20 active tokens; revoke one first.".into(),
                            ))
                        } else {
                            Ok(())
                        }
                    }
                    _ => Err(AccessError::NotFound),
                }
            })?;
        }
        let mut sign_ins = self.cli_sign_ins()?;
        let sign_in = sign_ins.undecided(person, user_code, at)?;
        sign_in.decision = if allow {
            Decision::Allowed {
                person: person.to_owned(),
                org: org.to_owned(),
            }
        } else {
            Decision::Denied
        };
        Ok(())
    }

    /// Hands the command line its token if its person said yes. A decided sign-in is spent by
    /// this call whatever the answer; an undecided one stays until it expires.
    pub fn collect_cli_sign_in(
        &self,
        address: Option<std::net::IpAddr>,
        code: &str,
    ) -> Result<CliSignInCollected, AccessError> {
        self.collect_cli_sign_in_at(address, code, now())
    }

    fn collect_cli_sign_in_at(
        &self,
        address: Option<std::net::IpAddr>,
        code: &str,
        at: u64,
    ) -> Result<CliSignInCollected, AccessError> {
        let decided = {
            let mut sign_ins = self.cli_sign_ins()?;
            if !admit(
                &mut sign_ins.collects,
                &address_key(address),
                at,
                60,
                COLLECTS_PER_ADDRESS,
            ) {
                return Err(AccessError::Throttled);
            }
            sign_ins.expire(at);
            let key = digest(code);
            match sign_ins.waiting.get(&key).map(|s| &s.decision) {
                None => return Ok(CliSignInCollected::Gone),
                Some(Decision::Waiting) => return Ok(CliSignInCollected::Waiting),
                Some(_) => sign_ins.waiting.remove(&key),
            }
        };
        let Some(SignIn {
            computer,
            decision: Decision::Allowed { person, org },
            ..
        }) = decided
        else {
            return Ok(CliSignInCollected::Denied);
        };
        let name = if computer.is_empty() {
            "Command line".to_owned()
        } else {
            format!("Command line on {computer}")
        };
        // Issued now, with every check a token made on the CLI tokens page gets: the person may
        // have left the organization or been disabled since they said yes.
        Ok(
            match self.issue_token(|| None, &person, &org, &name, Some(20)) {
                Ok((token_id, token)) => CliSignInCollected::Allowed {
                    token,
                    token_id,
                    person,
                    org,
                },
                Err(AccessError::Invalid(message)) => CliSignInCollected::Failed(message),
                Err(_) => CliSignInCollected::Failed(
                    "The sign-in could not be completed. Start it again.".into(),
                ),
            },
        )
    }

    /// Revokes the token a caller presents, so signing out on a computer ends the token too.
    /// The server's own service token is not a person's token and is not found.
    pub fn revoke_presented_token(&self, bearer: &str) -> Result<(String, String), AccessError> {
        let wanted = digest(bearer);
        let _gate = self.gate()?;
        self.with_gated(|state| {
            for record in &mut state.users {
                if let Some(at) = record.tokens.iter().position(|t| t.sha256 == wanted) {
                    let token = record.tokens.remove(at);
                    return Ok((record.user.id.clone(), token.id));
                }
            }
            Err(AccessError::NotFound)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hosted store with one person in one organization, and the person's session.
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

    #[test]
    fn a_confirmed_sign_in_hands_over_one_token_once() {
        let (_dir, access) = hosted();
        let started = access
            .start_cli_sign_in_at(address(1), "ada-laptop", 1000)
            .unwrap();
        assert!(
            cli_user_code(&started.user_code).is_some(),
            "{}",
            started.user_code
        );
        // Nobody has decided: the command line waits, as often as it asks.
        for _ in 0..3 {
            assert!(matches!(
                access
                    .collect_cli_sign_in_at(address(1), &started.code, 1010)
                    .unwrap(),
                CliSignInCollected::Waiting
            ));
        }
        // The short code is no credential: it collects nothing.
        assert!(matches!(
            access
                .collect_cli_sign_in_at(address(1), &started.user_code, 1010)
                .unwrap(),
            CliSignInCollected::Gone
        ));
        let shown = access
            .waiting_cli_sign_in_at("ada", &started.user_code.to_lowercase(), 1015)
            .unwrap();
        assert_eq!(shown.computer, "ada-laptop");
        access
            .decide_cli_sign_in_at(
                ada(&access),
                "ada",
                "aaaa0001",
                &started.user_code,
                true,
                1020,
            )
            .unwrap();
        let CliSignInCollected::Allowed {
            token,
            token_id,
            person,
            org,
        } = access
            .collect_cli_sign_in_at(address(1), &started.code, 1030)
            .unwrap()
        else {
            panic!("an allowed sign-in was not handed over");
        };
        assert_eq!((person.as_str(), org.as_str()), ("ada", "aaaa0001"));
        // It is an ordinary CLI token of that person in that organization, named after the computer.
        let Principal::User(user) = access.authenticate(Some(&token), None) else {
            panic!("the token does not sign its person in");
        };
        assert_eq!(
            (user.id.as_str(), user.token_org.as_deref()),
            ("ada", Some("aaaa0001"))
        );
        let listed = access.tokens_for_org("ada", "aaaa0001");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0]["id"], token_id);
        assert_eq!(listed[0]["name"], "Command line on ada-laptop");
        // Single use: the same secret collects nothing more, and the code confirms nothing more.
        assert!(matches!(
            access
                .collect_cli_sign_in_at(address(1), &started.code, 1031)
                .unwrap(),
            CliSignInCollected::Gone
        ));
        assert!(matches!(
            access.decide_cli_sign_in_at(
                ada(&access),
                "ada",
                "aaaa0001",
                &started.user_code,
                true,
                1032
            ),
            Err(AccessError::NotFound)
        ));
        assert_eq!(access.tokens_for_org("ada", "aaaa0001").len(), 1);
        // Signing out revokes exactly the presented token, once.
        assert_eq!(
            access.revoke_presented_token(&token).unwrap(),
            ("ada".to_owned(), token_id)
        );
        assert!(matches!(
            access.authenticate(Some(&token), None),
            Principal::Anonymous
        ));
        assert!(matches!(
            access.revoke_presented_token(&token),
            Err(AccessError::NotFound)
        ));
    }

    #[test]
    fn a_sign_in_expires_and_a_refused_one_gives_no_token() {
        let (_dir, access) = hosted();
        // Expired before anyone decided: it cannot be confirmed or collected.
        let late = access.start_cli_sign_in_at(address(1), "", 1000).unwrap();
        let after = 1000 + CLI_SIGN_IN_SECONDS;
        assert!(matches!(
            access.decide_cli_sign_in_at(
                ada(&access),
                "ada",
                "aaaa0001",
                &late.user_code,
                true,
                after
            ),
            Err(AccessError::NotFound)
        ));
        assert!(matches!(
            access
                .collect_cli_sign_in_at(address(1), &late.code, after)
                .unwrap(),
            CliSignInCollected::Gone
        ));
        // Confirmed in time but collected too late: still nothing.
        let slow = access.start_cli_sign_in_at(address(1), "", 2000).unwrap();
        access
            .decide_cli_sign_in_at(ada(&access), "ada", "aaaa0001", &slow.user_code, true, 2001)
            .unwrap();
        assert!(matches!(
            access
                .collect_cli_sign_in_at(address(1), &slow.code, 2000 + CLI_SIGN_IN_SECONDS)
                .unwrap(),
            CliSignInCollected::Gone
        ));
        // Refused: the command line hears it once, then the sign-in is gone.
        let refused = access.start_cli_sign_in_at(address(1), "", 5000).unwrap();
        access
            .decide_cli_sign_in_at(
                ada(&access),
                "ada",
                "aaaa0001",
                &refused.user_code,
                false,
                5001,
            )
            .unwrap();
        assert!(matches!(
            access
                .collect_cli_sign_in_at(address(1), &refused.code, 5002)
                .unwrap(),
            CliSignInCollected::Denied
        ));
        assert!(matches!(
            access
                .collect_cli_sign_in_at(address(1), &refused.code, 5003)
                .unwrap(),
            CliSignInCollected::Gone
        ));
        assert!(access.tokens_for_org("ada", "aaaa0001").is_empty());
    }

    #[test]
    fn only_the_persons_own_session_in_their_own_organization_can_allow() {
        let (_dir, access) = hosted();
        let started = access.start_cli_sign_in_at(address(1), "", 1000).unwrap();
        let code = started.user_code.as_str();
        // Nobody, and a token instead of a session, decide nothing.
        assert!(matches!(
            access.decide_cli_sign_in_at(
                || Principal::Anonymous,
                "ada",
                "aaaa0001",
                code,
                true,
                1001
            ),
            Err(AccessError::Credentials)
        ));
        let (_, token) = access
            .create_self_token_for_fixture("ada", "aaaa0001", "old")
            .unwrap();
        assert!(matches!(
            access.decide_cli_sign_in_at(
                || access.authenticate(Some(&token), None),
                "ada",
                "aaaa0001",
                code,
                true,
                1001
            ),
            Err(AccessError::Credentials)
        ));
        // An organization the person does not belong to.
        assert!(matches!(
            access.decide_cli_sign_in_at(ada(&access), "ada", "bbbb0002", code, true, 1001),
            Err(AccessError::NotFound)
        ));
        // None of that spent the sign-in.
        access
            .decide_cli_sign_in_at(ada(&access), "ada", "aaaa0001", code, true, 1002)
            .unwrap();
    }

    #[test]
    fn starting_collecting_and_guessing_are_bounded() {
        let (_dir, access) = hosted();
        // One address starts only so many sign-ins in ten minutes; another address is not held up.
        for _ in 0..STARTS_PER_ADDRESS {
            access.start_cli_sign_in_at(address(1), "", 1000).unwrap();
        }
        assert!(matches!(
            access.start_cli_sign_in_at(address(1), "", 1001),
            Err(AccessError::Throttled)
        ));
        access.start_cli_sign_in_at(address(2), "", 1001).unwrap();
        access
            .start_cli_sign_in_at(address(1), "", 1000 + CLI_SIGN_IN_SECONDS)
            .unwrap();
        // An IPv6 client is one client across its /64: another of its addresses is held up too,
        // and the next /64 is not.
        let six = |text: &str| Some(text.parse::<std::net::IpAddr>().unwrap());
        for n in 0..STARTS_PER_ADDRESS {
            access
                .start_cli_sign_in_at(six(&format!("2001:db8:1:2::{:x}", n + 1)), "", 5000)
                .unwrap();
        }
        assert!(matches!(
            access.start_cli_sign_in_at(six("2001:db8:1:2:ffff::1"), "", 5001),
            Err(AccessError::Throttled)
        ));
        access
            .start_cli_sign_in_at(six("2001:db8:1:3::1"), "", 5001)
            .unwrap();
        // Collection attempts are bounded per address and minute.
        for _ in 0..COLLECTS_PER_ADDRESS {
            access
                .collect_cli_sign_in_at(address(3), "no-such-code", 9000)
                .unwrap();
        }
        assert!(matches!(
            access.collect_cli_sign_in_at(address(3), "no-such-code", 9001),
            Err(AccessError::Throttled)
        ));
        access
            .collect_cli_sign_in_at(address(3), "no-such-code", 9060)
            .unwrap();
        // A person who keeps typing wrong codes is refused the right one too, for a while.
        let real = access.start_cli_sign_in_at(address(4), "", 20_000).unwrap();
        for _ in 0..WRONG_CODES_PER_PERSON {
            assert!(matches!(
                access.decide_cli_sign_in_at(
                    ada(&access),
                    "ada",
                    "aaaa0001",
                    "BBBB-BBBB",
                    true,
                    20_001
                ),
                Err(AccessError::NotFound)
            ));
        }
        assert!(matches!(
            access.decide_cli_sign_in_at(
                ada(&access),
                "ada",
                "aaaa0001",
                &real.user_code,
                true,
                20_002
            ),
            Err(AccessError::Throttled)
        ));
    }

    #[test]
    fn everything_waiting_at_once_is_bounded() {
        let (_dir, access) = hosted();
        for n in 0..MAX_WAITING {
            let address = Some(std::net::IpAddr::from([
                10,
                0,
                (n / 250) as u8,
                (n % 250) as u8,
            ]));
            access.start_cli_sign_in_at(address, "", 1000).unwrap();
        }
        assert!(matches!(
            access.start_cli_sign_in_at(address(9), "", 1001),
            Err(AccessError::Throttled)
        ));
        // They expire and make room.
        access
            .start_cli_sign_in_at(address(9), "", 1000 + CLI_SIGN_IN_SECONDS)
            .unwrap();
    }

    #[test]
    fn a_short_code_reads_the_way_a_person_types_it() {
        assert_eq!(cli_user_code("bcdf-ghjk").as_deref(), Some("BCDF-GHJK"));
        assert_eq!(cli_user_code("BCDF GHJK").as_deref(), Some("BCDF-GHJK"));
        assert_eq!(cli_user_code("bcdfghjk").as_deref(), Some("BCDF-GHJK"));
        for wrong in [
            "",
            "BCDF-GHJ",
            "BCDF-GHJKL",
            "ABCD-EFGH",
            "1234-5678",
            "BCDF/GHJK",
        ] {
            assert!(cli_user_code(wrong).is_none(), "{wrong}");
        }
        for _ in 0..50 {
            let code = user_code().unwrap();
            assert_eq!(cli_user_code(&code).as_deref(), Some(code.as_str()));
        }
        assert_eq!(computer_name("  ada\u{1b}[31m-laptop\n "), "ada[31m-laptop");
        assert_eq!(computer_name(&"x".repeat(100)).len(), MAX_COMPUTER_CHARS);
    }
}
