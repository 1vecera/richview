//! Roles, sharing, sign-in and the audit trail of the report server (FEA-SF-19, FEA-SF-18, OPN-54).
//!
//! The model is small and strict. A person has an organisation role (`admin`, `author` or
//! `reader`). A report has an owner, a visibility (`private`, `org` or `public`) and per-person
//! grants (`owner`, `editor` or `viewer`). The rights a caller has on a report follow from those
//! and nothing else: `view`, `edit` (publish versions), `manage` (pin, unpin, regenerate),
//! `share` and `delete`. The account-free CLI, MCP server and VS Code extension publish with the
//! server's service token, which acts for the whole organisation.
//!
//! State lives in `<data-dir>/.access/`: `state.json` (people, sign-ins, report access, rewritten
//! atomically) and `audit.log` (one JSON line per action; addresses can be redacted on deletion). Passwords are
//! Argon2id; tokens and session cookies are stored only as SHA-256 digests.

pub use crate::hosted_links::LinkPurpose;
#[path = "access/teams.rs"]
mod teams;
use argon2::{Algorithm, Argon2, Params, Version};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
pub use teams::{
    Accepted, AccessRow, Invitation, InviteView, Landing, MemberRow, Relation, ReportAccess,
    VisibleReport,
};

/// A person's role in the organisation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrgRole {
    /// The organization's owner, with admin rights and last-owner protection.
    Owner,
    /// Manages people and sees and changes every report.
    Admin,
    /// Publishes reports and owns what they publish.
    Author,
    /// Reads what is shared with them.
    Reader,
}

/// What a grant lets a person do with one report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReportRole {
    Owner,
    Editor,
    Viewer,
}

/// Who can read a report without a grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    /// Only the owner, people with a grant, and admins.
    Private,
    /// Every signed-in person.
    Org,
    /// Anyone with the address.
    Public,
}

impl Visibility {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "private" => Some(Self::Private),
            "org" => Some(Self::Org),
            "public" => Some(Self::Public),
            _ => None,
        }
    }
}

/// One thing a caller may do with a report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Right {
    View,
    Edit,
    /// Pin, unpin and regenerate.
    Manage,
    Share,
    Delete,
}

impl ReportRole {
    fn rights(self) -> &'static [Right] {
        match self {
            Self::Owner => &[
                Right::View,
                Right::Edit,
                Right::Manage,
                Right::Share,
                Right::Delete,
            ],
            Self::Editor => &[Right::View, Right::Edit],
            Self::Viewer => &[Right::View],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub name: String,
    pub email: String,
    pub role: OrgRole,
    #[serde(default)]
    pub disabled: bool,
    pub created: String,
    /// When the person's email address was verified (legacy administrators vouched for theirs).
    #[serde(default)]
    pub verified: Option<String>,
    /// Host operator, separate from an organization administrator.
    #[serde(default)]
    pub operator: bool,
    /// Organization bound to the presented token; never serialized in a person record.
    #[serde(skip)]
    pub token_org: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Organization {
    pub id: String,
    pub name: String,
    pub created: String,
    #[serde(default)]
    pub suspended: bool,
    #[serde(default)]
    pub limits: OrganizationLimits,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrganizationLimits {
    pub bytes: Option<u64>,
    pub reports: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Membership {
    pub person: String,
    pub org: String,
    pub role: OrgRole,
    pub created: String,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Tenancy {
    #[default]
    Single,
    Hosted,
}

/// The name of the organization sign-up makes for a new author: "Jana Novak’s organization", so
/// pages and mails never offer to share with the person by their own name. It stays within the
/// limit a rename has, shortening a long name; the owner renames it on the members page.
fn first_organization_name(person: &str) -> String {
    const SUFFIX: &str = "’s organization";
    let room = teams::MAX_NAME_CHARS - SUFFIX.chars().count();
    let person: String = person.trim().chars().take(room).collect();
    format!("{}{SUFFIX}", person.trim_end())
}

fn default_org() -> String {
    "default".into()
}
fn schema_version() -> u32 {
    2
}
fn default_organizations() -> Vec<Organization> {
    vec![Organization {
        id: default_org(),
        name: "Default".into(),
        created: stamp(),
        suspended: false,
        limits: OrganizationLimits::default(),
    }]
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PasswordHash {
    salt: String,
    hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TokenRecord {
    id: String,
    name: String,
    sha256: String,
    created: String,
    #[serde(default = "default_org")]
    org: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserRecord {
    #[serde(flatten)]
    user: User,
    #[serde(default)]
    password: Option<PasswordHash>,
    #[serde(default)]
    tokens: Vec<TokenRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Session {
    sha256: String,
    csrf: String,
    user: String,
    expires: u64,
    /// Empty for a sign-in on the app origin; `report:<slug>` for a read session on the report
    /// origin, which opens that one report and nothing else (ARC-15).
    #[serde(default)]
    scope: String,
    /// The organization the person last chose in this browser; their first one when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    org: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingRegistration {
    email: String,
    token_sha256: String,
    created: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Grant {
    pub user: String,
    pub role: ReportRole,
}

/// Who may do what with one report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReportAcl {
    #[serde(default = "default_org")]
    pub org: String,
    /// A person's id, or `service` for a report published with the service token.
    pub owner: String,
    pub visibility: Visibility,
    #[serde(default)]
    pub grants: Vec<Grant>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct State {
    #[serde(default = "schema_version")]
    schema: u32,
    #[serde(default)]
    tenancy: Tenancy,
    #[serde(default = "default_organizations")]
    organizations: Vec<Organization>,
    #[serde(default)]
    memberships: Vec<Membership>,
    // Missing legacy entries conservatively count as one creation.
    #[serde(default)]
    org_creations: BTreeMap<String, usize>,
    users: Vec<UserRecord>,
    sessions: Vec<Session>,
    acls: BTreeMap<String, ReportAcl>,
    #[serde(default)]
    registrations: Vec<PendingRegistration>,
    #[serde(default)]
    registration_requests: Vec<u64>,
    #[serde(default)]
    signups_paused: bool,
    /// Pending invitations to an organization or to one report (hosted only).
    #[serde(default)]
    invitations: Vec<Invitation>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            schema: 2,
            tenancy: Tenancy::Single,
            organizations: default_organizations(),
            memberships: vec![],
            org_creations: BTreeMap::new(),
            users: vec![],
            sessions: vec![],
            acls: BTreeMap::new(),
            registrations: vec![],
            registration_requests: vec![],
            signups_paused: false,
            invitations: vec![],
        }
    }
}

fn valid_hosted_person_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn hosted_grants(grants: Vec<Grant>, owner: &str) -> Result<Vec<Grant>, AccessError> {
    let rank = |role: ReportRole| match role {
        ReportRole::Viewer => 0,
        ReportRole::Editor => 1,
        ReportRole::Owner => 2,
    };
    let mut unique = BTreeMap::<String, ReportRole>::new();
    for grant in grants.into_iter().filter(|g| g.user != owner) {
        if !valid_hosted_person_id(&grant.user) {
            return Err(AccessError::Invalid(
                "Invalid person id in report grants.".into(),
            ));
        }
        unique
            .entry(grant.user)
            .and_modify(|role| {
                if rank(grant.role) > rank(*role) {
                    *role = grant.role;
                }
            })
            .or_insert(grant.role);
        if unique.len() > MAX_HOSTED_GRANTS {
            return Err(AccessError::Invalid(format!(
                "A report can have at most {MAX_HOSTED_GRANTS} named grants."
            )));
        }
    }
    Ok(unique
        .into_iter()
        .map(|(user, role)| Grant { user, role })
        .collect())
}

fn hosted_claim_capacity(state: &State, org: &str, slug: &str) -> Result<(), AccessError> {
    if state.tenancy == Tenancy::Hosted && !state.acls.contains_key(slug) {
        if state.acls.len() >= MAX_HOSTED_ACLS {
            return Err(AccessError::Invalid("This server has reached its stored report-address limit; ask the operator for help.".into()));
        }
        if state.acls.values().filter(|acl| acl.org == org).count() >= MAX_HOSTED_ACLS_PER_ORG {
            return Err(AccessError::Invalid(format!(
                "This organization has reached its {MAX_HOSTED_ACLS_PER_ORG} stored report-address limit, including retired addresses."
            )));
        }
    }
    Ok(())
}

/// The shares of `slug` that are offered to an address and not yet accepted or expired.
fn pending_shares(state: &State, slug: &str) -> usize {
    let at = now();
    state
        .invitations
        .iter()
        .filter(|i| i.report.as_deref() == Some(slug) && i.expires > at)
        .count()
}

/// The one capacity rule for named people: a report's grants and its pending shares count
/// together, per report, per organization and on the server. `grants` and `pending` are what
/// `slug` would hold after the change; every other report counts as it stands. An acceptance
/// passes one grant more and one pending share fewer, so the offer it consumes counts once.
fn hosted_grant_capacity(
    state: &State,
    org: &str,
    slug: &str,
    grants: usize,
    pending: usize,
) -> Result<(), AccessError> {
    if state.tenancy != Tenancy::Hosted {
        return Ok(());
    }
    let at = now();
    let after = grants.saturating_add(pending);
    if after > MAX_HOSTED_GRANTS {
        return Err(AccessError::Invalid(format!(
            "A report can be shared with at most {MAX_HOSTED_GRANTS} named people, counting invitations that are still open."
        )));
    }
    let others = |in_org: Option<&str>| -> usize {
        let granted: usize = state
            .acls
            .iter()
            .filter(|(key, acl)| key.as_str() != slug && in_org.is_none_or(|org| acl.org == org))
            .map(|(_, acl)| acl.grants.len())
            .sum();
        let offered = state
            .invitations
            .iter()
            .filter(|i| {
                i.expires > at
                    && i.report.as_deref().is_some_and(|key| key != slug)
                    && in_org.is_none_or(|org| i.org == org)
            })
            .count();
        granted.saturating_add(offered)
    };
    if others(None).saturating_add(after) > MAX_HOSTED_TOTAL_GRANTS
        || others(Some(org)).saturating_add(after) > MAX_HOSTED_GRANTS_PER_ORG
    {
        return Err(AccessError::Invalid(
            "This organization or server has reached its named-grant limit, counting invitations that are still open.".into(),
        ));
    }
    Ok(())
}

// Run under the state lock so suspension, membership and this mutation have one ordering.
fn check_hosted_share(
    state: &State,
    slug: &str,
    caller: Option<&Principal>,
) -> Result<(), AccessError> {
    if state.tenancy != Tenancy::Hosted {
        return Ok(());
    }
    let acl = state.acls.get(slug).ok_or(AccessError::NotFound)?;
    if !state
        .organizations
        .iter()
        .any(|org| org.id == acl.org && !org.suspended)
    {
        return Err(AccessError::NotFound);
    }
    let Some(caller) = caller else {
        return Ok(());
    };
    let Principal::User(user) = caller else {
        return Err(AccessError::NotFound);
    };
    if user.token_org.as_ref().is_some_and(|org| org != &acl.org)
        || !state
            .users
            .iter()
            .any(|record| record.user.id == user.id && !record.user.disabled)
    {
        return Err(AccessError::NotFound);
    }
    let membership = state
        .memberships
        .iter()
        .find(|m| m.person == user.id && m.org == acl.org);
    let is_admin = membership.is_some_and(|m| matches!(m.role, OrgRole::Owner | OrgRole::Admin));
    let is_owner = membership.is_some() && acl.owner == user.id;
    let granted = acl
        .grants
        .iter()
        .any(|g| g.user == user.id && g.role == ReportRole::Owner);
    if is_admin || is_owner || granted {
        Ok(())
    } else {
        Err(AccessError::NotFound)
    }
}

/// The caller of a request.
#[derive(Debug, Clone)]
pub enum Principal {
    Anonymous,
    /// The server's service token: the account-free publishing clients.
    Service,
    User(User),
}

impl Principal {
    /// A short label for the audit trail.
    pub fn label(&self) -> String {
        match self {
            Self::Anonymous => "anonymous".into(),
            Self::Service => "service".into(),
            Self::User(user) => format!("user:{}", user.id),
        }
    }
}

/// Why a sign-in or an account change was refused.
/// Marks a publication commit under way: the count is odd from `open` until this is dropped.
struct CommitWindow(Arc<std::sync::atomic::AtomicU64>);

impl CommitWindow {
    fn open(commits: &Arc<std::sync::atomic::AtomicU64>) -> Self {
        commits.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Self(commits.clone())
    }
}

impl Drop for CommitWindow {
    fn drop(&mut self) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum AccessError {
    /// Wrong or unknown credentials; one answer for both.
    Credentials,
    /// Too many failed sign-ins; try again later.
    Throttled,
    /// The request breaks a rule, in one sentence.
    Invalid(String),
    NotFound,
    Storage(String),
}

impl From<String> for AccessError {
    fn from(error: String) -> Self {
        Self::Storage(error)
    }
}

const SESSION_SECONDS: u64 = 12 * 3600;
const MIN_PASSWORD: usize = 12;
const MAX_EMAIL_BYTES: usize = 254;
const MAX_PASSWORD_BYTES: usize = 4096;
const MAX_FAILURE_KEYS: usize = 1024;
const MAX_SIGN_IN_WORKERS: usize = 4;
const MAX_HOSTED_ACCOUNTS: usize = 500;
/// Hosted ACLs include retired addresses. Bound both the history and each report's grants.
const MAX_HOSTED_GRANTS: usize = 32;
const MAX_HOSTED_GRANTS_PER_ORG: usize = 256;
const MAX_HOSTED_TOTAL_GRANTS: usize = 128_000;
const MAX_HOSTED_ACLS_PER_ORG: usize = 256;
const MAX_HOSTED_ACLS: usize = 128_000;
/// The audit trail rolls over to `audit.log.1` past this size.
const AUDIT_LIMIT_BYTES: u64 = 8 * 1024 * 1024;
/// The current file and this many older ones are kept per trail.
const AUDIT_GENERATIONS: usize = 5;
const MAX_FAILURES: u32 = 5;
const LOCKOUT_SECONDS: u64 = 15 * 60;

/// A hand-over ticket: the person, the report, the app session it was issued from and the second
/// it lapses.
struct Ticket {
    user: String,
    slug: String,
    parent: String,
    until: u64,
}

/// A read session on the report origin. It lives in memory only, for one report, and ends with
/// the app session it came from.
struct ReportSession {
    user: String,
    slug: String,
    parent: String,
    expires: u64,
}

/// Report read sessions last an hour; the app session they hang from lasts twelve.
const REPORT_SESSION_SECONDS: u64 = 3600;
/// Live tickets and read sessions one person may hold at once.
const TICKETS_PER_USER: usize = 8;
const REPORT_SESSIONS_PER_USER: usize = 32;

#[derive(Default)]
struct SignInFailures {
    known: BTreeMap<String, (u32, u64)>,
    unknown: (u32, u64),
}

#[derive(Clone)]
pub(crate) struct AccountDeletion {
    /// Organizations the person is the only member of: removed with their reports.
    pub orgs: Vec<String>,
    pub email: String,
    pub invitations: Vec<String>,
}

/// Promote only a remaining member of an organization the deleted person owns.
fn deletion_successors(
    state: &mut State,
    person: &str,
    successors: &BTreeMap<String, String>,
) -> Result<(), AccessError> {
    for (org, successor) in successors {
        if successor == person
            || !state
                .memberships
                .iter()
                .any(|m| m.org == *org && m.person == person && m.role == OrgRole::Owner)
            || !state.users.iter().any(|u| u.user.id == *successor)
        {
            return Err(AccessError::Invalid(
                "Invalid successor for organization".into(),
            ));
        }
        let member = state
            .memberships
            .iter_mut()
            .find(|m| m.org == *org && m.person == *successor)
            .ok_or_else(|| AccessError::Invalid("Successor must be an existing member".into()))?;
        member.role = OrgRole::Owner;
    }
    Ok(())
}

/// What deleting `person` removes. An organization they alone belong to goes with them. In an
/// organization with other members their membership ends and their reports there pass to its
/// longest-standing other owner; a named successor is promoted first when needed.
fn account_deletion(state: &State, person: &str) -> Result<AccountDeletion, AccessError> {
    if state.tenancy != Tenancy::Hosted || !valid_hosted_person_id(person) {
        return Err(AccessError::NotFound);
    }
    let user = state
        .users
        .iter()
        .find(|record| record.user.id == person)
        .ok_or(AccessError::NotFound)?;
    if user.user.email.trim().is_empty() {
        return Err(AccessError::Invalid("Account email is unavailable".into()));
    }
    let mut orgs = Vec::new();
    for membership in state.memberships.iter().filter(|m| m.person == person) {
        if !state.organizations.iter().any(|o| o.id == membership.org) {
            return Err(AccessError::Invalid(
                "Account belongs to an unknown organization".into(),
            ));
        }
        let others = || {
            state
                .memberships
                .iter()
                .filter(|m| m.org == membership.org && m.person != person)
        };
        if others().next().is_none() {
            orgs.push(membership.org.clone());
        } else if !others().any(|m| m.role == OrgRole::Owner) {
            return Err(AccessError::Invalid(
                "Account is the last owner of an organization with other members".into(),
            ));
        }
    }
    // A report left in an organization that stays needs another owner there to pass to.
    if state.acls.values().any(|acl| {
        acl.owner == person && !orgs.contains(&acl.org) && heir(state, &acl.org, person).is_none()
    }) {
        return Err(AccessError::Invalid(
            "Account owns reports in an organization without another owner".into(),
        ));
    }
    orgs.sort();
    orgs.dedup();
    Ok(AccountDeletion {
        orgs,
        email: user.user.email.clone(),
        invitations: state
            .invitations
            .iter()
            .filter(|invitation| invitation.inviter == person)
            .map(|invitation| invitation.id.clone())
            .collect(),
    })
}

/// The longest-standing owner of `org` other than `person`.
fn heir(state: &State, org: &str, person: &str) -> Option<String> {
    state
        .memberships
        .iter()
        .filter(|m| m.org == org && m.person != person && m.role == OrgRole::Owner)
        .min_by(|a, b| a.created.cmp(&b.created))
        .map(|m| m.person.clone())
}

#[derive(Clone)]
pub struct Access {
    dir: Arc<PathBuf>,
    links: Option<Arc<crate::hosted_links::Links>>,
    mail: Arc<std::sync::OnceLock<(crate::mail::MailQueue, bool)>>,
    /// Hosted state is a file rewritten from memory; a second server must not open it concurrently.
    _host_lock: Option<Arc<fs::File>>,
    state: Arc<Mutex<State>>,
    /// Serializes the slug ownership decision through the publication and its claim.
    pub(crate) publication: Arc<tokio::sync::Mutex<()>>,
    /// Held by every change to the access state, and by a publication from its last rights
    /// check to its stored version: an access change is wholly before or wholly after it.
    mutations: Arc<Mutex<()>>,
    /// Access changes waiting for `mutations` right now.
    waiting: Arc<std::sync::atomic::AtomicUsize>,
    /// Odd while a publication commits, and two higher after each one. A reader that saw it
    /// change cannot tell which state its check and its bytes belong to (`commit_epoch`).
    commits: Arc<std::sync::atomic::AtomicU64>,
    /// Serializes email-link and outbox work with permanent account deletion.
    pub(crate) mail_privacy: Arc<tokio::sync::Mutex<()>>,
    audit_lock: Arc<Mutex<()>>,
    failures: Arc<Mutex<SignInFailures>>,
    sign_in_workers: Arc<tokio::sync::Semaphore>,
    link_request_workers: Arc<tokio::sync::Semaphore>,
    link_request_ips: Arc<Mutex<BTreeMap<String, std::collections::VecDeque<u64>>>>,
    publish_limits: Arc<Mutex<BTreeMap<String, std::collections::VecDeque<u64>>>>,
    publish_workers: Arc<tokio::sync::Semaphore>,
    compute_limits: Arc<Mutex<BTreeMap<String, std::collections::VecDeque<u64>>>>,
    compute_workers: Arc<tokio::sync::Semaphore>,
    signup_account_limit: Arc<std::sync::atomic::AtomicUsize>,
    org_creation_limit: Arc<std::sync::atomic::AtomicUsize>,
    /// Invitation and share mails one person asked for in the last day.
    invite_limits: Arc<Mutex<BTreeMap<String, std::collections::VecDeque<u64>>>>,
    /// Registration has a separate lane so signup floods cannot consume sign-in admission.
    registration_workers: Arc<tokio::sync::Semaphore>,
    /// Single-use hand-overs from the app origin to the report origin: digest → (person, slug, expiry).
    grants: Arc<Mutex<BTreeMap<String, Ticket>>>,
    report_sessions: Arc<Mutex<BTreeMap<String, ReportSession>>>,
    default_visibility: Visibility,
    service_token: Option<Arc<str>>,
    /// How long a sign-in waits for the hashing lane before it is refused as throttled.
    lane_patience: std::time::Duration,
}

/// One hash at a time: each takes 19 MiB, and an attacker chooses how many are asked for.
static HASHING: Mutex<()> = Mutex::new(());

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn stamp() -> String {
    crate::time::Instant::now().millis()
}

fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

/// `bytes` random bytes from the operating system, as unpadded URL-safe base64.
fn random_text(bytes: usize) -> Result<String, String> {
    let mut buffer = vec![0u8; bytes];
    getrandom::fill(&mut buffer).map_err(|e| e.to_string())?;
    Ok(base64::Engine::encode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        buffer,
    ))
}

pub(crate) fn sync_directory(dir: &Path) -> Result<(), String> {
    #[cfg(unix)]
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|e| e.to_string())?;
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}

/// Persist each new directory entry, including newly created ancestors.
pub(crate) fn create_durable_directory(path: &Path) -> Result<(), String> {
    let mut missing = Vec::new();
    let mut current = path;
    while !current.exists() {
        missing.push(current.to_path_buf());
        current = current
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
    }
    fs::create_dir_all(path).map_err(|e| e.to_string())?;
    for created in missing.iter().rev() {
        let parent = created
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        sync_directory(parent)?;
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    sync_directory(parent)
}

/// Install only a complete, synced v1 backup; recognize the old interrupted-prefix failure.
fn migration_backup(dir: &Path, original: &[u8]) -> Result<(), String> {
    let backup = dir.join("state.v1.json");
    let existing = match fs::symlink_metadata(&backup) {
        Ok(meta) if meta.file_type().is_file() => {
            Some(fs::read(&backup).map_err(|e| e.to_string())?)
        }
        Ok(_) => return Err("The v1 backup must be a regular file".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.to_string()),
    };
    if existing.as_deref() == Some(original) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if fs::metadata(&backup)
                .map_err(|e| e.to_string())?
                .permissions()
                .mode()
                & 0o077
                != 0
            {
                return Err("The v1 access backup must not be readable by others".into());
            }
        }
        return sync_directory(dir);
    }
    let interrupted = existing.as_ref().is_some_and(|bytes| {
        bytes.len() < original.len()
            && original.starts_with(bytes)
            && serde_json::from_slice::<Value>(bytes).is_err()
    });
    if existing.is_some() && !interrupted {
        return Err("Existing v1 backup differs from the state being migrated".into());
    }
    let mut staged = tempfile::NamedTempFile::new_in(dir).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        staged
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
    }
    staged
        .write_all(original)
        .and_then(|_| staged.as_file().sync_all())
        .map_err(|e| e.to_string())?;
    if interrupted {
        // Keep the incomplete predecessor as evidence, even though it is provably a prefix of
        // this source. A crash at any point leaves the original state and at least one backup.
        let mut partial = tempfile::NamedTempFile::new_in(dir).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            partial
                .as_file()
                .set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|e| e.to_string())?;
        }
        partial
            .write_all(existing.as_deref().unwrap_or_default())
            .and_then(|_| partial.as_file().sync_all())
            .map_err(|e| e.to_string())?;
        let preserved = dir.join(format!("state.v1.partial-{}.json", random_text(12)?));
        partial
            .persist_noclobber(preserved)
            .map_err(|e| e.error.to_string())?;
        sync_directory(dir)?;
        staged.persist(&backup).map_err(|e| e.error.to_string())?;
    } else {
        staged
            .persist_noclobber(&backup)
            .map_err(|e| e.error.to_string())?;
    }
    sync_directory(dir)
}

fn hash_password(password: &str, salt: &[u8]) -> Result<Vec<u8>, String> {
    // OWASP's Argon2id floor: 19 MiB, two passes, one lane.
    let params = Params::new(19 * 1024, 2, 1, Some(32)).map_err(|e| e.to_string())?;
    let mut out = vec![0u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, &mut out)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

fn b64(bytes: &[u8]) -> String {
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD_NO_PAD, bytes)
}

fn unb64(text: &str) -> Option<Vec<u8>> {
    base64::Engine::decode(&base64::engine::general_purpose::STANDARD_NO_PAD, text).ok()
}

/// What a person reads when the password they chose is too short: the rule itself.
pub(crate) fn short_password() -> String {
    format!("A password needs at least {MIN_PASSWORD} characters.")
}

fn new_password_hash(password: &str) -> Result<PasswordHash, AccessError> {
    if password.chars().count() < MIN_PASSWORD {
        return Err(AccessError::Invalid(short_password()));
    }
    if password.len() > MAX_PASSWORD_BYTES {
        return Err(AccessError::Invalid(format!(
            "A password has at most {MAX_PASSWORD_BYTES} bytes."
        )));
    }
    let mut salt = [0u8; 16];
    getrandom::fill(&mut salt).map_err(|e| AccessError::Storage(e.to_string()))?;
    Ok(PasswordHash {
        salt: b64(&salt),
        hash: b64(&hash_password(password, &salt)?),
    })
}

fn password_matches(stored: &PasswordHash, password: &str) -> bool {
    let (Some(salt), Some(expected)) = (unb64(&stored.salt), unb64(&stored.hash)) else {
        return false;
    };
    let Ok(actual) = hash_password(password, &salt) else {
        return false;
    };
    // Both are 32 bytes of a hash: compare without an early exit.
    actual.len() == expected.len()
        && actual
            .iter()
            .zip(&expected)
            .fold(0u8, |diff, (a, b)| diff | (a ^ b))
            == 0
}

impl Access {
    /// Opens the access state under `data_dir/.access`, creating it when absent.
    pub fn open(
        data_dir: &Path,
        default_visibility: Visibility,
        service_token: Option<Arc<str>>,
    ) -> Result<Self, String> {
        Self::open_for(data_dir, default_visibility, service_token, Tenancy::Single)
    }

    /// Opens a fresh or already hosted data directory; single-server data cannot be reinterpreted.
    pub fn open_hosted(data_dir: &Path) -> Result<Self, String> {
        Self::open_for(data_dir, Visibility::Private, None, Tenancy::Hosted)
    }

    fn open_for(
        data_dir: &Path,
        default_visibility: Visibility,
        service_token: Option<Arc<str>>,
        tenancy: Tenancy,
    ) -> Result<Self, String> {
        if tenancy == Tenancy::Hosted {
            create_durable_directory(data_dir)?;
        } else {
            fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
        }
        if tenancy == Tenancy::Hosted
            && !data_dir.join(".access/state.json").exists()
            && fs::read_dir(data_dir)
                .map_err(|e| e.to_string())?
                .next()
                .is_some()
        {
            return Err("Hosted mode requires an empty or already hosted data directory".into());
        }
        let dir = data_dir.join(".access");
        if tenancy == Tenancy::Hosted {
            create_durable_directory(&dir)?;
        } else {
            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        }
        // Digests of passwords and tokens, and the trail: this server's user only.
        #[cfg(unix)]
        {
            let protected =
                fs::set_permissions(&dir, std::os::unix::fs::PermissionsExt::from_mode(0o700));
            if tenancy == Tenancy::Hosted {
                protected.map_err(|e| e.to_string())?;
            }
        }
        let host_lock = if tenancy == Tenancy::Hosted {
            let path = dir.join(".hosted.lock");
            if fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink()) {
                return Err("Hosted state lock cannot be a symlink".into());
            }
            let mut options = fs::OpenOptions::new();
            options.read(true).write(true).create(true).truncate(false);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
            let lock = options.open(path).map_err(|e| e.to_string())?;
            fs2::FileExt::try_lock_exclusive(&lock)
                .map_err(|_| "Hosted data directory is already in use".to_owned())?;
            Some(Arc::new(lock))
        } else {
            None
        };
        let path = dir.join("state.json");
        let mut migrated = false;
        let state = match fs::read(&path) {
            Ok(bytes) => {
                let mut value: Value = serde_json::from_slice(&bytes)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                let legacy = value.get("schema").is_none();
                if legacy && tenancy == Tenancy::Hosted {
                    return Err("This data directory is single-server data; hosted mode requires a separate directory".into());
                }
                if legacy {
                    migration_backup(&dir, &bytes)?;
                } else {
                    if value.get("schema").and_then(Value::as_u64) != Some(2) {
                        return Err("Unsupported access state schema".into());
                    }
                    // Role is a membership property on disk. The compatibility User returned to
                    // the single-server API still carries its default-organization role.
                    let roles: BTreeMap<String, Value> = value
                        .get("memberships")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter(|m| tenancy == Tenancy::Hosted || m["org"] == "default")
                        .filter_map(|m| Some((m["person"].as_str()?.to_owned(), m["role"].clone())))
                        .collect();
                    if let Some(users) = value.get_mut("users").and_then(Value::as_array_mut) {
                        for user in users {
                            let role = user
                                .get("id")
                                .and_then(Value::as_str)
                                .and_then(|id| roles.get(id))
                                .cloned()
                                .unwrap_or(json!("reader"));
                            if let Some(object) = user.as_object_mut() {
                                object.insert("role".into(), role);
                            }
                        }
                    }
                }
                let mut state: State = serde_json::from_value(value)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                if state.tenancy != tenancy {
                    return Err("This data directory belongs to a different server mode".into());
                }
                if legacy {
                    state.memberships = state
                        .users
                        .iter_mut()
                        .map(|record| {
                            record.user.verified = Some(record.user.created.clone());
                            Membership {
                                person: record.user.id.clone(),
                                org: default_org(),
                                role: record.user.role,
                                created: record.user.created.clone(),
                            }
                        })
                        .collect();
                    migrated = true;
                }
                if tenancy == Tenancy::Single
                    && (state.organizations.len() != 1
                        || state.organizations[0].id != "default"
                        || state.users.iter().any(|user| {
                            state
                                .memberships
                                .iter()
                                .filter(|m| m.person == user.user.id && m.org == "default")
                                .count()
                                != 1
                        })
                        || state.memberships.iter().any(|m| {
                            m.org != "default" || !state.users.iter().any(|u| u.user.id == m.person)
                        })
                        || state.acls.values().any(|acl| acl.org != "default")
                        || state
                            .users
                            .iter()
                            .any(|u| u.tokens.iter().any(|t| t.org != "default")))
                {
                    return Err(
                        "Single-server access state has invalid organization membership".into(),
                    );
                }
                if tenancy == Tenancy::Hosted {
                    if state.acls.len() > MAX_HOSTED_ACLS {
                        return Err(
                            "Hosted access state exceeds the stored report-address limit".into(),
                        );
                    }
                    let mut org_acl_counts = BTreeMap::<String, usize>::new();
                    let mut org_grant_counts = BTreeMap::<String, usize>::new();
                    let mut total_grants = 0usize;
                    for (key, acl) in &mut state.acls {
                        let valid_key = key.split_once('/').is_some_and(|(org, slug)| {
                            org == acl.org && crate::storage::ReportKey::hosted(org, slug).is_ok()
                        });
                        if !valid_key || !valid_hosted_person_id(&acl.owner) {
                            return Err("Hosted access state has invalid report metadata".into());
                        }
                        let count = org_acl_counts.entry(acl.org.clone()).or_default();
                        *count += 1;
                        if *count > MAX_HOSTED_ACLS_PER_ORG {
                            return Err("Hosted access state exceeds an organization's stored report-address limit".into());
                        }
                        let old = std::mem::take(&mut acl.grants);
                        let old_len = old.len();
                        acl.grants = hosted_grants(old, &acl.owner)
                            .map_err(|_| "Hosted access state exceeds its report-grant limit")?;
                        migrated |= acl.grants.len() != old_len;
                        let grants = org_grant_counts.entry(acl.org.clone()).or_default();
                        *grants += acl.grants.len();
                        total_grants += acl.grants.len();
                        if *grants > MAX_HOSTED_GRANTS_PER_ORG
                            || total_grants > MAX_HOSTED_TOTAL_GRANTS
                        {
                            return Err(
                                "Hosted access state exceeds its retained grant limit".into()
                            );
                        }
                    }
                    let valid_org = |id: &str| {
                        id.len() == 8
                            && id
                                .bytes()
                                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
                    };
                    let orgs: BTreeSet<_> = state
                        .organizations
                        .iter()
                        .map(|org| org.id.as_str())
                        .collect();
                    let memberships: BTreeSet<_> = state
                        .memberships
                        .iter()
                        .map(|m| (m.person.as_str(), m.org.as_str()))
                        .collect();
                    if orgs.len() != state.organizations.len()
                        || orgs.iter().any(|id| !valid_org(id))
                        || memberships.len() != state.memberships.len()
                        || state.memberships.iter().any(|m| {
                            !orgs.contains(m.org.as_str())
                                || !state.users.iter().any(|u| u.user.id == m.person)
                        })
                        || state.users.iter().any(|u| {
                            !valid_hosted_person_id(&u.user.id)
                                || u.user.verified.is_none()
                                || u.tokens.iter().any(|t| !orgs.contains(t.org.as_str()))
                        })
                        || state.acls.iter().any(|(key, acl)| {
                            !orgs.contains(acl.org.as_str())
                                || !key.starts_with(&format!("{}/", acl.org))
                        })
                    {
                        return Err(
                            "Hosted access state has invalid organization membership".into()
                        );
                    }
                    let known: BTreeSet<String> = state
                        .organizations
                        .iter()
                        .map(|org| org.id.clone())
                        .collect();
                    let before = state.invitations.len();
                    state.invitations.retain(|i| known.contains(&i.org));
                    migrated |= state.invitations.len() != before;
                }
                state
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut state = State::default();
                if tenancy == Tenancy::Hosted {
                    state.tenancy = Tenancy::Hosted;
                    state.organizations.clear();
                    migrated = true;
                }
                state
            }
            Err(e) => return Err(e.to_string()),
        };
        let links = if tenancy == Tenancy::Hosted {
            Some(Arc::new(crate::hosted_links::Links::open(&dir)?))
        } else {
            None
        };
        let invite_limits = if tenancy == Tenancy::Hosted {
            Self::load_invite_limits(&dir)?
        } else {
            BTreeMap::new()
        };
        let access = Self {
            dir: Arc::new(dir),
            links,
            mail: Arc::default(),
            _host_lock: host_lock,
            state: Arc::new(Mutex::new(state)),
            publication: Arc::default(),
            mutations: Arc::default(),
            waiting: Arc::default(),
            commits: Arc::default(),
            mail_privacy: Arc::default(),
            audit_lock: Arc::default(),
            failures: Arc::default(),
            sign_in_workers: Arc::new(tokio::sync::Semaphore::new(MAX_SIGN_IN_WORKERS)),
            link_request_workers: Arc::new(tokio::sync::Semaphore::new(4)),
            link_request_ips: Arc::default(),
            publish_limits: Arc::default(),
            publish_workers: Arc::new(tokio::sync::Semaphore::new(4)),
            compute_limits: Arc::default(),
            compute_workers: Arc::new(tokio::sync::Semaphore::new(4)),
            signup_account_limit: Arc::new(std::sync::atomic::AtomicUsize::new(
                MAX_HOSTED_ACCOUNTS,
            )),
            org_creation_limit: Arc::new(std::sync::atomic::AtomicUsize::new(1)),
            invite_limits: Arc::new(Mutex::new(invite_limits)),
            registration_workers: Arc::new(tokio::sync::Semaphore::new(1)),
            grants: Arc::default(),
            report_sessions: Arc::default(),
            default_visibility,
            service_token: service_token.filter(|t| !t.is_empty()),
            // An unoptimized build hashes many times slower, so a test suite signing in on many
            // threads is given far longer than the release path's few seconds.
            lane_patience: std::time::Duration::from_secs(if cfg!(debug_assertions) {
                60
            } else {
                3
            }),
        };
        if migrated {
            let state = access
                .state
                .lock()
                .map_err(|_| "Access state lock poisoned")?;
            access.save(&state)?;
            sync_directory(&access.dir)?;
        }
        // An invitation that expired while the server was down does not wait for a write.
        access
            .purge_expired_invitations()
            .map_err(|_| "Could not remove expired invitations from the access state")?;
        if tenancy == Tenancy::Hosted {
            access.purge_expired_invite_mail_budgets()?;
        }
        Ok(access)
    }

    pub fn default_visibility(&self) -> Visibility {
        self.default_visibility
    }

    pub fn is_hosted(&self) -> bool {
        self.read(|state| state.tenancy == Tenancy::Hosted)
    }

    /// Configure hosted delivery once, before serving requests.
    pub fn configure_mail(
        &self,
        transport: Arc<dyn crate::mail::Transport>,
        daily_limit: u64,
        manual_outbox: bool,
    ) -> Result<(), String> {
        if !self.is_hosted() {
            return Err("Mail requires hosted mode".into());
        }
        if self.mail.get().is_some() {
            return Err("Mail is already configured".into());
        }
        let links = self.links.clone().ok_or("Links require hosted mode")?;
        let check = Arc::new(move |token: &str| links.peek(token).map(|link| link.expires));
        let queue = crate::mail::MailQueue::start_with_links(
            transport,
            &self.dir,
            daily_limit,
            Some(check),
            Some(self.mail_maintenance()),
        )?;
        self.mail
            .set((queue, manual_outbox))
            .map_err(|_| "Mail is already configured".to_owned())
    }

    fn mail_maintenance(&self) -> crate::mail::AccessMaintenance {
        let mut access = self.clone();
        // The queue owns this closure; its access clone must not own that same queue.
        access.mail = Arc::default();
        Arc::new(move || {
            let invitations = access
                .purge_expired_invitations()
                .map(|_| ())
                .map_err(|_| "Could not remove expired invitations".to_owned());
            let budgets = access.purge_expired_invite_mail_budgets();
            invitations.and(budgets)
        })
    }

    pub fn mail_queue(&self) -> Option<&crate::mail::MailQueue> {
        self.mail.get().map(|(queue, _)| queue)
    }

    /// Whether configured mail is written for a person to deliver by hand.
    pub fn manual_outbox(&self) -> Option<bool> {
        self.mail.get().map(|(_, manual)| *manual)
    }

    /// Inspect a live email link without consuming it, including when a mail scanner follows it.
    pub fn inspect_link(&self, token: &str) -> Option<LinkPurpose> {
        self.links.as_ref()?.inspect(token)
    }

    /// Reserve a link for delivery; a refused address or rate limit returns no token.
    pub fn issue_link(&self, email: &str, purpose: LinkPurpose) -> Result<Option<String>, String> {
        self.links
            .as_ref()
            .ok_or("Links require hosted mode")?
            .issue(email, purpose)
    }

    pub(crate) fn issue_anonymous_link(
        &self,
        email: &str,
        purpose: LinkPurpose,
    ) -> Result<Option<String>, String> {
        self.links
            .as_ref()
            .ok_or("Links require hosted mode")?
            .issue_anonymous(email, purpose)
    }

    /// Bound anonymous link requests by the trusted proxy's client IP, before allocating work.
    pub(crate) fn admit_link_request(
        &self,
        ip: Option<std::net::IpAddr>,
    ) -> Result<tokio::sync::OwnedSemaphorePermit, AccessError> {
        let key = ip.map_or_else(|| "unknown".to_owned(), |address| address.to_string());
        let at = now();
        {
            let mut counters = self
                .link_request_ips
                .lock()
                .map_err(|_| AccessError::Storage("Link request counter poisoned".into()))?;
            counters.retain(|_, times| {
                while times
                    .front()
                    .is_some_and(|time| *time <= at.saturating_sub(3600))
                {
                    times.pop_front();
                }
                !times.is_empty()
            });
            if !counters.contains_key(&key) && counters.len() >= 1024 {
                return Err(AccessError::Throttled);
            }
            let entry = counters.entry(key).or_default();
            if entry.len() >= 10 {
                return Err(AccessError::Throttled);
            }
            entry.push_back(at);
        }
        self.link_request_workers
            .clone()
            .try_acquire_owned()
            .map_err(|_| AccessError::Throttled)
    }

    pub(crate) fn hosted_email_exists(&self, email: &str) -> bool {
        let email = email.trim();
        self.read(|state| {
            state
                .users
                .iter()
                .any(|user| user.user.email.eq_ignore_ascii_case(email))
        })
    }

    /// Admit at most 60 hosted publish attempts per organization in a rolling-hour window.
    pub(crate) fn admit_hosted_publish(&self, key: &str) -> Result<(), AccessError> {
        if !self.is_hosted() {
            return Ok(());
        }
        let org = self.report_org(key);
        let at = now();
        let mut counters = self
            .publish_limits
            .lock()
            .map_err(|_| AccessError::Storage("Publish counter poisoned".into()))?;
        counters.retain(|_, times| {
            while times
                .front()
                .is_some_and(|time| *time <= at.saturating_sub(3600))
            {
                times.pop_front();
            }
            !times.is_empty()
        });
        if !counters.contains_key(&org) && counters.len() >= 1024 {
            return Err(AccessError::Throttled);
        }
        let entry = counters.entry(org).or_default();
        if entry.len() >= 60 {
            return Err(AccessError::Throttled);
        }
        entry.push_back(at);
        Ok(())
    }

    pub(crate) fn admit_hosted_publish_worker(
        &self,
    ) -> Result<tokio::sync::OwnedSemaphorePermit, AccessError> {
        self.publish_workers
            .clone()
            .try_acquire_owned()
            .map_err(|_| AccessError::Throttled)
    }

    /// Admit hosted computation before its request body is read, by the caller's organization.
    pub(crate) fn admit_hosted_compute(
        &self,
        principal: &Principal,
    ) -> Result<tokio::sync::OwnedSemaphorePermit, AccessError> {
        let Principal::User(user) = principal else {
            return Err(AccessError::Credentials);
        };
        let org = self
            .read(|state| {
                state
                    .memberships
                    .iter()
                    .find(|m| {
                        m.person == user.id
                            && user.token_org.as_ref().is_none_or(|bound| bound == &m.org)
                            && state
                                .organizations
                                .iter()
                                .any(|org| org.id == m.org && !org.suspended)
                    })
                    .map(|m| m.org.clone())
            })
            .ok_or(AccessError::Credentials)?;
        let permit = self
            .compute_workers
            .clone()
            .try_acquire_owned()
            .map_err(|_| AccessError::Throttled)?;
        let at = now();
        let mut counters = self
            .compute_limits
            .lock()
            .map_err(|_| AccessError::Storage("Compute counter poisoned".into()))?;
        counters.retain(|_, times| {
            while times
                .front()
                .is_some_and(|time| *time <= at.saturating_sub(3600))
            {
                times.pop_front();
            }
            !times.is_empty()
        });
        if !counters.contains_key(&org) && counters.len() >= 1024 {
            return Err(AccessError::Throttled);
        }
        let entry = counters.entry(org).or_default();
        if entry.len() >= 60 {
            return Err(AccessError::Throttled);
        }
        entry.push_back(at);
        Ok(permit)
    }

    /// Spend a verified link and create an organization or reset an existing password.
    pub async fn complete_hosted_link(
        &self,
        token: &str,
        name: &str,
        password: &str,
    ) -> Result<User, AccessError> {
        self.complete_hosted_link_inner(token, name, password, false, None)
            .await
            .map(|(user, _)| user)
    }

    /// Complete a verified link and create the browser session in the same state transaction.
    pub(crate) async fn complete_hosted_link_and_sign_in(
        &self,
        token: &str,
        name: &str,
        password: &str,
        publication: Option<tokio::sync::OwnedMutexGuard<()>>,
    ) -> Result<(User, String, String), AccessError> {
        let (user, session) = self
            .complete_hosted_link_inner(token, name, password, true, publication)
            .await?;
        let (cookie, csrf) = session.ok_or(AccessError::Credentials)?;
        Ok((user, cookie, csrf))
    }

    async fn complete_hosted_link_inner(
        &self,
        token: &str,
        name: &str,
        password: &str,
        sign_in: bool,
        publication: Option<tokio::sync::OwnedMutexGuard<()>>,
    ) -> Result<(User, Option<(String, String)>), AccessError> {
        // Bound requests waiting on the deletion lock as well as the hashing workers.
        let permit = self
            .registration_workers
            .clone()
            .try_acquire_owned()
            .map_err(|_| AccessError::Throttled)?;
        let privacy = self.mail_privacy.clone().lock_owned().await;
        let purpose = self.inspect_link(token).ok_or(AccessError::Credentials)?;
        if purpose == LinkPurpose::Signup && !self.signup_account_capacity() {
            return Err(AccessError::Throttled);
        }
        if purpose == LinkPurpose::Invite
            || (purpose == LinkPurpose::Signup
                && (name.trim().is_empty() || name.trim().chars().count() > 120))
            || password.len() > MAX_PASSWORD_BYTES
        {
            return Err(AccessError::Invalid("Check the name and password.".into()));
        }
        if password.chars().count() < MIN_PASSWORD {
            return Err(AccessError::Invalid(short_password()));
        }
        let access = self.clone();
        let token = token.to_owned();
        let name = name.trim().to_owned();
        let password = password.to_owned();
        let session = if sign_in {
            Some((random_text(32)?, random_text(24)?))
        } else {
            None
        };
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let _privacy = privacy;
            // Cancellation must not release the publication lock while this worker can still
            // create an account after the storage-threshold check.
            let _publication = publication;
            let started = std::time::Instant::now();
            let _lane = loop {
                match HASHING.try_lock() {
                    Ok(lane) => break lane,
                    Err(std::sync::TryLockError::Poisoned(error)) => break error.into_inner(),
                    Err(std::sync::TryLockError::WouldBlock)
                        if started.elapsed() < access.lane_patience =>
                    {
                        std::thread::sleep(std::time::Duration::from_millis(20));
                    }
                    Err(std::sync::TryLockError::WouldBlock) => return Err(AccessError::Throttled),
                }
            };
            let password_hash = new_password_hash(&password)?;
            access.with(|state| {
                if state.tenancy != Tenancy::Hosted {
                    return Err(AccessError::Credentials);
                }
                if purpose == LinkPurpose::Signup
                    && (state.signups_paused
                        || state.users.len()
                            >= access
                                .signup_account_limit
                                .load(std::sync::atomic::Ordering::Relaxed))
                {
                    return Err(AccessError::Throttled);
                }
                let link = access
                    .links
                    .as_ref()
                    .ok_or(AccessError::Credentials)?
                    .take(&token)?
                    .ok_or(AccessError::Credentials)?;
                let user = match link.purpose {
                    LinkPurpose::Signup => {
                        if state
                            .users
                            .iter()
                            .any(|u| u.user.email.eq_ignore_ascii_case(&link.email))
                        {
                            return Err(AccessError::Credentials);
                        }
                        let id = loop {
                            let candidate = format!(
                                "u-{}",
                                random_text(9)?.to_lowercase().replace(['_', '-'], "x")
                            );
                            if !state.users.iter().any(|u| u.user.id == candidate) {
                                break candidate;
                            }
                        };
                        let org = loop {
                            let mut bytes = [0u8; 8];
                            getrandom::fill(&mut bytes)
                                .map_err(|e| AccessError::Storage(e.to_string()))?;
                            if bytes.iter().any(|b| *b >= 252) {
                                continue;
                            }
                            let alphabet = b"abcdefghijklmnopqrstuvwxyz0123456789";
                            let candidate: String = bytes
                                .iter()
                                .map(|b| alphabet[(*b % 36) as usize] as char)
                                .collect();
                            if !state.organizations.iter().any(|o| o.id == candidate) {
                                break candidate;
                            }
                        };
                        let created = stamp();
                        let user = User {
                            id,
                            name,
                            email: link.email,
                            role: OrgRole::Owner,
                            disabled: false,
                            created: created.clone(),
                            verified: Some(created.clone()),
                            operator: false,
                            token_org: None,
                        };
                        state.organizations.push(Organization {
                            id: org.clone(),
                            name: first_organization_name(&user.name),
                            created: created.clone(),
                            suspended: false,
                            limits: OrganizationLimits::default(),
                        });
                        state.memberships.push(Membership {
                            person: user.id.clone(),
                            org,
                            role: OrgRole::Owner,
                            created,
                        });
                        state.org_creations.insert(user.id.clone(), 1);
                        state.users.push(UserRecord {
                            user: user.clone(),
                            password: Some(password_hash),
                            tokens: vec![],
                        });
                        user
                    }
                    LinkPurpose::Reset => {
                        let user = state
                            .users
                            .iter_mut()
                            .find(|u| {
                                u.user.email.eq_ignore_ascii_case(&link.email) && !u.user.disabled
                            })
                            .ok_or(AccessError::Credentials)?;
                        user.password = Some(password_hash);
                        user.tokens.clear();
                        let result = user.user.clone();
                        state.sessions.retain(|s| s.user != result.id);
                        result
                    }
                    LinkPurpose::Invite => return Err(AccessError::Credentials),
                };
                if let Some((cookie, csrf)) = &session {
                    let at = now();
                    state.sessions.retain(|s| s.expires > at);
                    let mut count = state.sessions.iter().filter(|s| s.user == user.id).count();
                    state.sessions.retain(|s| {
                        if s.user == user.id && count >= 10 {
                            count -= 1;
                            false
                        } else {
                            true
                        }
                    });
                    state.sessions.push(Session {
                        sha256: digest(cookie),
                        csrf: csrf.clone(),
                        user: user.id.clone(),
                        expires: at + SESSION_SECONDS,
                        scope: String::new(),
                        org: None,
                    });
                }
                Ok((user, session))
            })
        })
        .await
        .map_err(|_| AccessError::Storage("Link completion worker failed".into()))?
    }

    fn report_org(&self, key: &str) -> String {
        if self.is_hosted() {
            key.split_once('/').map_or("", |(org, _)| org).to_owned()
        } else {
            default_org()
        }
    }

    pub fn report_org_active(&self, key: &str) -> bool {
        let org_id = self.report_org(key);
        self.read(|state| {
            state
                .organizations
                .iter()
                .any(|org| org.id == org_id && !org.suspended)
        })
    }

    pub fn organizations_for(&self, person: &str) -> Vec<Organization> {
        self.read(|state| {
            state
                .organizations
                .iter()
                .filter(|org| {
                    state
                        .memberships
                        .iter()
                        .any(|m| m.person == person && m.org == org.id)
                })
                .cloned()
                .collect()
        })
    }

    pub(crate) fn operator_organizations(&self) -> Vec<Organization> {
        self.read(|state| state.organizations.clone())
    }

    pub fn signups_paused(&self) -> bool {
        self.read(|state| state.signups_paused)
    }

    pub(crate) fn operator_accounts_half_full(&self) -> bool {
        let limit = self
            .signup_account_limit
            .load(std::sync::atomic::Ordering::Relaxed);
        self.read(|state| state.users.len().saturating_mul(2) >= limit)
    }

    pub(crate) fn signup_account_capacity(&self) -> bool {
        let limit = self
            .signup_account_limit
            .load(std::sync::atomic::Ordering::Relaxed);
        self.read(|state| {
            state.tenancy == Tenancy::Hosted && !state.signups_paused && state.users.len() < limit
        })
    }

    pub fn set_org_creation_limit(&self, limit: usize) -> Result<(), String> {
        if limit == 0 || limit > MAX_FAILURE_KEYS {
            return Err(format!(
                "Organization creation limit must be between 1 and {MAX_FAILURE_KEYS}."
            ));
        }
        self.org_creation_limit
            .store(limit, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    pub fn set_signup_account_limit(&self, limit: usize) -> Result<(), String> {
        if limit == 0 || limit > MAX_FAILURE_KEYS {
            return Err(format!(
                "Hosted account limit must be between 1 and {MAX_FAILURE_KEYS}."
            ));
        }
        let _state = self
            .state
            .lock()
            .map_err(|_| "Access state lock poisoned")?;
        self.signup_account_limit
            .store(limit, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    pub(crate) fn operator_set_signups_paused(&self, paused: bool) -> Result<(), AccessError> {
        self.with(|state| {
            state.signups_paused = paused;
            Ok(())
        })
    }

    pub(crate) fn operator_set_suspended(
        &self,
        org: &str,
        suspended: bool,
    ) -> Result<(), AccessError> {
        self.with(|state| {
            let organization = state
                .organizations
                .iter_mut()
                .find(|o| o.id == org)
                .ok_or(AccessError::NotFound)?;
            organization.suspended = suspended;
            Ok(())
        })
    }

    pub(crate) fn operator_disable_person(
        &self,
        person: &str,
        disabled: bool,
    ) -> Result<(), AccessError> {
        self.with(|state| {
            let user = state
                .users
                .iter_mut()
                .find(|u| u.user.id == person)
                .ok_or(AccessError::NotFound)?;
            user.user.disabled = disabled;
            if disabled {
                state.sessions.retain(|s| s.user != person);
            }
            Ok(())
        })
    }

    pub(crate) fn operator_account_deletion(
        &self,
        person: &str,
        successors: &BTreeMap<String, String>,
    ) -> Result<AccountDeletion, AccessError> {
        self.read(|state| {
            let mut next = state.clone();
            deletion_successors(&mut next, person, successors)?;
            account_deletion(&next, person)
        })
    }

    /// Disable access first. A partial deletion stays retryable and cannot expose its reports.
    pub(crate) fn operator_begin_account_deletion(
        &self,
        person: &str,
        successors: &BTreeMap<String, String>,
    ) -> Result<AccountDeletion, AccessError> {
        self.with(|state| {
            deletion_successors(state, person, successors)?;
            let target = account_deletion(state, person)?;
            let user = state
                .users
                .iter_mut()
                .find(|record| record.user.id == person)
                .ok_or(AccessError::NotFound)?;
            user.user.disabled = true;
            user.tokens.clear();
            state.sessions.retain(|session| session.user != person);
            for org in state
                .organizations
                .iter_mut()
                .filter(|org| target.orgs.contains(&org.id))
            {
                org.suspended = true;
            }
            Ok(target)
        })
    }

    pub(crate) fn operator_purge_account_links(&self, email: &str) -> Result<(), String> {
        self.links
            .as_ref()
            .ok_or("Links unavailable")?
            .purge_address(email)
    }

    pub(crate) fn operator_finish_account_deletion(
        &self,
        person: &str,
        target: &AccountDeletion,
        successors: &BTreeMap<String, String>,
    ) -> Result<(), String> {
        let _audit = self.audit_lock.lock().map_err(|_| "Audit lock poisoned")?;
        self.redact_account_audits(&target.email)?;
        self.purge_legacy_backup(person, target)?;
        self.grants
            .lock()
            .map_err(|_| "Ticket lock poisoned")?
            .retain(|_, ticket| ticket.user != person);
        self.report_sessions
            .lock()
            .map_err(|_| "Report session lock poisoned")?
            .retain(|_, session| session.user != person);
        self.failures
            .lock()
            .map_err(|_| "Failure counter poisoned")?
            .known
            .remove(&digest(&target.email.trim().to_lowercase()));
        for org in &target.orgs {
            self.publish_limits
                .lock()
                .map_err(|_| "Publish counter poisoned")?
                .remove(org);
            self.compute_limits
                .lock()
                .map_err(|_| "Compute counter poisoned")?
                .remove(org);
        }
        {
            let mut counters = self
                .invite_limits
                .lock()
                .map_err(|_| "Invitation counter poisoned")?;
            if counters.contains_key(person) {
                let mut next = counters.clone();
                next.remove(person);
                self.save_invite_limits(&next)?;
                *counters = next;
            }
        }
        self.with(|state| {
            let current = account_deletion(state, person)?;
            if current.orgs != target.orgs || !current.email.eq_ignore_ascii_case(&target.email) {
                return Err(AccessError::Invalid(
                    "Account changed during deletion".into(),
                ));
            }
            for (org, successor) in successors {
                if !state.users.iter().any(|user| user.user.id == *successor)
                    || !state.memberships.iter().any(|member| {
                        member.org == *org
                            && member.person == *successor
                            && member.role == OrgRole::Owner
                    })
                {
                    return Err(AccessError::Invalid(
                        "Successor changed during deletion".into(),
                    ));
                }
            }
            // The state lock excludes concurrent state writes while stale private staging files
            // from an interrupted write are inspected and removed.
            for entry in fs::read_dir(&*self.dir)
                .map_err(|_| AccessError::Storage("Cannot inspect access staging".into()))?
            {
                let entry = entry
                    .map_err(|_| AccessError::Storage("Cannot inspect access staging".into()))?;
                let name = entry.file_name().to_string_lossy().into_owned();
                if !(name.starts_with(".state.") && name.ends_with(".tmp")
                    || name.starts_with(".tmp")
                    || name.starts_with("state.v1.partial-"))
                {
                    continue;
                }
                let metadata = fs::symlink_metadata(entry.path())
                    .map_err(|_| AccessError::Storage("Cannot inspect access staging".into()))?;
                if !metadata.is_file() || metadata.len() > 64 * 1024 * 1024 {
                    return Err(AccessError::Storage("Invalid access staging file".into()));
                }
                let bytes = fs::read(entry.path())
                    .map_err(|_| AccessError::Storage("Cannot inspect access staging".into()))?;
                let text = String::from_utf8_lossy(&bytes).to_ascii_lowercase();
                if text.contains(&target.email.to_ascii_lowercase())
                    || text.contains(&person.to_ascii_lowercase())
                {
                    fs::remove_file(entry.path())
                        .map_err(|_| AccessError::Storage("Cannot remove access staging".into()))?;
                }
            }
            let gone = |org: &str| target.orgs.iter().any(|id| id == org);
            // Reports in an organization that stays pass to its longest-standing other owner.
            let heirs: BTreeMap<String, String> = state
                .acls
                .values()
                .filter(|acl| acl.owner == person && !gone(&acl.org))
                .filter_map(|acl| {
                    Some((
                        acl.org.clone(),
                        successors
                            .get(&acl.org)
                            .cloned()
                            .or_else(|| heir(state, &acl.org, person))?,
                    ))
                })
                .collect();
            state.users.retain(|record| record.user.id != person);
            state.org_creations.remove(person);
            state.organizations.retain(|org| !gone(&org.id));
            state
                .memberships
                .retain(|membership| membership.person != person && !gone(&membership.org));
            state.sessions.retain(|session| session.user != person);
            state
                .registrations
                .retain(|registration| !registration.email.eq_ignore_ascii_case(&target.email));
            state.invitations.retain(|invitation| {
                invitation.inviter != person
                    && !invitation.email.eq_ignore_ascii_case(&target.email)
                    && !gone(&invitation.org)
            });
            state.acls.retain(|_, acl| !gone(&acl.org));
            for acl in state.acls.values_mut() {
                acl.grants.retain(|grant| grant.user != person);
                if acl.owner == person {
                    acl.owner = heirs.get(&acl.org).cloned().ok_or(AccessError::Invalid(
                        "Account changed during deletion".into(),
                    ))?;
                    let owner = acl.owner.clone();
                    acl.grants.retain(|grant| grant.user != owner);
                }
            }
            Ok(())
        })
        .map_err(|_| "Could not finish account deletion".to_owned())
    }

    pub(crate) fn operator_pending_links(&self) -> Result<Vec<Value>, String> {
        self.links
            .as_ref()
            .ok_or("Links require hosted mode")?
            .pending()
    }

    pub fn member_of(&self, who: &Principal, org: &str) -> bool {
        match who {
            Principal::User(user) => {
                user.token_org.as_ref().is_none_or(|bound| bound == org)
                    && self.role_in(&user.id, org).is_some()
                    && self.read(|state| {
                        state
                            .organizations
                            .iter()
                            .any(|o| o.id == org && !o.suspended)
                    })
            }
            _ => false,
        }
    }

    /// Whether anyone can authenticate: a service token or at least one person.
    pub fn has_credentials(&self) -> bool {
        self.service_token.is_some() || self.has_users()
    }

    /// Whether any person has an account yet.
    pub fn has_users(&self) -> bool {
        self.state
            .lock()
            .map(|s| !s.users.is_empty())
            .unwrap_or(false)
    }

    fn save(&self, state: &State) -> Result<(), String> {
        let path = self.dir.join("state.json");
        let staging = self.dir.join(format!(".state.{}.tmp", std::process::id()));
        // Password digests and token digests: readable by the server's user only.
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(&staging).map_err(|e| e.to_string())?;
        let mut value = serde_json::to_value(state).map_err(|e| e.to_string())?;
        if let Some(users) = value.get_mut("users").and_then(Value::as_array_mut) {
            for user in users {
                if let Some(object) = user.as_object_mut() {
                    object.remove("role");
                }
            }
        }
        file.write_all(&serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        fs::rename(&staging, &path).map_err(|e| e.to_string())
    }

    fn with<T>(
        &self,
        f: impl FnOnce(&mut State) -> Result<T, AccessError>,
    ) -> Result<T, AccessError> {
        let _gate = self.gate()?;
        self.with_gated(f)
    }

    /// Authenticate after taking the access mutation lock and keep it through the write.
    fn with_caller<T>(
        &self,
        caller: impl FnOnce() -> Principal,
        f: impl FnOnce(&mut State, &Principal) -> Result<T, AccessError>,
    ) -> Result<T, AccessError> {
        let _gate = self.gate()?;
        let who = caller();
        if matches!(who, Principal::Anonymous) {
            return Err(AccessError::Credentials);
        }
        self.with_gated(|state| f(state, &who))
    }

    /// The lock every access change takes. It is free except while a publication commits,
    /// which can write for a while. A caller on an async worker then hands its worker's other
    /// tasks to the runtime before it waits, so requests of other people keep being served;
    /// such waits run on the runtime's blocking threads, which are capped.
    fn gate(&self) -> Result<std::sync::MutexGuard<'_, ()>, AccessError> {
        let poisoned = || AccessError::Storage("Access change lock poisoned".into());
        match self.mutations.try_lock() {
            Ok(gate) => return Ok(gate),
            Err(std::sync::TryLockError::Poisoned(_)) => return Err(poisoned()),
            Err(std::sync::TryLockError::WouldBlock) => {}
        }
        self.waiting
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let wait = || self.mutations.lock();
        let gate = match tokio::runtime::Handle::try_current() {
            Ok(runtime)
                if runtime.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread =>
            {
                tokio::task::block_in_place(wait)
            }
            _ => wait(),
        };
        self.waiting
            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        gate.map_err(|_| poisoned())
    }

    /// How many access changes are waiting for a publication commit to finish.
    #[cfg(all(test, feature = "embedded-runtime"))]
    pub(crate) fn waiting_changes(&self) -> usize {
        self.waiting.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// How many report tickets are issued and not yet used or expired.
    #[cfg(all(test, feature = "embedded-runtime"))]
    pub(crate) fn outstanding_tickets(&self) -> usize {
        self.grants.lock().map_or(0, |tickets| tickets.len())
    }

    /// `with` for a caller that already holds `mutations`.
    fn with_gated<T>(
        &self,
        f: impl FnOnce(&mut State) -> Result<T, AccessError>,
    ) -> Result<T, AccessError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| AccessError::Storage("Access state lock poisoned".into()))?;
        // Change a copy, save it, then keep it: a failed save leaves memory as the disk is.
        let mut next = state.clone();
        // An invitation or share nobody accepted leaves the store, address included, with the
        // first write after it expires.
        let at = now();
        next.invitations.retain(|i| i.expires > at);
        let result = f(&mut next)?;
        self.save(&next)?;
        *state = next;
        Ok(result)
    }

    fn read<T>(&self, f: impl FnOnce(&State) -> T) -> T {
        match self.state.lock() {
            Ok(state) => f(&state),
            Err(poisoned) => f(&poisoned.into_inner()),
        }
    }

    // ---------------------------------------------------------------- who is calling

    /// The caller behind a bearer token or a session cookie value. A disabled person, an expired
    /// session or an unknown credential is anonymous.
    pub fn authenticate(&self, bearer: Option<&str>, session_cookie: Option<&str>) -> Principal {
        if let Some(token) = bearer.filter(|t| !t.is_empty()) {
            if let Some(service) = &self.service_token
                && constant_time_eq(service.as_bytes(), token.as_bytes())
            {
                return Principal::Service;
            }
            let wanted = digest(token);
            return self.read(|state| {
                state
                    .users
                    .iter()
                    .filter(|r| !r.user.disabled)
                    .find_map(|r| {
                        r.tokens
                            .iter()
                            .find(|t| {
                                t.sha256 == wanted
                                    && state
                                        .memberships
                                        .iter()
                                        .any(|m| m.person == r.user.id && m.org == t.org)
                                    && state
                                        .organizations
                                        .iter()
                                        .any(|o| o.id == t.org && !o.suspended)
                            })
                            .map(|t| {
                                let mut user = r.user.clone();
                                user.token_org = Some(t.org.clone());
                                Principal::User(user)
                            })
                    })
                    .unwrap_or(Principal::Anonymous)
            });
        }
        if let Some(cookie) = session_cookie.filter(|c| !c.is_empty()) {
            let wanted = digest(cookie);
            let at = now();
            return self.read(|state| {
                state
                    .sessions
                    .iter()
                    .find(|s| s.sha256 == wanted && s.expires > at && s.scope.is_empty())
                    .and_then(|s| state.users.iter().find(|r| r.user.id == s.user))
                    .filter(|r| !r.user.disabled)
                    .map_or(Principal::Anonymous, |r| Principal::User(r.user.clone()))
            });
        }
        Principal::Anonymous
    }

    /// A one-minute, single-use ticket that carries a signed-in person to the report origin for
    /// one report they may view. It hangs from the person's app session; only a person signed in
    /// by cookie can hold one, and at most a few at a time.
    pub fn issue_grant(
        &self,
        who: &Principal,
        slug: &str,
        session_cookie: Option<&str>,
    ) -> Option<String> {
        let Principal::User(user) = who else {
            return None;
        };
        let parent = digest(session_cookie.filter(|c| !c.is_empty())?);
        if !self.can(who, slug, Right::View) {
            return None;
        }
        let ticket = random_text(24).ok()?;
        let at = now();
        let mut grants = self.grants.lock().ok()?;
        grants.retain(|_, t| t.until > at);
        if grants.values().filter(|t| t.user == user.id).count() >= TICKETS_PER_USER
            || grants.len() >= 4096
        {
            return None;
        }
        grants.insert(
            digest(&ticket),
            Ticket {
                user: user.id.clone(),
                slug: slug.to_owned(),
                parent,
                until: at + 60,
            },
        );
        Some(ticket)
    }

    /// Exchanges a ticket for a read session on the report origin, for the report it was issued
    /// for. The ticket is spent either way.
    pub fn redeem_grant(&self, ticket: &str, slug: &str) -> Option<String> {
        let ticket = self.grants.lock().ok()?.remove(&digest(ticket))?;
        let at = now();
        if ticket.until <= at || ticket.slug != slug {
            return None;
        }
        let cookie = random_text(32).ok()?;
        let mut sessions = self.report_sessions.lock().ok()?;
        sessions.retain(|_, s| s.expires > at);
        let mine: Vec<_> = sessions
            .iter()
            .filter(|(_, s)| s.user == ticket.user)
            .map(|(key, s)| (s.expires, key.clone()))
            .collect();
        if mine.len() >= REPORT_SESSIONS_PER_USER
            && let Some((_, oldest)) = mine.into_iter().min()
        {
            sessions.remove(&oldest);
        }
        sessions.insert(
            digest(&cookie),
            ReportSession {
                user: ticket.user,
                slug: slug.to_owned(),
                parent: ticket.parent,
                expires: at + REPORT_SESSION_SECONDS,
            },
        );
        Some(cookie)
    }

    /// The person behind a report-origin cookie, for that report only, while the app session it
    /// came from is still signed in.
    pub fn authenticate_report(&self, cookie: &str, slug: &str) -> Principal {
        let at = now();
        let found = self.report_sessions.lock().ok().and_then(|sessions| {
            sessions
                .get(&digest(cookie))
                .filter(|s| s.expires > at && s.slug == slug)
                .map(|s| (s.user.clone(), s.parent.clone()))
        });
        let Some((user, parent)) = found else {
            return Principal::Anonymous;
        };
        self.read(|state| {
            state
                .sessions
                .iter()
                .find(|s| s.sha256 == parent && s.expires > at && s.scope.is_empty())
                .and_then(|_| state.users.iter().find(|r| r.user.id == user))
                .filter(|r| !r.user.disabled)
                .map_or(Principal::Anonymous, |r| Principal::User(r.user.clone()))
        })
    }

    /// The CSRF token bound to a session cookie, for state-changing browser requests.
    pub fn csrf_for(&self, session_cookie: &str) -> Option<String> {
        let wanted = digest(session_cookie);
        self.read(|state| {
            state
                .sessions
                .iter()
                .find(|s| s.sha256 == wanted && s.scope.is_empty())
                .map(|s| s.csrf.clone())
        })
    }

    fn check_sign_in_fields(email: &str, password: &str) -> Result<(), AccessError> {
        if email.len() > MAX_EMAIL_BYTES || password.len() > MAX_PASSWORD_BYTES {
            return Err(AccessError::Credentials);
        }
        Ok(())
    }

    /// Admit only a bounded number of blocking sign-ins; the permit stays with the worker.
    pub(crate) fn admit_sign_in(
        &self,
        email: &str,
        password: &str,
    ) -> Result<tokio::sync::OwnedSemaphorePermit, AccessError> {
        Self::check_sign_in_fields(email, password)?;
        self.sign_in_workers
            .clone()
            .try_acquire_owned()
            .map_err(|_| AccessError::Throttled)
    }

    /// Signs a person in. Returns the cookie value and its CSRF token. Wrong credentials, an
    /// unknown address and a disabled account all answer `Credentials`; repeated failures lock
    /// the address out for a while.
    pub fn sign_in(&self, email: &str, password: &str) -> Result<(String, String), AccessError> {
        Self::check_sign_in_fields(email, password)?;
        let key = email.trim().to_lowercase();
        let failure_key = digest(&key);
        let at = now();
        let record = self.read(|state| {
            state
                .users
                .iter()
                .find(|r| r.user.email.to_lowercase() == key)
                .cloned()
        });
        // The attempt is counted before the password is checked, so parallel guesses cannot all
        // pass the limit. Unknown addresses share a counter and cannot occupy known-user slots.
        let blocked = {
            let mut failures = self
                .failures
                .lock()
                .map_err(|_| AccessError::Storage("Failure counter poisoned".into()))?;
            let entry = if record.is_some() {
                if !failures.known.contains_key(&failure_key)
                    && failures.known.len() >= MAX_FAILURE_KEYS
                {
                    failures.known.retain(|_, (_, until)| *until > at);
                }
                if !failures.known.contains_key(&failure_key)
                    && failures.known.len() >= MAX_FAILURE_KEYS
                {
                    None
                } else {
                    Some(failures.known.entry(failure_key.clone()).or_insert((0, 0)))
                }
            } else {
                Some(&mut failures.unknown)
            };
            if let Some(entry) = entry {
                if entry.1 <= at {
                    *entry = (0, 0);
                }
                if entry.0 >= MAX_FAILURES {
                    true
                } else {
                    entry.0 += 1;
                    entry.1 = at + LOCKOUT_SECONDS;
                    false
                }
            } else {
                true
            }
        };
        if blocked && !self.is_hosted() {
            return Err(AccessError::Throttled);
        }
        // A flood of attempts waits a few seconds at most, then is refused as throttled, so it
        // cannot park every worker thread behind the lane.
        let patience = self.lane_patience;
        let waited = std::time::Instant::now();
        let _hashing = loop {
            match HASHING.try_lock() {
                Ok(guard) => break guard,
                Err(std::sync::TryLockError::Poisoned(e)) => break e.into_inner(),
                Err(std::sync::TryLockError::WouldBlock) => {
                    if waited.elapsed() > patience {
                        return Err(AccessError::Throttled);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
            }
        };
        // Unknown and disabled accounts cost the same hashing time as wrong passwords.
        let matched = match &record {
            _ if blocked => {
                // The aggregate unknown bucket and a known account's lockout must cost the same
                // bounded work and produce the same public response as a wrong password.
                let _ = hash_password(password, &[0u8; 16]);
                false
            }
            Some(UserRecord {
                user,
                password: Some(hash),
                ..
            }) if !user.disabled => password_matches(hash, password),
            _ => {
                let _ = hash_password(password, &[0u8; 16]);
                false
            }
        };
        drop(_hashing);
        let mut failures = self
            .failures
            .lock()
            .map_err(|_| AccessError::Storage("Failure counter poisoned".into()))?;
        if !matched {
            return Err(AccessError::Credentials);
        }
        failures.known.remove(&failure_key);
        drop(failures);
        let verified = record.ok_or(AccessError::Credentials)?;
        let verified_hash = verified.password.clone();
        let user = verified.user.id;
        let cookie = random_text(32)?;
        let csrf = random_text(24)?;
        let session = Session {
            sha256: digest(&cookie),
            csrf: csrf.clone(),
            user,
            expires: at + SESSION_SECONDS,
            scope: String::new(),
            org: None,
        };
        self.with(|state| {
            // A reset or disable can commit while hashing waits on the shared lane. Never create
            // a session from a credential snapshot that is no longer current.
            if !state.users.iter().any(|current| {
                current.user.id == session.user
                    && !current.user.disabled
                    && current.password == verified_hash
            }) {
                return Err(AccessError::Credentials);
            }
            state.sessions.retain(|s| s.expires > at);
            let mut count = state
                .sessions
                .iter()
                .filter(|s| s.user == session.user)
                .count();
            state.sessions.retain(|s| {
                if s.user == session.user && count >= 10 {
                    count -= 1;
                    false
                } else {
                    true
                }
            });
            state.sessions.push(session);
            Ok(())
        })?;
        Ok((cookie, csrf))
    }

    pub fn sign_out(
        &self,
        caller: impl FnOnce() -> Principal,
        session_cookie: &str,
    ) -> Result<(), AccessError> {
        let wanted = digest(session_cookie);
        self.with_caller(caller, |state, who| {
            let Principal::User(user) = who else {
                return Err(AccessError::Credentials);
            };
            if !state
                .sessions
                .iter()
                .any(|s| s.sha256 == wanted && s.user == user.id)
            {
                return Err(AccessError::Credentials);
            }
            state.sessions.retain(|s| s.sha256 != wanted);
            Ok(())
        })?;
        if let Ok(mut sessions) = self.report_sessions.lock() {
            sessions.retain(|_, s| s.parent != wanted);
        }
        Ok(())
    }

    /// End a fixture session by its cookie without an HTTP request.
    pub fn sign_out_for_fixture(&self, session_cookie: &str) -> Result<(), AccessError> {
        self.sign_out(
            || self.authenticate(None, Some(session_cookie)),
            session_cookie,
        )
    }

    /// Reserve a short-lived email verification. Existing accounts and recent requests return
    /// the same public result; only a token digest is persisted.
    pub(crate) fn reserve_registration(
        &self,
        email: &str,
    ) -> Result<Option<(String, String)>, AccessError> {
        let email = crate::email::normalize(email)
            .ok_or_else(|| AccessError::Invalid("That is not an email address.".into()))?;
        let token = random_text(32)?;
        let at = now();
        self.with(|state| {
            state
                .registration_requests
                .retain(|&time| time > at.saturating_sub(3600));
            state
                .registrations
                .retain(|pending| pending.created > at.saturating_sub(3600));
            if state.registration_requests.len() >= 60 {
                return Err(AccessError::Throttled);
            }
            state.registration_requests.push(at);
            if state
                .users
                .iter()
                .any(|u| u.user.email.eq_ignore_ascii_case(&email))
                || state
                    .registrations
                    .iter()
                    .any(|p| p.email == email && p.created > at.saturating_sub(300))
            {
                return Ok(None);
            }
            // Keep the total below the reserved known-account failure-counter slots, so a
            // flood of registered accounts cannot crowd a legitimate account out of sign-in.
            if state.registrations.len() >= 1000 || state.users.len() >= 1000 {
                return Err(AccessError::Throttled);
            }
            state.registrations.retain(|p| p.email != email);
            state.registrations.push(PendingRegistration {
                email: email.clone(),
                token_sha256: digest(&token),
                created: at,
            });
            Ok(Some((email, token)))
        })
    }

    pub(crate) fn cancel_registration(&self, token: &str) -> Result<(), AccessError> {
        let wanted = digest(token);
        self.with(|state| {
            state.registrations.retain(|p| p.token_sha256 != wanted);
            Ok(())
        })
    }

    /// Check a verified email token before admitting bounded password hashing. Consumption and
    /// account creation are one state write; a duplicate or expired token never creates a user.
    pub(crate) async fn complete_registration(
        &self,
        token: &str,
        name: &str,
        password: &str,
    ) -> Result<User, AccessError> {
        if token.len() != 43
            || !token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || name.trim().is_empty()
            || name.trim().chars().count() > 120
            || password.chars().count() < MIN_PASSWORD
            || password.len() > MAX_PASSWORD_BYTES
        {
            return Err(AccessError::Invalid(
                "Check the name, password and verification link.".into(),
            ));
        }
        let wanted = digest(token);
        let at = now();
        if !self.read(|state| {
            state
                .registrations
                .iter()
                .any(|p| p.token_sha256 == wanted && p.created > at.saturating_sub(3600))
        }) {
            return Err(AccessError::Credentials);
        }
        let permit = self
            .registration_workers
            .clone()
            .try_acquire_owned()
            .map_err(|_| AccessError::Throttled)?;
        let access = self.clone();
        let name = name.trim().to_owned();
        let password = password.to_owned();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let started = std::time::Instant::now();
            let _lane = loop {
                match HASHING.try_lock() {
                    Ok(lane) => break lane,
                    Err(std::sync::TryLockError::Poisoned(error)) => break error.into_inner(),
                    Err(std::sync::TryLockError::WouldBlock)
                        if started.elapsed() < std::time::Duration::from_secs(3) =>
                    {
                        std::thread::sleep(std::time::Duration::from_millis(20));
                    }
                    Err(std::sync::TryLockError::WouldBlock) => return Err(AccessError::Throttled),
                }
            };
            let hash = new_password_hash(&password)?;
            let id = format!(
                "u-{}",
                random_text(9)?.to_lowercase().replace(['_', '-'], "x")
            );
            access.with(|state| {
                let position = state
                    .registrations
                    .iter()
                    .position(|p| {
                        p.token_sha256 == wanted && p.created > now().saturating_sub(3600)
                    })
                    .ok_or(AccessError::Credentials)?;
                let pending = &state.registrations[position];
                if state
                    .users
                    .iter()
                    .any(|u| u.user.email.eq_ignore_ascii_case(&pending.email))
                {
                    return Err(AccessError::Credentials);
                }
                let user = User {
                    id,
                    name,
                    email: pending.email.clone(),
                    role: OrgRole::Author,
                    disabled: false,
                    created: stamp(),
                    verified: Some(stamp()),
                    operator: false,
                    token_org: None,
                };
                state.registrations.remove(position);
                state.memberships.push(Membership {
                    person: user.id.clone(),
                    org: default_org(),
                    role: user.role,
                    created: user.created.clone(),
                });
                state.users.push(UserRecord {
                    user: user.clone(),
                    password: Some(hash),
                    tokens: vec![],
                });
                Ok(user)
            })
        })
        .await
        .map_err(|_| AccessError::Storage("Registration worker failed".into()))?
    }

    // ---------------------------------------------------------------- rights

    /// Whether the report has an access record of its own.
    pub fn has_record(&self, slug: &str) -> bool {
        self.read(|state| state.acls.contains_key(slug))
    }

    /// The report's access record, or the default one for a report that has none.
    pub fn acl(&self, slug: &str) -> ReportAcl {
        let org = self.report_org(slug);
        self.read(|state| {
            state.acls.get(slug).cloned().unwrap_or(ReportAcl {
                org,
                owner: "service".into(),
                visibility: self.default_visibility,
                grants: vec![],
            })
        })
    }

    /// The stored report addresses charged to this owner.
    pub fn owned_slugs(&self, owner: &str) -> Vec<String> {
        self.read(|state| {
            state
                .acls
                .iter()
                .filter(|(_, acl)| acl.owner == owner)
                .map(|(slug, _)| slug.clone())
                .collect()
        })
    }

    pub fn memberships_for(&self, person: &str) -> Vec<Membership> {
        self.read(|state| {
            state
                .memberships
                .iter()
                .filter(|m| m.person == person)
                .cloned()
                .collect()
        })
    }

    pub fn role_in(&self, person: &str, org: &str) -> Option<OrgRole> {
        self.read(|state| {
            state
                .memberships
                .iter()
                .find(|m| m.person == person && m.org == org)
                .map(|m| m.role)
        })
    }

    /// What `who` may do with the report `slug`.
    pub fn rights(&self, who: &Principal, slug: &str) -> BTreeSet<Right> {
        let all = || {
            [
                Right::View,
                Right::Edit,
                Right::Manage,
                Right::Share,
                Right::Delete,
            ]
            .into_iter()
            .collect()
        };
        let acl = self.acl(slug);
        if self.read(|state| {
            !state
                .organizations
                .iter()
                .any(|org| org.id == acl.org && !org.suspended)
        }) {
            return BTreeSet::new();
        }
        let user = match who {
            Principal::Service => return all(),
            Principal::User(user) if user.token_org.as_ref().is_none_or(|org| org == &acl.org) => {
                Some(user)
            }
            Principal::Anonymous => None,
            Principal::User(_) => None,
        };
        if user.is_some_and(|user| {
            matches!(
                self.role_in(&user.id, &acl.org),
                Some(OrgRole::Owner | OrgRole::Admin)
            )
        }) {
            return all();
        }
        let mut rights = BTreeSet::new();
        match acl.visibility {
            Visibility::Public => {
                rights.insert(Right::View);
            }
            Visibility::Org
                if user.is_some_and(|user| self.role_in(&user.id, &acl.org).is_some()) =>
            {
                rights.insert(Right::View);
            }
            _ => {}
        }
        if let Some(user) = user {
            if acl.owner == user.id && self.role_in(&user.id, &acl.org).is_some() {
                rights.extend(ReportRole::Owner.rights());
            }
            for grant in acl.grants.iter().filter(|g| g.user == user.id) {
                rights.extend(grant.role.rights());
            }
        }
        rights
    }

    pub fn can(&self, who: &Principal, slug: &str, right: Right) -> bool {
        self.rights(who, slug).contains(&right)
    }

    /// Whether `who` may publish a report address the server has not stored before.
    /// Whether `who` owns the report, holds a grant on it or administers the server: the people a
    /// link-shared report is listed to.
    pub fn named_on(&self, who: &Principal, slug: &str) -> bool {
        match who {
            Principal::Service => true,
            Principal::Anonymous => false,
            Principal::User(user) => {
                let acl = self.acl(slug);
                if user.token_org.as_ref().is_some_and(|org| org != &acl.org) {
                    return false;
                }
                matches!(
                    self.role_in(&user.id, &acl.org),
                    Some(OrgRole::Owner | OrgRole::Admin)
                ) || (acl.owner == user.id && self.role_in(&user.id, &acl.org).is_some())
                    || acl.grants.iter().any(|g| g.user == user.id)
            }
        }
    }

    /// Whether `who` could publish to some address: they may create reports, or they hold an
    /// editor grant on one. Asked before a request body is read, so a refusal comes first.
    pub fn may_publish_anywhere(&self, who: &Principal) -> bool {
        if self.can_create(who) {
            return true;
        }
        let Principal::User(user) = who else {
            return false;
        };
        self.read(|state| {
            state.acls.values().any(|acl| {
                acl.grants
                    .iter()
                    .any(|g| g.user == user.id && g.role == ReportRole::Editor)
            })
        })
    }

    pub fn can_create(&self, who: &Principal) -> bool {
        match who {
            Principal::Service => true,
            Principal::User(user) => self.read(|state| {
                state.memberships.iter().any(|m| {
                    m.person == user.id
                        && user.token_org.as_ref().is_none_or(|org| org == &m.org)
                        && matches!(m.role, OrgRole::Owner | OrgRole::Admin | OrgRole::Author)
                        && state
                            .organizations
                            .iter()
                            .any(|o| o.id == m.org && !o.suspended)
                })
            }),
            Principal::Anonymous => false,
        }
    }

    /// Whether `who` is an admin or the service.
    pub fn is_admin(&self, who: &Principal) -> bool {
        if self.is_hosted() {
            return false;
        }
        match who {
            Principal::Service => true,
            Principal::User(user) => matches!(
                self.role_in(&user.id, user.token_org.as_deref().unwrap_or("default")),
                Some(OrgRole::Owner | OrgRole::Admin)
            ),
            Principal::Anonymous => false,
        }
    }

    /// Refuse a new hosted address before writing its files if its ACL history is full.
    pub fn check_claim_capacity(&self, slug: &str) -> Result<(), AccessError> {
        let org = self.report_org(slug);
        self.read(|state| hosted_claim_capacity(state, &org, slug))
    }

    /// Records the owner of a report the first time it is published.
    pub fn claim(&self, caller: impl FnOnce() -> Principal, slug: &str) -> Result<(), AccessError> {
        let visibility = self.default_visibility;
        let org = self.report_org(slug);
        self.with_caller(caller, |state, who| {
            let owner = match who {
                Principal::User(user) => user.id.clone(),
                _ => "service".to_owned(),
            };
            if state.tenancy == Tenancy::Hosted
                && !matches!(who, Principal::User(user)
                    if user.token_org.as_ref().is_none_or(|bound| bound == &org)
                        && state.memberships.iter().any(|m| m.person == user.id && m.org == org
                            && matches!(m.role, OrgRole::Owner | OrgRole::Admin | OrgRole::Author)))
            {
                return Err(AccessError::Credentials);
            }
            hosted_claim_capacity(state, &org, slug)?;
            state.acls.entry(slug.to_owned()).or_insert(ReportAcl {
                org,
                owner,
                visibility,
                grants: vec![],
            });
            Ok(())
        })
    }

    /// Replaces who can see a report and who holds a grant on it. The owner is kept.
    pub fn set_acl_for_fixture(
        &self,
        slug: &str,
        visibility: Visibility,
        grants: Vec<Grant>,
    ) -> Result<ReportAcl, AccessError> {
        self.set_acl_inner(|| None, slug, visibility, grants)
    }

    /// Recheck a hosted caller and organization in the same transaction as the sharing change.
    pub fn set_acl_authorized(
        &self,
        caller: impl FnOnce() -> Principal,
        slug: &str,
        visibility: Visibility,
        grants: Vec<Grant>,
    ) -> Result<ReportAcl, AccessError> {
        self.set_acl_inner(|| Some(caller()), slug, visibility, grants)
    }

    /// Change visibility without overwriting grants changed by another request.
    pub fn set_visibility_authorized(
        &self,
        caller: impl FnOnce() -> Principal,
        slug: &str,
        visibility: Visibility,
    ) -> Result<ReportAcl, AccessError> {
        #[cfg(test)]
        crate::test_points::reach("visibility.gating", slug);
        if !self.is_hosted() {
            return self.set_acl_authorized(caller, slug, visibility, self.acl(slug).grants);
        }
        self.with_caller(caller, |state, who| {
            check_hosted_share(state, slug, Some(who))?;
            let acl = state.acls.get_mut(slug).ok_or(AccessError::NotFound)?;
            acl.visibility = visibility;
            Ok(acl.clone())
        })
    }

    /// Counts publication commits: odd while one is under way. A reader compares the value
    /// before its rights check with the value after it has its bytes; if they differ, or the
    /// first was odd, a publication changed the report in between and the read is repeated.
    pub fn commit_epoch(&self) -> u64 {
        self.commits.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// The access half and the storage half of a publication as one step. Under the lock every
    /// access change takes, it reads the publisher's rights again (rendering took time), puts
    /// the requested visibility or the new report's ownership in force, and only then runs
    /// `store`. No membership, role, grant or visibility change can fall between those, so
    /// nobody the visibility excludes can be let in to the new version. When `store` fails,
    /// the report's access record goes back to exactly what it was, and nothing was stored.
    pub(crate) fn commit_publication<T, E>(
        &self,
        caller: impl FnOnce() -> Principal,
        slug: &str,
        known: bool,
        wanted: Option<Visibility>,
        store: impl FnOnce() -> Result<T, E>,
    ) -> Result<Result<T, E>, AccessError> {
        let _gate = self.gate()?;
        let _window = CommitWindow::open(&self.commits);
        // Who is publishing is read here, from the request itself: a token revoked or an
        // account disabled while the report rendered is no longer the publisher, and neither
        // can change again until the commit is over.
        let who = &caller();
        if matches!(who, Principal::Anonymous) {
            return Err(AccessError::Credentials);
        }
        let before = self.read(|state| state.acls.get(slug).cloned());
        if known {
            if !self.can(who, slug, Right::Edit) {
                return Err(AccessError::NotFound);
            }
        } else {
            let org = self.report_org(slug);
            if self.is_hosted()
                && !matches!(who, Principal::User(user)
                    if user.token_org.as_ref().is_none_or(|bound| bound == &org)
                        && matches!(self.role_in(&user.id, &org), Some(OrgRole::Owner | OrgRole::Admin | OrgRole::Author)))
            {
                return Err(AccessError::Credentials);
            }
            let owner = match who {
                Principal::User(user) => user.id.clone(),
                _ => "service".to_owned(),
            };
            let visibility = self.default_visibility;
            // A retired address keeps its record, and with it its owner.
            self.with_gated(|state| {
                hosted_claim_capacity(state, &org, slug)?;
                state.acls.entry(slug.to_owned()).or_insert(ReportAcl {
                    org,
                    owner,
                    visibility,
                    grants: vec![],
                });
                Ok(())
            })?;
        }
        let restore = |access: &Self| {
            if let Err(error) = access.with_gated(|state| {
                match &before {
                    Some(acl) => state.acls.insert(slug.to_owned(), acl.clone()),
                    None => state.acls.remove(slug),
                };
                Ok(())
            }) {
                eprintln!(
                    "richview: could not restore access for report {slug}: {error:?}; inspect access state before retrying"
                );
            }
        };
        if let Some(wanted) = wanted
            && wanted != self.acl(slug).visibility
        {
            let changed = if self.is_hosted() {
                self.with_gated(|state| {
                    check_hosted_share(state, slug, Some(who))?;
                    state
                        .acls
                        .get_mut(slug)
                        .ok_or(AccessError::NotFound)?
                        .visibility = wanted;
                    Ok(())
                })
            } else if known && !self.can(who, slug, Right::Share) {
                Err(AccessError::NotFound)
            } else {
                let org = self.report_org(slug);
                self.with_gated(|state| {
                    state
                        .acls
                        .entry(slug.to_owned())
                        .or_insert(ReportAcl {
                            org,
                            owner: "service".to_owned(),
                            visibility: wanted,
                            grants: vec![],
                        })
                        .visibility = wanted;
                    Ok(())
                })
            };
            if let Err(error) = changed {
                restore(self);
                return Err(error);
            }
        }
        #[cfg(test)]
        crate::test_points::reach("publish.closed", slug);
        let stored = store();
        if stored.is_err() {
            restore(self);
        }
        Ok(stored)
    }

    /// Remove every invitation and share past its expiry from the store, addresses included.
    /// Runs when a hosted server starts and is the one call a periodic sweep makes; every other
    /// write does the same on its way. Returns how many were removed.
    pub fn purge_expired_invitations(&self) -> Result<usize, AccessError> {
        // Counted under the lock every change takes, so a write that removed them a moment
        // ago is seen, and nothing is saved when nothing is left to remove.
        let _gate = self.gate()?;
        let at = now();
        let expired =
            self.read(|state| state.invitations.iter().filter(|i| i.expires <= at).count());
        if expired > 0 {
            // `with_gated` drops them before it saves.
            self.with_gated(|_| Ok(()))?;
        }
        Ok(expired)
    }

    fn set_acl_inner(
        &self,
        caller: impl FnOnce() -> Option<Principal>,
        slug: &str,
        visibility: Visibility,
        grants: Vec<Grant>,
    ) -> Result<ReportAcl, AccessError> {
        let org = self.report_org(slug);
        let _gate = self.gate()?;
        let caller = caller();
        if matches!(caller, Some(Principal::Anonymous)) {
            return Err(AccessError::Credentials);
        }
        self.with_gated(|state| {
            check_hosted_share(state, slug, caller.as_ref())?;
            let grants = if state.tenancy == Tenancy::Hosted {
                let owner = state
                    .acls
                    .get(slug)
                    .map(|acl| acl.owner.as_str())
                    .unwrap_or("service");
                hosted_grants(grants, owner)?
            } else {
                grants
            };
            hosted_grant_capacity(state, &org, slug, grants.len(), pending_shares(state, slug))?;
            for grant in &grants {
                if !state.users.iter().any(|r| r.user.id == grant.user) {
                    return Err(AccessError::Invalid(format!(
                        "No person has the id {}.",
                        grant.user
                    )));
                }
            }
            let default_owner = "service".to_owned();
            let acl = state.acls.entry(slug.to_owned()).or_insert(ReportAcl {
                org,
                owner: default_owner,
                visibility,
                grants: vec![],
            });
            acl.visibility = visibility;
            acl.grants = grants.into_iter().filter(|g| g.user != acl.owner).collect();
            Ok(acl.clone())
        })
    }

    /// Hands the report to `owner` (when given) and replaces its visibility and grants in one
    /// step, so a refusal or a failed save leaves nothing half changed.
    pub fn set_access_for_fixture(
        &self,
        slug: &str,
        owner: Option<&str>,
        visibility: Visibility,
        grants: Vec<Grant>,
    ) -> Result<ReportAcl, AccessError> {
        self.set_access_inner(|| None, slug, owner, visibility, grants, &[])
    }

    /// Replace report access only if the caller still has the hosted share right. Every open
    /// share of the report ends.
    pub fn set_access_authorized(
        &self,
        caller: impl FnOnce() -> Principal,
        slug: &str,
        owner: Option<&str>,
        visibility: Visibility,
        grants: Vec<Grant>,
    ) -> Result<ReportAcl, AccessError> {
        self.replace_access_authorized(caller, slug, owner, visibility, grants, &[])
    }

    /// `set_access_authorized` that keeps the open shares addressed to `pending`; every other
    /// open share of the report ends.
    pub fn replace_access_authorized(
        &self,
        caller: impl FnOnce() -> Principal,
        slug: &str,
        owner: Option<&str>,
        visibility: Visibility,
        grants: Vec<Grant>,
        pending: &[String],
    ) -> Result<ReportAcl, AccessError> {
        #[cfg(test)]
        crate::test_points::reach("replace.gating", slug);
        let pending: Vec<String> = pending
            .iter()
            .filter_map(|email| crate::email::normalize(email))
            .collect();
        self.set_access_inner(|| Some(caller()), slug, owner, visibility, grants, &pending)
    }

    fn set_access_inner(
        &self,
        caller: impl FnOnce() -> Option<Principal>,
        slug: &str,
        owner: Option<&str>,
        visibility: Visibility,
        grants: Vec<Grant>,
        pending: &[String],
    ) -> Result<ReportAcl, AccessError> {
        let org = self.report_org(slug);
        let _gate = self.gate()?;
        let caller = caller();
        if matches!(caller, Some(Principal::Anonymous)) {
            return Err(AccessError::Credentials);
        }
        self.with_gated(|state| {
            let caller = caller.as_ref();
            check_hosted_share(state, slug, caller)?;
            if state.tenancy == Tenancy::Hosted
                && owner.is_some()
                && let Some(caller) = caller
            {
                let Principal::User(user) = caller else {
                    return Err(AccessError::NotFound);
                };
                let acl = state.acls.get(slug).ok_or(AccessError::NotFound)?;
                let org_admin = state.memberships.iter().any(|m| {
                    m.person == user.id
                        && m.org == acl.org
                        && matches!(m.role, OrgRole::Owner | OrgRole::Admin)
                });
                if acl.owner != user.id && !org_admin {
                    return Err(AccessError::NotFound);
                }
            }
            let grants = if state.tenancy == Tenancy::Hosted {
                let final_owner = owner
                    .or_else(|| state.acls.get(slug).map(|acl| acl.owner.as_str()))
                    .unwrap_or("service");
                hosted_grants(grants, final_owner)?
            } else {
                grants
            };
            // A full replacement: a share still waiting for its address stays only when the
            // caller names that address, and never with more than it offered.
            let at = now();
            let kept = |i: &teams::Invitation| {
                i.report.as_deref() != Some(slug) || (i.expires > at && pending.contains(&i.email))
            };
            let waiting = state
                .invitations
                .iter()
                .filter(|i| i.report.as_deref() == Some(slug) && kept(i))
                .count();
            hosted_grant_capacity(state, &org, slug, grants.len(), waiting)?;
            if state.tenancy == Tenancy::Hosted {
                state.invitations.retain(kept);
            }
            let known = |id: &str| state.users.iter().any(|r| r.user.id == id);
            if let Some(owner) = owner
                && !known(owner)
            {
                return Err(AccessError::Invalid(format!(
                    "No person has the id {owner}."
                )));
            }
            if state.tenancy == Tenancy::Hosted
                && owner.is_some_and(|owner| {
                    !state
                        .memberships
                        .iter()
                        .any(|m| m.person == owner && m.org == org)
                })
            {
                return Err(AccessError::Invalid(
                    "The owner must belong to this organization.".into(),
                ));
            }
            if let Some(grant) = grants.iter().find(|g| !known(&g.user)) {
                return Err(AccessError::Invalid(format!(
                    "No person has the id {}.",
                    grant.user
                )));
            }
            let acl = state.acls.entry(slug.to_owned()).or_insert(ReportAcl {
                org,
                owner: owner.unwrap_or("service").to_owned(),
                visibility,
                grants: vec![],
            });
            if let Some(owner) = owner {
                acl.owner = owner.to_owned();
            }
            acl.visibility = visibility;
            acl.grants = grants.into_iter().filter(|g| g.user != acl.owner).collect();
            Ok(acl.clone())
        })
    }

    // ---------------------------------------------------------------- governance

    pub fn users(&self) -> Vec<User> {
        self.read(|state| state.users.iter().map(|r| r.user.clone()).collect())
    }

    pub fn user(&self, id: &str) -> Option<User> {
        self.read(|state| {
            state
                .users
                .iter()
                .find(|r| r.user.id == id)
                .map(|r| r.user.clone())
        })
    }

    pub fn create_user(
        &self,
        name: &str,
        email: &str,
        role: OrgRole,
        password: Option<&str>,
    ) -> Result<User, AccessError> {
        if self.is_hosted() {
            return Err(AccessError::Invalid(
                "Hosted accounts must be created through verified registration.".into(),
            ));
        }
        let (name, email) = (name.trim(), email.trim());
        if name.is_empty() || name.chars().count() > 120 {
            return Err(AccessError::Invalid(
                "A name has 1 to 120 characters.".into(),
            ));
        }
        if !email.contains('@')
            || email.len() > MAX_EMAIL_BYTES
            || email.chars().any(char::is_whitespace)
        {
            return Err(AccessError::Invalid("That is not an email address.".into()));
        }
        let password = password.map(new_password_hash).transpose()?;
        let created_at = stamp();
        let user = User {
            id: format!(
                "u-{}",
                random_text(9)?.to_lowercase().replace(['_', '-'], "x")
            ),
            name: name.to_owned(),
            email: email.to_owned(),
            role,
            disabled: false,
            created: created_at.clone(),
            verified: Some(created_at),
            operator: false,
            token_org: None,
        };
        let created = user.clone();
        self.with(|state| {
            if state
                .users
                .iter()
                .any(|r| r.user.email.to_lowercase() == email.to_lowercase())
            {
                return Err(AccessError::Invalid(
                    "Someone already has that email.".into(),
                ));
            }
            state.memberships.push(Membership {
                person: user.id.clone(),
                org: default_org(),
                role: user.role,
                created: user.created.clone(),
            });
            state.users.push(UserRecord {
                user,
                password,
                tokens: vec![],
            });
            Ok(())
        })?;
        Ok(created)
    }

    /// Changes a person's role, name or standing. Disabling signs them out and ends their tokens'
    /// effect; the last admin cannot be demoted or disabled.
    pub fn update_user(
        &self,
        id: &str,
        name: Option<&str>,
        role: Option<OrgRole>,
        disabled: Option<bool>,
    ) -> Result<User, AccessError> {
        if self.is_hosted() {
            return Err(AccessError::Invalid(
                "Server-wide user management is unavailable in hosted mode.".into(),
            ));
        }
        self.with(|state| {
            let admins = state
                .users
                .iter()
                .filter(|r| {
                    matches!(r.user.role, OrgRole::Owner | OrgRole::Admin) && !r.user.disabled
                })
                .count();
            let record = state
                .users
                .iter_mut()
                .find(|r| r.user.id == id)
                .ok_or(AccessError::NotFound)?;
            let loses_admin = matches!(record.user.role, OrgRole::Owner | OrgRole::Admin)
                && !record.user.disabled
                && (role.is_some_and(|r| !matches!(r, OrgRole::Owner | OrgRole::Admin))
                    || disabled == Some(true));
            if loses_admin && admins <= 1 {
                return Err(AccessError::Invalid(
                    "The last admin cannot be demoted or disabled.".into(),
                ));
            }
            if let Some(name) = name.map(str::trim).filter(|n| !n.is_empty()) {
                record.user.name = name.to_owned();
            }
            if let Some(role) = role {
                record.user.role = role;
                if let Some(member) = state
                    .memberships
                    .iter_mut()
                    .find(|m| m.person == id && m.org == "default")
                {
                    member.role = role;
                }
            }
            if let Some(disabled) = disabled {
                record.user.disabled = disabled;
            }
            let updated = record.user.clone();
            if updated.disabled {
                state.sessions.retain(|s| s.user != id);
            }
            Ok(updated)
        })
    }

    pub fn set_password(&self, id: &str, password: &str) -> Result<(), AccessError> {
        let hash = new_password_hash(password)?;
        self.with(|state| {
            let record = state
                .users
                .iter_mut()
                .find(|r| r.user.id == id)
                .ok_or(AccessError::NotFound)?;
            record.password = Some(hash);
            // A new password ends every sign-in made with the old one.
            state.sessions.retain(|s| s.user != id);
            Ok(())
        })
    }

    /// Removes a person, their sign-ins and tokens; reports they own go to `service`.
    pub fn delete_user(&self, id: &str) -> Result<(), AccessError> {
        if self.is_hosted() {
            return Err(AccessError::Invalid(
                "Server-wide user management is unavailable in hosted mode.".into(),
            ));
        }
        self.with(|state| {
            let admins = state
                .users
                .iter()
                .filter(|r| {
                    matches!(r.user.role, OrgRole::Owner | OrgRole::Admin) && !r.user.disabled
                })
                .count();
            let record = state
                .users
                .iter()
                .find(|r| r.user.id == id)
                .ok_or(AccessError::NotFound)?;
            if matches!(record.user.role, OrgRole::Owner | OrgRole::Admin)
                && !record.user.disabled
                && admins <= 1
            {
                return Err(AccessError::Invalid(
                    "The last admin cannot be removed.".into(),
                ));
            }
            state.users.retain(|r| r.user.id != id);
            state.memberships.retain(|m| m.person != id);
            state.sessions.retain(|s| s.user != id);
            for acl in state.acls.values_mut() {
                if acl.owner == id {
                    acl.owner = "service".into();
                }
                acl.grants.retain(|g| g.user != id);
            }
            Ok(())
        })
    }

    /// Issues an API token for a person; it is shown once, here, and stored only as a digest.
    pub fn create_token(&self, id: &str, name: &str) -> Result<(String, String), AccessError> {
        if self.is_hosted() {
            return Err(AccessError::Invalid(
                "Hosted tokens must name an organization.".into(),
            ));
        }
        self.issue_token(|| None, id, "default", name, None)
    }

    /// Issues a person's own token, keeping self-service token storage bounded.
    pub fn create_self_token(
        &self,
        caller: impl FnOnce() -> Principal,
        id: &str,
        org: &str,
        name: &str,
    ) -> Result<(String, String), AccessError> {
        self.issue_token(|| Some(caller()), id, org, name, Some(20))
    }

    /// Seed a token without a request, for local fixtures.
    pub fn create_self_token_for_fixture(
        &self,
        id: &str,
        org: &str,
        name: &str,
    ) -> Result<(String, String), AccessError> {
        self.issue_token(|| None, id, org, name, Some(20))
    }

    fn issue_token(
        &self,
        caller: impl FnOnce() -> Option<Principal>,
        id: &str,
        org: &str,
        name: &str,
        max: Option<usize>,
    ) -> Result<(String, String), AccessError> {
        let secret = format!("rvt_{}", random_text(32)?);
        let record = TokenRecord {
            id: format!(
                "t-{}",
                random_text(6)?.to_lowercase().replace(['_', '-'], "x")
            ),
            name: name.trim().chars().take(80).collect(),
            sha256: digest(&secret),
            created: stamp(),
            org: org.to_owned(),
        };
        let token_id = record.id.clone();
        let _gate = self.gate()?;
        let caller = caller();
        if let Some(who) = &caller
            && !matches!(who, Principal::User(user) if user.id == id && user.token_org.is_none())
        {
            return Err(AccessError::Credentials);
        }
        self.with_gated(|state| {
            if !state
                .memberships
                .iter()
                .any(|m| m.person == id && m.org == org)
                || !state
                    .organizations
                    .iter()
                    .any(|o| o.id == org && !o.suspended)
            {
                return Err(AccessError::NotFound);
            }
            let user = state
                .users
                .iter_mut()
                .find(|r| r.user.id == id)
                .ok_or(AccessError::NotFound)?;
            if max.is_some() && user.user.disabled {
                return Err(AccessError::Credentials);
            }
            if max.is_some_and(|max| user.tokens.len() >= max) {
                return Err(AccessError::Invalid(
                    "You can have at most 20 active tokens; revoke one first.".into(),
                ));
            }
            user.tokens.push(record);
            Ok(())
        })?;
        Ok((token_id, secret))
    }

    pub fn revoke_self_token(
        &self,
        caller: impl FnOnce() -> Principal,
        id: &str,
        token_id: &str,
    ) -> Result<(), AccessError> {
        let _gate = self.gate()?;
        if !matches!(caller(), Principal::User(user) if user.id == id && user.token_org.is_none()) {
            return Err(AccessError::Credentials);
        }
        self.revoke_token_gated(id, token_id)
    }

    /// Revoke a token without a request, for local fixtures.
    pub fn revoke_token_for_fixture(&self, id: &str, token_id: &str) -> Result<(), AccessError> {
        let _gate = self.gate()?;
        self.revoke_token_gated(id, token_id)
    }

    /// Server-wide token administration exists only in single-server mode.
    pub fn revoke_single_server_token(&self, id: &str, token_id: &str) -> Result<(), AccessError> {
        if self.is_hosted() {
            return Err(AccessError::NotFound);
        }
        let _gate = self.gate()?;
        self.revoke_token_gated(id, token_id)
    }

    fn revoke_token_gated(&self, id: &str, token_id: &str) -> Result<(), AccessError> {
        self.with_gated(|state| {
            let record = state
                .users
                .iter_mut()
                .find(|r| r.user.id == id)
                .ok_or(AccessError::NotFound)?;
            let before = record.tokens.len();
            record.tokens.retain(|t| t.id != token_id);
            if record.tokens.len() == before {
                return Err(AccessError::NotFound);
            }
            Ok(())
        })
    }

    /// The tokens a person holds, without their secrets.
    pub fn tokens(&self, id: &str) -> Vec<Value> {
        self.read(|state| {
            state
                .users
                .iter()
                .find(|r| r.user.id == id)
                .map(|r| {
                    r.tokens
                        .iter()
                        .map(|t| json!({"id": t.id, "name": t.name, "created": t.created}))
                        .collect()
                })
                .unwrap_or_default()
        })
    }

    pub fn tokens_for_org(&self, id: &str, org: &str) -> Vec<Value> {
        self.read(|state| {
            state
                .users
                .iter()
                .find(|r| r.user.id == id)
                .map(|r| {
                    r.tokens
                        .iter()
                        .filter(|t| t.org == org)
                        .map(|t| json!({"id":t.id,"name":t.name,"created":t.created,"org":t.org}))
                        .collect()
                })
                .unwrap_or_default()
        })
    }

    // ---------------------------------------------------------------- audit

    /// Appends one line to the audit trail. A failure to write it is reported, never swallowed.
    pub fn audit(
        &self,
        actor: &Principal,
        action: &str,
        target: &str,
        allowed: bool,
    ) -> Result<(), String> {
        self.audit_as(&actor.label(), action, target, allowed)
    }

    /// Record an operator action separately from the single-server service principal.
    pub(crate) fn audit_operator(
        &self,
        action: &str,
        target: &str,
        allowed: bool,
    ) -> Result<(), String> {
        self.audit_as("operator", action, target, allowed)
    }

    fn audit_as(
        &self,
        actor: &str,
        action: &str,
        target: &str,
        allowed: bool,
    ) -> Result<(), String> {
        let _guard = self.audit_lock.lock().map_err(|_| "Audit lock poisoned")?;
        // A logged name is bounded: a caller chooses what it says, never how much of the trail it
        // fills.
        let target: String = target.chars().take(200).collect();
        let line = json!({
            "time": stamp(),
            "actor": actor.chars().take(200).collect::<String>(),
            "action": action,
            "target": target,
            "allowed": allowed,
        });
        // Refusals have a trail of their own, so a flood of them cannot push out what changed.
        let name = if allowed {
            "audit.log"
        } else {
            "audit-refused.log"
        };
        let path = self.dir.join(name);
        if fs::metadata(&path).is_ok_and(|m| m.len() > AUDIT_LIMIT_BYTES) {
            for generation in (1..AUDIT_GENERATIONS).rev() {
                let from = if generation == 1 {
                    path.clone()
                } else {
                    self.dir.join(format!("{name}.{}", generation - 1))
                };
                let _ = fs::rename(&from, self.dir.join(format!("{name}.{generation}")));
            }
        }
        let mut options = fs::OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(&path).map_err(|e| e.to_string())?;
        writeln!(file, "{line}").map_err(|e| e.to_string())?;
        // Refusals are logged for review; only changes are forced to disk one by one.
        if allowed {
            file.sync_data().map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Redact the address from the local audit history while retaining the other audit fields.
    /// The caller holds `audit_lock`, so unrelated audit writes cannot be lost during replacement.
    fn redact_account_audits(&self, email: &str) -> Result<(), String> {
        fn redact(value: &mut Value, needles: &[String]) -> bool {
            match value {
                Value::String(text) => {
                    if needles
                        .iter()
                        .any(|needle| text.trim().eq_ignore_ascii_case(needle))
                    {
                        *text = "[deleted]".into();
                        true
                    } else {
                        false
                    }
                }
                Value::Array(values) => {
                    let mut changed = false;
                    for value in values {
                        changed |= redact(value, needles);
                    }
                    changed
                }
                Value::Object(values) => {
                    let mut changed = false;
                    for value in values.values_mut() {
                        changed |= redact(value, needles);
                    }
                    changed
                }
                _ => false,
            }
        }
        let full = email.to_ascii_lowercase();
        let truncated = email
            .chars()
            .take(200)
            .collect::<String>()
            .to_ascii_lowercase();
        let needles = if truncated == full {
            vec![full]
        } else {
            vec![full, truncated]
        };
        for base in ["audit.log", "audit-refused.log"] {
            for generation in 0..AUDIT_GENERATIONS {
                let name = if generation == 0 {
                    base.to_owned()
                } else {
                    format!("{base}.{generation}")
                };
                let path = self.dir.join(name);
                let metadata = match fs::symlink_metadata(&path) {
                    Ok(metadata) if metadata.is_file() => metadata,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    _ => return Err("Invalid audit file".into()),
                };
                if metadata.len() > AUDIT_LIMIT_BYTES + 1024 * 1024 {
                    return Err("Audit file exceeds its limit".into());
                }
                let text = fs::read_to_string(&path).map_err(|_| "Cannot read audit file")?;
                let mut changed = false;
                let mut output = String::new();
                for line in text.lines() {
                    let mut record: Value =
                        serde_json::from_str(line).map_err(|_| "Invalid audit record")?;
                    if redact(&mut record, &needles) {
                        changed = true;
                        output.push_str(
                            &serde_json::to_string(&record).map_err(|_| "Invalid audit record")?,
                        );
                    } else {
                        output.push_str(line);
                    }
                    output.push('\n');
                }
                if changed {
                    self.replace_private(&path, output.as_bytes())?;
                }
            }
        }
        Ok(())
    }

    fn replace_private(&self, path: &Path, bytes: &[u8]) -> Result<(), String> {
        let mut file = tempfile::NamedTempFile::new_in(&*self.dir).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.as_file()
                .set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|e| e.to_string())?;
        }
        file.write_all(bytes)
            .and_then(|_| file.as_file().sync_all())
            .map_err(|e| e.to_string())?;
        file.persist(path).map_err(|e| e.error.to_string())?;
        Ok(())
    }

    fn purge_legacy_backup(&self, person: &str, target: &AccountDeletion) -> Result<(), String> {
        let path = self.dir.join("state.v1.json");
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Ok(metadata) if metadata.is_file() => {}
            _ => return Err("Invalid legacy access backup".into()),
        }
        let mut state: Value = serde_json::from_slice(
            &fs::read(&path).map_err(|_| "Cannot read legacy access backup")?,
        )
        .map_err(|_| "Invalid legacy access backup")?;
        if let Some(users) = state.get_mut("users").and_then(Value::as_array_mut) {
            users.retain(|user| {
                user["id"].as_str() != Some(person)
                    && !user["email"]
                        .as_str()
                        .is_some_and(|email| email.eq_ignore_ascii_case(&target.email))
            });
        }
        if let Some(sessions) = state.get_mut("sessions").and_then(Value::as_array_mut) {
            sessions.retain(|session| session["user"].as_str() != Some(person));
        }
        if let Some(organizations) = state.get_mut("organizations").and_then(Value::as_array_mut) {
            organizations.retain(|organization| {
                !organization["id"]
                    .as_str()
                    .is_some_and(|id| target.orgs.iter().any(|org| org == id))
            });
        }
        if let Some(memberships) = state.get_mut("memberships").and_then(Value::as_array_mut) {
            memberships.retain(|membership| {
                membership["person"].as_str() != Some(person)
                    && !membership["org"]
                        .as_str()
                        .is_some_and(|id| target.orgs.iter().any(|org| org == id))
            });
        }
        if let Some(registrations) = state.get_mut("registrations").and_then(Value::as_array_mut) {
            registrations.retain(|registration| {
                !registration["email"]
                    .as_str()
                    .is_some_and(|email| email.eq_ignore_ascii_case(&target.email))
            });
        }
        if let Some(acls) = state.get_mut("acls").and_then(Value::as_object_mut) {
            acls.retain(|key, acl| {
                !target
                    .orgs
                    .iter()
                    .any(|org| key.starts_with(&format!("{org}/")))
                    && acl["owner"].as_str() != Some(person)
            });
            for acl in acls.values_mut() {
                if let Some(grants) = acl.get_mut("grants").and_then(Value::as_array_mut) {
                    grants.retain(|grant| grant["user"].as_str() != Some(person));
                }
            }
        }
        self.replace_private(
            &path,
            &serde_json::to_vec_pretty(&state).map_err(|_| "Invalid legacy access backup")?,
        )
    }

    /// The latest `limit` audit lines, changes and refusals together, newest first.
    pub fn read_audit(&self, limit: usize) -> Vec<Value> {
        let mut lines: Vec<Value> = Vec::new();
        for name in ["audit.log", "audit-refused.log"] {
            let text = fs::read_to_string(self.dir.join(name)).unwrap_or_default();
            lines.extend(
                text.lines()
                    .rev()
                    .take(limit)
                    .filter_map(|line| serde_json::from_str(line).ok()),
            );
        }
        lines.sort_by(|a, b| b["time"].as_str().cmp(&a["time"].as_str()));
        lines.truncate(limit);
        lines
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let mut diff = a.len() ^ b.len();
    for (i, byte) in a.iter().enumerate() {
        diff |= (*byte ^ b.get(i).copied().unwrap_or(0)) as usize;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    #[test]
    fn refused_publication_logs_failed_access_restoration() {
        const CHILD: &str = "RICHVIEW_TEST_RESTORE_LOG_CHILD";
        if std::env::var_os(CHILD).is_some() {
            use super::{Access, Principal, Visibility};
            let dir = tempfile::tempdir().unwrap();
            let access = Access::open(dir.path(), Visibility::Private, None).unwrap();
            let result = access.commit_publication(
                || Principal::Service,
                "test-report",
                false,
                None,
                || -> Result<(), &str> {
                    std::fs::rename(dir.path().join(".access"), dir.path().join("saved")).unwrap();
                    std::fs::write(dir.path().join(".access"), b"blocked").unwrap();
                    Err("store refused")
                },
            );
            assert!(matches!(result, Ok(Err("store refused"))));
            return;
        }
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "access::tests::refused_publication_logs_failed_access_restoration",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(output.status.success());
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            stderr.contains("restore") && stderr.contains("test-report"),
            "{stderr}"
        );
    }

    #[tokio::test]
    async fn periodic_mail_sweep_removes_expired_invitations_and_budget_timestamps_without_a_request()
     {
        use super::{Access, Invitation, OrgRole, now};
        use crate::mail::{Outbox, Transport, start_transport_cleanup};
        use std::{sync::Arc, time::Duration};

        let dir = tempfile::tempdir().unwrap();
        let access = Access::open_hosted(dir.path()).unwrap();
        let at = now();
        {
            let mut state = access.state.lock().unwrap();
            for (id, email, expires) in [
                ("expired", "expired@example.test", at - 1),
                ("live", "live@example.test", at + 3600),
            ] {
                state.invitations.push(Invitation {
                    id: id.into(),
                    org: "org".into(),
                    email: email.into(),
                    role: Some(OrgRole::Reader),
                    report: None,
                    report_role: None,
                    inviter: "person".into(),
                    created: at - 100,
                    expires,
                });
            }
            access.save(&state).unwrap();
        }
        {
            let mut budgets = access.invite_limits.lock().unwrap();
            budgets.insert("person".into(), [at - 86_401, at].into_iter().collect());
            budgets.insert("stale".into(), [at - 86_401].into_iter().collect());
            access.save_invite_limits(&budgets).unwrap();
        }
        let transport: Arc<dyn Transport> =
            Arc::new(Outbox::open(&dir.path().join("outbox")).unwrap());
        let maintenance = access.mail_maintenance();
        start_transport_cleanup(&transport, Duration::from_millis(10), Some(&maintenance));
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let state: serde_json::Value = serde_json::from_slice(
                    &std::fs::read(dir.path().join(".access/state.json")).unwrap(),
                )
                .unwrap();
                let budgets: serde_json::Value = serde_json::from_slice(
                    &std::fs::read(dir.path().join(".access/invite-mail-budget.json")).unwrap(),
                )
                .unwrap();
                if !state.to_string().contains("expired@example.test")
                    && budgets["person"]
                        .as_array()
                        .is_some_and(|times| times.len() == 1)
                    && budgets.get("stale").is_none()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let state = access.state.lock().unwrap();
        assert_eq!(state.invitations.len(), 1);
        assert_eq!(state.invitations[0].email, "live@example.test");
    }

    #[test]
    fn startup_prunes_expired_invitation_budget_timestamps_from_disk() {
        use super::{Access, now};
        let dir = tempfile::tempdir().unwrap();
        let at = now();
        {
            let access = Access::open_hosted(dir.path()).unwrap();
            let mut budgets = access.invite_limits.lock().unwrap();
            budgets.insert("person".into(), [at - 86_401, at].into_iter().collect());
            access.save_invite_limits(&budgets).unwrap();
        }
        let _access = Access::open_hosted(dir.path()).unwrap();
        let budgets: serde_json::Value = serde_json::from_slice(
            &std::fs::read(dir.path().join(".access/invite-mail-budget.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(budgets["person"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn historical_padded_targets_of_other_addresses_are_not_redacted() {
        use super::Access;
        let dir = tempfile::tempdir().unwrap();
        let access = Access::open_hosted(dir.path()).unwrap();
        let targets = [
            format!("{}alice@example.co", " ".repeat(184)),
            format!("{}a", " ".repeat(199)),
        ];
        let audit = dir.path().join(".access");
        for (name, target) in ["audit-refused.log", "audit-refused.log.1"]
            .iter()
            .zip(&targets)
        {
            assert_eq!(target.chars().count(), 200);
            let old = serde_json::json!({"time":"old", "actor":"anonymous", "action":"sign-in", "target":target, "allowed":false});
            let deleted = serde_json::json!({"time":"old", "actor":"anonymous", "action":"sign-in", "target":"alice@example.com", "allowed":false});
            std::fs::write(audit.join(name), format!("{old}\n{deleted}\n")).unwrap();
        }
        access.redact_account_audits("alice@example.com").unwrap();
        let mut actual = Vec::new();
        for name in ["audit-refused.log", "audit-refused.log.1"] {
            let contents = std::fs::read_to_string(audit.join(name)).unwrap();
            let records: Vec<serde_json::Value> = contents
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            actual.push(records[0]["target"].as_str().unwrap().to_owned());
            assert_eq!(records[1]["target"], "[deleted]");
        }
        assert_eq!(actual, targets);
    }

    #[test]
    fn audit_preserves_mixed_case_identifier() {
        use super::{Access, Principal};
        let dir = tempfile::tempdir().unwrap();
        let access = Access::open_hosted(dir.path()).unwrap();
        access
            .audit(&Principal::Anonymous, "operator.disable", "u-aB3_XyZ", true)
            .unwrap();
        let line = std::fs::read_to_string(dir.path().join(".access/audit.log")).unwrap();
        let entry: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(entry["target"], "u-aB3_XyZ");
    }

    #[test]
    fn account_redaction_keeps_other_targets_and_redacts_clipped_addresses() {
        use super::{Access, Principal};
        let dir = tempfile::tempdir().unwrap();
        let access = Access::open_hosted(dir.path()).unwrap();
        let email = format!("{}@example.test", "a".repeat(200));
        let clipped = email.chars().take(200).collect::<String>();
        access
            .audit(&Principal::Anonymous, "sign-in", &email, false)
            .unwrap();
        access
            .audit(
                &Principal::Anonymous,
                "sign-in",
                &format!("b{clipped}"),
                false,
            )
            .unwrap();
        access.redact_account_audits(&email).unwrap();
        let records =
            std::fs::read_to_string(dir.path().join(".access/audit-refused.log")).unwrap();
        assert!(records.contains("[deleted]"));
        assert!(records.contains(&format!("b{}", "a".repeat(199))));
    }

    use super::*;

    #[tokio::test]
    async fn organization_creation_count_survives_ownership_transfer_and_restart() {
        let dir = tempfile::tempdir().unwrap();
        let access = Access::open_hosted(dir.path()).unwrap();
        access.set_signup_account_limit(2).unwrap();
        assert!(!access.operator_accounts_half_full());
        let token = access
            .issue_link("owner@example.test", LinkPurpose::Signup)
            .unwrap()
            .unwrap();
        let user = access
            .complete_hosted_link(&token, "Owner", "long enough password")
            .await
            .unwrap();
        assert!(access.operator_accounts_half_full());
        access
            .with(|state| {
                state
                    .memberships
                    .iter_mut()
                    .find(|m| m.person == user.id)
                    .unwrap()
                    .role = OrgRole::Admin;
                Ok(())
            })
            .unwrap();
        drop(access);
        let access = Access::open_hosted(dir.path()).unwrap();
        assert!(matches!(
            access.create_organization(|| Principal::User(user.clone()), "Second"),
            Err(AccessError::Invalid(_))
        ));
        access.set_org_creation_limit(2).unwrap();
        assert!(
            access
                .create_organization(|| Principal::User(user.clone()), "Second")
                .is_ok()
        );
    }

    #[tokio::test]
    async fn signup_pause_and_account_cap_prevent_account_creation_without_spending_the_link() {
        let dir = tempfile::tempdir().unwrap();
        let access = Access::open_hosted(dir.path()).unwrap();
        let token = access
            .issue_link("new@example.test", LinkPurpose::Signup)
            .unwrap()
            .unwrap();
        access.operator_set_signups_paused(true).unwrap();
        assert!(!access.signup_account_capacity());
        assert_eq!(
            access
                .complete_hosted_link(&token, "New", "long password here")
                .await
                .unwrap_err(),
            AccessError::Throttled
        );
        assert_eq!(access.inspect_link(&token), Some(LinkPurpose::Signup));
        access.operator_set_signups_paused(false).unwrap();
        access
            .with(|state| {
                for i in 0..MAX_HOSTED_ACCOUNTS {
                    let user = User {
                        id: format!("u-{i}"),
                        name: "Existing".into(),
                        email: format!("u-{i}@example.test"),
                        role: OrgRole::Owner,
                        disabled: false,
                        created: "now".into(),
                        verified: Some("now".into()),
                        operator: false,
                        token_org: None,
                    };
                    state.users.push(UserRecord {
                        user,
                        password: None,
                        tokens: vec![],
                    });
                }
                Ok(())
            })
            .unwrap();
        assert!(!access.signup_account_capacity());
        assert_eq!(
            access
                .complete_hosted_link(&token, "New", "long password here")
                .await
                .unwrap_err(),
            AccessError::Throttled
        );
        assert_eq!(access.inspect_link(&token), Some(LinkPurpose::Signup));
    }

    #[tokio::test]
    async fn sign_up_names_the_first_organization_after_its_owner_with_a_suffix() {
        let dir = tempfile::tempdir().unwrap();
        let access = Access::open_hosted(dir.path()).unwrap();
        let signup = access
            .issue_link("jana@example.test", LinkPurpose::Signup)
            .unwrap()
            .unwrap();
        let (user, _, _) = access
            .complete_hosted_link_and_sign_in(&signup, "  Jana Novak ", "first long password", None)
            .await
            .unwrap();
        let organizations = access.organizations_for(&user.id);
        assert_eq!(organizations.len(), 1);
        assert_eq!(organizations[0].name, "Jana Novak’s organization");
    }

    #[test]
    fn a_long_name_still_gives_a_first_organization_name_a_rename_accepts() {
        let name = first_organization_name(&"Maximiliana ".repeat(10));
        assert!(name.ends_with("’s organization"), "{name}");
        assert!(name.chars().count() <= teams::MAX_NAME_CHARS);
        assert!(!name.contains(" ’s"));
    }

    #[tokio::test]
    async fn link_completion_creates_only_the_new_verified_session_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let access = Access::open_hosted(dir.path()).unwrap();
        let signup = access
            .issue_link("owner@example.test", LinkPurpose::Signup)
            .unwrap()
            .unwrap();
        let (user, first_cookie, _) = access
            .complete_hosted_link_and_sign_in(&signup, "Owner", "first long password", None)
            .await
            .unwrap();
        assert!(matches!(
            access.authenticate(None, Some(&first_cookie)),
            Principal::User(_)
        ));
        let org = access.organizations_for(&user.id)[0].id.clone();
        let (_, old_token) = access
            .create_self_token_for_fixture(&user.id, &org, "old")
            .unwrap();
        let reset = access
            .issue_link("owner@example.test", LinkPurpose::Reset)
            .unwrap()
            .unwrap();
        let (_, new_cookie, _) = access
            .complete_hosted_link_and_sign_in(&reset, "", "second long password", None)
            .await
            .unwrap();
        assert!(matches!(
            access.authenticate(None, Some(&first_cookie)),
            Principal::Anonymous
        ));
        assert!(matches!(
            access.authenticate(Some(&old_token), None),
            Principal::Anonymous
        ));
        assert!(matches!(
            access.authenticate(None, Some(&new_cookie)),
            Principal::User(_)
        ));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancelled_signup_keeps_publication_lock_until_its_worker_finishes() {
        let dir = tempfile::tempdir().unwrap();
        let mut access = Access::open_hosted(dir.path()).unwrap();
        access.lane_patience = std::time::Duration::from_secs(2);
        let token = access
            .issue_link("cancel@example.test", LinkPurpose::Signup)
            .unwrap()
            .unwrap();
        let (locked_tx, locked_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let lane = std::thread::spawn(move || {
            let _guard = HASHING.lock().unwrap_or_else(|e| e.into_inner());
            locked_tx.send(()).unwrap();
            release_rx.recv().unwrap();
        });
        locked_rx.recv().unwrap();
        let publication = access.publication.clone().lock_owned().await;
        let worker_access = access.clone();
        let task = tokio::spawn(async move {
            worker_access
                .complete_hosted_link_and_sign_in(
                    &token,
                    "Cancel",
                    "long password here",
                    Some(publication),
                )
                .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while access.registration_workers.available_permits() != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        task.abort();
        assert!(task.await.is_err());
        assert!(access.publication.try_lock().is_err());
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while access.registration_workers.available_permits() == 0 {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(access.publication.try_lock().is_ok());
        release_tx.send(()).unwrap();
        lane.join().unwrap();
    }

    #[test]
    fn link_request_ips_and_workers_are_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let access = Access::open_hosted(dir.path()).unwrap();
        for i in 0..1024 {
            let ip =
                std::net::IpAddr::from(std::net::Ipv4Addr::new(10, (i >> 8) as u8, i as u8, 1));
            drop(access.admit_link_request(Some(ip)).unwrap());
        }
        assert!(matches!(
            access.admit_link_request(Some("192.0.2.1".parse().unwrap())),
            Err(AccessError::Throttled)
        ));
        assert_eq!(access.link_request_ips.lock().unwrap().len(), 1024);
        let known: std::net::IpAddr = "10.0.0.1".parse().unwrap();
        let permits: Vec<_> = (0..4)
            .map(|_| access.admit_link_request(Some(known)).unwrap())
            .collect();
        assert!(matches!(
            access.admit_link_request(Some(known)),
            Err(AccessError::Throttled)
        ));
        drop(permits);
        for _ in 0..4 {
            drop(access.admit_link_request(Some(known)).unwrap());
        }
        assert!(matches!(
            access.admit_link_request(Some(known)),
            Err(AccessError::Throttled)
        ));
        assert!(
            access
                .admit_link_request(Some("10.0.1.1".parse().unwrap()))
                .is_ok()
        );
    }

    #[test]
    fn hosted_publish_rate_is_per_organization_and_expires() {
        let dir = tempfile::tempdir().unwrap();
        let access = Access::open_hosted(dir.path()).unwrap();
        for _ in 0..60 {
            access.admit_hosted_publish("aaaa0001/report").unwrap();
        }
        assert!(matches!(
            access.admit_hosted_publish("aaaa0001/another"),
            Err(AccessError::Throttled)
        ));
        assert!(access.admit_hosted_publish("bbbb0002/report").is_ok());
        access
            .publish_limits
            .lock()
            .unwrap()
            .get_mut("aaaa0001")
            .unwrap()
            .iter_mut()
            .for_each(|time| *time = now().saturating_sub(3601));
        assert!(access.admit_hosted_publish("aaaa0001/report").is_ok());
    }

    #[test]
    fn hosted_unknown_lockout_still_uses_the_hashing_lane() {
        let dir = tempfile::tempdir().unwrap();
        let access = Access::open_hosted(dir.path()).unwrap();
        access.failures.lock().unwrap().unknown = (MAX_FAILURES, now() + LOCKOUT_SECONDS);
        let lane = HASHING.lock().unwrap_or_else(|e| e.into_inner());
        let attempt = access.clone();
        let (started, ready) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            started.send(()).unwrap();
            attempt.sign_in("missing@example.test", "wrong password long")
        });
        ready.recv().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(75));
        assert!(
            !worker.is_finished(),
            "a locked unknown address must still enter the hashing lane"
        );
        drop(lane);
        assert_eq!(worker.join().unwrap(), Err(AccessError::Credentials));
    }

    #[test]
    fn sign_in_worker_admission_is_independent_of_email() {
        let (_dir, access) = access(Visibility::Private);
        let permits: Vec<_> = (0..MAX_SIGN_IN_WORKERS)
            .map(|_| {
                access
                    .admit_sign_in("any@example.test", "password")
                    .unwrap()
            })
            .collect();
        assert!(matches!(
            access.admit_sign_in("known@example.test", "password"),
            Err(AccessError::Throttled)
        ));
        assert!(matches!(
            access.admit_sign_in("absent@example.test", "password"),
            Err(AccessError::Throttled)
        ));
        drop(permits);
    }

    #[test]
    fn hosted_acl_history_has_a_global_bound() {
        let mut state = State {
            tenancy: Tenancy::Hosted,
            ..State::default()
        };
        for i in 0..MAX_HOSTED_ACLS {
            let org = format!("{i:08}");
            state.acls.insert(
                format!("{org}/report"),
                ReportAcl {
                    org,
                    owner: "a".into(),
                    visibility: Visibility::Private,
                    grants: vec![],
                },
            );
        }
        assert!(hosted_claim_capacity(&state, "aaaa0001", "aaaa0001/new").is_err());
        assert!(hosted_claim_capacity(&state, "00000000", "00000000/report").is_ok());
    }

    #[test]
    fn hosted_retained_grants_have_organization_and_global_bounds() {
        let grants: Vec<_> = (0..MAX_HOSTED_GRANTS)
            .map(|i| Grant {
                user: format!("u-{i}"),
                role: ReportRole::Viewer,
            })
            .collect();
        let mut state = State {
            tenancy: Tenancy::Hosted,
            ..State::default()
        };
        for i in 0..(MAX_HOSTED_GRANTS_PER_ORG / MAX_HOSTED_GRANTS) {
            state.acls.insert(
                format!("aaaa0001/report-{i}"),
                ReportAcl {
                    org: "aaaa0001".into(),
                    owner: "owner".into(),
                    visibility: Visibility::Private,
                    grants: grants.clone(),
                },
            );
        }
        assert!(hosted_grant_capacity(&state, "aaaa0001", "aaaa0001/new", 1, 0).is_err());
        assert!(
            hosted_grant_capacity(
                &state,
                "aaaa0001",
                "aaaa0001/report-0",
                MAX_HOSTED_GRANTS - 1,
                0
            )
            .is_ok()
        );
        for i in 0..(MAX_HOSTED_TOTAL_GRANTS / MAX_HOSTED_GRANTS - state.acls.len()) {
            let org = format!("{i:08}");
            state.acls.insert(
                format!("{org}/report"),
                ReportAcl {
                    org,
                    owner: "owner".into(),
                    visibility: Visibility::Private,
                    grants: grants.clone(),
                },
            );
        }
        assert!(hosted_grant_capacity(&state, "zzzz9999", "zzzz9999/new", 1, 0).is_err());
    }

    fn access(visibility: Visibility) -> (tempfile::TempDir, Access) {
        let dir = tempfile::tempdir().unwrap();
        let access =
            Access::open(dir.path(), visibility, Some(Arc::from("service-token"))).unwrap();
        (dir, access)
    }

    fn person(access: &Access, name: &str, role: OrgRole) -> User {
        access
            .create_user(
                name,
                &format!("{name}@example.test"),
                role,
                Some("a long enough password"),
            )
            .unwrap()
    }

    #[test]
    fn sign_in_cannot_issue_a_session_after_its_password_or_account_changes() {
        for disable in [false, true] {
            let (_dir, access) = access(Visibility::Private);
            let user = person(&access, "race", OrgRole::Author);
            let replacement = new_password_hash("a different long password").unwrap();
            let lane = HASHING.lock().unwrap_or_else(|e| e.into_inner());
            let login_access = access.clone();
            let email = user.email.clone();
            let login =
                std::thread::spawn(move || login_access.sign_in(&email, "a long enough password"));
            let failure_key = digest(&user.email);
            let start = std::time::Instant::now();
            loop {
                let admitted = access
                    .failures
                    .lock()
                    .unwrap()
                    .known
                    .contains_key(&failure_key);
                if admitted {
                    break;
                }
                assert!(
                    start.elapsed() < std::time::Duration::from_secs(10),
                    "sign-in did not reach the hashing lane"
                );
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            access
                .with(|state| {
                    let current = state
                        .users
                        .iter_mut()
                        .find(|r| r.user.id == user.id)
                        .unwrap();
                    if disable {
                        current.user.disabled = true;
                    } else {
                        current.password = Some(replacement);
                    }
                    Ok(())
                })
                .unwrap();
            drop(lane);
            assert_eq!(login.join().unwrap(), Err(AccessError::Credentials));
            assert!(access.read(|state| state.sessions.iter().all(|s| s.user != user.id)));
        }
    }

    #[test]
    fn rights_follow_visibility_grants_and_roles() {
        let (_dir, access) = access(Visibility::Private);
        let owner = person(&access, "owner", OrgRole::Author);
        let reader = person(&access, "reader", OrgRole::Reader);
        let editor = person(&access, "editor", OrgRole::Reader);
        let admin = person(&access, "admin", OrgRole::Admin);
        access
            .claim(|| Principal::User(owner.clone()), "r")
            .unwrap();
        let (o, r, e, a) = (
            Principal::User(owner),
            Principal::User(reader.clone()),
            Principal::User(editor.clone()),
            Principal::User(admin),
        );
        assert!(access.can(&o, "r", Right::Delete));
        assert!(
            access.rights(&r, "r").is_empty(),
            "private hides it from others"
        );
        assert!(access.rights(&Principal::Anonymous, "r").is_empty());
        access
            .set_acl_for_fixture(
                "r",
                Visibility::Org,
                vec![Grant {
                    user: editor.id.clone(),
                    role: ReportRole::Editor,
                }],
            )
            .unwrap();
        assert_eq!(access.rights(&r, "r"), [Right::View].into_iter().collect());
        assert!(access.can(&e, "r", Right::Edit) && !access.can(&e, "r", Right::Delete));
        assert!(
            access.rights(&Principal::Anonymous, "r").is_empty(),
            "org is not public"
        );
        access
            .set_acl_for_fixture("r", Visibility::Public, vec![])
            .unwrap();
        assert!(access.can(&Principal::Anonymous, "r", Right::View));
        assert!(!access.can(&Principal::Anonymous, "r", Right::Edit));
        assert!(
            access.can(&a, "r", Right::Share),
            "an admin can do everything"
        );
        assert!(access.can(&Principal::Service, "r", Right::Delete));
        assert!(
            access.can_create(&o)
                && !access.can_create(&r)
                && !access.can_create(&Principal::Anonymous)
        );
    }

    #[test]
    fn sign_in_sessions_tokens_and_lockout() {
        let (_dir, access) = access(Visibility::Private);
        let user = person(&access, "ada", OrgRole::Author);
        assert_eq!(
            access.sign_in("ada@example.test", "wrong password!!"),
            Err(AccessError::Credentials)
        );
        assert_eq!(
            access.sign_in("nobody@example.test", "a long enough password"),
            Err(AccessError::Credentials)
        );
        let (cookie, csrf) = access
            .sign_in("ADA@example.test", "a long enough password")
            .unwrap();
        assert!(
            matches!(access.authenticate(None, Some(&cookie)), Principal::User(u) if u.id == user.id)
        );
        assert_eq!(access.csrf_for(&cookie).as_deref(), Some(csrf.as_str()));
        access.sign_out_for_fixture(&cookie).unwrap();
        assert!(matches!(
            access.authenticate(None, Some(&cookie)),
            Principal::Anonymous
        ));
        let (token_id, secret) = access.create_token(&user.id, "ci").unwrap();
        assert!(
            matches!(access.authenticate(Some(&secret), None), Principal::User(u) if u.id == user.id)
        );
        assert!(matches!(
            access.authenticate(Some("service-token"), None),
            Principal::Service
        ));
        assert!(matches!(
            access.authenticate(Some("wrong"), None),
            Principal::Anonymous
        ));
        access
            .revoke_token_for_fixture(&user.id, &token_id)
            .unwrap();
        assert!(matches!(
            access.authenticate(Some(&secret), None),
            Principal::Anonymous
        ));
        for _ in 0..MAX_FAILURES {
            let _ = access.sign_in("ada@example.test", "wrong password!!");
        }
        assert_eq!(
            access.sign_in("ada@example.test", "a long enough password"),
            Err(AccessError::Throttled),
            "five failures lock the address out even for the right password"
        );
    }

    #[test]
    fn a_sign_in_that_cannot_get_the_hashing_lane_in_time_is_throttled() {
        let (_dir, mut access) = access(Visibility::Private);
        person(&access, "ada", OrgRole::Author);
        access.lane_patience = std::time::Duration::from_millis(60);
        // Someone else is hashing: the lane is full for longer than this sign-in will wait.
        let busy = HASHING.lock().unwrap_or_else(|e| e.into_inner());
        let started = std::time::Instant::now();
        let result = access.sign_in("ada@example.test", "a long enough password");
        drop(busy);
        assert_eq!(result, Err(AccessError::Throttled));
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        // With the lane free, the same sign-in succeeds.
        access.lane_patience = std::time::Duration::from_secs(60);
        assert!(
            access
                .sign_in("ada@example.test", "a long enough password")
                .is_ok()
        );
    }

    #[test]
    fn distinct_fresh_sign_in_failures_and_credential_sizes_are_bounded() {
        let (_dir, mut access) = access(Visibility::Private);
        access.lane_patience = std::time::Duration::ZERO;
        let busy = HASHING.lock().unwrap_or_else(|e| e.into_inner());
        for n in 0..1100 {
            let _ = access.sign_in(&format!("fresh-{n}@example.test"), "wrong password");
        }
        {
            let failures = access.failures.lock().unwrap();
            assert!(failures.known.is_empty());
            assert_eq!(failures.unknown.0, MAX_FAILURES);
        }
        assert_eq!(
            access.sign_in(&"a".repeat(255), "wrong password"),
            Err(AccessError::Credentials)
        );
        assert_eq!(
            access.sign_in("a@example.test", &"p".repeat(4097)),
            Err(AccessError::Credentials)
        );
        access.failures.lock().unwrap().unknown.1 = 0;
        let _ = access.sign_in("new-after-expiry@example.test", "wrong password");
        let failures = access.failures.lock().unwrap();
        assert!(failures.known.is_empty());
        assert_eq!(failures.unknown.0, 1);
        drop(failures);
        drop(busy);
    }

    #[test]
    fn known_account_counters_are_bounded_and_expired_slots_are_reused() {
        let (_dir, mut access) = access(Visibility::Private);
        person(&access, "author", OrgRole::Author);
        access.lane_patience = std::time::Duration::ZERO;
        let busy = HASHING.lock().unwrap_or_else(|e| e.into_inner());
        {
            let mut failures = access.failures.lock().unwrap();
            for n in 0..MAX_FAILURE_KEYS {
                failures.known.insert(
                    digest(&format!("account-{n}")),
                    (1, now() + LOCKOUT_SECONDS),
                );
            }
        }
        assert_eq!(
            access.sign_in("author@example.test", "wrong password"),
            Err(AccessError::Throttled)
        );
        assert_eq!(
            access.failures.lock().unwrap().known.len(),
            MAX_FAILURE_KEYS
        );
        access
            .failures
            .lock()
            .unwrap()
            .known
            .values_mut()
            .next()
            .unwrap()
            .1 = 0;
        let _ = access.sign_in("author@example.test", "wrong password");
        let failures = access.failures.lock().unwrap();
        assert_eq!(failures.known.len(), MAX_FAILURE_KEYS);
        assert!(failures.known.contains_key(&digest("author@example.test")));
        drop(failures);
        drop(busy);
    }

    #[test]
    fn existing_author_can_sign_in_after_unknown_address_flood() {
        let (_dir, mut access) = access(Visibility::Private);
        let author = person(&access, "author", OrgRole::Author);
        access.lane_patience = std::time::Duration::ZERO;
        let busy = HASHING.lock().unwrap_or_else(|e| e.into_inner());
        for n in 0..1100 {
            let _ = access.sign_in(&format!("unknown-{n}@example.test"), "wrong password");
        }
        drop(busy);
        access.lane_patience = std::time::Duration::from_secs(60);
        let (cookie, _) = access
            .sign_in("author@example.test", "a long enough password")
            .expect("unknown addresses must not exhaust author sign-in counters");
        assert!(
            matches!(access.authenticate(None, Some(&cookie)), Principal::User(user) if user.id == author.id)
        );
    }

    #[test]
    fn sign_in_admission_is_shared_bounded_and_released() {
        let (_dir, access) = access(Visibility::Private);
        let mut permits = Vec::new();
        for _ in 0..MAX_SIGN_IN_WORKERS {
            permits.push(access.admit_sign_in("x@example.test", "password").unwrap());
        }
        assert!(matches!(
            access.clone().admit_sign_in("y@example.test", "password"),
            Err(AccessError::Throttled)
        ));
        drop(permits.pop());
        assert!(access.admit_sign_in("y@example.test", "password").is_ok());
        assert!(matches!(
            access.admit_sign_in(&"a".repeat(255), "password"),
            Err(AccessError::Credentials)
        ));
    }

    #[test]
    fn disabling_signs_out_and_the_last_admin_stays() {
        let (_dir, access) = access(Visibility::Private);
        let admin = person(&access, "root", OrgRole::Admin);
        let author = person(&access, "writer", OrgRole::Author);
        let (cookie, _) = access
            .sign_in("writer@example.test", "a long enough password")
            .unwrap();
        access
            .update_user(&author.id, None, None, Some(true))
            .unwrap();
        assert!(matches!(
            access.authenticate(None, Some(&cookie)),
            Principal::Anonymous
        ));
        assert_eq!(
            access.sign_in("writer@example.test", "a long enough password"),
            Err(AccessError::Credentials)
        );
        assert!(matches!(
            access.update_user(&admin.id, None, Some(OrgRole::Reader), None),
            Err(AccessError::Invalid(_))
        ));
        assert!(matches!(
            access.delete_user(&admin.id),
            Err(AccessError::Invalid(_))
        ));
        assert!(matches!(
            access.create_user("x", "x@example.test", OrgRole::Reader, Some("short")),
            Err(AccessError::Invalid(_))
        ));
    }

    #[test]
    fn state_and_audit_survive_a_restart_and_digests_hide_secrets() {
        let (dir, access) = access(Visibility::Org);
        let user = person(&access, "kept", OrgRole::Author);
        let (_, secret) = access.create_token(&user.id, "laptop").unwrap();
        let (cookie, _) = access
            .sign_in("kept@example.test", "a long enough password")
            .unwrap();
        access
            .audit(&Principal::User(user.clone()), "report.publish", "r", true)
            .unwrap();
        let again = Access::open(dir.path(), Visibility::Org, None).unwrap();
        assert_eq!(again.users().len(), 1);
        assert!(matches!(
            again.authenticate(Some(&secret), None),
            Principal::User(_)
        ));
        assert!(matches!(
            again.authenticate(None, Some(&cookie)),
            Principal::User(_)
        ));
        let state = std::fs::read_to_string(dir.path().join(".access/state.json")).unwrap();
        assert!(!state.contains(&secret) && !state.contains(&cookie));
        assert!(!state.contains("a long enough password"));
        assert_eq!(again.read_audit(10)[0]["action"], "report.publish");
    }

    #[test]
    fn a_refused_access_change_leaves_the_owner_and_grants_as_they_were() {
        let (_dir, access) = access(Visibility::Private);
        let first = person(&access, "first", OrgRole::Author);
        let second = person(&access, "second", OrgRole::Author);
        access
            .set_access_for_fixture("r", Some(&first.id), Visibility::Private, vec![])
            .unwrap();
        let bad = Grant {
            user: "nobody".into(),
            role: ReportRole::Viewer,
        };
        let refused =
            access.set_access_for_fixture("r", Some(&second.id), Visibility::Public, vec![bad]);
        assert!(refused.is_err());
        let acl = access.acl("r");
        assert_eq!(acl.owner, first.id);
        assert_eq!(acl.visibility, Visibility::Private);
    }
}
