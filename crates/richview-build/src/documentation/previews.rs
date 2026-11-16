//! Shared-runtime previews and semantic gallery grids for the docs visual pass.
use super::*;

/// The flagship report (FEA-SF-36): first in the Reports list, opened on its own showcase page
/// rather than in the docs gallery.
pub(super) const FLAGSHIP: &str = "openai-vs-anthropic";
pub(super) const FLAGSHIP_URL: &str = "/showcase/openai-vs-anthropic/";

pub(super) fn style(report: &str) -> &'static str {
    match report {
        "longer-lives" | FLAGSHIP => "sleek",
        "air-we-changed" | "families-and-lives" => "air",
        "first-charts" => "hand-drawn",
        "titanic" => "brutalist",
        _ => "scientific",
    }
}

fn card(slug: &str, title: &str, note: &str, style: &str, link: &str) -> String {
    format!(
        "<article class=\"docs-chart-card\"><a class=\"docs-chart-card__link\" href=\"{link}\"><div class=\"docs-thumbnail\"><iframe data-docs-preview tabindex=\"-1\" aria-hidden=\"true\" title=\"{} chart preview\" data-src=\"/docs/previews/{slug}/{style}.html#rv-chart-0\" sandbox=\"allow-scripts\"></iframe></div><h3>{}</h3><p>{}</p><p class=\"docs-chart-card__style\">{}</p></a></article>",
        escape(title),
        escape(title),
        escape(note),
        chrome::style_label(style)
    )
}

pub(super) fn featured_menu() -> String {
    let mut html = "<div class=\"docs-featured-grid\">".to_owned();
    for &(slug, label, name, _) in FEATURED {
        let why = match slug {
            "playfair" => "Prices and wages on separate scales; selected historical observations.",
            "nightingale" => "Sanitary reform made visible; annualized mortality, 1854–1856.",
            "anscombe" => "Four shapes, almost the same statistics; 1973.",
            "old-faithful" => "The clusters an average conceals.",
            "titanic" => "Follow the counts across class, sex and survival; 1912.",
            "gapminder" => "Play six decades of country observations; 1960–2020.",
            "global-temperature" => "Annual change against the same NASA baseline; 1880–2025.",
            "keeling-curve" => "A seasonal cycle on a long rise in CO₂.",
            _ => "15-minute electricity prices, including negative intervals; 2025–2026.",
        };
        html.push_str(&card(
            slug,
            label,
            why,
            style(name),
            &format!("/docs/charts/{slug}"),
        ));
    }
    html.push_str("</div>");
    html
}

/// The flagship's cover graphic, as its report names it in `title_image` and the showcase
/// publishes it beside the report.
pub(super) const FLAGSHIP_COVER: &str = "assets/cover-two-publics.svg";

/// The flagship's card spans the Reports grid and shows the cover the report opens with, which
/// says its finding at a glance; the image is decorative, because the title names the link.
fn flagship_card() -> String {
    format!(
        "<article class=\"docs-chart-card docs-chart-card--flagship\"><a class=\"docs-chart-card__link\" href=\"{FLAGSHIP_URL}\"><div class=\"docs-thumbnail docs-thumbnail--cover\"><img src=\"{FLAGSHIP_URL}{FLAGSHIP_COVER}\" alt=\"\" width=\"1200\" height=\"630\" loading=\"lazy\" decoding=\"async\"></div><h3>The Most Expensive Race Ever Run</h3><p>The flagship · OpenAI against Anthropic, population by population · 2023–2026</p></a></article>"
    )
}

pub(super) fn report_grid() -> String {
    let mut html = "<div class=\"docs-report-grid\">".to_owned();
    html.push_str(&flagship_card());
    for (name, title, note) in [
        (
            "longer-lives",
            "Longer lives",
            "Gapminder · regional life expectancy · 1950–2019",
        ),
        (
            "air-we-changed",
            "A warmer world",
            "NASA GISTEMP · annual anomalies · 1880–2025",
        ),
        (
            "same-numbers",
            "The same summary",
            "Anscombe · four datasets · 1973",
        ),
        (
            "titanic",
            "Passengers and crew",
            "Titanic · class, sex and survival · 1912",
        ),
        (
            "first-charts",
            "Wheat and wages",
            "Playfair · a historical sample · 1565–1805",
        ),
        (
            "families-and-lives",
            "Families and longer lives",
            "Gapminder · country trajectories · 1960–2020",
        ),
    ] {
        html.push_str(&card(
            &format!("report-{name}"),
            title,
            note,
            style(name),
            &format!("/docs/gallery/{name}/"),
        ));
    }
    html.push_str("</div>");
    html
}

pub(super) fn doors() -> String {
    "<nav class=\"docs-doors\" aria-label=\"Explore the documentation\"><a href=\"/docs/charts\"><strong>Gallery</strong><span>Choose a chart, then take its source.</span></a><a href=\"/docs/first-report\"><strong>How to use</strong><span>Install, make a report and check the result.</span></a><a href=\"/docs/reference\"><strong>Nitty-gritty</strong><span>Find a field, rule or diagnostic.</span></a></nav>".into()
}

/// The end of the rules a preview frame carries in `docs.css`; everything after it styles the
/// documentation pages and stays out of the hundreds of frames.
const PREVIEW_CSS_END: &str = "/* docs-preview-end */";

/// The part of the documentation stylesheet that each preview frame inlines.
fn preview_css(stylesheet: &str) -> Result<String> {
    if stylesheet.matches(PREVIEW_CSS_END).count() != 1 {
        return Err("docs.css must mark the end of its preview rules exactly once".into());
    }
    Ok(stylesheet
        .split(PREVIEW_CSS_END)
        .next()
        .unwrap_or_default()
        .to_owned())
}

fn render(
    source: &str,
    options: &RenderOptions,
    folder: &Path,
    styles: &[&str],
    flags: BuildFlags,
    css: &str,
) -> Result<()> {
    fs::create_dir_all(folder)?;
    let source = without_reading_aids(&public::source(source, flags)?);
    for style in styles {
        let mut options = options.clone();
        options.styles = vec![(*style).into()];
        let report = richview_core::document::render(&source, &options)
            .map_err(|errors| format!("Preview: {errors:?}"))?;
        ensure_clean(style, &report.diagnostics)?;
        let mut html = report.html;
        // Opaque sandboxed frames need credentials for the gated preview host.
        let script = html
            .find("<script src=\"")
            .ok_or("preview runtime script is missing")?;
        let end = html[script..]
            .find('>')
            .ok_or("preview runtime script is malformed")?
            + script;
        let tag = &html[script..end];
        if !tag.contains("crossorigin=\"anonymous\"") {
            return Err("preview runtime must have an anonymous CORS attribute".into());
        }
        let tag = tag.replace(
            "crossorigin=\"anonymous\"",
            "crossorigin=\"use-credentials\"",
        );
        html.replace_range(script..end, &tag);
        if let Some(start) = html.find("<header class=\"rv-report__header\"")
            && let Some(end) = html[start..].find("</header>")
        {
            html.replace_range(start..start + end + "</header>".len(), "");
        }
        // The frame is laid out 1000 px wide and shown at a third to four fifths of that: drawing
        // at more than 1.5 device pixels per CSS pixel only spends the reader's memory.
        html = html
            .replacen(
                "</head>",
                &format!(
                    "<meta name=\"richview-max-dpr\" content=\"1.5\"><style>{css}</style></head>"
                ),
                1,
            )
            .replacen("<body>", "<body class=\"docs-preview\">", 1);
        fs::write(folder.join(format!("{style}.html")), html)?;
    }
    Ok(())
}

pub(super) fn stage(
    root: &Path,
    docs: &Path,
    public: &Path,
    charts: &[ChartPage],
    flags: BuildFlags,
) -> Result<()> {
    let css = preview_css(&fs::read_to_string(root.join("assets/docs/docs.css"))?)?;
    let mut options = documentation_options();
    crate::runtime::load(public, &mut options)?;
    // WebKit treats an opaque frame's own origin as distinct even for connect-src 'self'. Bundle
    // Wasm into the shared script so the frame needs no connection outside its report policy.
    use base64::{Engine, engine::general_purpose::STANDARD};
    use sha2::Digest;
    let js = fs::read_to_string(public.join("richview_runtime.js"))?;
    let fetch = "module_or_path = fetch(module_or_path);";
    if js.matches(fetch).count() != 1 {
        return Err("unexpected wasm-bindgen runtime fetch; cannot stage docs previews".into());
    }
    let wasm = fs::read(public.join("richview_runtime_bg.wasm"))?;
    // A plain loop: a mapping callback per byte costs every frame 160 ms on a fast laptop and
    // about a second on a phone, on the page's main thread.
    let js = js.replacen(
        fetch,
        &format!(
            "module_or_path = (s => {{ const b = new Uint8Array(s.length); for (let i = 0; i < s.length; i++) b[i] = s.charCodeAt(i); return b; }})(atob('{}'));",
            STANDARD.encode(&wasm)
        ),
        1,
    );
    let hash = format!(
        "{:x}",
        sha2::Sha256::digest([js.as_bytes(), wasm.as_slice()].concat())
    );
    let folder = docs.join("runtime").join(&hash[..16]);
    fs::create_dir_all(&folder)?;
    fs::write(folder.join("richview_runtime.js"), &js)?;
    fs::write(folder.join("richview_runtime_bg.wasm"), &wasm)?;
    options.runtime_js = Some(js);
    options.runtime_url = Some(format!("/docs/runtime/{}/richview_runtime", &hash[..16]));
    options.runtime_wasm = None;
    let preview = docs.join("previews");
    for page in charts {
        render(
            &example_source(page),
            &options,
            &preview.join(&page.name),
            &chrome::site_styles(),
            flags,
            &css,
        )?;
    }
    for &(slug, _, _, _) in FEATURED {
        let folder = public.join("examples/charts").join(slug);
        let source = fs::read_to_string(folder.join("report.rmd"))?;
        let mut own = options.clone();
        own.report_folder = Some(richview_app::resolver::report_folder(
            &folder.join("report.rmd"),
        )?);
        own.data.insert(
            "data.csv".into(),
            fs::read_to_string(folder.join("data.csv"))?,
        );
        render(
            &source,
            &own,
            &preview.join(slug),
            &chrome::site_styles(),
            flags,
            &css,
        )?;
    }
    report_preview(
        FLAGSHIP,
        &root.join("examples").join(FLAGSHIP).join("report.rmd"),
        &options,
        &preview,
        flags,
        &css,
    )?;
    for &(name, _) in REPORTS
        .iter()
        .filter(|(name, _)| !["blocks", "typography"].contains(name))
    {
        report_preview(
            name,
            &root
                .join("docs/public/gallery")
                .join(name)
                .join("report.rmd"),
            &options,
            &preview,
            flags,
            &css,
        )?;
    }
    Ok(())
}

/// A report's card preview: its first chart and the credit line under it, in the report's style.
fn report_preview(
    name: &str,
    file: &Path,
    options: &RenderOptions,
    preview: &Path,
    flags: BuildFlags,
    css: &str,
) -> Result<()> {
    let source = fs::read_to_string(file)?;
    let (_, mut own) =
        richview_app::resolver::resolve_document_options_policy(&source, file, &[], false, false)?;
    own.runtime_js = options.runtime_js.clone();
    own.runtime_url = options.runtime_url.clone();
    own.runtime_wasm = None;
    let parsed = richview_core::document::parse(&source)
        .map_err(|errors| format!("Report preview: {errors:?}"))?;
    let chart = parsed
        .blocks
        .into_iter()
        .find(|block| block.tag == "chart")
        .ok_or("report preview needs a chart")?;
    let credit = source
        .lines()
        .find(|line| line.starts_with("*Source:"))
        .unwrap_or("");
    let source = format!(
        "---\ntitle: {}\nstyles: [{}]\n---\n\n```chart\n{}\n```\n\n{credit}\n",
        serde_json::to_string(name)?,
        style(name),
        serde_json::to_string_pretty(&chart.value)?
    );
    render(
        &source,
        &own,
        &preview.join(format!("report-{name}")),
        &[style(name)],
        flags,
        css,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preview_frame_carries_only_the_rules_marked_for_it() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let stylesheet = fs::read_to_string(root.join("assets/docs/docs.css")).unwrap();
        let css = preview_css(&stylesheet).unwrap();
        assert!(css.contains("body.docs-preview .rv-report"));
        assert!(!css.contains("body.docs-page"));
        assert!(css.len() < 2048, "{} bytes", css.len());
        // The docs script loads every preview frame in its turn (preview_frames.rs).
        let card = card("bar", "Bar", "Note", "sleek", "/docs/charts/bar");
        assert!(card.contains(" data-src=\"/docs/previews/bar/sleek.html#rv-chart-0\""));
        assert!(!card.contains(" src=") && !card.contains("loading="));
        assert!(preview_css("body {}").is_err());
    }
}
