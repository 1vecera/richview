//! Apply publication settings to documentation without changing the repository's sources.
use super::{BuildFlags, Result};
use std::{fs, path::Path};

pub(super) fn source(text: &str, flags: BuildFlags) -> Result<String> {
    let mut text = crate::distribution::windows_text(text, flags.windows_preview)?;
    let start = "<!-- repository-public:start -->";
    let end = "<!-- repository-public:end -->";
    while let Some(from) = text.find(start) {
        let to = from
            + text[from..]
                .find(end)
                .ok_or("unclosed repository-public section")?;
        let body = if flags.repository_public {
            text[from + start.len()..to].to_owned()
        } else {
            String::new()
        };
        text.replace_range(from..to + end.len(), &body);
    }
    text = text
        .split("\n\n")
        .filter(|paragraph| !paragraph.starts_with("The reference tables in this spec "))
        .collect::<Vec<_>>()
        .join("\n\n");
    text = strip_ids(&text);
    if !flags.repository_public {
        text = unlink_github(&text);
        check_private(&text)?;
    }
    Ok(text)
}

fn strip_ids(text: &str) -> String {
    let mut text = text.to_owned();
    let mut offset = 0;
    while let Some(open) = text[offset..].find('(').map(|n| n + offset) {
        let Some(close) = text[open..].find(')').map(|n| n + open) else {
            break;
        };
        let words: Vec<_> = text[open + 1..close]
            .split([',', ' '])
            .filter(|word| !word.is_empty())
            .collect();
        let card = |word: &&str| {
            let Some((prefix, number)) = word.rsplit_once('-') else {
                return false;
            };
            !number.is_empty()
                && number.bytes().all(|byte| byte.is_ascii_digit())
                && ["INT", "ARC", "DEL", "TYP", "STY", "PRF", "FEA"]
                    .iter()
                    .any(|name| prefix.starts_with(name))
        };
        if !words.is_empty() && words.iter().all(card) {
            let from = if open > 0 && text.as_bytes()[open - 1] == b' ' {
                open - 1
            } else {
                open
            };
            text.replace_range(from..close + 1, "");
            offset = from;
        } else {
            offset = open + 1;
        }
    }
    text
}

fn unlink_github(text: &str) -> String {
    let mut text = text.to_owned();
    let mut offset = 0;
    while let Some(open) = text[offset..].find("](").map(|n| n + offset) {
        let Some(close) = text[open + 2..].find(')').map(|n| n + open + 2) else {
            break;
        };
        if text[open + 2..close]
            .to_ascii_lowercase()
            .contains("github.com")
            && let Some(label) = text[..open].rfind('[')
        {
            let value = text[label + 1..open].to_owned();
            text.replace_range(label..close + 1, &value);
            offset = label + value.len();
            continue;
        }
        offset = close + 1;
    }
    text
}

fn private_html(text: &str) -> String {
    unlink_html(text, false)
}

fn unlink_html(text: &str, private_only: bool) -> String {
    let mut text = text.to_owned();
    let mut offset = 0;
    while let Some(open) = text[offset..].find("<a ").map(|n| n + offset) {
        let Some(tag_end) = text[open..].find('>').map(|n| n + open) else {
            break;
        };
        let tag = text[open..tag_end].to_ascii_lowercase();
        let remove = if private_only {
            tag.contains("github.com/1vecera/richview")
                || tag.contains("github.com/richview-universe/richview-v3")
        } else {
            tag.contains("github.com")
        };
        if remove {
            let Some(close) = text[tag_end..].find("</a>").map(|n| n + tag_end) else {
                break;
            };
            let body = &text[tag_end + 1..close];
            let value = if body.contains("<svg") {
                String::new()
            } else {
                body.to_owned()
            };
            text.replace_range(open..close + 4, &value);
            offset = open + value.len();
        } else {
            offset = tag_end + 1;
        }
    }
    text
}

pub(super) fn hide_repository_links(root: &Path) -> Result<()> {
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() {
            hide_repository_links(&path)?;
        } else if path.extension().is_some_and(|ext| ext == "html") {
            let original = fs::read_to_string(&path)?;
            let cleaned = unlink_html(&original, true);
            if cleaned != original {
                fs::write(path, cleaned)?;
            }
        }
    }
    Ok(())
}

fn check_private(text: &str) -> Result<()> {
    let lower = text.to_ascii_lowercase();
    if ["//github.com", "//www.github.com", "//gist.github.com"]
        .iter()
        .any(|host| lower.contains(host))
        || text.contains("git clone ")
    {
        return Err("Private-repository docs contain a GitHub URL or source-build command; put source installation inside a repository-public section".into());
    }
    Ok(())
}

fn check_html(text: &str) -> Result<()> {
    // Font notices are inert JSON, not hyperlinks. Keep the original licence text intact,
    // including a font project's address; publication controls must never rewrite licences.
    let marker = "<script id=\"richview-font-notices\" type=\"application/json\">";
    let checked = if let Some(start) = text.find(marker) {
        let end = start
            + text[start..]
                .find("</script>")
                .ok_or("unclosed font notices")?
            + "</script>".len();
        format!("{}{}", &text[..start], &text[end..])
    } else {
        text.to_owned()
    };
    check_private(&checked)
}

pub(super) fn finish(docs: &Path, flags: BuildFlags, revision: &str) -> Result<()> {
    for entry in fs::read_dir(docs)? {
        let path = entry?.path();
        if path.is_dir() {
            finish(&path, flags, revision)?;
            continue;
        }
        let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        if !["html", "md", "rmd", "rslide"].contains(&extension) {
            continue;
        }
        let raw = fs::read_to_string(&path)?;
        let mut text = if extension == "html" {
            strip_ids(&raw)
        } else {
            source(&raw, flags)?
        };
        if flags.repository_public {
            for kind in ["tree", "blob"] {
                text = text.replace(
                    &format!("github.com/1vecera/richview/{kind}/dev/"),
                    &format!("github.com/1vecera/richview/{kind}/{revision}/"),
                );
            }
        }
        if !flags.repository_public {
            text = if extension == "html" {
                private_html(&text)
            } else {
                unlink_github(&text)
            };
            if extension == "html" {
                check_html(&text)
            } else {
                check_private(&text)
            }
            .map_err(|error| format!("{}: {error}", path.display()))?;
        }
        fs::write(path, text)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_page_and_guide_name_windows_from_the_same_switch() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let page = fs::read_to_string(root.join("assets/install/index.html")).unwrap();
        let guide = fs::read_to_string(root.join("docs/public/install.md")).unwrap();
        let command = "releases/latest/download/install.ps1";
        for preview in [false, true] {
            let page = crate::distribution::windows_text(&page, preview).unwrap();
            let flags = BuildFlags {
                windows_preview: preview,
                ..Default::default()
            };
            let guide = source(&guide, flags).unwrap();
            for text in [&page, &guide] {
                assert_eq!(
                    text.contains("Windows and Intel Mac are not supported yet"),
                    !preview
                );
                assert_eq!(text.contains("Windows 10 and 11 on x64"), preview);
                assert!(!text.contains("windows-preview") && !text.contains("windows-unsupported"));
            }
            // No page shows a PowerShell command before the release that it downloads exists.
            assert_eq!(page.contains(command), preview);
            assert!(!guide.contains(command));
        }
    }

    #[test]
    fn windows_spans_must_open_and_close() {
        let span = "a<!-- windows-preview:start --> b<!-- windows-preview:end -->.";
        assert_eq!(
            crate::distribution::windows_text(span, true).unwrap(),
            "a b."
        );
        assert_eq!(
            crate::distribution::windows_text(span, false).unwrap(),
            "a."
        );
        for broken in [
            "a<!-- windows-unsupported:start --> b",
            "a<!-- windows-preview:end --> b<!-- windows-preview:start -->",
        ] {
            assert!(
                crate::distribution::windows_text(broken, true).is_err(),
                "{broken}"
            );
        }
    }
    #[test]
    fn public_links_use_the_public_branch_not_a_private_commit() {
        let dir = tempfile::tempdir().unwrap();
        let page = dir.path().join("index.md");
        fs::write(
            &page,
            "[source](https://github.com/1vecera/richview/blob/dev/a)",
        )
        .unwrap();
        finish(
            dir.path(),
            BuildFlags {
                repository_public: true,
                ..Default::default()
            },
            "main",
        )
        .unwrap();
        let output = fs::read_to_string(page).unwrap();
        assert!(output.contains("github.com/1vecera/richview/blob/main/a"));
        let install = fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/public/install.md"),
        )
        .unwrap();
        assert!(install.contains("git clone https://github.com/1vecera/richview.git\ncd richview\nnpm ci --ignore-scripts"));
        assert!(!install.contains("git checkout"));
        assert!(!install.contains("{{source-revision}}"));
    }

    #[test]
    fn private_site_hides_repository_links_but_keeps_other_navigation() {
        let dir = tempfile::tempdir().unwrap();
        let page = dir.path().join("index.html");
        fs::write(
            &page,
            "<a href=\"https://github.com/1vecera/richview\">Source</a><a href=\"https://github.com/other/project\">Credit</a><a href=\"/docs\">Docs</a>",
        )
        .unwrap();
        hide_repository_links(dir.path()).unwrap();
        let result = fs::read_to_string(page).unwrap();
        assert!(!result.contains("github.com/1vecera/richview"));
        assert!(result.contains("https://github.com/other/project"));
        assert!(result.contains("<a href=\"/docs\">Docs</a>"));
    }
    #[test]
    fn private_sources_hide_only_declared_source_sections_and_links() {
        let text = "Try the playground.\n<!-- repository-public:start -->\ngit clone https://github.com/1vecera/richview.git\n<!-- repository-public:end -->\n[Source](https://github.com/1vecera/richview) (TYP-36, FEA-CH-37)";
        let private = source(text, BuildFlags::default()).unwrap();
        assert!(private.contains("Try the playground."));
        assert!(!private.contains("git clone"));
        assert!(!private.contains("github.com"));
        assert!(!private.contains("TYP-36"));
        assert!(
            source(
                "git clone https://github.com/example/repo",
                BuildFlags::default()
            )
            .is_err()
        );
        assert!(
            source(
                text,
                BuildFlags {
                    repository_public: true,
                    ..Default::default()
                }
            )
            .unwrap()
            .contains("git clone")
        );
    }
}
