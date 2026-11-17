//! The link-preview thumbnail service (ARC-69, FEA-SF-55, INT-07, STY-70): one pure function from a
//! story's title, byline, style and lead to a deterministic 1200x630 PNG. Publish draws it when it
//! stores a version; `richview serve` and Cowork call the same function for `/thumbnail`.
//!
//! # Calling it
//!
//! ```
//! use richview_share::{render, ShareCard, Lead};
//! let card = ShareCard {
//!     title: "Why the gap keeps widening".into(),
//!     byline: "Daniel Vecera".into(),
//!     style: "scientific".into(), // a builtin style name; any other name draws Sleek
//!     accent: None,               // or Some("#rrggbb") to override the style's accent
//!     rule: None,                 // or the style's `components.thumbnail.rule`
//!     lead: Lead::None,           // a chart SVG, a cover's stored bytes, or nothing
//!     company: None,              // or the report's validated company mark
//! };
//! let png = render(&card).unwrap();
//! assert_eq!(&png[1..4], b"PNG");
//! ```
//!
//! What the caller decides: whether a story may have a card at all and who may fetch it. A story an
//! anonymous reader may not see is never given a card carrying its words: pass [`ShareCard::neutral`].
//! [`alt_text`] is the one wording for the image's alt text.
//!
//! What this crate guarantees: no filesystem, network, clock, environment or system fonts; the same
//! input gives the same bytes. A remote cover is never fetched: `Lead::Cover` takes stored bytes
//! only, and an unsupported type draws the title card. Text is set in the style's own faces, bundled
//! here (Space Grotesk, Geist, Newsreader, Big Shoulders, Patrick Hand, IBM Plex Mono; all OFL),
//! with IBM Plex Mono as the per-glyph fallback for characters a face lacks.
//!
//! The layout lives here behind this one interface so the renderer's core can take it over without
//! changing callers.
mod look;
mod text;

use look::{FACES, Look, PLEX, look};

pub use look::Rule;
use resvg::{tiny_skia, usvg};
use std::sync::{Arc, OnceLock};
use text::{Measure, Ruler, fit, wrap};

pub use look::STYLES;

/// The face the card draws a chart in: the one this crate bundles for every style.
const CHART_FAMILY: &str = "IBM Plex Mono";
/// The width and height the card's lead panel gives its chart (the panel less its padding).
const CHART_PANEL: (f64, f64) = (480.0, 434.0);

/// The report's first chart drawn for the card (ARC-69): the engine lays the chart out at the panel's
/// size in its initial state, from the same geometry model the PDF uses, and the card rasterizes it
/// natively. Every face is the bundled monospace the card measures and draws in. `charts` are the
/// render's chart payloads.
pub fn chart_svg(charts: &[serde_json::Value]) -> Option<String> {
    use richview_chart::block::ChartBlock;
    use richview_chart::reader_controls::Controls;
    let (payload, block) = charts.iter().find_map(|value| {
        let payload: richview_chart::payload::Payload =
            serde_json::from_value(value.clone()).ok()?;
        let block = ChartBlock::from_value(&payload.block)?;
        Some((payload, block))
    })?;
    let mut theme = payload.theme.clone();
    theme.data_font = CHART_FAMILY.into();
    theme.ui_font = CHART_FAMILY.into();
    theme.figures_font = None;
    theme.motion = false;
    let objects = payload.objects();
    let controls = Controls::parse(&payload.block, &objects);
    let state = controls.initial();
    let (block, rows) = match controls.chart(&payload.block, &objects, &state) {
        Some(shown) if !controls.is_empty() => (shown.block, shown.rows),
        _ => (block, objects),
    };
    Some(richview_chart::vector::share_svg(
        &block,
        &rows,
        &theme,
        CHART_PANEL.0,
        CHART_PANEL.1,
    ))
}

/// The card's rule from the story's resolved builtin style (`components.thumbnail.rule`, STY-70);
/// a custom style file leaves it to the builtin the card falls back to.
pub fn style_rule(style: &str) -> Option<Rule> {
    let mut diagnostics = Vec::new();
    let resolved = richview_core::resolve_styles(&[style.to_owned()], &mut diagnostics);
    resolved.document["components"]["thumbnail"]["rule"]
        .as_str()
        .and_then(Rule::parse)
}

/// The link-preview card of a render (FEA-SF-55) as a PNG: the first chart of `charts` leads, the
/// style's thumbnail rule applies, and the style's own accent is kept. Without a chart the card
/// carries the title alone.
pub fn card_png(
    title: &str,
    byline: &str,
    style: &str,
    charts: &[serde_json::Value],
) -> Result<Vec<u8>, String> {
    card_png_with_company(title, byline, style, charts, None)
}

/// Draw a report's share card with its validated company mark when one was supplied.
pub fn card_png_with_company(
    title: &str,
    byline: &str,
    style: &str,
    charts: &[serde_json::Value],
    company: Option<richview_core::CompanyBrand>,
) -> Result<Vec<u8>, String> {
    render(&ShareCard {
        title: title.to_owned(),
        byline: byline.to_owned(),
        style: style.to_owned(),
        rule: style_rule(style),
        lead: chart_svg(charts).map_or(Lead::None, Lead::ChartSvg),
        company,
        ..Default::default()
    })
}

pub const WIDTH: u32 = 1200;
pub const HEIGHT: u32 = 630;

/// What fills the card beside the title.
#[derive(Clone, Debug, Default)]
pub enum Lead {
    /// A standalone chart SVG (it must not rely on page CSS), shown whole.
    ChartSvg(String),
    /// A cover image (`image/svg+xml`, `image/png`, `image/jpeg` or `image/webp`), cropped to fill.
    Cover { bytes: Vec<u8>, mime: String },
    /// Title only.
    #[default]
    None,
}

/// `ShareCard::default()` is the neutral card; callers set the fields they have and take
/// `..Default::default()` for the rest, so a new field never breaks them.
#[derive(Clone, Debug, Default)]
pub struct ShareCard {
    pub title: String,
    pub byline: String,
    /// A builtin style name (`sleek`, `air`, `scientific`, `brutalist`, `hand-drawn`); any other
    /// name draws the default, Sleek (STY-06).
    pub style: String,
    /// The accent as `#rrggbb` in place of the style's own; anything else keeps the style's.
    pub accent: Option<String>,
    /// The style's `components.thumbnail.rule` (STY-70); `None` takes the builtin's own.
    pub rule: Option<Rule>,
    pub lead: Lead,
    /// A company mark already resolved from the report's style and bounded resources.
    pub company: Option<richview_core::CompanyBrand>,
}

impl ShareCard {
    /// The card that names nothing: no title, byline or lead, the mark alone on Sleek.
    pub fn neutral() -> Self {
        Self {
            title: String::new(),
            byline: String::new(),
            style: "sleek".into(),
            accent: None,
            rule: None,
            lead: Lead::None,
            company: None,
        }
    }
}

/// The image's alt text (FEA-SF-55): the title, and the byline when there is one.
pub fn alt_text(title: &str, byline: &str) -> String {
    if byline.trim().is_empty() {
        title.trim().to_owned()
    } else {
        format!("{}, by {}", title.trim(), byline.trim())
    }
}

/// The card as a PNG, or why it could not be drawn.
pub fn render(card: &ShareCard) -> Result<Vec<u8>, String> {
    if let Some(company) = &card.company {
        for logo in [company.logo.as_ref(), company.logo_dark.as_ref()]
            .into_iter()
            .flatten()
        {
            let checked = richview_core::validate_company_logo(&logo.bytes)
                .map_err(|error| format!("Company logo: {error}"))?;
            if checked.mime != logo.mime
                || checked.width != logo.width
                || checked.height != logo.height
            {
                return Err("Company logo metadata differs from its bytes".into());
            }
        }
    }
    let svg = compose(card);
    let options = usvg::Options {
        fontdb: fonts(),
        font_family: PLEX.family.into(),
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_str(&svg, &options).map_err(|e| e.to_string())?;
    let mut pixmap = tiny_skia::Pixmap::new(WIDTH, HEIGHT).ok_or("Share image has no pixels")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().map_err(|e| e.to_string())
}

fn fonts() -> Arc<usvg::fontdb::Database> {
    static DB: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        for face in FACES {
            db.load_font_data(face.data.to_vec());
        }
        db.set_monospace_family(PLEX.family);
        db.set_sans_serif_family(PLEX.family);
        db.set_serif_family(PLEX.family);
        Arc::new(db)
    })
    .clone()
}

const GLYPH: &str = r#"<path d="M221.4 51.39H0V0h221.4c7.32 0 13.25 5.93 13.25 13.25v24.89c0 7.32-5.93 13.25-13.25 13.25z"/><path d="M255.32 124.17H0V72.78h255.32c7.32 0 13.25 5.93 13.25 13.25v24.89c0 7.32-5.93 13.25-13.25 13.25z"/><path d="M221.4 196.95H0v-51.39h221.4c7.32 0 13.25 5.93 13.25 13.25v24.89c0 7.32-5.93 13.25-13.25 13.25z"/><path d="M93.12 269.73H0v-51.39h93.12c7.32 0 13.25 5.93 13.25 13.25v24.89c0 7.32-5.93 13.25-13.25 13.25z"/><circle cx="242.88" cy="218.33" r="25.7"/>"#;

/// Left and right margin; the title and byline stay inside it so apps that crop a preview do not cut them.
const MARGIN: f32 = 64.0;
/// The lead's panel: x, y, width, height.
const PANEL: (f32, f32, f32, f32) = (620.0, 80.0, 516.0, 470.0);
/// The title column beside the panel, and the full width without one.
const COLUMN: f32 = 520.0;
const FULL: f32 = 1072.0;
/// The lowest baseline a title row may sit on.
const TITLE_FOOT: f32 = 470.0;

fn compose(card: &ShareCard) -> String {
    let look = look(&card.style);
    let rule = card.rule.unwrap_or(look.rule);
    let lead = lead_markup(look, rule, &card.lead);
    let has_lead = lead.is_some();
    let title = card.title.trim();
    let byline = card.byline.trim();
    let neutral = title.is_empty() && byline.is_empty() && !has_lead;
    let accent = (!neutral)
        .then(|| {
            card.company
                .as_ref()
                .and_then(|company| company.accent.as_deref())
        })
        .flatten()
        .filter(|c| is_hex(c))
        .or(card.accent.as_deref().filter(|c| is_hex(c)))
        .unwrap_or(look.accent);
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" viewBox=\"0 0 {WIDTH} {HEIGHT}\" width=\"{WIDTH}\" height=\"{HEIGHT}\"><rect width=\"{WIDTH}\" height=\"{HEIGHT}\" fill=\"{}\"/>",
        look.bg
    );
    out.push_str(&frame(rule, look, accent));
    if let Some(markup) = lead {
        out.push_str(&markup);
    }
    let mark = (!neutral)
        .then(|| company_mark(card.company.as_ref(), look.dark))
        .flatten();
    if let Some((markup, _)) = &mark {
        out.push_str(markup);
    }
    if neutral {
        // A neutral card: the mark alone.
        out.push_str(&format!(
            "<g transform=\"translate(528 245) scale(0.5)\" fill=\"{accent}\">{GLYPH}</g>"
        ));
    }
    let ruler = Ruler {
        primary: Measure::new(look.title.data),
        fallback: Measure::new(PLEX.data),
        tracking: 0.0,
    };
    let shown = if look.capitals {
        title.to_uppercase()
    } else {
        title.to_owned()
    };
    let (max, min, width) = if has_lead {
        (60.0, 34.0, COLUMN)
    } else {
        (104.0, 48.0, FULL)
    };
    let top = mark.map_or_else(
        || if rule == Rule::InkBars { 112.0 } else { 100.0 },
        |(_, height)| 72.0 + height + 28.0,
    );
    // The title keeps clear of the byline's mark: as many rows as fit above `TITLE_FOOT`.
    let (size, rows) = fit(&ruler, &shown, max, min, width, |size| {
        ((TITLE_FOOT - top - size * 0.82) / (size * look.leading)).floor() as usize + 1
    });
    for (i, row) in rows.rows.iter().enumerate() {
        out.push_str(&format!(
            "<text x=\"{MARGIN}\" y=\"{}\" font-family=\"{}, {}\" font-size=\"{size}\" fill=\"{}\">{}</text>",
            top + size * 0.82 + size * look.leading * i as f32,
            look.title.family,
            PLEX.family,
            look.ink,
            escape(row)
        ));
    }
    if !byline.is_empty() {
        let small = Ruler {
            primary: Measure::new(look.body.data),
            fallback: Measure::new(PLEX.data),
            tracking: 0.0,
        };
        let row = wrap(
            &small,
            byline,
            28.0,
            if has_lead { COLUMN } else { FULL },
            1,
        )
        .rows
        .into_iter()
        .next()
        .unwrap_or_default();
        out.push_str(&byline_mark(rule, accent));
        out.push_str(&format!(
            "<text x=\"{MARGIN}\" y=\"{}\" font-family=\"{}, {}\" font-size=\"28\" fill=\"{}\">{}</text>",
            if rule == Rule::InkBars { 552 } else { 560 },
            look.body.family,
            PLEX.family,
            if rule == Rule::None { accent } else { look.muted },
            escape(&row)
        ));
    }
    out.push_str("</svg>");
    out
}

/// Place the company mark above the title and return its height including any light plate.
fn company_mark(
    company: Option<&richview_core::CompanyBrand>,
    dark: bool,
) -> Option<(String, f32)> {
    let company = company?;
    let standard = company.logo.as_ref()?;
    let plate = dark && company.logo_dark.is_none();
    let logo = if dark {
        company.logo_dark.as_ref().unwrap_or(standard)
    } else {
        standard
    };
    let ratio = logo.width as f32 / logo.height as f32;
    let mut height = (108.0 / ratio.sqrt()).clamp(32.0, 64.0);
    let mut width = ratio * height;
    if width > 300.0 {
        width = 300.0;
        height = width / ratio;
    }
    let (pad_x, pad_y) = if plate { (12.0, 8.0) } else { (0.0, 0.0) };
    let (x, y) = (MARGIN + pad_x, 72.0 + pad_y);
    let background = if plate {
        format!(
            "<rect x=\"{MARGIN}\" y=\"72\" width=\"{}\" height=\"{}\" rx=\"8\" fill=\"#fff\"/>",
            width + 2.0 * pad_x,
            height + 2.0 * pad_y
        )
    } else {
        String::new()
    };
    Some((
        format!(
            "{background}<image x=\"{x}\" y=\"{y}\" width=\"{width}\" height=\"{height}\" preserveAspectRatio=\"xMidYMid meet\" xlink:href=\"{}\"/>",
            logo.data_uri()
        ),
        height + 2.0 * pad_y,
    ))
}

/// The structure that makes the style's card its own (STY-01, STY-70).
fn frame(rule: Rule, look: &Look, accent: &str) -> String {
    match rule {
        Rule::None => String::new(),
        Rule::Hairline => format!(
            "<rect x=\"{MARGIN}\" y=\"48\" width=\"{FULL}\" height=\"1.5\" fill=\"{}\"/><rect x=\"{MARGIN}\" y=\"588\" width=\"{FULL}\" height=\"1\" fill=\"{}\"/>",
            look.ink, look.line
        ),
        Rule::InkBars => format!(
            "<rect width=\"{WIDTH}\" height=\"22\" fill=\"{}\"/><rect y=\"608\" width=\"{WIDTH}\" height=\"22\" fill=\"{}\"/><rect x=\"{MARGIN}\" y=\"22\" width=\"120\" height=\"14\" fill=\"{accent}\"/>",
            look.ink, look.ink
        ),
        Rule::Pencil => format!(
            "<path d=\"{}\" fill=\"none\" stroke=\"{accent}\" stroke-width=\"3\" stroke-linecap=\"round\"/>",
            pencil(MARGIN, 52.0, 360.0, 52.0, 1)
        ),
    }
}

/// The mark above the byline.
fn byline_mark(rule: Rule, accent: &str) -> String {
    match rule {
        Rule::None => String::new(),
        Rule::Hairline => format!(
            "<rect x=\"{MARGIN}\" y=\"512\" width=\"72\" height=\"1.5\" fill=\"{accent}\"/>"
        ),
        Rule::InkBars => {
            format!("<rect x=\"{MARGIN}\" y=\"500\" width=\"88\" height=\"12\" fill=\"{accent}\"/>")
        }
        Rule::Pencil => format!(
            "<path d=\"{}\" fill=\"none\" stroke=\"{accent}\" stroke-width=\"3\" stroke-linecap=\"round\"/>",
            pencil(MARGIN, 510.0, 190.0, 510.0, 2)
        ),
    }
}

/// A pencil stroke from `(x1, y1)` to `(x2, y2)`: a polyline with small deterministic wobble. The
/// seed is fixed per stroke, never per render, so the same card draws the same strokes.
fn pencil(x1: f32, y1: f32, x2: f32, y2: f32, seed: u64) -> String {
    let mut state = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    let mut jitter = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state % 1000) as f32 / 1000.0 * 3.0 - 1.5
    };
    let steps = (((x2 - x1).hypot(y2 - y1)) / 18.0).ceil().max(1.0) as usize;
    let mut d = String::new();
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let (x, y) = (x1 + (x2 - x1) * t, y1 + (y2 - y1) * t + jitter());
        d.push_str(&format!("{}{x:.1} {y:.1}", if i == 0 { "M" } else { " L" }));
    }
    d
}

fn lead_markup(look: &Look, rule: Rule, lead: &Lead) -> Option<String> {
    let (x, y, w, h) = PANEL;
    let radius = if rule == Rule::None { look.radius } else { 0 };
    let shape = |fill: &str, stroke: &str, width: f32| {
        if rule == Rule::Pencil {
            format!(
                "<path d=\"{} L{x} {y}\" fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"{width}\" stroke-linejoin=\"round\"/>",
                [
                    pencil(x, y, x + w, y, 3),
                    pencil(x + w, y, x + w, y + h, 4).replacen('M', "L", 1),
                    pencil(x + w, y + h, x, y + h, 5).replacen('M', "L", 1),
                    pencil(x, y + h, x, y, 6).replacen('M', "L", 1),
                ]
                .concat()
            )
        } else {
            format!(
                "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" rx=\"{radius}\" fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"{width}\"/>"
            )
        }
    };
    let (stroke, width) = match rule {
        Rule::InkBars => (look.ink, 5.0),
        Rule::Pencil => (look.ink, 2.5),
        _ => (look.line, 1.0),
    };
    let shadow = if rule == Rule::InkBars {
        format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{w}\" height=\"{h}\" fill=\"{}\"/>",
            x + 12.0,
            y + 12.0,
            look.ink
        )
    } else {
        String::new()
    };
    let frame = format!("{shadow}{}", shape(look.panel, stroke, width));
    match lead {
        Lead::None => None,
        Lead::ChartSvg(svg) => {
            let pad = 18.0;
            Some(format!(
                "{frame}<image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"xMidYMid meet\" xlink:href=\"data:image/svg+xml;base64,{}\"/>",
                x + pad,
                y + pad,
                w - 2.0 * pad,
                h - 2.0 * pad,
                base64(svg.as_bytes())
            ))
        }
        Lead::Cover { bytes, mime } => {
            if !matches!(
                mime.as_str(),
                "image/svg+xml" | "image/png" | "image/jpeg" | "image/webp"
            ) {
                return None;
            }
            Some(format!(
                "<clipPath id=\"panel\"><rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" rx=\"{radius}\"/></clipPath>{frame}<image x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" clip-path=\"url(#panel)\" preserveAspectRatio=\"xMidYMid slice\" xlink:href=\"data:{mime};base64,{}\"/>",
                base64(bytes)
            ))
        }
    }
}

fn is_hex(color: &str) -> bool {
    color.len() == 7 && color.starts_with('#') && color[1..].chars().all(|c| c.is_ascii_hexdigit())
}

fn escape(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .map(|c| match c {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '>' => "&gt;".to_owned(),
            '"' => "&quot;".to_owned(),
            c => c.to_string(),
        })
        .collect()
}

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

#[cfg(test)]
mod tests;
