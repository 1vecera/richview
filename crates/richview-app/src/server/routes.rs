//! The route table (TYP-70): every route of the report host with the access rule it carries. The
//! router is built from this table and nothing else, so a route cannot exist without a rule, and
//! the generated role matrix (FEA-SF-64) reads the same table.
use super::{AppState, auth, governance, hosted_auth, hosted_routes as hosted, reports, ui};
use axum::{
    Router,
    routing::{MethodRouter, delete, get, patch, post, put},
};
use std::collections::BTreeMap;

/// Who may call a route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// Anyone, signed in or not.
    Public,
    /// Needs `view` on the report in the path; a caller who cannot see it gets a 404.
    View,
    Edit,
    Manage,
    Share,
    Delete,
    /// Publishing: `create` for a new address, `edit` for a stored one.
    Publish,
    /// Admins and the service only.
    Admin,
    /// Whoever is signed in sees their own part: lists show only what the caller may see.
    Shared,
    /// Credentials in the body.
    SignIn,
    /// Any signed-in caller; the CSRF token for cookie sessions.
    Session,
    /// The local preview of `richview serve` only.
    Preview,
}

/// One route: a method, a path pattern and the rule it carries.
#[derive(Debug, Clone, Copy)]
pub struct Spec {
    pub method: &'static str,
    pub path: &'static str,
    pub rule: Rule,
}

struct Entry {
    spec: Spec,
    handler: MethodRouter<AppState>,
}

fn entry(
    method: &'static str,
    path: &'static str,
    rule: Rule,
    handler: MethodRouter<AppState>,
) -> Entry {
    Entry {
        spec: Spec { method, path, rule },
        handler,
    }
}

/// Sign-in takes credentials and nothing else: the body is cut off as it streams in, so chunked
/// bodies without a length are bounded too (FEA-SF-19).
/// The most a sign-in request may carry.
pub(super) const SIGN_IN_BODY_BYTES: u64 = 64 * 1024;

fn sign_in_limit() -> axum::extract::DefaultBodyLimit {
    axum::extract::DefaultBodyLimit::max(SIGN_IN_BODY_BYTES as usize)
}

fn table() -> Vec<Entry> {
    use Rule::*;
    vec![
        entry(
            "GET",
            "/health",
            Public,
            get(|| async { axum::Json(serde_json::json!({"ok": true})) }),
        ),
        // Reports and their files.
        entry("GET", "/r/{slug}", View, get(reports::report_latest)),
        entry(
            "GET",
            "/r/{slug}/v/{version}",
            View,
            get(reports::report_version),
        ),
        entry(
            "GET",
            "/r/{slug}/v/{version}/files/{*path}",
            View,
            get(reports::report_file),
        ),
        entry(
            "GET",
            "/r/{slug}/download",
            View,
            get(reports::report_download_latest),
        ),
        entry(
            "GET",
            "/r/{slug}/v/{version}/download",
            View,
            get(reports::report_download),
        ),
        entry(
            "GET",
            "/r/{slug}/share.png",
            View,
            get(reports::share_latest),
        ),
        entry(
            "GET",
            "/r/{slug}/v/{version}/share.png",
            View,
            get(reports::share_version),
        ),
        entry("GET", "/raw/{slug}", View, get(reports::raw_latest)),
        entry(
            "GET",
            "/raw/{slug}/v/{version}",
            View,
            get(reports::raw_version),
        ),
        entry("GET", "/_rv/{name}", Public, get(reports::runtime_file)),
        // The report API.
        entry("GET", "/api/reports", Shared, get(reports::list_reports)),
        entry("POST", "/api/reports", Publish, post(reports::publish_api)),
        entry(
            "GET",
            "/api/reports/{slug}",
            View,
            get(reports::report_info),
        ),
        entry(
            "DELETE",
            "/api/reports/{slug}",
            Delete,
            delete(reports::delete_report),
        ),
        entry(
            "GET",
            "/api/reports/{slug}/versions",
            View,
            get(reports::report_versions),
        ),
        entry(
            "GET",
            "/api/reports/{slug}/v/{version}",
            Edit,
            get(reports::report_bundle),
        ),
        entry(
            "DELETE",
            "/api/reports/{slug}/v/{version}",
            Delete,
            delete(reports::delete_version),
        ),
        entry(
            "PUT",
            "/api/reports/{slug}/latest",
            Manage,
            put(reports::pin_latest),
        ),
        entry(
            "DELETE",
            "/api/reports/{slug}/latest",
            Manage,
            delete(reports::unpin_latest),
        ),
        entry(
            "POST",
            "/api/reports/{slug}/v/{version}/regenerate",
            Manage,
            post(reports::regenerate),
        ),
        entry(
            "GET",
            "/api/reports/{slug}/access",
            Share,
            get(governance::get_access),
        ),
        entry(
            "PUT",
            "/api/reports/{slug}/access",
            Share,
            put(governance::put_access),
        ),
        // Stateless operations over HTTP: the same envelopes as the CLI (ARC-08); nothing stored.
        entry(
            "POST",
            "/api/validate",
            Publish,
            post(reports::validate_api),
        ),
        entry("POST", "/api/check", Publish, post(reports::check_api)),
        // Sign-in.
        entry(
            "POST",
            "/api/session",
            SignIn,
            post(auth::sign_in).layer(sign_in_limit()),
        ),
        entry("DELETE", "/api/session", Session, delete(auth::sign_out)),
        entry("GET", "/api/session", Public, get(auth::whoami)),
        // Governance.
        entry(
            "GET",
            "/api/governance/users",
            Admin,
            get(governance::list_users),
        ),
        entry(
            "POST",
            "/api/governance/users",
            Admin,
            post(governance::create_user),
        ),
        entry(
            "PATCH",
            "/api/governance/users/{id}",
            Admin,
            patch(governance::update_user),
        ),
        entry(
            "DELETE",
            "/api/governance/users/{id}",
            Admin,
            delete(governance::delete_user),
        ),
        entry(
            "PUT",
            "/api/governance/users/{id}/password",
            Admin,
            put(governance::set_password),
        ),
        entry(
            "GET",
            "/api/governance/users/{id}/tokens",
            Admin,
            get(governance::list_tokens),
        ),
        entry(
            "POST",
            "/api/governance/users/{id}/tokens",
            Admin,
            post(governance::create_token),
        ),
        entry(
            "DELETE",
            "/api/governance/users/{id}/tokens/{token}",
            Admin,
            delete(governance::revoke_token),
        ),
        entry(
            "GET",
            "/api/governance/audit",
            Admin,
            get(governance::read_audit),
        ),
        // The pages and the forms they post (FEA-SF-18).
        entry("GET", "/", Public, get(ui::index)),
        entry("GET", "/governance", Admin, get(ui::governance)),
        entry(
            "POST",
            "/ui/sign-in",
            SignIn,
            post(ui::sign_in).layer(sign_in_limit()),
        ),
        entry("POST", "/ui/sign-out", Session, post(ui::sign_out)),
        entry("POST", "/ui/share/{slug}", Share, post(ui::share)),
        entry("POST", "/ui/governance/users", Admin, post(ui::create_user)),
        entry(
            "POST",
            "/ui/governance/users/{id}",
            Admin,
            post(ui::update_user),
        ),
        entry(
            "POST",
            "/ui/governance/users/{id}/password",
            Admin,
            post(ui::set_password),
        ),
        entry(
            "POST",
            "/ui/governance/users/{id}/tokens",
            Admin,
            post(ui::create_token),
        ),
        entry(
            "POST",
            "/ui/governance/users/{id}/tokens/{token}/revoke",
            Admin,
            post(ui::revoke_token),
        ),
        entry(
            "POST",
            "/ui/governance/users/{id}/delete",
            Admin,
            post(ui::delete_user),
        ),
        // The local preview, mounted only by `richview serve`.
        entry("GET", "/__revision", Preview, get(super::preview_revision)),
        entry("GET", "/thumbnail", Preview, get(super::preview_thumbnail)),
    ]
}

/// Every route with its rule, for the role matrix and the contract check.
pub fn specs() -> Vec<Spec> {
    table().into_iter().map(|e| e.spec).collect()
}

fn hosted_table() -> Vec<Entry> {
    use Rule::*;
    let mut entries: Vec<_> = table()
        .into_iter()
        .filter_map(|e| {
            let spec = e.spec;
            if spec.path == "/governance"
                || spec.path.starts_with("/api/governance/")
                || spec.path.starts_with("/ui/governance/")
                || spec.path == "/ui/share/{slug}"
            {
                return None;
            }
            Some(match (spec.method, spec.path) {
                ("GET", "/r/{slug}") => {
                    entry("GET", "/r/{org}/{slug}", View, get(hosted::report_latest))
                }
                ("GET", "/r/{slug}/v/{version}") => entry(
                    "GET",
                    "/r/{org}/{slug}/v/{version}",
                    View,
                    get(hosted::report_version),
                ),
                ("GET", "/r/{slug}/v/{version}/files/{*path}") => entry(
                    "GET",
                    "/r/{org}/{slug}/v/{version}/files/{*path}",
                    View,
                    get(hosted::report_file),
                ),
                ("GET", "/r/{slug}/download") => entry(
                    "GET",
                    "/r/{org}/{slug}/download",
                    View,
                    get(hosted::report_download_latest),
                ),
                ("GET", "/r/{slug}/v/{version}/download") => entry(
                    "GET",
                    "/r/{org}/{slug}/v/{version}/download",
                    View,
                    get(hosted::report_download),
                ),
                ("GET", "/r/{slug}/share.png") => entry(
                    "GET",
                    "/r/{org}/{slug}/share.png",
                    View,
                    get(hosted::share_latest),
                ),
                ("GET", "/r/{slug}/v/{version}/share.png") => entry(
                    "GET",
                    "/r/{org}/{slug}/v/{version}/share.png",
                    View,
                    get(hosted::share_version),
                ),
                ("GET", "/raw/{slug}") => {
                    entry("GET", "/raw/{org}/{slug}", View, get(hosted::raw_latest))
                }
                ("GET", "/raw/{slug}/v/{version}") => entry(
                    "GET",
                    "/raw/{org}/{slug}/v/{version}",
                    View,
                    get(hosted::raw_version),
                ),
                ("GET", "/api/reports/{slug}") => entry(
                    "GET",
                    "/api/reports/{org}/{slug}",
                    View,
                    get(hosted::report_info),
                ),
                ("DELETE", "/api/reports/{slug}") => entry(
                    "DELETE",
                    "/api/reports/{org}/{slug}",
                    Delete,
                    delete(hosted::delete_report),
                ),
                ("GET", "/api/reports/{slug}/versions") => entry(
                    "GET",
                    "/api/reports/{org}/{slug}/versions",
                    View,
                    get(hosted::report_versions),
                ),
                ("GET", "/api/reports/{slug}/v/{version}") => entry(
                    "GET",
                    "/api/reports/{org}/{slug}/v/{version}",
                    Edit,
                    get(hosted::report_bundle),
                ),
                ("DELETE", "/api/reports/{slug}/v/{version}") => entry(
                    "DELETE",
                    "/api/reports/{org}/{slug}/v/{version}",
                    Delete,
                    delete(hosted::delete_version),
                ),
                ("PUT", "/api/reports/{slug}/latest") => entry(
                    "PUT",
                    "/api/reports/{org}/{slug}/latest",
                    Manage,
                    put(hosted::pin_latest),
                ),
                ("DELETE", "/api/reports/{slug}/latest") => entry(
                    "DELETE",
                    "/api/reports/{org}/{slug}/latest",
                    Manage,
                    delete(hosted::unpin_latest),
                ),
                ("POST", "/api/reports/{slug}/v/{version}/regenerate") => entry(
                    "POST",
                    "/api/reports/{org}/{slug}/v/{version}/regenerate",
                    Manage,
                    post(hosted::regenerate),
                ),
                ("GET", "/api/reports/{slug}/access") => entry(
                    "GET",
                    "/api/reports/{org}/{slug}/access",
                    Share,
                    get(hosted::get_access),
                ),
                ("PUT", "/api/reports/{slug}/access") => entry(
                    "PUT",
                    "/api/reports/{org}/{slug}/access",
                    Share,
                    put(hosted::put_access).layer(axum::extract::DefaultBodyLimit::max(32 * 1024)),
                ),
                _ if spec.path.contains("{slug}") => panic!(
                    "Hosted report route has no organization-scoped handler: {} {}",
                    spec.method, spec.path
                ),
                _ => e,
            })
        })
        .collect();
    entries.extend([
        entry("GET", "/sign-up", Public, get(hosted_auth::signup_page)),
        entry("GET", "/forgot", Public, get(ui::forgot_page)),
        entry("GET", "/link/{token}", Public, get(ui::link_page)),
        entry(
            "POST",
            "/ui/sign-up",
            SignIn,
            post(hosted_auth::signup).layer(sign_in_limit()),
        ),
        entry(
            "POST",
            "/ui/forgot",
            SignIn,
            post(hosted_auth::forgot).layer(sign_in_limit()),
        ),
        entry(
            "POST",
            "/ui/link/{token}",
            SignIn,
            post(hosted_auth::complete).layer(sign_in_limit()),
        ),
        entry("GET", "/tokens", Session, get(ui::personal_tokens)),
        // The command line's browser sign-in: two routes anyone may call, bounded by address;
        // the page and its form for a signed-in person; and a token revoking itself.
        entry(
            "POST",
            "/api/cli/sign-in",
            Public,
            post(ui::start_cli_sign_in).layer(sign_in_limit()),
        ),
        entry(
            "POST",
            "/api/cli/sign-in/token",
            Public,
            post(ui::collect_cli_sign_in).layer(sign_in_limit()),
        ),
        entry(
            "DELETE",
            "/api/cli/token",
            Session,
            delete(ui::revoke_cli_token),
        ),
        // The same sign-in through a redirect to the command line's own loopback port: how a
        // server signs in, a trade of the one-time code or a refresh token for the sign-in, and
        // the page and form where a signed-in person answers.
        entry(
            "GET",
            "/api/cli/oauth/metadata",
            Public,
            get(ui::cli_oauth_metadata),
        ),
        entry(
            "POST",
            "/api/cli/oauth/token",
            Public,
            post(ui::cli_oauth_token).layer(sign_in_limit()),
        ),
        entry(
            "POST",
            "/api/cli/oauth/revoke",
            Public,
            post(ui::cli_oauth_revoke).layer(sign_in_limit()),
        ),
        entry(
            "GET",
            "/cli/authorize",
            Session,
            get(ui::cli_authorize_page),
        ),
        entry(
            "POST",
            "/ui/cli/authorize",
            Session,
            post(ui::decide_cli_authorize).layer(sign_in_limit()),
        ),
        entry("GET", "/cli", Session, get(ui::cli_sign_in_page)),
        entry(
            "POST",
            "/ui/cli",
            Session,
            post(ui::decide_cli_sign_in).layer(sign_in_limit()),
        ),
        entry("GET", "/account", Session, get(ui::account_page)),
        // Sign-in with a provider. Anyone may start a plain sign-in or sign-up, which changes no
        // account; a connection starts from a signed-in page and an invitation from its own page,
        // each by a form post. The provider's answer and the two steps after it need the cookie
        // the start set.
        entry("GET", "/sign-in/{provider}", Public, get(ui::social_start)),
        entry(
            "POST",
            "/sign-in/{provider}/connect",
            Session,
            post(ui::social_connect).layer(sign_in_limit()),
        ),
        entry(
            "POST",
            "/sign-in/{provider}/invite",
            Public,
            post(ui::social_invite).layer(sign_in_limit()),
        ),
        entry(
            "GET",
            "/sign-in/{provider}/callback",
            Public,
            get(ui::social_callback),
        ),
        entry(
            "GET",
            "/sign-in/{provider}/finish",
            Session,
            get(ui::social_finish),
        ),
        entry(
            "POST",
            "/sign-in/{provider}/confirm",
            Public,
            post(ui::social_confirm).layer(sign_in_limit()),
        ),
        entry(
            "POST",
            "/ui/connections/{provider}/disconnect",
            Session,
            post(ui::social_disconnect).layer(sign_in_limit()),
        ),
        entry(
            "POST",
            "/ui/tokens",
            Session,
            post(ui::create_personal_token).layer(sign_in_limit()),
        ),
        entry(
            "POST",
            "/ui/tokens/{token}/revoke",
            Session,
            post(ui::revoke_personal_token).layer(sign_in_limit()),
        ),
        entry(
            "POST",
            "/ui/share/{org}/{slug}",
            Share,
            post(ui::share_hosted).layer(sign_in_limit()),
        ),
        // Organizations, members and invitations: any signed-in person may ask; the handler
        // answers from their membership and role in the organization named.
        entry("GET", "/members", Session, get(ui::members_page)),
        entry(
            "POST",
            "/ui/organization",
            Session,
            post(ui::switch_organization).layer(sign_in_limit()),
        ),
        entry(
            "POST",
            "/ui/orgs",
            Session,
            post(ui::create_organization).layer(sign_in_limit()),
        ),
        entry(
            "POST",
            "/ui/orgs/{org}/name",
            Session,
            post(ui::rename_organization).layer(sign_in_limit()),
        ),
        entry(
            "POST",
            "/ui/orgs/{org}/invitations",
            Session,
            post(ui::invite_member).layer(sign_in_limit()),
        ),
        entry(
            "POST",
            "/ui/orgs/{org}/invitations/{id}/resend",
            Session,
            post(ui::resend_invitation).layer(sign_in_limit()),
        ),
        entry(
            "POST",
            "/ui/orgs/{org}/invitations/{id}/cancel",
            Session,
            post(ui::cancel_invitation).layer(sign_in_limit()),
        ),
        entry(
            "POST",
            "/ui/orgs/{org}/members/{person}/role",
            Session,
            post(ui::set_member_role).layer(sign_in_limit()),
        ),
        entry(
            "POST",
            "/ui/orgs/{org}/members/{person}/remove",
            Session,
            post(ui::remove_member).layer(sign_in_limit()),
        ),
        entry(
            "POST",
            "/ui/invitations/{id}/accept",
            Session,
            post(ui::accept_invitation).layer(sign_in_limit()),
        ),
        // One report's page and the people named on it.
        entry("GET", "/reports/{org}/{slug}", View, get(ui::report_page)),
        entry(
            "POST",
            "/ui/reports/{org}/{slug}/people",
            Share,
            post(ui::share_person).layer(sign_in_limit()),
        ),
        entry(
            "POST",
            "/ui/reports/{org}/{slug}/people/remove",
            Share,
            post(ui::unshare_person).layer(sign_in_limit()),
        ),
        // The online editor: a stored report's source beside its rendering, and a new report
        // on a starter. The `edit` right is checked in the handler, which sends someone who can
        // only read the report to its page.
        entry("GET", "/reports/new", Session, get(ui::editor_new_page)),
        entry(
            "GET",
            "/reports/{org}/{slug}/edit",
            Edit,
            get(ui::editor_page),
        ),
        entry(
            "GET",
            "/reports/{org}/{slug}/delete",
            Delete,
            get(ui::delete_confirmation_page),
        ),
        entry(
            "POST",
            "/ui/reports/{org}/{slug}/delete",
            Delete,
            post(ui::delete_report).layer(sign_in_limit()),
        ),
    ]);
    entries.push(entry(
        "GET",
        "/richview_docs.js",
        Public,
        get(hosted::docs_js),
    ));
    entries.push(entry(
        "GET",
        "/richview_docs_bg.wasm",
        Public,
        get(hosted::docs_wasm),
    ));
    entries.push(entry(
        "GET",
        "/bootstrap.js",
        Public,
        get(hosted::docs_bootstrap),
    ));
    // What the online editor's page loads from the app origin: the site runtime and its render
    // worker, the font catalogs the worker provides, and the report runtime its preview runs.
    for (path, handler) in [
        ("/richview_site.js", get(ui::editor_site_js)),
        ("/richview_site_bg.wasm", get(ui::editor_site_wasm)),
        ("/worker.js", get(ui::editor_worker)),
        ("/richview-fonts.css", get(ui::editor_fonts)),
        ("/fonts-math.css", get(ui::editor_math_fonts)),
        ("/richview_runtime.js", get(ui::editor_runtime_js)),
        ("/richview_runtime_bg.wasm", get(ui::editor_runtime_wasm)),
    ] {
        entries.push(entry("GET", path, Public, handler));
    }
    entries
}

pub fn hosted_specs() -> Vec<Spec> {
    hosted_table().into_iter().map(|e| e.spec).collect()
}

/// Which origin a listener serves (ARC-15).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// One origin serves everything: `richview serve` and the unsplit server.
    All,
    /// The app origin: sign-in, governance, the API and the pages around the reports.
    App,
    /// The report origin: report pages and their files, and nothing else.
    Report,
    /// `richview serve`: the preview, its runtime, its revision poll and its thumbnail.
    Preview,
}

/// Report pages, their files, downloads, cards and the runtime they link.
fn is_content(path: &str) -> bool {
    path.starts_with("/r/") || path.starts_with("/_rv/")
}

/// The router, built from the table; `preview` mounts the preview's revision poll as well.
pub fn router(preview: bool, side: Side) -> Router<AppState> {
    router_for(preview, side, false)
}

pub(crate) fn router_for(preview: bool, side: Side, hosted: bool) -> Router<AppState> {
    let mut by_path: BTreeMap<&'static str, MethodRouter<AppState>> = BTreeMap::new();
    for entry in if hosted { hosted_table() } else { table() } {
        if entry.spec.rule == Rule::Preview && !preview {
            continue;
        }
        if side == Side::Preview
            && !matches!(
                entry.spec.path,
                "/" | "/__revision" | "/thumbnail" | "/_rv/{name}" | "/health"
            )
        {
            continue;
        }
        let content = is_content(entry.spec.path);
        if (side == Side::App && content)
            || (side == Side::Report && !content && entry.spec.path != "/health")
        {
            continue;
        }
        let merged = match by_path.remove(entry.spec.path) {
            Some(existing) => existing.merge(entry.handler),
            None => entry.handler,
        };
        by_path.insert(entry.spec.path, merged);
    }
    by_path
        .into_iter()
        .fold(Router::new(), |router, (path, handler)| {
            router.route(path, handler)
        })
}
