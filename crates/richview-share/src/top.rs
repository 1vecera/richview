//! The top of a report (FEA-SF-55, STY-70, ARC-69; Daniel, 16 Nov 2026: "when they share a link,
//! I want the preview to look as cool as this does"): the link-preview card shows a report as it
//! opens, its title over its first chart, and the docs gallery shows the same chart as a card's
//! picture. Both are drawn here, natively, from the chart payloads a render returns.
//!
//! The chart is laid out wide and whole by the engine, from the same geometry model the PDF uses,
//! at the size it is shown; nothing fades over data. A faceted chart shows its first row of
//! panels. The paper, inks and accent are the report's own (its theme's `bg`, `chart-ink`,
//! `ink-soft` and accent), with the builtin style's look where a colour does not convert; titles
//! are set in the style's bundled display face, the chart in the bundled monospace.

use crate::look::{Look, PLEX, look};
use crate::text::{Measure, Ruler, wrap};
use crate::{GLYPH, HEIGHT, MARGIN, WIDTH, base64, company_mark, escape, raster};
use richview_chart::block::ChartBlock;
use richview_chart::layout::ChartTheme;
use serde_json::Value;

/// The card's text width between its margins.
const TEXT: f32 = WIDTH as f32 - 2.0 * MARGIN;
/// Where the title block starts without a company mark.
const TOP: f32 = 52.0;
/// The byline's baseline; the chart ends 24 px above the byline row.
const BYLINE: f32 = 596.0;
const CHART_FOOT: f32 = BYLINE - 24.0 - 24.0;
/// A chart shorter than this does not read on a card: the card carries its title alone.
const SMALLEST_CHART: f32 = 160.0;
/// The chart's own title and subtitle, as the report sets them above the chart.
const CHART_TITLE: f32 = 20.0;
const CHART_SUBTITLE: f32 = 16.0;

/// The first chart of a render, in the reader's initial state, ready to lay out at `width`.
pub(crate) struct First {
    pub(crate) block: ChartBlock,
    pub(crate) rows: Vec<Value>,
    pub(crate) theme: ChartTheme,
}

/// The first chart a reader sees, its controls in their initial state. Every face is the bundled
/// monospace, and a faceted chart keeps the panels of its first row at `width` (two columns from
/// 600 px, as the engine sets them, FEA-CH-35).
pub(crate) fn first(charts: &[Value], width: f64) -> Option<First> {
    use richview_chart::reader_controls::Controls;
    let (payload, block) = charts.iter().find_map(|value| {
        let payload: richview_chart::payload::Payload =
            serde_json::from_value(value.clone()).ok()?;
        let block = ChartBlock::from_value(&payload.block)?;
        Some((payload, block))
    })?;
    let mut theme = payload.theme.clone();
    theme.data_font = PLEX.family.into();
    theme.ui_font = PLEX.family.into();
    theme.figures_font = None;
    theme.motion = false;
    let objects = payload.objects();
    let controls = Controls::parse(&payload.block, &objects);
    let state = controls.initial();
    let (block, mut rows) = match controls.chart(&payload.block, &objects, &state) {
        Some(shown) if !controls.is_empty() => (shown.block, shown.rows),
        _ => (block, objects),
    };
    if let Some(field) = block.encoding.facet.clone() {
        let names = richview_chart::rows::distinct(&rows, &field);
        let columns = if width >= 600.0 { 2 } else { 1 };
        if names.len() > columns {
            let kept = &names[..columns];
            rows.retain(|row| {
                richview_chart::rows::text(row, &field).is_some_and(|name| kept.contains(&name))
            });
        }
    }
    Some(First { block, rows, theme })
}

/// The report's colours: its own where they convert to sRGB, else the builtin style's.
struct Colours {
    paper: String,
    ink: String,
    soft: String,
    accent: String,
}

fn colours(first: &First, look: &Look) -> Colours {
    let hex = |css: &str, fallback: &str| {
        richview_chart::color::parse(css).map_or_else(|| fallback.to_owned(), |c| c.hex())
    };
    Colours {
        paper: hex(&first.theme.page, look.bg),
        ink: hex(&first.theme.ink, look.ink),
        soft: hex(&first.theme.ink_soft, look.muted),
        accent: hex(&first.theme.accent, look.accent),
    }
}

/// One line of text at `size`, cut with an ellipsis to `width`, as SVG.
fn line(face: &str, size: f32, x: f32, y: f32, width: f32, fill: &str, text: &str) -> String {
    let data = crate::look::FACES
        .iter()
        .find(|f| f.family == face)
        .map_or(PLEX.data, |f| f.data);
    let ruler = Ruler {
        primary: Measure::new(data),
        fallback: Measure::new(PLEX.data),
        tracking: 0.0,
    };
    let row = wrap(&ruler, text, size, width, 1)
        .rows
        .into_iter()
        .next()
        .unwrap_or_default();
    format!(
        "<text x=\"{x}\" y=\"{y}\" font-family=\"{face}, {}\" font-size=\"{size}\" fill=\"{fill}\">{}</text>",
        PLEX.family,
        escape(&row)
    )
}

/// The chart part: its own title and subtitle, then the chart, filling `(x, y, w, h)`. `None`
/// when the chart would be shorter than [`SMALLEST_CHART`].
fn chart_part(
    first: &First,
    look: &Look,
    colours: &Colours,
    (x, y, w, h): (f32, f32, f32, f32),
) -> Option<String> {
    let title: String = first.block.title.chars().take(160).collect();
    let subtitle: String = first
        .block
        .subtitle
        .as_deref()
        .unwrap_or_default()
        .chars()
        .take(200)
        .collect();
    let mut out = String::new();
    let mut top = y;
    if !title.trim().is_empty() {
        top += CHART_TITLE * 0.8;
        out.push_str(&line(
            look.body.family,
            CHART_TITLE,
            x,
            top,
            w,
            &colours.ink,
            title.trim(),
        ));
        top += CHART_TITLE * 0.45;
    }
    if !subtitle.trim().is_empty() {
        top += 6.0 + CHART_SUBTITLE * 0.8;
        out.push_str(&line(
            look.body.family,
            CHART_SUBTITLE,
            x,
            top,
            w,
            &colours.soft,
            subtitle.trim(),
        ));
        top += CHART_SUBTITLE * 0.45;
    }
    if top > y {
        top += 14.0;
    }
    let height = y + h - top;
    if height < SMALLEST_CHART {
        return None;
    }
    let svg = richview_chart::vector::share_svg(
        &first.block,
        &first.rows,
        &first.theme,
        f64::from(w),
        f64::from(height),
    );
    out.push_str(&format!(
        "<image x=\"{x}\" y=\"{top}\" width=\"{w}\" height=\"{height}\" preserveAspectRatio=\"xMidYMin meet\" xlink:href=\"data:image/svg+xml;base64,{}\"/>",
        base64(svg.as_bytes())
    ));
    Some(out)
}

/// The card as SVG: the company mark when there is one, the title (58 px on at most two lines,
/// else 48 px and never cut), the first chart wide and whole, the byline and a small RichView mark.
/// `None` when the report has no chart that fits under its title.
pub(crate) fn card_svg(
    title: &str,
    byline: &str,
    style: &str,
    first: &First,
    company: Option<&richview_core::CompanyBrand>,
) -> Option<String> {
    let look = look(style);
    let colours = colours(first, look);
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" viewBox=\"0 0 {WIDTH} {HEIGHT}\" width=\"{WIDTH}\" height=\"{HEIGHT}\"><rect width=\"{WIDTH}\" height=\"{HEIGHT}\" fill=\"{}\"/>",
        colours.paper
    );
    let mut top = TOP;
    if let Some((markup, height)) = company_mark(company, look.dark, TOP) {
        out.push_str(&markup);
        top += height + 24.0;
    }
    let title = title.trim();
    let shown = if look.capitals {
        title.to_uppercase()
    } else {
        title.to_owned()
    };
    let ruler = Ruler {
        primary: Measure::new(look.title.data),
        fallback: Measure::new(PLEX.data),
        tracking: 0.0,
    };
    let (size, rows) = match wrap(&ruler, &shown, 58.0, TEXT, 2) {
        rows if !rows.cut => (58.0, rows.rows),
        _ => (48.0, wrap(&ruler, &shown, 48.0, TEXT, usize::MAX).rows),
    };
    for (i, row) in rows.iter().enumerate() {
        out.push_str(&format!(
            "<text x=\"{MARGIN}\" y=\"{}\" font-family=\"{}, {}\" font-size=\"{size}\" fill=\"{}\">{}</text>",
            top + size * 0.82 + size * look.leading * i as f32,
            look.title.family,
            PLEX.family,
            colours.ink,
            escape(row)
        ));
    }
    let below = if rows.is_empty() {
        top
    } else {
        top + size * 0.82 + size * look.leading * (rows.len() - 1) as f32 + size * 0.22 + 26.0
    };
    out.push_str(&chart_part(
        first,
        look,
        &colours,
        (MARGIN, below, TEXT, CHART_FOOT - below),
    )?);
    let byline = byline.trim();
    if !byline.is_empty() {
        out.push_str(&line(
            look.body.family,
            24.0,
            MARGIN,
            BYLINE,
            TEXT - 80.0,
            &colours.soft,
            byline,
        ));
    }
    let mark = 26.0;
    out.push_str(&format!(
        "<g transform=\"translate({} {}) scale({})\" fill=\"{}\">{GLYPH}</g>",
        WIDTH as f32 - MARGIN - mark,
        BYLINE - mark * 0.85,
        mark / 269.73,
        colours.accent
    ));
    out.push_str("</svg>");
    Some(out)
}

/// The title of the chart [`chart_picture_png`] draws, which names the picture for a reader who
/// cannot see it (Daniel's lead, 16 Nov: the chart's title, not the report's). `None` when the
/// render has no chart or the chart has no title.
pub fn chart_picture_title(charts: &[Value]) -> Option<String> {
    let first = first(charts, 0.0)?;
    let title = first.block.title.trim();
    (!title.is_empty()).then(|| title.to_owned())
}

/// The docs gallery's picture of a report or chart (FEA-SF-43): the first chart with its own
/// title and subtitle on the report's paper, laid out at `width` by `height` and drawn `scale`
/// times as large, as a PNG. `None` when the render has no chart, or no room for one.
pub fn chart_picture_png(
    style: &str,
    charts: &[Value],
    width: u32,
    height: u32,
    scale: f32,
) -> Result<Option<Vec<u8>>, String> {
    let pad = (width.min(height) as f32 * 0.04).round().max(12.0);
    let inner = width as f32 - 2.0 * pad;
    let Some(first) = first(charts, f64::from(inner)) else {
        return Ok(None);
    };
    let look = look(style);
    let colours = colours(&first, look);
    let Some(part) = chart_part(
        &first,
        look,
        &colours,
        (pad, pad, inner, height as f32 - 2.0 * pad),
    ) else {
        return Ok(None);
    };
    let (shown_width, shown_height) = (
        (width as f32 * scale).round() as u32,
        (height as f32 * scale).round() as u32,
    );
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" viewBox=\"0 0 {width} {height}\" width=\"{shown_width}\" height=\"{shown_height}\"><rect width=\"{width}\" height=\"{height}\" fill=\"{}\"/>{part}</svg>",
        colours.paper
    );
    raster(&svg, shown_width, shown_height).map(Some)
}
