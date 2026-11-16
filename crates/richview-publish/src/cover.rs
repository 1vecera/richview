//! Cover images: local files, or Unsplash photos hotlinked from `images.unsplash.com`.
//!
//! The builder never fetches or embeds a remote photo; Unsplash's API guidelines require the
//! returned URL (with its `ixid` parameter) to be hotlinked, so the reader's browser loads it.
use crate::{Credit, escape};

type Result<T> = std::result::Result<T, String>;

/// The only remote host a cover may name.
pub const UNSPLASH_HOST: &str = "images.unsplash.com";
/// The credit's link to Unsplash when a remote cover does not name its own.
pub const UNSPLASH_HOME: &str = "https://unsplash.com/?utm_source=richview&utm_medium=referral";
const MAX_URL: usize = 2048;

/// Whether `cover` is a web address (`https://` or `http://`, any case) rather than a file
/// beside the report. Anything else keeps its local-file handling.
pub fn is_remote(cover: &str) -> bool {
    let lower = |n: usize| cover.get(..n).map(str::to_ascii_lowercase);
    lower(8).as_deref() == Some("https://") || lower(7).as_deref() == Some("http://")
}

fn plain(value: &str) -> bool {
    value.len() <= MAX_URL
        && value.bytes().all(|b| {
            b.is_ascii_graphic() && !matches!(b, b'"' | b'\'' | b'<' | b'>' | b'\\' | b'#')
        })
}

/// A remote cover must be an `https://images.unsplash.com/...` URL: no other host, no
/// credentials, no port, no fragment.
pub fn validate_remote(cover: &str) -> Result<()> {
    let shown: String = cover.chars().take(80).collect();
    let rule = "Remote covers must be https://images.unsplash.com/... URLs";
    let lowercase = format!("{rule}; use lowercase https://{UNSPLASH_HOST} ({shown})");
    let Some(rest) = cover.strip_prefix("https://") else {
        return Err(if cover.to_ascii_lowercase().starts_with("https://") {
            lowercase
        } else if cover.to_ascii_lowercase().starts_with("http://") {
            format!("{rule}; http is not allowed ({shown})")
        } else {
            format!("{rule}; got {shown}")
        });
    };
    if !plain(cover) {
        return Err(format!(
            "{rule}, at most {MAX_URL} ASCII characters without spaces, quotes, backslashes or fragments ({shown})"
        ));
    }
    let authority = rest.split(['/', '?']).next().unwrap_or_default();
    if authority.contains('@') {
        return Err(format!("{rule}; credentials are not allowed ({shown})"));
    }
    if authority.contains(':') {
        return Err(format!("{rule}; ports are not allowed ({shown})"));
    }
    if authority != UNSPLASH_HOST && authority.eq_ignore_ascii_case(UNSPLASH_HOST) {
        return Err(lowercase);
    }
    if authority != UNSPLASH_HOST {
        return Err(format!(
            "{rule}; the host must be {UNSPLASH_HOST} ({shown})"
        ));
    }
    let path = rest[authority.len()..]
        .split('?')
        .next()
        .unwrap_or_default();
    if path.len() < 2 {
        return Err(format!("{rule}; the URL needs a photo path ({shown})"));
    }
    Ok(())
}

/// The same URL with its display width set to `width`, keeping `ixid` and every other
/// parameter Unsplash returned.
pub fn sized(url: &str, width: u32) -> String {
    let (base, query) = url.split_once('?').unwrap_or((url, ""));
    let mut parameters: Vec<&str> = query
        .split('&')
        .filter(|p| !p.is_empty() && !p.starts_with("w="))
        .collect();
    let width = format!("w={width}");
    parameters.push(&width);
    format!("{base}?{}", parameters.join("&"))
}

fn web_address(url: &str) -> bool {
    (url.starts_with("https://") || url.starts_with("http://"))
        && plain(url)
        && url
            .split_once("://")
            .is_some_and(|(_, rest)| !rest.is_empty() && !rest.starts_with(['/', '?']))
}

/// An `https://unsplash.com/...` page, such as a photographer's profile or the Unsplash home.
fn unsplash_page(url: &str) -> bool {
    plain(url)
        && url
            .strip_prefix("https://")
            .and_then(|rest| rest.split(['/', '?']).next())
            == Some("unsplash.com")
}

/// The credit must name the photographer; every URL it carries must be a plain http(s) address.
/// A remote (Unsplash) photo must be credited to Unsplash: the source is absent or "Unsplash",
/// and the photographer and source links point to `https://unsplash.com`.
pub fn validate_credit(credit: &Credit, remote: bool) -> Result<()> {
    let within = |s: &str, max: usize| !s.trim().is_empty() && s.chars().count() <= max;
    if !within(&credit.author, 200) {
        return Err("cover_credit needs an author of at most 200 characters".into());
    }
    if credit.source.as_deref().is_some_and(|s| !within(s, 100)) {
        return Err("cover_credit source must be 1-100 characters".into());
    }
    for (key, url) in [
        ("author_url", &credit.author_url),
        ("source_url", &credit.source_url),
    ] {
        if url.as_deref().is_some_and(|u| !web_address(u)) {
            return Err(format!("cover_credit {key} must be an http(s) address"));
        }
    }
    if remote {
        if credit
            .source
            .as_deref()
            .is_some_and(|s| !s.trim().eq_ignore_ascii_case("Unsplash"))
        {
            return Err("A remote Unsplash cover must be credited to Unsplash: leave cover_credit source out or set it to Unsplash".into());
        }
        match credit.author_url.as_deref() {
            None => {
                return Err("A remote Unsplash cover needs cover_credit author_url, the photographer's https://unsplash.com/@name profile".into());
            }
            Some(url) if !unsplash_page(url) => {
                return Err("cover_credit author_url must be an https://unsplash.com/... profile URL (lowercase host)".into());
            }
            Some(_) => {}
        }
        if credit
            .source_url
            .as_deref()
            .is_some_and(|u| !unsplash_page(u))
        {
            return Err(
                "cover_credit source_url must be an https://unsplash.com/... URL (lowercase host)"
                    .into(),
            );
        }
    }
    if !remote && credit.source.is_none() && credit.source_url.is_some() {
        return Err("cover_credit source_url needs a source".into());
    }
    Ok(())
}

fn named(text: &str, url: Option<&str>) -> String {
    match url.filter(|u| web_address(u)) {
        Some(url) => format!(
            "<a href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">{}</a>",
            escape(url),
            escape(text)
        ),
        None => escape(text),
    }
}

/// Author and source as (text, link) pairs. A remote photo is always credited to Unsplash.
fn parts(credit: &Credit, remote: bool) -> (String, Option<String>) {
    let author = named(credit.author.trim(), credit.author_url.as_deref());
    let source = match (credit.source.as_deref().map(str::trim), remote) {
        (_, true) => Some(named(
            "Unsplash",
            Some(credit.source_url.as_deref().unwrap_or(UNSPLASH_HOME)),
        )),
        (Some(source), false) => Some(named(source, credit.source_url.as_deref())),
        (None, false) => None,
    };
    (author, source)
}

/// `<p>` credit line: "Photo by <author> on <source>", names linked.
pub fn line(credit: &Credit, remote: bool) -> String {
    let (author, source) = parts(credit, remote);
    format!(
        "<p class=\"credit\">Photo by {author}{}</p>",
        source.map(|s| format!(" on {s}")).unwrap_or_default()
    )
}

/// The compact credit for a list row: "Photo: <author> / <source>", names linked.
pub fn compact(credit: &Credit, remote: bool) -> String {
    let (author, source) = parts(credit, remote);
    format!(
        "<p class=\"row-credit\">Photo: {author}{}</p>",
        source.map(|s| format!(" / {s}")).unwrap_or_default()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const PHOTO: &str = "https://images.unsplash.com/photo-1559538834-8b012e20a84f?ixid=M3w&w=1600&q=80&fm=jpg&fit=max";

    fn credit() -> Credit {
        Credit {
            author: "Marek Rucinski".into(),
            author_url: Some(
                "https://unsplash.com/@marulla?utm_source=richview&utm_medium=referral".into(),
            ),
            source: Some("Unsplash".into()),
            source_url: Some(UNSPLASH_HOME.into()),
        }
    }

    #[test]
    fn only_https_unsplash_urls_are_remote_covers() {
        assert!(validate_remote(PHOTO).is_ok());
        for (bad, reason) in [
            ("http://images.unsplash.com/photo-1", "http is not allowed"),
            ("https://example.com/photo.jpg", "host must be"),
            ("https://unsplash.com/photo-1", "host must be"),
            ("https://images.unsplash.com.evil.test/p", "host must be"),
            ("https://user@images.unsplash.com/p", "credentials"),
            ("https://images.unsplash.com:8443/p", "ports"),
            ("https://images.unsplash.com", "photo path"),
            ("https://images.unsplash.com/p#frag", "fragments"),
            ("https://images.unsplash.com/p\"onerror=1", "quotes"),
            ("https://images.unsplash.com\\@evil.test/p", "quotes"),
            (
                "HTTPS://images.unsplash.com/p",
                "use lowercase https://images.unsplash.com",
            ),
            (
                "https://Images.Unsplash.com/p",
                "use lowercase https://images.unsplash.com",
            ),
            ("//images.unsplash.com/p", "https"),
            ("data:image/png;base64,AAAA", "https"),
        ] {
            let error = validate_remote(bad).unwrap_err();
            assert!(error.contains(reason), "{bad}: {error}");
        }
        assert!(is_remote(PHOTO) && is_remote("HTTP://x/y") && is_remote("Https://x/y"));
        for local in [
            "cover.svg",
            "art/cover.png",
            "./a.svg",
            "cover:v2.png",
            "//x/y",
            "data:x",
        ] {
            assert!(!is_remote(local), "{local}");
        }
    }

    #[test]
    fn sizing_sets_the_width_and_keeps_ixid() {
        let url = sized(PHOTO, 800);
        assert!(url.contains("ixid=M3w") && url.contains("&w=800"));
        assert_eq!(url.matches("w=").count(), 1, "{url}");
        assert_eq!(
            sized("https://images.unsplash.com/p", 240),
            "https://images.unsplash.com/p?w=240"
        );
    }

    #[test]
    fn credits_link_both_names_and_escape_everything() {
        let html = line(&credit(), true);
        assert_eq!(
            html,
            "<p class=\"credit\">Photo by <a href=\"https://unsplash.com/@marulla?utm_source=richview&amp;utm_medium=referral\" target=\"_blank\" rel=\"noopener noreferrer\">Marek Rucinski</a> on <a href=\"https://unsplash.com/?utm_source=richview&amp;utm_medium=referral\" target=\"_blank\" rel=\"noopener noreferrer\">Unsplash</a></p>"
        );
        assert!(compact(&credit(), true).starts_with("<p class=\"row-credit\">Photo: <a "));
        let hostile = Credit {
            author: "<b>\"x\"".into(),
            author_url: Some("javascript:alert(1)".into()),
            source: None,
            source_url: None,
        };
        let html = line(&hostile, true);
        assert!(html.contains("&lt;b&gt;&quot;x&quot;"), "{html}");
        assert!(!html.contains("javascript"), "{html}");
        assert!(html.contains(">Unsplash</a>"), "{html}");
        assert_eq!(
            line(&hostile, false),
            "<p class=\"credit\">Photo by &lt;b&gt;&quot;x&quot;</p>"
        );
    }

    #[test]
    fn a_remote_photo_is_credited_to_unsplash_with_unsplash_links() {
        let mut c = credit();
        c.source = Some("unsplash".into());
        assert!(validate_credit(&c, true).is_ok());
        for (mutate, expected) in [
            (
                (|c: &mut Credit| c.source = Some("Pexels".into())) as fn(&mut Credit),
                "credited to Unsplash",
            ),
            (|c| c.author_url = None, "needs cover_credit author_url"),
            (
                |c| c.author_url = Some("https://evil.test/@a".into()),
                "author_url must be",
            ),
            (
                |c| c.author_url = Some("http://unsplash.com/@a".into()),
                "author_url must be",
            ),
            (
                |c| c.author_url = Some("https://unsplash.com.evil.test/@a".into()),
                "author_url must be",
            ),
            (
                |c| c.author_url = Some("https://Unsplash.com/@a".into()),
                "author_url must be",
            ),
            (
                |c| c.source_url = Some("https://evil.test".into()),
                "source_url must be",
            ),
        ] {
            let mut c = credit();
            mutate(&mut c);
            let error = validate_credit(&c, true).unwrap_err();
            assert!(error.contains(expected), "{error}");
        }
        // A local cover's credit may name any source and address.
        let local = Credit {
            author: "A".into(),
            author_url: Some("https://example.com/a".into()),
            source: Some("Archive".into()),
            source_url: Some("https://example.com".into()),
        };
        assert!(validate_credit(&local, false).is_ok());
    }

    #[test]
    fn credits_need_an_author_and_http_addresses() {
        assert!(validate_credit(&credit(), true).is_ok());
        let mut c = credit();
        c.author = "  ".into();
        assert!(validate_credit(&c, true).is_err());
        let mut c = credit();
        c.author_url = Some("javascript:alert(1)".into());
        assert!(
            validate_credit(&c, true)
                .unwrap_err()
                .contains("author_url")
        );
        let mut c = credit();
        c.source = None;
        assert!(validate_credit(&c, false).is_err());
        assert!(validate_credit(&c, true).is_ok());
        let mut c = credit();
        c.source_url = Some("javascript:alert(1)".into());
        assert!(
            validate_credit(&c, true)
                .unwrap_err()
                .contains("source_url")
        );
    }
}
