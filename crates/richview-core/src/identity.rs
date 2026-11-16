//! Report identity: the slug and version grammars and the one slug derivation (TYP-60, TYP-62).
//!
//! Pure functions only (ARC-02). Hosts apply them: the publishing client, the server and the
//! store all call these, so a slug or version means the same thing everywhere.
use unicode_normalization::UnicodeNormalization;

/// Longest slug, in characters.
pub(crate) const MAX_SLUG_LEN: usize = 80;
/// Longest version, in bytes.
pub(crate) const MAX_VERSION_BYTES: usize = 64;

/// The slug grammar in words, for diagnostics.
pub const SLUG_GRAMMAR: &str =
    "1-80 characters of a-z, 0-9 and hyphen, with no leading, trailing or double hyphen";
/// The version grammar in words, for diagnostics.
pub(crate) const VERSION_GRAMMAR: &str =
    "MAJOR.MINOR.PATCH with an optional -prerelease, no leading zeros, no +build, at most 64 bytes";

/// Why `slug` breaks the slug grammar, as a sentence, or `None` when it is valid.
pub fn slug_problem(slug: &str) -> Option<String> {
    let fix = "Use lowercase letters, digits and single hyphens, for example `revenue-q3`";
    if slug.is_empty() {
        return Some(format!("The slug is empty. {fix}."));
    }
    if slug.len() > MAX_SLUG_LEN {
        return Some(format!(
            "The slug is {} characters; the limit is {MAX_SLUG_LEN}. Shorten it.",
            slug.len()
        ));
    }
    if !slug
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Some(format!(
            "The slug `{slug}` has characters outside a-z, 0-9 and hyphen (no uppercase, underscores, spaces or accents). {fix}."
        ));
    }
    if slug.starts_with('-') || slug.ends_with('-') || slug.contains("--") {
        return Some(format!(
            "The slug `{slug}` has a leading, trailing or double hyphen. {fix}."
        ));
    }
    None
}

/// Derive a slug from free text: NFKD, drop combining marks, transliterate the remaining Latin
/// letters, lowercase, turn each run of other characters into one hyphen, trim, and cut at the
/// last hyphen before 80 characters. `None` when nothing is left, for example for a title in a
/// script that has no transliteration.
pub(crate) fn derive_slug(text: &str) -> Option<String> {
    let mut out = String::new();
    let mut pending_hyphen = false;
    for c in text
        .nfkd()
        .filter(|c| !is_combining(*c))
        .flat_map(char::to_lowercase)
    {
        let mapped: &str = match c {
            'ß' => "ss",
            'ł' => "l",
            'đ' | 'ð' => "d",
            'ø' => "o",
            'æ' => "ae",
            'œ' => "oe",
            'þ' => "th",
            'ħ' => "h",
            'ı' => "i",
            _ => "",
        };
        if !mapped.is_empty() || c.is_ascii_lowercase() || c.is_ascii_digit() {
            if pending_hyphen && !out.is_empty() {
                out.push('-');
            }
            pending_hyphen = false;
            if mapped.is_empty() {
                out.push(c);
            } else {
                out.push_str(mapped);
            }
        } else {
            pending_hyphen = true;
        }
    }
    if out.len() > MAX_SLUG_LEN {
        let cut = if out.as_bytes()[MAX_SLUG_LEN] == b'-' {
            MAX_SLUG_LEN
        } else {
            out[..MAX_SLUG_LEN].rfind('-').unwrap_or(MAX_SLUG_LEN)
        };
        out.truncate(cut);
    }
    let out = out.trim_matches('-').to_owned();
    (!out.is_empty()).then_some(out)
}

fn is_combining(c: char) -> bool {
    matches!(c as u32, 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F)
}

/// Why `version` breaks the version grammar (SemVer 2.0.0 without build metadata), as a
/// sentence, or `None` when it is valid.
pub fn version_problem(version: &str) -> Option<String> {
    let fix = "Write it as MAJOR.MINOR.PATCH, for example `0.0.1` or `1.2.0-beta.1`";
    if version != version.trim() {
        return Some(format!("The version has surrounding whitespace. {fix}."));
    }
    if version.len() > MAX_VERSION_BYTES {
        return Some(format!(
            "The version is {} bytes; the limit is {MAX_VERSION_BYTES}. Shorten it.",
            version.len()
        ));
    }
    if version.contains('+') {
        return Some(
            "The version has build metadata (`+...`), which is not allowed because two builds would be separate versions of equal precedence. Remove the `+` part or move it into the pre-release."
                .to_owned(),
        );
    }
    let (core, pre) = match version.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (version, None),
    };
    let numbers: Vec<&str> = core.split('.').collect();
    let numeric = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if numbers.len() != 3 || !numbers.iter().all(|n| numeric(n)) {
        return Some(format!("`{version}` is not MAJOR.MINOR.PATCH. {fix}."));
    }
    if numbers.iter().any(|n| n.len() > 1 && n.starts_with('0')) {
        return Some(format!(
            "`{version}` has a number with a leading zero. {fix}, without leading zeros."
        ));
    }
    if let Some(pre) = pre {
        for id in pre.split('.') {
            if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
                return Some(format!(
                    "The pre-release `{pre}` of `{version}` must be dot-separated identifiers of letters, digits and hyphens. {fix}."
                ));
            }
            if numeric(id) && id.len() > 1 && id.starts_with('0') {
                return Some(format!(
                    "The pre-release `{pre}` has a numeric identifier with a leading zero. {fix}."
                ));
            }
        }
    }
    None
}

/// Whether `version` is valid.
pub(crate) fn is_valid_version(version: &str) -> bool {
    version_problem(version).is_none()
}

#[cfg(test)]
mod tests {
    use super::{derive_slug, slug_problem};

    #[test]
    fn derivation_transliterates_latin_and_cuts_at_a_hyphen() {
        for (title, slug) in [
            (
                "Průzkum žluťoučkého koně v Česku 2026",
                "pruzkum-zlutouckeho-kone-v-cesku-2026",
            ),
            ("İstanbul grows", "istanbul-grows"),
            ("Straße & Łódź: đøæœ", "strasse-lodz-doaeoe"),
            ("  --Revenue rose, again!--  ", "revenue-rose-again"),
            ("Q3___2026", "q3-2026"),
            ("Café Ångström", "cafe-angstrom"),
        ] {
            assert_eq!(derive_slug(title).as_deref(), Some(slug), "{title}");
            assert!(slug_problem(slug).is_none());
        }
        assert_eq!(derive_slug("東京"), None);
        assert_eq!(derive_slug("---"), None);
        let words = "word ".repeat(30);
        let slug = derive_slug(&words).unwrap();
        assert!(slug.len() <= 80 && slug.ends_with("word") && slug_problem(&slug).is_none());
        assert_eq!(slug.len(), 79, "cut at the last hyphen before 80: {slug}");
        assert_eq!(derive_slug(&"a".repeat(120)).unwrap(), "a".repeat(80));
    }
}
