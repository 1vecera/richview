//! The report host's bar above a hosted report: whose report it is, who can open it, and the way
//! back to the app. It is the server's own markup, inserted when a report is served; the report's
//! HTML is otherwise untouched, and the bar sits above the report, never over it.
//!
//! A public report's response is cached as `public`, so it carries the one bar every reader gets
//! and may name the publisher declared by that version's style. Only a private response that the browser
//! cannot store carries a bar written for the person reading it.

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

/// Whether a mark is a lockup: wider than 2.5 times its height, so its artwork already carries the
/// company name and the bar shows it alone (FEA-SF-66). The shape is read from the dimensions
/// stored with the validated image, when the version is published; nothing is measured later.
fn is_lockup(logo: &richview_core::CompanyLogo) -> bool {
    u64::from(logo.width) * 2 > u64::from(logo.height) * 5
}

/// Freeze only the author's publishing credit. A report owner or viewer never supplies this markup.
/// A lockup stands alone with the name as its alt text; a squarer mark keeps the name beside it.
pub(super) fn publisher_credit(brand: &richview_core::CompanyBrand) -> Option<String> {
    if brand.name.is_empty() {
        return None;
    }
    let name = escape(&brand.name);
    let Some(light) = brand.logo.as_ref() else {
        return Some(format!(
            "<span class=\"rv-publisher\" title=\"{name}\"><strong>{name}</strong></span>"
        ));
    };
    let logo = if brand.dark_surface {
        brand.logo_dark.as_ref().unwrap_or(light)
    } else {
        light
    };
    let plate = if brand.dark_surface && brand.logo_dark.is_none() {
        " class=\"rv-credit-plate\""
    } else {
        ""
    };
    let lockup = is_lockup(logo);
    let mark = format!(
        "<img{plate} src=\"{}\" width=\"{}\" height=\"{}\" alt=\"{}\">",
        logo.data_uri(),
        logo.width,
        logo.height,
        if lockup { name.as_str() } else { "" }
    );
    Some(if lockup {
        format!("<span class=\"rv-publisher rv-publisher--lockup\" title=\"{name}\">{mark}</span>")
    } else {
        format!(
            "<span class=\"rv-publisher\" title=\"{name}\">{mark}<strong>{name}</strong></span>"
        )
    })
}

/// The bar: the mark, then whom the report comes from as a face (a person's initials in a circle,
/// an organization's initial in a square), what it says, and its ways on as buttons.
fn render(publisher: Option<&str>, face: &str, says: &str, go: &str) -> String {
    let mark = publisher.unwrap_or(MARK);
    let face = if face.is_empty() {
        String::new()
    } else {
        format!("<span class=\"rv-sep\" aria-hidden=\"true\"></span>{face}")
    };
    format!(
        "<div class=\"rv-host-bar\" role=\"region\" aria-label=\"RichView\"><div class=\"rv-in\"><p class=\"rv-says\">{mark}{face}<span>{says}</span></p><nav class=\"rv-go\" aria-label=\"RichView\">{go}</nav></div></div>"
    )
}

/// A person's initials in a circle, or an organization's initial in a square.
fn face(name: &str, organization: bool) -> String {
    let letters = if organization {
        name.trim()
            .chars()
            .next()
            .map(|c| c.to_uppercase().collect())
            .unwrap_or_default()
    } else {
        super::ui::initials(name)
    };
    if letters.is_empty() {
        return String::new();
    }
    format!(
        "<span class=\"rv-face{}\" aria-hidden=\"true\">{}</span>",
        if organization { " rv-face--org" } else { "" },
        escape(&letters)
    )
}

fn public_bar(app: &str, publisher: Option<&str>) -> String {
    let says = if publisher.is_some() {
        "<span class=\"rv-soft rv-wide\">anyone with the link can open it</span>"
    } else {
        "<span class=\"rv-soft\">Published with</span> <strong>RichView</strong><span class=\"rv-soft rv-wide\"> · anyone with the link can open it</span>"
    };
    render(
        publisher,
        "",
        says,
        &format!("<a href=\"{app}/\">Sign in</a>"),
    )
}

/// The bar for this reader and report on the report host, or `None` when there is none: off the
/// hosted service, without an app address, or for a reader the server does not know.
pub(super) fn bar(
    state: &AppState,
    principal: &Principal,
    key: &str,
    public: bool,
    publisher: Option<&str>,
) -> Option<String> {
    if !state.access.is_hosted() {
        return None;
    }
    let app = state.public_origin.as_deref()?.trim_end_matches('/');
    let app = escape(app);
    if public {
        return Some(public_bar(&app, publisher));
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
            publisher,
            "",
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
            publisher,
            &face(&access.owner_name, false),
            &format!("<strong>Shared with you</strong><span class=\"rv-soft\">{by}</span>{from}"),
            &format!(
                "<a href=\"{app}/?show=shared\"><span class=\"rv-wide\">Find it again under Shared with me</span><span class=\"rv-narrow\">Find it again</span></a>"
            ),
        ));
    }
    if access.visibility == Visibility::Org {
        return Some(render(
            publisher,
            &face(&access.org_name, true),
            &format!(
                "From <strong>{org}</strong><span class=\"rv-soft rv-wide\"> · every member can open it</span>"
            ),
            &all,
        ));
    }
    Some(render(
        publisher,
        &face(&access.org_name, true),
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
pub(super) fn insert(html: &str, bar: &str, branded: bool) -> String {
    let (Some(head), Some(body)) = (html.find("</head>"), html.find("<body")) else {
        return html.to_owned();
    };
    let Some(open) = html[body..].find('>').map(|end| body + end + 1) else {
        return html.to_owned();
    };
    if head > body {
        return html.to_owned();
    }
    let brand_css = if branded {
        include_str!("host-credit.css")
    } else {
        ""
    };
    format!(
        "{}<style>{CSS}{brand_css}</style>{}{bar}{}",
        &html[..head],
        &html[head..open],
        &html[open..]
    )
}

/// Stage a working documentation example with the same publisher credit and public controls as the host.
pub fn public_report_example(report: &richview_core::document::RenderedReport) -> String {
    let publisher = publisher_credit(&report.company);
    let bar = public_bar("https://app.richview.ai", publisher.as_deref());
    insert(&report.html, &bar, publisher.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_credit_uses_only_the_resolved_name_and_a_validated_mark_for_the_report_surface() {
        let mut style = richview_core::resolve_styles(&["air".into()], &mut Vec::new());
        style.document["company"] = serde_json::json!({"name": "North & <West>", "logo": "day.bin", "logo-dark": "night.bin"});
        use base64::{Engine, engine::general_purpose::STANDARD};
        let resource = |bytes: &[u8]| {
            format!(
                "data:application/octet-stream;base64,{}",
                STANDARD.encode(bytes)
            )
        };
        let day = include_bytes!("../../../../examples/company-style/logo.png");
        let night = include_bytes!("../../../../examples/company-style/logo-dark.png");
        let data = std::collections::BTreeMap::from([
            ("day.bin".into(), resource(day)),
            ("night.bin".into(), resource(night)),
        ]);
        for (scheme, expected) in [("light", day.as_slice()), ("dark", night.as_slice())] {
            style.scheme = scheme.into();
            let mut diagnostics = Vec::new();
            let brand = richview_core::resolve_company_brand(&style, &data, &mut diagnostics);
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
            let credit = publisher_credit(&brand).unwrap();
            assert!(credit.contains(&format!(
                "src=\"data:image/png;base64,{}\"",
                STANDARD.encode(expected)
            )));
            // The example mark is 872 x 144: a lockup, shown alone with the name as its alt text.
            assert!(credit.contains("rv-publisher rv-publisher--lockup"));
            assert!(credit.contains("alt=\"North &amp; &lt;West&gt;\""));
            assert!(!credit.contains("<strong>"));
            assert!(!credit.contains("rv-credit-plate"));
        }
        style.document["company"]
            .as_object_mut()
            .unwrap()
            .remove("logo-dark");
        let brand = richview_core::resolve_company_brand(&style, &data, &mut Vec::new());
        assert!(
            publisher_credit(&brand)
                .unwrap()
                .contains("rv-credit-plate")
        );

        for invalid in ["https://example.invalid/logo.png", "vector.bin"] {
            style.document["company"]["logo"] = serde_json::json!(invalid);
            let mut invalid_data = data.clone();
            invalid_data.insert("vector.bin".into(), resource(b"<svg/>"));
            let mut diagnostics = Vec::new();
            let brand =
                richview_core::resolve_company_brand(&style, &invalid_data, &mut diagnostics);
            assert!(!diagnostics.is_empty());
            let credit = publisher_credit(&brand).unwrap();
            assert!(!credit.contains("<img"));
            assert!(credit.contains("<strong>North &amp; &lt;West&gt;</strong>"));
        }
        style.document["company"] = serde_json::json!({"logo":"day.bin"});
        let brand = richview_core::resolve_company_brand(&style, &data, &mut Vec::new());
        assert!(brand.logo.is_some());
        assert!(publisher_credit(&brand).is_none());
        assert!(public_bar("https://app.test", None).contains("Published with"));
    }

    fn brand_with_marks(
        light: (u32, u32),
        dark: Option<(u32, u32)>,
        dark_surface: bool,
    ) -> richview_core::CompanyBrand {
        let mark = |(width, height)| richview_core::CompanyLogo {
            bytes: vec![1, 2, 3],
            mime: "image/png",
            width,
            height,
        };
        richview_core::CompanyBrand {
            name: "Acme \"North\" & Co".into(),
            dark_surface,
            logo: Some(mark(light)),
            logo_dark: dark.map(mark),
            ..Default::default()
        }
    }

    #[test]
    fn a_mark_wider_than_two_and_a_half_times_its_height_stands_alone_as_a_lockup() {
        let name = "Acme &quot;North&quot; &amp; Co";
        // Not a lockup: the mark keeps the name beside it and is decorative (empty alt).
        for (width, height) in [(100, 100), (1, 1024), (250, 100), (2, 1)] {
            let credit = publisher_credit(&brand_with_marks((width, height), None, false)).unwrap();
            assert!(!credit.contains("--lockup"), "{width}x{height}: {credit}");
            assert!(credit.contains(&format!("<strong>{name}</strong>")));
            assert!(credit.contains("alt=\"\""));
            assert!(credit.contains(&format!("title=\"{name}\"")));
        }
        // A lockup: alone, the name only as its alt text and title, the size kept for the layout.
        for (width, height) in [(251, 100), (872, 144), (1024, 1), (3, 1)] {
            let credit = publisher_credit(&brand_with_marks((width, height), None, false)).unwrap();
            assert!(
                credit.starts_with("<span class=\"rv-publisher rv-publisher--lockup\""),
                "{width}x{height}: {credit}"
            );
            assert!(credit.contains(&format!("alt=\"{name}\"")));
            assert!(credit.contains(&format!("title=\"{name}\"")));
            assert!(credit.contains(&format!("width=\"{width}\" height=\"{height}\"")));
            assert!(!credit.contains("<strong>"));
            assert!(!credit.contains("Acme \"North\""));
        }
    }

    #[test]
    fn the_shown_mark_decides_the_class_on_a_dark_report() {
        // A square light mark and a wide dark one: a dark report shows the dark mark, a lockup.
        let wide_dark = brand_with_marks((100, 100), Some((400, 100)), true);
        let credit = publisher_credit(&wide_dark).unwrap();
        assert!(credit.contains("rv-publisher--lockup"));
        assert!(!credit.contains("rv-credit-plate"));
        // On a light report the same brand shows the square light mark with the name.
        let light = richview_core::CompanyBrand {
            dark_surface: false,
            ..wide_dark.clone()
        };
        let credit = publisher_credit(&light).unwrap();
        assert!(!credit.contains("--lockup"));
        assert!(credit.contains("<strong>"));
        // Without a dark mark, a wide light mark on a dark report is a lockup on its light backing.
        let plated = brand_with_marks((400, 100), None, true);
        let credit = publisher_credit(&plated).unwrap();
        assert!(credit.contains("rv-publisher--lockup"));
        assert!(credit.contains("class=\"rv-credit-plate\""));
        assert!(!credit.contains("<strong>"));
    }

    #[test]
    fn the_mark_fills_the_bar_height_and_its_width_is_capped() {
        // FEA-SF-66: 24px tall and up to 180px wide; 20px and 120px under 640px.
        let css = include_str!("host-credit.css");
        let (screen, phone) = css.split_once("@media (max-width: 639px)").unwrap();
        let phone = phone.split_once("@media screen").unwrap().0;
        for (rules, height, width) in [(screen, "24px", "180px"), (phone, "20px", "120px")] {
            let img = rules
                .lines()
                .find(|line| line.contains(".rv-publisher img {"))
                .unwrap();
            assert!(img.contains(&format!("height: {height};")), "{img}");
            assert!(img.contains(&format!("max-width: {width};")), "{img}");
            assert!(!img.contains("max-height"), "{img}");
        }
        // A lockup is never narrowed by the name's room, on a phone either (it out-ranks both).
        assert!(screen.contains(".rv-publisher.rv-publisher--lockup { max-width: none; }"));
    }

    #[test]
    fn a_square_marks_name_stays_on_one_line_and_ends_in_an_ellipsis_on_a_phone() {
        let css = include_str!("host-credit.css");
        let (screen, phone) = css.split_once("@media (max-width: 639px)").unwrap();
        let phone = phone.split_once("@media screen").unwrap().0;
        let name = screen
            .lines()
            .find(|line| line.contains(".rv-publisher strong {"))
            .unwrap();
        for rule in [
            "min-width: 0;",
            "overflow: hidden;",
            "text-overflow: ellipsis;",
            "white-space: nowrap;",
        ] {
            assert!(name.contains(rule), "{rule}: {name}");
        }
        // The phone rules touch only the mark: the name keeps the bar's own text size, never wraps
        // or clamps to two lines, and no cap on the credit crowds out the access message or Sign in.
        assert!(!phone.contains("strong"), "{phone}");
        assert!(!phone.contains("line-clamp"), "{phone}");
        assert!(!phone.contains("white-space"), "{phone}");
        assert!(!phone.contains("font-size"), "{phone}");
        assert!(!phone.contains(".rv-publisher {"), "{phone}");
    }

    #[test]
    fn the_bar_goes_first_in_the_body_and_its_style_last_in_the_head() {
        let page = "<!doctype html><html><head><style>a{}</style></head><body><article>x</article></body></html>";
        let out = insert(page, "<div class=\"rv-host-bar\">B</div>", false);
        assert!(out.contains("<style>a{}</style><style>"));
        assert!(
            out.contains("</head><body><div class=\"rv-host-bar\">B</div><article>x</article>")
        );
        assert!(out.ends_with("</body></html>"));
    }

    #[test]
    fn a_page_without_a_head_and_body_is_served_as_it_is() {
        assert_eq!(insert("<p>plain</p>", "B", false), "<p>plain</p>");
        assert_eq!(
            insert("<body></body></head>", "B", false),
            "<body></body></head>"
        );
    }

    #[test]
    fn typed_names_cannot_add_markup_to_the_bar() {
        let out = render(None, &face("<b>Acme", true), &escape("<b>Acme</b>"), "");
        assert!(out.contains("&lt;b&gt;Acme&lt;/b&gt;"));
        assert!(
            out.contains("<span class=\"rv-face rv-face--org\" aria-hidden=\"true\">&lt;</span>")
        );
        assert!(!out.contains("<b>"));
    }

    #[test]
    fn a_person_shows_as_initials_and_no_name_shows_no_face() {
        assert_eq!(
            face("Tomáš Beneš", false),
            "<span class=\"rv-face\" aria-hidden=\"true\">TB</span>"
        );
        assert_eq!(face("  ", true), "");
        assert!(!render(None, "", "Shared", "").contains("rv-sep"));
    }
}
