use crate::{DEFAULT_ACCENT, Figure, Item, Kind, Layout, Metadata, Site, Theme, cover, escape};
use chrono::DateTime;

/// Publication pages: shared tokens and fonts, then the page layout.
pub const CSS: &str = concat!(
    include_str!("../assets/tokens.css"),
    include_str!("../assets/publication.css")
);
/// The RichView glyph (four rounded bars and a dot), coloured by the publication accent.
const GLYPH: &str = r#"<svg class="glyph" viewBox="0 0 268.58 269.73" aria-hidden="true" focusable="false"><path d="M221.4 51.39H0V0h221.4c7.32 0 13.25 5.93 13.25 13.25v24.89c0 7.32-5.93 13.25-13.25 13.25z"/><path d="M255.32 124.17H0V72.78h255.32c7.32 0 13.25 5.93 13.25 13.25v24.89c0 7.32-5.93 13.25-13.25 13.25z"/><path d="M221.4 196.95H0v-51.39h221.4c7.32 0 13.25 5.93 13.25 13.25v24.89c0 7.32-5.93 13.25-13.25 13.25z"/><path d="M93.12 269.73H0v-51.39h93.12c7.32 0 13.25 5.93 13.25 13.25v24.89c0 7.32-5.93 13.25-13.25 13.25z"/><circle cx="242.88" cy="218.33" r="25.7"/></svg>"#;

fn theme_name(site: &Site) -> &'static str {
    match site.theme {
        Theme::Paper => "paper",
        Theme::Ink => "ink",
    }
}

/// The accent as used on the dark Ink theme: the brand's tuned cobalt, or the configured
/// accent mixed 40% toward white so links keep their contrast on dark paper.
fn ink_accent(accent: &str) -> String {
    if accent.eq_ignore_ascii_case(DEFAULT_ACCENT) {
        return "#5aa2db".into();
    }
    let channel = |i: usize| u8::from_str_radix(&accent[i..i + 2], 16).unwrap_or(0) as f64;
    let lift = |c: f64| (c + (255.0 - c) * 0.4).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        lift(channel(1)),
        lift(channel(3)),
        lift(channel(5))
    )
}

fn root_style(site: &Site) -> String {
    format!(
        ":root{{--publication-accent:{};--publication-accent-ink:{}}}",
        site.accent,
        ink_accent(&site.accent)
    )
}

/// `18 Oct 2026` in the timestamp's own offset; validation guarantees RFC3339 input.
pub fn human_date(value: &str) -> String {
    DateTime::parse_from_rfc3339(value)
        .map(|d| d.format("%-d %b %Y").to_string())
        .unwrap_or_else(|_| value.chars().take(10).collect())
}

fn time(value: &str) -> String {
    format!(
        "<time datetime=\"{}\">{}</time>",
        escape(value),
        human_date(value)
    )
}

fn section_title<'a>(site: &'a Site, id: &'a str) -> &'a str {
    site.sections
        .iter()
        .find(|s| s.id == id)
        .map(|s| s.title.as_str())
        .unwrap_or(id)
}

/// The first sentence of a summary, for one-line list standfirsts.
fn first_sentence(summary: &str) -> &str {
    let bytes = summary.as_bytes();
    for (i, w) in bytes.windows(2).enumerate() {
        if matches!(w[0], b'.' | b'?' | b'!') && w[1] == b' ' && i >= 24 {
            return &summary[..=i];
        }
    }
    summary
}

/// Display widths requested from Unsplash; the browser scales them into fixed-ratio frames.
const THUMB_WIDTH: u32 = 240;
const LEAD_WIDTHS: [u32; 2] = [800, 1600];
const STORY_WIDTHS: [u32; 2] = [800, 1600];

fn remote_cover(meta: &Metadata) -> bool {
    meta.cover.as_deref().is_some_and(cover::is_remote)
}

/// The cover's address for the reader: the Unsplash URL itself (hotlinked, `ixid` kept) at
/// `width`, or the item's own stored cover.
fn cover_src(meta: &Metadata, width: u32) -> Option<String> {
    let cover = meta.cover.as_deref()?;
    Some(if cover::is_remote(cover) {
        cover::sized(cover, width)
    } else {
        format!("/_content/{}/cover", meta.id)
    })
}

/// A credit rendered by `render`, or nothing when the cover has none.
fn cover_credit(meta: &Metadata, render: fn(&crate::Credit, bool) -> String) -> String {
    meta.cover_credit
        .as_ref()
        .map(|c| render(c, remote_cover(meta)))
        .unwrap_or_default()
}

/// A remote cover `<img>` with intrinsic proportions, so the page does not shift while it
/// loads, and a two-step `srcset`. Only the page's main photo asks for high fetch priority.
fn cover_img(
    meta: &Metadata,
    widths: [u32; 2],
    ratio: (u32, u32),
    sizes: &str,
    priority: bool,
) -> String {
    let alt = escape(meta.cover_alt.as_deref().unwrap_or_default());
    let [small, large] = widths;
    let at = |w| escape(&cover_src(meta, w).unwrap_or_default());
    format!(
        "<img src=\"{}\" srcset=\"{} {small}w, {} {large}w\" sizes=\"{sizes}\" alt=\"{alt}\" width=\"{}\" height=\"{}\" loading=\"eager\"{} decoding=\"async\">",
        at(large),
        at(small),
        at(large),
        ratio.0,
        ratio.1,
        if priority {
            " fetchpriority=\"high\""
        } else {
            ""
        }
    )
}

pub fn shell(site: &Site, title: &str, summary: &str, path: &str, main: &str) -> String {
    shell_in(site, title, summary, path, path, main)
}

/// The page shell; `current` names the navigation entry to mark, such as a story's section.
pub fn shell_in(
    site: &Site,
    title: &str,
    summary: &str,
    path: &str,
    current: &str,
    main: &str,
) -> String {
    let canonical = format!("{}{}", site.base_url.trim_end_matches('/'), path);
    let link = |href: &str, label: &str| {
        format!(
            "<li><a href=\"{}\"{}>{}</a></li>",
            escape(href),
            if href == current {
                " aria-current=\"page\""
            } else {
                ""
            },
            escape(label)
        )
    };
    let navigation = site
        .navigation
        .iter()
        .map(|n| link(&n.path, &n.label))
        .chain([link("/archive", "Archive"), link("/search", "Search")])
        .collect::<String>();
    let theme = theme_name(site);
    let layout = match site.layout {
        Layout::Broadsheet => "broadsheet",
        Layout::Dispatch => "dispatch",
    };
    let scheme = if site.theme == Theme::Ink {
        "dark"
    } else {
        "light"
    };
    let page_title = if path == "/" {
        escape(&site.title)
    } else {
        format!("{} — {}", escape(title), escape(&site.title))
    };
    format!(
        r##"<!doctype html><html lang="en" class="theme-{theme}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{page_title}</title><meta name="description" content="{description}"><link rel="canonical" href="{canonical}"><meta property="og:title" content="{title}"><meta property="og:description" content="{description}"><meta property="og:type" content="website"><meta property="og:url" content="{canonical}"><meta name="color-scheme" content="{scheme}"><link rel="preload" href="/_fonts/newsreader-latin-500-normal.woff2" as="font" type="font/woff2" crossorigin><link rel="preload" href="/_fonts/source-serif-4-latin-400-normal.woff2" as="font" type="font/woff2" crossorigin><script src="/_reading.js" defer></script><link rel="alternate" type="application/rss+xml" title="{masthead}" href="/feed.xml"><style>{root}{css}</style></head><body class="{layout}"><a class="skip" href="#main">Skip to content</a><div class="wrap"><header class="masthead"><a class="brand" href="/" aria-label="{masthead}, front page">{glyph}<span class="wordmark">{masthead}</span></a><nav class="sections" aria-label="Sections"><ul>{navigation}</ul></nav></header></div><main id="main" tabindex="-1" class="wrap">{main}</main><footer class="site-footer"><div class="wrap footer-grid"><div><a class="brand" href="/">{glyph}<span class="footer-wordmark">{masthead}</span></a><p class="footer-mission">{tagline}</p></div><ul class="footer-links"><li><a href="/about">About &amp; corrections</a></li><li><a href="/archive">Archive</a></li><li><a href="/feed.xml">RSS</a></li><li><a href="/internal">Member edition</a></li><li><a href="/_editor/">Editorial desk</a></li></ul><p class="footer-made"><a href="https://richview.ai">Made with RichView</a><a class="release" href="/_status">Release <!--PUBLISH:RELEASE--></a></p></div></footer></body></html>"##,
        title = escape(title),
        masthead = escape(&site.title),
        description = escape(summary),
        canonical = escape(&canonical),
        root = root_style(site),
        css = CSS,
        glyph = GLYPH,
        tagline = escape(&site.description)
    )
}

/// A text-led list row: date | title and one-line standfirst | section, with an optional
/// small cover thumbnail on the right.
pub fn card(site: &Site, meta: &Metadata, title: &str, route: &str) -> String {
    let thumb = cover_src(meta, THUMB_WIDTH)
        .map(|src| {
            format!(
                "<a class=\"row-thumb\" href=\"{}\" tabindex=\"-1\" aria-hidden=\"true\"><img src=\"{}\" alt=\"\" loading=\"lazy\" width=\"96\" height=\"64\"></a>",
                escape(route),
                escape(&src)
            )
        })
        .unwrap_or_default();
    let credit = cover_credit(meta, cover::compact);
    format!(
        "<article class=\"row\" data-item=\"{id}\"><div class=\"row-copy\"><h3 class=\"row-title\"><a href=\"{route}\">{title}</a></h3><p class=\"row-summary\">{summary}</p>{credit}</div><div class=\"row-meta\">{date}<p class=\"row-section\"><a href=\"/sections/{section}\">{section_title}</a></p></div>{thumb}</article>",
        id = meta.id,
        route = escape(route),
        title = escape(title),
        summary = escape(first_sentence(&meta.summary)),
        date = time(&meta.published).replacen("<time ", "<time class=\"row-date\" ", 1),
        section = meta.section,
        section_title = escape(section_title(site, &meta.section)),
    )
}

fn figure_parts(figure: &Figure) -> String {
    format!(
        "<span class=\"figure-value\">{}</span><span class=\"figure-label\">{}</span><span class=\"figure-asof\">{}</span>",
        escape(&figure.value),
        escape(&figure.label),
        escape(&figure.as_of)
    )
}

/// One entry for the front-page figures strip, or nothing when the item has no figure.
pub fn stat(meta: &Metadata, route: &str) -> String {
    meta.figure
        .as_ref()
        .map(|f| {
            format!(
                "<a class=\"figure\" href=\"{}\" data-item=\"{}\">{}</a>",
                escape(route),
                meta.id,
                figure_parts(f)
            )
        })
        .unwrap_or_default()
}

/// The single front-page lead: headline, standfirst and meta on the left; the report's first
/// chart (or the story's figure) on the right.
/// The lead visual taken from a report's first chart.
pub struct LeadChart {
    pub title: String,
    /// The report's source note directly under the chart, such as `Source: Eurostat, …`.
    pub source: Option<String>,
    /// Series names and validated CSS colours, for charts whose compact frame keys series in HTML.
    pub legend: Vec<(String, String)>,
}

pub fn lead(
    site: &Site,
    meta: &Metadata,
    title: &str,
    route: &str,
    chart: Option<&LeadChart>,
) -> String {
    let visual = match (chart, &meta.figure) {
        (Some(chart), _) => format!(
            "<figure class=\"lead-chart\"><a href=\"{route}\" tabindex=\"-1\"><img src=\"/_content/{id}/thumbnail.svg\" alt=\"Chart: {caption}\" loading=\"eager\"></a>{legend}<figcaption><strong>{caption}</strong>{source}</figcaption></figure>",
            route = escape(route),
            id = meta.id,
            caption = escape(&chart.title),
            legend = if chart.legend.is_empty() {
                String::new()
            } else {
                format!(
                    "<ul class=\"lead-legend\" aria-label=\"Series\">{}</ul>",
                    chart
                        .legend
                        .iter()
                        .map(|(name, colour)| format!(
                            "<li><i style=\"background:{}\" aria-hidden=\"true\"></i>{}</li>",
                            escape(colour),
                            escape(name)
                        ))
                        .collect::<String>()
                )
            },
            source = chart
                .source
                .as_deref()
                .map(|s| format!(" <span>{}</span>", escape(s)))
                .unwrap_or_default()
        ),
        (None, Some(figure)) => format!(
            "<a class=\"lead-figure\" href=\"{}\" tabindex=\"-1\">{}</a>",
            escape(route),
            figure_parts(figure)
        ),
        (None, None) => String::new(),
    };
    // The large photo is for remote (Unsplash) covers; a local cover stays a list thumbnail.
    let photo = if remote_cover(meta) {
        let credit = cover_credit(meta, cover::line);
        format!(
            "<figure class=\"lead-photo\"><a href=\"{route}\" tabindex=\"-1\">{img}</a>{caption}</figure>",
            route = escape(route),
            img = cover_img(
                meta,
                LEAD_WIDTHS,
                (1600, 640),
                "(min-width: 1312px) 1200px, 100vw",
                site.lead.first() == Some(&meta.id)
            ),
            caption = if credit.is_empty() {
                String::new()
            } else {
                format!("<figcaption>{credit}</figcaption>")
            }
        )
    } else {
        String::new()
    };
    format!(
        "<article class=\"lead{photo_modifier}{modifier}\" data-item=\"{id}\">{photo}<div class=\"lead-copy\"><h2 class=\"lead-title\"><a href=\"{route}\">{title}</a></h2><p class=\"standfirst\">{summary}</p><p class=\"meta\"><a href=\"/sections/{section}\">{section_title}</a> · {date} · {byline}</p></div>{visual}</article>",
        photo_modifier = if photo.is_empty() { "" } else { " lead--photo" },
        modifier = if visual.is_empty() {
            ""
        } else {
            " lead--visual"
        },
        id = meta.id,
        route = escape(route),
        title = escape(title),
        summary = escape(&meta.summary),
        section = meta.section,
        section_title = escape(section_title(site, &meta.section)),
        date = time(&meta.published),
        byline = escape(&meta.byline),
    )
}

pub fn article(site: &Site, item: &Item) -> String {
    let m = &item.meta;
    let corrections = m
        .corrections
        .iter()
        .map(|c| format!("<li>{} {}</li>", time(&c.date), escape(&c.note)))
        .collect::<String>();
    let correction_block = if corrections.is_empty() {
        String::new()
    } else {
        format!(
            "<aside class=\"corrections\" aria-labelledby=\"corrections\"><h2 id=\"corrections\">Corrections and updates</h2><ul>{corrections}</ul></aside>"
        )
    };
    let meta = if m.kind == Kind::Page {
        String::new()
    } else {
        let updated = if m.updated.get(..10) == m.published.get(..10) {
            String::new()
        } else {
            format!(" · Updated {}", time(&m.updated))
        };
        format!(
            "<p class=\"meta\"><a href=\"/sections/{section}\">{section_title}</a> · Published {published}{updated} · {byline}</p>",
            section = m.section,
            section_title = escape(section_title(site, &m.section)),
            published = time(&m.published),
            byline = escape(&m.byline),
        )
    };
    let cover_figure = if remote_cover(m) {
        let credit = cover_credit(m, cover::line);
        format!(
            "<figure class=\"story-cover\">{img}{caption}</figure>",
            img = cover_img(
                m,
                STORY_WIDTHS,
                (1600, 900),
                "(min-width: 928px) 880px, 100vw",
                true
            ),
            caption = if credit.is_empty() {
                String::new()
            } else {
                format!("<figcaption>{credit}</figcaption>")
            }
        )
    } else {
        String::new()
    };
    let main = format!(
        r#"<article class="story"><header class="story-head"><h1>{title}</h1><p class="standfirst">{summary}</p>{meta}{cover_figure}</header><noscript><style>.report{{height:6000px}}</style><p class="noscript-note">Scripts are off, so this page cannot size the report to its full length. <a href="/_content/{id}/report.html" target="_blank" rel="noopener">Open the full report</a> to read all of it.</p></noscript><iframe class="report" title="{title}" src="/_content/{id}/report.html?embed=1" sandbox="allow-scripts allow-downloads allow-popups allow-popups-to-escape-sandbox" loading="eager"></iframe><p class="open-report"><a href="/_content/{id}/report.html" target="_blank" rel="noopener">Open the full report</a></p>{correction_block}{more}</article>"#,
        title = escape(&item.title),
        summary = escape(&m.summary),
        id = m.id,
        more = if m.kind == Kind::Page {
            ""
        } else {
            "<!--PUBLISH:MORE-->"
        },
    );
    let current = if m.kind == Kind::Page {
        item.route.clone()
    } else {
        format!("/sections/{}", m.section)
    };
    let page = shell_in(site, &item.title, &m.summary, &item.route, &current, &main);
    if m.kind == Kind::Page {
        page
    } else {
        // The server decides per request whether this story may advertise a preview image.
        page.replacen(crate::OG_WEBSITE, crate::SOCIAL_SLOT, 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ink_accent_keeps_the_brand_cobalt_and_lifts_custom_accents() {
        assert_eq!(ink_accent("#1A6DAC"), "#5aa2db");
        assert_eq!(ink_accent("#000000"), "#666666");
    }

    #[test]
    fn dates_and_list_standfirsts_read_naturally() {
        assert_eq!(human_date("2026-10-08T23:30:00-02:00"), "8 Oct 2026");
        assert_eq!(
            first_sentence("A first sentence that is long. A second one."),
            "A first sentence that is long."
        );
        assert_eq!(first_sentence("Short. Then more."), "Short. Then more.");
    }
}
