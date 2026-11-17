//! Hosted pages and forms for organizations: members, invitations, switching, and one report's
//! page with its sharing. Every handler authorizes from the session and the access state before
//! it reads a form field's meaning; a page that hides a control is never the protection.
use super::{
    AppState, Hosted, access_error, audit, auth, form_caller, hosted_page, hosted_pages,
    hosted_session, redirect, report_date, visibility_name,
};
use crate::access::{AccessError, Invitation, Landing, OrgRole, ReportRole, Right};
use axum::{
    Form,
    extract::{Path as AxumPath, Query, State},
    http::{HeaderMap, StatusCode},
    response::Response,
};
use serde::Deserialize;

const ROLES: [OrgRole; 4] = [
    OrgRole::Owner,
    OrgRole::Admin,
    OrgRole::Author,
    OrgRole::Reader,
];

fn rank(role: OrgRole) -> u8 {
    match role {
        OrgRole::Owner => 3,
        OrgRole::Admin => 2,
        OrgRole::Author => 1,
        OrgRole::Reader => 0,
    }
}

fn role_name(role: OrgRole) -> &'static str {
    match role {
        OrgRole::Owner => "owner",
        OrgRole::Admin => "admin",
        OrgRole::Author => "author",
        OrgRole::Reader => "reader",
    }
}

fn parse_role(text: &str) -> Option<OrgRole> {
    ROLES.into_iter().find(|role| role_name(*role) == text)
}

/// A refusal the person can act on returns to the page as a notice; anything else answers as the
/// API would, so a stranger's guess at another organization still sees a plain 404.
fn back(to: &str, error: AccessError) -> Response {
    match error {
        AccessError::Invalid(message) => redirect(to, Some(&message)),
        AccessError::Throttled => redirect(to, Some("Too many requests. Try again later.")),
        other => access_error(other),
    }
}

/// Queue the mail for an invitation. `false` when it could not be queued now: the invitation
/// stays open and can be sent again.
async fn send_invitation(state: &AppState, sender: &str, invitation: Invitation) -> bool {
    let Some(queue) = state.access.mail_queue().cloned() else {
        return false;
    };
    if queue.exhausted() {
        return false;
    }
    // Account deletion waits for this lock before it clears an address's links and mail, so an
    // invitation never slips in behind it. The task keeps the lock if the browser disconnects.
    let privacy = state.access.mail_privacy.clone().lock_owned().await;
    let state = state.clone();
    let sender = sender.to_owned();
    tokio::spawn(async move {
        let _privacy = privacy;
        if !state
            .access
            .invitation_can_be_mailed(&invitation.id, &sender)
        {
            return false;
        }
        if state
            .access
            .admit_invite_mail(&sender, &invitation.org)
            .is_err()
        {
            return false;
        }
        if super::super::hosted_auth::manual(&state) {
            return queue
                .enqueue_manual_invitation(invitation.email.clone(), invitation.id.clone())
                .await
                .is_ok();
        }
        let access = state.access.clone();
        let pending = invitation.clone();
        let Ok(Ok(Some(token))) =
            tokio::task::spawn_blocking(move || access.issue_invitation_link(&pending)).await
        else {
            return false;
        };
        let Some(link) = state
            .public_origin
            .as_deref()
            .and_then(|origin| reqwest::Url::parse(origin).ok())
            .and_then(|base| base.join(&format!("/link/{token}")).ok())
        else {
            return false;
        };
        let Some(view) = state.access.invitation_view(&invitation.id) else {
            return false;
        };
        let worker = state.clone();
        let Ok(mail) = tokio::task::spawn_blocking(move || {
            super::super::hosted::invitation_mail(&worker, &view, link.as_str())
        })
        .await
        else {
            return false;
        };
        queue.enqueue_link(mail, token).await.is_ok()
    })
    .await
    .unwrap_or(false)
}

fn sent_notice(state: &AppState, sent: bool, done: &str) -> String {
    if !sent {
        format!("{done} The mail could not be sent right now; send it again later.")
    } else if super::super::hosted_auth::manual(state) {
        format!("{done} In these first days we send mails by hand, so it can take a few hours.")
    } else {
        format!(
            "{done} If the mail doesn’t arrive within a few minutes, ask them to look in their junk folder."
        )
    }
}

// ---------------------------------------------------------------- organizations

#[derive(Deserialize)]
pub struct OrganizationForm {
    #[serde(default)]
    csrf: String,
    #[serde(default)]
    org: String,
    #[serde(default)]
    name: String,
}

/// `POST /ui/organization`: work in another of the person's organizations.
pub async fn switch_organization(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<OrganizationForm>,
) -> Response {
    let caller = match form_caller(&state, &headers, &form.csrf) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    let Some(session) = caller.session.as_deref() else {
        return auth::not_found();
    };
    match state.access.select_organization(
        || auth::caller(&state, &headers).principal,
        session,
        &form.org,
    ) {
        Ok(()) => redirect("/", None),
        Err(error) => access_error(error),
    }
}

/// `POST /ui/orgs`: a person who owns no organization starts one.
pub async fn create_organization(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<OrganizationForm>,
) -> Response {
    let caller = match form_caller(&state, &headers, &form.csrf) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    match state
        .access
        .create_organization(|| auth::caller(&state, &headers).principal, &form.name)
    {
        Ok(organization) => {
            audit(&state, &caller, "organization.create", &organization.id);
            if let Some(session) = caller.session.as_deref() {
                let _ = state.access.select_organization(
                    || auth::caller(&state, &headers).principal,
                    session,
                    &organization.id,
                );
            }
            redirect("/", Some(&format!("{} is ready.", organization.name)))
        }
        Err(error) => back("/", error),
    }
}

/// `POST /ui/orgs/{org}/name`: an owner renames the organization.
pub async fn rename_organization(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(org): AxumPath<String>,
    Form(form): Form<OrganizationForm>,
) -> Response {
    let caller = match form_caller(&state, &headers, &form.csrf) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    match state.access.rename_organization(
        || auth::caller(&state, &headers).principal,
        &org,
        &form.name,
    ) {
        Ok(()) => {
            audit(&state, &caller, "organization.rename", &org);
            redirect("/members", Some("Name saved."))
        }
        Err(error) => back("/members", error),
    }
}

// ---------------------------------------------------------------- members

/// `GET /members`: the people of the current organization; owners and admins also get the
/// invitations and the controls.
pub async fn members_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(notice): Query<super::Notice>,
) -> Response {
    let caller = auth::caller(&state, &headers);
    let hosted = match hosted_session(&state, &caller) {
        Ok(hosted) => hosted,
        Err(response) => return response,
    };
    let nav = hosted.nav();
    let (Some(org), Some(mine)) = (
        hosted.org.as_ref().filter(|org| !org.suspended),
        hosted.role,
    ) else {
        if hosted.org.is_some() {
            return hosted_page("Account unavailable", &hosted_pages::suspended(&nav));
        }
        return hosted_page(
            "Members",
            &hosted_pages::members(&hosted_pages::MembersPage {
                nav: &nav,
                members: &[],
                invites: &[],
                manager: false,
                owner: false,
                roles: &[],
                notice: &notice.m,
            }),
        );
    };
    let people = match state.access.members(&caller.principal, &org.id) {
        Ok(people) => people,
        Err(error) => return access_error(error),
    };
    let manager = rank(mine) >= rank(OrgRole::Admin);
    let members: Vec<_> = people
        .iter()
        .map(|member| hosted_pages::Member {
            person: &member.person,
            name: &member.name,
            email: &member.email,
            role: role_name(member.role),
            me: member.person == hosted.person.id,
            manageable: manager && rank(member.role) <= rank(mine),
        })
        .collect();
    let invitations = if manager {
        state
            .access
            .invitations(&caller.principal, &org.id)
            .unwrap_or_default()
    } else {
        vec![]
    };
    let expiries: Vec<String> = invitations
        .iter()
        .map(|view| {
            crate::time::Instant::from_unix(view.invitation.expires as i64, 0)
                .and_then(crate::time::Instant::utc_day)
                .unwrap_or_default()
        })
        .collect();
    let invites: Vec<_> = invitations
        .iter()
        .zip(&expiries)
        .filter_map(|(view, expires)| {
            Some(hosted_pages::PendingInvite {
                id: &view.invitation.id,
                email: &view.invitation.email,
                role: role_name(view.invitation.role?),
                inviter: &view.inviter_name,
                expires,
            })
        })
        .collect();
    let roles: Vec<&str> = ROLES
        .into_iter()
        .filter(|role| rank(*role) <= rank(mine))
        .map(role_name)
        .collect();
    hosted_page(
        "Members",
        &hosted_pages::members(&hosted_pages::MembersPage {
            nav: &nav,
            members: &members,
            invites: &invites,
            manager,
            owner: mine == OrgRole::Owner,
            roles: &roles,
            notice: &notice.m,
        }),
    )
}

#[derive(Deserialize)]
pub struct MemberForm {
    #[serde(default)]
    csrf: String,
    #[serde(default)]
    email: String,
    #[serde(default)]
    role: String,
}

/// `POST /ui/orgs/{org}/invitations`: invite an address to the organization.
pub async fn invite_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(org): AxumPath<String>,
    Form(form): Form<MemberForm>,
) -> Response {
    let caller = match form_caller(&state, &headers, &form.csrf) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    let hosted = match hosted_session(&state, &caller) {
        Ok(hosted) => hosted,
        Err(response) => return response,
    };
    let Some(role) = parse_role(&form.role) else {
        return back("/members", AccessError::Invalid("Choose a role.".into()));
    };
    match state.access.invite_member(
        || auth::caller(&state, &headers).principal,
        &org,
        &form.email,
        role,
    ) {
        Ok(invitation) => {
            audit(
                &state,
                &caller,
                "member.invite",
                &format!("{org}/{}", invitation.id),
            );
            let email = invitation.email.clone();
            let sent = send_invitation(&state, &hosted.person.id, invitation).await;
            redirect(
                "/members",
                Some(&sent_notice(&state, sent, &format!("Invited {email}."))),
            )
        }
        Err(error) => back("/members", error),
    }
}

/// `POST /ui/orgs/{org}/invitations/{id}/resend`: mail an open invitation again.
pub async fn resend_invitation(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((org, id)): AxumPath<(String, String)>,
    Form(form): Form<MemberForm>,
) -> Response {
    let caller = match form_caller(&state, &headers, &form.csrf) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    let hosted = match hosted_session(&state, &caller) {
        Ok(hosted) => hosted,
        Err(response) => return response,
    };
    match state
        .access
        .renew_invitation(|| auth::caller(&state, &headers).principal, &org, &id)
    {
        Ok(invitation) => {
            audit(&state, &caller, "member.invite", &format!("{org}/{id}"));
            let email = invitation.email.clone();
            let sent = send_invitation(&state, &hosted.person.id, invitation).await;
            redirect(
                "/members",
                Some(&sent_notice(
                    &state,
                    sent,
                    &format!("Invited {email} again."),
                )),
            )
        }
        Err(error) => back("/members", error),
    }
}

/// `POST /ui/orgs/{org}/invitations/{id}/cancel`
pub async fn cancel_invitation(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((org, id)): AxumPath<(String, String)>,
    Form(form): Form<MemberForm>,
) -> Response {
    let caller = match form_caller(&state, &headers, &form.csrf) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    match state
        .access
        .cancel_invitation(|| auth::caller(&state, &headers).principal, &org, &id)
    {
        Ok(()) => {
            audit(
                &state,
                &caller,
                "member.invite.cancel",
                &format!("{org}/{id}"),
            );
            redirect("/members", Some("Invitation cancelled."))
        }
        Err(error) => back("/members", error),
    }
}

/// `POST /ui/orgs/{org}/members/{person}/role`
pub async fn set_member_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((org, person)): AxumPath<(String, String)>,
    Form(form): Form<MemberForm>,
) -> Response {
    let caller = match form_caller(&state, &headers, &form.csrf) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    let Some(role) = parse_role(&form.role) else {
        return back("/members", AccessError::Invalid("Choose a role.".into()));
    };
    match state.access.set_member_role(
        || auth::caller(&state, &headers).principal,
        &org,
        &person,
        role,
    ) {
        Ok(()) => {
            audit(
                &state,
                &caller,
                "member.role",
                &format!("{org}/{person}/{}", role_name(role)),
            );
            redirect("/members", Some("Role saved."))
        }
        Err(error) => back("/members", error),
    }
}

/// `POST /ui/orgs/{org}/members/{person}/remove`: remove a member, or leave.
pub async fn remove_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((org, person)): AxumPath<(String, String)>,
    Form(form): Form<MemberForm>,
) -> Response {
    let caller = match form_caller(&state, &headers, &form.csrf) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    let hosted = match hosted_session(&state, &caller) {
        Ok(hosted) => hosted,
        Err(response) => return response,
    };
    #[cfg(test)]
    crate::test_points::reach("remove.waiting", &org);
    // Reports change hands with the membership; publishing waits so none lands in between.
    let _publication = state.access.publication.clone().lock_owned().await;
    // The wait can be long: the session is read again once it is over, so one that ended
    // meanwhile removes nobody.
    let caller = auth::caller(&state, &headers);
    if hosted_session(&state, &caller).is_err() {
        return auth::not_found();
    }
    match state
        .access
        .remove_member(|| auth::caller(&state, &headers).principal, &org, &person)
    {
        Ok(()) => {
            audit(&state, &caller, "member.remove", &format!("{org}/{person}"));
            if person == hosted.person.id {
                redirect("/", Some("You left the organization."))
            } else {
                redirect("/members", Some("Member removed."))
            }
        }
        Err(error) => back("/members", error),
    }
}

/// `POST /ui/invitations/{id}/accept`: join an organization from the reports page. The session
/// of the invited address is the proof.
pub async fn accept_invitation(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
    Form(form): Form<MemberForm>,
) -> Response {
    let caller = match form_caller(&state, &headers, &form.csrf) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    match state
        .access
        .accept_invitation(|| auth::caller(&state, &headers).principal, &id)
    {
        Ok(Landing::Organization(org)) => {
            audit(&state, &caller, "member.join", &org);
            if let Some(session) = caller.session.as_deref() {
                let _ = state.access.select_organization(
                    || auth::caller(&state, &headers).principal,
                    session,
                    &org,
                );
            }
            let name = state
                .access
                .organization(&org)
                .map(|org| org.name)
                .unwrap_or_default();
            redirect("/", Some(&format!("You joined {name}.")))
        }
        Ok(Landing::Report(key)) => redirect(&format!("/reports/{key}"), None),
        Err(error) => back("/", error),
    }
}

/// What an invitation link shows before anything is spent.
pub(super) async fn invitation_page(
    state: &AppState,
    token: &str,
    submitted_name: &str,
    error: &str,
) -> Response {
    let Some(view) = state.access.inspect_invite(token) else {
        return super::expired_link_page();
    };
    let title = match view.invitation.report.clone() {
        Some(key) => {
            let storage = state.storage.clone();
            match tokio::task::spawn_blocking(move || storage.summary(&key, None)).await {
                Ok(Ok(Some(report))) => Some(
                    report["frontmatter"]["title"]
                        .as_str()
                        .unwrap_or("a report")
                        .to_owned(),
                ),
                _ => return super::expired_link_page(),
            }
        }
        None => None,
    };
    let body = hosted_pages::accept_invite(&hosted_pages::InvitePage {
        token,
        inviter: &view.inviter_name,
        inviter_email: &view.inviter_email,
        organization: &view.org_name,
        role: view
            .invitation
            .role
            .map(super::super::mail_texts::role_phrase),
        report: title.as_deref(),
        account_exists: view.account_exists,
        submitted_name,
        error,
    });
    hosted_page("Invitation", &body)
}

// ---------------------------------------------------------------- one report

fn report_key(state: &AppState, org: &str, slug: &str) -> Result<String, Response> {
    let key = format!("{org}/{slug}");
    state
        .storage
        .validate_key(&key)
        .map_err(|_| auth::not_found())?;
    Ok(key)
}

fn version_order(version: &str) -> Vec<u64> {
    version
        .split(['.', '-', '+'])
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

/// `GET /reports/{org}/{slug}`: the report's link, versions, sharing and removal, each shown only
/// to someone who holds the right behind it.
pub async fn report_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((org, slug)): AxumPath<(String, String)>,
    Query(notice): Query<super::Notice>,
) -> Response {
    let key = match report_key(&state, &org, &slug) {
        Ok(key) => key,
        Err(response) => return response,
    };
    let caller = auth::caller(&state, &headers);
    let hosted: Hosted<'_> = match hosted_session(&state, &caller) {
        Ok(hosted) => hosted,
        Err(response) => return response,
    };
    if let Err(response) = auth::authorize(&state, &caller, &key, Right::View) {
        return response;
    }
    let Some(origin) = state.report_origin.as_deref() else {
        return super::hosted_error_page(StatusCode::INTERNAL_SERVER_ERROR);
    };
    let storage = state.storage.clone();
    let lookup = key.clone();
    let loaded = tokio::task::spawn_blocking(move || {
        let Some(latest) = storage.summary(&lookup, None)? else {
            return Ok::<_, String>(None);
        };
        let mut versions = storage.list_versions(&lookup)?;
        versions.sort_by_key(|version| std::cmp::Reverse(version_order(version)));
        versions.truncate(50);
        let mut dated = Vec::new();
        for version in versions {
            if let Some(row) = storage.summary(&lookup, Some(&version))? {
                dated.push((
                    version,
                    report_date(row["updatedAt"].as_str().unwrap_or("")),
                ));
            }
        }
        Ok(Some((latest, dated)))
    })
    .await;
    let (latest, versions) = match loaded {
        Ok(Ok(Some(found))) => found,
        Ok(Ok(None)) => return auth::not_found(),
        _ => return super::hosted_error_page(StatusCode::INTERNAL_SERVER_ERROR),
    };
    let rights = state.access.rights(&caller.principal, &key);
    let Some(access) = state.access.report_access(&key) else {
        return auth::not_found();
    };
    let people: Vec<_> = access
        .named
        .iter()
        .map(|row| hosted_pages::Person {
            email: &row.email,
            name: row.name.as_deref(),
            role: if row.role == ReportRole::Viewer {
                "viewer"
            } else {
                "editor"
            },
            pending: row.pending,
        })
        .collect();
    let mine = state.access.acl(&key).owner == hosted.person.id;
    let owner = if mine {
        "You".to_owned()
    } else if access.owner_name.is_empty() {
        "A former member".to_owned()
    } else {
        access.owner_name.clone()
    };
    let nav = hosted.nav();
    let url = format!("{}/r/{key}", origin.trim_end_matches('/'));
    let body = hosted_pages::report(&hosted_pages::ReportPage {
        nav: &nav,
        key: &key,
        title: latest["frontmatter"]["title"].as_str().unwrap_or(&slug),
        description: latest["frontmatter"]["description"].as_str().unwrap_or(""),
        owner: &owner,
        organization: &access.org_name,
        visibility: visibility_name(access.visibility),
        shared_with_you: !mine
            && !rights.contains(&Right::Share)
            && access
                .named
                .iter()
                .any(|row| row.email.eq_ignore_ascii_case(&hosted.person.email)),
        url: &url,
        versions: &versions,
        latest: latest["version"].as_str().unwrap_or(""),
        sharing: rights
            .contains(&Right::Share)
            .then_some(hosted_pages::Sharing {
                owner_name: &access.owner_name,
                owner_email: &access.owner_email,
                people: &people,
            }),
        can_delete: rights.contains(&Right::Delete),
        notice: &notice.m,
    });
    hosted_page("Report", &body)
}

#[derive(Deserialize)]
pub struct PersonForm {
    #[serde(default)]
    csrf: String,
    #[serde(default)]
    email: String,
    #[serde(default)]
    role: String,
}

/// The caller of a sharing form, once they hold `share` on a report that exists. Anyone else
/// gets the answer a missing report gets.
async fn sharing_form(
    state: &AppState,
    headers: &HeaderMap,
    org: &str,
    slug: &str,
    csrf: &str,
) -> Result<(auth::Caller, String), Response> {
    let key = report_key(state, org, slug)?;
    let caller = form_caller(state, headers, csrf)?;
    hosted_session(state, &caller)?;
    auth::authorize(state, &caller, &key, Right::Share)?;
    let storage = state.storage.clone();
    let lookup = key.clone();
    match tokio::task::spawn_blocking(move || storage.summary(&lookup, None)).await {
        Ok(Ok(Some(_))) => {
            // The load can wait for a worker. The session is read again after it, silently:
            // one that ended meanwhile changes nothing about who may read the report.
            let caller = auth::caller(state, headers);
            if hosted_session(state, &caller).is_err()
                || !state.access.can(&caller.principal, &key, Right::Share)
            {
                return Err(auth::not_found());
            }
            Ok((caller, key))
        }
        Ok(Ok(None)) => Err(auth::not_found()),
        _ => Err(super::hosted_error_page(StatusCode::INTERNAL_SERVER_ERROR)),
    }
}

/// `POST /ui/reports/{org}/{slug}/people`: share the report with an address as viewer or editor.
pub async fn share_person(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((org, slug)): AxumPath<(String, String)>,
    Form(form): Form<PersonForm>,
) -> Response {
    let (caller, key) = match sharing_form(&state, &headers, &org, &slug, &form.csrf).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let page = format!("/reports/{key}");
    let role = match form.role.as_str() {
        "viewer" => ReportRole::Viewer,
        "editor" => ReportRole::Editor,
        _ => {
            return back(
                &page,
                AccessError::Invalid("Choose what they can do.".into()),
            );
        }
    };
    let sender = caller.principal.label();
    let sender = sender.trim_start_matches("user:");
    match state.access.share_with_email(
        || auth::caller(&state, &headers).principal,
        &key,
        &form.email,
        role,
    ) {
        Ok(Some(invitation)) => {
            audit(&state, &caller, "report.share", &key);
            let email = invitation.email.clone();
            let sent = send_invitation(&state, sender, invitation).await;
            redirect(
                &page,
                Some(&sent_notice(&state, sent, &format!("Shared with {email}."))),
            )
        }
        Ok(None) => {
            audit(&state, &caller, "report.share", &key);
            redirect(&page, Some("Saved."))
        }
        Err(error) => back(&page, error),
    }
}

/// `POST /ui/reports/{org}/{slug}/people/remove`: take an address off the report.
pub async fn unshare_person(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((org, slug)): AxumPath<(String, String)>,
    Form(form): Form<PersonForm>,
) -> Response {
    let (caller, key) = match sharing_form(&state, &headers, &org, &slug, &form.csrf).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let page = format!("/reports/{key}");
    match state.access.unshare_email(
        || auth::caller(&state, &headers).principal,
        &key,
        &form.email,
    ) {
        Ok(()) => {
            audit(&state, &caller, "report.unshare", &key);
            redirect(&page, Some("Removed."))
        }
        Err(error) => back(&page, error),
    }
}

#[cfg(test)]
mod deletion_mail_tests {
    use super::*;
    use crate::{access::Access, mail::Outbox, storage::FsStorage};
    use serde_json::json;
    use std::sync::Arc;

    #[tokio::test]
    async fn invitation_waiting_for_privacy_lock_is_not_queued_after_sender_is_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let access_dir = dir.path().join(".access");
        std::fs::create_dir_all(&access_dir).unwrap();
        let seed = json!({"schema":2,"tenancy":"hosted",
            "organizations":[{"id":"aaaa0001","name":"A","created":"now","limits":{}}],
            "memberships":[{"person":"person-a","org":"aaaa0001","role":"owner","created":"now"}],
            "users":[{"id":"person-a","name":"Alice","email":"alice@example.test","created":"now","verified":"now"}],
            "sessions":[],"acls":{},"invitations":[{"id":"i-invite","org":"aaaa0001","email":"friend@example.test","role":"reader","inviter":"person-a","created":1,"expires":4102444800u64}]});
        std::fs::write(
            access_dir.join("state.json"),
            serde_json::to_vec(&seed).unwrap(),
        )
        .unwrap();
        let access = Access::open_hosted(dir.path()).unwrap();
        access
            .configure_mail(
                Arc::new(Outbox::open(&dir.path().join("outbox")).unwrap()),
                20,
                true,
            )
            .unwrap();
        let state = AppState {
            storage: FsStorage::new(dir.path()).unwrap().with_hosted(true),
            access: access.clone(),
            preview: None,
            public_origin: Some("https://app.test".into()),
            report_origin: Some("https://reports.test".into()),
        };
        let invitation = access.invitation_view("i-invite").unwrap().invitation;
        let privacy = access.mail_privacy.clone().lock_owned().await;
        let task =
            tokio::spawn(async move { send_invitation(&state, "person-a", invitation).await });
        tokio::task::yield_now().await;
        assert!(!task.is_finished());
        access.operator_disable_person("person-a", true).unwrap();
        drop(privacy);
        assert!(!task.await.unwrap());
        assert!(
            access
                .mail_queue()
                .unwrap()
                .pending_manual()
                .unwrap()
                .is_empty()
        );
        assert!(!dir.path().join(".access/invite-mail-budget.json").exists());
    }
}
