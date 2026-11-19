//! The docs: guides, a report and chart gallery, and the format
//! reference, all written as RichView reports and rendered by the report engine (FEA-SF-41,
//! FEA-SF-43, ARC-43). Every page wears the site header and footer (FEA-SF-28).
use crate::{Result, chrome};
mod previews;
mod public;
mod reference;

pub(crate) fn hide_private_repository_links(root: &std::path::Path) -> Result<()> {
    public::hide_repository_links(root)
}

#[derive(Clone, Copy, Default)]
pub struct BuildFlags {
    pub install_enabled: bool,
    pub repository_public: bool,
    pub feedback_enabled: bool,
    /// The release carries the Windows preview (`assets/install/release.json`, ARC-67).
    pub windows_preview: bool,
}

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Parser, Tag, TagEnd};
use richview_core::HighlightLanguage;
use richview_core::document::{RenderOptions, ReportFolder};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// The task guides, in the sidebar's order, which the pager follows: file stem under `docs/`,
/// label.
const GUIDES: &[(&str, &str)] = &[
    ("index", "Report gallery"),
    ("elements", "Elements gallery"),
    ("playground", "Playground guide"),
    ("install", "Install"),
    ("first-report", "Your first report"),
    ("cli", "Command line"),
    ("mcp", "MCP"),
    ("vscode", "VS Code"),
    ("styles", "Styles & your brand"),
    ("export", "Export HTML & PDF"),
    ("publishing", "Publish: walkthrough"),
    ("tools", "CLI & MCP reference"),
    ("style-yaml", "Style YAML reference"),
    ("authoring", "Rich Markdown"),
    ("blocks", "Blocks & media"),
    ("chart-reference", "Charts & interaction"),
    ("data", "Data & connectors"),
    ("storage", "Asset storage"),
    ("recovery", "Diagnostics & repair"),
    ("access", "Server, access and API"),
    ("architecture", "Architecture & performance"),
];

const FEATURED: &[(&str, &str, &str, usize)] = &[
    (
        "playfair",
        "Playfair: a sample of the 1821 chart",
        "first-charts",
        0,
    ),
    (
        "nightingale",
        "Nightingale: causes of death",
        "first-charts",
        1,
    ),
    ("anscombe", "Anscombe's quartet", "same-numbers", 0),
    (
        "old-faithful",
        "Old Faithful: eruptions and waits",
        "same-numbers",
        1,
    ),
    ("titanic", "Titanic: class, sex and survival", "titanic", 0),
    (
        "gapminder",
        "Gapminder: families and longer lives",
        "families-and-lives",
        0,
    ),
    (
        "global-temperature",
        "NASA: a warmer world",
        "air-we-changed",
        0,
    ),
    (
        "keeling-curve",
        "The Keeling curve",
        "co2-mauna-loa-monthly",
        0,
    ),
    (
        "german-electricity",
        "German electricity prices",
        "de-power-15min",
        0,
    ),
];

const REPORTS: &[(&str, &[&str])] = &[
    ("typography", &["report.rmd"]),
    (
        "blocks",
        &[
            "report.rmd",
            "media/nightingale.jpg",
            "media/apollo-poster.jpg",
            "media/apollo-step.mp3",
            "media/apollo-step.mp4",
            "media/transcript.md",
            "media/video-transcript.md",
            "media/captions.vtt",
        ],
    ),
    (
        "longer-lives",
        &[
            "report.rmd",
            "data/regions-by-decade.csv",
            "data/life-expectancy-by-region.csv",
        ],
    ),
    (
        "air-we-changed",
        &[
            "report.rmd",
            "data/global-temperature.csv",
            "media/gistemp-2025-anomaly-map.jpg",
            "media/SOURCES.md",
        ],
    ),
    (
        "same-numbers",
        &[
            "report.rmd",
            "data/anscombe.csv",
            "data/old-faithful.csv",
            "media/old-faithful-eruption.jpg",
            "media/SOURCES.md",
        ],
    ),
    (
        "titanic",
        &[
            "report.rmd",
            "data/titanic.csv",
            "media/lifeboat-6.jpg",
            "media/SOURCES.md",
        ],
    ),
    (
        "first-charts",
        &[
            "report.rmd",
            "data/selected.csv",
            "data/wheat-and-wages.csv",
            "data/nightingale.csv",
            "media/nightingale-diagram-1858.jpg",
            "media/SOURCES.md",
        ],
    ),
    (
        "families-and-lives",
        &["report.rmd", "data/families-and-lives.csv"],
    ),
];

// The frozen datasets retained from /charts/ (FEA-SF-44); one canonical CSV each.
const TIME_SERIES: [&str; 3] = [
    "co2-mauna-loa-monthly",
    "us-cpi-components",
    "de-power-15min",
];

/// The elements a report is made of, as the elements gallery overviews them: the heading's
/// anchor on that page and the label. The sidebar lists them under the gallery: charts and
/// flow charts open their own pages, the others move the reader within the page.
const ELEMENTS: &[(&str, &str)] = &[
    ("charts", "Charts"),
    ("flow-charts", "Flow charts"),
    ("tables", "Tables"),
    ("expands", "Expands"),
    ("code-blocks", "Code blocks"),
    ("columns", "Columns"),
    ("type-and-fonts", "Type and fonts"),
    ("pictures", "Pictures"),
    ("players-and-blocks", "Players and blocks"),
];

/// The chart types that are elements of their own (Daniel, 16 Nov 2026): chart type and the
/// element it belongs to. Their pages sit under that element, not in the chart gallery.
const ELEMENT_PAGES: &[(&str, &str)] = &[("flow", "flow-charts"), ("pivot", "tables")];

/// The element a chart type's page belongs to, when it is not a chart of the chart gallery.
fn element_of(chart: &str) -> Option<&'static str> {
    ELEMENT_PAGES
        .iter()
        .find(|(name, _)| *name == chart)
        .map(|(_, element)| *element)
}

/// The chart types the chart gallery lists: every type but the elements of their own.
fn gallery_charts(charts: &[ChartPage]) -> impl Iterator<Item = &ChartPage> {
    charts
        .iter()
        .filter(|page| element_of(&page.name).is_none())
}

/// The sidebar's groups of guides; the galleries and the playground have their own entries.
const HOW_TO: &[&str] = &[
    "install",
    "first-report",
    "cli",
    "mcp",
    "vscode",
    "styles",
    "export",
    "publishing",
];
const REFERENCES: &[&str] = &["tools", "style-yaml"];
const NITTY_GRITTY: &[&str] = &[
    "authoring",
    "blocks",
    "chart-reference",
    "data",
    "storage",
    "recovery",
    "access",
    "architecture",
];

/// The URL of a guide: the index is the docs root, the rest are clean URLs.
fn guide_url(stem: &str) -> String {
    if stem == "index" {
        "/docs/".into()
    } else {
        format!("/docs/{stem}")
    }
}

/// Where a docs page sits, for the sidebar's current marker and its chart-type list.
#[derive(Clone, Copy, PartialEq)]
enum Place<'a> {
    Guide(&'a str),
    Gallery,
    Featured(&'a str),
    ChartType(&'a str),
    Example(&'a str),
    Report(&'a str),
    /// The format reference, one long page, and its chart field pages.
    Reference,
}

/// One chart type's page, read from `docs/public/charts/<type>.md`.
struct ChartPage {
    name: String,
    title: String,
    source: String,
    /// The one sentence on when to use the type: the first paragraph after the frontmatter.
    use_when: String,
    /// The live example's `chart` fence, repeated on the gallery index as a small chart.
    chart_fence: String,
}

pub fn stage(root: &Path, public: &Path, flags: BuildFlags) -> Result<()> {
    let revision = "main";
    let docs = public.join("docs");
    // Pages of an older build (the single gallery page, the raw spec) must not linger.
    if docs.exists() {
        fs::remove_dir_all(&docs)?;
    }
    fs::create_dir_all(&docs)?;
    fs::copy(root.join("assets/docs/docs.css"), docs.join("docs.css"))?;
    let charts = read_chart_pages(root)?;
    for &(stem, _) in GUIDES {
        let mut source = fs::read_to_string(root.join(format!("docs/public/{stem}.md")))?;
        source = source
            .replace("{{native-install}}", &native_install(flags))
            .replace("{{windows-install}}", &windows_install(flags))
            .replace(
                "{{agent-prompt}}",
                chrome::agent_prompt(flags.install_enabled),
            );
        source = source.replace("{{make-own}}", if flags.install_enabled { "[Install RichView](https://richview.ai/docs/install), then [make your first report](https://richview.ai/docs/first-report). Your agent writes the source; you review the story." } else { "[Try the playground](https://richview.ai/playground) now. The hosted native installer is not available yet." });
        richview_app::reference::validate_cli_examples(&source)?;
        let mut source = if stem == "tools" {
            richview_app::reference::tools_guide(&source)?
        } else if stem == "style-yaml" {
            source.replace("<!-- richview:style-reference -->", &style_reference())
        } else if stem == "recovery" {
            source.replace(
                "<!-- richview:diagnostic-reference -->",
                &richview_core::spec::fragment("codes").ok_or("missing diagnostic reference")?,
            )
        } else {
            source
        };
        let html = if stem == "elements" {
            let tour = fs::read_to_string(report_source_folder(root, "blocks").join("report.rmd"))?;
            let (_, body) = tour
                .strip_prefix("---\n")
                .and_then(|text| text.split_once("\n---\n"))
                .ok_or("block tour needs frontmatter")?;
            // The tour reads as one part of the page: its sections sit under "Players and
            // blocks", so the page's headings stay the elements.
            source.push_str(
                &body
                    .replace("media/", "gallery/blocks/media/")
                    .replace("\n## ", "\n### "),
            );
            // Each element shows the live example of its own page.
            for page in &charts {
                source =
                    source.replace(&format!("{{{{example:{}}}}}", page.name), &page.chart_fence);
            }
            let mut options = documentation_options();
            let file = report_source_folder(root, "blocks").join("report.rmd");
            let (_, media) = richview_app::resolver::resolve_document_options_policy(
                &tour,
                &file,
                &[],
                false,
                false,
            )?;
            for (key, value) in media.data {
                let key = format!("gallery/blocks/{key}");
                options
                    .report_folder
                    .as_mut()
                    .unwrap()
                    .files
                    .push(key.clone());
                options.data.insert(key, value);
            }
            render_page_with_options(&source, stem, Place::Guide(stem), &charts, &options, flags)?
        } else {
            render_page(&source, stem, Place::Guide(stem), &charts, flags)?
        };
        fs::write(docs.join(format!("{stem}.html")), html)?;
        fs::write(docs.join(format!("{stem}.md")), source)?;
    }
    reference::stage(root, &docs, &charts, flags)?;
    stage_gallery(root, public, &docs, &charts, flags)?;
    stage_time_series(root, &docs, &charts, flags)?;
    let sql = docs.join("examples/sql");
    for name in [
        "report.rmd",
        "report.connectors.yaml",
        "data/seed.sql",
        "queries/orders-by-month.sql",
    ] {
        let output = sql.join(name);
        fs::create_dir_all(output.parent().unwrap())?;
        fs::copy(root.join("examples/sql-report").join(name), output)?;
    }
    stage_first_report(root, public, &docs)?;
    stage_report_gallery(root, public, &docs, &charts, flags)?;
    previews::stage(root, &docs, public, &charts, flags)?;
    stage_report_headers(public)?;
    public::finish(&docs, flags, revision)?;
    reference::search_index(&docs)?;
    check_runtime_budget(&docs)?;
    Ok(())
}

/// Standalone exports enforce their own report CSP, as the tutorial export does.
fn stage_report_headers(public: &Path) -> Result<()> {
    let path = public.join("_headers");
    let mut headers = fs::read_to_string(&path)?;
    let marker = "\n# Standalone documentation gallery exports\n";
    if let Some(start) = headers.find(marker) {
        headers.truncate(start);
    }
    let policy = headers
        .split("\n/docs/*\n")
        .nth(1)
        .and_then(|section| {
            section
                .lines()
                .find(|line| line.trim_start().starts_with("Content-Security-Policy:"))
        })
        .ok_or("documentation policy is missing")?
        .to_owned();
    let frames = "frame-src 'self' blob: data: about:";
    if policy.matches(frames).count() != 1 {
        return Err("documentation frame policy is missing or repeated".into());
    }
    headers.push_str(marker);
    for path in ["/docs/elements", "/docs/elements.html"] {
        headers.push_str(&format!(
            "{path}\n  ! Content-Security-Policy\n{}\n\n",
            policy.replace(
                frames,
                &format!("media-src 'self' data:; {frames} https://www.youtube-nocookie.com")
            )
        ));
    }
    headers.push_str("/docs/previews/*\n  ! Content-Security-Policy\n  Content-Security-Policy: base-uri 'none'; frame-ancestors 'self'\n\n/docs/runtime/*\n  Access-Control-Allow-Origin: null\n  Access-Control-Allow-Credentials: true\n\n/landing/runtime/*\n  Access-Control-Allow-Origin: *\n\n");
    for &(name, _) in REPORTS {
        headers.push_str(&format!("/docs/gallery/{name}/Rendered/*\n  ! Content-Security-Policy\n  Content-Security-Policy: base-uri 'none'; frame-ancestors 'self'\n\n"));
    }
    fs::write(path, headers)?;
    Ok(())
}

fn native_install(flags: BuildFlags) -> String {
    if !flags.install_enabled {
        let source = if flags.repository_public {
            "Or [build the command from source](https://richview.ai/docs/install#build-from-source). "
        } else {
            ""
        };
        return format!(
            "The native release is not available yet. Try the [playground](https://richview.ai/playground) now. {source}The walkthrough below is for an installed native command."
        );
    }
    "Install the latest [GitHub release](https://github.com/1vecera/richview/releases/latest):\n\n```sh\ncurl -fsSL https://github.com/1vecera/richview/releases/latest/download/install.sh | sh\nexport PATH=\"$HOME/.richview/bin:$PATH\"\nrichview --help\n```\n\nThe installer verifies the archive checksum and does not change your PATH. The PATH change applies to this shell; add the same line to your shell profile to keep it.".to_string()
}

/// The PowerShell install command, only once the release it downloads exists; the guide names it
/// inside its Windows section, which the release's Windows switch keeps or drops (ARC-67).
fn windows_install(flags: BuildFlags) -> String {
    if !flags.install_enabled {
        return "The native release is not available yet.".to_string();
    }
    "```powershell\nirm https://github.com/1vecera/richview/releases/latest/download/install.ps1 | iex\n```".to_string()
}

// Keep the downloadable block tour beside the other runnable examples: documentation
// snippets use its real resources through the existing example-context contract.
fn report_source_folder(root: &Path, name: &str) -> PathBuf {
    if name == "blocks" {
        root.join("examples/elements")
    } else {
        root.join("docs/public/gallery").join(name)
    }
}

/// Stage a fixed allowlist of reports and their local resources, without copying scratch files.
fn stage_report_gallery(
    root: &Path,
    public: &Path,
    docs: &Path,
    charts: &[ChartPage],
    flags: BuildFlags,
) -> Result<()> {
    for &(name, files) in REPORTS {
        let folder = docs.join("gallery").join(name);
        for &file in files {
            let target = folder.join(file);
            fs::create_dir_all(target.parent().ok_or("gallery file has no parent")?)?;
            fs::copy(report_source_folder(root, name).join(file), target)?;
        }
        let file = folder.join("report.rmd");
        let source = fs::read_to_string(&file)?;
        let (_, options) = crate::resolve_document_options(&source, &file, &[], public)?;
        let rendered = richview_core::document::render(&source, &options)
            .map_err(|errors| format!("Gallery {name} failed: {errors:?}"))?;
        ensure_clean(name, &rendered.diagnostics)?;
        richview_app::resolver::export_report(&file, None, &rendered, &options)?;
        if name == "typography" {
            for style in chrome::site_styles() {
                let mut options = options.clone();
                options.styles = vec![(*style).into()];
                let sample = richview_core::document::render(&source, &options)
                    .map_err(|errors| format!("Typography {style} failed: {errors:?}"))?;
                ensure_clean(style, &sample.diagnostics)?;
                richview_app::resolver::export_report(
                    &file,
                    Some(Path::new(&format!("typography-{style}.html"))),
                    &sample,
                    &options,
                )?;
            }
        }
        let title = front_value(&source, "title").ok_or("gallery report needs a title")?;
        let description =
            front_value(&source, "description").ok_or("gallery report needs a description")?;
        let wrapper = format!(
            "---\ntitle: {title}\ndescription: {description}\nstyles: [scientific]\n---\n\n{{{{report:{name}}}}}\n\n## Take it apart\n\nDownload the [Rich Markdown source](https://richview.ai/docs/gallery/{name}/report.rmd) and the data below into one folder. Each report keeps its own observations and source notes.\n"
        );
        let mut wrapper = wrapper;
        wrapper.push_str("\n## Credits\n\n");
        for line in source
            .lines()
            .filter(|line| line.starts_with("*Source:") || line.contains("NASA audio collection"))
        {
            wrapper.push_str(line);
            wrapper.push_str("\n\n");
        }
        let document = richview_core::document::parse(&source)
            .map_err(|errors| format!("Gallery credits: {errors:?}"))?;
        let credit = &document.frontmatter["title_image"]["credit"];
        if let (Some(author), Some(url), Some(page)) = (
            credit["author"].as_str(),
            credit["author_url"].as_str(),
            credit["source_url"].as_str(),
        ) {
            wrapper.push_str(&format!(
                "Photograph: [{author}]({url}), [Unsplash photo and credit]({page}).\n\n"
            ));
        }
        for file in files.iter().filter(|file| **file != "report.rmd") {
            wrapper.push_str(&format!(
                "\n- [{file}](https://richview.ai/docs/gallery/{name}/{file})\n"
            ));
        }
        let html = render_page(&wrapper, name, Place::Report(name), charts, flags)?;
        let byline = front_value(&source, "author").unwrap_or_else(|| EXAMPLE_BYLINE.to_owned());
        let html = own_preview(
            &html,
            &folder.join("share.png"),
            &format!("/docs/gallery/{name}/share.png"),
            &title,
            &byline,
            previews::style(name),
            &rendered.charts,
        )?;
        fs::write(folder.join("index.html"), html)?;
    }
    Ok(())
}

/// Every style leaf comes from the same closed grammar the loader validates.
fn style_reference() -> String {
    let mut out = String::new();
    for (mark, heading) in [
        (richview_core::StyleMark::Safe, "Settings"),
        (
            richview_core::StyleMark::Legacy,
            "Legacy leaves: still applied, each with a `style.legacy` warning naming its replacement",
        ),
    ] {
        out.push_str(&format!(
            "\n## {heading}\n\n| Path | Accepted value |\n| --- | --- |\n"
        ));
        for leaf in richview_core::style_vocabulary()
            .iter()
            .filter(|leaf| leaf.mark == mark)
        {
            out.push_str(&format!(
                "| `{}` | {} — {} |\n",
                leaf.path,
                grammar_label(leaf.grammar).replace('|', "\\|"),
                leaf.about
            ));
        }
    }
    out
}

fn grammar_label(grammar: richview_core::StyleGrammar) -> String {
    use richview_core::StyleGrammar as Grammar;
    match grammar {
        Grammar::Colour => "CSS colour, preferably OKLCH".into(),
        Grammar::Font => "Font family stack".into(),
        Grammar::Dimension => "Nonnegative number or length".into(),
        Grammar::Shadow => "`none` or x, y, blur and colour".into(),
        Grammar::Number(min, max) => format!("Number from {min} to {max}"),
        Grammar::Boolean => "`true` or `false`".into(),
        Grammar::Enum(values) => values
            .iter()
            .map(|value| format!("`{value}`"))
            .collect::<Vec<_>>()
            .join(", "),
        Grammar::Weight => "Font weight, 1–1000 or a weight name".into(),
        Grammar::LetterSpacing => "`normal` or signed length".into(),
        Grammar::Palette => "Builtin palette or 1–8 colours".into(),
        Grammar::Colours(count) => format!("Exactly {count} colours"),
        Grammar::FontSetting => "Bundled font stack or `{family, file}` with local WOFF2".into(),
        Grammar::Builtin => "Builtin style name".into(),
        Grammar::BrandColour => "Opaque colour".into(),
        Grammar::BrandName => "Non-empty company name".into(),
        Grammar::BrandLogo => "Report-relative image file".into(),
        Grammar::Step => "`xsmall`, `small`, `medium`, `large` or `xlarge`".into(),
        Grammar::Preset => "Preset name".into(),
    }
}

/// Each report on the Reports page: a picture of its first chart, drawn by the build from the
/// report, which opens the live report like the button under it. The two block tours embed their
/// complete export, which no picture can stand for.
fn report_frames(html: &str) -> String {
    let mut html = html.to_owned();
    let picture = |name: &str, title: &str, live: &str| {
        format!(
            "<div class=\"docs-report-preview__viewport\"><a href=\"{live}\">{}</a></div>",
            previews::picture_img(
                &previews::picture_src(
                    &format!("report-{name}"),
                    previews::style(name),
                    previews::PAGE
                ),
                title,
                previews::PAGE
            )
        )
    };
    // The flagship opens on its showcase page, where the reader can switch its style.
    let url = previews::FLAGSHIP_URL;
    html = html.replace(
        &format!("<p>{{{{report:{}}}}}</p>", previews::FLAGSHIP),
        &format!(
            "<div class=\"docs-report-preview\">{}<div class=\"docs-actions\"><a class=\"docs-button\" href=\"{url}\">Open live report</a><a class=\"docs-button docs-button--secondary\" href=\"{url}report.rmd\">Rich Markdown source</a><a href=\"{url}ledger/sources.csv\">Sources</a></div></div>",
            picture(previews::FLAGSHIP, previews::FLAGSHIP_TITLE, url)
        ),
    );
    for &(name, _) in REPORTS {
        let marker = format!("<p>{{{{report:{name}}}}}</p>");
        let url = format!("/docs/gallery/{name}");
        let live = format!("{url}/Rendered/report.html");
        let preview = if matches!(name, "blocks" | "typography") {
            format!(
                "<iframe loading=\"lazy\" title=\"Live report: {name}\" src=\"{live}\" sandbox=\"allow-scripts allow-downloads\"></iframe>"
            )
        } else {
            let title = previews::REPORT_CARDS
                .iter()
                .find(|(card, _, _)| *card == name)
                .map_or(name, |(_, title, _)| title);
            picture(name, title, &live)
        };
        let frame = format!(
            "<div class=\"docs-report-preview\">{preview}<div class=\"docs-actions\"><a class=\"docs-button\" href=\"{live}\">Open live report</a><a class=\"docs-button docs-button--secondary\" href=\"{url}/report.rmd\">Rich Markdown source</a><a href=\"{url}/\">Data &amp; credits</a></div></div>"
        );
        html = html.replace(&marker, &frame);
    }
    html
}

/// The elements page's type sample: one live typography report and a row of the builtin styles
/// that swaps it, as a chart page's style row does. The page so starts one sample's runtime rather
/// than one per style (`RUNTIME_BUDGET`). Without scripts each style link opens its sample.
fn typography_switch() -> String {
    let sample = |style: &str| format!("/docs/gallery/typography/Rendered/typography-{style}.html");
    let first = "sleek";
    let links: String = chrome::site_styles()
        .iter()
        .map(|style| {
            let current = if *style == first {
                " aria-current=\"true\""
            } else {
                ""
            };
            format!(
                "<a href=\"{src}\" data-preview-src=\"{src}\" data-style-link=\"{style}\"{current}>{}</a>",
                chrome::style_label(style),
                src = sample(style)
            )
        })
        .collect();
    format!(
        "<div class=\"docs-styles docs-styles--type\"><p class=\"docs-styles__label\">The same sample in each style</p><nav class=\"docs-styles__links\" aria-label=\"Style\">{links}</nav><iframe class=\"docs-styles__frame\" title=\"Typography and elements in the chosen style\" src=\"{src}\" loading=\"lazy\" sandbox=\"allow-scripts allow-downloads\"></iframe><a class=\"docs-styles__open\" data-style-open href=\"{src}\">Open the sample</a></div>",
        src = sample(first)
    )
}

/// The most RichView runtimes a docs page may start when it loads: its own and one for each live
/// frame. iOS gives a page's process three fast Wasm memories; each instance past them costs a full
/// garbage collection, a slower memory and its module compiled again, and on an iPhone the gallery
/// stopped loading at five.
const RUNTIME_BUDGET: usize = 3;

/// How many RichView runtimes a page starts when it loads: the report runtime it loads itself, and
/// one for each frame that opens one of the site's own pages. A frame without an address (a style
/// row's until a style is picked) starts none, and another site's frame runs no RichView runtime.
fn runtimes_started(html: &str) -> usize {
    let own = usize::from(html.contains("src=\"/richview_runtime.js\""));
    let frames = html
        .split("<iframe")
        .skip(1)
        .filter(|rest| {
            let tag = &rest[..rest.find('>').unwrap_or(rest.len())];
            tag.split_once(" src=\"")
                .is_some_and(|(_, address)| address.starts_with('/'))
        })
        .count();
    own + frames
}

/// Fails the build when a docs page would start more runtimes than `RUNTIME_BUDGET`.
fn check_runtime_budget(folder: &Path) -> Result<()> {
    for entry in fs::read_dir(folder)? {
        let path = entry?.path();
        if path.is_dir() {
            check_runtime_budget(&path)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension == "html")
        {
            let html = fs::read_to_string(&path)?;
            let started = runtimes_started(&html);
            if html.contains("data-richview-page=\"docs\"") && started > RUNTIME_BUDGET {
                return Err(format!(
                    "{} starts {started} RichView runtimes when it loads; a docs page may start at most {RUNTIME_BUDGET}, as many as a phone runs at full speed",
                    path.display()
                )
                .into());
            }
        }
    }
    Ok(())
}

fn feedback(place: Place, flags: BuildFlags, name: &str) -> String {
    let path = match place {
        Place::Guide(stem) => guide_url(stem),
        Place::Gallery => "/docs/charts".into(),
        Place::Featured(name) | Place::ChartType(name) => format!("/docs/charts/{name}"),
        Place::Example(name) => format!("/docs/examples/{name}/"),
        Place::Report(name) => format!("/docs/gallery/{name}/"),
        Place::Reference if name == "reference" => "/docs/reference".into(),
        Place::Reference => format!("/docs/reference/{name}"),
    };
    let issue = if flags.repository_public {
        format!(
            "<p><a data-feedback-issue data-feedback-issue-base=\"https://github.com/1vecera/richview/issues/new?title=Documentation%20feedback\" href=\"https://github.com/1vecera/richview/issues/new?title=Documentation%20feedback&amp;body=Page%3A%20https%3A%2F%2Frichview.ai{}\" target=\"_blank\" rel=\"noopener noreferrer\">Open an issue draft</a>; review it before posting. Your email is excluded.</p>",
            path.replace('/', "%2F")
        )
    } else {
        String::new()
    };
    let (enabled, submit, status) = if flags.feedback_enabled {
        (
            " data-feedback-enabled",
            "<button type=\"submit\" class=\"docs-button\">Send feedback</button>",
            "Your note stays here until receipt is confirmed.",
        )
    } else {
        (
            "",
            "",
            "Feedback collection is not connected. Write a note and download it to keep locally.",
        )
    };
    format!(
        "<details class=\"docs-feedback\" id=\"feedback\"><summary>Was this page useful? Suggest an edit</summary><form class=\"docs-feedback__form\" data-docs-feedback{enabled} hidden><input type=\"hidden\" data-feedback-page value=\"{}\"><label>Your feedback<textarea data-feedback-message required minlength=\"5\" maxlength=\"2000\" rows=\"3\"></textarea></label><label>Email for a reply (optional)<input data-feedback-email type=\"email\" maxlength=\"254\" autocomplete=\"email\"></label><div class=\"docs-feedback__trap\" aria-hidden=\"true\"><input data-feedback-website tabindex=\"-1\" autocomplete=\"off\"></div><div class=\"docs-actions\">{submit}<a data-feedback-download download=\"richview-feedback.json\" hidden>Download your note</a></div><p data-feedback-status role=\"status\">{status}</p></form><noscript><p>JavaScript is off. <a download=\"richview-feedback.txt\" href=\"data:text/plain;charset=utf-8,Page%3A%20https%3A%2F%2Frichview.ai{}%0A%0AYour%20feedback%3A%20\">Download a feedback template</a>, add your note and keep it locally.</p></noscript>{issue}</details>",
        escape(&path),
        path.replace('/', "%2F")
    )
}

/// The chart types the release run has accepted (FEA-CH-75), one name a line in
/// `docs/public/charts/accepted.txt`. The public gallery, its pages and its counts list only
/// those; without the file every type the engine lists is public, so a checkout that has no
/// release run yet still builds the whole gallery.
pub fn accepted_chart_types(root: &Path) -> Option<Vec<String>> {
    let text = fs::read_to_string(root.join("docs/public/charts/accepted.txt")).ok()?;
    Some(
        text.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(str::to_owned)
            .collect(),
    )
}

/// Read every chart type's page. A supported type without one is reported here and fails the
/// site gate (FEA-BR-22, FEA-SF-62); the build stays usable for streams that add types.
fn read_chart_pages(root: &Path) -> Result<Vec<ChartPage>> {
    let mut pages = Vec::new();
    let accepted = accepted_chart_types(root);
    for name in richview_core::CHART_TYPES {
        if accepted
            .as_ref()
            .is_some_and(|list| !list.contains(&name.to_string()))
        {
            continue;
        }
        let file = root.join(format!("docs/public/charts/{name}.md"));
        let Ok(source) = fs::read_to_string(&file) else {
            println!(
                "warning: chart type `{name}` has no docs page: add docs/public/charts/{name}.md (FEA-SF-43)"
            );
            continue;
        };
        let title = front_value(&source, "title").unwrap_or_else(|| (*name).to_string());
        let body = source
            .strip_prefix("---\n")
            .and_then(|rest| rest.split_once("\n---\n"))
            .map_or(source.as_str(), |(_, body)| body);
        let use_when = body
            .split("\n\n")
            .map(str::trim)
            .find(|paragraph| !paragraph.is_empty() && !paragraph.starts_with("```"))
            .ok_or_else(|| format!("docs/public/charts/{name}.md needs an opening sentence"))?
            .to_string();
        let chart_fence = body
            .find("```chart")
            .and_then(|start| {
                let rest = &body[start..];
                rest[3..].find("```").map(|end| rest[..end + 6].to_string())
            })
            .ok_or_else(|| format!("docs/public/charts/{name}.md needs a chart example"))?;
        pages.push(ChartPage {
            name: (*name).to_string(),
            title,
            source,
            use_when,
            chart_fence,
        });
    }
    Ok(pages)
}

/// The spec points at neighbouring files of the repository; on the site those links lead
/// nowhere, so they keep their text and lose the link.
fn unlink_repository_paths(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len());
    let mut rest = markdown;
    while let Some(open) = rest.find("](") {
        let after = &rest[open + 2..];
        let Some(close) = after.find(')') else { break };
        let target = &after[..close];
        let local = !(target.starts_with("http")
            || target.starts_with('#')
            || target.starts_with("mailto:"));
        if local && let Some(text_start) = rest[..open].rfind('[') {
            out.push_str(&rest[..text_start]);
            out.push_str(&rest[text_start + 1..open]);
        } else {
            out.push_str(&rest[..open + 2 + close + 1]);
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

fn front_value(source: &str, key: &str) -> Option<String> {
    let front = source.strip_prefix("---\n")?.split_once("\n---\n")?.0;
    front.lines().find_map(|line| {
        line.strip_prefix(key)
            .and_then(|rest| rest.strip_prefix(':'))
            .map(|value| value.trim().trim_matches('"').to_string())
    })
}

/// A compact featured menu, the scrolling chart types and each example's own page.
/// The chart the gallery opens with (Daniel, 15 Nov: one big example, the most complex that is
/// still meaningful, showing off the interaction): a chart of the flagship report, found by its
/// title, so a rewrite of the report that drops it fails the build instead of leaving a gap.
const SHOWCASE_TITLE: &str =
    "Developer talk flipped to Anthropic in mid-2025; the wider public barely moved";

/// The showcase as Rich Markdown: the flagship chart at the gallery's full width, live like every
/// chart on the page, with its source note, what to try and the way to the whole report.
fn showcase(root: &Path) -> Result<String> {
    let source = fs::read_to_string(root.join("examples/openai-vs-anthropic/report.rmd"))?;
    let document = richview_core::document::parse(&source)
        .map_err(|errors| format!("Flagship report: {errors:?}"))?;
    let mut chart = document
        .blocks
        .into_iter()
        .filter(|block| block.tag == "chart")
        .find(|block| block.value["title"].as_str() == Some(SHOWCASE_TITLE))
        .ok_or("the gallery showcase chart is no longer in the flagship report")?
        .value;
    chart["wide"] = true.into();
    // The chart's source note, its links to the report's own files made absolute: the flagship
    // publishes its folder beside the report.
    let note = source
        .split(SHOWCASE_TITLE)
        .nth(1)
        .and_then(|after| after.split_once("\n```\n"))
        .and_then(|(_, rest)| rest.trim_start().lines().next())
        .filter(|line| line.starts_with("*Source"))
        .unwrap_or_default();
    let mut note_parts = note.split("](");
    let mut credited = note_parts.next().unwrap_or_default().to_owned();
    for part in note_parts {
        credited.push_str("](");
        if !part.starts_with("http") && !part.starts_with('#') && !part.starts_with('/') {
            credited.push_str("https://richview.ai/showcase/openai-vs-anthropic/");
        }
        credited.push_str(part);
    }
    let note = credited;
    // The report's own title and description introduce it, so the slot follows the report.
    let front = |key: &str| {
        front_value(&source, key)
            .map(|value| value.trim_matches('\'').to_owned())
            .ok_or(format!("the flagship report needs a {key}"))
    };
    let (title, description) = (front("title")?, front("description")?);
    Ok(format!(
        "\n## {title}\n\n{description} [Read the flagship report](https://richview.ai/showcase/openai-vs-anthropic/).\n\n```chart\n{}\n```\n\n{note}\n\nPoint at a month to read every population at once, drag across the plot to zoom into a stretch, and choose a name in the legend to hide its line.\n",
        serde_json::to_string_pretty(&chart)?
    ))
}

/// How many countries the gallery's live play chart keeps. All 194 on one page weigh more than
/// every other chart of the gallery together, and the gallery once overwhelmed a phone, so the
/// page plays the most populous and links to the full chart.
const PLAY_COUNTRIES: usize = 30;

/// The line above the gallery's zoom chart: the gesture, the way back and the phone equivalent
/// (Daniel, 15 Nov: the gallery should show off the interaction features). Inline, touch
/// scrolls the page, so a phone zooms in the expanded view (FEA-CH-49).
const ZOOM_CUE: &str = "Drag across the plot to zoom, and double-click to go back. On a phone, expand it first, then pinch.";

/// What marks a paragraph of the gallery as a cue; the page gives it the cue's class.
const CUE: &str = "{{cue}}";

/// The line above the gallery's play chart, naming the years its data really has.
fn play_cue(first: &str, last: &str) -> String {
    format!(
        "Press Play to run {first} to {last}, or drag the year. Hover or tap a country to read its values."
    )
}

/// The countries the gallery's play chart keeps.
struct PlayData {
    /// Their records, unchanged and in the file's order.
    csv: String,
    first: String,
    last: String,
    /// How many countries the whole file has.
    all: usize,
}

/// The year of a `year,country,…` record.
fn year(line: &str) -> &str {
    line.split_once(',').map_or("", |(year, _)| year)
}

/// The second field of a CSV record, without its quotes.
fn second_field(line: &str) -> Option<&str> {
    let rest = line.split_once(',')?.1;
    match rest.strip_prefix('"') {
        Some(quoted) => quoted.split_once('"').map(|(name, _)| name),
        None => rest.split_once(',').map(|(name, _)| name),
    }
}

/// The records of the `count` countries with the largest population in the last year of a
/// `year,country,…,population` CSV.
fn most_populous(csv: &str, count: usize) -> Result<PlayData> {
    let mut lines = csv.lines();
    let header = lines.next().unwrap_or_default();
    if !(header.starts_with("year,country,") && header.ends_with(",population")) {
        return Err(
            "the play chart's data needs year, country and a last population column".into(),
        );
    }
    let rows: Vec<&str> = lines.filter(|line| !line.is_empty()).collect();
    let years = || rows.iter().map(|line| year(line));
    let (first, last) = years()
        .min()
        .zip(years().max())
        .ok_or("the play chart's data has no rows")?;
    let mut ranked = Vec::new();
    for line in rows.iter().filter(|line| year(line) == last) {
        let population: f64 = line
            .rsplit_once(',')
            .and_then(|(_, value)| value.parse().ok())
            .ok_or_else(|| format!("no population in `{line}`"))?;
        let country = second_field(line).ok_or_else(|| format!("no country in `{line}`"))?;
        ranked.push((population, country));
    }
    let all = ranked.len();
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(b.1)));
    if all < count {
        return Err(format!("the play chart's data has only {all} countries").into());
    }
    let keep: std::collections::BTreeSet<&str> = ranked
        .iter()
        .take(count)
        .map(|(_, country)| *country)
        .collect();
    let mut subset = format!("{header}\n");
    for line in rows {
        if second_field(line).is_some_and(|country| keep.contains(country)) {
            subset.push_str(line);
            subset.push('\n');
        }
    }
    Ok(PlayData {
        csv: subset,
        first: first.to_owned(),
        last: last.to_owned(),
        all,
    })
}

/// The first chart of a report and the source note under it.
fn first_chart(root: &Path, report: &str) -> Result<(serde_json::Value, String)> {
    let source = fs::read_to_string(root.join(report))?;
    let chart = richview_core::document::parse(&source)
        .map_err(|errors| format!("{report}: {errors:?}"))?
        .blocks
        .into_iter()
        .find(|block| block.tag == "chart")
        .ok_or_else(|| format!("{report} has no chart"))?
        .value;
    let note = source
        .split("```chart")
        .nth(1)
        .and_then(|part| part.split_once("```"))
        .and_then(|(_, after)| after.trim_start().lines().next())
        .filter(|line| line.starts_with("*Source:"))
        .ok_or_else(|| format!("{report}: its chart needs a source note"))?;
    Ok((chart, note.to_owned()))
}

/// "Try it yourself" as Rich Markdown: a chart that plays and a chart that zooms, live under the
/// showcase, each under a line that names the gesture, so a visitor finds the interactions before
/// the menu (Daniel, 15 Nov). The play chart opens on its last year and waits for Play; the link
/// to every country stands under it. Their observations are registered in `options`.
fn interactions(root: &Path, options: &mut RenderOptions) -> Result<String> {
    let mut register = |key: &str, data: String| {
        if let Some(folder) = options.report_folder.as_mut() {
            folder.files.push(key.to_owned());
        }
        options.data.insert(key.to_owned(), data);
    };
    let (mut play, play_note) =
        first_chart(root, "docs/public/gallery/families-and-lives/report.rmd")?;
    let observations = fs::read_to_string(
        root.join("docs/public/gallery/families-and-lives/data/families-and-lives.csv"),
    )?;
    let countries = most_populous(&observations, PLAY_COUNTRIES)?;
    let key = "gallery/families-and-lives/most-populous.csv";
    play["data"]["csv"] = key.into();
    play["wide"] = true.into();
    // The example's own page is 4:3; at the gallery's width that is taller than a window, and
    // 16:9 leaves a phone too flat a plot (Chart design, 16 Nov).
    play["style"]["ratio"] = "3:2".into();
    play["title"] = "Press Play: six decades of smaller families and longer lives".into();
    play["subtitle"] = format!(
        "The {PLAY_COUNTRIES} most populous countries of {}, every year; bubble area represents population",
        countries.last
    )
    .into();
    let cue = play_cue(&countries.first, &countries.last);
    let all = countries.all;
    register(key, countries.csv);

    let (mut zoom, zoom_note) = first_chart(
        root,
        "docs/public/examples/co2-mauna-loa-monthly/report.rmd",
    )?;
    let key = "examples/co2-mauna-loa-monthly/data.csv";
    zoom["data"]["csv"] = key.into();
    zoom["wide"] = true.into();
    // The reader hovers and zooms for values here; on a phone the automatic end values crowd.
    zoom["auto_annotations"] = false.into();
    zoom["title"] = "Drag to zoom: a seasonal cycle on a long rise in CO₂".into();
    let monthly =
        fs::read_to_string(root.join("benchmarks/fixtures/time-series/co2-mauna-loa-monthly.csv"))?
            .lines()
            .skip_while(|line| line.starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
    register(key, monthly);

    Ok(format!(
        "\n## Try it yourself\n\nTwo charts to play with before you browse.\n\n{CUE}{cue}\n\n```chart\n{}\n```\n\n[See all {all} countries play](https://richview.ai/docs/charts/gapminder).\n\n{play_note}\n\n{CUE}{ZOOM_CUE}\n\n```chart\n{}\n```\n\n[Zoom from a year down to one day of German electricity prices](https://richview.ai/docs/charts/german-electricity).\n\n{zoom_note}\n",
        serde_json::to_string_pretty(&play)?,
        serde_json::to_string_pretty(&zoom)?
    ))
}

fn stage_gallery(
    root: &Path,
    public: &Path,
    docs: &Path,
    charts: &[ChartPage],
    flags: BuildFlags,
) -> Result<()> {
    let mut index = fs::read_to_string(root.join("docs/public/charts.md"))?;
    let featured_at = index
        .find("\n## Featured charts")
        .ok_or("docs/public/charts.md needs a Featured charts section")?;
    let mut gallery_options = documentation_options();
    let opening = showcase(root)? + &interactions(root, &mut gallery_options)?;
    index.insert_str(featured_at, &opening);
    index.push('\n');
    let folder = docs.join("charts");
    fs::create_dir_all(&folder)?;
    index.push_str("\n{{featured-menu}}\n");
    for &(slug, label, name, chart_index) in FEATURED {
        let mut options = documentation_options();
        let temporal = TIME_SERIES.contains(&name);
        let location = if temporal { "examples" } else { "gallery" };
        let source =
            fs::read_to_string(root.join(format!("docs/public/{location}/{name}/report.rmd")))?;
        let document = richview_core::document::parse(&source)
            .map_err(|errors| format!("Famous chart {name}: {errors:?}"))?;
        let mut chart = document
            .blocks
            .into_iter()
            .filter(|block| block.tag == "chart")
            .nth(chart_index)
            .ok_or("missing famous chart")?
            .value;
        let csv = chart["data"]["csv"]
            .as_str()
            .ok_or("famous chart needs CSV")?
            .to_owned();
        let key = format!("{location}/{name}/{csv}");
        let data = if temporal {
            fs::read_to_string(root.join(format!("benchmarks/fixtures/time-series/{name}.csv")))?
                .lines()
                .skip_while(|line| line.starts_with('#'))
                .collect::<Vec<_>>()
                .join("\n")
                + "\n"
        } else {
            fs::read_to_string(root.join(format!("docs/public/{location}/{name}/{csv}")))?
        };
        chart["data"]["csv"] = key.clone().into();
        options
            .report_folder
            .as_mut()
            .unwrap()
            .files
            .push(key.clone());
        options.data.insert(key.clone(), data);
        let note = match slug {
            "playfair" => {
                "This view selects every eighth observation with a known wage. The axes have different scales; compare changes, not mark heights. Prices are not inflation-adjusted."
            }
            "nightingale" => {
                "Florence Nightingale used her 1858 diagram to make the scale of preventable deaths visible. These stacked bars replot the annualized rates on a linear rate scale; each period runs April to March."
            }
            "anscombe" => {
                "In 1973 F. J. Anscombe published four datasets with almost identical summaries and sharply different shapes. Plotting each set shows what a mean, variance or fitted equation can conceal."
            }
            "old-faithful" => {
                "Old Faithful observations pair eruption duration with waiting time. Their two clusters are a useful reminder to inspect a distribution before reducing it to one average."
            }
            "titanic" => {
                "The 1912 Titanic disaster is recorded here as aggregated class, sex and survival counts. Ribbons show contingency-table counts rather than the paths of individual people."
            }
            "gapminder" => {
                "Gapminder made country trajectories over time a familiar way to explore development. These frozen yearly observations from 1960–2020 keep fertility, life expectancy, population and region together."
            }
            "global-temperature" => {
                "NASA GISS combines land and ocean observations into a global temperature index. This frozen series compares completed annual means through 2025 with the 1951–1980 baseline."
            }
            "keeling-curve" => {
                "The Mauna Loa record began in 1958 with Charles David Keeling’s measurements. Its seasonal cycle sits on a long-term rise in atmospheric carbon dioxide."
            }
            "german-electricity" => {
                "This modern example uses 15-minute German day-ahead electricity prices from October 2025 through September 2026. Negative prices remain observations, distinct from missing values."
            }
            _ => "",
        };
        // Preserve the source note of the chart selected from the original report.
        let source_note = source
            .split("```chart")
            .nth(chart_index + 1)
            .and_then(|part| part.split_once("```"))
            .and_then(|(_, after)| after.trim_start().lines().next())
            .filter(|line| line.starts_with("*Source:"))
            .ok_or("featured chart needs a source note")?;
        let mut example_chart = chart.clone();
        example_chart["data"]["csv"] = "data.csv".into();
        let example = format!(
            "---\ntitle: {}\nstyles: [{}]\n---\n\n```chart\n{}\n```\n\n{source_note}\n",
            serde_json::to_string(label)?,
            previews::style(name),
            serde_json::to_string_pretty(&example_chart)?
        );
        let example_folder = public.join("examples/charts").join(slug);
        fs::create_dir_all(&example_folder)?;
        fs::write(example_folder.join("report.rmd"), &example)?;
        fs::write(
            example_folder.join("data.csv"),
            options.data.get(&key).ok_or("featured data is missing")?,
        )?;
        let summary = if slug == "anscombe" {
            source
                .split("## Almost the same summary")
                .nth(1)
                .and_then(|text| text.split("## A second distribution").next())
                .map(|body| format!("\n## Almost the same summary\n{body}"))
                .unwrap_or_default()
        } else {
            String::new()
        };
        let featured = format!(
            "---\ntitle: {}\nstyles: [scientific]\n---\n\n[Back to the chart gallery](https://richview.ai/docs/charts)\n\n{note}\n\n```chart\n{}\n```\n{summary}\n## Source\n\n````rmd\n{example}\n````\n\n## Data and credits\n\n[Complete report and provenance](https://richview.ai/docs/{location}/{name}/) · [Rich Markdown source](https://richview.ai/examples/charts/{slug}/report.rmd) · [CSV observations](https://richview.ai/docs/{key}).\n",
            serde_json::to_string(label)?,
            serde_json::to_string_pretty(&chart)?
        );
        let html = render_page_with_options(
            &featured,
            slug,
            Place::Featured(slug),
            charts,
            &options,
            flags,
        )?;
        let shown = richview_core::document::render(&featured, &options)
            .map_err(|errors| format!("Featured {slug} preview: {errors:?}"))?;
        let html = own_preview(
            &html,
            &folder.join(format!("{slug}.share.png")),
            &format!("/docs/charts/{slug}.share.png"),
            label,
            EXAMPLE_BYLINE,
            "scientific",
            &shown.charts,
        )?;
        fs::write(folder.join(format!("{slug}.html")), html)?;
        fs::write(folder.join(format!("{slug}.md")), featured)?;
    }
    index.push_str("\n## Every chart type\n\nKeep scrolling: every supported type is rendered here. Each also has a deep link with its complete source, important fields and style comparison. These compact examples use illustrative data. [Flow charts](https://richview.ai/docs/charts/flow) and [pivot tables](https://richview.ai/docs/charts/pivot) are elements of their own: the [elements gallery](https://richview.ai/docs/elements) shows them.\n");
    for page in gallery_charts(charts) {
        index.push_str(&format!(
            "\n### [{}](https://richview.ai/docs/charts/{})\n\n{}\n\n{}\n\n*Source: [the {} example](https://richview.ai/docs/charts/{}), illustrative data.*\n",
            page.title, page.name, page.use_when, page.chart_fence, page.title, page.name
        ));
    }
    index.push_str("\n## Time-series examples\n\nExplore a frozen real-world dataset, then download its Rich Markdown source and CSV to work with it locally.\n");
    for name in TIME_SERIES {
        let source =
            fs::read_to_string(root.join(format!("docs/public/examples/{name}/report.rmd")))?;
        let title = front_value(&source, "title").ok_or("Time-series example needs a title")?;
        let description =
            front_value(&source, "description").ok_or("Time-series example needs a description")?;
        index.push_str(&format!(
            "\n- [{title}](https://richview.ai/docs/examples/{name}/) — {description}\n"
        ));
    }
    let html = render_page_with_options(
        &index,
        "charts",
        Place::Gallery,
        charts,
        &gallery_options,
        flags,
    )?;
    fs::write(docs.join("charts.html"), html)?;
    fs::write(docs.join("charts.md"), index)?;
    for page in charts {
        let html = render_page(
            &page.source,
            &page.name,
            Place::ChartType(&page.name),
            charts,
            flags,
        )?;
        let example =
            richview_core::document::render(&example_source(page), &documentation_options())
                .map_err(|errors| format!("Chart {} preview: {errors:?}", page.name))?;
        let html = own_preview(
            &html,
            &folder.join(format!("{}.share.png", page.name)),
            &format!("/docs/charts/{}.share.png", page.name),
            &page.title,
            EXAMPLE_BYLINE,
            "scientific",
            &example.charts,
        )?;
        fs::write(folder.join(format!("{}.html", page.name)), html)?;
        // The playground and the page's Open link load the example from its own report folder.
        let example = public.join("examples/charts").join(&page.name);
        fs::create_dir_all(&example)?;
        fs::write(example.join("report.rmd"), example_source(page))?;
    }
    Ok(())
}

/// Stage the frozen observations once, beside each copyable report. The fixtures' leading
/// source comments live in the report's source note; CSV input starts with its column header.
fn stage_time_series(
    root: &Path,
    docs: &Path,
    charts: &[ChartPage],
    flags: BuildFlags,
) -> Result<()> {
    for name in TIME_SERIES {
        let folder = docs.join("examples").join(name);
        fs::create_dir_all(&folder)?;
        let source =
            fs::read_to_string(root.join(format!("docs/public/examples/{name}/report.rmd")))?;
        let file = folder.join("report.rmd");
        fs::write(&file, &source)?;
        let csv =
            fs::read_to_string(root.join(format!("benchmarks/fixtures/time-series/{name}.csv")))?;
        let data = csv
            .lines()
            .skip_while(|line| line.starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        fs::write(folder.join("data.csv"), data)?;
        let (_, options) = richview_app::resolver::resolve_document_options_policy(
            &source,
            &file,
            &[],
            false,
            false,
        )?;
        let html =
            render_page_with_options(&source, name, Place::Example(name), charts, &options, flags)?;
        fs::write(folder.join("index.html"), html)?;
    }
    Ok(())
}

/// The row of style links and the frame they fill: the page's example rendered by the site
/// runtime in the chosen builtin style (FEA-SF-43). Without scripts the row stays hidden.
fn style_frame(chart: &str) -> String {
    let links: String = chrome::site_styles()
        .iter()
        .map(|style| {
            format!(
                "<a href=\"/docs/previews/{chart}/{style}.html#rv-chart-0\" data-preview-src=\"/docs/previews/{chart}/{style}.html#rv-chart-0\" data-style-link=\"{style}\" data-chart=\"{chart}\">{}</a>",
                chrome::style_label(style)
            )
        })
        .collect();
    format!(
        "<div class=\"docs-styles\"><p class=\"docs-styles__label\">The same chart in each style</p><nav class=\"docs-styles__links\" aria-label=\"Style\">{links}</nav><iframe class=\"docs-styles__frame\" title=\"{chart} chart in the chosen style\" sandbox=\"allow-scripts allow-downloads\" hidden></iframe></div>"
    )
}

/// The report the playground opens for a chart type: its frontmatter and live example.
/// The home image every site page names until it has its own (FEA-SF-55).
const HOME_PREVIEW: &str =
    "<meta property=\"og:image\" content=\"https://richview.ai/share/home.png\" />";
/// The byline of the site's own example reports on their cards.
const EXAMPLE_BYLINE: &str = "RichView example";

/// A page that is a report or a chart is its own link preview (FEA-SF-55; Daniel, 16 Nov 2026:
/// "when they share a link, I want the preview to look as cool as this does"): the top of the
/// report, drawn by this build beside the page from the charts it renders, names the page in
/// place of the home image.
fn own_preview(
    html: &str,
    file: &Path,
    url: &str,
    title: &str,
    byline: &str,
    style: &str,
    charts: &[serde_json::Value],
) -> Result<String> {
    if !html.contains(HOME_PREVIEW) {
        return Err(format!("{url} names no home preview to replace").into());
    }
    fs::write(
        file,
        richview_share::card_png(title, byline, style, charts)?,
    )?;
    Ok(html.replacen(
        HOME_PREVIEW,
        &format!(
            "<meta property=\"og:image\" content=\"https://richview.ai{url}\" />\n\t\t<meta property=\"og:image:alt\" content=\"{}\" />",
            escape(&richview_share::alt_text(title, byline))
        ),
        1,
    ))
}

fn example_source(page: &ChartPage) -> String {
    format!(
        "---\ntitle: {}\nstyles: [scientific]\n---\n\n{}\n\n*Source: fictional inline example data.*\n",
        page.title, page.chart_fence
    )
}

/// Render one docs page and wrap it in the site's chrome and the docs layout.
fn render_page(
    source: &str,
    name: &str,
    place: Place,
    charts: &[ChartPage],
    flags: BuildFlags,
) -> Result<String> {
    let options = documentation_options();
    render_page_with_options(source, name, place, charts, &options, flags)
}

/// A docs page keeps its own contents column and has no reading bar, so the rendered report
/// inside it turns its reading aids off (FEA-RP-63, FEA-RP-64).
fn without_reading_aids(source: &str) -> String {
    const OFF: &str = "toc: false\nprogress: false\n";
    match source.strip_prefix("---\n") {
        Some(rest) => format!("---\n{OFF}{rest}"),
        None => format!("---\n{OFF}---\n\n{source}"),
    }
}

fn documentation_options() -> RenderOptions {
    RenderOptions {
        report_folder: Some(ReportFolder {
            root: "documentation".into(),
            source: "guide.rmd".into(),
            files: vec!["guide.rmd".into()],
        }),
        ..Default::default()
    }
}

fn render_page_with_options(
    source: &str,
    name: &str,
    place: Place,
    charts: &[ChartPage],
    options: &RenderOptions,
    flags: BuildFlags,
) -> Result<String> {
    let source = without_reading_aids(&public::source(source, flags)?);
    let source = source.as_str();
    let rendered = richview_core::document::render(source, options)
        .map_err(|errors| format!("Documentation {name} failed: {errors:?}"))?;
    ensure_clean(name, &rendered.diagnostics)?;
    let (decorated, mut toc) = decorate(source, &rendered.html)?;
    if place == Place::Gallery {
        toc.push_str("<details><summary>Chart types</summary>");
        for page in gallery_charts(charts) {
            toc.push_str(&format!(
                "<a href=\"#{}\">{}</a>",
                reference::slug(&page.title),
                escape(&page.title)
            ));
        }
        toc.push_str("</details>");
    }
    let decorated = report_frames(&decorated);
    let decorated = decorated
        .replace(&format!("<p>{CUE}"), "<p class=\"docs-cue\">")
        .replace("<p>{{featured-menu}}</p>", &previews::featured_menu())
        .replace("<p>{{report-grid}}</p>", &previews::report_grid())
        .replace("<p>{{docs-doors}}</p>", &previews::doors())
        .replace(
            "<p>{{style-comparison}}</p>",
            &style_frame("bar")
                .replace(
                    " hidden></iframe>",
                    " src=\"/docs/previews/bar/sleek.html#rv-chart-0\"></iframe>",
                )
                .replacen(
                    "data-style-link=\"sleek\"",
                    "data-style-link=\"sleek\" aria-current=\"true\"",
                    1,
                ),
        );
    let decorated = decorated
        .replace("<p>{{typography}}</p>", &typography_switch())
        .replace("<p>{{elements}}</p>", &previews::elements_grid());
    if place == Place::Guide("elements") {
        for (anchor, label) in ELEMENTS {
            if !toc.contains(&format!("href=\"#{anchor}\"")) {
                return Err(format!("the elements gallery needs a `## {label}` section").into());
            }
        }
    }
    let sidebar = sidebar(place, charts, flags);
    let source_link = match place {
        Place::Guide(stem) => format!("/docs/{stem}.md"),
        Place::Reference => "/docs/reference.md".into(),
        Place::Featured(name) => format!("/docs/charts/{name}.md"),
        Place::ChartType(chart) => format!("/examples/charts/{chart}/report.rmd"),
        Place::Example(name) => format!("/docs/examples/{name}/report.rmd"),
        Place::Report(name) => format!("/docs/gallery/{name}/report.rmd"),
        Place::Gallery => "/docs/charts.md".into(),
    };
    let source_link = if matches!(
        place,
        Place::Gallery | Place::Guide("index" | "elements" | "playground" | "styles")
    ) || source_link.is_empty()
    {
        String::new()
    } else {
        format!(
            "<a class=\"docs-source-link\" href=\"{source_link}\">View Markdown source <span aria-hidden=\"true\">↗</span></a>"
        )
    };
    let top = format!(
        "<a class=\"skip\" href=\"#guide-content\">Skip to content</a>{}<details class=\"docs-mobile-menu\"><summary>Documentation</summary>{sidebar}</details><div class=\"docs-layout\"><aside class=\"docs-sidebar\">{sidebar}</aside>",
        chrome::header()
            + "<search class=\"docs-search\"><label for=\"docs-search\">Search documentation <kbd>⌘ / Ctrl K</kbd></label><input id=\"docs-search\" data-docs-search type=\"search\" placeholder=\"Find a field, chart or guide\" autocomplete=\"off\" aria-controls=\"docs-search-results\"><ol id=\"docs-search-results\" data-docs-search-results aria-live=\"polite\"></ol></search>"
    );
    let end = format!(
        "<aside class=\"docs-toc\"><nav aria-label=\"On this page\">{toc}</nav>{source_link}</aside></div>{}",
        chrome::footer()
    );
    let actions = match place {
        Place::Report(name) => format!("<div class=\"docs-actions\"><a class=\"docs-button\" href=\"/docs/gallery/{name}/report.rmd\">View source <span aria-hidden=\"true\">↗</span></a></div>"),
        Place::Guide("index") if !flags.install_enabled => "<div class=\"docs-actions\"><a class=\"docs-button\" href=\"/playground\">Try in your browser <span aria-hidden=\"true\">↗</span></a><a class=\"docs-button docs-button--secondary\" href=\"/docs/first-report\">Read the first-report guide</a></div>".to_string(),
        Place::Guide("index") => "<div class=\"docs-actions\"><a class=\"docs-button\" href=\"/docs/first-report\">Make your first report <span aria-hidden=\"true\">→</span></a><a class=\"docs-button docs-button--secondary\" href=\"/playground\">Try in your browser <span aria-hidden=\"true\">↗</span></a></div>".to_string(),
        _ => String::new(),
    };
    let mut html = strip_report_policy(&decorated)
        // The report shell names an empty icon; the page names the site's, and only that one.
        .replace("<link rel=\"icon\" href=\"data:,\">", "")
        // These guides navigate the website rather than reference report resources.
        // Rebase public web links so the same pages work on a loopback preview.
        .replace("href=\"https://richview.ai/", "href=\"/")
        .replacen(
            "</head>",
            "<link rel=\"preload\" href=\"/fonts/geist-400.woff2\" as=\"font\" type=\"font/woff2\" crossorigin><link rel=\"preload\" href=\"/fonts/bricolage-grotesque-600.woff2\" as=\"font\" type=\"font/woff2\" crossorigin><link rel=\"stylesheet\" href=\"/site.css\"><link rel=\"stylesheet\" href=\"/docs/docs.css\"><link rel=\"icon\" type=\"image/svg+xml\" href=\"/landing/favicon.svg\"></head>",
            1,
        )
        .replacen("<body>", &format!("<body class=\"docs-page\" data-richview-page=\"docs\">{top}"), 1)
        .replacen("</header><main", &format!("{actions}</header><main"), 1)
        .replacen(
            "<main class=\"rv-report__body\">",
            "<main class=\"rv-report__body\" id=\"guide-content\" tabindex=\"-1\">",
            1,
        );
    if let Place::Guide(stem) = place {
        html = html.replacen(
            "<footer class=\"rv-report__footer\">",
            &format!("{}<footer class=\"rv-report__footer\">", pager(stem)),
            1,
        );
    }
    html = html.replacen(
        "<footer class=\"rv-report__footer\">",
        &format!(
            "{}<footer class=\"rv-report__footer\">",
            feedback(place, flags, name)
        ),
        1,
    );
    if let Place::ChartType(chart) | Place::Featured(chart) = place {
        html = html.replacen(
            "<h2 id=\"source\">Source</h2>",
            &format!(
                "{}<h2 id=\"source\">Source</h2><div class=\"docs-actions\"><button class=\"docs-button\" type=\"button\" data-copy-source>Copy source</button><a class=\"docs-button docs-button--secondary\" href=\"/playground?src=/examples/charts/{chart}/report.rmd\">Open in playground <span aria-hidden=\"true\">↗</span></a></div>",
                style_frame(chart)
            ),
            1,
        );
    }
    html = html.replacen("</body>", &format!("{end}</body>"), 1);
    // Docs pages that embed charts load the report runtime (ARC-05, ARC-06), which draws them
    // and runs their interactions; the site runtime provides example copy and page feedback.
    let needs_runtime = html.contains("data-rv-payload");
    let needs_copy = html.contains("data-docs-search")
        || html.contains("data-copy-source")
        || html.contains("data-copy-code")
        || html.contains("data-docs-feedback");
    let mut scripts = String::new();
    if needs_runtime {
        scripts.push_str("<script src=\"/richview_runtime.js\" defer></script><script src=\"/report-runtime.js\" defer></script>");
    }
    if needs_copy {
        scripts.push_str("<script src=\"/richview_docs.js\" defer></script><script src=\"/bootstrap.js\" defer></script>");
    }
    let html = soft_breaks(&reference::field_anchors(&html));
    Ok(chrome::share_metadata(&html.replacen(
        "</body>",
        &format!("{scripts}</body>"),
        1,
    )))
}

/// Offers a line break after the dots, slashes and underscores of the inline code in table cells,
/// so a long dotted name wraps between its parts instead of widening its column. The break is
/// invisible and adds no character: the words stay as written, and version numbers stay whole.
fn soft_breaks(html: &str) -> String {
    let mut out = String::with_capacity(html.len() + html.len() / 64);
    let mut rest = html;
    while let Some(start) = rest.find("<td>") {
        let cell = start + "<td>".len();
        let end = rest[cell..].find("</td>").map_or(rest.len(), |n| n + cell);
        out.push_str(&rest[..cell]);
        out.push_str(&break_code(&rest[cell..end]));
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

fn break_code(cell: &str) -> String {
    let mut out = String::with_capacity(cell.len());
    let mut rest = cell;
    while let Some(start) = rest.find("<code") {
        let Some(open) = rest[start..].find('>').map(|n| n + start + 1) else {
            break;
        };
        let Some(close) = rest[open..].find("</code>").map(|n| n + open) else {
            break;
        };
        out.push_str(&rest[..open]);
        let mut chars = rest[open..close].chars().peekable();
        let (mut previous, mut in_tag) = (' ', false);
        while let Some(c) = chars.next() {
            out.push(c);
            match c {
                '<' => in_tag = true,
                '>' => in_tag = false,
                '.' | '/' | '_'
                    if !in_tag
                        && chars.peek().is_some_and(|next| {
                            next.is_alphanumeric()
                                && !(previous.is_ascii_digit() && next.is_ascii_digit())
                        }) =>
                {
                    out.push_str("<wbr>");
                }
                _ => {}
            }
            previous = c;
        }
        rest = &rest[close..];
    }
    out.push_str(rest);
    out
}

/// A rendered report carries its own strict policy in a `<meta>`. Docs pages are site pages:
/// their policy is the response header of FEA-SF-60, which lets them load the site's styles and
/// the report runtime from richview.ai.
fn strip_report_policy(html: &str) -> String {
    let marker = "<meta http-equiv=\"Content-Security-Policy\"";
    let Some(start) = html.find(marker) else {
        return html.to_owned();
    };
    let Some(length) = html[start..].find('>') else {
        return html.to_owned();
    };
    format!("{}{}", &html[..start], &html[start + length + 1..])
}

fn sidebar(place: Place, charts: &[ChartPage], flags: BuildFlags) -> String {
    let link = |url: &str, label: &str, current: bool| {
        format!(
            "<a href=\"{url}\"{}>{}</a>",
            if current {
                " aria-current=\"page\""
            } else {
                ""
            },
            escape(label)
        )
    };
    let mut html = String::from(
        "<nav class=\"docs-nav\" aria-label=\"Documentation\"><p class=\"docs-nav__group\">Gallery</p>",
    );
    html.push_str(&link(
        "/docs/",
        "Reports",
        matches!(place, Place::Guide("index") | Place::Report(_)),
    ));
    // A flow chart's or a pivot table's page belongs to its element, not to the chart gallery.
    let own_element = match place {
        Place::ChartType(chart) => element_of(chart),
        _ => None,
    };
    let in_gallery = own_element.is_none()
        && matches!(
            place,
            Place::Gallery | Place::Featured(_) | Place::ChartType(_) | Place::Example(_)
        );
    // The elements gallery overviews everything a report is made of; each element under it
    // is a way in.
    html.push_str(&link(
        "/docs/elements",
        "Elements gallery",
        place == Place::Guide("elements"),
    ));
    html.push_str("<div class=\"docs-nav__sub\" data-nav=\"elements\">");
    for &(anchor, label) in ELEMENTS {
        match anchor {
            "charts" => html.push_str(&link("/docs/charts", label, in_gallery)),
            "flow-charts" => html.push_str(&link(
                "/docs/charts/flow",
                label,
                place == Place::ChartType("flow"),
            )),
            // A part of the elements gallery; a page that belongs to it marks it as its place.
            _ => html.push_str(&format!(
                "<a href=\"/docs/elements#{anchor}\"{}>{}</a>",
                if own_element == Some(anchor) {
                    " aria-current=\"true\""
                } else {
                    ""
                },
                escape(label)
            )),
        }
        if anchor == "charts" && in_gallery {
            html.push_str("<div class=\"docs-nav__sub\"><details class=\"docs-nav__more docs-chart-types\"><summary>Chart types</summary><div class=\"docs-nav__sub\">");
            for page in gallery_charts(charts) {
                // On the gallery every type is on the page: the list moves the reader to it.
                // From a type's own page it opens the other types' pages.
                let url = if place == Place::Gallery {
                    format!("#{}", reference::slug(&page.title))
                } else {
                    format!("/docs/charts/{}", page.name)
                };
                html.push_str(&link(
                    &url,
                    &page.title,
                    place == Place::ChartType(&page.name),
                ));
            }
            html.push_str("</div></details></div>");
        }
    }
    html.push_str("</div>");
    html.push_str("<p class=\"docs-nav__group\">Playground</p>");
    html.push_str(&link(
        "/docs/playground",
        "Using the playground",
        place == Place::Guide("playground"),
    ));
    html.push_str(&link("/playground", "Open playground", false));
    let guides = |html: &mut String, stems: &[&str]| {
        for (stem, label) in GUIDES.iter().filter(|(stem, _)| stems.contains(stem)) {
            html.push_str(&link(&guide_url(stem), label, place == Place::Guide(stem)));
        }
    };
    html.push_str("<p class=\"docs-nav__group\">How to use</p>");
    guides(&mut html, HOW_TO);
    // The references stand above the deeper guides, with the format reference's topics under
    // it: parts of its one long page, open inside the reference, one click away elsewhere.
    html.push_str("<p class=\"docs-nav__group\">Reference</p>");
    html.push_str(&link(
        "/docs/reference",
        "Format reference",
        place == Place::Reference,
    ));
    html.push_str(&format!(
        "<div class=\"docs-nav__sub\"><details class=\"docs-nav__more\" data-nav=\"topics\"{}><summary>Topics</summary><div class=\"docs-nav__sub\">",
        if place == Place::Reference {
            " open"
        } else {
            ""
        }
    ));
    for (name, title) in reference::topic_pages() {
        html.push_str(&link(&format!("/docs/reference#{name}"), title, false));
    }
    html.push_str(&link(
        "/docs/reference#chart-specific-fields",
        "Chart-specific fields",
        false,
    ));
    html.push_str("</div></details></div>");
    guides(&mut html, REFERENCES);
    html.push_str("<p class=\"docs-nav__group\">Nitty-gritty</p>");
    guides(&mut html, NITTY_GRITTY);
    html.push_str("</nav>");
    if !flags.install_enabled {
        html.push_str("<p class=\"docs-sidebar__note\">Hosted installer: at launch</p>");
    }
    html
}

/// The first report: a downloadable folder that renders clean in every builtin style.
fn stage_first_report(root: &Path, public: &Path, docs: &Path) -> Result<()> {
    // Explicit tutorial allowlist: never publish the checkout or scratch directory.
    let example = docs.join("example");
    fs::create_dir_all(example.join("data"))?;
    for file in ["report.rmd", "report.rstyle.yaml", "data/orders.csv"] {
        fs::copy(
            root.join("examples/first-report").join(file),
            example.join(file),
        )?;
    }
    let file = example.join("report.rmd");
    let source = fs::read_to_string(&file)?;
    let mut renders: Vec<(String, Vec<String>)> = vec![("report".into(), vec![])];
    for style in chrome::site_styles() {
        renders.push((style.to_string(), vec![style.to_string()]));
    }
    for (name, styles) in renders {
        let options = crate::resolve_document_options(&source, &file, &styles, public)?.1;
        let rendered = richview_core::document::render(&source, &options)
            .map_err(|errors| format!("Tutorial {name} failed: {errors:?}"))?;
        ensure_clean(&name, &rendered.diagnostics)?;
        richview_app::resolver::export_report(
            &file,
            Some(Path::new(&format!("{name}.html"))),
            &rendered,
            &options,
        )?;
    }
    Ok(())
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Highlight a code sample with core's highlighter: the one site palette colours its spans.
fn highlight(code: &str, language: &str) -> String {
    richview_core::highlight_html(code, HighlightLanguage::from_name(language))
}

/// Wrap code samples, mark inline commands and link the page's second-level headings.
fn decorate(source: &str, rendered: &str) -> Result<(String, String)> {
    let markdown = source
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map_or(source, |(_, body)| body);
    let mut html = rendered.to_owned();
    let mut block: Option<(String, String)> = None;
    let mut heading: Option<String> = None;
    let mut toc = String::new();
    for event in Parser::new_ext(markdown, richview_core::document::MARKDOWN_OPTIONS) {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                block = Some((
                    info.split_whitespace().next().unwrap_or("text").to_owned(),
                    String::new(),
                ));
            }
            Event::Text(text) => {
                if let Some((_, code)) = &mut block {
                    code.push_str(&text);
                }
                if let Some(title) = &mut heading {
                    title.push_str(&text);
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some((language, code)) = block.take() {
                    // Rich-block fences (chart, table, big, callout, image, embed, audio,
                    // video, metric) are rendered by richview-core as inline SVG/HTML,
                    // not as <pre><code>.
                    if richview_core::RICH_TAGS.contains(&language.as_str()) {
                        continue;
                    }
                    let opening = format!("<pre><code class=\"language-{language}\">");
                    let start = html
                        .find(&opening)
                        .ok_or_else(|| format!("Missing documentation code block: {language}"))?;
                    let end = start
                        + html[start..]
                            .find("</code></pre>")
                            .ok_or("Unclosed documentation code block")?
                        + "</code></pre>".len();
                    let label = match language.as_str() {
                        "sh" | "bash" => "Shell",
                        "powershell" => "PowerShell",
                        "rmd" => "Rich Markdown",
                        "yaml" => "YAML",
                        "json" => "JSON",
                        "text" if code.contains("\n  ") => "Report folder",
                        "text" if code.starts_with("Install RichView") => "Agent prompt",
                        "text" => "Configuration",
                        _ => &language,
                    };
                    let prose = language == "text" && label == "Agent prompt";
                    let code = highlight(&code, &language);
                    html.replace_range(start..end, &format!("<figure class=\"docs-code{}\"><figcaption><span>{}</span><button type=\"button\" data-copy-code aria-label=\"Copy {} example\">Copy</button></figcaption><pre tabindex=\"0\" aria-label=\"{} example\"><code class=\"language-{language}\">{code}</code></pre></figure>", if prose { " docs-code--prose" } else { "" }, escape(label), escape(label), escape(label)));
                }
            }
            Event::Code(code) => {
                if let Some(title) = &mut heading {
                    title.push_str(&code);
                }
                if [
                    "richview ",
                    "richview-server ",
                    "cargo ",
                    "npm ",
                    "rustup ",
                    "curl ",
                ]
                .iter()
                .any(|prefix| code.starts_with(prefix))
                {
                    html = html.replacen(
                        &format!("<code>{}</code>", escape(&code)),
                        &format!(
                            "<code class=\"docs-inline-command\">{}</code>",
                            highlight(&code, "sh")
                        ),
                        1,
                    );
                }
            }
            Event::Start(Tag::Heading {
                level: HeadingLevel::H2,
                ..
            }) => heading = Some(String::new()),
            Event::End(TagEnd::Heading(HeadingLevel::H2)) => {
                if let Some(title) = heading.take() {
                    let id = title
                        .to_ascii_lowercase()
                        .split(|c: char| !c.is_ascii_alphanumeric())
                        .filter(|word| !word.is_empty())
                        .collect::<Vec<_>>()
                        .join("-");
                    html = html.replacen("<h2>", &format!("<h2 id=\"{id}\">"), 1);
                    toc.push_str(&format!("<a href=\"#{id}\">{}</a>", escape(&title)));
                }
            }
            _ => {}
        }
    }
    Ok((html, toc))
}

fn pager(page: &str) -> String {
    let position = GUIDES.iter().position(|(name, _)| *name == page).unwrap();
    let mut html = String::from("<nav class=\"docs-pager\" aria-label=\"Guide pages\">");
    for (neighbor, direction, arrow) in [
        (position.checked_sub(1), "Previous", "←"),
        (Some(position + 1), "Next", "→"),
    ] {
        if let Some((name, label)) = neighbor.and_then(|index| GUIDES.get(index)) {
            html.push_str(&format!("<a href=\"{}\"><span>{direction}</span><strong>{} <span aria-hidden=\"true\">{arrow}</span></strong></a>", guide_url(name), escape(label)));
        } else {
            html.push_str("<span></span>");
        }
    }
    html.push_str("</nav>");
    html
}

fn ensure_clean(name: &str, diagnostics: &[richview_core::document::Diagnostic]) -> Result<()> {
    if diagnostics.iter().any(|d| d.level == "error") {
        return Err(format!("Documentation {name} has hard failures: {diagnostics:?}").into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_guide_shows_the_powershell_command_only_when_the_release_ships_it() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let guide = std::fs::read_to_string(root.join("docs/public/install.md")).unwrap();
        let command = "releases/latest/download/install.ps1 | iex";
        for (install_enabled, windows_preview) in
            [(false, false), (false, true), (true, false), (true, true)]
        {
            // An enabled release builds only for the public repository.
            let flags = super::BuildFlags {
                install_enabled,
                repository_public: install_enabled,
                windows_preview,
                ..Default::default()
            };
            let staged = guide.replace("{{windows-install}}", &super::windows_install(flags));
            let text = super::public::source(&staged, flags).unwrap();
            assert_eq!(
                text.contains(command),
                install_enabled && windows_preview,
                "install {install_enabled}, windows {windows_preview}"
            );
            assert_eq!(
                text.contains("## Windows"),
                windows_preview,
                "the Windows heading follows the switch"
            );
        }
    }

    #[test]
    fn a_page_starts_its_own_runtime_and_one_for_each_frame_of_the_site() {
        let page = |body: &str| format!("<script src=\"/richview_runtime.js\"></script>{body}");
        assert_eq!(runtimes_started("<p>no charts</p>"), 0);
        assert_eq!(runtimes_started(&page("")), 1);
        // A style row's frame has no address until a style is picked.
        assert_eq!(runtimes_started(&page(&style_frame("bar"))), 1);
        assert_eq!(
            runtimes_started(&page(
                "<iframe loading=\"lazy\" src=\"/docs/previews/bar/sleek.html\"></iframe><iframe src=\"https://www.youtube-nocookie.com/embed/x\"></iframe>"
            )),
            2
        );
        // The five builtin styles' type samples share one frame: the page stays within the budget
        // with the elements overview's live Type card beside it.
        let switch = typography_switch();
        assert_eq!(switch.matches("<iframe").count(), 1);
        assert_eq!(
            switch.matches("data-style-link=").count(),
            chrome::site_styles().len()
        );
        assert_eq!(switch.matches("aria-current=\"true\"").count(), 1);
        assert!(
            runtimes_started(&page(&format!("{}{switch}", previews::elements_grid())))
                <= RUNTIME_BUDGET
        );
    }

    #[test]
    fn the_build_refuses_a_docs_page_over_the_runtime_budget() {
        let folder = tempfile::tempdir().unwrap();
        let nested = folder.path().join("guides");
        fs::create_dir_all(&nested).unwrap();
        let frames =
            "<iframe src=\"/docs/previews/bar/sleek.html\"></iframe>".repeat(RUNTIME_BUDGET);
        let page = format!(
            "<body data-richview-page=\"docs\"><script src=\"/richview_runtime.js\"></script>{frames}"
        );
        // A report with that many frames is not a docs page; only docs pages are held to it.
        fs::write(nested.join("report.html"), &frames).unwrap();
        check_runtime_budget(folder.path()).unwrap();
        fs::write(nested.join("many.html"), &page).unwrap();
        let error = check_runtime_budget(folder.path()).unwrap_err().to_string();
        assert!(
            error.contains("many.html starts 4 RichView runtimes"),
            "{error}"
        );
    }

    #[test]
    fn docs_pages_turn_the_reports_reading_aids_off() {
        assert_eq!(
            without_reading_aids("---\ntitle: A\n---\n\nBody\n"),
            "---\ntoc: false\nprogress: false\ntitle: A\n---\n\nBody\n"
        );
        assert_eq!(
            without_reading_aids("Body\n"),
            "---\ntoc: false\nprogress: false\n---\n\nBody\n"
        );
    }

    #[test]
    fn the_flagship_leads_the_reports_and_opens_on_its_showcase() {
        let grid = previews::report_grid();
        let first = grid.find("docs-chart-card__link").unwrap();
        assert!(grid[first..].starts_with(&format!(
            "docs-chart-card__link\" href=\"{}\"",
            previews::FLAGSHIP_URL
        )));
        // The card shows the cover the report opens with.
        let cover = format!("{}{}", previews::FLAGSHIP_URL, previews::FLAGSHIP_COVER);
        assert!(grid.contains(&format!("<img src=\"{cover}\" alt=\"\"")));
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples")
            .join(previews::FLAGSHIP);
        let report = fs::read_to_string(root.join("report.rmd")).unwrap();
        assert!(report.contains(&format!(
            "title_image:\n  src: {}\n",
            previews::FLAGSHIP_COVER
        )));
        assert!(root.join(previews::FLAGSHIP_COVER).is_file());
        let html = report_frames("<p>{{report:openai-vs-anthropic}}</p>");
        assert!(html.contains("href=\"/showcase/openai-vs-anthropic/\">Open live report</a>"));
        assert!(html.contains("<a href=\"/showcase/openai-vs-anthropic/\"><img src=\"/docs/previews/report-openai-vs-anthropic/sleek-page.png\" alt=\"The Most Expensive Race Ever Run\" width=\"1000\" height=\"750\""));
        assert!(!html.contains("{{report:"));
    }

    #[test]
    fn a_chart_or_report_page_names_its_own_card_instead_of_the_home_image() {
        let page = chrome::share_metadata(
            "<html><head><title>Line chart</title></head><body></body></html>",
        );
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("line.share.png");
        let source = "---\ntitle: Line\nstyles: [scientific]\n---\n\n```chart\ntype: line\ntitle: Rates\ndata:\n  inline:\n    - {month: 1, rate: 1}\n    - {month: 2, rate: 3}\nencoding: {x: month, y: rate}\n```\n";
        let charts = richview_core::document::render(source, &documentation_options())
            .unwrap()
            .charts;
        let html = own_preview(
            &page,
            &file,
            "/docs/charts/line.share.png",
            "Line chart",
            EXAMPLE_BYLINE,
            "scientific",
            &charts,
        )
        .unwrap();
        assert!(html.contains("<meta property=\"og:image\" content=\"https://richview.ai/docs/charts/line.share.png\" />"));
        assert!(html.contains(
            "<meta property=\"og:image:alt\" content=\"Line chart, by RichView example\" />"
        ));
        assert!(
            !html.contains("share/home.png") && html.contains("og:image:width\" content=\"1200\"")
        );
        assert_eq!(&fs::read(&file).unwrap()[1..4], b"PNG");
        // A page without the home image is a page that names its own: it is refused, not changed.
        assert!(own_preview(&html, &file, "/x", "X", "", "scientific", &charts).is_err());
    }

    #[test]
    fn block_tours_use_their_complete_export_as_the_preview() {
        for name in ["blocks", "typography"] {
            let html = report_frames(&format!("<p>{{{{report:{name}}}}}</p>"));
            assert!(html.contains(&format!(
                "src=\"/docs/gallery/{name}/Rendered/report.html\""
            )));
            assert!(!html.contains("data-docs-preview"));
        }
    }

    #[test]
    fn documentation_build_renders_both_install_states_and_preserves_gallery_sources() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        fs::create_dir_all(root.join("tmp")).unwrap();
        let scratch = tempfile::tempdir_in(root.join("tmp")).unwrap();
        for (enabled, repository_public, folder) in [
            (false, false, "private"),
            (false, true, "before-launch"),
            (true, true, "launch"),
        ] {
            let output = scratch.path().join(folder);
            fs::create_dir_all(&output).unwrap();
            for file in [
                "richview_runtime.js",
                "richview_runtime_bg.wasm",
                "richview_runtime_bg.wasm.gz",
            ] {
                fs::copy(root.join("dist/web").join(file), output.join(file)).unwrap();
            }
            fs::copy(root.join("assets/_headers"), output.join("_headers")).unwrap();
            stage(
                &root,
                &output,
                BuildFlags {
                    install_enabled: enabled,
                    repository_public,
                    ..Default::default()
                },
            )
            .unwrap();
            let docs = output.join("docs");
            // A topic's old address only forwards, and a forward is not a page of the site:
            // nothing the build writes may lead a reader, or the link crawl, to one.
            assert_eq!(forward_links(&docs), Vec::<String>::new());
            let fields = fs::read_to_string(docs.join("reference/charts/line.html")).unwrap();
            assert!(fields.contains("href=\"/docs/reference#charts\""));
            let reference = fs::read_to_string(docs.join("reference.html")).unwrap();
            assert!(reference.contains("<h2 id=\"charts\""));
            let first = fs::read_to_string(docs.join("first-report.md")).unwrap();
            let install = fs::read_to_string(docs.join("install.md")).unwrap();
            if !repository_public {
                assert!(!first.contains("github.com"));
                assert!(!install.contains("github.com"));
            }
            assert_eq!(
                first.contains(
                    "https://github.com/1vecera/richview/releases/latest/download/install.sh"
                ),
                enabled
            );
            assert_eq!(
                install.contains("The native release is not available yet."),
                !enabled
            );
            let extension = fs::read_to_string(docs.join("vscode.md")).unwrap();
            assert!(extension.contains("not on the Visual Studio Marketplace yet"));

            let index = fs::read_to_string(docs.join("index.html")).unwrap();
            assert!(!index.contains("{{report:"));
            assert!(index.contains("/docs/gallery/longer-lives/Rendered/report.html"));
            let gallery = fs::read_to_string(docs.join("charts.html")).unwrap();
            let pages = read_chart_pages(&root).unwrap();
            assert_eq!(
                gallery.matches("data-rv-payload>").count(),
                gallery_charts(&pages).count() + 3,
                "the gallery opens with the flagship showcase, a chart to play and a chart to zoom, and keeps every chart type inline after the featured menu"
            );
            // A flow chart and a pivot table are elements of their own: the gallery links
            // their pages and draws neither, and the elements gallery draws both.
            assert_eq!(
                gallery_charts(&pages).count(),
                pages.len() - ELEMENT_PAGES.len()
            );
            let live = fs::read_to_string(docs.join("elements.html")).unwrap();
            for &(chart, _) in ELEMENT_PAGES {
                let page = pages.iter().find(|page| page.name == chart).unwrap();
                let heading = format!("<h3 id=\"{}\"", reference::slug(&page.title));
                assert!(
                    !gallery.contains(&heading),
                    "{chart} is in the chart gallery"
                );
                assert!(gallery.contains("<h3 id=\"bar-chart\""));
                assert!(gallery.contains(&format!("href=\"/docs/charts/{chart}\"")));
                assert!(docs.join(format!("charts/{chart}.html")).is_file());
                assert!(live.contains(&format!("\"type\":\"{chart}\"")), "{chart}");
            }
            let showcase = gallery.find(SHOWCASE_TITLE).unwrap();
            let invitation = gallery.find("Try it yourself").unwrap();
            assert!(showcase < invitation);
            assert!(invitation < gallery.find("docs-featured-grid").unwrap());
            assert!(gallery.contains("rv-chart rv-chart--full"));
            for name in [
                "playfair",
                "nightingale",
                "anscombe",
                "old-faithful",
                "titanic",
                "gapminder",
                "global-temperature",
                "keeling-curve",
                "german-electricity",
            ] {
                assert!(gallery.contains(&format!("href=\"/docs/charts/{name}\"")));
                let example = fs::read_to_string(docs.join(format!("charts/{name}.html"))).unwrap();
                assert_eq!(example.matches("data-rv-payload>").count(), 1);
                assert!(example.contains("href=\"/docs/charts\""));
                assert!(example.contains("data-docs-feedback"));
                assert!(docs.join(format!("charts/{name}.md")).is_file());
            }
            let elements = fs::read_to_string(docs.join("elements.md")).unwrap();
            let tags: std::collections::BTreeSet<_> = richview_core::document::parse(&elements)
                .unwrap()
                .blocks
                .into_iter()
                .map(|block| block.tag)
                .collect();
            for tag in richview_core::RICH_TAGS {
                assert!(tags.contains(*tag), "elements gallery is missing {tag}");
            }
            for &(stem, _) in GUIDES {
                let html = fs::read_to_string(docs.join(format!("{stem}.html"))).unwrap();
                assert!(
                    html.contains("data-docs-feedback"),
                    "missing feedback on {stem}"
                );
            }
            for &(name, files) in REPORTS {
                for file in files {
                    let original = fs::read(report_source_folder(&root, name).join(file)).unwrap();
                    let expected = if !repository_public
                        && (file.ends_with(".md") || file.ends_with(".rmd"))
                    {
                        public::source(&String::from_utf8(original).unwrap(), BuildFlags::default())
                            .unwrap()
                            .into_bytes()
                    } else {
                        original
                    };
                    assert_eq!(
                        fs::read(docs.join("gallery").join(name).join(file)).unwrap(),
                        expected
                    );
                }
                assert!(
                    docs.join("gallery")
                        .join(name)
                        .join("Rendered/report.html")
                        .is_file()
                );
            }
            let headers = fs::read_to_string(output.join("_headers")).unwrap();
            assert!(headers.contains("/docs/*\n  Content-Security-Policy:"));
            assert!(
                headers
                    .contains("/docs/gallery/longer-lives/Rendered/*\n  ! Content-Security-Policy")
            );
            let styles = fs::read_to_string(docs.join("style-yaml.md")).unwrap();
            for leaf in richview_core::style_vocabulary()
                .iter()
                .filter(|leaf| leaf.mark != richview_core::StyleMark::Internal)
            {
                assert!(
                    styles.contains(&format!("`{}`", leaf.path)),
                    "missing {}",
                    leaf.path
                );
            }
        }
    }

    use super::*;

    /// Every place a built documentation file names a reference topic's old address, as
    /// `file: address`. Each old address must exist as a forward, or the list would prove
    /// nothing.
    fn forward_links(docs: &Path) -> Vec<String> {
        fn visit(root: &Path, dir: &Path, old: &[String], found: &mut Vec<String>) {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    visit(root, &path, old, found);
                    continue;
                }
                // Images, fonts and Wasm hold no links.
                let Ok(text) = fs::read_to_string(&path) else {
                    continue;
                };
                for address in old {
                    for (at, _) in text.match_indices(address.as_str()) {
                        // A longer address is another page, such as a chart type's fields
                        // under `charts/`.
                        let next = text[at + address.len()..].chars().next();
                        if !next.is_some_and(|c| c.is_ascii_alphanumeric() || c == '-' || c == '/')
                        {
                            let file = path.strip_prefix(root).unwrap().display();
                            found.push(format!("{file}: {address}"));
                        }
                    }
                }
            }
        }
        let old: Vec<String> = reference::topic_pages()
            .iter()
            .map(|(name, _)| format!("/docs/reference/{name}"))
            .collect();
        for address in &old {
            let forward = docs.join(format!("{}.html", &address["/docs/".len()..]));
            let html = fs::read_to_string(&forward).unwrap();
            assert!(html.contains("http-equiv=\"refresh\""), "{address}");
        }
        let mut found = Vec::new();
        visit(docs, docs, &old, &mut found);
        found.sort();
        found.dedup();
        found
    }

    #[test]
    fn table_codes_may_break_after_their_dots_and_nowhere_else() {
        let html = "<p><code>a.b_c</code></p><table><tr><td><code id=\"field-x\">data.metricflow.cache_freshness</code> 1.0.2 <code>/api/v/0.0.1</code></td><td>plain.text</td></tr></table>";
        assert_eq!(
            soft_breaks(html),
            "<p><code>a.b_c</code></p><table><tr><td><code id=\"field-x\">data.<wbr>metricflow.<wbr>cache_<wbr>freshness</code> 1.0.2 <code>/<wbr>api/<wbr>v/<wbr>0.0.1</code></td><td>plain.text</td></tr></table>"
        );
    }

    #[test]
    fn the_report_policy_meta_is_removed_and_the_rest_stays() {
        let html = "<head><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'\"><title>x</title></head>";
        assert_eq!(
            strip_report_policy(html),
            "<head><meta charset=\"utf-8\"><title>x</title></head>"
        );
    }

    #[test]
    fn guide_urls_are_clean() {
        assert_eq!(guide_url("index"), "/docs/");
        assert_eq!(guide_url("authoring"), "/docs/authoring");
    }

    #[test]
    fn repository_links_lose_their_target_and_keep_their_text() {
        let text =
            "See [the motion rules](MOTION.md), [the site](https://richview.ai/) and [top](#top).";
        assert_eq!(
            unlink_repository_paths(text),
            "See the motion rules, [the site](https://richview.ai/) and [top](#top)."
        );
    }

    #[test]
    fn front_values_read_quoted_and_plain() {
        let source = "---\ntitle: \"Line chart\"\nstyles: [scientific]\n---\n\nbody";
        assert_eq!(front_value(source, "title").as_deref(), Some("Line chart"));
        assert_eq!(
            front_value(source, "styles").as_deref(),
            Some("[scientific]")
        );
        assert_eq!(front_value(source, "missing"), None);
    }

    #[test]
    fn the_sidebar_lists_chart_types_only_inside_the_gallery() {
        let pages = vec![ChartPage {
            name: "line".into(),
            title: "Line chart".into(),
            source: String::new(),
            use_when: String::new(),
            chart_fence: String::new(),
        }];
        // On the gallery the list moves the reader to the type on the page; from a type's own
        // page it opens the other types' pages.
        let gallery = sidebar(Place::Gallery, &pages, BuildFlags::default());
        assert!(gallery.contains("<a href=\"#line-chart\">Line chart</a>"));
        assert!(!gallery.contains("/docs/charts/line"));
        assert!(
            sidebar(Place::ChartType("line"), &pages, BuildFlags::default())
                .contains("href=\"/docs/charts/line\" aria-current=\"page\"")
        );
        assert!(
            !sidebar(Place::Guide("index"), &pages, BuildFlags::default()).contains("Line chart")
        );
        // A flow chart is an element of its own: the chart types do not list it.
        let with_flow = [ChartPage {
            name: "flow".into(),
            title: "Flow chart".into(),
            source: String::new(),
            use_when: String::new(),
            chart_fence: String::new(),
        }];
        assert!(
            !sidebar(Place::Gallery, &with_flow, BuildFlags::default()).contains("#flow-chart\"")
        );
        assert!(
            sidebar(Place::Gallery, &pages, BuildFlags::default())
                .contains("Hosted installer: at launch")
        );
        assert!(
            !sidebar(
                Place::Gallery,
                &pages,
                BuildFlags {
                    install_enabled: true,
                    ..Default::default()
                }
            )
            .contains("Hosted installer: at launch")
        );
    }

    #[test]
    fn the_sidebar_lists_the_elements_under_their_gallery_in_the_pages_order() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let page = fs::read_to_string(root.join("docs/public/elements.md")).unwrap();
        let side = sidebar(Place::Guide("elements"), &[], BuildFlags::default());
        let at = |text: &str| side.find(text).unwrap_or_else(|| panic!("no `{text}`"));
        let mut last = at("href=\"/docs/elements\" aria-current=\"page\"");
        let mut section = 0;
        for &(anchor, label) in ELEMENTS {
            // Charts and flow charts have pages of their own; the others are parts of the page.
            let link = match anchor {
                "charts" => "href=\"/docs/charts\"".to_owned(),
                "flow-charts" => "href=\"/docs/charts/flow\"".to_owned(),
                _ => format!("href=\"/docs/elements#{anchor}\""),
            };
            assert!(at(&link) > last, "{label} is out of order in the sidebar");
            last = at(&link);
            assert_eq!(reference::slug(label), anchor);
            let heading = page
                .find(&format!("\n## {label}\n"))
                .unwrap_or_else(|| panic!("the elements gallery has no `## {label}`"));
            assert!(heading > section, "{label} is out of order on the page");
            section = heading;
        }
        assert!(last < at(">Playground<"));
        assert_eq!(side.matches("aria-current").count(), 1);
        // Inside the chart gallery the charts are the current element.
        let gallery = sidebar(Place::Gallery, &[], BuildFlags::default());
        assert!(gallery.contains("href=\"/docs/charts\" aria-current=\"page\""));
        assert!(!gallery.contains("href=\"/docs/elements\" aria-current"));
        // A flow chart's and a pivot table's page sit under their element, not under the charts.
        let flow = sidebar(Place::ChartType("flow"), &[], BuildFlags::default());
        assert!(flow.contains("href=\"/docs/charts/flow\" aria-current=\"page\""));
        let pivot = sidebar(Place::ChartType("pivot"), &[], BuildFlags::default());
        assert!(pivot.contains("href=\"/docs/elements#tables\" aria-current=\"true\""));
        for side in [flow, pivot] {
            assert_eq!(side.matches("aria-current").count(), 1);
            assert!(!side.contains("docs-chart-types"));
        }
        for &(chart, element) in ELEMENT_PAGES {
            assert!(ELEMENTS.iter().any(|(anchor, _)| *anchor == element));
            assert!(
                root.join(format!("docs/public/charts/{chart}.md"))
                    .is_file()
            );
        }
        // The overview has a card for each, in the same order.
        let grid = previews::elements_grid();
        let mut last = 0;
        for &(_, label) in ELEMENTS {
            let card = grid
                .find(&format!("<h3>{label}</h3>"))
                .unwrap_or_else(|| panic!("no card for {label}"));
            assert!(card > last);
            last = card;
        }
        assert_eq!(
            grid.matches("docs-chart-card__link").count(),
            ELEMENTS.len()
        );
    }

    #[test]
    fn every_element_without_a_part_opens_a_section_of_its_guide() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let grid = previews::elements_grid();
        for (title, _, guide, anchor, _) in previews::MORE_ELEMENTS {
            let source = fs::read_to_string(root.join(format!("docs/public/{guide}.md"))).unwrap();
            assert!(
                source
                    .lines()
                    .filter_map(|line| line.strip_prefix("## "))
                    .any(|heading| reference::slug(heading) == *anchor),
                "{title}: no section #{anchor} in {guide}"
            );
            assert!(
                grid.contains(&format!("href=\"/docs/{guide}#{anchor}\"")),
                "{title}"
            );
        }
    }

    #[test]
    fn the_sidebar_sets_the_references_above_the_deeper_guides_with_the_topics_under_them() {
        let closed = sidebar(Place::Guide("index"), &[], BuildFlags::default());
        let at = |text: &str| closed.find(text).unwrap_or_else(|| panic!("no `{text}`"));
        assert!(at(">How to use<") < at(">Reference<") && at(">Reference<") < at(">Nitty-gritty<"));
        // The format reference, its topics as parts of its one page, then the two other
        // references.
        let topics = at("<details class=\"docs-nav__more\" data-nav=\"topics\">");
        assert!(at("href=\"/docs/reference\"") < topics);
        assert!(topics < at("href=\"/docs/reference#charts\""));
        assert!(!closed.contains("href=\"/docs/reference/charts\""));
        assert!(at("href=\"/docs/reference#diagnostic-codes\"") < at("href=\"/docs/tools\""));
        assert!(at("href=\"/docs/tools\"") < at("href=\"/docs/style-yaml\""));
        assert!(at("href=\"/docs/style-yaml\"") < at(">Nitty-gritty<"));
        // Every guide has one entry, and the pager walks them in the sidebar's order.
        let mut last = 0;
        for &(stem, _) in GUIDES {
            let link = format!("href=\"{}\"", guide_url(stem));
            assert_eq!(closed.matches(&link).count(), 1, "{stem}");
            assert!(at(&link) > last, "{stem} is out of the sidebar's order");
            last = at(&link);
        }
        // Inside the reference the topics are open and the page is marked.
        let open = sidebar(Place::Reference, &[], BuildFlags::default());
        assert!(open.contains("data-nav=\"topics\" open>"));
        assert!(open.contains("href=\"/docs/reference\" aria-current=\"page\""));
        assert_eq!(open.matches("aria-current").count(), 1);
    }

    #[test]
    fn the_play_chart_keeps_whole_countries_of_the_last_years_largest() {
        let csv = "year,country,region,population\n\
            1960,Small,Asia,9\n1960,\"Big, Dem. Rep.\",Africa,1\n1960,Middle,Europe,5\n\
            2020,Small,Asia,2\n2020,\"Big, Dem. Rep.\",Africa,30\n2020,Middle,Europe,7\n";
        let kept = most_populous(csv, 2).unwrap();
        assert_eq!(
            (kept.first.as_str(), kept.last.as_str(), kept.all),
            ("1960", "2020", 3)
        );
        assert_eq!(
            kept.csv,
            "year,country,region,population\n\
             1960,\"Big, Dem. Rep.\",Africa,1\n1960,Middle,Europe,5\n\
             2020,\"Big, Dem. Rep.\",Africa,30\n2020,Middle,Europe,7\n",
            "the largest of the last year keep every year's record, unchanged and in order"
        );
        assert!(
            most_populous(csv, 4).is_err(),
            "more countries than the data has"
        );
        assert!(
            most_populous("country,year\nA,1\n", 1).is_err(),
            "other columns"
        );
    }

    #[test]
    fn the_gallery_invites_play_and_zoom_with_a_cue_each() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut options = documentation_options();
        let section = interactions(&root, &mut options).unwrap();
        assert!(section.starts_with("\n## Try it yourself\n"));
        assert_eq!(section.matches("```chart\n").count(), 2);
        // The line that names the gesture stands above each chart, with the years the data has;
        // the way to the whole example and the source note follow the chart.
        let at = |text: &str| section.find(text).unwrap_or_else(|| panic!("no `{text}`"));
        let play_cue = at(&format!(
            "{CUE}Press Play to run 1960 to 2020, or drag the year."
        ));
        let play = at("\"kind\": \"playback\"");
        let every = at("](https://richview.ai/docs/charts/gapminder)");
        let zoom_cue = at(&format!("{CUE}{ZOOM_CUE}"));
        let zoom = at("Drag to zoom: a seasonal cycle");
        let finer = at("](https://richview.ai/docs/charts/german-electricity)");
        assert!(play_cue < play && play < every && every < zoom_cue);
        assert!(zoom_cue < zoom && zoom < finer);
        assert_eq!(section.matches("*Source:").count(), 2);
        // The chart opens on the last year and plays only when asked (FEA-BR-02).
        assert!(section.contains("\"initial\": \"2020\""));
        // The gallery's copies: the play chart fits a window, the zoom chart prints no values.
        assert!(section.contains("\"ratio\": \"3:2\""));
        assert!(section.contains("\"auto_annotations\": false"));
        assert!(section.contains("See all 194 countries play"));

        // The play chart carries the most populous countries only, every year of each.
        let subset = &options.data["gallery/families-and-lives/most-populous.csv"];
        let countries: std::collections::BTreeSet<&str> =
            subset.lines().skip(1).filter_map(second_field).collect();
        assert_eq!(countries.len(), PLAY_COUNTRIES);
        assert!(countries.contains("India") && countries.contains("Congo, Dem. Rep."));
        let years: std::collections::BTreeSet<&str> = subset
            .lines()
            .skip(1)
            .filter_map(|line| line.split_once(',').map(|(year, _)| year))
            .collect();
        assert_eq!(subset.lines().count(), 1 + PLAY_COUNTRIES * years.len());
        assert!(section.contains(&format!(
            "The {PLAY_COUNTRIES} most populous countries of {}, every year",
            years.last().unwrap()
        )));
        // The page stays light: both charts' observations together are a small part of what
        // the full play chart alone would add.
        let weight: usize = options.data.values().map(String::len).sum();
        assert!(weight < 120_000, "{weight} bytes of observations");

        // Both charts draw from the registered observations without a hard failure.
        let page = format!("---\ntitle: The chart gallery\nstyles: [scientific]\n---\n{section}");
        let rendered = richview_core::document::render(&page, &options).unwrap();
        ensure_clean("gallery invitation", &rendered.diagnostics).unwrap();
        assert_eq!(rendered.html.matches("data-rv-payload>").count(), 2);
        // Each cue is a paragraph of its own, which the page turns into the cue line.
        assert_eq!(rendered.html.matches(&format!("<p>{CUE}")).count(), 2);
    }

    #[test]
    fn time_series_examples_ship_copyable_sources_and_the_frozen_observations() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let scratch = root.join("tmp");
        fs::create_dir_all(&scratch).unwrap();
        let output = tempfile::tempdir_in(scratch).unwrap();
        stage_time_series(&root, output.path(), &[], BuildFlags::default()).unwrap();
        for (name, rows, field) in [
            ("co2-mauna-loa-monthly", 822, "monthly_mean"),
            ("us-cpi-components", 836, "energy"),
            ("de-power-15min", 35040, "price_eur_mwh"),
        ] {
            let folder = output.path().join("examples").join(name);
            let source = fs::read_to_string(folder.join("report.rmd")).unwrap();
            assert_eq!(
                source,
                fs::read_to_string(root.join(format!("docs/public/examples/{name}/report.rmd")))
                    .unwrap()
            );
            richview_app::reference::validate_cli_examples(&source).unwrap();
            let original = fs::read_to_string(
                root.join(format!("benchmarks/fixtures/time-series/{name}.csv")),
            )
            .unwrap();
            let data = fs::read_to_string(folder.join("data.csv")).unwrap();
            // The downloaded CSV is the complete original data body, not a sampled chart export.
            assert!(original.ends_with(&data), "{name}: observations changed");
            assert_eq!(data.lines().count(), rows + 1);
            let html = fs::read_to_string(folder.join("index.html")).unwrap();
            let payload = html
                .split("data-rv-payload>")
                .nth(1)
                .unwrap()
                .split("</script>")
                .next()
                .unwrap();
            let payload: serde_json::Value = serde_json::from_str(payload).unwrap();
            assert_eq!(payload["rows"].as_array().unwrap().len(), rows);
            let columns = payload["columns"].as_array().unwrap();
            let y = columns.iter().position(|column| column == field).unwrap();
            if name == "us-cpi-components" {
                let month = columns.iter().position(|column| column == "month").unwrap();
                let gap = payload["rows"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|row| row[month] == "2025-10")
                    .unwrap();
                assert!(gap[y].is_null(), "The missing CPI month must remain a gap");
            }
            if name == "de-power-15min" {
                assert!(
                    payload["shown"][field]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|price| price.as_f64().is_some_and(|price| price < 0.0)),
                    "Negative electricity prices must survive rendering"
                );
            }
        }
    }
}
