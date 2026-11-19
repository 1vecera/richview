//! Shared-runtime previews, the gallery's pictures and semantic gallery grids for the docs visual
//! pass.
//!
//! A gallery card shows a picture of its report's first chart, drawn by this build from the same
//! source in the card's style (Daniel, 16 Nov 2026: a static picture instead of loading live reports
//! one by one, which emptied the cards whenever the reader scrolled away and back). The link opens
//! the live report. Only the elements overview's typography card is still a live frame.
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

/// The size a gallery picture is laid out at, and how much larger it is drawn.
#[derive(Clone, Copy)]
pub(super) struct Picture {
    pub(super) name: &'static str,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) scale: f32,
}

/// A gallery card: laid out 1000 px wide, as the live frames were, at the card's 1000:620.
pub(super) const CARD: Picture = Picture {
    name: "card",
    width: 1000,
    height: 620,
    scale: 1.0,
};
/// An element's card: laid out half as wide and drawn twice as large, so the element fills it.
const ELEMENT: Picture = Picture {
    name: "element",
    width: 500,
    height: 310,
    scale: 2.0,
};
/// The chart types the elements overview shows as pictures: charts, flow charts and tables.
const ELEMENT_CHARTS: [&str; 3] = ["streamgraph", "flow", "pivot"];
/// A report's larger preview on the Reports page, its 1000:750 viewport.
pub(super) const PAGE: Picture = Picture {
    name: "page",
    width: 1000,
    height: 750,
    scale: 1.0,
};

/// The address of a picture the build draws for `slug` in `style`.
pub(super) fn picture_src(slug: &str, style: &str, picture: Picture) -> String {
    format!("/docs/previews/{slug}/{style}-{}.png", picture.name)
}

/// The image of a picture, at its size so its box keeps its place while it loads. `alt` is the
/// card's own title until [`stage`] names the picture by the chart it shows.
pub(super) fn picture_img(src: &str, alt: &str, picture: Picture) -> String {
    let scaled = |n: u32| (n as f32 * picture.scale).round() as u32;
    format!(
        "<img src=\"{src}\" alt=\"{}\" width=\"{}\" height=\"{}\" loading=\"lazy\" decoding=\"async\">",
        escape(alt),
        scaled(picture.width),
        scaled(picture.height)
    )
}

fn card(slug: &str, title: &str, note: &str, style: &str, link: &str) -> String {
    format!(
        "<article class=\"docs-chart-card\"><a class=\"docs-chart-card__link\" href=\"{link}\"><div class=\"docs-thumbnail\">{}</div><h3>{}</h3><p>{}</p><p class=\"docs-chart-card__style\">{}</p></a></article>",
        picture_img(&picture_src(slug, style, CARD), title, CARD),
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
            "gapminder" => "Press Play: every country through six decades; 1960–2020.",
            "global-temperature" => "Annual change against the same NASA baseline; 1880–2025.",
            "keeling-curve" => "Drag to zoom into a seasonal cycle on a long rise in CO₂.",
            _ => {
                "Drag to zoom into 15-minute electricity prices, negative ones included; 2025–2026."
            }
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

/// The gallery's reports after the flagship: name, title and the card's note.
pub(super) const REPORT_CARDS: &[(&str, &str, &str)] = &[
    (
        "longer-lives",
        "Longer lives",
        "Gapminder · people, births, lives · 1950–2020",
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
];

/// The flagship's title, which names its larger preview on the Reports page.
pub(super) const FLAGSHIP_TITLE: &str = "The Most Expensive Race Ever Run";

pub(super) fn report_grid() -> String {
    let mut html = "<div class=\"docs-report-grid\">".to_owned();
    html.push_str(&flagship_card());
    for &(name, title, note) in REPORT_CARDS {
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

/// The elements that have no part of their own on the elements page: name, what it is for,
/// the guide and section that show it, and a still of it in the page's own type.
pub(super) const MORE_ELEMENTS: &[(&str, &str, &str, &str, &str)] = &[
    (
        "Metrics",
        "One large number, with its change.",
        "blocks",
        "evidence-in-charts-tables-and-metrics",
        "<b class=\"docs-mini__number\">450</b><span>Total orders, up 12%</span>",
    ),
    (
        "Callouts",
        "A finding or caveat beside its evidence.",
        "blocks",
        "keep-the-caveat-near-the-claim",
        "<span class=\"docs-mini__callout\"><b>A small sample.</b> The observations are fictional.</span>",
    ),
    (
        "Audio",
        "A player with its transcript.",
        "blocks",
        "audio-video-and-transcripts",
        "<span class=\"docs-mini__play\"></span><span class=\"docs-mini__wave\"><i></i><i></i><i></i><i></i><i></i><i></i><i></i><i></i><i></i><i></i></span>",
    ),
    (
        "Video",
        "Video with a poster and captions.",
        "blocks",
        "audio-video-and-transcripts",
        "<span class=\"docs-mini__poster\"><span class=\"docs-mini__play\"></span></span>",
    ),
    (
        "External embeds",
        "A page of an allowed host, sandboxed.",
        "blocks",
        "embed-an-external-page",
        "<span class=\"docs-mini__browser\"><i></i><i></i><i></i></span>",
    ),
    (
        "Scrollytelling",
        "A figure that changes as the reader scrolls.",
        "blocks",
        "explain-a-figure-as-the-reader-scrolls",
        "<span class=\"docs-mini__steps\"><i></i><i class=\"is-on\"></i><i></i></span><span class=\"docs-mini__figure\"></span>",
    ),
    (
        "Tabs",
        "Panels the reader switches between.",
        "authoring",
        "split-a-long-report-into-tabs",
        "<span class=\"docs-mini__tabs\"><b>Overview</b><span>Detail</span><span>Method</span></span>",
    ),
    (
        "Contents",
        "The report's sections, and how far the reader is.",
        "authoring",
        "contents-and-reading-progress",
        "<span class=\"docs-mini__toc\"><b>The verdict</b><span>Money and risk</span><span>Method</span></span>",
    ),
];

/// The elements without a part of their own, listed under the overview: each shows a still of
/// itself and opens the guide section that shows it live.
fn more_elements() -> String {
    let mut html =
        "<p class=\"docs-more-title\">More elements</p><div class=\"docs-more-grid\">".to_owned();
    for (title, note, guide, anchor, still) in MORE_ELEMENTS {
        html.push_str(&format!(
            "<a class=\"docs-more\" href=\"/docs/{guide}#{anchor}\"><span class=\"docs-mini\" aria-hidden=\"true\">{still}</span><span class=\"docs-more__text\"><b>{}</b><span>{}</span></span></a>",
            escape(title),
            escape(note)
        ));
    }
    html.push_str("</div>");
    html
}

/// The elements overview: one card per element, each a way in. Charts and flow charts open
/// their own pages; the others move to their part of the elements gallery. A live frame is
/// laid out half as wide as a gallery card's and shown twice as large (the box around it is
/// twice the card's width, which the docs script scales by), so an element fills its card.
pub(super) fn elements_grid() -> String {
    // Type is not a chart: its card stays the typography sample, live and scaled by the docs script.
    let frame = |title: &str, src: &str| {
        format!(
            "<div class=\"docs-thumbnail__near\"><iframe data-docs-scale tabindex=\"-1\" aria-hidden=\"true\" title=\"{title} preview\" src=\"{src}\" loading=\"lazy\" sandbox=\"allow-scripts\"></iframe></div>"
        )
    };
    let preview = |chart: &str, title: &str| {
        picture_img(&picture_src(chart, "scientific", ELEMENT), title, ELEMENT)
    };
    let picture = |file: &str, width: u32, height: u32| {
        format!(
            "<img src=\"/docs/gallery/blocks/media/{file}\" alt=\"\" width=\"{width}\" height=\"{height}\" loading=\"lazy\" decoding=\"async\">"
        )
    };
    let mut html = "<div class=\"docs-featured-grid docs-elements-grid\">".to_owned();
    for (anchor, title) in ELEMENTS {
        let (note, link, thumbnail) = match *anchor {
            "charts" => (
                "Every chart type, live with its source.",
                "/docs/charts".to_owned(),
                preview("streamgraph", title),
            ),
            "flow-charts" => (
                "Steps, decisions and lanes, placed and routed for you.",
                "/docs/charts/flow".to_owned(),
                preview("flow", title),
            ),
            "tables" => (
                "Exact values, in tables and pivot tables.",
                format!("#{anchor}"),
                preview("pivot", title),
            ),
            "expands" => (
                "Long detail one tap away, printed open.",
                format!("#{anchor}"),
                "<div class=\"docs-thumb-mock docs-thumb-mock--expand\" aria-hidden=\"true\"><p class=\"docs-thumb-mock__row is-open\">How the sample was drawn</p><p class=\"docs-thumb-mock__text\">1,200 orders drawn at random, at least 150 in each region.</p><p class=\"docs-thumb-mock__row\">What the columns mean</p><p class=\"docs-thumb-mock__row\">Known gaps in the data</p></div>".to_owned(),
            ),
            "code-blocks" => (
                "Code with its title, numbers and marked lines.",
                format!("#{anchor}"),
                "<div class=\"docs-thumb-mock docs-thumb-mock--code\" aria-hidden=\"true\"><p class=\"docs-thumb-mock__head\"><span>chart.yaml</span><span>YAML</span></p><pre><span><b>type</b>: bar</span><span class=\"is-marked\"><b>title</b>: Orders by region</span><span><b>x</b>: region</span><span><b>y</b>: amount</span></pre></div>".to_owned(),
            ),
            "columns" => (
                "Two to four blocks side by side, stacked on a phone.",
                format!("#{anchor}"),
                "<div class=\"docs-thumb-mock docs-thumb-mock--columns\" aria-hidden=\"true\"><div class=\"docs-thumb-mock__bars\"><span></span><span></span><span></span><span></span><span></span></div><div><p class=\"docs-thumb-mock__big\">1,240</p><p class=\"docs-thumb-mock__text\">orders in launch week, more than March and April together.</p></div></div>".to_owned(),
            ),
            "type-and-fonts" => (
                "Five builtin styles, each with its own type.",
                format!("#{anchor}"),
                frame(
                    "Typography",
                    "/docs/gallery/typography/Rendered/typography-sleek.html",
                ),
            ),
            "pictures" => (
                "Photographs and documents, with their credit.",
                format!("#{anchor}"),
                picture("nightingale.jpg", 1280, 700),
            ),
            _ => (
                "Audio, video, embeds, metrics and callouts.",
                format!("#{anchor}"),
                picture("apollo-poster.jpg", 640, 438),
            ),
        };
        html.push_str(&format!(
            "<article class=\"docs-chart-card\"><a class=\"docs-chart-card__link\" href=\"{link}\"><div class=\"docs-thumbnail\">{thumbnail}</div><h3>{}</h3><p>{}</p></a></article>",
            escape(title),
            escape(note)
        ));
    }
    html.push_str("</div>");
    html.push_str(&more_elements());
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

/// Write the preview page of `source` in each of `styles` into `folder`, and return each style's
/// chart payloads.
fn render(
    source: &str,
    options: &RenderOptions,
    folder: &Path,
    styles: &[&str],
    flags: BuildFlags,
    css: &str,
) -> Result<Vec<(String, Vec<serde_json::Value>)>> {
    fs::create_dir_all(folder)?;
    let source = without_reading_aids(&public::source(source, flags)?);
    let mut charts = Vec::new();
    for style in styles {
        let mut options = options.clone();
        options.styles = vec![(*style).into()];
        let report = richview_core::document::render(&source, &options)
            .map_err(|errors| format!("Preview: {errors:?}"))?;
        ensure_clean(style, &report.diagnostics)?;
        charts.push(((*style).to_owned(), report.charts));
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
    Ok(charts)
}

/// Draw the gallery pictures of a render's first chart in `style` into `folder` (FEA-SF-43): the
/// same drawing a link-preview card leads with, so a changed report changes its picture.
fn draw(
    preview: &Path,
    slug: &str,
    style: &str,
    charts: &[(String, Vec<serde_json::Value>)],
    pictures: &[Picture],
    names: &mut Names,
) -> Result<()> {
    let folder = &preview.join(slug);
    let (_, charts) = charts
        .iter()
        .find(|(name, _)| name == style)
        .ok_or_else(|| {
            format!(
                "no {style} render for a gallery picture in {}",
                folder.display()
            )
        })?;
    fs::create_dir_all(folder)?;
    for picture in pictures {
        let png = richview_share::chart_picture_png(
            style,
            charts,
            picture.width,
            picture.height,
            picture.scale,
        )?
        .ok_or_else(|| format!("{} has no chart for its gallery picture", folder.display()))?;
        fs::write(folder.join(format!("{style}-{}.png", picture.name)), png)?;
        if let Some(title) = richview_share::chart_picture_title(charts) {
            names.push((picture_src(slug, style, *picture), title));
        }
    }
    Ok(())
}

/// Each drawn picture's address and the title of the chart it shows.
type Names = Vec<(String, String)>;

/// Name every gallery picture on the docs pages by the chart it shows (lead, 16 Nov: the chart's
/// title, as Chart design asked, because the card's heading already names the report in the same
/// link). The pages are written before the pictures are drawn, so this replaces the alt text they
/// were given. A picture that no page shows is an error, so names and pictures cannot drift apart.
fn name_pictures(docs: &Path, names: &Names) -> Result<()> {
    fn pages(folder: &Path, found: &mut Vec<PathBuf>) -> Result<()> {
        for entry in fs::read_dir(folder)? {
            let path = entry?.path();
            if path.is_dir() {
                pages(&path, found)?;
            } else if path
                .extension()
                .is_some_and(|extension| extension == "html")
            {
                found.push(path);
            }
        }
        Ok(())
    }
    let mut found = Vec::new();
    pages(docs, &mut found)?;
    let mut shown = vec![false; names.len()];
    for path in found {
        let html = fs::read_to_string(&path)?;
        if !html.contains("/docs/previews/") {
            continue;
        }
        let mut named = html.clone();
        for ((src, title), shown) in names.iter().zip(&mut shown) {
            if let Some(changed) = with_alt(&named, src, title) {
                named = changed;
                *shown = true;
            }
        }
        if named != html {
            fs::write(&path, named)?;
        }
    }
    match names.iter().zip(&shown).find(|(_, shown)| !**shown) {
        Some(((src, _), _)) => Err(format!("no docs page shows the gallery picture {src}").into()),
        None => Ok(()),
    }
}

/// `html` with the alt text of every image of `src` set to `title`; `None` when it has none.
fn with_alt(html: &str, src: &str, title: &str) -> Option<String> {
    let needle = format!("<img src=\"{src}\" alt=\"");
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(at) = rest.find(&needle) {
        let start = at + needle.len();
        let end = start + rest[start..].find('"')?;
        out.push_str(&rest[..start]);
        out.push_str(&escape(title));
        rest = &rest[end..];
    }
    if out.is_empty() {
        return None;
    }
    out.push_str(rest);
    Some(out)
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
    let mut names = Names::new();
    for page in charts {
        let folder = preview.join(&page.name);
        let rendered = render(
            &example_source(page),
            &options,
            &folder,
            &chrome::site_styles(),
            flags,
            &css,
        )?;
        if ELEMENT_CHARTS.contains(&page.name.as_str()) {
            draw(
                &preview,
                &page.name,
                "scientific",
                &rendered,
                &[ELEMENT],
                &mut names,
            )?;
        }
    }
    for &(slug, _, name, _) in FEATURED {
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
        let rendered = render(
            &source,
            &own,
            &preview.join(slug),
            &chrome::site_styles(),
            flags,
            &css,
        )?;
        draw(&preview, slug, style(name), &rendered, &[CARD], &mut names)?;
    }
    // The Reports page: each report's card and larger preview are pictures of its first chart,
    // drawn from the report itself; the flagship's card is its cover.
    report_pictures(
        FLAGSHIP,
        &root.join("examples").join(FLAGSHIP).join("report.rmd"),
        &preview,
        &[PAGE],
        &mut names,
    )?;
    for &(name, _) in REPORTS
        .iter()
        .filter(|(name, _)| !["blocks", "typography"].contains(name))
    {
        report_pictures(
            name,
            &root
                .join("docs/public/gallery")
                .join(name)
                .join("report.rmd"),
            &preview,
            &[CARD, PAGE],
            &mut names,
        )?;
    }
    name_pictures(docs, &names)
}

/// A report's pictures on the Reports page: its first chart as the report renders it, with its own
/// folder, data and styles, with the card's style passed last so it wins as the live preview's did
/// (the flagship's own source asks for `scientific`; its card has always been `sleek`).
fn report_pictures(
    name: &str,
    file: &Path,
    preview: &Path,
    pictures: &[Picture],
    names: &mut Names,
) -> Result<()> {
    let source = fs::read_to_string(file)?;
    let (_, options) = richview_app::resolver::resolve_document_options_policy(
        &source,
        file,
        &[style(name).to_owned()],
        false,
        false,
    )?;
    let report = richview_core::document::render(&source, &options)
        .map_err(|errors| format!("Report picture {name}: {errors:?}"))?;
    draw(
        preview,
        &format!("report-{name}"),
        style(name),
        &[(style(name).to_owned(), report.charts)],
        pictures,
        names,
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
        assert!(preview_css("body {}").is_err());
    }

    #[test]
    fn a_gallery_card_is_a_built_picture_that_keeps_its_size_and_names_its_report() {
        let card = card("bar", "Bar & line", "Note", "sleek", "/docs/charts/bar");
        assert!(card.contains(
            "<a class=\"docs-chart-card__link\" href=\"/docs/charts/bar\"><div class=\"docs-thumbnail\"><img src=\"/docs/previews/bar/sleek-card.png\" alt=\"Bar &amp; line\" width=\"1000\" height=\"620\" loading=\"lazy\""
        ));
        assert!(!card.contains("<iframe"));
        for grid in [featured_menu(), report_grid()] {
            assert!(!grid.contains("<iframe") && !grid.contains("data-docs-preview"));
            assert!(!grid.contains("alt=\"\" width=\"1000\" height=\"620\""));
        }
        // An element is laid out half as wide and drawn twice as large; only type stays live.
        let elements = elements_grid();
        assert!(elements.contains("src=\"/docs/previews/flow/scientific-element.png\" alt=\"Flow charts\" width=\"1000\" height=\"620\""));
        assert_eq!(elements.matches("<iframe").count(), 1);
        assert!(
            elements.contains("<iframe data-docs-scale") && !elements.contains("data-docs-preview")
        );
    }

    #[test]
    fn a_picture_is_named_by_the_chart_it_shows_wherever_a_page_shows_it() {
        let src = picture_src("report-titanic", "brutalist", CARD);
        let page = format!(
            "{0}<p>between</p>{0}",
            picture_img(&src, "Passengers and crew", CARD)
        );
        let named = with_alt(&page, &src, "Who survived, by class & sex").unwrap();
        assert_eq!(
            named
                .matches("alt=\"Who survived, by class &amp; sex\"")
                .count(),
            2
        );
        assert!(!named.contains("Passengers and crew") && named.contains("<p>between</p>"));
        assert_eq!(
            with_alt(&page, &picture_src("bar", "sleek", CARD), "Bars"),
            None
        );
    }
}
