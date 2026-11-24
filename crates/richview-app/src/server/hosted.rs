//! The machine-local operator actions. The public router never mounts these commands.
use super::AppState;
pub use crate::storage::HostedUsage;
use crate::storage::ReportKey;
use serde_json::{Value, json};

fn usage_workers() -> &'static std::sync::Arc<tokio::sync::Semaphore> {
    static WORKERS: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>> =
        std::sync::OnceLock::new();
    WORKERS.get_or_init(|| std::sync::Arc::new(tokio::sync::Semaphore::new(2)))
}

/// Read one active member's organization usage, with bounded filesystem scans.
pub async fn account_usage(
    state: &AppState,
    principal: &crate::access::Principal,
    org: &str,
) -> Result<HostedUsage, String> {
    if !state.access.is_hosted() || !state.access.member_of(principal, org) {
        return Err("Organization is unavailable".into());
    }
    let permit = usage_workers()
        .clone()
        .try_acquire_owned()
        .map_err(|_| "Usage is busy; try again shortly")?;
    let storage = state.storage.clone();
    let org = org.to_owned();
    tokio::task::spawn_blocking(move || {
        // A disconnected browser must not free a slot while the scan still runs.
        let _permit = permit;
        storage.hosted_usage(&org)
    })
    .await
    .map_err(|_| "Usage worker failed".to_owned())?
}

/// The mail for an invitation: who invited the address to which organization or report.
pub(crate) fn invitation_mail(
    state: &AppState,
    view: &crate::access::InviteView,
    link: &str,
) -> crate::mail::Mail {
    use super::mail_texts::{self, Sender};
    let sender = Sender {
        name: &view.inviter_name,
        email: &view.inviter_email,
    };
    let invitation = &view.invitation;
    let (subject, body, html) = match (&invitation.role, &invitation.report) {
        (Some(role), _) => (
            mail_texts::invite_subject(&sender, &view.org_name),
            mail_texts::invite(&sender, &view.org_name, *role, link),
            mail_texts::invite_html(&sender, &view.org_name, *role, link),
        ),
        (None, report) => {
            let front = report
                .as_deref()
                .and_then(|key| state.storage.summary(key, None).ok().flatten())
                .map(|row| row["frontmatter"].clone())
                .unwrap_or_default();
            let text = |field: &str| front[field].as_str().unwrap_or_default().to_owned();
            let (title, description) = (text("title"), text("description"));
            let report = mail_texts::Shared {
                organization: &view.org_name,
                title: &title,
                description: &description,
                editor: invitation.report_role == Some(crate::access::ReportRole::Editor),
            };
            (
                mail_texts::share_subject(&sender, &title),
                mail_texts::share(&sender, &report, link),
                mail_texts::share_html(&sender, &report, link),
            )
        }
    };
    crate::mail::Mail {
        to: invitation.email.clone(),
        subject,
        body,
        html: Some(html),
    }
}

pub enum OperatorAction {
    ListOrgs,
    SetSuspended { org: String, suspended: bool },
    DeleteReport { org: String, slug: String },
    DisablePerson { person: String, disabled: bool },
    SetSignupsPaused { paused: bool },
    PendingLinks,
    DispatchManual { id: String },
    Summary,
}

#[derive(Debug)]
pub(crate) enum DeleteAccountError {
    NotFound,
    NeedsSuccessor,
    Failed(&'static str),
}

/// Permanently remove one person and every organization only they belong to. The detached task retains both
/// guards if the operator disconnects; a partial failure leaves the account disabled for retry.
pub(crate) async fn delete_account(
    state: &AppState,
    person: String,
    successors: std::collections::BTreeMap<String, String>,
) -> Result<Value, DeleteAccountError> {
    if !state.access.is_hosted() || !state.storage.is_hosted() {
        return Err(DeleteAccountError::Failed(
            "Operator actions require hosted mode",
        ));
    }
    let state = state.clone();
    tokio::spawn(async move {
        let publication = state.access.publication.clone().lock_owned().await;
        let privacy = state.access.mail_privacy.clone().lock_owned().await;
        let prepared = async {
            let target = state
                .access
                .operator_account_deletion(&person, &successors)
                .map_err(|error| match error {
                    crate::access::AccessError::NotFound => DeleteAccountError::NotFound,
                    crate::access::AccessError::Invalid(_) => DeleteAccountError::NeedsSuccessor,
                    _ => DeleteAccountError::Failed(
                        "Cannot inspect account for deletion; no action taken",
                    ),
                })?;
            let queue = state
                .access
                .mail_queue()
                .cloned()
                .ok_or(DeleteAccountError::Failed(
                    "Mail is unavailable; no action taken",
                ))?;
            tokio::time::timeout(std::time::Duration::from_secs(30), queue.flush())
                .await
                .map_err(|_| DeleteAccountError::Failed("Mail queue is busy; no action taken"))?
                .map_err(|_| {
                    DeleteAccountError::Failed("Mail queue is unavailable; no action taken")
                })?;
            Ok((target, queue))
        }
        .await;
        let audit_target = if crate::access::valid_hosted_person_id(&person) {
            &person
        } else {
            "[invalid id]"
        };
        state
            .access
            .audit_operator("operator.delete-account", audit_target, prepared.is_ok())
            .map_err(|_| {
                DeleteAccountError::Failed("Operator audit unavailable; no action taken")
            })?;
        let (target, queue) = prepared?;
        tokio::task::spawn_blocking(move || {
            let (_publication, _privacy) = (publication, privacy);
            let failed =
                || DeleteAccountError::Failed("Deletion may be partial; retry this person id");
            let current = state
                .access
                .operator_begin_account_deletion(&person, &successors)
                .map_err(|_| failed())?;
            if current.orgs != target.orgs || !current.email.eq_ignore_ascii_case(&target.email) {
                return Err(failed());
            }
            for org in &target.orgs {
                state
                    .storage
                    .delete_hosted_organization(org)
                    .map_err(|_| failed())?;
            }
            state
                .access
                .operator_purge_account_links(&target.email)
                .map_err(|_| failed())?;
            queue.purge_address(&target.email).map_err(|_| failed())?;
            queue
                .purge_invitations(&target.email, &current.invitations)
                .map_err(|_| failed())?;
            state
                .access
                .operator_finish_account_deletion(&person, &target, &successors)
                .map_err(|_| failed())?;
            Ok(json!({"person":person,"org":target.orgs.first(),"orgs":target.orgs,"deleted":true}))
        })
        .await
        .map_err(|_| DeleteAccountError::Failed("Deletion may be partial; retry this person id"))?
    })
    .await
    .map_err(|_| DeleteAccountError::Failed("Deletion may be partial; retry this person id"))?
}

/// Audit every operator attempt before acting; a failed completion audit is an uncertain outcome.
pub async fn operator_action(state: &AppState, action: OperatorAction) -> Result<Value, String> {
    if !state.access.is_hosted() || !state.storage.is_hosted() {
        return Err("Operator actions require hosted mode".into());
    }
    let (kind, target) = match &action {
        OperatorAction::ListOrgs => ("list-orgs", String::new()),
        OperatorAction::SetSuspended { org, .. } => ("suspension", org.clone()),
        OperatorAction::DeleteReport { org, slug } => ("delete-report", format!("{org}/{slug}")),
        OperatorAction::DisablePerson { person, .. } => ("disable-person", person.clone()),
        OperatorAction::SetSignupsPaused { .. } => ("signups", String::new()),
        OperatorAction::PendingLinks => ("pending-links", String::new()),
        OperatorAction::DispatchManual { id } => ("dispatch-manual", id.clone()),
        OperatorAction::Summary => ("summary", String::new()),
    };
    state
        .access
        .audit_operator(&format!("operator.attempt.{kind}"), &target, true)
        .map_err(|_| "Operator audit unavailable; no action taken".to_owned())?;
    let result = run_action(state, action).await;
    match result {
        Ok(value) => {
            state.access.audit_operator(&format!("operator.complete.{kind}"), &target, true)
                .map_err(|_| "Operator action may have completed, but its completion audit failed; inspect state before retrying".to_owned())?;
            Ok(value)
        }
        Err(message) => {
            let _ =
                state
                    .access
                    .audit_operator(&format!("operator.refused.{kind}"), &target, false);
            Err(message)
        }
    }
}

async fn run_action(state: &AppState, action: OperatorAction) -> Result<Value, String> {
    match action {
        OperatorAction::ListOrgs => {
            let mut rows = Vec::new();
            for org in state.access.operator_organizations() {
                let (bytes, reports) = state.storage.operator_usage(&org.id)?;
                rows.push(json!({"id":org.id,"name":org.name,"suspended":org.suspended,"bytes":bytes,"reports":reports}));
            }
            Ok(json!({"organizations":rows}))
        }
        OperatorAction::SetSuspended { org, suspended } => {
            ReportKey::hosted(&org, "operator")?;
            let _publication = state.access.publication.clone().lock_owned().await;
            state
                .access
                .operator_set_suspended(&org, suspended)
                .map_err(|_| "Organization not found")?;
            Ok(json!({"org":org,"suspended":suspended}))
        }
        OperatorAction::DeleteReport { org, slug } => {
            let key = ReportKey::hosted(&org, &slug)?;
            let publication = state.access.publication.clone().lock_owned().await;
            if !state
                .access
                .operator_organizations()
                .iter()
                .any(|o| o.id == org)
            {
                return Err("Organization not found".into());
            }
            let store = state.storage.clone();
            let key_text = key.as_str().to_owned();
            let deleted = tokio::task::spawn_blocking(move || {
                let _publication = publication;
                store.delete_report(&key_text)
            })
            .await
            .map_err(|_| "Report deletion may have partially completed; inspect before retrying")?
            .map_err(|_| "Report deletion may have partially completed; inspect before retrying")?;
            if !deleted {
                return Err("Report not found".into());
            }
            Ok(json!({"org":org,"slug":slug,"deleted":true}))
        }
        OperatorAction::DisablePerson { person, disabled } => {
            if person.is_empty()
                || person.len() > 128
                || !person
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            {
                return Err("Invalid person id".into());
            }
            let _publication = state.access.publication.clone().lock_owned().await;
            state
                .access
                .operator_disable_person(&person, disabled)
                .map_err(|_| "Person not found")?;
            Ok(json!({"person":person,"disabled":disabled}))
        }
        OperatorAction::SetSignupsPaused { paused } => {
            state
                .access
                .operator_set_signups_paused(paused)
                .map_err(|_| "Could not change signup state")?;
            Ok(json!({"signupsPaused":paused}))
        }
        OperatorAction::PendingLinks => {
            let links = state.access.operator_pending_links()?;
            let requests = if state.access.manual_outbox() == Some(true) {
                state
                    .access
                    .mail_queue()
                    .ok_or("Mail unavailable")?
                    .pending_manual()?
                    .into_iter()
                    // A request whose invitation was cancelled or replaced, as sharing a report
                    // again with the same address does, cannot be dispatched: the operator
                    // sees one mail to send for that address, not two.
                    .filter(|request| {
                        request.invitation.as_deref().is_none_or(|id| {
                            state
                                .access
                                .invitation_view(id)
                                .is_some_and(|view| view.invitation.email == request.to)
                        })
                    })
                    .collect()
            } else {
                Vec::new()
            };
            Ok(json!({"links":links,"manualRequests":requests}))
        }
        OperatorAction::DispatchManual { id } => {
            use crate::{access::LinkPurpose, mail::ManualKind};
            if state.access.manual_outbox() != Some(true) {
                return Err("Manual outbox is not configured".into());
            }
            let _privacy = state.access.mail_privacy.clone().lock_owned().await;
            let queue = state.access.mail_queue().ok_or("Mail unavailable")?;
            let origin = state
                .public_origin
                .as_deref()
                .ok_or("Public origin missing")?;
            let base = reqwest::Url::parse(origin).map_err(|_| "Invalid public origin")?;
            let mut issue = |request: &crate::mail::ManualRequest| {
                let purpose = match request.kind {
                    ManualKind::Signup => LinkPurpose::Signup,
                    ManualKind::Reset | ManualKind::AlreadyRegistered => LinkPurpose::Reset,
                    ManualKind::Invite => {
                        // The invitation may have been cancelled or accepted since it was queued.
                        let view = request
                            .invitation
                            .as_deref()
                            .and_then(|id| state.access.invitation_view(id))
                            .filter(|view| view.invitation.email == request.to)
                            .ok_or("Invitation is no longer open")?;
                        let token = state
                            .access
                            .issue_invitation_link(&view.invitation)?
                            .ok_or("Link request is throttled")?;
                        let link = base
                            .join(&format!("/link/{token}"))
                            .map_err(|_| "Invalid link")?;
                        return Ok(invitation_mail(state, &view, link.as_str()));
                    }
                };
                let token = if matches!(
                    request.kind,
                    crate::mail::ManualKind::Signup | crate::mail::ManualKind::AlreadyRegistered
                ) {
                    state.access.issue_anonymous_link(&request.to, purpose)?
                } else {
                    state.access.issue_link(&request.to, purpose)?
                }
                .ok_or("Link request is throttled")?;
                let link = base
                    .join(&format!("/link/{token}"))
                    .map_err(|_| "Invalid link")?;
                let (subject, body) = match request.kind {
                    ManualKind::Signup => (
                        "Finish creating your RichView account",
                        super::mail_texts::signup(link.as_str()),
                    ),
                    ManualKind::Reset => (
                        "Reset your RichView password",
                        super::mail_texts::reset(link.as_str()),
                    ),
                    ManualKind::AlreadyRegistered => (
                        "Your RichView account",
                        super::mail_texts::already_registered(link.as_str()),
                    ),
                    ManualKind::Invite => return Err("Invitation is no longer open".into()),
                };
                Ok(crate::mail::Mail {
                    to: request.to.clone(),
                    subject: subject.into(),
                    body,
                    html: None,
                })
            };
            let mail = queue.dispatch_manual(&id, &mut issue)?;
            Ok(json!({"to":mail.to,"subject":mail.subject,"body":mail.body}))
        }
        OperatorAction::Summary => {
            let (waiting, oldest, sent, total, remaining) = match state.access.mail_queue() {
                Some(queue) => {
                    let (waiting, oldest, total) = queue.summary()?;
                    (
                        waiting,
                        oldest,
                        queue.sent_since_start(),
                        total,
                        Some(queue.remaining_allowances()),
                    )
                }
                None => (0, None, 0, 0, None),
            };
            let bytes = state.storage.operator_total_bytes()?;
            let (paused, room) = state.access.signup_account_status();
            let mut reasons = Vec::new();
            if paused {
                reasons.push("manual-pause");
            }
            if !room {
                reasons.push("account-limit");
            }
            if !state.storage.signup_space_available_at(bytes) {
                reasons.push("storage-limit");
            }
            match remaining {
                None => reasons.push("mail-unavailable"),
                Some((_, 0)) => reasons.push("signup-mail-limit"),
                Some(_) => {}
            }
            Ok(json!({
                "accounts": state.access.users().len(), "organizations": state.access.operator_organizations().len(),
                "bytes": bytes, "accountsHalfFull": state.access.operator_accounts_half_full(),
                "storageHalfFull": state.storage.operator_storage_half_full(bytes),
                "signupsPaused": paused, "signupsOpen": reasons.is_empty(), "signupClosedReasons": reasons,
                "mailsRemainingToday": remaining.map_or(0, |(daily, _)| daily),
                "mailsSent": null, "refusals": null, "mailsWaiting": waiting,
                "oldestMailAgeSeconds": oldest, "mailsSentSinceStart": sent, "mailsSentTotal": total,
            }))
        }
    }
}

#[cfg(test)]
mod usage_tests {
    use super::*;
    use crate::{
        access::{Access, LinkPurpose},
        storage::FsStorage,
    };

    #[tokio::test]
    async fn usage_refuses_excess_scans_before_reading_storage() {
        let dir = tempfile::tempdir().unwrap();
        let access = Access::open_hosted(dir.path()).unwrap();
        let token = access
            .issue_link("usage@example.test", LinkPurpose::Signup)
            .unwrap()
            .unwrap();
        let user = access
            .complete_hosted_link(&token, "Usage", "a-long-password")
            .await
            .unwrap();
        let org = access.organizations_for(&user.id)[0].id.clone();
        let (_, token) = access
            .create_self_token_for_fixture(&user.id, &org, "test")
            .unwrap();
        let principal = access.authenticate(Some(&token), None);
        let state = AppState {
            storage: FsStorage::new(dir.path()).unwrap().with_hosted(true),
            access,
            preview: None,
            public_origin: None,
            report_origin: None,
        };
        let first = usage_workers().clone().try_acquire_owned().unwrap();
        let second = usage_workers().clone().try_acquire_owned().unwrap();
        assert!(account_usage(&state, &principal, &org).await.is_err());
        drop((first, second));
        let usage = account_usage(&state, &principal, &org).await.unwrap();
        assert_eq!((usage.bytes, usage.reports), (0, 0));
    }
}
