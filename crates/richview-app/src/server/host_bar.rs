//! The report host's bar above a hosted report: whose report it is, who can open it, and the way
//! back to the app. It is the server's own markup, inserted when a report is served; the report's
//! HTML is otherwise untouched, and the bar sits above the report, never over it.
//!
//! A public report's response is cached as `public`, so it carries the one bar every reader gets
//! and names no person or organization. Only a response that is never cached (`private,
//! no-store`) carries a bar written for the person reading it.

use super::AppState;
use crate::access::{Principal, Visibility};

const CSS: &str = include_str!("host-bar.css");

const MARK: &str = "<svg viewBox=\"0 0 268.58 269.73\" aria-hidden=\"true\"><g fill=\"currentColor\"><path d=\"M221.4,51.39 H0 V0 h221.4 c7.32,0 13.25,5.93 13.25,13.25 v24.89 c0,7.32 -5.93,13.25 -13.25,13.25 z\"/><path d=\"M255.32,124.17 H0 V72.78 h255.32 c7.32,0 13.25,5.93 13.25,13.25 v24.89 c0,7.32 -5.93,13.25 -13.25,13.25 z\"/><path d=\"M221.4,196.95 H0 v-51.39 h221.4 c7.32,0 13.25,5.93 13.25,13.25 v24.89 c0,7.32 -5.93,13.25 -13.25,13.25 z\"/><path d=\"M93.12,269.73 H0 v-51.39 h93.12 c7.32,0 13.25,5.93 13.25,13.25 v24.89 c0,7.32 -5.93,13.25 -13.25,13.25 z\"/><circle cx=\"242.88\" cy=\"244.04\" r=\"25.7\"/></g></svg>";

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn render(says: &str, go: &str) -> String {
    format!(
        "<div class=\"rv-host-bar\" role=\"region\" aria-label=\"RichView\"><div class=\"rv-in\"><p class=\"rv-says\">{MARK}<span>{says}</span></p><nav class=\"rv-go\" aria-label=\"RichView\">{go}</nav></div></div>"
    )
}

/// The bar for this reader and report on the report host, or `None` when there is none: off the
/// hosted service, without an app address, or for a reader the server does not know.
pub(super) fn bar(
    state: &AppState,
    principal: &Principal,
    key: &str,
    public: bool,
) -> Option<String> {
    if !state.access.is_hosted() {
        return None;
    }
    let app = state.public_origin.as_deref()?.trim_end_matches('/');
    let app = escape(app);
    if public {
        return Some(render(
            "<span class=\"rv-soft\">Published with</span> <strong>RichView</strong><span class=\"rv-soft rv-wide\"> · anyone with the link can open it</span>",
            &format!("<a href=\"{app}/\">Sign in</a>"),
        ));
    }
    let Principal::User(user) = principal else {
        return None;
    };
    let access = state.access.report_access(key)?;
    let acl = state.access.acl(key);
    let org = escape(&access.org_name);
    // An organization made at sign-up carries its owner's name until it is renamed: name them once.
    let own = access.owner_name.trim().to_lowercase() == access.org_name.trim().to_lowercase();
    let by = if access.owner_name.is_empty() {
        String::new()
    } else {
        format!(" · by {}", escape(&access.owner_name))
    };
    let from = if own {
        String::new()
    } else {
        format!("<span class=\"rv-soft rv-wide\"> · {org}</span>")
    };
    // On a phone the organization and the longer clauses give way, so the bar stays one line.
    let all = format!("<a href=\"{app}/\">All reports</a>");
    if acl.owner == user.id {
        // A phone shows the badge and the button; the count of people is for wider screens.
        let (badge, who) = match access.visibility {
            Visibility::Org => (
                format!("<span class=\"rv-vis\">{org}</span>"),
                "every member can open it".to_owned(),
            ),
            _ => (
                "<span class=\"rv-vis\">Private</span>".to_owned(),
                match access.named.len() {
                    0 => "not shared with anyone".to_owned(),
                    1 => "shared with 1 person".to_owned(),
                    n => format!("shared with {n} people"),
                },
            ),
        };
        // The author navigates from here too (Daniel, 16 Nov): back to the list, then Share. A
        // phone has room for one of them beside the badge, and keeps Share, which leads to the app.
        return Some(render(
            &format!("Your report {badge}<span class=\"rv-soft rv-wide\"> · {who}</span>"),
            &format!(
                "<a class=\"rv-wide\" href=\"{app}/\">All reports</a><a class=\"rv-pill\" href=\"{app}/reports/{}\">Share</a>",
                escape(key)
            ),
        ));
    }
    let named = access
        .named
        .iter()
        .any(|row| !row.pending && row.email.eq_ignore_ascii_case(&user.email));
    if named {
        return Some(render(
            &format!("<strong>Shared with you</strong><span class=\"rv-soft\">{by}</span>{from}"),
            &format!(
                "<a href=\"{app}/?show=shared\"><span class=\"rv-wide\">Find it again under Shared with me</span><span class=\"rv-narrow\">Find it again</span></a>"
            ),
        ));
    }
    if access.visibility == Visibility::Org {
        return Some(render(
            &format!(
                "From <strong>{org}</strong><span class=\"rv-soft rv-wide\"> · every member can open it</span>"
            ),
            &all,
        ));
    }
    Some(render(
        &if own {
            format!("<strong>{org}</strong>")
        } else {
            format!("<strong>{org}</strong><span class=\"rv-soft\">{by}</span>")
        },
        &all,
    ))
}

/// The report with the bar: its style at the end of the head, its markup first in the body.
/// A page without both tags is returned as it is.
pub(super) fn insert(html: &str, bar: &str) -> String {
    let (Some(head), Some(body)) = (html.find("</head>"), html.find("<body")) else {
        return html.to_owned();
    };
    let Some(open) = html[body..].find('>').map(|end| body + end + 1) else {
        return html.to_owned();
    };
    if head > body {
        return html.to_owned();
    }
    format!(
        "{}<style>{CSS}</style>{}{bar}{}",
        &html[..head],
        &html[head..open],
        &html[open..]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bar_goes_first_in_the_body_and_its_style_last_in_the_head() {
        let page = "<!doctype html><html><head><style>a{}</style></head><body><article>x</article></body></html>";
        let out = insert(page, "<div class=\"rv-host-bar\">B</div>");
        assert!(out.contains("<style>a{}</style><style>"));
        assert!(
            out.contains("</head><body><div class=\"rv-host-bar\">B</div><article>x</article>")
        );
        assert!(out.ends_with("</body></html>"));
    }

    #[test]
    fn a_page_without_a_head_and_body_is_served_as_it_is() {
        assert_eq!(insert("<p>plain</p>", "B"), "<p>plain</p>");
        assert_eq!(insert("<body></body></head>", "B"), "<body></body></head>");
    }

    #[test]
    fn typed_names_cannot_add_markup_to_the_bar() {
        let out = render(&escape("<b>Acme</b>"), "");
        assert!(out.contains("&lt;b&gt;Acme&lt;/b&gt;"));
        assert!(!out.contains("<b>"));
    }
}
