//! The online editor's page and the files it runs on (the hosted app only). A signed-in person
//! who may edit a report opens its source beside the report it renders; the new-report page opens
//! the same editor on a starter. The page is a shell: the site runtime on it, the playground's
//! own renderer, reads the stored report through the API and saves a new version through the
//! publish API. Nothing here renders a report or accepts an edit.
use super::{
    AppState, EDITORIAL_FONTS, UI_FONTS, auth, display_fonts, escape, hosted_pages, hosted_session,
    icon, redirect, site_rules,
};
use crate::access::{OrgRole, Right};
use axum::{
    extract::{Path as AxumPath, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};

/// The source's colours, the completion list and the line numbers: the playground's own sheet.
const SYNTAX_CSS: &str = include_str!("../../../../assets/landing/editor.css");
const EDITOR_CSS: &str = include_str!("editor.css");

/// The styles the pills offer, in the order the pills show them, with the pill's words (STY-08).
const PILLS: [(&str, &str); 5] = [
    ("sleek", "Sleek"),
    ("air", "Air"),
    ("scientific", "Scientific"),
    ("brutalist", "Brutalist"),
    ("hand-drawn", "Hand-drawn"),
];

/// What the Insert strip adds, as the client names it, with the strip's words.
const INSERTS: [(&str, &str); 4] = [
    ("chart", "Chart"),
    ("table", "Table"),
    ("metric", "Metric"),
    ("callout", "Callout"),
];

/// What the shell needs to say about the report it opens.
struct EditorPage<'a> {
    /// Who is editing: the page keeps their drafts under it, apart from anybody else's.
    person: &'a str,
    /// The organization the report lives in, or the one a new report will be created in.
    org: &'a str,
    /// The report's address within the organization; empty for a report not saved yet.
    slug: &'a str,
    /// The latest stored version; empty for a report not saved yet.
    version: &'a str,
    title: &'a str,
    csrf: &'a str,
}

impl EditorPage<'_> {
    fn is_new(&self) -> bool {
        self.slug.is_empty()
    }

    /// The report's own page, where its versions and sharing are.
    fn report_path(&self) -> String {
        format!("/reports/{}/{}", self.org, self.slug)
    }
}

fn report_key(state: &AppState, org: &str, slug: &str) -> Result<String, Response> {
    let key = format!("{org}/{slug}");
    state
        .storage
        .validate_key(&key)
        .map_err(|_| auth::not_found())?;
    Ok(key)
}

/// `GET /reports/{org}/{slug}/edit`: the editor on a stored report, for whoever holds the `edit`
/// right on it. Someone who can read it but not edit it is taken to its page, which says so; for
/// anyone else the report does not exist.
pub async fn editor_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((org, slug)): AxumPath<(String, String)>,
) -> Response {
    let key = match report_key(&state, &org, &slug) {
        Ok(key) => key,
        Err(response) => return response,
    };
    let caller = auth::caller(&state, &headers);
    let hosted = match hosted_session(&state, &caller) {
        Ok(hosted) => hosted,
        Err(response) => return response,
    };
    if let Err(response) = auth::authorize(&state, &caller, &key, Right::View) {
        return response;
    }
    let page = format!("/reports/{key}");
    if !state
        .access
        .rights(&caller.principal, &key)
        .contains(&Right::Edit)
    {
        return redirect(&page, Some("You can read this report but not edit it."));
    }
    if !state.access.report_org_active(&key) {
        return redirect(&page, Some("This organization is suspended."));
    }
    let storage = state.storage.clone();
    let lookup = key.clone();
    let latest = match tokio::task::spawn_blocking(move || storage.summary(&lookup, None)).await {
        Ok(Ok(Some(latest))) => latest,
        Ok(Ok(None)) => return auth::not_found(),
        _ => return super::hosted_error_page(StatusCode::INTERNAL_SERVER_ERROR),
    };
    let title = latest["frontmatter"]["title"]
        .as_str()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or(&slug);
    answer(&EditorPage {
        person: &hosted.person.id,
        org: &org,
        slug: &slug,
        version: latest["version"].as_str().unwrap_or(""),
        title,
        csrf: &hosted.csrf,
    })
}

/// `GET /reports/new`: the editor on a starter, for a person who may publish in their current
/// organization. The report exists only once the first save publishes it.
pub async fn editor_new_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let caller = auth::caller(&state, &headers);
    let hosted = match hosted_session(&state, &caller) {
        Ok(hosted) => hosted,
        Err(response) => return response,
    };
    let (Some(org), Some(role)) = (&hosted.org, hosted.role) else {
        return redirect("/", Some("Create or join an organization first."));
    };
    if org.suspended {
        return redirect("/", Some("This organization is suspended."));
    }
    if role == OrgRole::Reader {
        return redirect("/", Some("Readers open reports; authors create them."));
    }
    answer(&EditorPage {
        person: &hosted.person.id,
        org: &org.id,
        slug: "",
        version: "",
        title: "Untitled report",
        csrf: &hosted.csrf,
    })
}

/// The page with the policy that lets it run the editor: the report runtime's one inline module,
/// pinned by hash for the sandboxed preview frame that inherits this policy, the worker and the
/// frame sources the playground's own page allows. Reports and their files are fetched, never
/// framed from another origin.
fn answer(page: &EditorPage<'_>) -> Response {
    let policy = policy(crate::runtime::preview_script_hash());
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
            (header::REFERRER_POLICY, "no-referrer"),
            (header::CONTENT_SECURITY_POLICY, policy.as_str()),
        ],
        document(page),
    )
        .into_response()
}

fn policy(preview_module: Option<&str>) -> String {
    format!(
        "default-src 'none'; script-src 'self' 'wasm-unsafe-eval'{}; style-src 'unsafe-inline'; img-src data: blob:; font-src data:; connect-src 'self'; frame-src about: blob: data:; worker-src 'self' blob:; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
        preview_module.map_or(String::new(), |hash| format!(" {hash}"))
    )
}

fn icon_use(name: &str) -> String {
    format!("<svg class=\"ic\" aria-hidden=\"true\"><use href=\"#i-{name}\"/></svg>")
}

/// The whole document: the app's tokens, typefaces and sprite, this page's sheet, and the markup
/// the client script binds to by id (`richview-site`'s `browser/hosted_editor.rs`).
fn document(page: &EditorPage<'_>) -> String {
    let is_new = page.is_new();
    let report = page.report_path();
    let state = if is_new {
        "New report".to_owned()
    } else {
        format!("Editing v{}", page.version)
    };
    // Before a first save there is no report page to open.
    let hidden = if is_new { " hidden" } else { "" };
    let (href, back) = if is_new {
        (String::from("/"), "/")
    } else {
        (report.clone(), report.as_str())
    };
    let inserts: String = INSERTS
        .iter()
        .map(|(name, label)| {
            format!(
                "<button class=\"ed-add\" type=\"button\" data-add=\"{name}\">{}{label}</button>",
                icon_use("plus")
            )
        })
        .collect();
    let pills: String = PILLS
        .iter()
        .map(|(name, label)| {
            format!(
                "<button type=\"button\" role=\"radio\" aria-checked=\"false\" tabindex=\"-1\" data-style=\"{name}\">{}{label}</button>",
                icon_use("check")
            )
        })
        .collect();
    let (pop_title, pop_text) = if is_new {
        (
            "Save the report",
            "The first save creates the report, private to you. You choose who can open it afterwards.",
        )
    } else {
        (
            "Save a new version",
            "It goes to this report\u{2019}s address. People it is shared with see it at once; every earlier version stays in History.",
        )
    };
    let body = format!(
        "<body class=\"app-editor\" data-richview-page=\"editor\" data-person=\"{person}\" data-org=\"{org}\" data-slug=\"{slug}\" data-version=\"{version}\" data-csrf=\"{csrf}\" data-new=\"{is_new}\" data-pane=\"source\">\
{icons}\
<a class=\"skip\" href=\"#source\">Skip to the source</a>\
<header class=\"ed-top\">\
<a class=\"ed-back\" href=\"/\" aria-label=\"Back to reports\">{back_icon}<span>Reports</span></a>\
<span class=\"ed-sep\" aria-hidden=\"true\"></span>\
<div class=\"ed-title\"><strong id=\"ed-title\">{title}</strong><span class=\"ed-state\" id=\"ed-state\">{state}</span></div>\
<div class=\"ed-tools\">\
<a class=\"ed-btn\" id=\"ed-history\" href=\"{href}\" aria-label=\"History\"{hidden}>{clock}<span class=\"ed-lab\">History</span></a>\
<a class=\"ed-btn ed-btn--line\" id=\"ed-share\" href=\"{href}\" aria-label=\"Share\"{hidden}>{share}<span class=\"ed-lab\">Share</span></a>\
<button class=\"ed-btn ed-btn--primary\" id=\"ed-save\" type=\"button\" aria-haspopup=\"dialog\" aria-controls=\"ed-pop\"><span class=\"ed-long\">Save version</span><span class=\"ed-short\">Save</span><kbd class=\"ed-lab\">\u{2318}S</kbd></button>\
</div></header>\
<div class=\"ed-tabs\" role=\"tablist\" aria-label=\"Pane\"><button type=\"button\" role=\"tab\" id=\"ed-tab-source\" aria-selected=\"true\">Source</button><button type=\"button\" role=\"tab\" id=\"ed-tab-report\" aria-selected=\"false\">Report</button></div>\
<div class=\"ed-work\" id=\"ed-work\">\
<section class=\"ed-src\" aria-label=\"Source\">\
<noscript><p class=\"ed-nojs\">The editor needs JavaScript. <a href=\"{back}\">Go back</a></p></noscript>\
<div class=\"ed-insert\" id=\"ed-insert\" role=\"toolbar\" aria-label=\"Insert\"><span>Insert</span>{inserts}</div>\
<div class=\"pg__code ed-code\" data-language=\"rmd\"><pre class=\"code__hl\" aria-hidden=\"true\"><code></code></pre><textarea id=\"source\" spellcheck=\"false\" autocapitalize=\"off\" autocomplete=\"off\" wrap=\"soft\" aria-label=\"Rich Markdown source\"></textarea></div>\
</section>\
<div class=\"ed-split\" id=\"ed-split\" role=\"separator\" aria-orientation=\"vertical\" aria-label=\"Resize the source and the report\" aria-valuemin=\"25\" aria-valuemax=\"75\" aria-valuenow=\"50\" tabindex=\"0\"></div>\
<section class=\"ed-view\" aria-label=\"Report\">\
<div class=\"ed-view__bar\"><strong>Report</strong><div class=\"pills\" id=\"ed-styles\" role=\"radiogroup\" aria-label=\"Style\">{pills}</div></div>\
<p class=\"ed-banner\" id=\"ed-banner\" role=\"status\" hidden>{alert}<span id=\"ed-banner-text\"></span><button type=\"button\" id=\"ed-banner-go\" hidden></button></p>\
<div class=\"ed-sheet\"><iframe id=\"preview\" title=\"Rendered report\" sandbox=\"allow-scripts\"></iframe></div>\
</section>\
<section class=\"ed-list\" id=\"ed-problems\" aria-label=\"Problems\" hidden><h2>Problems</h2><p id=\"ed-problems-empty\">No problems.</p><ul id=\"ed-problems-list\"></ul></section>\
</div>\
<div class=\"ed-pop\" id=\"ed-pop\" role=\"dialog\" aria-labelledby=\"ed-pop-title\" hidden>\
<h2 id=\"ed-pop-title\">{pop_title}</h2><p>{pop_text}</p>\
<div id=\"ed-slug-row\" hidden><label for=\"ed-slug\">Address</label><input id=\"ed-slug\" autocomplete=\"off\" spellcheck=\"false\" autocapitalize=\"off\"><p class=\"hint\">Letters, numbers and dashes. It is the report\u{2019}s address and does not change later.</p></div>\
<label for=\"ed-version\">Version</label><input id=\"ed-version\" autocomplete=\"off\" spellcheck=\"false\" autocapitalize=\"off\"><p class=\"hint\">The editor writes it into the source\u{2019}s <code>version</code> line.</p>\
<p class=\"ed-pop__note\" id=\"ed-pop-note\" role=\"status\" hidden></p>\
<button class=\"ed-pop__link\" type=\"button\" id=\"ed-reload\" hidden>Drop my changes and reload</button>\
<p class=\"ed-pop__error\" id=\"ed-pop-error\" role=\"alert\" hidden></p>\
<div class=\"ed-pop__acts\"><button type=\"button\" id=\"ed-pop-cancel\">Cancel</button><button type=\"button\" class=\"go\" id=\"ed-pop-go\">Save version</button></div>\
</div>\
<footer class=\"ed-status\">\
<button type=\"button\" class=\"ed-count\" id=\"ed-count\" data-state=\"ok\" aria-expanded=\"false\" aria-controls=\"ed-problems\">{check}{alert}<span id=\"ed-count-text\">No problems</span></button>\
<span class=\"ed-hide\" id=\"ed-pos\">Ln 1, Col 1</span>\
<span class=\"ed-push ed-hide\" id=\"ed-time\"></span>\
<span class=\"ed-hide\" id=\"ed-draft\"></span>\
</footer>\
<div class=\"ed-toast\" id=\"ed-toast\" hidden><span id=\"ed-toast-text\"></span> <a id=\"ed-toast-link\" href=\"{href}\">Open the report page</a></div>\
<p class=\"sr-only\" id=\"ed-live\" role=\"status\" aria-live=\"polite\"></p>\
<script src=\"/richview_site.js\" defer></script><script src=\"/bootstrap.js\" defer></script>\
</body>",
        person = escape(page.person),
        org = escape(page.org),
        slug = escape(page.slug),
        version = escape(page.version),
        csrf = escape(page.csrf),
        title = escape(page.title),
        state = escape(&state),
        href = escape(&href),
        back = escape(back),
        icons = hosted_pages::ICONS,
        back_icon = icon_use("back"),
        clock = icon_use("clock"),
        share = icon_use("share"),
        alert = icon_use("alert"),
        check = icon_use("check"),
    );
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta name=\"robots\" content=\"noindex\"><title>{} \u{b7} RichView</title>{}<style>{}\n{EDITORIAL_FONTS}\n{UI_FONTS}\n{}\n{SYNTAX_CSS}\n{EDITOR_CSS}</style></head>{body}</html>",
        escape(&format!("Edit {}", page.title)),
        icon(),
        site_rules(),
        display_fonts(),
    )
}

/// One file the editor loads from the app's own origin: bytes the binary carries.
fn asset(bytes: Option<&'static [u8]>, mime: &'static str) -> Response {
    match bytes {
        Some(bytes) => (
            [
                (header::CONTENT_TYPE, mime),
                (header::CACHE_CONTROL, "no-cache"),
            ],
            bytes,
        )
            .into_response(),
        None => (StatusCode::SERVICE_UNAVAILABLE, "The editor is unavailable").into_response(),
    }
}

const JS: &str = "application/javascript; charset=utf-8";
const CSS: &str = "text/css; charset=utf-8";

pub async fn editor_site_js() -> Response {
    asset(crate::runtime::SITE_JS, JS)
}
pub async fn editor_site_wasm() -> Response {
    asset(crate::runtime::SITE_WASM, "application/wasm")
}
pub async fn editor_worker() -> Response {
    asset(crate::runtime::SITE_WORKER, JS)
}
pub async fn editor_fonts() -> Response {
    asset(crate::runtime::SITE_FONTS, CSS)
}
pub async fn editor_math_fonts() -> Response {
    asset(crate::runtime::SITE_MATH_FONTS, CSS)
}

/// The report runtime the editor's preview frame runs, fetched by the page and handed to the
/// frame in its source: the same bytes every report carries.
pub async fn editor_runtime_js() -> Response {
    match crate::runtime::embedded() {
        Some(runtime) => (
            [
                (header::CONTENT_TYPE, JS),
                (header::CACHE_CONTROL, "no-cache"),
            ],
            runtime.js.clone(),
        )
            .into_response(),
        None => asset(None, JS),
    }
}
pub async fn editor_runtime_wasm() -> Response {
    asset(crate::runtime::raw_wasm(), "application/wasm")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page<'a>(slug: &'a str, version: &'a str, title: &'a str) -> EditorPage<'a> {
        EditorPage {
            person: "p-ada",
            org: "g02nkloj",
            slug,
            version,
            title,
            csrf: "csrf-token",
        }
    }

    /// Every id the client script binds to by name, so a rename on either side fails here.
    const IDS: &[&str] = &[
        "ed-title",
        "ed-state",
        "ed-save",
        "ed-history",
        "ed-share",
        "ed-tab-source",
        "ed-tab-report",
        "ed-insert",
        "source",
        "ed-styles",
        "preview",
        "ed-banner",
        "ed-banner-text",
        "ed-banner-go",
        "ed-problems",
        "ed-problems-list",
        "ed-problems-empty",
        "ed-count",
        "ed-count-text",
        "ed-pos",
        "ed-time",
        "ed-draft",
        "ed-split",
        "ed-work",
        "ed-pop",
        "ed-version",
        "ed-slug-row",
        "ed-slug",
        "ed-pop-note",
        "ed-reload",
        "ed-pop-error",
        "ed-pop-cancel",
        "ed-pop-go",
    ];
    const MORE_IDS: [&str; 3] = ["ed-live", "ed-toast", "ed-toast-text"];

    #[test]
    fn the_page_carries_every_id_and_attribute_the_client_binds_to() {
        let html = document(&page("first-quarter", "0.0.1", "Q1"));
        for id in IDS.iter().chain(&MORE_IDS).chain(&["ed-toast-link"]) {
            assert!(html.contains(&format!("id=\"{id}\"")), "no #{id}");
        }
        for attribute in [
            "data-richview-page=\"editor\"",
            "data-person=\"p-ada\"",
            "data-org=\"g02nkloj\"",
            "data-slug=\"first-quarter\"",
            "data-version=\"0.0.1\"",
            "data-csrf=\"csrf-token\"",
            "data-new=\"false\"",
            "data-pane=\"source\"",
        ] {
            assert!(html.contains(attribute), "no {attribute}");
        }
        assert!(html.contains("<div class=\"pg__code ed-code\" data-language=\"rmd\"><pre class=\"code__hl\" aria-hidden=\"true\"><code></code></pre><textarea id=\"source\""));
        assert!(html.contains(
            "<iframe id=\"preview\" title=\"Rendered report\" sandbox=\"allow-scripts\">"
        ));
        assert!(html.contains("<script src=\"/richview_site.js\" defer></script><script src=\"/bootstrap.js\" defer></script>"));
        assert!(
            !html.contains("richview_docs.js"),
            "the page starts the site runtime, not the docs one"
        );
    }

    #[test]
    fn a_saved_report_links_its_page_and_a_new_one_does_not_until_it_is_saved() {
        let saved = document(&page("first-quarter", "0.0.1", "Q1"));
        assert!(saved.contains(
            "id=\"ed-history\" href=\"/reports/g02nkloj/first-quarter\" aria-label=\"History\">"
        ));
        assert!(saved.contains("Editing v0.0.1"));
        assert!(saved.contains("data-new=\"false\""));
        let new = document(&page("", "", "Untitled report"));
        assert!(new.contains("data-new=\"true\""));
        assert!(new.contains("id=\"ed-history\" href=\"/\" aria-label=\"History\" hidden>"));
        assert!(new.contains("id=\"ed-share\" href=\"/\" aria-label=\"Share\" hidden>"));
        assert!(new.contains("New report"));
        assert!(new.contains("The first save creates the report, private to you."));
    }

    #[test]
    fn a_title_or_an_address_cannot_break_out_of_the_page() {
        let html = document(&page(
            "x\"><script>1</script>",
            "1\"",
            "<img src=x onerror=alert(1)>",
        ));
        assert!(!html.contains("<img src=x"));
        assert!(!html.contains("<script>1</script>"));
        assert!(html.contains("&lt;img src=x onerror=alert(1)&gt;"));
        assert!(html.contains("<title>Edit &lt;img"));
    }

    #[test]
    fn the_pills_are_the_builtin_styles_and_the_inserts_are_ones_the_client_knows() {
        let mut offered: Vec<_> = PILLS.iter().map(|(name, _)| *name).collect();
        offered.sort_unstable();
        let mut builtin = richview_core::builtin_style_names().to_vec();
        builtin.sort_unstable();
        assert_eq!(offered, builtin);
        let html = document(&page("a", "1.0.0", "A"));
        for (name, _) in INSERTS {
            assert!(html.contains(&format!("data-add=\"{name}\"")));
        }
        assert!(
            !html.contains("<select"),
            "STY-08: the styles are pills, not a select"
        );
    }

    #[test]
    fn the_policy_pins_the_preview_module_and_allows_the_worker_and_nothing_remote() {
        let policy = policy(Some("'sha256-abc='"));
        assert!(policy.contains("script-src 'self' 'wasm-unsafe-eval' 'sha256-abc=';"));
        assert!(policy.contains("worker-src 'self' blob:"));
        assert!(policy.contains("frame-src about: blob: data:"));
        assert!(policy.contains("connect-src 'self'"));
        assert!(policy.contains("frame-ancestors 'none'"));
        assert!(policy.contains("default-src 'none'"));
        assert!(!policy.contains("http"), "no remote source: {policy}");
        assert!(!policy.contains("'unsafe-eval'"));
        assert!(
            !policy.contains("script-src 'self' 'wasm-unsafe-eval' ;"),
            "an absent hash leaves no stray space"
        );
        assert!(super::policy(None).contains("script-src 'self' 'wasm-unsafe-eval';"));
    }

    #[test]
    fn the_sheet_does_not_depend_on_a_remote_font_or_image() {
        assert!(!EDITOR_CSS.contains("url(http"));
        assert!(!EDITOR_CSS.contains("@import"));
    }
}
