//! Provider accounts joined to people: signing in with one, joining one to an account, taking one
//! off again, and making an account for a stranger the provider vouches for.
//!
//! An identity is the provider and the provider's own id for the account (`social`), never an
//! address, so a later change of address at the provider changes nothing here. It belongs to at
//! most one person, and a person has at most one of each provider. Every change happens inside one
//! access transaction, so two requests cannot both win the same identity.
//!
//! A provider's word about an address counts only when the provider vouches for it: GitHub's
//! primary address marked verified, Microsoft's `email` with `xms_edov` true, or Google's `email`
//! with `email_verified` true. Such an address
//! joins the identity to the account that has it at once (Daniel's Q30 answer, 18 Nov 2026), or
//! makes a new account after a confirmation; without one, nothing is created or joined (see
//! `server::hosted_social`).

use super::*;
use crate::social::Provider;

/// A person signed in with a fresh browser session.
pub struct SignedIn {
    pub user: User,
    pub cookie: String,
    pub csrf: String,
}

/// What an invitation accepted with a provider says for an address that already has an account:
/// the link is not proof enough to give an account a lasting new way in.
pub(crate) const PROVIDER_ON_EXISTING: &str = "You already have a RichView account for this address. Accept the invitation as usual, sign in, and connect the provider on the Account page.";

const MAX_LABEL_CHARS: usize = 120;
const MAX_SUBJECT_BYTES: usize = 128;
/// How many removed provider accounts a person remembers; the oldest go first.
const MAX_REMOVED: usize = 16;

/// A provider account taken off a person, by Disconnect or by a password reset that did not keep
/// it. It joins that person again only through Connect while signed in, never by the address its
/// provider vouches for: a reset is for someone who may have lost control of that account.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct RemovedIdentity {
    provider: String,
    subject: String,
}

/// Remembers that `identity` was taken off `record`.
pub(super) fn remember_removed(record: &mut UserRecord, identity: &Identity) {
    let removed = RemovedIdentity {
        provider: identity.provider.clone(),
        subject: identity.subject.clone(),
    };
    record.removed_identities.retain(|have| *have != removed);
    record.removed_identities.push(removed);
    let excess = record.removed_identities.len().saturating_sub(MAX_REMOVED);
    record.removed_identities.drain(..excess);
}

/// How many unsent join notices a person keeps; the oldest go first.
const MAX_UNSENT: usize = 16;

/// A join whose notice mail could not go, because the day's mail budget was spent or the mail
/// queue refused it. The Account page shows it, and it is mailed when the budget allows.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnsentNotice {
    /// The provider's slug.
    pub provider: String,
    /// When the provider was connected, as the mail says it: a UTC minute.
    pub at: String,
}

fn was_removed(record: &UserRecord, identity: &Identity) -> bool {
    record
        .removed_identities
        .iter()
        .any(|have| have.provider == identity.provider && have.subject == identity.subject)
}

fn provider_name(provider: &str) -> &'static str {
    Provider::parse(provider).map_or("That provider", Provider::name)
}

/// The record that owns an identity, if any.
fn owner<'a>(state: &'a State, provider: &str, subject: &str) -> Option<&'a UserRecord> {
    state.users.iter().find(|record| {
        record
            .identities
            .iter()
            .any(|identity| identity.provider == provider && identity.subject == subject)
    })
}

/// Starts a browser session for `person`, keeping at most ten of theirs.
fn start_session(state: &mut State, person: &str, cookie: &str, csrf: &str, at: u64) {
    state.sessions.retain(|s| s.expires > at);
    let mut count = state.sessions.iter().filter(|s| s.user == person).count();
    state.sessions.retain(|s| {
        if s.user == person && count >= 10 {
            count -= 1;
            false
        } else {
            true
        }
    });
    state.sessions.push(Session {
        sha256: digest(cookie),
        csrf: csrf.to_owned(),
        user: person.to_owned(),
        expires: at + SESSION_SECONDS,
        scope: String::new(),
        org: None,
    });
}

/// Joins `identity` to `person`. `Ok(false)` when it was joined already. Refused when it belongs
/// to someone else, or when the person has another account of the same provider.
pub(super) fn attach(
    state: &mut State,
    person: &str,
    identity: Identity,
) -> Result<bool, AccessError> {
    let name = provider_name(&identity.provider);
    if Provider::parse(&identity.provider).is_none()
        || identity.subject.is_empty()
        || identity.subject.len() > MAX_SUBJECT_BYTES
        || !identity.subject.bytes().all(|b| b.is_ascii_graphic())
    {
        return Err(AccessError::Invalid(format!(
            "{name} did not give an account to connect."
        )));
    }
    if let Some(holder) = owner(state, &identity.provider, &identity.subject) {
        return if holder.user.id == person {
            Ok(false)
        } else {
            Err(AccessError::Invalid(format!(
                "That {name} account is already connected to another RichView account."
            )))
        };
    }
    let record = state
        .users
        .iter_mut()
        .find(|record| record.user.id == person && !record.user.disabled)
        .ok_or(AccessError::Credentials)?;
    if record
        .identities
        .iter()
        .any(|have| have.provider == identity.provider)
    {
        return Err(AccessError::Invalid(format!(
            "Another {name} account is already connected to this account. Disconnect it first."
        )));
    }
    // Whoever calls, the connection is dated by the server.
    let connected = if identity.connected.is_empty() {
        stamp()
    } else {
        identity.connected.clone()
    };
    // Connecting it again, signed in or by an invitation, is the person's own choice.
    record
        .removed_identities
        .retain(|have| !(have.provider == identity.provider && have.subject == identity.subject));
    record.identities.push(Identity {
        label: identity.label.chars().take(MAX_LABEL_CHARS).collect(),
        connected,
        ..identity
    });
    Ok(true)
}

impl Access {
    /// Turns on sign-in with a provider, once, before serving requests. Hosted servers only: it
    /// needs accounts with verified addresses, mail and the Account page.
    pub fn configure_social(&self, social: crate::social::Social) -> Result<(), String> {
        if !self.is_hosted() {
            return Err("Sign-in with GitHub, Microsoft or Google requires hosted mode".into());
        }
        self.social
            .set(Arc::new(social))
            .map_err(|_| "Sign-in with a provider is already configured".to_owned())
    }

    /// The providers connected to the account a reset link is for, so the reset page can ask which
    /// to keep. Nothing for any other link.
    pub(crate) fn reset_identities(&self, token: &str) -> Vec<Identity> {
        let Some(link) = self.links.as_ref().and_then(|links| links.peek(token)) else {
            return Vec::new();
        };
        if link.purpose != LinkPurpose::Reset {
            return Vec::new();
        }
        self.read(|state| {
            state
                .users
                .iter()
                .find(|record| {
                    record.user.email.eq_ignore_ascii_case(&link.email) && !record.user.disabled
                })
                .map(|record| record.identities.clone())
                .unwrap_or_default()
        })
    }

    /// The server's sign-in with providers, when it has any.
    pub fn social(&self) -> Option<&Arc<crate::social::Social>> {
        self.social.get()
    }

    /// The person with this address, if any.
    pub(crate) fn user_by_email(&self, email: &str) -> Option<User> {
        let email = email.trim();
        self.read(|state| {
            state
                .users
                .iter()
                .find(|record| record.user.email.eq_ignore_ascii_case(email))
                .map(|record| record.user.clone())
        })
    }

    /// Who owns an identity: the person's id and whether they are disabled.
    pub(crate) fn identity_holder(&self, provider: &str, subject: &str) -> Option<(String, bool)> {
        self.read(|state| {
            owner(state, provider, subject)
                .map(|record| (record.user.id.clone(), record.user.disabled))
        })
    }

    /// The provider accounts joined to a person.
    pub fn identities_of(&self, person: &str) -> Vec<Identity> {
        self.read(|state| {
            state
                .users
                .iter()
                .find(|record| record.user.id == person)
                .map(|record| record.identities.clone())
                .unwrap_or_default()
        })
    }

    /// Whether the person can sign in with a password.
    pub fn has_password(&self, person: &str) -> bool {
        self.read(|state| {
            state
                .users
                .iter()
                .any(|record| record.user.id == person && record.password.is_some())
        })
    }

    /// Signs in the person an identity belongs to. `NotFound` when it belongs to nobody;
    /// `Credentials` when its person is disabled.
    pub(crate) fn sign_in_identity(
        &self,
        provider: &str,
        subject: &str,
    ) -> Result<SignedIn, AccessError> {
        let (cookie, csrf) = (random_text(32)?, random_text(24)?);
        self.with(|state| {
            let record = owner(state, provider, subject).ok_or(AccessError::NotFound)?;
            if record.user.disabled {
                return Err(AccessError::Credentials);
            }
            let user = record.user.clone();
            start_session(state, &user.id, &cookie, &csrf, now());
            Ok(SignedIn {
                user,
                cookie: cookie.clone(),
                csrf: csrf.clone(),
            })
        })
    }

    /// Joins an identity to the signed-in person that `caller` names. The session is the proof,
    /// so the addresses need not match.
    pub(crate) fn join_identity(
        &self,
        caller: impl FnOnce() -> Principal,
        person: &str,
        identity: Identity,
    ) -> Result<(), AccessError> {
        self.with_caller(caller, |state, who| {
            match who {
                Principal::User(user) if user.id == person && user.token_org.is_none() => {}
                _ => return Err(AccessError::Credentials),
            }
            attach(state, person, identity).map(|_| ())
        })
    }

    /// Takes one provider off the signed-in person, unless it is their last way in.
    pub(crate) fn disconnect_identity(
        &self,
        caller: impl FnOnce() -> Principal,
        person: &str,
        provider: &str,
    ) -> Result<(), AccessError> {
        self.with_caller(caller, |state, who| {
            match who {
                Principal::User(user) if user.id == person && user.token_org.is_none() => {}
                _ => return Err(AccessError::Credentials),
            }
            let record = state
                .users
                .iter_mut()
                .find(|record| record.user.id == person && !record.user.disabled)
                .ok_or(AccessError::Credentials)?;
            let at = record
                .identities
                .iter()
                .position(|identity| identity.provider == provider)
                .ok_or(AccessError::NotFound)?;
            if record.password.is_none() && record.identities.len() <= 1 {
                return Err(AccessError::Invalid(
                    "This is your only way to sign in. Set a password first, or connect another account."
                        .into(),
                ));
            }
            let identity = record.identities.remove(at);
            remember_removed(record, &identity);
            Ok(())
        })
    }

    /// Keeps a join's notice that could not be mailed, so the person sees it and it goes later.
    pub(crate) fn keep_unsent_notice(
        &self,
        person: &str,
        notice: UnsentNotice,
    ) -> Result<(), AccessError> {
        self.with(|state| {
            let record = state
                .users
                .iter_mut()
                .find(|record| record.user.id == person)
                .ok_or(AccessError::NotFound)?;
            if !record.unsent_notices.contains(&notice) {
                record.unsent_notices.push(notice);
                let excess = record.unsent_notices.len().saturating_sub(MAX_UNSENT);
                record.unsent_notices.drain(..excess);
            }
            Ok(())
        })
    }

    /// The person's join notices that have not been mailed yet, oldest first.
    pub(crate) fn unsent_notices(&self, person: &str) -> Vec<UnsentNotice> {
        self.read(|state| {
            state
                .users
                .iter()
                .find(|record| record.user.id == person)
                .map(|record| record.unsent_notices.clone())
                .unwrap_or_default()
        })
    }

    /// Forgets a notice once its mail is queued.
    pub(crate) fn notice_sent(
        &self,
        person: &str,
        notice: &UnsentNotice,
    ) -> Result<(), AccessError> {
        self.with(|state| {
            if let Some(record) = state
                .users
                .iter_mut()
                .find(|record| record.user.id == person)
            {
                record.unsent_notices.retain(|have| have != notice);
            }
            Ok(())
        })
    }

    /// Whether the account at `email` had `identity` taken off it, by Disconnect or a reset, so the
    /// sign-in page can say what to do next. [`Self::join_by_address`] refuses it either way.
    pub(crate) fn was_removed_from(&self, email: &str, identity: &Identity) -> bool {
        let email = email.trim();
        self.read(|state| {
            state.users.iter().any(|record| {
                record.user.email.eq_ignore_ascii_case(email)
                    && !record.user.disabled
                    && was_removed(record, identity)
            })
        })
    }

    /// Joins `identity` to the account at `email`, the address its provider vouched for, and signs
    /// that account in. Daniel's rule (Q30): a vouched address joins at once. `Credentials` when the
    /// account is disabled or gone; `Invalid` when the identity belongs to someone else, the
    /// account has another of that provider, or the account had it removed.
    pub(crate) async fn join_by_address(
        &self,
        email: &str,
        identity: Identity,
    ) -> Result<SignedIn, AccessError> {
        let permit = self
            .registration_workers
            .clone()
            .try_acquire_owned()
            .map_err(|_| AccessError::Throttled)?;
        // Serialized with account deletion, like every other change to a person.
        let privacy = self.mail_privacy.clone().lock_owned().await;
        let (cookie, csrf) = (random_text(32)?, random_text(24)?);
        let access = self.clone();
        let email = email.trim().to_owned();
        tokio::task::spawn_blocking(move || {
            let _held = (permit, privacy);
            access.with(|state| {
                if state.tenancy != Tenancy::Hosted {
                    return Err(AccessError::Credentials);
                }
                let record = state
                    .users
                    .iter()
                    .find(|record| {
                        record.user.email.eq_ignore_ascii_case(&email) && !record.user.disabled
                    })
                    .ok_or(AccessError::Credentials)?;
                if was_removed(record, &identity) {
                    return Err(AccessError::Invalid(format!(
                        "That {} account was disconnected from this RichView account.",
                        provider_name(&identity.provider)
                    )));
                }
                let person = record.user.id.clone();
                attach(
                    state,
                    &person,
                    Identity {
                        connected: stamp(),
                        ..identity
                    },
                )?;
                let user = state
                    .users
                    .iter()
                    .find(|record| record.user.id == person)
                    .map(|record| record.user.clone())
                    .ok_or(AccessError::Credentials)?;
                start_session(state, &person, &cookie, &csrf, now());
                Ok(SignedIn { user, cookie, csrf })
            })
        })
        .await
        .map_err(|_| AccessError::Storage("Join worker failed".into()))?
    }

    /// Makes the account sign-up makes for a stranger a provider vouches for: no password, the
    /// provider's account joined, a first organization, and a session. The rules are sign-up's:
    /// the pause, the account limit and the storage threshold (`publication` is the lock that
    /// orders the threshold check with report writes).
    pub(crate) async fn create_person_for_identity(
        &self,
        name: &str,
        email: &str,
        identity: Identity,
        publication: Option<tokio::sync::OwnedMutexGuard<()>>,
    ) -> Result<SignedIn, AccessError> {
        let permit = self
            .registration_workers
            .clone()
            .try_acquire_owned()
            .map_err(|_| AccessError::Throttled)?;
        let privacy = self.mail_privacy.clone().lock_owned().await;
        let email = crate::email::normalize(email)
            .ok_or_else(|| AccessError::Invalid("That is not an email address.".into()))?;
        let name = crate::social::person_name(name)
            .or_else(|| email.split('@').next().and_then(crate::social::person_name))
            .ok_or_else(|| AccessError::Invalid("Check the name.".into()))?;
        if !self.signup_account_capacity() {
            return Err(AccessError::Throttled);
        }
        let (cookie, csrf) = (random_text(32)?, random_text(24)?);
        let access = self.clone();
        tokio::task::spawn_blocking(move || {
            // Cancellation must not release the publication lock while this worker can still
            // create an account after the storage-threshold check.
            let _held = (permit, privacy, publication);
            access.with(|state| {
                if state.tenancy != Tenancy::Hosted {
                    return Err(AccessError::Credentials);
                }
                if state.signups_paused
                    || state.users.len()
                        >= access
                            .signup_account_limit
                            .load(std::sync::atomic::Ordering::Relaxed)
                {
                    return Err(AccessError::Throttled);
                }
                if state
                    .users
                    .iter()
                    .any(|record| record.user.email.eq_ignore_ascii_case(&email))
                {
                    return Err(AccessError::Invalid(
                        "An account already exists for this address.".into(),
                    ));
                }
                if Provider::parse(&identity.provider).is_none()
                    || owner(state, &identity.provider, &identity.subject).is_some()
                {
                    return Err(AccessError::Invalid(format!(
                        "That {} account is already connected to a RichView account.",
                        provider_name(&identity.provider)
                    )));
                }
                let identity = Identity {
                    connected: stamp(),
                    label: identity.label.chars().take(MAX_LABEL_CHARS).collect(),
                    ..identity
                };
                let user = new_account(state, name, email, None, Some(identity))?;
                start_session(state, &user.id, &cookie, &csrf, now());
                Ok(SignedIn { user, cookie, csrf })
            })
        })
        .await
        .map_err(|_| AccessError::Storage("Account worker failed".into()))?
    }
}
