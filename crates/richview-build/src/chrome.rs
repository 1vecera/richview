//! The one header and footer of every site page (FEA-SF-28, STY-46). Pages carry the markers
//! `<!-- rv-site-header -->` and `<!-- rv-site-footer -->`; staging replaces them, so no page
//! keeps its own navigation. A link here must point at a route the site serves (FEA-SF-62).

const MARK: &str = r##"<svg viewBox="0 0 268.58 269.73" aria-hidden="true"><g fill="#1A6DAC"><path d="M221.4,51.39 H0 V0 h221.4 c7.32,0 13.25,5.93 13.25,13.25 v24.89 c0,7.32 -5.93,13.25 -13.25,13.25 z"/><path d="M255.32,124.17 H0 V72.78 h255.32 c7.32,0 13.25,5.93 13.25,13.25 v24.89 c0,7.32 -5.93,13.25 -13.25,13.25 z"/><path d="M221.4,196.95 H0 v-51.39 h221.4 c7.32,0 13.25,5.93 13.25,13.25 v24.89 c0,7.32 -5.93,13.25 -13.25,13.25 z"/><path d="M93.12,269.73 H0 v-51.39 h93.12 c7.32,0 13.25,5.93 13.25,13.25 v24.89 c0,7.32 -5.93,13.25 -13.25,13.25 z"/><circle cx="242.88" cy="244.04" r="25.7"/></g></svg>"##;

/// The footer links, in order. The playground joins at launch (FEA-SF-33).
pub const LINKS: [(&str, &str); 3] = [
    ("/showcase/openai-vs-anthropic/", "Flagship report"),
    ("/docs/charts", "Chart gallery"),
    ("/docs/", "Docs"),
];
/// The header links, in order (Daniel, 9 Nov): the docs and the gallery, then the repository as an icon.
pub const HEADER_LINKS: [(&str, &str); 2] = [("/docs/", "Docs"), ("/docs/charts", "Gallery")];
/// The repository the header's icon link opens.
const REPOSITORY: &str = "https://github.com/1vecera/richview";
/// The hosted app's sign-in, the header's last and quietest link (Daniel, 15 Nov).
const APP_SIGN_IN: &str = "https://app.richview.ai/";
/// The GitHub mark (Octicons), drawn in the link's own colour.
const GITHUB_MARK: &str = r##"<svg viewBox="0 0 16 16" aria-hidden="true" focusable="false"><path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0 0 16 8c0-4.42-3.58-8-8-8z"/></svg>"##;

fn links() -> String {
    LINKS
        .iter()
        .map(|(href, label)| format!("<a href=\"{href}\">{label}</a>"))
        .collect()
}

/// The header's links: the docs and the gallery, the repository as a small muted icon, and the
/// hosted app's sign-in set apart at the end.
fn header_links() -> String {
    let words: String = HEADER_LINKS
        .iter()
        .map(|(href, label)| format!("<a href=\"{href}\">{label}</a>"))
        .collect();
    format!(
        "{words}<a class=\"nav__github\" href=\"{REPOSITORY}\" aria-label=\"RichView on GitHub\">{GITHUB_MARK}</a><a class=\"nav__signin\" href=\"{APP_SIGN_IN}\">Sign in</a>"
    )
}

/// The site header: the mark and the links.
pub fn header() -> String {
    header_with("")
}

/// The site header with `slot` between the mark and the links, laid out by the header's own row.
/// The flagship page puts its phone style menu there (Daniel, 17 Nov 2026: "top area is all single
/// line with a dropdown to change styles"); every other page's header has no slot.
pub fn header_with(slot: &str) -> String {
    format!(
        "<header class=\"nav\"><a class=\"nav__brand\" href=\"/\" aria-label=\"RichView home\">{MARK}RichView</a>{slot}<nav class=\"nav__links\" aria-label=\"Main navigation\">{}</nav></header>",
        header_links()
    )
}

pub fn footer() -> String {
    format!(
        "<footer class=\"foot\"><span>RichView · Daniel Vecera</span><div>{}<a href=\"/about\">About</a><a href=\"/impressum\">Impressum</a><a href=\"/privacy\">Privacy</a><a href=\"/terms\">Terms</a><a href=\"/licenses\">Font licences</a><a href=\"/third-party-notices.txt\">Third-party notices</a></div></footer>",
        links()
    )
}

/// Replace the chrome markers of a page.
pub fn apply(html: &str) -> String {
    apply_with(html, "")
}

/// Replace the chrome markers of a page whose header carries `slot` (see [`header_with`]).
pub fn apply_with(html: &str, slot: &str) -> String {
    let page = html
        .replace("<!-- rv-site-header -->", &header_with(slot))
        .replace("<!-- rv-site-footer -->", &footer());
    share_metadata(&page)
}

/// Every site page carries a link preview (FEA-SF-55): pages that declare their own `og:image`
/// keep it, the rest take the home image with the page's own title and description.
pub fn share_metadata(page: &str) -> String {
    if page.contains("property=\"og:image\"") || !page.contains("</head>") {
        return page.to_owned();
    }
    let between = |open: &str, close: &str| {
        let start = page.find(open)? + open.len();
        let end = page[start..].find(close)? + start;
        Some(page[start..end].trim().to_owned())
    };
    let title = between("<title>", "</title>").unwrap_or_else(|| "RichView".into());
    let description = between("name=\"description\" content=\"", "\"").unwrap_or_default();
    // A page that already declares a tag, as the report shell does for its title and type, keeps it.
    let mut tags = String::new();
    let mut add = |property: &str, tag: String| {
        if !page.contains(&format!("property=\"{property}\"")) {
            tags.push_str(&tag);
        }
    };
    add(
        "og:type",
        "<meta property=\"og:type\" content=\"website\" />\n\t\t".into(),
    );
    add(
        "og:site_name",
        "<meta property=\"og:site_name\" content=\"RichView\" />\n\t\t".into(),
    );
    add(
        "og:title",
        format!("<meta property=\"og:title\" content=\"{title}\" />\n\t\t"),
    );
    if !description.is_empty() {
        add(
            "og:description",
            format!("<meta property=\"og:description\" content=\"{description}\" />\n\t\t"),
        );
    }
    tags.push_str("<meta property=\"og:image\" content=\"https://richview.ai/share/home.png\" />\n\t\t<meta property=\"og:image:width\" content=\"1200\" />\n\t\t<meta property=\"og:image:height\" content=\"630\" />\n\t\t<meta name=\"twitter:card\" content=\"summary_large_image\" />\n\t");
    page.replacen("</head>", &format!("{tags}</head>"), 1)
}

/// A themes-card tile in a builtin's own tokens, resolved by core: paper, ink, rule, accent, the
/// display face and the small radius (STY-58). No colour or face is written down here.
fn tile_style(name: &str) -> String {
    let style = richview_core::resolve_styles(&[name.to_owned()], &mut Vec::new());
    let token = |key: &str| {
        style
            .custom_properties
            .get(&format!("--rv-{key}"))
            .map(|value| value.replace(['"', ';', '<', '>'], ""))
            .unwrap_or_default()
    };
    format!(
        "--tile-bg:{};--tile-ink:{};--tile-rule:{};--tile-accent:{};--tile-font:{};--tile-radius:{}",
        token("bg"),
        token("ink"),
        token("rule"),
        token("accent"),
        token("font-display"),
        token("radius-s"),
    )
}

/// The styles the site names: core's builtins, in core's order. Core owns the list (DEL-18), so
/// a style added or renamed there reaches every page at the next build.
pub fn site_styles() -> Vec<&'static str> {
    richview_core::builtin_style_names().to_vec()
}

/// A style's display name: "Sleek", "Air", "Hand-drawn".
pub fn style_label(name: &str) -> String {
    let mut label = String::new();
    for (i, word) in name.split('-').enumerate() {
        let mut chars = word.chars();
        let Some(first) = chars.next() else { continue };
        if i == 0 {
            label.extend(first.to_uppercase());
        } else if name == "hand-drawn" {
            label.push('-');
            label.push(first);
        } else {
            label.push(' ');
            label.extend(first.to_uppercase());
        }
        label.push_str(chars.as_str());
    }
    label
}

/// "Sleek, Air, Scientific, Brutalist and Hand-drawn".
fn style_list(styles: &[&str]) -> String {
    let labels: Vec<String> = styles.iter().map(|s| style_label(s)).collect();
    match labels.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        _ => labels.join(""),
    }
}

/// The first report of the size proof: the HTML a coding agent wrote and the Rich Markdown source of
/// the same analysis. The home page's proof pair shows the first lines of each.
const PROOF_HTML: &str = include_str!("../../../examples/size-proof/01-trends/agent-report.html");
const PROOF_RMD: &str = include_str!("../../../examples/size-proof/01-trends/report.rmd");
/// How many lines of each file the page carries; the card shows as many as fit.
const PROOF_LINES: usize = 26;

/// The words the home page's prompt boxes hold, and what the box says above them (Daniel, 9 Nov).
const PROMPT_LABEL: &str = "Let your agent make your first report.";
pub(crate) const AGENT_PROMPT: &str = "Install RichView from https://github.com/1vecera/richview/releases/latest and run richview learn. Then make me a beautiful report from my data, or from the starter example if I have none. Check it, fix anything it flags, and show me the result. Keep everything on my machine.";
pub(crate) const AGENT_PROMPT_INSTALL_OFF: &str = "If RichView is installed, run richview learn. Then make me a beautiful report from my data, or from the starter example if I have none. Check it, fix anything it flags, and show me the result. Keep everything on my machine.";

pub(crate) fn agent_prompt(install_enabled: bool) -> &'static str {
    if install_enabled {
        AGENT_PROMPT
    } else {
        AGENT_PROMPT_INSTALL_OFF
    }
}

/// The prompt box of the home page, used in its first screen and again at its close: the words to
/// paste into a coding agent, and a Copy button the site runtime answers (a press that starts the
/// runtime is answered when it has started). Without scripts the words stay selectable text.
fn prompt_box() -> String {
    format!(
        "<div class=\"prompt\" data-prompt><p class=\"prompt__label\">{PROMPT_LABEL}</p><div class=\"prompt__box\"><pre class=\"prompt__text\" tabindex=\"0\"><code>{AGENT_PROMPT}</code></pre><button class=\"prompt__copy\" type=\"button\" data-copy>Copy</button></div><p class=\"prompt__status\" role=\"status\" aria-live=\"polite\"></p></div>"
    )
}

/// The example company's style file, shown in the brand section beside the report it makes.
const BRAND_STYLE: &str = include_str!("../../../assets/landing/brand/company.rstyle.yaml");
/// The example report's data: the best elements' chart specimen draws it.
const ANCHOR_DATA: &str =
    include_str!("../../../assets/landing/anchor/data/life-expectancy-by-region.csv");

/// Text escaped for an HTML element.
fn escape_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The best elements' chart specimen: the example's regions, 1950 to 2019, as one small SVG line
/// chart with a label at each line's end and the famine's dip marked. The site draws it from the CSV
/// the example report reads, so it shows the report's own data; its accessible name says what the
/// lines show, from the same rows.
fn elements_chart() -> String {
    let mut series: Vec<(&str, Vec<(f64, f64)>)> = Vec::new();
    for row in ANCHOR_DATA.lines().skip(1) {
        let mut fields = row.split(',').map(str::trim);
        let (Some(year), Some(region), Some(life)) = (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let (Ok(year), Ok(life)) = (year.parse::<f64>(), life.parse::<f64>()) else {
            continue;
        };
        match series.iter_mut().find(|(name, _)| *name == region) {
            Some((_, points)) => points.push((year, life)),
            None => series.push((region, vec![(year, life)])),
        }
    }
    let (first, last, low, high) = (1950.0, 2019.0, 40.0, 80.0);
    let (left, right, top, bottom) = (30.0, 540.0, 8.0, 250.0);
    let x = |year: f64| left + (year - first) / (last - first) * (right - left);
    let y = |value: f64| bottom - (value - low) / (high - low) * (bottom - top);
    let mut grid = String::new();
    for value in [40.0, 50.0, 60.0, 70.0, 80.0] {
        grid.push_str(&format!(
            "<line class=\"grid\" x1=\"{left}\" x2=\"{right}\" y1=\"{at:.1}\" y2=\"{at:.1}\"/><text class=\"tick\" x=\"0\" y=\"{text:.1}\">{value}</text>",
            at = y(value),
            text = y(value) + 4.0
        ));
    }
    for year in [1950.0, 1970.0, 1990.0, 2010.0] {
        grid.push_str(&format!(
            "<text class=\"tick\" x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\">{year}</text>",
            x(year),
            bottom + 22.0
        ));
    }
    let mut lines = String::new();
    let mut ends: Vec<(f64, &str)> = Vec::new();
    let mut said: Vec<String> = Vec::new();
    for (name, points) in &series {
        let class = name.to_lowercase();
        let path: Vec<String> = points
            .iter()
            .map(|(year, value)| format!("{:.1},{:.1}", x(*year), y(*value)))
            .collect();
        lines.push_str(&format!(
            "<polyline class=\"series series--{class}\" points=\"{}\"/>",
            path.join(" ")
        ));
        if let (Some((_, start)), Some((_, end))) = (points.first(), points.last()) {
            ends.push((y(*end), name));
            said.push(format!("{name} from {start:.1} to {end:.1}"));
        }
    }
    // A label at each line's end, moved down where it would touch the one above.
    ends.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut labels = String::new();
    let mut above = f64::NEG_INFINITY;
    for (at, name) in ends {
        let at = at.max(above + 15.0);
        above = at;
        labels.push_str(&format!(
            "<text class=\"name name--{}\" x=\"{:.1}\" y=\"{:.1}\">{name}</text>",
            name.to_lowercase(),
            right + 8.0,
            at + 4.0
        ));
    }
    // The sharp dip the example report annotates.
    if let Some((year, value)) = series
        .iter()
        .flat_map(|(_, points)| points.iter())
        .copied()
        .min_by(|a, b| a.1.total_cmp(&b.1))
    {
        labels.push_str(&format!(
            "<circle class=\"grid\" cx=\"{:.1}\" cy=\"{:.1}\" r=\"4\" fill=\"none\"/><text class=\"note\" x=\"{:.1}\" y=\"{:.1}\">Famine in China, 1959&#8211;61</text>",
            x(year),
            y(value),
            x(year) + 9.0,
            y(value) + 4.0
        ));
    }
    let description = format!(
        "A line chart of life expectancy at birth, 1950 to 2019, in years: {}.",
        said.join(", ")
    );
    format!(
        "<svg class=\"spec-chart\" viewBox=\"0 0 640 280\" role=\"img\" aria-label=\"{description}\">{grid}{lines}{labels}</svg>"
    )
}

/// The best elements' colour specimen: each style's accent, read from core's tokens, for three soft
/// tints the page mixes from it (Daniel, 9 Nov: soft colours), so a style that changes its accent
/// changes them here at the next build.
fn elements_palettes() -> String {
    site_styles()
        .iter()
        .map(|name| {
            let style = richview_core::resolve_styles(&[(*name).to_owned()], &mut Vec::new());
            let token = |key: &str| {
                style
                    .custom_properties
                    .get(&format!("--rv-{key}"))
                    .map(|value| value.replace(['"', ';', '<', '>'], ""))
                    .unwrap_or_default()
            };
            let accent = token("accent");
            format!(
                "<li style=\"--chip:{accent}\"><span>{}</span><i></i><i></i><i></i></li>",
                style_label(name)
            )
        })
        .collect()
}

/// The first lines of a file, escaped for a code element.
fn first_lines(file: &str) -> String {
    file.lines()
        .take(PROOF_LINES)
        .collect::<Vec<_>>()
        .join("\n")
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Where each style's page starts: at the report's first chart, whose title stands just above the lines,
/// so a tile shows the whole chart in its style and not only the header photo and title. The fragment
/// scrolls the frame itself, and the report's file (so its download) is the same one the live example reads.
const VOICES_VIEW: &str = "#rv-chart-0";

/// Render the hosted call to action only when an HTTPS origin is configured at build time.
fn hosted_section(origin: Option<&str>) -> String {
    let Some(origin) = origin.map(str::trim).filter(|s| !s.is_empty()) else {
        return String::new();
    };
    let Some(host) = origin.strip_prefix("https://") else {
        return String::new();
    };
    let host = host.trim_end_matches('/');
    if host.is_empty()
        || !host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':'))
    {
        return String::new();
    }
    format!(
        "<section class=\"ecosystem hosted\" id=\"hosted\" aria-labelledby=\"h-hosted\"><header class=\"section-head\"><h2 id=\"h-hosted\">Publish and share your reports.</h2><p class=\"section-lede\">Publish a report from your agent and share it with your colleagues. The hosted server is free while it is early.</p><p><a class=\"btn btn--primary\" href=\"https://{host}/sign-up\">Create an account &#8594;</a></p></header><div class=\"hosted__capture\"><img src=\"/landing/hosted-reports.png\" alt=\"A hosted reports list showing an organization report and a private report\" loading=\"lazy\" width=\"1280\" height=\"900\"></div></section>"
    )
}

/// Replace the placeholders of the home page: the demo's pills, the name list, the style tiles, the
/// styles row, the prompt box, the proof pair's lines, the brand's style file and the best elements'
/// chart and colours (FEA-SF-30, STY-08, STY-58).
pub fn apply_styles(html: &str) -> String {
    let styles = site_styles();
    let pills: String = styles
        .iter()
        .enumerate()
        .map(|(i, name)| {
            format!(
                "<button type=\"button\" role=\"radio\" aria-checked=\"{}\" data-theme=\"{name}\"{}>{}</button>",
                i == 0,
                if i == 0 { "" } else { " tabindex=\"-1\"" },
                style_label(name)
            )
        })
        .collect();
    let tiles: String = styles
        .iter()
        .map(|name| {
            format!(
                "<i class=\"tile\" style=\"{}\" title=\"{}\">Aa</i>",
                tile_style(name),
                style_label(name)
            )
        })
        .collect();
    let links: String = styles
        .iter()
        .map(|name| {
            let query = if *name == "sleek" {
                String::new()
            } else {
                format!("?theme={name}")
            };
            format!(
                "<a href=\"/showcase/openai-vs-anthropic/{query}\">{}</a>",
                style_label(name)
            )
        })
        .collect();
    // The playground starts with no pill chosen: the source's own styles apply (FEA-SF-34).
    let unchosen: String = styles
        .iter()
        .enumerate()
        .map(|(i, name)| {
            format!(
                "<button type=\"button\" role=\"radio\" aria-checked=\"false\" data-theme=\"{name}\"{}>{}</button>",
                if i == 0 { "" } else { " tabindex=\"-1\"" },
                style_label(name)
            )
        })
        .collect();
    // The voices scene: the anchor report in each style, as the build stages it, and the rail that
    // names the style on screen. A page's holder names its report, and the site runtime
    // makes the frame as the reader comes near: Chromium loads a lazy frame up to about
    // 2,500 px away, so a lazy frame in the markup would load all five reports, and the report
    // runtime they link, on the first screen (PRF-36). Without JavaScript the `<noscript>` frame
    // stands in, and the browser loads it lazily.
    let rail: String = styles
        .iter()
        .map(|name| format!("<li data-voice=\"{name}\">{}</li>", style_label(name)))
        .collect();
    let pages: String = styles
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let page = if i == 0 {
                "anchor".to_owned()
            } else {
                format!("anchor-{name}")
            };
            let label = style_label(name);
            format!(
                "<figure class=\"voices__sheet\" data-voice=\"{name}\"><div class=\"voices__page\" inert data-src=\"/landing/{page}{VOICES_VIEW}\" data-title=\"Longer lives in {label}\"><noscript><iframe src=\"/landing/{page}{VOICES_VIEW}\" loading=\"lazy\" scrolling=\"no\" tabindex=\"-1\" title=\"Longer lives in {label}\"></iframe></noscript></div><figcaption>{label}</figcaption></figure>"
            )
        })
        .collect();
    html.replace(
        "<!-- rv-hosted-server -->",
        &hosted_section(std::env::var("RICHVIEW_HOSTED_ORIGIN").ok().as_deref()),
    )
    .replace("<!-- rv-style-pills-unchosen -->", &unchosen)
    .replace("<!-- rv-style-pills -->", &pills)
    .replace("<!-- rv-style-list -->", &style_list(&styles))
    .replace("<!-- rv-style-tiles -->", &tiles)
    .replace("<!-- rv-flagship-style-links -->", &links)
    .replace("<!-- rv-voices-rail -->", &rail)
    .replace("<!-- rv-voices-pages -->", &pages)
    .replace("<!-- rv-prompt-box -->", &prompt_box())
    .replace("<!-- rv-agent-prompt -->", AGENT_PROMPT)
    .replace("<!-- rv-proof-html -->", &first_lines(PROOF_HTML))
    .replace("<!-- rv-proof-rmd -->", &first_lines(PROOF_RMD))
    .replace(
        "<!-- rv-brand-style -->",
        &escape_text(BRAND_STYLE.trim_end()),
    )
    .replace("<!-- rv-elements-chart -->", &elements_chart())
    .replace("<!-- rv-elements-palettes -->", &elements_palettes())
}

/// The page's voices: one block for each builtin, from core's tokens, which the voices
/// scene switches with `data-voice` on `<html>`. A voice brings its faces and its blue to the page,
/// and its paper and ink only when it is light: the site is light only (STY-47), so while a dark
/// style is on screen only its report is dark. The blue is the style's brand shade, which the mark
/// keeps (STY-01); a dark style's brand shade is drawn for dark paper, so it brings its accent.
/// Each voice's report sheet also waits on that style's paper until its report has drawn.
pub fn voices_css() -> String {
    let mut css = String::from(
        "/* Generated by richview-build from core's style tokens; edit the styles, not this file. */\n",
    );
    for name in site_styles() {
        let style = richview_core::resolve_styles(&[name.to_owned()], &mut Vec::new());
        let token = |key: &str| {
            style
                .custom_properties
                .get(&format!("--rv-{key}"))
                .map(|value| value.replace(['"', ';', '<', '>', '{', '}'], ""))
                .unwrap_or_default()
        };
        let light = style.scheme != "dark";
        let blue = token(if light { "brand" } else { "accent" });
        let mut page = vec![("--blue", blue.clone()), ("--blue-ink", blue)];
        if light {
            page.extend([
                ("--paper", token("bg")),
                ("--paper-2", token("bg-elev")),
                ("--panel", token("bg")),
                ("--ink", token("ink")),
                ("--soft", token("ink-soft")),
                ("--faint", token("ink-faint")),
                ("--rule", token("rule")),
            ]);
        }
        let declarations = |pairs: &[(&str, String)]| {
            pairs
                .iter()
                .filter(|(_, value)| !value.is_empty())
                .map(|(key, value)| format!("{key}:{value}"))
                .collect::<Vec<_>>()
                .join(";")
        };
        css.push_str(&format!(
            ":root[data-voice=\"{name}\"]{{{}}}\n",
            declarations(&page)
        ));
        // Faces change from the scene down, so nothing above the reader moves. The header keeps the
        // site's type and takes only the voice's colour: its links never reflow while the reader
        // scrolls, and a monospaced voice would crowd them on a phone.
        let faces = [
            ("--font-display", token("font-display")),
            ("--font-ui", token("font-ui")),
            ("--font-mono", token("font-mono")),
        ];
        css.push_str(&format!(
            ":root[data-voice=\"{name}\"] :is(.voices,.voices~*,.foot){{{}}}\n",
            declarations(&faces)
        ));
        css.push_str(&format!(
            ".voices__sheet[data-voice=\"{name}\"] .voices__page{{background:{}}}\n",
            token("bg")
        ));
    }
    css
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_marker_is_replaced_once() {
        let page = apply("<body><!-- rv-site-header -->x<!-- rv-site-footer --></body>");
        assert!(!page.contains("rv-site-"));
        assert_eq!(page.matches("class=\"nav\"").count(), 1);
        assert_eq!(page.matches("class=\"foot\"").count(), 1);
    }

    #[test]
    fn the_footer_links_the_about_page_before_the_legal_pages() {
        let footer = footer();
        let about = footer
            .find("href=\"/about\">About</a>")
            .expect("About link");
        assert!(about < footer.find("href=\"/impressum\"").unwrap());
        let page = apply(include_str!("../../../assets/about.html"));
        assert!(!page.contains("rv-site-"));
        assert!(page.contains("founder of RichView"));
        assert!(page.contains("href=\"https://github.com/1vecera/richview\""));
    }

    #[test]
    fn the_voices_scene_has_one_page_and_one_rail_name_for_each_style() {
        let page =
            apply_styles("<ol><!-- rv-voices-rail --></ol><div><!-- rv-voices-pages --></div>");
        assert!(!page.contains("rv-voices-"), "{page}");
        let styles = site_styles();
        assert_eq!(page.matches("<li data-voice=").count(), styles.len());
        assert_eq!(
            page.matches("class=\"voices__sheet\"").count(),
            styles.len()
        );
        // The first style's page is the one the live example's scene writes, so the scene reuses its
        // download.
        assert!(
            page.contains(&format!("data-src=\"/landing/anchor{VOICES_VIEW}\"")),
            "{page}"
        );
        for name in &styles[1..] {
            assert!(
                page.contains(&format!("data-src=\"/landing/anchor-{name}{VOICES_VIEW}\"")),
                "{name}"
            );
        }
        // No frame loads with the page: each one in the markup waits inside `<noscript>`, for
        // readers without JavaScript.
        assert_eq!(page.matches("<iframe ").count(), styles.len());
        assert_eq!(
            page.matches("<noscript><iframe ").count(),
            styles.len(),
            "{page}"
        );
    }

    #[test]
    fn a_voice_brings_paper_and_ink_only_when_its_style_is_light() {
        let css = voices_css();
        for name in site_styles() {
            let style = richview_core::resolve_styles(&[name.to_owned()], &mut Vec::new());
            let block = css
                .lines()
                .find(|line| line.starts_with(&format!(":root[data-voice=\"{name}\"]{{")))
                .unwrap_or_else(|| panic!("{name} has no block: {css}"));
            assert!(block.contains("--blue:oklch("), "{block}");
            // The site is light only (STY-47): a dark style leaves the page's paper and ink alone.
            let light = style.scheme != "dark";
            assert_eq!(block.contains("--paper:"), light, "{block}");
            assert_eq!(block.contains("--ink:"), light, "{block}");
            let faces = css
                .lines()
                .find(|line| line.starts_with(&format!(":root[data-voice=\"{name}\"] :is(")))
                .unwrap_or_else(|| panic!("{name} has no faces: {css}"));
            assert!(
                faces.contains("--font-display:") && faces.contains("--font-ui:"),
                "{faces}"
            );
        }
        assert!(site_styles().iter().any(|name| {
            richview_core::resolve_styles(&[(*name).to_owned()], &mut Vec::new()).scheme == "dark"
        }));
    }

    #[test]
    fn tiles_take_their_tokens_from_core() {
        let page = apply_styles("<!-- rv-style-tiles -->");
        for name in site_styles() {
            assert!(
                page.contains(&format!("title=\"{}\"", style_label(name))),
                "{name}"
            );
        }
        assert_eq!(
            page.matches("--tile-bg:oklch(").count(),
            site_styles().len(),
            "{page}"
        );
        assert!(!page.contains("--tile-font:;"), "{page}");
    }

    #[test]
    fn style_labels_and_list_follow_core() {
        assert_eq!(style_label("air"), "Air");
        assert_eq!(style_label("hand-drawn"), "Hand-drawn");
        assert_eq!(style_label("scientific"), "Scientific");
        let page = apply_styles("<!-- rv-style-pills --><!-- rv-style-list -->");
        for name in site_styles() {
            assert!(page.contains(&format!("data-theme=\"{name}\"")), "{name}");
        }
        assert!(page.contains(" and Hand-drawn"));
    }

    #[test]
    fn the_header_has_the_docs_the_gallery_the_repository_and_sign_in_but_no_signup() {
        let header = header();
        assert_eq!(header.matches("<a ").count(), 5, "{header}");
        assert!(header.find(">Docs<").unwrap() < header.find(">Gallery<").unwrap());
        let github = &header[header.find("nav__github").unwrap()..];
        assert!(github.contains("href=\"https://github.com/1vecera/richview\""));
        assert!(github.contains("aria-label=\"RichView on GitHub\""));
        // Sign-in to the hosted app is the last link; the header still has no sign-up button.
        assert!(header.ends_with(
            "<a class=\"nav__signin\" href=\"https://app.richview.ai/\">Sign in</a></nav></header>"
        ));
        assert!(!header.contains("#signup"));
        assert!(!header.contains("sign-up"));
        // The footer keeps the flagship report, which the header no longer names.
        assert!(footer().contains("Flagship report"));
    }

    #[test]
    fn the_header_has_no_motto() {
        assert!(!header().contains("motto"));
        assert!(!header().contains("Let your agent present"));
    }

    #[test]
    fn the_prompt_box_holds_the_instruction_and_its_label_twice() {
        assert!(AGENT_PROMPT.starts_with(
            "Install RichView from https://github.com/1vecera/richview/releases/latest"
        ));
        let page = apply_styles("<!-- rv-prompt-box --><!-- rv-prompt-box -->");
        assert!(!page.contains("rv-prompt-box"), "{page}");
        // The same component twice, each with the whole instruction, its label and one Copy button.
        assert_eq!(page.matches("data-prompt>").count(), 2);
        assert_eq!(page.matches(AGENT_PROMPT).count(), 2);
        assert_eq!(page.matches(PROMPT_LABEL).count(), 2);
        assert_eq!(page.matches("data-copy>Copy</button>").count(), 2);
    }

    #[test]
    fn hosted_section_requires_an_https_origin() {
        assert!(hosted_section(None).is_empty());
        assert!(hosted_section(Some("http://example.com")).is_empty());
        assert!(hosted_section(Some("https://example.com/evil\" onclick=\"x")).is_empty());
        assert!(
            hosted_section(Some("https://app.richview.ai/"))
                .contains("href=\"https://app.richview.ai/sign-up\"")
        );
    }

    #[test]
    fn the_elements_chart_draws_every_region_of_the_example_data() {
        let svg = elements_chart();
        assert_eq!(svg.matches("<polyline class=\"series").count(), 4, "{svg}");
        for region in ["Africa", "Americas", "Asia", "Europe"] {
            assert!(svg.contains(&format!(">{region}</text>")), "{region}");
        }
        // Its name says what the lines show, read from the same rows as the lines.
        assert!(svg.contains("Europe from 64.6 to 78.8"), "{svg}");
        assert!(svg.contains("Asia from 45.6 to 74.9"), "{svg}");
    }

    #[test]
    fn the_brand_section_shows_the_style_file_that_made_its_report() {
        let page = apply_styles("<code data-hl=\"yaml\"><!-- rv-brand-style --></code>");
        assert!(!page.contains("rv-brand-style"), "{page}");
        assert!(page.contains("Your Company Name Inc."), "{page}");
        assert!(page.contains("palette: ["), "{page}");
    }

    #[test]
    fn the_colour_specimen_tints_each_styles_accent() {
        let list = elements_palettes();
        assert_eq!(
            list.matches("<li style=\"--chip:").count(),
            site_styles().len(),
            "{list}"
        );
        assert_eq!(list.matches("<i></i>").count(), 3 * site_styles().len());
        // No row lacks its colour.
        assert!(!list.contains("--chip:\""), "{list}");
    }

    #[test]
    fn the_proof_pair_carries_the_first_lines_of_both_files() {
        let page = apply_styles("<!-- rv-proof-html -->|<!-- rv-proof-rmd -->");
        let (html, rmd) = page.split_once('|').unwrap();
        assert!(html.starts_with("&lt;!doctype html&gt;"), "{html}");
        assert!(rmd.starts_with("---\ntitle: "), "{rmd}");
        assert_eq!(html.lines().count(), PROOF_LINES);
        // Escaped, so the page shows the markup rather than running it.
        assert!(!html.contains('<') && !rmd.contains('<'));
    }
}
