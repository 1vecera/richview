//! The site's fonts are the report font builds (STY-28): the faces the pages use are cut from the
//! catalog core embeds in every report, under the report names, and one licence page lists their
//! notices. No site page carries a second copy of a family under another name.
use base64::Engine;
use std::fs;
use std::path::Path;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// The faces the site pages load: family as the reports name it, weight, and the file stem.
const FACES: [(&str, u16, &str); 10] = [
    ("Newsreader", 400, "newsreader-400"),
    ("Newsreader", 600, "newsreader-600"),
    ("Geist", 400, "geist-400"),
    ("Geist", 600, "geist-600"),
    ("RichView Mono", 400, "richview-mono-400"),
    ("RichView Mono", 600, "richview-mono-600"),
    ("RichView Serif", 400, "richview-serif-400"),
    ("Space Grotesk", 600, "space-grotesk-600"),
    ("Big Shoulders", 800, "big-shoulders-800"),
    ("Patrick Hand", 400, "patrick-hand-400"),
];

/// The families on the licence page: report name, upstream name, notice file, what the site sets
/// in it, and why the name differs.
const NOTICES: [(&str, &str, &str, &str, &str); 10] = [
    (
        "Newsreader",
        "Newsreader",
        "newsreader.txt",
        "Headlines",
        "",
    ),
    ("Geist", "Geist", "geist.txt", "Interface and body text", ""),
    (
        "RichView Mono",
        "IBM Plex Mono",
        "ibm-plex-mono.txt",
        "Code",
        "IBM reserves the name “Plex”, so the subset is named RichView Mono.",
    ),
    (
        "RichView Serif",
        "Source Serif 4",
        "source-serif-4.txt",
        "The Scientific style's button",
        "Adobe reserves the name “Source”, so the subset is named RichView Serif.",
    ),
    (
        "Space Grotesk",
        "Space Grotesk",
        "space-grotesk.txt",
        "The Sleek style's button",
        "",
    ),
    (
        "Big Shoulders",
        "Big Shoulders",
        "big-shoulders.txt",
        "The Brutalist style's button",
        "",
    ),
    (
        "Patrick Hand",
        "Patrick Hand",
        "patrick-hand.txt",
        "The Hand-drawn style's button",
        "",
    ),
    (
        "Caveat",
        "Caveat",
        "caveat.txt",
        "The home page's prompt boxes",
        "The home page alone sets text in it; no report embeds it.",
    ),
    (
        "Caveat Brush",
        "Caveat Brush",
        "caveat-brush.txt",
        "The home page's painted hero lettering",
        "The home page alone sets text in it; no report embeds it.",
    ),
    (
        "Bricolage Grotesque",
        "Bricolage Grotesque",
        "bricolage-grotesque.txt",
        "Headlines on the home page, the playground and the documentation",
        "Two static cuts of the variable font, 400 and 600; the site's own pages set text in it, and no report embeds it.",
    ),
];

/// The site's own faces: files the catalog does not hold, kept in this folder and staged beside the
/// catalog's faces under their own names.
const OWN_FACES: &str = "assets/site-fonts";

/// The base64 payload of the catalog's `@font-face` for one family, style and weight.
fn payload(catalog: &str, family: &str, weight: u16) -> Option<Vec<u8>> {
    let mut rest = catalog;
    while let Some(start) = rest.find("@font-face") {
        let rule = &rest[start..];
        let end = rule.find('}').unwrap_or(rule.len());
        let (rule, next) = rule.split_at(end);
        rest = next;
        let declared = |key: &str| -> Option<&str> {
            let at = rule.find(&format!("{key}:"))? + key.len() + 1;
            Some(rule[at..].split(';').next()?.trim().trim_matches('"'))
        };
        // A face with a unicode-range only lends the family its script digits or signs.
        if declared("font-family") == Some(family)
            && declared("font-style") == Some("normal")
            && declared("font-weight") == Some(&weight.to_string())
            && declared("unicode-range").is_none()
        {
            let data = rule.split("base64,").nth(1)?.split(')').next()?;
            return base64::engine::general_purpose::STANDARD.decode(data).ok();
        }
    }
    None
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Writes `fonts/<face>.woff2` for every site face and stages the licence page.
pub fn stage(root: &Path, public: &Path) -> Result<()> {
    let catalog = richview_core::bundled_font_catalog();
    let folder = public.join("fonts");
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder)?;
    for (family, weight, stem) in FACES {
        let bytes = payload(&catalog, family, weight)
            .ok_or_else(|| format!("the font catalog has no {family} {weight}"))?;
        fs::write(folder.join(format!("{stem}.woff2")), bytes)?;
    }
    for entry in fs::read_dir(root.join(OWN_FACES))? {
        let entry = entry?;
        fs::copy(entry.path(), folder.join(entry.file_name()))?;
    }
    let notices = root.join("crates/richview-core/assets/font-licenses");
    let mut sections = String::new();
    for (family, upstream, file, role, reason) in NOTICES {
        let notice = fs::read_to_string(notices.join(file))?;
        let note = if reason.is_empty() {
            format!("{role}. {upstream}, subset to the characters the site and reports use.")
        } else {
            format!(
                "{role}. {upstream}, subset to the characters the site and reports use. {reason}"
            )
        };
        sections.push_str(&format!(
            "<section>\n<h2>{family}</h2>\n<p>{}</p>\n<pre tabindex=\"0\" role=\"region\" aria-label=\"{family} licence notice\">{}</pre>\n</section>\n",
            escape(&note),
            escape(notice.trim())
        ));
    }
    let page = fs::read_to_string(root.join("assets/licenses.html"))?
        .replace("<!-- rv-font-licenses -->", &sections);
    fs::write(public.join("licenses.html"), crate::chrome::apply(&page))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_site_face_is_in_the_catalog_and_is_a_woff2() {
        let catalog = richview_core::bundled_font_catalog();
        for (family, weight, _) in FACES {
            let bytes = payload(&catalog, family, weight)
                .unwrap_or_else(|| panic!("{family} {weight} is not in the catalog"));
            assert_eq!(&bytes[..4], b"wOF2", "{family} {weight}");
        }
    }

    #[test]
    fn every_own_face_is_a_small_woff2_with_a_notice_on_the_licence_page() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut found = 0;
        for entry in fs::read_dir(root.join(OWN_FACES)).unwrap() {
            let path = entry.unwrap().path();
            let bytes = fs::read(&path).unwrap();
            assert_eq!(&bytes[..4], b"wOF2", "{path:?}");
            assert!(bytes.len() < 40 * 1024, "{path:?} is {} bytes", bytes.len());
            // A file is named for its family, words joined by hyphens, then its weight:
            // `bricolage-grotesque-600.woff2` is Bricolage Grotesque.
            let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
            assert!(
                NOTICES.iter().any(|(name, ..)| stem
                    .strip_prefix(&name.to_lowercase().replace(' ', "-"))
                    .is_some_and(|weight| weight.starts_with('-'))),
                "{stem} has no licence notice"
            );
            found += 1;
        }
        assert!(found > 0);
        for (_, _, file, ..) in NOTICES {
            assert!(
                root.join("crates/richview-core/assets/font-licenses")
                    .join(file)
                    .is_file(),
                "{file}"
            );
        }
    }

    #[test]
    fn no_site_family_carries_a_reserved_name() {
        for (family, ..) in FACES {
            assert!(
                !family.contains("Plex") && !family.starts_with("Source"),
                "{family}"
            );
        }
    }
}
