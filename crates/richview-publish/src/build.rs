use crate::{
    Correction, Figure, Item, Kind, Metadata, Object, Release, SHARE_IMAGE, Site, State, Theme,
    cover, escape, identifier, presentation,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, String>;
const MAX_ITEMS: usize = 1000;
const MAX_OBJECT: usize = 20 * 1024 * 1024;
const MAX_RELEASE: usize = 256 * 1024 * 1024;

fn read(path: &Path) -> Result<Vec<u8>> {
    let meta = fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.len() > MAX_OBJECT as u64 {
        return Err(format!("{} exceeds the 20 MiB file limit", path.display()));
    }
    fs::read(path).map_err(|e| e.to_string())
}
fn text(path: &Path) -> Result<String> {
    String::from_utf8(read(path)?).map_err(|e| e.to_string())
}
fn contained(root: &Path, relative: &str) -> Result<PathBuf> {
    if !richview_core::document::report_relative_path(relative) {
        return Err(format!("Invalid local resource path: {relative}"));
    }
    let path = fs::canonicalize(root.join(relative)).map_err(|e| format!("{relative}: {e}"))?;
    if !path.starts_with(root) {
        return Err(format!("Resource escapes its content folder: {relative}"));
    }
    Ok(path)
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn put(
    bytes: Vec<u8>,
    kind: &str,
    objects: &mut BTreeMap<String, Object>,
    data: &mut BTreeMap<String, Vec<u8>>,
) -> Result<String> {
    if bytes.len() > MAX_OBJECT {
        return Err("Generated object exceeds 20 MiB".into());
    }
    let hash = digest(&bytes);
    objects.insert(
        hash.clone(),
        Object {
            sha256: hash.clone(),
            bytes: bytes.len(),
            content_type: kind.into(),
        },
    );
    data.insert(hash.clone(), bytes);
    if data.values().map(Vec::len).sum::<usize>() > MAX_RELEASE {
        return Err("Publication exceeds the bounded 256 MiB release budget".into());
    }
    Ok(hash)
}
fn validate_site(site: &Site) -> Result<()> {
    if site.title.trim().is_empty() || site.title.len() > 120 || site.description.len() > 500 {
        return Err("Site needs a short title and description".into());
    }
    if !site.base_url.starts_with("https://")
        || site
            .base_url
            .contains(['\"', '\'', '<', '>', '?', '#', '@'])
        || site.base_url.ends_with('/')
    {
        return Err(
            "base_url must be HTTPS, without credentials, query, fragment or trailing slash".into(),
        );
    }
    if site.accent.len() != 7
        || !site.accent.starts_with('#')
        || !site.accent[1..].bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err("accent must be a six-digit hex color".into());
    }
    let mut sections = BTreeSet::new();
    for section in &site.sections {
        if !identifier(&section.id) || !sections.insert(&section.id) || section.title.is_empty() {
            return Err("Section IDs must be unique lowercase slugs with titles".into());
        }
    }
    if sections.is_empty()
        || sections.len() > 20
        || site.navigation.len() > 12
        || site.lead.len() > 4
    {
        return Err("Use 1–20 sections, at most 12 navigation links and 4 lead items".into());
    }
    for link in &site.navigation {
        if !link.path.starts_with('/')
            || link.path.starts_with("//")
            || link.path.contains(['\\', '\"', '<', '>', '?', '#', '%'])
            || link.label.is_empty()
        {
            return Err("Navigation needs local paths and labels".into());
        }
    }
    Ok(())
}
/// The instant an RFC 3339 date names, read as chrono read it.
fn instant(date: &str) -> Option<richview_app::time::Instant> {
    richview_app::time::parse_rfc3339(date).map(|parsed| parsed.instant)
}
fn valid_date(date: &str) -> bool {
    instant(date).is_some() && date.len() >= 20
}
fn validate_metadata(meta: &Metadata, site: &Site) -> Result<()> {
    if meta.kind == Kind::Dashboard || meta.snapshot.is_some() {
        return Err("publish.kind takes only report and page: dashboard and snapshot pages are retired (FEA-BR-30)".into());
    }
    if !identifier(&meta.id) || !identifier(&meta.slug) || !identifier(&meta.audience) {
        return Err("Item id, slug and audience must be lowercase slugs".into());
    }
    if !site.sections.iter().any(|s| s.id == meta.section) {
        return Err(format!("Unknown section {}", meta.section));
    }
    if meta.summary.is_empty()
        || meta.summary.len() > 600
        || meta.byline.is_empty()
        || meta.byline.len() > 160
        || meta.tags.len() > 16
    {
        return Err(
            "Items require a summary (up to 600 characters), byline, and at most 16 tags".into(),
        );
    }
    if !valid_date(&meta.published)
        || !valid_date(&meta.updated)
        || instant(&meta.updated) < instant(&meta.published)
    {
        return Err("Dates must be RFC3339, with update on or after publication".into());
    }
    for Correction { date, note } in &meta.corrections {
        if !valid_date(date) || note.trim().is_empty() {
            return Err("Corrections require a date and explanation".into());
        }
    }
    if meta.aliases.iter().any(|a| !identifier(a)) {
        return Err("Aliases must be former slugs, not URLs".into());
    }
    if meta.cover.is_some() && meta.cover_alt.as_ref().is_none_or(|a| a.trim().is_empty()) {
        return Err("A cover needs meaningful cover_alt text".into());
    }
    match (&meta.cover, &meta.cover_credit) {
        (None, Some(_)) => return Err("cover_credit needs a cover".into()),
        (Some(src), credit) => {
            let remote = cover::is_remote(src);
            if remote {
                cover::validate_remote(src)?;
                if credit.is_none() {
                    return Err(
                        "A remote Unsplash cover needs cover_credit with the photographer's name"
                            .into(),
                    );
                }
            }
            if let Some(credit) = credit {
                cover::validate_credit(credit, remote)?;
            }
        }
        (None, None) => {}
    }
    if let Some(Figure {
        value,
        label,
        as_of,
    }) = &meta.figure
    {
        let within = |s: &str, max: usize| !s.trim().is_empty() && s.chars().count() <= max;
        if !within(value, 16) || !within(label, 80) || !within(as_of, 32) {
            return Err(
                "figure needs a value (up to 16 characters), label (up to 80) and as_of (up to 32)"
                    .into(),
            );
        }
    }
    Ok(())
}

/// Assemble all dependent routes and assets into one content-addressed release. No network reads.
/// A failed build never changes an existing output directory or any hosted release.
pub fn build(site_path: &Path, output: &Path, runtime_dir: &Path, commit: &str) -> Result<Release> {
    build_with_connectors(site_path, output, runtime_dir, commit, None)
}

/// Build from stored snapshots selected by trusted host configuration; never execute queries.
pub fn build_with_connectors(
    site_path: &Path,
    output: &Path,
    runtime_dir: &Path,
    commit: &str,
    connectors: Option<&Path>,
) -> Result<Release> {
    if commit.len() != 40 || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("source-commit must be a full Git commit SHA".into());
    }
    if output.exists() {
        return Err("Output already exists; choose a fresh release directory".into());
    }
    let site_path = fs::canonicalize(site_path).map_err(|e| e.to_string())?;
    let root = site_path.parent().ok_or("Site has no parent")?;
    let site: Site = richview_core::yaml::from_str(&text(&site_path)?)
        .map_err(|e| e.to_string())
        .and_then(|value| serde_json::from_value(value).map_err(|e| e.to_string()))
        .map_err(|e| format!("site.yaml: {e}"))?;
    validate_site(&site)?;
    let mut objects = BTreeMap::new();
    let mut data = BTreeMap::new();
    let mut runtime = BTreeMap::new();
    for (name, mime) in [
        ("richview_runtime.js", "text/javascript; charset=utf-8"),
        ("richview_runtime_bg.wasm", "application/wasm"),
    ] {
        runtime.insert(
            name.to_owned(),
            put(
                read(&runtime_dir.join(name))?,
                mime,
                &mut objects,
                &mut data,
            )?,
        );
    }
    let runtime_id = digest(serde_json::to_string(&runtime).unwrap().as_bytes());
    let runtime_js = String::from_utf8(data[&runtime["richview_runtime.js"]].clone())
        .map_err(|e| format!("Runtime bindings are not UTF-8: {e}"))?;
    if runtime_js.is_empty() {
        return Err("Runtime bindings are empty".into());
    }
    let mut entries = fs::read_dir(root.join("items"))
        .map_err(|e| format!("items/: {e}"))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    entries.sort_by_key(|e| e.file_name());
    if entries.len() > MAX_ITEMS {
        return Err(format!(
            "At most {MAX_ITEMS} item folders per bounded build"
        ));
    }
    let mut items = Vec::new();
    let mut ids = BTreeSet::new();
    let mut routes = BTreeSet::new();
    for entry in entries {
        if entry.file_type().map_err(|e| e.to_string())?.is_symlink() {
            return Err("Symlinked item folders are not allowed".into());
        }
        if !entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            return Err("items/ contains a file; keep each report in its own folder".into());
        }
        let folder = fs::canonicalize(entry.path()).map_err(|e| e.to_string())?;
        let path = contained(&folder, "report.rmd")?;
        let source = richview_app::cli::read_source(&path)?;
        let doc = richview_core::document::parse(&source)
            .map_err(|d| serde_json::to_string(&d).unwrap())?;
        let title = doc.frontmatter["title"]
            .as_str()
            .filter(|s| !s.trim().is_empty() && s.len() <= 200)
            .ok_or("Report needs a title of at most 200 characters")?
            .to_owned();
        let meta: Metadata = serde_json::from_value(doc.frontmatter["publish"].clone())
            .map_err(|e| format!("{title} publish metadata: {e}"))?;
        if entry.file_name().to_string_lossy() != meta.id {
            return Err("Item folder must equal its stable id".into());
        }
        validate_metadata(&meta, &site).map_err(|e| format!("{title}: {e}"))?;
        if meta.state == crate::State::Published
            && richview_core::document::validate_publish(&source)
                .iter()
                .any(|d| d.level == "error")
        {
            return Err(format!("{title}: invalid published Rich Markdown metadata"));
        }
        if !ids.insert(meta.id.clone()) {
            return Err(format!("Duplicate stable id {}", meta.id));
        }
        let prefix = match meta.kind {
            Kind::Report => "stories",
            Kind::Page => "pages",
            Kind::Dashboard => unreachable!("validate_metadata refuses dashboards"),
        };
        let route = if meta.kind == Kind::Page {
            format!("/{}", meta.slug)
        } else {
            format!("/{prefix}/{}", meta.slug)
        };
        for slug in std::iter::once(&meta.slug).chain(meta.aliases.iter()) {
            let alias = if meta.kind == Kind::Page {
                format!("/{slug}")
            } else {
                format!("/{prefix}/{slug}")
            };
            if [
                "/archive",
                "/search",
                "/internal",
                "/sections",
                "/feed.xml",
                "/sitemap.xml",
                "/robots.txt",
            ]
            .contains(&alias.as_str())
                || alias.starts_with("/_")
                || !routes.insert(alias)
            {
                return Err(format!("Reserved or duplicate route for {}", meta.id));
            }
        }
        let mut assets = BTreeMap::new();
        let mut cover_lead = richview_share::Lead::None;
        let mut thumbnail = None;
        let chart_svg: Option<String>;
        let company: richview_core::CompanyBrand;
        let content = {
            if doc.blocks.iter().any(|b| b.tag == "embed") {
                return Err("Hosted embedded services are outside the offline publication contract; use a sourced link".into());
            }
            if doc.blocks.iter().any(|block| {
                block.value.pointer("/data/sql").is_some()
                    || block.value.pointer("/data/metricflow").is_some()
            }) {
                return Err("Publication reads stored data.cache snapshots; materialize live queries before publishing".into());
            }
            let registry = if doc
                .blocks
                .iter()
                .any(|block| block.value.pointer("/data/cache").is_some())
            {
                Some(richview_app::connectors::config::load_snapshot_registry(
                    path.parent().ok_or("item has no report folder")?,
                    connectors,
                )?)
            } else {
                None
            };
            let (doc, mut options, mut inputs) = richview_app::resolver::resolve_version(
                &source,
                &path,
                &[],
                false,
                false,
                registry.as_ref(),
            )?;
            // A remote cover is hotlinked by the reader's browser: never read or fetched here.
            if let Some(cover) = meta.cover.as_ref().filter(|c| !cover::is_remote(c)) {
                let cover_path = contained(&folder, cover)?;
                let mime = match cover_path.extension().and_then(|e| e.to_str()) {
                    Some("svg") => "image/svg+xml",
                    Some("png") => "image/png",
                    Some("jpg" | "jpeg") => "image/jpeg",
                    Some("webp") => "image/webp",
                    _ => return Err("Cover must be SVG, PNG, JPEG or WebP".into()),
                };
                let bytes = inputs.attachment(&folder, cover)?;
                cover_lead = richview_share::Lead::Cover {
                    bytes: bytes.as_ref().clone(),
                    mime: mime.into(),
                };
                assets.insert(
                    "cover".into(),
                    put(bytes.as_ref().clone(), mime, &mut objects, &mut data)?,
                );
            }
            options.source = Some("report.rmd".into());
            if let Some(folder) = options.report_folder.as_mut() {
                folder.root = meta.id.clone();
            }
            company = richview_core::report_company_brand(&doc.frontmatter, &options);
            // Root-relative, so one release works on every allowed host (live and preview).
            options.runtime_url = Some(format!("/_runtime/{runtime_id}/richview_runtime"));
            options.runtime_js = Some(runtime_js.clone());
            let report = richview_core::document::render_document(doc.clone(), &options)
                .map_err(|d| serde_json::to_string(&d).unwrap())?;
            if report.diagnostics.iter().any(|d| d.level == "error") {
                return Err(format!(
                    "{title}: {}",
                    serde_json::to_string(&report.diagnostics).unwrap()
                ));
            }
            // Linked downloads retain per-item authorization, just like inline media and data.
            for reference in richview_app::resolver::linked_paths(&doc) {
                let bytes = inputs.attachment(&folder, &reference)?;
                assets.insert(
                    reference,
                    put(
                        bytes.as_ref().clone(),
                        "application/octet-stream",
                        &mut objects,
                        &mut data,
                    )?,
                );
            }
            // The first chart doubles as the front-page lead visual: a standalone, self-styled
            // SVG served under the same image sandbox as covers.
            if let Some((chart, svg)) = first_chart(&report.html, &report.charts)
                && !assets.contains_key(THUMBNAIL)
            {
                assets.insert(
                    THUMBNAIL.into(),
                    put(
                        svg.clone().into_bytes(),
                        "image/svg+xml",
                        &mut objects,
                        &mut data,
                    )?,
                );
                thumbnail = Some(chart);
            }
            chart_svg = richview_share::chart_svg(&report.charts);
            report.html
        };
        // The link-preview card (FEA-SF-55): only a published story has one, and the host decides
        // at request time who may be told about it. An author's own linked `og.png` is left alone.
        if meta.kind == Kind::Report
            && meta.state == State::Published
            && !assets.contains_key(SHARE_IMAGE)
        {
            let lead = match chart_svg {
                Some(svg) => richview_share::Lead::ChartSvg(svg),
                None => cover_lead,
            };
            // The story's own style (STY-70); one that names none takes the publication's theme.
            let style: String = doc.frontmatter["styles"]
                .get(0)
                .and_then(|s| s.as_str())
                .map_or_else(
                    || {
                        if site.theme == Theme::Ink {
                            "sleek"
                        } else {
                            "air"
                        }
                        .into()
                    },
                    str::to_owned,
                );
            let style = company.base.as_deref().unwrap_or(&style).to_owned();
            let rule = richview_share::style_rule(&style);
            let png = richview_share::render(&richview_share::ShareCard {
                title: title.clone(),
                byline: meta.byline.clone(),
                style: style.clone(),
                accent: Some(site.accent.clone()),
                rule,
                lead,
                company: (company.logo.is_some() || company.accent.is_some()).then_some(company),
            })
            .map_err(|e| format!("{title}: share image: {e}"))?;
            assets.insert(
                SHARE_IMAGE.into(),
                put(png, "image/png", &mut objects, &mut data)?,
            );
        }
        let content = put(
            content.into_bytes(),
            "text/html; charset=utf-8",
            &mut objects,
            &mut data,
        )?;
        let mut item = Item {
            title: title.clone(),
            source: format!("items/{}/report.rmd", entry.file_name().to_string_lossy()),
            route: route.clone(),
            article: String::new(),
            content,
            card: presentation::card(&site, &meta, &title, &route),
            lead: presentation::lead(&site, &meta, &title, &route, thumbnail.as_ref()),
            stat: presentation::stat(&meta, &route),
            meta,
            assets,
        };
        item.article = put(
            presentation::article(&site, &item).into_bytes(),
            "text/html; charset=utf-8",
            &mut objects,
            &mut data,
        )?;
        items.push(item);
    }
    items.sort_by(|a, b| {
        instant(&b.meta.published)
            .expect("validated as RFC 3339")
            .cmp(&instant(&a.meta.published).expect("validated as RFC 3339"))
            .then(a.meta.id.cmp(&b.meta.id))
    });
    for lead in &site.lead {
        if !ids.contains(lead) {
            return Err(format!("Lead refers to missing stable id {lead}"));
        }
    }
    let mut templates = BTreeMap::new();
    let pages = [
        (
            "home",
            site.title.clone(),
            "/",
            format!(
                "<h1 class=\"sr-only\">{}</h1><section class=\"intro\"><p class=\"mission\">{}</p><p class=\"dateline\"><!--PUBLISH:DATELINE--></p></section><!--PUBLISH:FIGURES--><!--PUBLISH:LEAD--><section class=\"latest\" aria-labelledby=\"latest\"><h2 class=\"list-title\" id=\"latest\">Latest</h2><div class=\"rows\"><!--PUBLISH:ITEMS--></div><!--PUBLISH:PAGINATION--></section>",
                escape(&site.title),
                escape(&site.description)
            ),
        ),
        (
            "archive",
            "Archive".to_owned(),
            "/archive",
            "<header class=\"page-head\"><h1>Archive</h1><p>Every story, newest first.</p></header><div class=\"rows\"><!--PUBLISH:ITEMS--></div><!--PUBLISH:PAGINATION-->".to_owned(),
        ),
        (
            "search",
            "Search".to_owned(),
            "/search",
            "<header class=\"page-head\"><h1>Search</h1><p>Find stories by title, summary, author or tag.</p><form class=\"search-form\" action=\"/search\" role=\"search\"><input type=\"search\" name=\"q\" aria-label=\"Search stories\" value=\"<!--PUBLISH:QUERY-->\" placeholder=\"Try a word such as denominator\"><button>Search</button></form></header><h2 class=\"sr-only\">Results</h2><div class=\"rows\"><!--PUBLISH:ITEMS--></div><!--PUBLISH:PAGINATION-->".to_owned(),
        ),
    ];
    for (key, title, path, main) in pages {
        templates.insert(
            key.into(),
            put(
                presentation::shell(&site, &title, &site.description, path, &main).into_bytes(),
                "text/html; charset=utf-8",
                &mut objects,
                &mut data,
            )?,
        );
    }
    for section in &site.sections {
        let main = format!(
            "<header class=\"page-head\"><h1>{}</h1><p>{}</p></header><h2 class=\"sr-only\">Stories</h2><div class=\"rows rows--section\"><!--PUBLISH:ITEMS--></div><!--PUBLISH:PAGINATION-->",
            escape(&section.title),
            escape(&section.description)
        );
        templates.insert(
            format!("section:{}", section.id),
            put(
                presentation::shell(
                    &site,
                    &section.title,
                    &section.description,
                    &format!("/sections/{}", section.id),
                    &main,
                )
                .into_bytes(),
                "text/html; charset=utf-8",
                &mut objects,
                &mut data,
            )?,
        );
    }
    let mut release = Release {
        schema: 1,
        id: String::new(),
        commit: commit.into(),
        toolchain: "rust-1.99.0/wasm-bindgen-0.2.129/publish-1".into(),
        site,
        items,
        templates,
        runtime,
        objects,
    };
    release.id = digest(&serde_json::to_vec(&release).map_err(|e| e.to_string())?);
    let staged = output.with_extension(format!("building-{}", std::process::id()));
    if staged.exists() {
        return Err(
            "Staging directory exists from an interrupted build; inspect it before retrying".into(),
        );
    }
    fs::create_dir_all(staged.join("objects")).map_err(|e| e.to_string())?;
    for (hash, bytes) in data {
        fs::write(staged.join("objects").join(hash), bytes).map_err(|e| e.to_string())?;
    }
    fs::write(
        staged.join("release.json"),
        serde_json::to_vec_pretty(&release).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    fs::rename(&staged, output).map_err(|e| e.to_string())?;
    Ok(release)
}

/// Asset name of the standalone first-chart SVG used as the front-page lead visual.
const THUMBNAIL: &str = "thumbnail.svg";
/// Lead visuals are about 480 CSS pixels wide.
const THUMBNAIL_WIDTH: f64 = 480.0;

/// The end of the element that starts at `start`, counting nested elements of the same tag.
fn element_end(html: &str, start: usize, tag: &str) -> Option<usize> {
    let (open, close) = (format!("<{tag}"), format!("</{tag}>"));
    let mut depth = 0usize;
    let mut at = start;
    loop {
        let next_open = html[at + 1..].find(&open).map(|i| i + at + 1);
        let next_close = html[at + 1..].find(&close).map(|i| i + at + 1)?;
        match next_open {
            Some(o) if o < next_close => {
                depth += 1;
                at = o;
            }
            _ if depth == 0 => return Some(next_close + close.len()),
            _ => {
                depth -= 1;
                at = next_close;
            }
        }
    }
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// Plain text of an HTML fragment produced by the renderer: tags dropped, entities decoded.
fn plain_text(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(c),
            _ => {}
        }
    }
    unescape(
        text.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .as_str(),
    )
}

/// The thumbnail's one face. The engine has no metrics for it and measures it at the fixed
/// monospace advance, which is Plex Mono's own.
const THUMBNAIL_FAMILY: &str = "\"RV Plex Mono\", ui-monospace, monospace";

/// IBM Plex Mono (latin, OFL; licence in cloudflare/publish/fonts/licenses) for thumbnail text.
/// An SVG loaded as <img> cannot fetch fonts, so the face travels inside it as a data URI.
const THUMBNAIL_FONT: &[u8] =
    include_bytes!("../../../cloudflare/publish/fonts/ibm-plex-mono-latin-400-normal.woff2");

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            out.push(if i <= chunk.len() {
                ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    out
}

/// The report's first chart as a standalone SVG with its font inside, plus the chart's title,
/// source note and legend. The engine lays the chart's payload out again at the lead column's
/// width in its initial state, every face measured as the data face the thumbnail draws in.
fn first_chart(
    html: &str,
    charts: &[serde_json::Value],
) -> Option<(presentation::LeadChart, String)> {
    use richview_chart::block::ChartBlock;
    use richview_chart::layout::{Options, layout};
    use richview_chart::reader_controls::Controls;
    // The first chart with a readable payload and a title leads.
    let (payload, block, caption) = charts.iter().find_map(|value| {
        let payload: richview_chart::payload::Payload =
            serde_json::from_value(value.clone()).ok()?;
        let block = ChartBlock::from_value(&payload.block)?;
        let caption = Some(block.title.trim().to_owned()).filter(|t| !t.is_empty())?;
        Some((payload, block, caption))
    })?;
    let start = html.find(&format!("<figure class=\"rv-chart\" id=\"{}\"", payload.id))?;
    let end = element_end(html, start, "figure")?;
    let source = html[end..]
        .trim_start()
        .strip_prefix("<p class=\"rv-source\">")
        .and_then(|rest| rest.split_once("</p>"))
        .map(|(note, _)| plain_text(note))
        .filter(|note| !note.is_empty());
    // An SVG loaded as <img> draws in the one face it carries. Every face is that monospace,
    // which the engine measures at its fixed advance, so text is laid out where it is drawn.
    let mut theme = payload.theme.clone();
    theme.data_font = THUMBNAIL_FAMILY.into();
    theme.ui_font = THUMBNAIL_FAMILY.into();
    theme.figures_font = None;
    theme.motion = false;
    let objects = payload.objects();
    let base = Options {
        width: THUMBNAIL_WIDTH,
        lang: payload.lang.clone(),
        colors: payload.colors.clone(),
        ..Options::default()
    };
    let controls = Controls::parse(&payload.block, &objects);
    let state = controls.initial();
    let g = match controls.chart(&payload.block, &objects, &state) {
        Some(shown) if !controls.is_empty() => {
            let options = controls.options(&payload.block, &objects, &state, &theme, &base);
            layout(&shown.block, &shown.rows, &theme, &options)
        }
        _ => layout(&block, &objects, &theme, &base),
    };
    let legend = g
        .legend
        .as_ref()
        .map(|l| {
            l.entries
                .iter()
                .filter_map(|&i| g.series.get(i))
                .map(|s| (s.name.clone(), s.color.hex()))
                .collect()
        })
        .unwrap_or_default();
    let mut svg = richview_chart::vector::svg(&g, &theme);
    svg.insert_str(
        svg.len() - "</svg>".len(),
        &format!(
            "<style>@font-face{{font-family:\"RV Plex Mono\";src:url(data:font/woff2;base64,{}) format(\"woff2\")}}</style>",
            base64(THUMBNAIL_FONT)
        ),
    );
    Some((
        presentation::LeadChart {
            title: caption,
            source,
            legend,
        },
        svg,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_chart_lays_the_payload_out_as_a_standalone_svg() {
        let source = "# Rates\n\n```chart\ntype: bar\ntitle: Rate & share\ndata:\n  inline:\n    - {month: Jan, region: North, rate: 1}\n    - {month: Feb, region: North, rate: 2}\n    - {month: Jan, region: South, rate: 3}\n    - {month: Feb, region: South, rate: 1}\nencoding: {x: month, y: rate, color: region}\n```\n\n*Source: invented.*\n\n```chart\ntype: bar\ntitle: Second\ndata: {inline: [{c: A, v: 1}]}\nencoding: {x: c, y: v}\n```\n";
        let options = richview_core::document::RenderOptions {
            report_folder: Some(richview_core::document::ReportFolder {
                root: "lead".into(),
                source: "report.rmd".into(),
                files: vec!["report.rmd".into()],
            }),
            ..Default::default()
        };
        let report = richview_core::document::render(source, &options).unwrap();
        let (chart, svg) = first_chart(&report.html, &report.charts).unwrap();
        assert_eq!(chart.title, "Rate & share");
        assert_eq!(chart.source.as_deref(), Some("Source: invented."));
        let names: Vec<&str> = chart.legend.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["North", "South"]);
        assert!(chart.legend.iter().all(|(_, c)| c.starts_with('#')));
        assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 480 "));
        assert!(svg.contains("data:font/woff2;base64,d09GMg"));
        assert!(svg.ends_with("</style></svg>") && svg.contains("<path"));
        assert!(!svg.contains("Second"));
        assert!(first_chart("<p>No charts</p>", &[]).is_none());
    }

    #[test]
    fn the_card_takes_its_rule_from_the_resolved_style_and_draws_the_first_chart() {
        use richview_share::Rule;
        assert_eq!(richview_share::style_rule("sleek"), Some(Rule::None));
        assert_eq!(richview_share::style_rule("air"), Some(Rule::None));
        assert_eq!(
            richview_share::style_rule("scientific"),
            Some(Rule::Hairline)
        );
        assert_eq!(richview_share::style_rule("brutalist"), Some(Rule::InkBars));
        assert_eq!(richview_share::style_rule("hand-drawn"), Some(Rule::Pencil));
        let source = "# Rates\n\n```chart\ntype: bar\ntitle: Rate\ndata:\n  inline:\n    - {month: Jan, rate: 1}\n    - {month: Feb, rate: 2}\nencoding: {x: month, y: rate}\n```\n";
        let options = richview_core::document::RenderOptions {
            report_folder: Some(richview_core::document::ReportFolder {
                root: "lead".into(),
                source: "report.rmd".into(),
                files: vec!["report.rmd".into()],
            }),
            ..Default::default()
        };
        let report = richview_core::document::render(source, &options).unwrap();
        let svg = richview_share::chart_svg(&report.charts).unwrap();
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("IBM Plex Mono") && svg.contains("<rect"));
        assert!(richview_share::chart_svg(&[]).is_none());
    }
}
