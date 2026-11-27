//! Sign-in with GitHub, Microsoft or Google on the hosted server: the routes, and what each answer of a
//! provider means for an account. The protocol is `social`; the people are `access::identities`.
//!
//! The joining rule, in the order a provider's answer is read:
//!
//! - Someone signed in who connects a provider: the session is the proof, so it joins at once,
//!   whatever the addresses.
//! - An identity already joined signs its person in.
//! - A new identity whose address the provider vouches for, where an account has that address: it
//!   joins that account at once and signs in (Daniel's Q30 answer), and the address gets the notice
//!   mail every connection sends.
//! - The same, where no account has it: a confirmation page, and then an account like sign-up's,
//!   with no password.
//! - A new identity with no vouched address: nothing is created or joined.
//! - An invitation accepted with a provider: the link proved its address, so the identity joins the
//!   invited account whatever address the provider reports.
//!
//! Three details are not obvious. The session cookie is `SameSite=Strict`, so it does not come
//! back with the provider's answer, which arrives from another site, and nothing that needs the
//! session happens there: a connection is held for a second step the browser takes by itself. For
//! the same reason a page here that ends in a sign-in answers with a page that moves on, not with a
//! redirect, which would not carry the session cookie either. And the provider's address is never
//! the proof: only the provider's id for the account is kept.
use super::{
    AppState, auth, encode_value, form_caller, hosted_error_page, hosted_page, hosted_pages,
    hosted_session, redirect, report_date, safe_next, signups_paused_page,
};
use crate::{
    access::{AccessError, Identity, Principal, SignedIn, UnsentNotice},
    mail::Mail,
    server::mail_texts,
    social::{self, Held, Intent, Profile, Provider, Refused, Social},
};
use axum::{
    Form,
    extract::{Path as AxumPath, Query, State},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::sync::Arc;

/// The longest `back` address a flow keeps, in bytes.
const MAX_BACK: usize = 512;

/// An invitation to an address that has an account does not take a provider: the link is not
/// proof enough to give the account a lasting new way in.
fn existing_account() -> Response {
    problem(
        StatusCode::CONFLICT,
        "Accept the invitation as usual",
        crate::access::PROVIDER_ON_EXISTING,
    )
}

/// The Account page again, with the message a code stands for.
fn account_redirect(kind: &str, provider: Provider) -> Response {
    redirect("/account", Some(&format!("{kind}-{}", provider.slug())))
}

/// The Account page's messages. The page shows only these texts, for these codes; any other
/// `?m=` shows nothing, so an address cannot make the page say anything.
pub fn account_notice(code: &str) -> String {
    let (kind, rest) = code.split_once('-').unwrap_or((code, ""));
    // Every part after the kind must be a provider, or the code is not one of ours.
    let names: Vec<&str> = rest
        .split('-')
        .map(|slug| Provider::parse(slug).map(Provider::name))
        .collect::<Option<_>>()
        .unwrap_or_default();
    let one = (names.len() == 1).then(|| names[0]);
    match (kind, one) {
        ("connected", Some(name)) => format!("Connected {name}."),
        ("disconnected", Some(name)) => format!("Disconnected {name}."),
        ("already", Some(name)) => format!("{name} is already connected."),
        ("unconnected", Some(name)) => format!("{name} is not connected."),
        ("inuse", Some(name)) => {
            format!("That {name} account is already connected to another RichView account.")
        }
        ("another", Some(name)) => format!(
            "Another {name} account is already connected to this account. Disconnect it first."
        ),
        ("noaccount", Some(name)) => format!("{name} did not give an account to connect."),
        ("onlyway", Some(_)) => {
            "This is your only way to sign in. Set a password first, or connect another account."
                .to_owned()
        }
        ("retry", Some(_)) => "Try again shortly.".to_owned(),
        ("reset", _) if !names.is_empty() => format!(
            "Your password was changed. Disconnected: {}.",
            names.join(" and ")
        ),
        _ => String::new(),
    }
}

/// Where a person goes after a password reset: home, or the Account page saying which providers
/// the reset disconnected.
pub fn reset_landing(removed: &[Identity]) -> String {
    let slugs: Vec<&str> = Provider::ALL
        .into_iter()
        .filter(|provider| {
            removed
                .iter()
                .any(|identity| identity.provider == provider.slug())
        })
        .map(Provider::slug)
        .collect();
    if slugs.is_empty() {
        "/".to_owned()
    } else {
        format!("/account?m=reset-{}", slugs.join("-"))
    }
}

/// The provider named in the address, when this server has a client for it.
fn provider_of(state: &AppState, slug: &str) -> Option<(Provider, Arc<Social>)> {
    let social = state.access.social()?.clone();
    let provider = Provider::parse(slug)?;
    social.enabled(provider).then_some((provider, social))
}

/// Where the provider sends the browser back to.
fn callback_uri(state: &AppState, provider: Provider) -> Option<String> {
    let origin = state.public_origin.as_deref()?;
    Some(format!(
        "{}/sign-in/{}/callback",
        origin.trim_end_matches('/'),
        provider.slug()
    ))
}

/// A page that sets no cookie of its own and is never kept.
fn add_cookie(response: &mut Response, cookie: &str) {
    if let Ok(value) = HeaderValue::try_from(cookie) {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
}

/// A page that sends the browser on after the moment it takes to read, by the `Refresh` header and
/// by a link. See the module's third detail for why it is not a redirect.
pub(super) fn moving_on(title: &str, message: &str, target: &str) -> Response {
    let mut response = hosted_page(title, &hosted_pages::continue_on(title, message, target));
    if let Ok(value) = HeaderValue::try_from(format!("0; url={target}")) {
        response
            .headers_mut()
            .insert(HeaderName::from_static("refresh"), value);
    }
    response
}

/// A refusal in a few plain words; nothing in it comes from a provider.
fn problem(status: StatusCode, title: &str, message: &str) -> Response {
    let mut response = hosted_page(title, &hosted_pages::sign_in_problem(title, message));
    *response.status_mut() = status;
    response
}

/// What a flow that ended says, whatever the reason: the same words for a replayed, a foreign and
/// a late answer.
fn stale() -> Response {
    problem(
        StatusCode::BAD_REQUEST,
        "This sign-in can’t be finished",
        "It was already used, it expired, or it was started in another browser. Nothing was changed. Start again from the sign-in page.",
    )
}

fn busy() -> Response {
    problem(
        StatusCode::TOO_MANY_REQUESTS,
        "Try again shortly",
        "Too many sign-ins were started. Wait a few minutes, then try again.",
    )
}

/// Where a person goes after signing in: the page they were going to, else their reports. A
/// hosted report is on another origin, which a browser does not follow a form to under
/// `form-action 'self'`: it returns to the report's page here instead.
fn landing(back: Option<&str>) -> String {
    let report = back
        .and_then(|back| back.strip_prefix("/r/"))
        .and_then(|rest| {
            let mut parts = rest.split(['/', '?']);
            Some(format!("/reports/{}/{}", parts.next()?, parts.next()?))
        });
    report
        .or_else(|| back.map(str::to_owned))
        .unwrap_or_else(|| "/".into())
}

/// A name for an address that gave none.
fn name_from(email: &str) -> String {
    email.split('@').next().unwrap_or(email).to_owned()
}

fn identity_of(profile: &Profile) -> Identity {
    Identity {
        provider: profile.provider.slug().to_owned(),
        subject: profile.subject.clone(),
        label: profile.label.clone(),
        connected: String::new(),
    }
}

/// Starts a flow: where to send the browser, and the cookie that ties the answer to it.
fn begin(
    state: &AppState,
    headers: &HeaderMap,
    social: &Social,
    provider: Provider,
    intent: Intent,
    back: Option<String>,
) -> Result<social::Begun, Response> {
    let Some(redirect_uri) = callback_uri(state, provider) else {
        return Err(hosted_error_page(StatusCode::INTERNAL_SERVER_ERROR));
    };
    social
        .begin(
            provider,
            intent,
            back,
            &redirect_uri,
            super::super::hosted_auth::client_ip(headers),
        )
        .map_err(|refused| match refused {
            Refused::Busy => busy(),
            Refused::Unavailable => auth::not_found(),
        })
}

#[derive(Deserialize, Default)]
pub struct StartQuery {
    /// The page the person was going to, a local path; anything else is ignored.
    #[serde(default)]
    back: String,
}

/// `GET /sign-in/{provider}`: starts a plain sign-in or sign-up. Anyone may: it changes no
/// account, only waits for the provider's answer.
pub async fn social_start(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(slug): AxumPath<String>,
    Query(query): Query<StartQuery>,
) -> Response {
    let Some((provider, social)) = provider_of(&state, &slug) else {
        return auth::not_found();
    };
    // A longer address is ignored as if there were none: nothing legitimate is that long, and the
    // flow store holds it for ten minutes.
    let back = safe_next(&query.back)
        .filter(|back| back.len() <= MAX_BACK)
        .map(str::to_owned);
    let begun = match begin(&state, &headers, &social, provider, Intent::SignIn, back) {
        Ok(begun) => begun,
        Err(response) => return response,
    };
    let mut response = (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, begun.location.clone()),
            (header::CACHE_CONTROL, "no-store".to_owned()),
        ],
    )
        .into_response();
    add_cookie(
        &mut response,
        &auth::flow_cookie(&state, &begun.browser, social::FLOW_SECONDS),
    );
    response
}

/// The page that takes a browser to the provider after a form post. A redirect from a form post is
/// not followed to another site under `form-action 'self'`; this page moves on by itself.
fn to_provider(state: &AppState, provider: Provider, begun: &social::Begun) -> Response {
    let mut response = moving_on(
        &format!("Continue to {}", provider.name()),
        &format!("Taking you to {} to confirm your account.", provider.name()),
        &begun.location,
    );
    add_cookie(
        &mut response,
        &auth::flow_cookie(state, &begun.browser, social::FLOW_SECONDS),
    );
    response
}

#[derive(Deserialize)]
pub struct ConnectForm {
    csrf: String,
}

/// `POST /sign-in/{provider}/connect`: a signed-in person connects a provider to their account.
pub async fn social_connect(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(slug): AxumPath<String>,
    Form(form): Form<ConnectForm>,
) -> Response {
    let caller = match form_caller(&state, &headers, &form.csrf) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    let Some((provider, social)) = provider_of(&state, &slug) else {
        return auth::not_found();
    };
    let hosted = match hosted_session(&state, &caller) {
        Ok(hosted) => hosted,
        Err(response) => return response,
    };
    let person = hosted.person.id.clone();
    if state
        .access
        .identities_of(&person)
        .iter()
        .any(|identity| identity.provider == provider.slug())
    {
        return account_redirect("already", provider);
    }
    match begin(
        &state,
        &headers,
        &social,
        provider,
        Intent::Connect { person },
        None,
    ) {
        Ok(begun) => to_provider(&state, provider, &begun),
        Err(response) => response,
    }
}

#[derive(Deserialize)]
pub struct InviteForm {
    token: String,
}

/// `POST /sign-in/{provider}/invite`: someone accepts an invitation with a provider. The link's
/// token travels in the body; the page it came from is the proof the person holds the link.
pub async fn social_invite(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(slug): AxumPath<String>,
    Form(form): Form<InviteForm>,
) -> Response {
    if !auth::same_origin(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some((provider, social)) = provider_of(&state, &slug) else {
        return auth::not_found();
    };
    let Some(view) = state.access.inspect_invite(&form.token) else {
        return super::expired_link_page();
    };
    if view.account_exists {
        return existing_account();
    }
    match begin(
        &state,
        &headers,
        &social,
        provider,
        Intent::Invite { token: form.token },
        None,
    ) {
        Ok(begun) => to_provider(&state, provider, &begun),
        Err(response) => response,
    }
}

#[derive(Deserialize, Default)]
pub struct CallbackQuery {
    #[serde(default)]
    code: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    error: String,
}

/// A refusal that also ends the browser's flow cookie.
fn ended(state: &AppState, mut response: Response) -> Response {
    add_cookie(&mut response, &auth::flow_cookie(state, "", 0));
    response
}

/// The audit line of a sign-in that did not happen. Nothing a provider said goes in it.
fn refused(state: &AppState, provider: Provider) {
    let _ = state.access.audit(
        &Principal::Anonymous,
        "identity.sign-in",
        provider.slug(),
        false,
    );
}

/// `GET /sign-in/{provider}/callback`: the provider's answer.
pub async fn social_callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(slug): AxumPath<String>,
    Query(query): Query<CallbackQuery>,
) -> Response {
    let Some((provider, social)) = provider_of(&state, &slug) else {
        return auth::not_found();
    };
    if !query.error.is_empty() {
        // The provider says it could not or would not sign the person in; its words are not
        // repeated here. The flow is spent either way.
        // Only a flow this server started is audited: a made-up state is noise.
        if social.knows(&query.state) {
            refused(&state, provider);
        }
        social.discard(&query.state);
        return ended(
            &state,
            problem(
                StatusCode::BAD_REQUEST,
                &format!("{} did not sign you in", provider.name()),
                &format!(
                    "You may have cancelled, or {} could not finish. Nothing was changed. You can try again, or sign in another way.",
                    provider.name()
                ),
            ),
        );
    }
    let browser = auth::cookie(&headers, social::COOKIE);
    let real = social.knows(&query.state);
    let flow = match social.take(&query.state, browser.as_deref(), provider) {
        Ok(flow) => flow,
        Err(social::Stale) => {
            if real {
                refused(&state, provider);
            }
            return ended(&state, stale());
        }
    };
    let Some(browser) = browser else {
        return ended(&state, stale());
    };
    let profile = match social.profile(&flow, &query.code).await {
        Ok(profile) => profile,
        Err(failure) => {
            refused(&state, provider);
            let (status, message) = if failure == social::Failure::Unreachable {
                (
                    StatusCode::BAD_GATEWAY,
                    format!(
                        "We couldn’t reach {} just now. Nothing was changed. Try again in a moment.",
                        provider.name()
                    ),
                )
            } else {
                (
                    StatusCode::BAD_REQUEST,
                    format!(
                        "{} did not confirm your account. Nothing was changed. Start again from the sign-in page.",
                        provider.name()
                    ),
                )
            };
            return ended(
                &state,
                problem(
                    status,
                    &format!("{} sign-in failed", provider.name()),
                    &message,
                ),
            );
        }
    };
    match flow.intent.clone() {
        Intent::SignIn => {
            let (response, keep) =
                signed_out(&state, &social, provider, profile, flow.back, &browser).await;
            if keep {
                response
            } else {
                ended(&state, response)
            }
        }
        Intent::Connect { person } => {
            // The session cookie is not here (see the module's third detail): the connection waits
            // for the browser to come back to a page of this site.
            match social.hold(Held::Connect { person, profile }, provider, &browser) {
                Ok(id) => {
                    let mut response = moving_on(
                        &format!("Connecting {}", provider.name()),
                        &format!(
                            "{} confirmed your account. Finishing the connection.",
                            provider.name()
                        ),
                        &format!(
                            "/sign-in/{}/finish?c={}",
                            provider.slug(),
                            encode_value(&id)
                        ),
                    );
                    add_cookie(
                        &mut response,
                        &auth::flow_cookie(&state, &browser, social::FLOW_SECONDS),
                    );
                    response
                }
                Err(_) => ended(&state, busy()),
            }
        }
        Intent::Invite { token } => ended(&state, invited(&state, provider, profile, &token).await),
    }
}

/// A provider's answer for someone who is not signed in.
async fn signed_out(
    state: &AppState,
    social: &Social,
    provider: Provider,
    profile: Profile,
    back: Option<String>,
    browser: &str,
) -> (Response, bool) {
    // An identity already joined signs its person in.
    match state
        .access
        .sign_in_identity(provider.slug(), &profile.subject)
    {
        Ok(signed) => {
            let _ = state.access.audit(
                &Principal::User(signed.user.clone()),
                "identity.sign-in",
                provider.slug(),
                true,
            );
            return (signed_in(state, &signed, &landing(back.as_deref())), false);
        }
        Err(AccessError::Credentials) => {
            refused(state, provider);
            return (
                problem(
                    StatusCode::FORBIDDEN,
                    "Account unavailable",
                    "This account can’t sign in right now.",
                ),
                false,
            );
        }
        Err(AccessError::NotFound) => {}
        Err(_) => return (hosted_error_page(StatusCode::SERVICE_UNAVAILABLE), false),
    }
    // A new identity. Only an address the provider vouches for decides what happens next.
    let Some(email) = profile.email.clone() else {
        return (
            hosted_page(
                "We couldn’t confirm your address",
                &hosted_pages::nothing_vouched(provider.name()),
            ),
            false,
        );
    };
    if let Some(person) = state.access.user_by_email(&email) {
        return (
            join_at_once(state, provider, &profile, &person.email, back.as_deref()).await,
            false,
        );
    }
    if !super::super::hosted_auth::signup_completion_open(state).await {
        return (signups_paused_page(), false);
    }
    let name = profile.name.clone().unwrap_or_else(|| name_from(&email));
    let offer = hosted_pages::Offer {
        slug: provider.slug(),
        name: provider.name(),
    };
    let shown = social::person_name(&name).unwrap_or_else(|| name_from(&email));
    let held = Held::NewAccount {
        profile,
        email: email.clone(),
        back,
    };
    match social.hold(held, provider, browser) {
        Ok(id) => {
            let mut response = hosted_page(
                "Create your account",
                &hosted_pages::confirm_new_account(&offer, &shown, &email, &id),
            );
            // The browser's cookie stays for the confirmation, for ten minutes more.
            add_cookie(
                &mut response,
                &auth::flow_cookie(state, browser, social::FLOW_SECONDS),
            );
            (response, true)
        }
        Err(_) => (busy(), false),
    }
}

/// The session a provider's sign-in made, and the page that carries the browser on with it.
fn signed_in(state: &AppState, signed: &SignedIn, target: &str) -> Response {
    let mut response = moving_on(
        "Signing you in",
        "You are signed in. Taking you on.",
        target,
    );
    add_cookie(
        &mut response,
        &auth::session_cookie(state, &signed.cookie, 12 * 3600),
    );
    response
}

/// Joins a new identity to the account that has the address its provider vouched for, and signs
/// that account in. The account's address is told by mail, as for every connection.
async fn join_at_once(
    state: &AppState,
    provider: Provider,
    profile: &Profile,
    email: &str,
    back: Option<&str>,
) -> Response {
    let identity = identity_of(profile);
    if state.access.was_removed_from(email, &identity) {
        return disconnected(provider);
    }
    match state.access.join_by_address(email, identity).await {
        Ok(signed) => {
            let _ = state.access.audit(
                &Principal::User(signed.user.clone()),
                "identity.join",
                &format!("{}/{}", signed.user.id, provider.slug()),
                true,
            );
            notify_joined(state, &signed.user.id, &signed.user.email, provider);
            signed_in(state, &signed, &landing(back))
        }
        Err(AccessError::Credentials | AccessError::NotFound) => {
            refused(state, provider);
            problem(
                StatusCode::FORBIDDEN,
                "Account unavailable",
                "This account can’t sign in right now.",
            )
        }
        Err(AccessError::Invalid(message)) => problem(
            StatusCode::CONFLICT,
            "We couldn’t connect it",
            &format!(
                "{message} Sign in with your email address and manage your connections on the Account page."
            ),
        ),
        Err(AccessError::Throttled) => busy(),
        Err(AccessError::Storage(_)) => hosted_error_page(StatusCode::SERVICE_UNAVAILABLE),
    }
}

/// A provider account its person took off, by Disconnect or a reset: it signs in no more, and
/// the page says how to bring it back.
fn disconnected(provider: Provider) -> Response {
    let name = provider.name();
    problem(
        StatusCode::CONFLICT,
        &format!("{name} was disconnected"),
        &format!(
            "This {name} account was disconnected from your RichView account, so it no longer signs you in. To use it again, sign in another way, with your email address and password or another account you connected, then choose Connect beside {name} on the Account page."
        ),
    )
}

/// An invitation accepted with a provider. The identity joins the invited account whatever
/// address the provider reports: the link proved the invited address.
async fn invited(state: &AppState, provider: Provider, profile: Profile, token: &str) -> Response {
    let Some(view) = state.access.inspect_invite(token) else {
        return super::expired_link_page();
    };
    // The account may have been made since the person started: it takes no provider this way.
    if view.account_exists {
        return existing_account();
    }
    // An identity that belongs to someone else is refused before the link is spent.
    if let Some((holder, _)) = state
        .access
        .identity_holder(provider.slug(), &profile.subject)
    {
        let invited = state.access.user_by_email(&view.invitation.email);
        if invited.map(|person| person.id) != Some(holder) {
            return problem(
                StatusCode::BAD_REQUEST,
                "That account is already in use",
                &format!(
                    "That {} account is already connected to another RichView account. Use a different {} account, or accept the invitation with a password.",
                    provider.name(),
                    provider.name()
                ),
            );
        }
    }
    let name = profile
        .name
        .clone()
        .unwrap_or_else(|| name_from(&view.invitation.email));
    let identity = identity_of(&profile);
    let response =
        super::super::hosted_auth::accept_invitation_as(state, token, &name, None, Some(identity))
            .await;
    // A page that moves on, not a redirect: the session it may set would not come back on one.
    if response.status() == StatusCode::SEE_OTHER
        && let Some(target) = response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    {
        let cookies: Vec<HeaderValue> = response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .cloned()
            .collect();
        let mut page = moving_on("Invitation accepted", "Taking you on.", &target);
        for cookie in cookies {
            page.headers_mut().append(header::SET_COOKIE, cookie);
        }
        if let Some(person) = state.access.user_by_email(&view.invitation.email) {
            let _ = state.access.audit(
                &Principal::User(person.clone()),
                "identity.join",
                &format!("{}/{}", person.id, provider.slug()),
                true,
            );
            // No notice mail: the person has just made this account.
        }
        return page;
    }
    response
}

#[derive(Deserialize, Default)]
pub struct FinishQuery {
    #[serde(default)]
    c: String,
}

/// `GET /sign-in/{provider}/finish`: the second step of a connection, taken by the browser itself
/// so that the session cookie comes with it. The id in the address is single-use, bound to the
/// cookie of the browser that started the connection, and good for ten minutes.
pub async fn social_finish(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(slug): AxumPath<String>,
    Query(query): Query<FinishQuery>,
) -> Response {
    let Some((provider, social)) = provider_of(&state, &slug) else {
        return auth::not_found();
    };
    let browser = auth::cookie(&headers, social::COOKIE);
    let held = social.release(&query.c, browser.as_deref(), provider);
    let Ok(Held::Connect { person, profile }) = held else {
        return ended(&state, stale());
    };
    // The session must still be the person's who started the connection.
    let caller = auth::caller(&state, &headers);
    let signed_in_as_them = matches!(&caller.principal, Principal::User(user) if user.id == person)
        && caller.session.is_some();
    if !signed_in_as_them {
        return ended(
            &state,
            problem(
                StatusCode::FORBIDDEN,
                "Sign in again",
                "The account that started this connection is no longer signed in here. Nothing was changed.",
            ),
        );
    }
    let identity = Identity {
        connected: String::new(),
        ..identity_of(&profile)
    };
    let joined = state.access.join_identity(
        || auth::caller(&state, &headers).principal,
        &person,
        identity,
    );
    let response = match joined {
        Ok(()) => {
            let _ = state.access.audit(
                &caller.principal,
                "identity.join",
                &format!("{person}/{}", provider.slug()),
                true,
            );
            if let Principal::User(user) = &caller.principal {
                notify_joined(&state, &user.id, &user.email, provider);
            }
            account_redirect("connected", provider)
        }
        Err(AccessError::Invalid(message)) => account_redirect(join_failure(&message), provider),
        Err(AccessError::Throttled) => account_redirect("retry", provider),
        Err(_) => hosted_error_page(StatusCode::SERVICE_UNAVAILABLE),
    };
    ended(&state, response)
}

#[derive(Deserialize)]
pub struct ConfirmForm {
    confirmation: String,
}

/// `POST /sign-in/{provider}/confirm`: the stranger a provider vouched for creates their account.
pub async fn social_confirm(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(slug): AxumPath<String>,
    Form(form): Form<ConfirmForm>,
) -> Response {
    if !auth::same_origin(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some((provider, social)) = provider_of(&state, &slug) else {
        return auth::not_found();
    };
    let browser = auth::cookie(&headers, social::COOKIE);
    let held = social.release(&form.confirmation, browser.as_deref(), provider);
    let Ok(Held::NewAccount {
        profile,
        email,
        back,
    }) = held
    else {
        return ended(&state, stale());
    };
    // Account creation is ordered with report writes and the storage threshold, like sign-up.
    let publication = state.access.publication.clone().lock_owned().await;
    if !super::super::hosted_auth::signup_completion_open(&state).await {
        return ended(&state, signups_paused_page());
    }
    let name = profile.name.clone().unwrap_or_else(|| name_from(&email));
    let created = state
        .access
        .create_person_for_identity(&name, &email, identity_of(&profile), Some(publication))
        .await;
    let response = match created {
        Ok(signed) => {
            let who = Principal::User(signed.user.clone());
            let _ = state
                .access
                .audit(&who, "account.create", provider.slug(), true);
            let _ = state.access.audit(
                &who,
                "identity.join",
                &format!("{}/{}", signed.user.id, provider.slug()),
                true,
            );
            // No notice mail: the person has just made this account.
            signed_in(&state, &signed, &landing(back.as_deref()))
        }
        Err(AccessError::Throttled) => signups_paused_page(),
        Err(AccessError::Invalid(message)) => problem(
            StatusCode::CONFLICT,
            "We couldn’t create the account",
            &message,
        ),
        Err(_) => hosted_error_page(StatusCode::SERVICE_UNAVAILABLE),
    };
    ended(&state, response)
}

/// `POST /ui/connections/{provider}/disconnect`: takes a provider off the signed-in person, unless
/// it is their last way in.
pub async fn social_disconnect(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(slug): AxumPath<String>,
    Form(form): Form<ConnectForm>,
) -> Response {
    let caller = match form_caller(&state, &headers, &form.csrf) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    let Some(provider) = Provider::parse(&slug) else {
        return auth::not_found();
    };
    let hosted = match hosted_session(&state, &caller) {
        Ok(hosted) => hosted,
        Err(response) => return response,
    };
    let person = hosted.person.id.clone();
    match state.access.disconnect_identity(
        || auth::caller(&state, &headers).principal,
        &person,
        provider.slug(),
    ) {
        Ok(()) => {
            let _ = state.access.audit(
                &caller.principal,
                "identity.disconnect",
                &format!("{person}/{}", provider.slug()),
                true,
            );
            account_redirect("disconnected", provider)
        }
        Err(AccessError::Invalid(message)) => account_redirect(
            if message.contains("only way") {
                "onlyway"
            } else {
                "retry"
            },
            provider,
        ),
        Err(AccessError::NotFound) => account_redirect("unconnected", provider),
        Err(error) => auth::access_error(error),
    }
}

/// The Account page's code for what stopped a join.
fn join_failure(message: &str) -> &'static str {
    if message.contains("another RichView account") {
        "inuse"
    } else if message.starts_with("Another ") {
        "another"
    } else if message.contains("did not give") {
        "noaccount"
    } else {
        "retry"
    }
}

/// Tells an account's address that a provider now signs in to it, and when. A notice that cannot
/// be queued, because the day's mail budget is spent or the queue refuses it, is never dropped:
/// the account keeps it, the Account page shows it, and it is mailed when the person comes back
/// and the budget allows ([`retry_unsent_notices`]).
fn notify_joined(state: &AppState, person: &str, email: &str, provider: Provider) {
    let notice = UnsentNotice {
        provider: provider.slug().to_owned(),
        at: crate::time::Instant::now().utc_minute().unwrap_or_default(),
    };
    let (state, person, email) = (state.clone(), person.to_owned(), email.to_owned());
    tokio::spawn(async move {
        if !mail_notice(&state, &email, &notice).await {
            let kept = state.access.keep_unsent_notice(&person, notice.clone());
            let _ = state.access.audit(
                &Principal::Anonymous,
                "identity.notice-unsent",
                &format!("{person}/{}", notice.provider),
                kept.is_ok(),
            );
        }
    });
}

/// Mails the person's kept join notices that the budget now allows, and forgets each one queued.
pub(super) async fn retry_unsent_notices(state: &AppState, person: &str, email: &str) {
    for notice in state.access.unsent_notices(person) {
        if !mail_notice(state, email, &notice).await {
            break;
        }
        let _ = state.access.notice_sent(person, &notice);
    }
}

/// The person's kept join notices as the pages tell them: the provider's name and when.
pub(super) fn unsent_view(state: &AppState, person: &str) -> Vec<(&'static str, String)> {
    state
        .access
        .unsent_notices(person)
        .into_iter()
        .filter_map(|notice| Some((Provider::parse(&notice.provider)?.name(), notice.at)))
        .collect()
}

/// Queues one join notice; `false` when there is no mail queue or it refuses the mail.
async fn mail_notice(state: &AppState, email: &str, notice: &UnsentNotice) -> bool {
    let (Some(queue), Some(origin), Some(provider)) = (
        state.access.mail_queue().cloned(),
        state.public_origin.clone(),
        Provider::parse(&notice.provider),
    ) else {
        return false;
    };
    let link = format!("{}/account", origin.trim_end_matches('/'));
    let name = provider.name();
    queue
        .enqueue(Mail {
            to: email.to_owned(),
            subject: mail_texts::connected_subject(name),
            body: mail_texts::connected(name, &notice.at, &link),
            html: Some(mail_texts::connected_html(name, &notice.at, &link)),
        })
        .await
        .is_ok()
}

/// The rows of the Account page's sign-in section, owned so the page can borrow them.
pub(super) struct SignInView {
    has_password: bool,
    rows: Vec<(Provider, Option<(String, String)>)>,
    /// Joins whose notice mail has not gone yet: the provider's name and when.
    unsent: Vec<(&'static str, String)>,
}

impl SignInView {
    pub(super) fn as_options(&self) -> hosted_pages::SignInOptions<'_> {
        hosted_pages::SignInOptions {
            has_password: self.has_password,
            rows: self
                .rows
                .iter()
                .map(|(provider, connected)| hosted_pages::Connection {
                    slug: provider.slug(),
                    name: provider.name(),
                    connected: connected
                        .as_ref()
                        .map(|(label, since)| (label.as_str(), since.as_str())),
                })
                .collect(),
            unsent: self
                .unsent
                .iter()
                .map(|(name, at)| (*name, at.as_str()))
                .collect(),
        }
    }
}

/// What the Account page shows of how a person signs in; `None` on a server with no provider and
/// no connection, which shows the page it always did.
pub(super) fn sign_in_options(state: &AppState, person: &str) -> Option<SignInView> {
    let enabled = state
        .access
        .social()
        .map(|social| social.providers())
        .unwrap_or_default();
    let identities = state.access.identities_of(person);
    if enabled.is_empty() && identities.is_empty() {
        return None;
    }
    let rows = Provider::ALL
        .into_iter()
        .filter_map(|provider| {
            let have = identities
                .iter()
                .find(|identity| identity.provider == provider.slug());
            (enabled.contains(&provider) || have.is_some()).then(|| {
                (
                    provider,
                    have.map(|identity| (identity.label.clone(), report_date(&identity.connected))),
                )
            })
        })
        .collect();
    let unsent = unsent_view(state, person);
    Some(SignInView {
        has_password: state.access.has_password(person),
        rows,
        unsent,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_person_goes_back_to_where_they_were_going_and_a_hosted_report_to_its_page() {
        assert_eq!(landing(None), "/");
        assert_eq!(landing(Some("/members")), "/members");
        assert_eq!(landing(Some("/r/ab123456/q3?x=1")), "/reports/ab123456/q3");
    }

    #[test]
    fn a_name_for_an_address_that_gave_none_is_its_local_part() {
        assert_eq!(name_from("ada.lovelace@example.test"), "ada.lovelace");
    }
}
