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
}

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Parser, Tag, TagEnd};
use richview_core::HighlightLanguage;
use richview_core::document::{RenderOptions, ReportFolder};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// The task guides, in reading order: file stem under `docs/`, label.
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
    ("authoring", "Rich Markdown"),
    ("blocks", "Blocks & media"),
    ("chart-reference", "Charts & interaction"),
    ("data", "Data & connectors"),
    ("storage", "Asset storage"),
    ("style-yaml", "Style YAML reference"),
    ("recovery", "Diagnostics & repair"),
    ("access", "Server, access and API"),
    ("architecture", "Architecture & performance"),
    ("tools", "CLI & MCP reference"),
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
        &["report.rmd", "data/life-expectancy-by-region.csv"],
    ),
    (
        "air-we-changed",
        &["report.rmd", "data/global-temperature.csv"],
    ),
    (
        "same-numbers",
        &["report.rmd", "data/anscombe.csv", "data/old-faithful.csv"],
    ),
    ("titanic", &["report.rmd", "data/titanic.csv"]),
    (
        "first-charts",
        &[
            "report.rmd",
            "data/selected.csv",
            "data/wheat-and-wages.csv",
            "data/nightingale.csv",
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
            source.push_str(&body.replace("media/", "gallery/blocks/media/"));
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
        fs::write(folder.join("index.html"), html)?;
    }
    Ok(())
}

/// Every style leaf comes from the same closed grammar the loader validates.
fn style_reference() -> String {
    let mut out = String::new();
    for mark in [
        richview_core::StyleMark::Safe,
        richview_core::StyleMark::Experimental,
    ] {
        out.push_str(&format!(
            "\n## {} leaves\n\n| Path | Accepted value |\n| --- | --- |\n",
            if mark == richview_core::StyleMark::Safe {
                "Safe"
            } else {
                "Experimental"
            }
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
    }
}

/// Embed the actual export, so each report keeps its own style and working interactions.
fn report_frames(html: &str) -> String {
    let mut html = html.to_owned();
    // The flagship opens on its showcase page, where the reader can switch its style.
    let url = previews::FLAGSHIP_URL;
    html = html.replace(
        &format!("<p>{{{{report:{}}}}}</p>", previews::FLAGSHIP),
        &format!(
            "<div class=\"docs-report-preview\"><div class=\"docs-report-preview__viewport\"><iframe loading=\"lazy\" title=\"Live report: {name}\" src=\"/docs/previews/report-{name}/{style}.html#rv-chart-0\" sandbox=\"allow-scripts allow-downloads\" data-docs-preview></iframe></div><div class=\"docs-actions\"><a class=\"docs-button\" href=\"{url}\">Open live report</a><a class=\"docs-button docs-button--secondary\" href=\"{url}report.rmd\">Rich Markdown source</a><a href=\"{url}ledger/sources.csv\">Sources</a></div></div>",
            name = previews::FLAGSHIP,
            style = previews::style(previews::FLAGSHIP)
        ),
    );
    for &(name, _) in REPORTS {
        let marker = format!("<p>{{{{report:{name}}}}}</p>");
        let url = format!("/docs/gallery/{name}");
        let preview = if matches!(name, "blocks" | "typography") {
            format!(
                "<iframe loading=\"lazy\" title=\"Live report: {name}\" src=\"{url}/Rendered/report.html\" sandbox=\"allow-scripts allow-downloads\"></iframe>"
            )
        } else {
            format!(
                "<div class=\"docs-report-preview__viewport\"><iframe title=\"Live report: {}\" data-src=\"/docs/previews/report-{name}/{style}.html#rv-chart-0\" sandbox=\"allow-scripts allow-downloads\" data-docs-preview></iframe></div>",
                escape(name),
                style = previews::style(name)
            )
        };
        let frame = format!(
            "<div class=\"docs-report-preview\">{preview}<div class=\"docs-actions\"><a class=\"docs-button\" href=\"{url}/Rendered/report.html\">Open live report</a><a class=\"docs-button docs-button--secondary\" href=\"{url}/report.rmd\">Rich Markdown source</a><a href=\"{url}/\">Data &amp; credits</a></div></div>"
        );
        html = html.replace(&marker, &frame);
    }
    html
}

fn typography_frames() -> String {
    format!("<div class=\"docs-type-grid\">{}</div>", chrome::site_styles().iter().map(|style| format!("<section class=\"docs-type-preview\"><h3>{}</h3><iframe loading=\"lazy\" title=\"{} typography and elements\" src=\"/docs/gallery/typography/Rendered/typography-{style}.html\" sandbox=\"allow-scripts allow-downloads\"></iframe><a href=\"/docs/gallery/typography/Rendered/typography-{style}.html\">Open the {} sample</a></section>", chrome::style_label(style), chrome::style_label(style), chrome::style_label(style))).collect::<String>())
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
    index.insert_str(featured_at, &showcase(root)?);
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
        fs::write(folder.join(format!("{slug}.html")), html)?;
        fs::write(folder.join(format!("{slug}.md")), featured)?;
    }
    index.push_str("\n## Every chart type\n\nKeep scrolling: every supported type is rendered here. Each also has a deep link with its complete source, important fields and style comparison. These compact examples use illustrative data.\n");
    for page in charts {
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
    let html = render_page(&index, "charts", Place::Gallery, charts, flags)?;
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
        for page in charts {
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
    let decorated = decorated.replace("<p>{{typography}}</p>", &typography_frames());
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
    html.push_str(&link(
        "/docs/charts",
        "Chart gallery",
        matches!(
            place,
            Place::Gallery | Place::Featured(_) | Place::Example(_) | Place::ChartType(_)
        ),
    ));
    let in_gallery = matches!(
        place,
        Place::Gallery | Place::Featured(_) | Place::ChartType(_) | Place::Example(_)
    );
    if in_gallery {
        html.push_str("<details class=\"docs-chart-types\"><summary>Chart types</summary><div class=\"docs-nav__sub\">");
        for page in charts {
            html.push_str(&link(
                &format!("/docs/charts/{}", page.name),
                &page.title,
                place == Place::ChartType(&page.name),
            ));
        }
        html.push_str("</div></details>");
    }
    html.push_str(&link(
        "/docs/elements",
        "Elements gallery",
        place == Place::Guide("elements"),
    ));
    html.push_str("<p class=\"docs-nav__group\">Playground</p>");
    html.push_str(&link(
        "/docs/playground",
        "Using the playground",
        place == Place::Guide("playground"),
    ));
    html.push_str(&link("/playground", "Open playground", false));
    html.push_str("<p class=\"docs-nav__group\">How to use</p>");
    for (stem, label) in GUIDES.iter().filter(|(stem, _)| {
        !matches!(
            *stem,
            "index"
                | "elements"
                | "playground"
                | "authoring"
                | "blocks"
                | "chart-reference"
                | "data"
                | "storage"
                | "style-yaml"
                | "recovery"
                | "access"
                | "architecture"
                | "tools"
        )
    }) {
        html.push_str(&link(&guide_url(stem), label, place == Place::Guide(stem)));
    }
    html.push_str("<p class=\"docs-nav__group\">Nitty-gritty</p>");
    for (stem, label) in GUIDES.iter().filter(|(stem, _)| {
        matches!(
            *stem,
            "authoring"
                | "blocks"
                | "chart-reference"
                | "data"
                | "storage"
                | "style-yaml"
                | "recovery"
                | "access"
                | "architecture"
                | "tools"
        )
    }) {
        html.push_str(&link(&guide_url(stem), label, place == Place::Guide(stem)));
    }
    html.push_str(&link(
        "/docs/reference",
        "Format reference",
        place == Place::Reference,
    ));
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
        assert!(html.contains("src=\"/docs/previews/report-openai-vs-anthropic/"));
        assert!(!html.contains("{{report:"));
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
            assert_eq!(
                gallery.matches("data-rv-payload>").count(),
                read_chart_pages(&root).unwrap().len() + 1,
                "the gallery opens with the flagship showcase and keeps every chart type inline after the featured menu"
            );
            let showcase = gallery.find(SHOWCASE_TITLE).unwrap();
            assert!(showcase < gallery.find("docs-featured-grid").unwrap());
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
        assert!(
            sidebar(Place::Gallery, &pages, BuildFlags::default()).contains("/docs/charts/line")
        );
        assert!(
            !sidebar(Place::Guide("index"), &pages, BuildFlags::default())
                .contains("/docs/charts/line")
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
