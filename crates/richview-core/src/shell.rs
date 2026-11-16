//! The report's HTML document: head metadata, Content Security Policy, header, byline and
//! colophon (FEA-RP-02..06, 50, 51).

use crate::{Diagnostic, escape};
use base64::Engine as _;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// The format marker rendered HTML carries (TYP-05).
pub(crate) const FORMAT: &str = "rmd/1";

/// The commit this build came from, when the build tool names it; reproducible output depends on
/// the build, not on the checkout it runs in.
const BUILD: &str = match option_env!("RICHVIEW_BUILD_COMMIT") {
    Some(commit) => commit,
    None => "",
};

/// A BCP 47 tag as an attribute value: letters, digits and hyphens only, else English.
pub(crate) fn language(frontmatter: &Value) -> String {
    let tag = frontmatter
        .get("lang")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    let ok = !tag.is_empty()
        && tag.len() <= 35
        && tag.split('-').all(|part| {
            !part.is_empty() && part.len() <= 8 && part.bytes().all(|b| b.is_ascii_alphanumeric())
        })
        && tag
            .split('-')
            .next()
            .is_some_and(|p| p.bytes().all(|b| b.is_ascii_alphabetic()));
    if ok { tag.to_owned() } else { "en".to_owned() }
}

fn primary(lang: &str) -> String {
    lang.split('-').next().unwrap_or("en").to_ascii_lowercase()
}

/// The date in the language's long form with its ISO value, or the text as written when it is
/// not an ISO date (FEA-RP-50).
pub(crate) fn byline_date(date: &str, lang: &str) -> String {
    let parts: Vec<&str> = date.trim().split('-').collect();
    let number = |s: &str| s.parse::<u32>().ok();
    let valid = match parts.as_slice() {
        [y, m, d] if y.len() == 4 && m.len() == 2 && d.len() == 2 => {
            match (number(y), number(m), number(d)) {
                (Some(y), Some(m), Some(d)) if (1..=12).contains(&m) && (1..=31).contains(&d) => {
                    Some((y, m, d))
                }
                _ => None,
            }
        }
        _ => None,
    };
    let Some((y, m, d)) = valid else {
        return format!("<time>{}</time>", escape(date));
    };
    let month = (m - 1) as usize;
    let long = match primary(lang).as_str() {
        "cs" => {
            const M: [&str; 12] = [
                "ledna",
                "února",
                "března",
                "dubna",
                "května",
                "června",
                "července",
                "srpna",
                "září",
                "října",
                "listopadu",
                "prosince",
            ];
            format!("{d}. {} {y}", M[month])
        }
        "de" => {
            const M: [&str; 12] = [
                "Januar",
                "Februar",
                "März",
                "April",
                "Mai",
                "Juni",
                "Juli",
                "August",
                "September",
                "Oktober",
                "November",
                "Dezember",
            ];
            format!("{d}. {} {y}", M[month])
        }
        "ja" => format!("{y}年{m}月{d}日"),
        _ => {
            const M: [&str; 12] = [
                "January",
                "February",
                "March",
                "April",
                "May",
                "June",
                "July",
                "August",
                "September",
                "October",
                "November",
                "December",
            ];
            if lang.eq_ignore_ascii_case("en-US") {
                format!("{} {d}, {y}", M[month])
            } else {
                format!("{d} {} {y}", M[month])
            }
        }
    };
    format!(
        "<time datetime=\"{}\">{}</time>",
        escape(date.trim()),
        escape(&long)
    )
}

/// SRI digest of a script's exact bytes, also used as a CSP source expression.
pub(crate) fn script_integrity(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    format!(
        "sha256-{}",
        base64::engine::general_purpose::STANDARD.encode(digest)
    )
}

fn script_hash(text: &str) -> String {
    format!("'{}'", script_integrity(text))
}

/// The inline script bodies of `runtime` (the markup `runtime_script` produced), hashed, and
/// the origin of an external one.
fn runtime_sources(runtime: &str) -> Vec<String> {
    let mut sources = vec![];
    let mut rest = runtime;
    while let Some(open) = rest.find("<script") {
        rest = &rest[open..];
        let Some(end_tag) = rest.find('>') else { break };
        let tag = &rest[..end_tag];
        let Some(close) = rest.find("</script>") else {
            break;
        };
        let body = &rest[end_tag + 1..close];
        if let Some(src) = tag.split("src=\"").nth(1).and_then(|s| s.split('"').next()) {
            if let Some(integrity) = tag
                .split("integrity=\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
            {
                sources.push(format!("'{integrity}'"));
            }
            let src = src.replace("&amp;", "&");
            if src.starts_with('/') {
                sources.push("'self'".to_owned());
            } else if let Ok(url) = url::Url::parse(&src) {
                sources.push(url.origin().ascii_serialization());
            }
        } else if !body.is_empty() {
            sources.push(script_hash(body));
        }
        rest = &rest[close + "</script>".len()..];
    }
    sources.sort();
    sources.dedup();
    sources
}

/// The policy of ARC-15 as a `<meta>`: no network, scripts only the embedded runtime by hash,
/// frames only the report's allowed embed hosts.
pub(crate) fn csp(runtime: &str, embed_hosts: &[String], hotlinks: bool) -> String {
    let mut script = vec!["'wasm-unsafe-eval'".to_owned()];
    script.extend(runtime_sources(runtime));
    let mut img = String::from("data: blob:");
    if hotlinks {
        img.push_str(" https://images.unsplash.com");
    }
    let frames = if embed_hosts.is_empty() {
        "'none'".to_owned()
    } else {
        embed_hosts
            .iter()
            .map(|h| format!("https://{h}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    // An external runtime loads its own wasm: it may connect to the origin it came from and to
    // nothing else. An embedded runtime keeps the report offline.
    let connect: Vec<&str> = script
        .iter()
        .map(String::as_str)
        .filter(|source| !source.starts_with("'sha") && *source != "'wasm-unsafe-eval'")
        .collect();
    let connect = if connect.is_empty() {
        String::new()
    } else {
        format!(" connect-src {};", connect.join(" "))
    };
    format!(
        "default-src 'none'; script-src {}; style-src 'unsafe-inline'; img-src {img}; media-src data: blob:; font-src data:; frame-src {frames};{connect} base-uri 'none'; form-action 'none'",
        script.join(" ")
    )
}

/// What the head says about the page beyond title and styles.
pub(crate) struct Head<'a> {
    pub title: &'a str,
    pub description: Option<&'a str>,
    pub scheme: &'a str,
    pub csp: String,
}

pub(crate) fn head(h: &Head) -> String {
    let mut out = format!(
        "<meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta http-equiv=\"Content-Security-Policy\" content=\"{}\"><title>{}</title><meta name=\"color-scheme\" content=\"{}\"><meta name=\"generator\" content=\"richview {FORMAT}\"><link rel=\"icon\" href=\"data:,\">",
        escape(&h.csp),
        escape(h.title),
        escape(h.scheme)
    );
    let build = if BUILD.is_empty() {
        crate::VERSION.to_owned()
    } else {
        format!("{}+{BUILD}", crate::VERSION)
    };
    out.push_str(&format!(
        "<meta name=\"richview-build\" content=\"{}\">",
        escape(&build)
    ));
    if let Some(description) = h.description.filter(|d| !d.trim().is_empty()) {
        out.push_str(&format!(
            "<meta name=\"description\" content=\"{0}\"><meta property=\"og:description\" content=\"{0}\">",
            escape(description)
        ));
    }
    out.push_str(&format!(
        "<meta property=\"og:title\" content=\"{}\"><meta property=\"og:type\" content=\"article\">",
        escape(h.title)
    ));
    out
}

/// The header image of `title_image`: a bare path is decorative and warns that `alt` is missing;
/// the title is never reused as alt text (FEA-RP-03).
pub(crate) fn header_image(
    frontmatter: &Value,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Value> {
    let mut image = crate::parser::title_image(frontmatter)?;
    if image.get("alt").and_then(Value::as_str).is_none() {
        diagnostics.push(
            Diagnostic::new(
                "warning",
                "image.alt-missing",
                "The header image has no alt text, so it is treated as decorative",
                "frontmatter.title_image.alt",
            )
            .fix("Describe the image in `alt`, or write `alt: \"\"` to mark it decorative"),
        );
        image["alt"] = Value::String(String::new());
    }
    Some(image)
}

/// Author and date under the deck.
pub(crate) fn byline(frontmatter: &Value, lang: &str) -> String {
    let mut meta = String::new();
    if let Some(author) = frontmatter.get("author").and_then(Value::as_str) {
        meta.push_str(&format!("<span>{}</span>", escape(author)));
    }
    if let Some(date) = frontmatter.get("date").and_then(Value::as_str) {
        meta.push_str(&byline_date(date, lang));
    }
    if meta.is_empty() {
        String::new()
    } else {
        format!("<p class=\"rv-report__meta\">{meta}</p>")
    }
}

/// The colophon, one line in the footer (FEA-RP-06); a style hides it with `colophon: hide`.
pub(crate) fn colophon(style: &Value) -> &'static str {
    if style.get("colophon").and_then(Value::as_str) == Some("hide") {
        ""
    } else {
        "<p class=\"rv-report__colophon\">Made with <a href=\"https://richview.ai\">RichView</a></p>"
    }
}

#[cfg(test)]
mod tests {
    use super::csp;

    #[test]
    fn an_external_runtime_may_connect_to_its_own_origin_and_an_embedded_one_to_nothing() {
        let external = csp(
            "<script src=\"https://richview.ai/richview_runtime.js\"></script>",
            &[],
            false,
        );
        assert!(
            external.contains("connect-src https://richview.ai;"),
            "{external}"
        );
        let embedded = csp("<script>run()</script>", &[], false);
        assert!(!embedded.contains("connect-src"), "{embedded}");
    }
}
