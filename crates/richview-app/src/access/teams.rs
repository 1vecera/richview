//! Hosted organizations with more than one person: members, invitations, and sharing one report
//! with a person named by email.
//!
//! Every function here decides from the access state inside one read or one write, never from
//! what a page showed. Two rules hold throughout. An invitation gives at most what its sender
//! still holds when it is accepted. And nothing here answers differently for an address that has
//! an account and one that has none: sharing by email always leaves a pending invitation, which
//! becomes a grant only once the person behind the address proves it, by the mail link or by
//! signing in.

use super::*;

/// An invitation stays open this long; each mail link for it lasts one day.
const INVITATION_SECONDS: u64 = 7 * 24 * 3600;
/// Members of one organization, pending invitations included.
const MAX_ORG_SEATS: usize = 100;
const MAX_ORG_INVITATIONS: usize = 50;
/// Organizations one person may belong to.
const MAX_MEMBERSHIPS: usize = 20;
/// Invitation and share mails one person may cause in a day.
const MAX_INVITE_MAILS_PER_DAY: usize = 40;
/// Invitation and share mails all members of one organization cause in a day.
const MAX_ORG_INVITE_MAILS_PER_DAY: usize = 100;
pub(super) const MAX_NAME_CHARS: usize = 80;

/// An offer to join an organization (`role`) or to open one report (`report`, `report_role`),
/// addressed to an email address and not yet accepted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Invitation {
    pub id: String,
    pub org: String,
    pub email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<OrgRole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report_role: Option<ReportRole>,
    pub inviter: String,
    pub created: u64,
    pub expires: u64,
}

/// An invitation with the names a page or a mail shows for it.
#[derive(Debug, Clone)]
pub struct InviteView {
    pub invitation: Invitation,
    pub org_name: String,
    pub inviter_name: String,
    pub inviter_email: String,
    /// Whether the invited address already has an account. Only ever shown to the invitee.
    pub account_exists: bool,
}

#[derive(Debug, Clone)]
pub struct MemberRow {
    pub person: String,
    pub name: String,
    pub email: String,
    pub role: OrgRole,
    pub created: String,
}

/// Where an accepted invitation leads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Landing {
    Organization(String),
    Report(String),
}

#[derive(Debug, Clone)]
pub struct Accepted {
    pub user: User,
    /// Cookie and CSRF token, only for an account this acceptance created.
    pub session: Option<(String, String)>,
    pub landing: Landing,
}

/// How a report reaches the person looking at their list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    Mine,
    Organization,
    Shared,
}

#[derive(Debug, Clone)]
pub struct VisibleReport {
    pub key: String,
    pub org: String,
    pub org_name: String,
    pub owner_name: String,
    pub visibility: Visibility,
    pub relation: Relation,
    /// Whether the person holds a grant on it by name.
    pub named: bool,
}

/// One named person on a report. `name` is set only for a member of the report's organization;
/// an outside address shows as the address alone, accepted or not.
#[derive(Debug, Clone)]
pub struct AccessRow {
    pub email: String,
    pub name: Option<String>,
    pub role: ReportRole,
    pub pending: bool,
}

#[derive(Debug, Clone)]
pub struct ReportAccess {
    pub org_name: String,
    pub owner_name: String,
    pub owner_email: String,
    pub visibility: Visibility,
    pub named: Vec<AccessRow>,
}

fn rank(role: OrgRole) -> u8 {
    match role {
        OrgRole::Owner => 3,
        OrgRole::Admin => 2,
        OrgRole::Author => 1,
        OrgRole::Reader => 0,
    }
}

fn manages(role: OrgRole) -> bool {
    matches!(role, OrgRole::Owner | OrgRole::Admin)
}

/// A name a person typed: trimmed, without control characters, of a bounded length.
fn clean_name(text: &str) -> Option<String> {
    let name: String = text
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!name.is_empty() && name.chars().count() <= MAX_NAME_CHARS).then_some(name)
}

fn active_org<'a>(state: &'a State, org: &str) -> Option<&'a Organization> {
    state
        .organizations
        .iter()
        .find(|o| o.id == org && !o.suspended)
}

/// The signed-in person as a member of `org`. A token is for publishing, not for managing
/// people, so only a browser session passes; everything else looks like a missing organization.
fn member<'a>(
    state: &'a State,
    who: &Principal,
    org: &str,
) -> Result<(&'a User, OrgRole), AccessError> {
    let Principal::User(user) = who else {
        return Err(AccessError::NotFound);
    };
    if state.tenancy != Tenancy::Hosted || user.token_org.is_some() {
        return Err(AccessError::NotFound);
    }
    let record = state
        .users
        .iter()
        .find(|r| r.user.id == user.id && !r.user.disabled)
        .ok_or(AccessError::NotFound)?;
    active_org(state, org).ok_or(AccessError::NotFound)?;
    let role = state
        .memberships
        .iter()
        .find(|m| m.person == user.id && m.org == org)
        .map(|m| m.role)
        .ok_or(AccessError::NotFound)?;
    Ok((&record.user, role))
}

fn manager<'a>(
    state: &'a State,
    who: &Principal,
    org: &str,
) -> Result<(&'a User, OrgRole), AccessError> {
    let (user, role) = member(state, who, org)?;
    if manages(role) {
        Ok((user, role))
    } else {
        Err(AccessError::Invalid(
            "Only an owner or admin of this organization can do that.".into(),
        ))
    }
}

/// Whether `person` may share the report: its owner while a member, an owner or admin of its
/// organization, or someone holding an owner grant.
fn may_share(state: &State, person: &str, key: &str) -> bool {
    let Some(acl) = state.acls.get(key) else {
        return false;
    };
    if active_org(state, &acl.org).is_none()
        || !state
            .users
            .iter()
            .any(|r| r.user.id == person && !r.user.disabled)
    {
        return false;
    }
    let membership = state
        .memberships
        .iter()
        .find(|m| m.person == person && m.org == acl.org);
    membership.is_some_and(|m| manages(m.role))
        || (membership.is_some() && acl.owner == person)
        || acl
            .grants
            .iter()
            .any(|g| g.user == person && g.role == ReportRole::Owner)
}

/// Whether the sender could still give today what the invitation offers.
fn offer_stands(state: &State, invitation: &Invitation) -> bool {
    if active_org(state, &invitation.org).is_none()
        || !state
            .users
            .iter()
            .any(|r| r.user.id == invitation.inviter && !r.user.disabled)
    {
        return false;
    }
    match (&invitation.role, &invitation.report) {
        (Some(role), None) => state.memberships.iter().any(|m| {
            m.person == invitation.inviter
                && m.org == invitation.org
                && manages(m.role)
                && rank(m.role) >= rank(*role)
        }),
        (None, Some(key)) => {
            invitation
                .report_role
                .is_some_and(|r| r != ReportRole::Owner)
                && state
                    .acls
                    .get(key)
                    .is_some_and(|acl| acl.org == invitation.org)
                && may_share(state, &invitation.inviter, key)
        }
        _ => false,
    }
}

/// Gives `person` what the invitation offers and removes it.
fn apply(state: &mut State, invitation: &Invitation, person: &str) -> Result<Landing, AccessError> {
    if !offer_stands(state, invitation) {
        return Err(AccessError::Credentials);
    }
    let landing = match (&invitation.role, &invitation.report) {
        (Some(role), None) => {
            let already = state
                .memberships
                .iter()
                .any(|m| m.person == person && m.org == invitation.org);
            if !already {
                if state
                    .memberships
                    .iter()
                    .filter(|m| m.org == invitation.org)
                    .count()
                    >= MAX_ORG_SEATS
                {
                    return Err(AccessError::Invalid(format!(
                        "This organization has reached its limit of {MAX_ORG_SEATS} members."
                    )));
                }
                if state
                    .memberships
                    .iter()
                    .filter(|m| m.person == person)
                    .count()
                    >= MAX_MEMBERSHIPS
                {
                    return Err(AccessError::Invalid(format!(
                        "An account can belong to at most {MAX_MEMBERSHIPS} organizations."
                    )));
                }
                state.memberships.push(Membership {
                    person: person.to_owned(),
                    org: invitation.org.clone(),
                    role: *role,
                    created: stamp(),
                });
            }
            Landing::Organization(invitation.org.clone())
        }
        (None, Some(key)) => {
            let role = invitation.report_role.ok_or(AccessError::Credentials)?;
            let acl = state.acls.get(key).ok_or(AccessError::Credentials)?;
            let owner = acl.owner.clone();
            let mut grants = acl.grants.clone();
            // A grant someone gave this person directly stands as it is: an offer made to
            // their address never raises it.
            if !grants.iter().any(|g| g.user == person) {
                grants.push(Grant {
                    user: person.to_owned(),
                    role,
                });
            }
            let grants = hosted_grants(grants, &owner)?;
            // The offer being accepted stops counting as pending in the same step.
            let consumed = usize::from(
                state
                    .invitations
                    .iter()
                    .any(|i| i.id == invitation.id && i.expires > now()),
            );
            hosted_grant_capacity(
                state,
                &invitation.org,
                key,
                grants.len(),
                pending_shares(state, key).saturating_sub(consumed),
            )?;
            if let Some(acl) = state.acls.get_mut(key) {
                acl.grants = grants;
            }
            Landing::Report(key.clone())
        }
        _ => return Err(AccessError::Credentials),
    };
    state.invitations.retain(|i| i.id != invitation.id);
    Ok(landing)
}

fn view(state: &State, invitation: &Invitation) -> Option<InviteView> {
    let org = active_org(state, &invitation.org)?;
    let inviter = state
        .users
        .iter()
        .find(|r| r.user.id == invitation.inviter)?;
    Some(InviteView {
        invitation: invitation.clone(),
        org_name: org.name.clone(),
        inviter_name: inviter.user.name.clone(),
        inviter_email: inviter.user.email.clone(),
        account_exists: state
            .users
            .iter()
            .any(|r| r.user.email.eq_ignore_ascii_case(&invitation.email)),
    })
}

fn new_invitation_id(state: &State) -> Result<String, AccessError> {
    loop {
        let id = format!(
            "i-{}",
            random_text(9)?.to_lowercase().replace(['_', '-'], "x")
        );
        if !state.invitations.iter().any(|i| i.id == id) {
            return Ok(id);
        }
    }
}

fn new_org_id(state: &State) -> Result<String, AccessError> {
    loop {
        let mut bytes = [0u8; 8];
        getrandom::fill(&mut bytes).map_err(|e| AccessError::Storage(e.to_string()))?;
        if bytes.iter().any(|b| *b >= 252) {
            continue;
        }
        let alphabet = b"abcdefghijklmnopqrstuvwxyz0123456789";
        let candidate: String = bytes
            .iter()
            .map(|b| alphabet[(*b % 36) as usize] as char)
            .collect();
        if !state.organizations.iter().any(|o| o.id == candidate) {
            return Ok(candidate);
        }
    }
}

fn last_owner(state: &State, org: &str, person: &str) -> bool {
    !state
        .memberships
        .iter()
        .any(|m| m.org == org && m.person != person && m.role == OrgRole::Owner)
}

impl Access {
    // ------------------------------------------------------------ organizations

    pub fn organization(&self, org: &str) -> Option<Organization> {
        self.read(|state| state.organizations.iter().find(|o| o.id == org).cloned())
    }

    /// The organization this browser session works in: the one last chosen while the person
    /// still belongs to it, otherwise the one they joined first. `None` for a person in none.
    pub fn current_organization(&self, session_cookie: &str) -> Option<String> {
        let wanted = digest(session_cookie);
        self.read(|state| {
            let session = state.sessions.iter().find(|s| s.sha256 == wanted)?;
            let mut mine: Vec<&Membership> = state
                .memberships
                .iter()
                .filter(|m| m.person == session.user)
                .collect();
            mine.sort_by(|a, b| a.created.cmp(&b.created));
            session
                .org
                .as_ref()
                .filter(|org| mine.iter().any(|m| &m.org == *org))
                .cloned()
                .or_else(|| mine.first().map(|m| m.org.clone()))
        })
    }

    /// Switch this browser session to another of the person's organizations.
    pub fn select_organization(
        &self,
        caller: impl FnOnce() -> Principal,
        session_cookie: &str,
        org: &str,
    ) -> Result<(), AccessError> {
        let wanted = digest(session_cookie);
        self.with_caller(caller, |state, who| {
            let user = state
                .sessions
                .iter()
                .find(|s| s.sha256 == wanted && s.scope.is_empty())
                .map(|s| s.user.clone())
                .ok_or(AccessError::NotFound)?;
            if !matches!(who, Principal::User(person) if person.id == user && person.token_org.is_none())
                || !state
                .memberships
                .iter()
                .any(|m| m.person == user && m.org == org)
            {
                return Err(AccessError::NotFound);
            }
            if let Some(session) = state.sessions.iter_mut().find(|s| s.sha256 == wanted) {
                session.org = Some(org.to_owned());
            }
            Ok(())
        })
    }

    /// A person who owns no organization may start one, so nobody is left unable to publish.
    pub fn create_organization(
        &self,
        caller: impl FnOnce() -> Principal,
        name: &str,
    ) -> Result<Organization, AccessError> {
        let name = clean_name(name)
            .ok_or_else(|| AccessError::Invalid("A name has 1 to 80 characters.".into()))?;
        self.with_caller(caller, |state, who| {
            let Principal::User(user) = who else {
                return Err(AccessError::NotFound);
            };
            if state.tenancy != Tenancy::Hosted
                || user.token_org.is_some()
                || !state
                    .users
                    .iter()
                    .any(|r| r.user.id == user.id && !r.user.disabled)
            {
                return Err(AccessError::NotFound);
            }
            let mine = || state.memberships.iter().filter(|m| m.person == user.id);
            if mine().any(|m| m.role == OrgRole::Owner) {
                return Err(AccessError::Invalid(
                    "You already own an organization.".into(),
                ));
            }
            let creations = state.org_creations.get(&user.id).copied().unwrap_or(1);
            if creations
                >= self
                    .org_creation_limit
                    .load(std::sync::atomic::Ordering::Relaxed)
            {
                return Err(AccessError::Invalid(
                    "You have reached the organization creation limit.".into(),
                ));
            }
            if mine().count() >= MAX_MEMBERSHIPS {
                return Err(AccessError::Invalid(format!(
                    "An account can belong to at most {MAX_MEMBERSHIPS} organizations."
                )));
            }
            let created = stamp();
            let organization = Organization {
                id: new_org_id(state)?,
                name,
                created: created.clone(),
                suspended: false,
                limits: OrganizationLimits::default(),
            };
            state.memberships.push(Membership {
                person: user.id.clone(),
                org: organization.id.clone(),
                role: OrgRole::Owner,
                created,
            });
            state.organizations.push(organization.clone());
            state.org_creations.insert(user.id.clone(), creations + 1);
            Ok(organization)
        })
    }

    pub fn rename_organization(
        &self,
        caller: impl FnOnce() -> Principal,
        org: &str,
        name: &str,
    ) -> Result<(), AccessError> {
        let name = clean_name(name)
            .ok_or_else(|| AccessError::Invalid("A name has 1 to 80 characters.".into()))?;
        self.with_caller(caller, |state, who| {
            let (_, role) = member(state, who, org)?;
            if role != OrgRole::Owner {
                return Err(AccessError::Invalid(
                    "Only an owner can rename the organization.".into(),
                ));
            }
            if let Some(organization) = state.organizations.iter_mut().find(|o| o.id == org) {
                organization.name = name;
            }
            Ok(())
        })
    }

    // ------------------------------------------------------------ members

    /// The people of an organization, for any of its members.
    pub fn members(&self, who: &Principal, org: &str) -> Result<Vec<MemberRow>, AccessError> {
        self.read(|state| {
            member(state, who, org)?;
            let mut rows: Vec<MemberRow> = state
                .memberships
                .iter()
                .filter(|m| m.org == org)
                .filter_map(|m| {
                    let user = &state.users.iter().find(|r| r.user.id == m.person)?.user;
                    Some(MemberRow {
                        person: user.id.clone(),
                        name: user.name.clone(),
                        email: user.email.clone(),
                        role: m.role,
                        created: m.created.clone(),
                    })
                })
                .collect();
            rows.sort_by(|a, b| {
                rank(b.role)
                    .cmp(&rank(a.role))
                    .then_with(|| a.created.cmp(&b.created))
            });
            Ok(rows)
        })
    }

    /// Open invitations to join the organization, for its owners and admins.
    pub fn invitations(&self, who: &Principal, org: &str) -> Result<Vec<InviteView>, AccessError> {
        let at = now();
        self.read(|state| {
            manager(state, who, org)?;
            Ok(state
                .invitations
                .iter()
                .filter(|i| i.org == org && i.role.is_some() && i.expires > at)
                .filter_map(|i| view(state, i))
                .collect())
        })
    }

    /// Invite an address to the organization. The role is at most the sender's own.
    pub fn invite_member(
        &self,
        caller: impl FnOnce() -> Principal,
        org: &str,
        email: &str,
        role: OrgRole,
    ) -> Result<Invitation, AccessError> {
        let email = crate::email::normalize(email)
            .ok_or_else(|| AccessError::Invalid("That is not an email address.".into()))?;
        let at = now();
        self.with_caller(caller, |state, who| {
            let (user, mine) = manager(state, who, org)?;
            let inviter = user.id.clone();
            if rank(role) > rank(mine) {
                return Err(AccessError::Invalid(
                    "You cannot invite someone to a role above your own.".into(),
                ));
            }
            // Members' addresses are on the members page already, so this says nothing new.
            if state.memberships.iter().any(|m| {
                m.org == org
                    && state.users.iter().any(|r| {
                        r.user.id == m.person && r.user.email.eq_ignore_ascii_case(&email)
                    })
            }) {
                return Err(AccessError::Invalid(format!(
                    "{email} is already a member."
                )));
            }
            state.invitations.retain(|i| {
                i.expires > at && !(i.org == org && i.role.is_some() && i.email == email)
            });
            let pending = state
                .invitations
                .iter()
                .filter(|i| i.org == org && i.role.is_some())
                .count();
            let members = state.memberships.iter().filter(|m| m.org == org).count();
            if pending >= MAX_ORG_INVITATIONS || members + pending >= MAX_ORG_SEATS {
                return Err(AccessError::Invalid(format!(
                    "This organization has reached its limit of {MAX_ORG_SEATS} members or {MAX_ORG_INVITATIONS} open invitations."
                )));
            }
            let invitation = Invitation {
                id: new_invitation_id(state)?,
                org: org.to_owned(),
                email,
                role: Some(role),
                report: None,
                report_role: None,
                inviter,
                created: at,
                expires: at + INVITATION_SECONDS,
            };
            state.invitations.push(invitation.clone());
            Ok(invitation)
        })
    }

    pub fn cancel_invitation(
        &self,
        caller: impl FnOnce() -> Principal,
        org: &str,
        id: &str,
    ) -> Result<(), AccessError> {
        self.with_caller(caller, |state, who| {
            manager(state, who, org)?;
            let before = state.invitations.len();
            state
                .invitations
                .retain(|i| !(i.id == id && i.org == org && i.role.is_some()));
            if state.invitations.len() == before {
                return Err(AccessError::NotFound);
            }
            Ok(())
        })
    }

    /// Keep an invitation open for another week, in the sender's name, to mail it again.
    pub fn renew_invitation(
        &self,
        caller: impl FnOnce() -> Principal,
        org: &str,
        id: &str,
    ) -> Result<Invitation, AccessError> {
        #[cfg(test)]
        crate::test_points::reach("renew.gating", id);
        let at = now();
        self.with_caller(caller, |state, who| {
            let (user, mine) = manager(state, who, org)?;
            let inviter = user.id.clone();
            let invitation = state
                .invitations
                .iter_mut()
                .find(|i| i.id == id && i.org == org && i.role.is_some())
                .ok_or(AccessError::NotFound)?;
            if invitation.role.is_some_and(|role| rank(role) > rank(mine)) {
                return Err(AccessError::Invalid(
                    "You cannot invite someone to a role above your own.".into(),
                ));
            }
            invitation.inviter = inviter;
            invitation.expires = at + INVITATION_SECONDS;
            Ok(invitation.clone())
        })
    }

    /// Change a member's role. Nobody changes a member who outranks them or hands out a role
    /// above their own, and the last owner stays an owner.
    pub fn set_member_role(
        &self,
        caller: impl FnOnce() -> Principal,
        org: &str,
        person: &str,
        role: OrgRole,
    ) -> Result<(), AccessError> {
        self.with_caller(caller, |state, who| {
            let (_, mine) = manager(state, who, org)?;
            let current = state
                .memberships
                .iter()
                .find(|m| m.person == person && m.org == org)
                .map(|m| m.role)
                .ok_or(AccessError::NotFound)?;
            if rank(current) > rank(mine) || rank(role) > rank(mine) {
                return Err(AccessError::Invalid(
                    "You cannot change a role above your own.".into(),
                ));
            }
            if current == OrgRole::Owner && role != OrgRole::Owner && last_owner(state, org, person)
            {
                return Err(AccessError::Invalid(
                    "An organization needs an owner. Make someone else an owner first.".into(),
                ));
            }
            if let Some(membership) = state
                .memberships
                .iter_mut()
                .find(|m| m.person == person && m.org == org)
            {
                membership.role = role;
            }
            Ok(())
        })
    }

    /// Remove a member, or leave. Their tokens for the organization and their grants on its
    /// reports end, and their reports there pass to the person removing them, or to the
    /// longest-standing owner when they leave by themselves.
    pub fn remove_member(
        &self,
        caller: impl FnOnce() -> Principal,
        org: &str,
        person: &str,
    ) -> Result<(), AccessError> {
        self.with_caller(caller, |state, who| {
            let (user, mine) = member(state, who, org)?;
            let actor = user.id.clone();
            let target = state
                .memberships
                .iter()
                .find(|m| m.person == person && m.org == org)
                .map(|m| m.role)
                .ok_or(AccessError::NotFound)?;
            if actor != person && (!manages(mine) || rank(target) > rank(mine)) {
                return Err(AccessError::Invalid(
                    "Only an owner or admin can remove a member, and never one above their own role.".into(),
                ));
            }
            if target == OrgRole::Owner && last_owner(state, org, person) {
                return Err(AccessError::Invalid(
                    "An organization needs an owner. Make someone else an owner first.".into(),
                ));
            }
            let next_owner = if actor != person {
                actor
            } else {
                heir(state, org, person).ok_or_else(|| {
                    AccessError::Invalid(
                        "An organization needs an owner. Make someone else an owner first.".into(),
                    )
                })?
            };
            state
                .memberships
                .retain(|m| !(m.person == person && m.org == org));
            if let Some(record) = state.users.iter_mut().find(|r| r.user.id == person) {
                record.tokens.retain(|t| t.org != org);
            }
            for acl in state.acls.values_mut().filter(|acl| acl.org == org) {
                acl.grants.retain(|g| g.user != person);
                if acl.owner == person {
                    acl.owner = next_owner.clone();
                    acl.grants.retain(|g| g.user != next_owner);
                }
            }
            // What they sent ends with them, and so does what is still addressed to them here:
            // an open share would otherwise hand the report back the next time they sign in.
            let address = state
                .users
                .iter()
                .find(|r| r.user.id == person)
                .map(|r| {
                    crate::email::normalize(&r.user.email).unwrap_or_else(|| r.user.email.clone())
                });
            state.invitations.retain(|i| {
                i.org != org
                    || (i.inviter != person
                        && !address
                            .as_deref()
                            .is_some_and(|address| i.email.eq_ignore_ascii_case(address)))
            });
            for session in state.sessions.iter_mut().filter(|s| s.user == person) {
                if session.org.as_deref() == Some(org) {
                    session.org = None;
                }
            }
            Ok(())
        })
    }

    // ------------------------------------------------------------ invitations, the invitee's side

    /// The invitation behind a mail link, while the link, the invitation and the sender's right
    /// to give it all still hold. Consumes nothing.
    pub fn inspect_invite(&self, token: &str) -> Option<InviteView> {
        let link = self.links.as_ref()?.peek(token)?;
        if link.purpose != LinkPurpose::Invite {
            return None;
        }
        let id = link.invitation?;
        let at = now();
        self.read(|state| {
            let invitation = state
                .invitations
                .iter()
                .find(|i| i.id == id && i.email == link.email && i.expires > at)?;
            offer_stands(state, invitation).then(|| view(state, invitation))?
        })
    }

    /// The names a mail for this invitation needs.
    pub fn invitation_view(&self, id: &str) -> Option<InviteView> {
        let at = now();
        self.read(|state| {
            let invitation = state
                .invitations
                .iter()
                .find(|i| i.id == id && i.expires > at)?;
            view(state, invitation)
        })
    }

    /// Recheck the sender and invitation after waiting for the mail privacy lock.
    pub(crate) fn invitation_can_be_mailed(&self, id: &str, sender: &str) -> bool {
        self.read(|state| {
            state.invitations.iter().any(|invitation| {
                invitation.id == id
                    && invitation.inviter == sender
                    && invitation.expires > now()
                    && offer_stands(state, invitation)
            })
        })
    }

    /// A one-day link for the invitation, counted against the address's mail limits.
    pub(crate) fn issue_invitation_link(
        &self,
        invitation: &Invitation,
    ) -> Result<Option<String>, String> {
        self.links
            .as_ref()
            .ok_or("Links require hosted mode")?
            .issue_invite(&invitation.email, &invitation.id)
    }

    pub(super) fn load_invite_limits(
        dir: &Path,
    ) -> Result<BTreeMap<String, std::collections::VecDeque<u64>>, String> {
        let path = dir.join("invite-mail-budget.json");
        let meta = match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(BTreeMap::new());
            }
            Err(error) => return Err(error.to_string()),
        };
        if !meta.file_type().is_file() || meta.len() > 4 * 1024 * 1024 {
            return Err("Invalid invitation mail budget".into());
        }
        let bytes = fs::read(&path).map_err(|error| error.to_string())?;
        let counters: BTreeMap<String, std::collections::VecDeque<u64>> =
            serde_json::from_slice(&bytes).map_err(|_| "Invalid invitation mail budget")?;
        if counters.len() > 2048
            || counters.values().any(|times| {
                times.len() > MAX_ORG_INVITE_MAILS_PER_DAY || !times.iter().is_sorted()
            })
        {
            return Err("Invalid invitation mail budget".into());
        }
        Ok(counters)
    }

    pub(super) fn save_invite_limits(
        &self,
        counters: &BTreeMap<String, std::collections::VecDeque<u64>>,
    ) -> Result<(), String> {
        let mut file =
            tempfile::NamedTempFile::new_in(&*self.dir).map_err(|error| error.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.as_file()
                .set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|error| error.to_string())?;
        }
        serde_json::to_writer(&mut file, counters).map_err(|error| error.to_string())?;
        file.flush()
            .and_then(|_| file.as_file().sync_all())
            .map_err(|error| error.to_string())?;
        file.persist(self.dir.join("invite-mail-budget.json"))
            .map_err(|error| error.error.to_string())?;
        sync_directory(&self.dir)
    }

    pub(super) fn purge_expired_invite_mail_budgets(&self) -> Result<(), String> {
        let mut counters = self
            .invite_limits
            .lock()
            .map_err(|_| "Invitation counter poisoned")?;
        let mut next = counters.clone();
        let at = now();
        next.retain(|_, times| {
            while times
                .front()
                .is_some_and(|time| *time <= at.saturating_sub(86_400))
            {
                times.pop_front();
            }
            !times.is_empty()
        });
        if next != *counters {
            self.save_invite_limits(&next)?;
            *counters = next;
        }
        Ok(())
    }

    /// Bound the invitation and share mails one person, and one organization, cause in a day.
    /// Both are counted or neither, so a refusal for the organization costs the person nothing.
    pub(crate) fn admit_invite_mail(&self, person: &str, org: &str) -> Result<(), AccessError> {
        let at = now();
        let mut counters = self
            .invite_limits
            .lock()
            .map_err(|_| AccessError::Storage("Invitation counter poisoned".into()))?;
        let mut next = counters.clone();
        next.retain(|_, times| {
            while times
                .front()
                .is_some_and(|time| *time <= at.saturating_sub(86_400))
            {
                times.pop_front();
            }
            !times.is_empty()
        });
        // Identifiers hold no space, so an organization's key never meets a person's.
        let organization = format!("org {org}");
        let limits = [
            (person, MAX_INVITE_MAILS_PER_DAY),
            (organization.as_str(), MAX_ORG_INVITE_MAILS_PER_DAY),
        ];
        let new = limits
            .iter()
            .filter(|(key, _)| !next.contains_key(*key))
            .count();
        if next.len() + new > 2048
            || limits
                .iter()
                .any(|(key, limit)| next.get(*key).is_some_and(|times| times.len() >= *limit))
        {
            return Err(AccessError::Throttled);
        }
        for (key, _) in limits {
            next.entry(key.to_owned()).or_default().push_back(at);
        }
        // Keep the in-memory count conservative even if the rename succeeded but sync failed.
        let saved = self.save_invite_limits(&next);
        *counters = next;
        saved.map_err(AccessError::Storage)
    }

    /// Invitations to join an organization that are addressed to the signed-in person's own
    /// verified address.
    pub fn invitations_for(&self, who: &Principal) -> Vec<InviteView> {
        let Principal::User(user) = who else {
            return vec![];
        };
        let at = now();
        self.read(|state| {
            let Some(record) = state
                .users
                .iter()
                .find(|r| r.user.id == user.id && !r.user.disabled)
            else {
                return vec![];
            };
            state
                .invitations
                .iter()
                .filter(|i| {
                    i.role.is_some()
                        && i.expires > at
                        && i.email.eq_ignore_ascii_case(&record.user.email)
                        && offer_stands(state, i)
                })
                .filter_map(|i| view(state, i))
                .collect()
        })
    }

    /// Join an organization from the signed-in person's own list of invitations. The session of
    /// the verified address is the proof; no mail link is needed.
    pub fn accept_invitation(
        &self,
        caller: impl FnOnce() -> Principal,
        id: &str,
    ) -> Result<Landing, AccessError> {
        let at = now();
        self.with_caller(caller, |state, who| {
            let Principal::User(user) = who else {
                return Err(AccessError::NotFound);
            };
            if user.token_org.is_some() {
                return Err(AccessError::NotFound);
            }
            let email = state
                .users
                .iter()
                .find(|r| r.user.id == user.id && !r.user.disabled)
                .map(|r| r.user.email.clone())
                .ok_or(AccessError::NotFound)?;
            let invitation = state
                .invitations
                .iter()
                .find(|i| i.id == id && i.expires > at && i.email.eq_ignore_ascii_case(&email))
                .cloned()
                .ok_or(AccessError::NotFound)?;
            apply(state, &invitation, &user.id).map_err(|error| match error {
                AccessError::Credentials => AccessError::NotFound,
                other => other,
            })
        })
    }

    /// Turn the reports shared with the signed-in person's address into grants. Returns how
    /// many; writes nothing when there are none.
    pub fn claim_shares(&self, caller: impl FnOnce() -> Principal) -> usize {
        let Ok(_gate) = self.gate() else {
            return 0;
        };
        let who = &caller();
        let Principal::User(user) = who else {
            return 0;
        };
        if user.token_org.is_some() {
            return 0;
        }
        let at = now();
        let waiting = self.read(|state| {
            state
                .users
                .iter()
                .find(|r| r.user.id == user.id && !r.user.disabled)
                .is_some_and(|record| {
                    state.invitations.iter().any(|i| {
                        i.report.is_some()
                            && i.expires > at
                            && i.email.eq_ignore_ascii_case(&record.user.email)
                    })
                })
        });
        if !waiting {
            return 0;
        }
        self.with_gated(|state| {
            let email = state
                .users
                .iter()
                .find(|r| r.user.id == user.id && !r.user.disabled)
                .map(|r| r.user.email.clone())
                .ok_or(AccessError::NotFound)?;
            let mine: Vec<Invitation> = state
                .invitations
                .iter()
                .filter(|i| {
                    i.report.is_some() && i.expires > at && i.email.eq_ignore_ascii_case(&email)
                })
                .cloned()
                .collect();
            let mut claimed = 0;
            for invitation in mine {
                // An offer its sender can no longer make, or one over a limit, is dropped.
                if apply(state, &invitation, &user.id).is_ok() {
                    claimed += 1;
                } else {
                    state.invitations.retain(|i| i.id != invitation.id);
                }
            }
            Ok(claimed)
        })
        .unwrap_or(0)
    }

    /// Spend an invitation link. An address without an account gets one, with the name and
    /// password given, and is signed in; an existing account just receives what was offered and
    /// signs in as usual, so a mail link never opens a session on an existing account.
    pub(crate) async fn accept_invitation_link(
        &self,
        token: &str,
        name: &str,
        password: &str,
        publication: Option<tokio::sync::OwnedMutexGuard<()>>,
    ) -> Result<Accepted, AccessError> {
        let permit = self
            .registration_workers
            .clone()
            .try_acquire_owned()
            .map_err(|_| AccessError::Throttled)?;
        let privacy = self.mail_privacy.clone().lock_owned().await;
        let link = self
            .links
            .as_ref()
            .and_then(|links| links.peek(token))
            .filter(|link| link.purpose == LinkPurpose::Invite)
            .ok_or(AccessError::Credentials)?;
        let known = self.hosted_email_exists(&link.email);
        let name = clean_name(name);
        if !known {
            if name.is_none() || password.len() > MAX_PASSWORD_BYTES {
                return Err(AccessError::Invalid("Check the name and password.".into()));
            }
            if password.chars().count() < MIN_PASSWORD {
                return Err(AccessError::Invalid(short_password()));
            }
            if !self.signup_account_capacity() {
                return Err(AccessError::Throttled);
            }
        }
        let access = self.clone();
        let token = token.to_owned();
        let password = password.to_owned();
        let session = (random_text(32)?, random_text(24)?);
        tokio::task::spawn_blocking(move || {
            let _held = (permit, privacy, publication);
            let password_hash = if known {
                None
            } else {
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
                        Err(std::sync::TryLockError::WouldBlock) => {
                            return Err(AccessError::Throttled);
                        }
                    }
                };
                Some(new_password_hash(&password)?)
            };
            let at = now();
            access.with(|state| {
                if state.tenancy != Tenancy::Hosted {
                    return Err(AccessError::Credentials);
                }
                let link = access
                    .links
                    .as_ref()
                    .ok_or(AccessError::Credentials)?
                    .take(&token)?
                    .filter(|link| link.purpose == LinkPurpose::Invite)
                    .ok_or(AccessError::Credentials)?;
                let id = link.invitation.ok_or(AccessError::Credentials)?;
                let invitation = state
                    .invitations
                    .iter()
                    .find(|i| i.id == id && i.email == link.email && i.expires > at)
                    .cloned()
                    .ok_or(AccessError::Credentials)?;
                let existing = state
                    .users
                    .iter()
                    .find(|r| r.user.email.eq_ignore_ascii_case(&invitation.email))
                    .map(|r| r.user.clone());
                let (user, created) = match existing {
                    Some(user) if user.disabled => return Err(AccessError::Credentials),
                    Some(user) => (user, false),
                    None => {
                        let (Some(password_hash), Some(name)) = (password_hash, name) else {
                            return Err(AccessError::Credentials);
                        };
                        if state.signups_paused
                            || state.users.len()
                                >= access
                                    .signup_account_limit
                                    .load(std::sync::atomic::Ordering::Relaxed)
                        {
                            return Err(AccessError::Throttled);
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
                        let created = stamp();
                        let user = User {
                            id,
                            name,
                            email: invitation.email.clone(),
                            role: invitation.role.unwrap_or(OrgRole::Reader),
                            disabled: false,
                            created: created.clone(),
                            verified: Some(created),
                            operator: false,
                            token_org: None,
                        };
                        state.org_creations.insert(user.id.clone(), 0);
                        state.users.push(UserRecord {
                            user: user.clone(),
                            password: Some(password_hash),
                            tokens: vec![],
                        });
                        (user, true)
                    }
                };
                let landing = apply(state, &invitation, &user.id)?;
                let session = if created {
                    state.sessions.retain(|s| s.expires > at);
                    state.sessions.push(Session {
                        sha256: digest(&session.0),
                        csrf: session.1.clone(),
                        user: user.id.clone(),
                        expires: at + SESSION_SECONDS,
                        scope: String::new(),
                        org: match &landing {
                            Landing::Organization(org) => Some(org.clone()),
                            Landing::Report(_) => None,
                        },
                    });
                    Some(session)
                } else {
                    None
                };
                Ok(Accepted {
                    user,
                    session,
                    landing,
                })
            })
        })
        .await
        .map_err(|_| AccessError::Storage("Invitation worker failed".into()))?
    }

    // ------------------------------------------------------------ sharing one report

    /// Share a report with an address as viewer or editor. A person already named on it has
    /// their role changed; anyone else gets a pending invitation, account or not, and `Some` is
    /// returned so a mail can follow.
    pub fn share_with_email(
        &self,
        caller: impl FnOnce() -> Principal,
        key: &str,
        email: &str,
        role: ReportRole,
    ) -> Result<Option<Invitation>, AccessError> {
        if role == ReportRole::Owner {
            return Err(AccessError::Invalid("Choose viewer or editor.".into()));
        }
        let email = crate::email::normalize(email)
            .ok_or_else(|| AccessError::Invalid("That is not an email address.".into()))?;
        #[cfg(test)]
        crate::test_points::reach("share.gating", key);
        // Who is sharing is read under the lock every access change takes and that lock is
        // kept through the write: a sign-out either came before, and nothing is shared, or
        // comes after.
        let _gate = self.gate()?;
        let who = &caller();
        let Principal::User(user) = who else {
            return Err(AccessError::NotFound);
        };
        let at = now();
        self.with_gated(|state| {
            check_hosted_share(state, key, Some(who))?;
            if state.tenancy != Tenancy::Hosted || user.token_org.is_some() {
                return Err(AccessError::NotFound);
            }
            let acl = state.acls.get(key).ok_or(AccessError::NotFound)?.clone();
            let address_of = |id: &str| {
                state
                    .users
                    .iter()
                    .find(|r| r.user.id == id)
                    .map(|r| r.user.email.clone())
            };
            // The owner and the people already named are on the page, so these say nothing new.
            if address_of(&acl.owner).is_some_and(|a| a.eq_ignore_ascii_case(&email)) {
                return Err(AccessError::Invalid(format!("{email} owns this report.")));
            }
            if let Some(named) = acl
                .grants
                .iter()
                .find(|g| address_of(&g.user).is_some_and(|a| a.eq_ignore_ascii_case(&email)))
                .map(|g| g.user.clone())
            {
                if let Some(grant) = state
                    .acls
                    .get_mut(key)
                    .and_then(|acl| acl.grants.iter_mut().find(|g| g.user == named))
                {
                    grant.role = role;
                }
                return Ok(None);
            }
            state.invitations.retain(|i| {
                i.expires > at && !(i.report.as_deref() == Some(key) && i.email == email)
            });
            hosted_grant_capacity(
                state,
                &acl.org,
                key,
                acl.grants.len(),
                pending_shares(state, key) + 1,
            )?;
            let invitation = Invitation {
                id: new_invitation_id(state)?,
                org: acl.org.clone(),
                email,
                role: None,
                report: Some(key.to_owned()),
                report_role: Some(role),
                inviter: user.id.clone(),
                created: at,
                expires: at + INVITATION_SECONDS,
            };
            state.invitations.push(invitation.clone());
            Ok(Some(invitation))
        })
    }

    /// Take an address off a report: its grant and any invitation still waiting.
    pub fn unshare_email(
        &self,
        caller: impl FnOnce() -> Principal,
        key: &str,
        email: &str,
    ) -> Result<(), AccessError> {
        let email = crate::email::normalize(email)
            .ok_or_else(|| AccessError::Invalid("That is not an email address.".into()))?;
        #[cfg(test)]
        crate::test_points::reach("unshare.gating", key);
        // As in `share_with_email`: the caller is read under the lock the write holds.
        let _gate = self.gate()?;
        let who = &caller();
        if !matches!(who, Principal::User(user) if user.token_org.is_none()) {
            return Err(AccessError::NotFound);
        }
        self.with_gated(|state| {
            check_hosted_share(state, key, Some(who))?;
            let people: Vec<String> = state
                .users
                .iter()
                .filter(|r| r.user.email.eq_ignore_ascii_case(&email))
                .map(|r| r.user.id.clone())
                .collect();
            state
                .invitations
                .retain(|i| !(i.report.as_deref() == Some(key) && i.email == email));
            if let Some(acl) = state.acls.get_mut(key) {
                acl.grants.retain(|g| !people.contains(&g.user));
            }
            Ok(())
        })
    }

    /// Who is on a report, for a caller the handler has already found to hold `share`.
    pub fn report_access(&self, key: &str) -> Option<ReportAccess> {
        let at = now();
        self.read(|state| {
            let acl = state.acls.get(key)?;
            let org = state.organizations.iter().find(|o| o.id == acl.org)?;
            let person = |id: &str| {
                state
                    .users
                    .iter()
                    .find(|r| r.user.id == id)
                    .map(|r| &r.user)
            };
            let in_org = |id: &str| {
                state
                    .memberships
                    .iter()
                    .any(|m| m.person == id && m.org == acl.org)
            };
            let mut named: Vec<AccessRow> = acl
                .grants
                .iter()
                .filter_map(|g| {
                    let user = person(&g.user)?;
                    Some(AccessRow {
                        email: user.email.clone(),
                        name: in_org(&user.id).then(|| user.name.clone()),
                        role: g.role,
                        pending: false,
                    })
                })
                .collect();
            named.extend(
                state
                    .invitations
                    .iter()
                    .filter(|i| i.report.as_deref() == Some(key) && i.expires > at)
                    .filter_map(|i| {
                        Some(AccessRow {
                            email: i.email.clone(),
                            name: None,
                            role: i.report_role?,
                            pending: true,
                        })
                    }),
            );
            named.sort_by(|a, b| a.email.cmp(&b.email));
            let owner = person(&acl.owner);
            Some(ReportAccess {
                org_name: org.name.clone(),
                owner_name: owner.map(|u| u.name.clone()).unwrap_or_default(),
                owner_email: owner.map(|u| u.email.clone()).unwrap_or_default(),
                visibility: acl.visibility,
                named,
            })
        })
    }

    /// The reports a signed-in person's list shows, and why each is there: their own, their
    /// organizations', and those shared with them by name. A public link alone lists nothing.
    pub fn visible_reports(&self, who: &Principal) -> Vec<VisibleReport> {
        let Principal::User(user) = who else {
            return vec![];
        };
        if user.token_org.is_some() {
            return vec![];
        }
        self.read(|state| {
            if !state
                .users
                .iter()
                .any(|r| r.user.id == user.id && !r.user.disabled)
            {
                return vec![];
            }
            state
                .acls
                .iter()
                .filter_map(|(key, acl)| {
                    let org = active_org(state, &acl.org)?;
                    let role = state
                        .memberships
                        .iter()
                        .find(|m| m.person == user.id && m.org == acl.org)
                        .map(|m| m.role);
                    let granted = acl.grants.iter().any(|g| g.user == user.id);
                    let relation = match role {
                        Some(_) if acl.owner == user.id => Relation::Mine,
                        Some(role)
                            if manages(role)
                                || granted
                                || acl.visibility != Visibility::Private =>
                        {
                            Relation::Organization
                        }
                        None if granted => Relation::Shared,
                        _ => return None,
                    };
                    Some(VisibleReport {
                        key: key.clone(),
                        org: acl.org.clone(),
                        org_name: org.name.clone(),
                        owner_name: state
                            .users
                            .iter()
                            .find(|r| r.user.id == acl.owner)
                            .map(|r| r.user.name.clone())
                            .unwrap_or_default(),
                        visibility: acl.visibility,
                        relation,
                        named: granted,
                    })
                })
                .collect()
        })
    }
}

#[cfg(test)]
mod invite_budget_tests {
    use super::*;

    #[tokio::test]
    async fn replaying_a_journaled_mail_does_not_charge_invitation_limits_again() {
        use std::{
            future::Future,
            pin::Pin,
            sync::atomic::{AtomicUsize, Ordering},
        };
        struct Provider(AtomicUsize);
        impl crate::mail::Transport for Provider {
            fn durable(&self) -> bool {
                true
            }
            fn send<'a>(
                &'a self,
                _: &'a crate::mail::Mail,
            ) -> Pin<Box<dyn Future<Output = Result<(), ()>> + Send + 'a>> {
                Box::pin(async move {
                    self.0.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
            }
        }
        let dir = tempfile::tempdir().unwrap();
        {
            let access = Access::open_hosted(dir.path()).unwrap();
            access.admit_invite_mail("person", "org").unwrap();
            let token = access
                .issue_link("person@example.test", LinkPurpose::Signup)
                .unwrap()
                .unwrap();
            let at = now();
            let journal = dir.path().join(".access/smtp-outbox");
            fs::create_dir(&journal).unwrap();
            let id = "00000000000000000000000000000010";
            fs::write(journal.join(format!("{id}.request.json")), serde_json::to_vec(&serde_json::json!({"id":id,"to":"person@example.test","kind":"signup","created":at,"expires":at+3600,"status":"pending","subject":"Test","body":"link","token":token})).unwrap()).unwrap();
        }
        let access = Access::open_hosted(dir.path()).unwrap();
        let sender = Arc::new(Provider(AtomicUsize::new(0)));
        access.configure_mail(sender.clone(), 10, false).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while sender.0.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let counters = access.invite_limits.lock().unwrap();
        assert_eq!(counters["person"].len(), 1);
        assert_eq!(counters["org org"].len(), 1);
    }

    #[tokio::test]
    async fn person_and_organization_mail_limits_survive_restart() {
        use std::{
            future::Future,
            pin::Pin,
            sync::atomic::{AtomicUsize, Ordering},
        };
        struct Provider(AtomicUsize);
        impl crate::mail::Transport for Provider {
            fn durable(&self) -> bool {
                true
            }
            fn send<'a>(
                &'a self,
                _: &'a crate::mail::Mail,
            ) -> Pin<Box<dyn Future<Output = Result<(), ()>> + Send + 'a>> {
                Box::pin(async move {
                    self.0.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
            }
        }
        async fn send(access: &Access, person: &str, org: &str, email: String) -> bool {
            if access.admit_invite_mail(person, org).is_err() {
                return false;
            }
            let at = now();
            let invitation = Invitation {
                id: format!("invite-{email}"),
                org: org.into(),
                email: email.clone(),
                role: Some(OrgRole::Reader),
                report: None,
                report_role: None,
                inviter: person.into(),
                created: at,
                expires: at + 86_400,
            };
            let token = access.issue_invitation_link(&invitation).unwrap().unwrap();
            access
                .mail_queue()
                .unwrap()
                .enqueue_link(
                    crate::mail::Mail {
                        to: email,
                        subject: "Test".into(),
                        body: "link".into(),
                        html: None,
                    },
                    token,
                )
                .await
                .is_ok()
        }
        let dir = tempfile::tempdir().unwrap();
        {
            let access = Access::open_hosted(dir.path()).unwrap();
            let sender = Arc::new(Provider(AtomicUsize::new(0)));
            access.configure_mail(sender.clone(), 1000, false).unwrap();
            for index in 0..100 {
                let person = if index < 40 {
                    "person-a"
                } else if index < 80 {
                    "person-b"
                } else {
                    "person-c"
                };
                assert!(
                    send(
                        &access,
                        person,
                        "org-a",
                        format!("recipient-{index}@example.test")
                    )
                    .await
                );
            }
            access.mail_queue().unwrap().flush().await.unwrap();
            assert_eq!(sender.0.load(Ordering::SeqCst), 100);
            let queue = access.mail_queue().unwrap().clone();
            queue.begin_shutdown();
            queue
                .finish_shutdown(tokio::time::Instant::now() + std::time::Duration::from_secs(1))
                .await;
        }
        let access = Access::open_hosted(dir.path()).unwrap();
        access
            .configure_mail(Arc::new(Provider(AtomicUsize::new(0))), 1000, false)
            .unwrap();
        assert!(
            !send(
                &access,
                "person-a",
                "other-org",
                "fresh-person@example.test".into()
            )
            .await
        );
        assert!(
            !send(
                &access,
                "person-d",
                "org-a",
                "fresh-org@example.test".into()
            )
            .await
        );
        assert!(
            send(
                &access,
                "person-d",
                "other-org",
                "allowed@example.test".into()
            )
            .await
        );
    }
}
