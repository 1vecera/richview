//! The flagship page: one sourced report rendered once per style (FEA-SF-36, FEA-SF-63).
//!
//! The engine renders the report folder in each of the five builtin styles and in each extended
//! theme, exactly as `richview generate --style <name>` would, and the page swaps the frame to
//! the chosen render. The source folder is published once beside the page, and the page names
//! the content hash of that folder (TYP-95) instead of a hash of `report.rmd` alone.
use crate::{Result, chrome, copy_public_examples, site_fonts};
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

/// Package the filtered report folder before the showcase adds its own pages and renders.
fn stage_source_zip(folder: &Path) -> Result<()> {
    let mut files = richview_app::resolver::report_folder(&folder.join("report.rmd"))?.files;
    files.sort_unstable();
    let name = folder
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("Missing report folder name")?;
    let archive = fs::File::create_new(folder.join(format!("{name}.zip")))?;
    let mut zip = zip::ZipWriter::new(archive);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default());
    for file in files {
        zip.start_file(format!("{name}/{file}"), options)?;
        std::io::copy(&mut fs::File::open(folder.join(file))?, &mut zip)?;
    }
    zip.finish()?;
    Ok(())
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
    stage_source_zip(&folder)?;
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
    // Every style the page can show, the extended themes too, takes its own faces into the frame.
    let all: Vec<&str> = ids.iter().map(String::as_str).collect();
    frame_faces_served(&tokens, &all, &richview_core::bundled_font_catalog())?;
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
            &format!(
                "{}{}",
                site_fonts::frame_faces_css(),
                frame_style(&tokens[styles[0]])
            ),
        )
        .replace("<!-- rv-content-hash -->", &content_hash(&folder)?)
        .replace(
            "<!-- rv-showcase-tokens -->",
            &serde_json::to_string(&tokens)?.replace("</", "<\\/"),
        );
    let ledger = fs::read_to_string(folder.join("ledger/sources.csv"))?;
    let listed = fs::read_to_string(folder.join("content/citations.json"))?;
    let (entries, cited) = citations(&source, &ledger, &listed)?;
    page = page
        .replace(
            "<!-- rv-source-count -->",
            &format!("{entries} dated ledger entries"),
        )
        .replace(
            "<!-- rv-showcase-citations -->",
            &serde_json::to_string(&cited)?.replace("</", "<\\/"),
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

/// The ledger's entry count, and the report's source links in reading order, each as its URL, its
/// text ("" for a table's link cell) and the ledger ids its sentence cites, as `build_report.py`
/// wrote them (`content/citations.json`). The page opens each link's own entry from this list and
/// loads the ledger only with the first card. The build refuses a link the report does not show
/// and an id whose ledger row records another URL, so no card can show another entry's evidence.
fn citations(source: &str, ledger: &str, listed: &str) -> Result<(usize, Value)> {
    let mut rows = csv_records(ledger).into_iter();
    let header = rows.next().unwrap_or_default();
    let column = |name: &str| header.iter().position(|cell| cell == name);
    let (Some(id), Some(url)) = (column("id"), column("url")) else {
        return Err("the source ledger has no id or url column".into());
    };
    let mut entries = 0;
    let mut recorded = std::collections::BTreeMap::new();
    for row in rows.filter(|row| row.len() > id.max(url)) {
        entries += 1;
        recorded.insert(row[id].clone(), row[url].clone());
    }
    let listed: Vec<Value> = serde_json::from_str(listed)?;
    let mut links = Vec::new();
    for link in &listed {
        let target = link["url"].as_str().unwrap_or_default();
        let text = link["text"].as_str().unwrap_or_default();
        let ids: Vec<&str> = link["ids"]
            .as_array()
            .map(|ids| ids.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let shown = if text.is_empty() {
            source.contains(target)
        } else {
            source.contains(&format!("[{text}]({target})"))
        };
        if target.is_empty() || ids.is_empty() || !shown {
            return Err(
                format!("the report does not show the citation {text:?} of {target}").into(),
            );
        }
        for cited in &ids {
            if recorded.get(*cited).map(String::as_str) != Some(target) {
                return Err(format!(
                    "{cited} is cited at {target}, which its ledger row does not record"
                )
                .into());
            }
        }
        // The page reads each link as [url, text, ids], in reading order.
        links.push(json!([target, text, ids]));
    }
    Ok((entries, Value::Array(links)))
}

/// Records of a CSV file whose quoted cells may hold commas, doubled quotes and line breaks.
fn csv_records(text: &str) -> Vec<Vec<String>> {
    let (mut records, mut record, mut cell) = (Vec::new(), Vec::new(), String::new());
    let (mut quoted, mut chars) = (false, text.chars().peekable());
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cell.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => record.push(std::mem::take(&mut cell)),
            '\n' | '\r' if !quoted => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                record.push(std::mem::take(&mut cell));
                records.push(std::mem::take(&mut record));
            }
            _ => cell.push(c),
        }
    }
    if !cell.is_empty() || !record.is_empty() {
        record.push(cell);
        records.push(record);
    }
    records
}

/// The first family a font stack names, without its quotes.
fn first_family(stack: &str) -> &str {
    stack
        .split(',')
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches(|c| c == '"' || c == '\'')
}

/// Refuses a style whose frame face the site does not serve: the page would set that
/// style's header, rail and cards in a fallback face (STY-60). A face the report catalog lacks
/// falls back in the report too, so only the catalog's faces are required.
fn frame_faces_served(tokens: &Value, styles: &[&str], catalog: &str) -> Result<()> {
    for id in styles {
        for key in ["font-text", "font-mono"] {
            let family = first_family(tokens[*id][key].as_str().unwrap_or(""));
            for weight in [400, 600] {
                if site_fonts::in_catalog(catalog, family, weight)
                    && !site_fonts::served(family, weight)
                {
                    return Err(format!(
                        "the {id} style's frame sets {family} {weight}, which the site does not serve from /fonts/"
                    )
                    .into());
                }
            }
        }
    }
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
    fn the_source_zip_reproduces_the_public_example_files_byte_for_byte() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/openai-vs-anthropic");
        let build = |out: &Path| {
            let folder = out.join("openai-vs-anthropic");
            copy_public_examples(&root, &folder).unwrap();
            let files = richview_app::resolver::report_folder(&folder.join("report.rmd"))
                .unwrap()
                .files;
            stage_source_zip(&folder).unwrap();
            let archive = folder.join("openai-vs-anthropic.zip");
            let mut zip = zip::ZipArchive::new(fs::File::open(&archive).unwrap()).unwrap();
            assert_eq!(zip.len(), files.len());
            for path in files {
                let mut member = zip.by_name(&format!("openai-vs-anthropic/{path}")).unwrap();
                let mut bytes = Vec::new();
                std::io::copy(&mut member, &mut bytes).unwrap();
                assert_eq!(bytes, fs::read(folder.join(path)).unwrap());
            }
            fs::read(archive).unwrap()
        };
        let out = tempfile::tempdir().unwrap();
        assert_eq!(
            build(&out.path().join("first")),
            build(&out.path().join("second"))
        );
    }

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
    fn the_build_refuses_a_style_whose_frame_face_is_not_served() {
        let catalog = richview_core::bundled_font_catalog();
        let served = json!({
            "sleek": {"font-text": "Geist, Inter, system-ui, sans-serif", "font-mono": "Geist Mono, ui-monospace, monospace"},
            "scientific": {"font-text": "\"RichView Serif\", Georgia, serif", "font-mono": "RichView Code, monospace"},
            "brutalist": {"font-text": "Newsreader, Georgia, serif", "font-mono": "Space Mono, monospace"},
            // A face the catalog lacks falls back in the report as well; the page need not serve it.
            "v02-original-sans": {"font-text": "\"Inter\", system-ui, sans-serif", "font-mono": "\"JetBrains Mono\", monospace"},
        });
        frame_faces_served(
            &served,
            &["sleek", "scientific", "brutalist", "v02-original-sans"],
            &catalog,
        )
        .unwrap();
        let unserved = json!({"sleek": {"font-text": "\"RichView Sans\", sans-serif", "font-mono": "Geist Mono"}});
        let error = frame_faces_served(&unserved, &["sleek"], &catalog)
            .unwrap_err()
            .to_string();
        assert!(error.contains("RichView Sans 400"), "{error}");
        let css = site_fonts::frame_faces_css();
        assert!(
            css.contains("font-family:\"RichView Code\"")
                && css.contains("url(\"/fonts/richview-code-400.woff2\")"),
            "{css}"
        );
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

    #[test]
    fn citations_keep_each_links_own_ids_and_refuse_a_wrong_one() {
        let ledger = "id,kind,title,url\nS004,survey,\"Wave, Feb\",https://a.example/report.pdf\n\
                      S005,survey,\"Wave \"\"May\"\"\nsecond line\",https://a.example/report.pdf\n\
                      D-NPM,dataset,npm,https://b.example/docs\nD-LABELS,dataset,Labels,\n";
        let source = "Feb [SSRS, 26 May 2026](https://a.example/report.pdf); May [SSRS, 26 May 2026](https://a.example/report.pdf); \
                      [npm downloads](https://b.example/docs).\n\n```table\n- link: https://b.example/docs\n```";
        let listed = r#"[
            {"url": "https://a.example/report.pdf", "text": "SSRS, 26 May 2026", "ids": ["S004"]},
            {"url": "https://a.example/report.pdf", "text": "SSRS, 26 May 2026", "ids": ["S005", "S004"]},
            {"url": "https://b.example/docs", "text": "npm downloads", "ids": ["D-NPM"]},
            {"url": "https://b.example/docs", "text": "", "ids": ["D-NPM"]}
        ]"#;
        let (entries, cited) = citations(source, ledger, listed).unwrap();
        assert_eq!(entries, 4);
        assert_eq!(cited[0][2], json!(["S004"]));
        assert_eq!(cited[1][2], json!(["S005", "S004"]));
        assert_eq!(cited[3][1], "");
        // An id whose row records another URL, and a link the report does not show, are refused.
        let wrong = r#"[{"url": "https://a.example/report.pdf", "text": "SSRS, 26 May 2026", "ids": ["D-NPM"]}]"#;
        assert!(
            citations(source, ledger, wrong)
                .unwrap_err()
                .to_string()
                .contains("D-NPM is cited at")
        );
        let unseen = r#"[{"url": "https://c.example/", "text": "Elsewhere", "ids": ["S004"]}]"#;
        assert!(citations(source, ledger, unseen).is_err());
    }
}
