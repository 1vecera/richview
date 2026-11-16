//! Playground share links (FEA-SF-35): the source and the chosen style are compressed into the
//! URL fragment, which browsers never send to a server. Nothing is uploaded.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};

/// The trusted runtime path, resolved against the parent host for each sandboxed preview.
pub const PREVIEW_RUNTIME: &str = "/richview_runtime";

/// The longest link the playground offers; a larger draft points to Download `.rmd`.
pub const MAX_LINK: usize = 8_000;
/// The fragment key a share link uses: `#draft=<payload>`.
pub const KEY: &str = "draft";

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Draft {
    s: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    t: String,
}

/// The fragment (without `#`) that restores `source` and `style`, or `None` when the link
/// would be longer than [`MAX_LINK`] characters once the page address is added.
pub fn encode(source: &str, style: &str, page: &str) -> Option<String> {
    let draft = Draft {
        s: source.to_owned(),
        t: style.to_owned(),
    };
    let json = serde_json::to_vec(&draft).ok()?;
    let packed = miniz_oxide::deflate::compress_to_vec(&json, 9);
    let fragment = format!("{KEY}={}", URL_SAFE_NO_PAD.encode(packed));
    (page.len() + 1 + fragment.len() <= MAX_LINK).then_some(fragment)
}

/// The source and style a fragment (with or without `#`) carries.
pub fn decode(fragment: &str) -> Option<(String, String)> {
    let payload = fragment
        .trim_start_matches('#')
        .split('&')
        .find_map(|pair| pair.strip_prefix(&format!("{KEY}=")))?;
    let packed = URL_SAFE_NO_PAD.decode(payload).ok()?;
    let json = miniz_oxide::inflate::decompress_to_vec_with_limit(&packed, 4 * 1024 * 1024).ok()?;
    let draft: Draft = serde_json::from_slice(&json).ok()?;
    Some((draft.s, draft.t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_draft_round_trips_through_the_fragment() {
        let source = "---\ntitle: Ünïcode ✓\n---\n\nbody\n".repeat(20);
        let fragment = encode(&source, "scientific", "https://richview.ai/playground").unwrap();
        assert_eq!(decode(&fragment), Some((source, "scientific".to_owned())));
        assert_eq!(decode(&format!("#{fragment}")).unwrap().1, "scientific");
    }

    #[test]
    fn no_style_chosen_stays_empty() {
        let fragment = encode("x", "", "/p").unwrap();
        assert_eq!(decode(&fragment), Some(("x".to_owned(), String::new())));
    }

    #[test]
    fn a_draft_over_the_cap_gets_no_link() {
        // Text from a xorshift generator does not compress below the cap.
        let mut state = 0x9E37_79B9_7F4A_7C15_u64;
        let noise: String = (0..20_000)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                char::from(b' ' + (state % 95) as u8)
            })
            .collect();
        assert_eq!(encode(&noise, "", "https://richview.ai/playground"), None);
    }

    #[test]
    fn foreign_or_damaged_fragments_decode_to_nothing() {
        assert_eq!(decode("section-2"), None);
        assert_eq!(decode("draft=@@@"), None);
        assert_eq!(decode("draft=AAAA"), None);
    }
}

#[cfg(test)]
mod preview_policy {
    use super::PREVIEW_RUNTIME;
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use richview_core::document::{RenderOptions, ReportFolder, render};
    use sha2::{Digest, Sha256};

    #[test]
    fn the_pages_policy_names_the_same_preview_initializer_on_every_host() {
        let mut initializer = None;
        for (base, origin) in [
            (PREVIEW_RUNTIME, "'self'"),
            (
                "https://richview.test/richview_runtime",
                "https://richview.test",
            ),
            (
                "http://127.0.0.1:4173/richview_runtime",
                "http://127.0.0.1:4173",
            ),
        ] {
            let options = RenderOptions {
                runtime_url: Some(base.into()),
                runtime_js: Some("fixture".into()),
                report_folder: Some(ReportFolder {
                    root: "preview".into(),
                    source: "report.rmd".into(),
                    files: vec!["report.rmd".into()],
                }),
                ..Default::default()
            };
            let report = render("---\ntitle: T\n---\n\nHello\n", &options).unwrap();
            let policy = report
                .html
                .split("Content-Security-Policy\" content=\"")
                .nth(1)
                .and_then(|rest| rest.split('"').next())
                .unwrap()
                .replace("&#39;", "'");
            assert!(
                policy.contains(&format!("connect-src {origin};")),
                "{base}: {policy}"
            );
            let script_sources = policy
                .split(';')
                .find(|p| p.trim_start().starts_with("script-src "))
                .unwrap();
            assert!(
                script_sources
                    .split_whitespace()
                    .any(|token| token == origin),
                "{base}: {policy}"
            );
            assert!(
                !script_sources
                    .split_whitespace()
                    .any(|token| matches!(token, "'unsafe-inline'" | "'unsafe-eval'")),
                "{policy}"
            );
            let script = report
                .html
                .split("<script>")
                .nth(1)
                .and_then(|rest| rest.split("</script>").next())
                .unwrap()
                .to_owned();
            let hash = format!(
                "'sha256-{}'",
                STANDARD.encode(Sha256::digest(script.as_bytes()))
            );
            assert!(
                script_sources.split_whitespace().any(|token| token == hash),
                "{base}: {policy}"
            );
            if let Some(first) = &initializer {
                assert_eq!(
                    &script, first,
                    "one parent policy must allow the initializer on every host"
                );
            } else {
                initializer = Some(script);
            }
        }
    }
}
