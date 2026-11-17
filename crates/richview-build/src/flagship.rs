//! The flagship page: one sourced report rendered once per style (FEA-SF-36, FEA-SF-63).
//!
//! The engine renders the report folder in each of the five builtin styles and in each extended
//! theme, exactly as `richview generate --style <name>` would, and the page swaps the frame to
//! the chosen render. The source folder is published once beside the page, and the page names
//! the content hash of that folder (TYP-95) instead of a hash of `report.rmd` alone.
use crate::{Result, chrome, copy_public_examples};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

const ROUTE: &str = "showcase/openai-vs-anthropic";

/// The extended themes: studio variants kept outside the normal style menus (STY-05). Each is a
/// real `.rstyle.yaml` under `assets/showcase/themes/`.
fn extended_themes(root: &Path) -> Result<Vec<(String, PathBuf)>> {
    let mut themes = Vec::new();
    let folder = root.join("assets/showcase/themes");
    if folder.is_dir() {
        for entry in fs::read_dir(&folder)? {
            let path = entry?.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if let Some(id) = name.strip_suffix(".rstyle.yaml") {
                themes.push((id.to_owned(), path.clone()));
            }
        }
    }
    themes.sort();
    Ok(themes)
}

/// A hash that identifies the report's content: the source, its data and its ledger, each file
/// named by its path inside the folder, in path order.
pub fn content_hash(folder: &Path) -> Result<String> {
    fn walk(folder: &Path, base: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
        for entry in fs::read_dir(folder)? {
            let path = entry?.path();
            let relative = path.strip_prefix(base)?.to_string_lossy().into_owned();
            if relative.starts_with("Rendered") || relative.starts_with("themes") {
                continue;
            }
            if path.is_dir() {
                walk(&path, base, files)?;
            } else if relative == "report.rmd"
                || relative.starts_with("data/")
                || relative.starts_with("ledger/")
            {
                files.push(path);
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(folder, folder, &mut files)?;
    files.sort();
    let mut hash = Sha256::new();
    for path in files {
        let relative = path.strip_prefix(folder)?.to_string_lossy().into_owned();
        let bytes = fs::read(&path)?;
        hash.update(relative.as_bytes());
        hash.update([0]);
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(&bytes);
    }
    let digest = hash.finalize();
    Ok(format!(
        "sha256-{}",
        digest[..8]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    ))
}

/// The value of a `--rv-*` custom property in a rendered report's stylesheet.
fn css_var(html: &str, name: &str) -> String {
    let marker = format!("{name}:");
    let mut from = 0;
    while let Some(at) = html[from..].find(&marker) {
        let start = from + at;
        // A whole name only: `--rv-bg:` and not `--rv-bg-elev:` or a longer one ending the same way.
        let before = html[..start].chars().next_back().unwrap_or(';');
        if before == ';' || before == '{' || before.is_whitespace() {
            let value = &html[start + marker.len()..];
            let end = value.find([';', '}']).unwrap_or(value.len());
            return value[..end].trim().to_owned();
        }
        from = start + marker.len();
    }
    String::new()
}

/// A style's display name: a builtin by its label, an extended theme by its file name.
fn style_name(id: &str) -> String {
    if chrome::site_styles().contains(&id.to_string().as_str()) {
        return chrome::style_label(id);
    }
    let words = id
        .split_once('-')
        .map_or(id, |(_, rest)| rest)
        .replace('-', " ");
    let mut chars = words.chars();
    chars.next().map_or(String::new(), |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// The page frame's tokens for each style, read from the render itself: the frame follows the
/// active style (STY-60), so the header, rail and footer take the report's own colours and faces.
fn frame_tokens(folder: &Path, ids: &[String]) -> Result<Value> {
    let mut tokens = serde_json::Map::new();
    for id in ids {
        let html = fs::read_to_string(folder.join(format!("Rendered/{id}.html")))?;
        let var = |name: &str| css_var(&html, &format!("--rv-{name}"));
        let dark = html.contains("name=\"color-scheme\" content=\"dark\"");
        tokens.insert(
            id.clone(),
            json!({
                "name": style_name(id),
                "mode": if dark { "dark" } else { "light" },
                "bg": var("bg"),
                "bg-elev": var("bg-elev"),
                "ink": var("ink"),
                "ink-soft": var("ink-soft"),
                "rule": var("rule"),
                "accent": var("accent"),
                "font-text": var("font-text"),
                "font-mono": var("font-mono"),
            }),
        );
    }
    Ok(Value::Object(tokens))
}

pub fn stage(root: &Path, public: &Path) -> Result<()> {
    let showcase = public.join("showcase");
    if showcase.exists() {
        fs::remove_dir_all(&showcase)?;
    }
    let folder = public.join(ROUTE);
    let source_file = root.join("examples/openai-vs-anthropic/report.rmd");
    copy_public_examples(
        source_file.parent().ok_or("Missing flagship folder")?,
        &folder,
    )?;
    fs::create_dir_all(folder.join("Rendered"))?;
    let source = fs::read_to_string(&source_file)?;
    // Render from the staged folder so linked source and data files are copied beside Rendered/.
    let staged = folder.join("report.rmd");

    let mut ids: Vec<String> = Vec::new();
    for name in chrome::site_styles() {
        render_one(public, &source, &staged, name, &[name.to_owned()])?;
        ids.push(name.to_owned());
    }
    let mut extended = Vec::new();
    for (id, file) in extended_themes(root)? {
        let relative = format!("themes/{id}.rstyle.yaml");
        fs::create_dir_all(folder.join("themes"))?;
        fs::copy(&file, folder.join(&relative))?;
        match render_one(public, &source, &staged, &id, &[relative]) {
            Ok(()) => {
                extended.push(id.clone());
                ids.push(id);
            }
            Err(error) => println!("warning: extended theme {id} did not render: {error}"),
        }
    }
    // Only the renders are public; the theme files were inputs.
    let _ = fs::remove_dir_all(folder.join("themes"));

    let tokens = frame_tokens(&folder, &ids)?;
    let styles = chrome::site_styles();
    let options: String = extended
        .iter()
        .map(|id| {
            let name = tokens[id]["name"].as_str().unwrap_or(id);
            format!(
                "<option value=\"{id}\">{}</option>",
                name.replace('&', "&amp;")
            )
        })
        .collect();
    let compare: String = ids
        .iter()
        .map(|id| {
            let name = tokens[id]["name"].as_str().unwrap_or(id);
            format!(
                "<option value=\"{id}\">{}</option>",
                name.replace('&', "&amp;")
            )
        })
        .collect();
    let menu = style_menu(&styles, &extended, &tokens);
    let mut page = fs::read_to_string(root.join("assets/showcase/index.html"))?
        .replace("<!-- rv-extended-options -->", &options)
        .replace("<!-- rv-compare-options -->", &compare)
        .replace(
            "<!-- rv-initial-frame-style -->",
            &frame_style(&tokens[styles[0]]),
        )
        .replace("<!-- rv-content-hash -->", &content_hash(&folder)?)
        .replace(
            "<!-- rv-showcase-tokens -->",
            &serde_json::to_string(&tokens)?.replace("</", "<\\/"),
        );
    page = page.replace("<!-- rv-style-count -->", &style_count(styles.len()));
    let page = chrome::apply_with(&chrome::apply_styles(&page), &menu);
    fs::write(folder.join("index.html"), page)?;
    fs::copy(
        root.join("assets/showcase/showcase.css"),
        showcase.join("showcase.css"),
    )?;
    Ok(())
}

/// A phone's style menu (Daniel, 17 Nov 2026: "top area is all single line with a dropdown to
/// change styles"): one native select in the header's row, the five styles first and the extended
/// themes under their own heading. It is in the page as built, so the header is the same with and
/// without the script; like the row of pills, it changes the style only once the script runs.
fn style_menu(styles: &[&str], extended: &[String], tokens: &Value) -> String {
    let option = |id: &str, selected: bool| {
        format!(
            "<option value=\"{id}\"{}>{}</option>",
            if selected { " selected" } else { "" },
            tokens[id]["name"]
                .as_str()
                .unwrap_or(id)
                .replace('&', "&amp;")
                .replace('<', "&lt;")
        )
    };
    let builtin: String = styles
        .iter()
        .enumerate()
        .map(|(i, id)| option(id, i == 0))
        .collect();
    let more: String = extended.iter().map(|id| option(id, false)).collect();
    let more = if more.is_empty() {
        more
    } else {
        format!("<optgroup label=\"Extended themes\">{more}</optgroup>")
    };
    format!(
        "<label class=\"nav__style\"><span class=\"sr-only\">Style</span><select id=\"style-menu\"><optgroup label=\"Styles\">{builtin}</optgroup>{more}</select></label>"
    )
}

/// The site tokens a style's own tokens set on the page frame (STY-60). The script applies the
/// same mapping when the reader switches style.
fn frame_style(tokens: &Value) -> String {
    let text = |key: &str| tokens[key].as_str().unwrap_or("").replace('<', "");
    format!(
        "body[data-richview-page=\"showcase\"]{{--paper:{};--panel:{};--ink:{};--soft:{};--rule:{};--blue:{};--blue-ink:{};--font-ui:{};--font-mono:{};color-scheme:{}}}",
        text("bg"),
        text("bg-elev"),
        text("ink"),
        text("ink-soft"),
        text("rule"),
        text("accent"),
        text("accent"),
        text("font-text"),
        text("font-mono"),
        text("mode"),
    )
}

fn style_count(count: usize) -> String {
    match count {
        5 => "five".into(),
        n => n.to_string(),
    }
}

/// Render the flagship in one style and export it as `Rendered/<id>.html`.
fn render_one(
    public: &Path,
    source: &str,
    staged: &Path,
    id: &str,
    styles: &[String],
) -> Result<()> {
    let options = crate::resolve_document_options(source, staged, styles, public)?.1;
    let report = richview_core::document::render(source, &options)
        .map_err(|errors| format!("Flagship in {id} failed: {errors:?}"))?;
    richview_app::resolver::export_report(
        staged,
        Some(Path::new(&format!("{id}.html"))),
        &report,
        &options,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_changes_with_data_and_ledger_but_not_with_renders() {
        let dir = tempfile_dir("flagship-hash");
        fs::create_dir_all(dir.join("data")).unwrap();
        fs::create_dir_all(dir.join("ledger")).unwrap();
        fs::create_dir_all(dir.join("Rendered")).unwrap();
        fs::write(dir.join("report.rmd"), "a").unwrap();
        fs::write(dir.join("data/x.csv"), "1").unwrap();
        fs::write(dir.join("ledger/sources.csv"), "s").unwrap();
        let first = content_hash(&dir).unwrap();
        fs::write(dir.join("Rendered/sleek.html"), "render").unwrap();
        assert_eq!(content_hash(&dir).unwrap(), first);
        fs::write(dir.join("data/x.csv"), "2").unwrap();
        assert_ne!(content_hash(&dir).unwrap(), first);
        let second = content_hash(&dir).unwrap();
        fs::write(dir.join("ledger/sources.csv"), "t").unwrap();
        assert_ne!(content_hash(&dir).unwrap(), second);
    }

    fn tempfile_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_frame_takes_the_styles_tokens() {
        let tokens = json!({"bg":"#111","bg-elev":"#222","ink":"#eee","ink-soft":"#aaa","rule":"#333","accent":"#46f","font-text":"Geist, sans-serif","font-mono":"monospace","mode":"dark"});
        let css = frame_style(&tokens);
        assert!(css.contains("--paper:#111;"));
        assert!(css.contains("--blue:#46f;"));
        assert!(css.ends_with("color-scheme:dark}"));
    }

    #[test]
    fn a_custom_property_is_read_whole() {
        let html = ":root{--rv-bg-elev:oklch(0.9 0 0);--rv-bg:oklch(1 0 0);--rv-font-text:Geist, sans-serif}";
        assert_eq!(css_var(html, "--rv-bg"), "oklch(1 0 0)");
        assert_eq!(css_var(html, "--rv-bg-elev"), "oklch(0.9 0 0)");
        assert_eq!(css_var(html, "--rv-font-text"), "Geist, sans-serif");
        assert_eq!(css_var(html, "--rv-missing"), "");
    }

    #[test]
    fn styles_are_named_by_label_or_file() {
        assert_eq!(style_name("hand-drawn"), "Hand-drawn");
        assert_eq!(style_name("v04-original-dark"), "Original dark");
    }

    #[test]
    fn a_phones_menu_lists_the_styles_then_the_extended_themes() {
        let tokens = json!({"sleek":{"name":"Sleek"},"air":{"name":"Air"},"v05-editorial":{"name":"Editorial & co"}});
        let menu = style_menu(&["sleek", "air"], &["v05-editorial".to_owned()], &tokens);
        assert!(menu.starts_with(
            "<label class=\"nav__style\"><span class=\"sr-only\">Style</span><select id=\"style-menu\">"
        ));
        assert!(menu.contains(
            "<optgroup label=\"Styles\"><option value=\"sleek\" selected>Sleek</option><option value=\"air\">Air</option></optgroup>"
        ));
        assert!(menu.contains(
            "<optgroup label=\"Extended themes\"><option value=\"v05-editorial\">Editorial &amp; co</option></optgroup>"
        ));
        // Without extended themes the menu has no empty heading.
        assert!(!style_menu(&["sleek"], &[], &tokens).contains("Extended themes"));
        // The header's own row holds it, between the mark and the links.
        let header = chrome::header_with(&menu);
        let at = |needle: &str| header.find(needle).unwrap();
        assert!(at("nav__brand") < at("nav__style") && at("nav__style") < at("nav__links"));
        assert!(!chrome::header().contains("nav__style"));
    }

    #[test]
    fn the_count_is_spelled_for_five() {
        assert_eq!(style_count(5), "five");
        assert_eq!(style_count(6), "6");
    }
}
