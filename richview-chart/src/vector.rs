//! The vector renderer: the geometry model as standalone SVG, for PDF export (ARC-18,
//! FEA-RP-43, FEA-CH-66).
//!
//! It draws exactly the model the WebGL renderer draws on the page, so the PDF and the page
//! agree. Everything geometry-critical travels in attributes (fills, strokes, dashes, caps,
//! fonts), so the SVG draws correctly on its own; it never carries `textLength`.

use crate::geometry::{
    Anchor, Cap, Cmd, Face, Fill, Geometry, Hatch, Item, Path, Rect, Rgba, Role, Series, Shape, Stroke, Swatch,
};
use crate::layout::ChartTheme;
use std::fmt::Write;

fn n(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    if r == r.trunc() {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    }
}

fn esc(text: &str) -> String {
    // Control characters other than tab and newline cannot appear in XML at all, even escaped.
    text.chars()
        .filter(|c| !c.is_control() || matches!(c, '\t' | '\n'))
        .collect::<String>()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// `path` as SVG path data.
pub fn d(path: &Path) -> String {
    let mut out = String::new();
    for cmd in &path.cmds {
        match cmd {
            Cmd::M { p } => write!(out, "M{},{}", n(p[0]), n(p[1])),
            Cmd::L { p } => write!(out, "L{},{}", n(p[0]), n(p[1])),
            Cmd::C { c1, c2, p } => write!(
                out,
                "C{},{} {},{} {},{}",
                n(c1[0]),
                n(c1[1]),
                n(c2[0]),
                n(c2[1]),
                n(p[0]),
                n(p[1])
            ),
            Cmd::Z => write!(out, "Z"),
        }
        .expect("write to string");
    }
    out
}

fn paint(color: Rgba, attr: &str) -> String {
    let alpha = color.alpha();
    if alpha >= 0.999 {
        format!(" {attr}=\"{}\"", color.hex())
    } else {
        format!(" {attr}=\"{}\" {attr}-opacity=\"{}\"", color.hex(), n(alpha))
    }
}

fn stroke_attrs(s: &Stroke) -> String {
    let mut out = paint(s.color, "stroke");
    write!(out, " stroke-width=\"{}\"", n(s.width)).expect("write");
    let cap = match s.cap {
        Cap::Butt => "butt",
        Cap::Round => "round",
        Cap::Square => "square",
    };
    write!(out, " stroke-linecap=\"{cap}\" stroke-linejoin=\"round\"").expect("write");
    if !s.dash.is_empty() {
        let dash: Vec<String> = s.dash.iter().map(|v| n(*v)).collect();
        write!(out, " stroke-dasharray=\"{}\"", dash.join(" ")).expect("write");
    }
    out
}

/// A rectangle whose corners have their own radii.
fn rounded(rect: Rect, r: [f64; 4]) -> String {
    let [tl, tr, br, bl] = r.map(|v| v.min(rect.w / 2.0).min(rect.h / 2.0).max(0.0));
    let (x, y, w, h) = (rect.x, rect.y, rect.w, rect.h);
    format!(
        "M{},{}H{}{}V{}{}H{}{}V{}{}Z",
        n(x + tl),
        n(y),
        n(x + w - tr),
        if tr > 0.0 {
            format!("A{0},{0} 0 0 1 {1},{2}", n(tr), n(x + w), n(y + tr))
        } else {
            String::new()
        },
        n(y + h - br),
        if br > 0.0 {
            format!("A{0},{0} 0 0 1 {1},{2}", n(br), n(x + w - br), n(y + h))
        } else {
            String::new()
        },
        n(x + bl),
        if bl > 0.0 {
            format!("A{0},{0} 0 0 1 {1},{2}", n(bl), n(x), n(y + h - bl))
        } else {
            String::new()
        },
        n(y + tl),
        if tl > 0.0 {
            format!("A{0},{0} 0 0 1 {1},{2}", n(tl), n(x + tl), n(y))
        } else {
            String::new()
        },
    )
}

/// The outline of a marker shape centred on `c` with radius `r`.
pub fn shape_path(shape: Shape, c: [f64; 2], r: f64) -> Option<String> {
    let [x, y] = c;
    Some(match shape {
        Shape::Circle => return None,
        Shape::Square => format!(
            "M{},{}h{}v{}h{}Z",
            n(x - r * 0.85),
            n(y - r * 0.85),
            n(r * 1.7),
            n(r * 1.7),
            n(-r * 1.7)
        ),
        Shape::Triangle => format!(
            "M{},{}L{},{}L{},{}Z",
            n(x),
            n(y - r),
            n(x + r * 0.95),
            n(y + r * 0.7),
            n(x - r * 0.95),
            n(y + r * 0.7)
        ),
        Shape::Diamond => format!(
            "M{},{}L{},{}L{},{}L{},{}Z",
            n(x),
            n(y - r),
            n(x + r),
            n(y),
            n(x),
            n(y + r),
            n(x - r),
            n(y)
        ),
        Shape::Plus => format!("M{},{}V{}M{},{}H{}", n(x), n(y - r), n(y + r), n(x - r), n(y), n(x + r)),
        Shape::Cross => format!(
            "M{},{}L{},{}M{},{}L{},{}",
            n(x - r * 0.75),
            n(y - r * 0.75),
            n(x + r * 0.75),
            n(y + r * 0.75),
            n(x + r * 0.75),
            n(y - r * 0.75),
            n(x - r * 0.75),
            n(y + r * 0.75)
        ),
    })
}

fn hatch_lines(h: Hatch) -> &'static str {
    match h {
        Hatch::Diagonal => "M-1,1 l2,-2 M0,6 l6,-6 M5,7 l2,-2",
        Hatch::Back => "M-1,5 l2,2 M0,0 l6,6 M5,-1 l2,2",
        Hatch::Horizontal => "M0,3 H6",
        Hatch::Vertical => "M3,0 V6",
        Hatch::Cross => "M0,3 H6 M3,0 V6",
        Hatch::Dots => "M2.5,3 h1",
    }
}

fn intersection(a: Rect, b: Rect) -> Rect {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    Rect::new(
        x,
        y,
        (a.right().min(b.right()) - x).max(0.0),
        (a.bottom().min(b.bottom()) - y).max(0.0),
    )
}

/// Repeat the original six-pixel hatch on its global grid without a pattern brush.
fn hatch_strokes(hatch: Hatch, bounds: Rect, scale: f64) -> String {
    let x0 = (bounds.x / scale / 6.0).floor() as i64 - 1;
    let y0 = (bounds.y / scale / 6.0).floor() as i64 - 1;
    let x1 = (bounds.right() / scale / 6.0).ceil() as i64 + 1;
    let y1 = (bounds.bottom() / scale / 6.0).ceil() as i64 + 1;
    let (left, top, right, bottom) = (x0 as f64 * 6.0, y0 as f64 * 6.0, x1 as f64 * 6.0, y1 as f64 * 6.0);
    let mut path = String::new();
    let mut line = |a: [f64; 2], b: [f64; 2]| {
        write!(path, "M{},{}L{},{}", n(a[0]), n(a[1]), n(b[0]), n(b[1])).expect("write");
    };
    match hatch {
        Hatch::Diagonal => {
            for k in (x0 + y0)..=(x1 + y1) {
                let sum = k as f64 * 6.0;
                let a = left.max(sum - bottom);
                let b = right.min(sum - top);
                if b > a {
                    line([a, sum - a], [b, sum - b]);
                }
            }
        }
        Hatch::Back => {
            for k in (y0 - x1)..=(y1 - x0) {
                let offset = k as f64 * 6.0;
                let a = left.max(top - offset);
                let b = right.min(bottom - offset);
                if b > a {
                    line([a, a + offset], [b, b + offset]);
                }
            }
        }
        Hatch::Horizontal | Hatch::Vertical | Hatch::Cross => {
            if matches!(hatch, Hatch::Horizontal | Hatch::Cross) {
                for y in y0..=y1 {
                    let y = y as f64 * 6.0 + 3.0;
                    line([left, y], [right, y]);
                }
            }
            if matches!(hatch, Hatch::Vertical | Hatch::Cross) {
                for x in x0..=x1 {
                    let x = x as f64 * 6.0 + 3.0;
                    line([x, top], [x, bottom]);
                }
            }
        }
        Hatch::Dots => {
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let (x, y) = (x as f64 * 6.0 + 2.5, y as f64 * 6.0 + 3.0);
                    line([x, y], [x + 1.0, y]);
                }
            }
        }
    }
    path
}

/// A marker as the plot draws it: shape, size, fill and outline.
#[derive(Clone)]
struct Marker {
    radius: f64,
    shape: Shape,
    fill: Option<Fill>,
    stroke: Stroke,
}

/// What a series' marks show a reader apart from colour: a hatched fill (and whether it is an
/// area's, whose hatch the plot draws at 60 %), a stroked line with its vertex marker, or a marker.
#[derive(Clone)]
enum Cue {
    Fill(Fill, bool),
    Line(Stroke, Option<Marker>),
    Mark(Marker),
}

/// The cue series `s`'s own marks draw, preferring the one `swatch` shows.
fn cue_of(g: &Geometry, s: usize, swatch: Swatch) -> Option<Cue> {
    let mut fill = None;
    let mut line = None;
    let mut marker = None;
    for item in g.items().filter(|i| i.role() == Role::Mark) {
        match item {
            Item::Rect { fill: f, series, .. } if *series == Some(s) && f.hatch.is_some() => {
                fill.get_or_insert((*f, false));
            }
            Item::Area { fill: f, series, .. } if *series == Some(s) && f.hatch.is_some() => {
                fill.get_or_insert((*f, true));
            }
            Item::Line { stroke, series, .. } if *series == Some(s) => {
                line.get_or_insert(stroke.clone());
            }
            Item::Point {
                radius,
                shape,
                fill: f,
                stroke,
                series,
                ..
            } if *series == Some(s) => {
                if let Some(stroke) = stroke {
                    marker.get_or_insert(Marker {
                        radius: *radius,
                        shape: *shape,
                        fill: *f,
                        stroke: stroke.clone(),
                    });
                } else if let Some(f) = f.as_ref().filter(|f| f.hatch.is_some()) {
                    fill.get_or_insert((*f, false));
                }
            }
            _ => {}
        }
    }
    let fill = || fill.map(|(f, area)| Cue::Fill(f, area));
    let line = || line.clone().map(|l| Cue::Line(l, marker.clone()));
    let mark = || marker.clone().map(Cue::Mark);
    match swatch {
        Swatch::Square => fill().or_else(line).or_else(mark),
        Swatch::Line => line().or_else(mark).or_else(fill),
        Swatch::Dot => mark().or_else(fill).or_else(line),
    }
}

/// `cue`, drawn by series `from`, redrawn in series `to`'s colour with its own hatch, dash and
/// shape: how a hidden series, which has no marks, keeps the swatch it showed.
fn recolour(cue: Cue, from: &Series, to: &Series) -> Option<Cue> {
    let paint = |c: Rgba| {
        if c.with_alpha(1.0) == from.color.with_alpha(1.0) {
            to.color.with_alpha(c.alpha())
        } else {
            c
        }
    };
    let marker = |m: Marker| -> Option<Marker> {
        Some(Marker {
            shape: to.shape?,
            fill: m.fill.map(|f| Fill {
                color: paint(f.color),
                ..f
            }),
            stroke: Stroke {
                color: paint(m.stroke.color),
                ..m.stroke
            },
            ..m
        })
    };
    Some(match cue {
        Cue::Fill(f, area) => Cue::Fill(
            Fill {
                color: paint(f.color),
                hatch: Some(to.hatch?),
                hatch_ink: f.hatch_ink.map(paint),
                ..f
            },
            area,
        ),
        Cue::Line(stroke, m) => Cue::Line(
            Stroke {
                color: paint(stroke.color),
                dash: to.dash.clone(),
                ..stroke
            },
            m.and_then(marker),
        ),
        Cue::Mark(m) => Cue::Mark(marker(m)?),
    })
}

/// The legend swatch of series `s` where it carries a cue apart from colour (FEA-CH-57,
/// FEA-CH-33): a square shows the series' hatch over its fill, a line its dash and vertex marker,
/// a dot its marker shape, each as the plot draws them, read from the series' own marks so the key
/// matches every chart type. A series without marks (hidden) takes the drawing of the first series
/// that has some, in its own colour, hatch, dash and shape. `None` when the series carries no cue:
/// the legend keeps STY-40's plain swatch.
pub fn legend_swatch(g: &Geometry, s: usize, swatch: Swatch) -> Option<String> {
    let series = g.series.get(s)?;
    if series.hatch.is_none() && series.shape.is_none() && series.dash.is_empty() {
        return None;
    }
    let cue = match cue_of(g, s, swatch) {
        Some(cue) => cue,
        None => g
            .series
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != s)
            .find_map(|(i, other)| cue_of(g, i, swatch).map(|cue| (cue, other)))
            .and_then(|(cue, other)| recolour(cue, other, series))?,
    };
    let marker = |m: &Marker, c: [f64; 2], most: f64| {
        let r = m.radius.min(most);
        let fill = m
            .fill
            .as_ref()
            .map_or_else(|| " fill=\"none\"".to_owned(), |f| paint(f.color, "fill"));
        let stroke = format!(
            "{} stroke-width=\"{}\"",
            paint(m.stroke.color, "stroke"),
            n(m.stroke.width)
        );
        match shape_path(m.shape, c, r) {
            Some(p) => format!("<path d=\"{p}\"{fill}{stroke}/>"),
            None => format!(
                "<circle cx=\"{}\" cy=\"{}\" r=\"{}\"{fill}{stroke}/>",
                n(c[0]),
                n(c[1]),
                n(r)
            ),
        }
    };
    let svg = |w: u32, h: u32, round: bool, body: String| {
        format!(
            "<svg class=\"rv-chart__decal\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\"{} aria-hidden=\"true\" focusable=\"false\">{body}</svg>",
            if round { " style=\"border-radius:50%\"" } else { "" }
        )
    };
    Some(match cue {
        // Two repeats of the plot's 6 px hatch, at its pitch, so it reads as it does on the marks.
        Cue::Fill(fill, area) => {
            let hatch = fill.hatch?;
            let ink = fill.hatch_ink.unwrap_or(Rgba([1.0, 1.0, 1.0, 1.0]));
            // An area's hatch draws at 0.6, as on the plot: the opacity of its one compound
            // stroke, never a group's, which a PDF prints as a transparency group rather than
            // vector paths.
            let ink = if area { ink.with_alpha(ink.alpha() * 0.6) } else { ink };
            let lines = format!(
                "<path d=\"{}\" fill=\"none\"{} stroke-width=\"1\" stroke-linecap=\"butt\"/>",
                hatch_strokes(hatch, Rect::new(0.0, 0.0, 12.0, 12.0), 1.0),
                paint(ink, "stroke")
            );
            // A dot's hatch keeps the dot's outline, as a hatched bubble does.
            svg(
                12,
                12,
                swatch == Swatch::Dot,
                format!("<rect width=\"12\" height=\"12\"{}/>{lines}", paint(fill.color, "fill")),
            )
        }
        Cue::Line(stroke, m) => {
            let mut stroke = Stroke { width: 2.0, ..stroke };
            stroke.cap = Cap::Butt;
            let marker = m.map(|m| marker(&m, [11.0, 5.0], 3.5)).unwrap_or_default();
            svg(
                22,
                10,
                false,
                format!("<path d=\"M0,5H22\" fill=\"none\"{}/>{marker}", stroke_attrs(&stroke)),
            )
        }
        Cue::Mark(m) => svg(10, 10, false, marker(&m, [5.0, 5.0], 4.0)),
    })
}

fn vector_hatch(
    fill: &Fill,
    outline: &str,
    bounds: Rect,
    opacity: f64,
    defs: &mut String,
    next: &mut usize,
    out: &mut String,
) {
    let Some(hatch) = fill.hatch else { return };
    if bounds.w <= 0.0 || bounds.h <= 0.0 {
        return;
    }
    let scale = f64::from((fill.hatch_scale.max(1.0) * 10.0).round() as u32) / 10.0;
    let ink = fill.hatch_ink.unwrap_or(Rgba([1.0, 1.0, 1.0, 1.0])).hex();
    let id = format!("rvh{}", *next);
    *next += 1;
    write!(defs, "<clipPath id=\"{id}\">{outline}/></clipPath>").expect("write");
    let transform = if scale == 1.0 {
        String::new()
    } else {
        format!(" transform=\"scale({})\"", n(scale))
    };
    let alpha = if opacity == 1.0 {
        String::new()
    } else {
        format!(" stroke-opacity=\"{}\"", n(opacity))
    };
    write!(out, "<g clip-path=\"url(#{id})\"><path d=\"{}\" stroke=\"{ink}\" stroke-width=\"{}\" stroke-linecap=\"butt\" fill=\"none\"{alpha}{transform}/></g>", hatch_strokes(hatch, bounds, scale), n(1.0 / scale)).expect("write");
}

fn clipped(role: Role) -> bool {
    matches!(role, Role::Mark | Role::Band | Role::Rug | Role::Missing)
}

/// The chart drawn for a card of `width` by `height` pixels (the share image, ARC-69): laid out
/// at that width and aspect ratio from the same geometry model the PDF uses, as a standalone SVG
/// with no interaction chrome. Pure: the block, its rows and the theme decide the drawing.
pub fn share_svg(
    block: &crate::block::ChartBlock,
    rows: &[serde_json::Value],
    theme: &ChartTheme,
    width: f64,
    height: f64,
) -> String {
    let mut block = block.clone();
    let options = crate::layout::Options {
        width,
        ..crate::layout::Options::default()
    };
    // The layout grows a figure whose lowest text would sit too close to the bottom, so the
    // plot's share of the height shrinks by the excess until the card is the asked size.
    let mut plot_height = height;
    let mut g = None;
    for _ in 0..3 {
        block.style.ratio = Some((width / plot_height).clamp(0.5, 6.0));
        let laid = crate::layout::layout(&block, rows, theme, &options);
        let excess = laid.height - height;
        g = Some(laid);
        if excess.abs() < 0.5 {
            break;
        }
        plot_height = (plot_height - excess).max(height * 0.5);
    }
    let mut g = g.expect("laid out");
    g.height = height;
    svg(&g, theme)
}

/// The chart as a standalone SVG document.
pub fn svg(g: &Geometry, theme: &ChartTheme) -> String {
    svg_impl(g, theme, false)
}

/// Draw PDF hatches as clipped vector strokes and isolate this figure's resources.
pub fn svg_for_pdf(g: &Geometry, theme: &ChartTheme, prefix: &str) -> String {
    // Keep resources in the reserved engine namespace; authors cannot choose these ids.
    crate::svg::namespace_resources(&svg_impl(g, theme, true), &format!("rvsvg0-{prefix}"))
}

fn svg_impl(g: &Geometry, theme: &ChartTheme, expand_hatches: bool) -> String {
    let mut defs = String::new();
    let mut hatch_defs = String::new();
    let mut next_hatch = 0;
    let mut body = String::new();
    let mut patterns: Vec<(Hatch, String, String, u32)> = Vec::new();
    let mut pattern_id = |fill: &Fill| -> Option<String> {
        let hatch = fill.hatch?;
        let ink = fill.hatch_ink.unwrap_or(Rgba([1.0, 1.0, 1.0, 1.0])).hex();
        let base = fill.color.hex();
        // Pitch in tenths of the 6 px base, so equal pitches share one pattern.
        let pitch = (fill.hatch_scale.max(1.0) * 10.0).round() as u32;
        if let Some(i) = patterns
            .iter()
            .position(|(h, b, k, p)| *h == hatch && *b == base && *k == ink && *p == pitch)
        {
            return Some(format!("rvp{i}"));
        }
        patterns.push((hatch, base, ink, pitch));
        Some(format!("rvp{}", patterns.len() - 1))
    };
    let family = |face: Face| -> String {
        match face {
            Face::Data => theme.data_font.clone(),
            Face::Figures => theme.figures_font.clone().unwrap_or_else(|| theme.data_font.clone()),
            Face::Ui => theme.ui_font.clone(),
        }
    };
    let mut draw = |item: &Item,
                    out: &mut String,
                    pattern_id: &mut dyn FnMut(&Fill) -> Option<String>,
                    visible: Rect| match item {
        Item::Rect {
            rect,
            fill,
            stroke,
            radius,
            ..
        } => {
            let shape = if radius.iter().any(|r| *r > 0.0) {
                format!("<path d=\"{}\"", rounded(*rect, *radius))
            } else {
                format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"",
                    n(rect.x),
                    n(rect.y),
                    n(rect.w.max(0.0)),
                    n(rect.h.max(0.0))
                )
            };
            out.push_str(&shape);
            out.push_str(&paint(fill.color, "fill"));
            if let Some(s) = stroke {
                out.push_str(&stroke_attrs(s));
            }
            out.push_str("/>");
            if fill.hatch.is_some() {
                let outline = if radius.iter().any(|r| *r > 0.0) {
                    format!("<path d=\"{}\"", rounded(*rect, *radius))
                } else {
                    format!(
                        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"",
                        n(rect.x),
                        n(rect.y),
                        n(rect.w.max(0.0)),
                        n(rect.h.max(0.0))
                    )
                };
                if expand_hatches {
                    vector_hatch(
                        fill,
                        &outline,
                        intersection(*rect, visible),
                        1.0,
                        &mut hatch_defs,
                        &mut next_hatch,
                        out,
                    );
                } else if let Some(id) = pattern_id(fill) {
                    write!(out, "{outline} fill=\"url(#{id})\"/>").expect("write");
                }
            }
        }
        Item::Line { path, stroke, .. } => {
            write!(out, "<path d=\"{}\" fill=\"none\"{}/>", d(path), stroke_attrs(stroke)).expect("write");
        }
        Item::Area { path, fill, .. } => {
            write!(out, "<path d=\"{}\"{}/>", d(path), paint(fill.color, "fill")).expect("write");
            if expand_hatches && fill.hatch.is_some() {
                let bounds = path
                    .bounds()
                    .map(|bounds| intersection(bounds, visible))
                    .unwrap_or_default();
                vector_hatch(
                    fill,
                    &format!("<path d=\"{}\"", d(path)),
                    bounds,
                    0.6,
                    &mut hatch_defs,
                    &mut next_hatch,
                    out,
                );
            } else if let Some(id) = pattern_id(fill) {
                write!(
                    out,
                    "<path d=\"{}\" fill=\"url(#{id})\" fill-opacity=\"0.6\"/>",
                    d(path)
                )
                .expect("write");
            }
        }
        Item::Point {
            center,
            radius,
            shape,
            fill,
            stroke,
            ..
        } => {
            let fill_attr = fill.map_or_else(|| " fill=\"none\"".to_owned(), |f| paint(f.color, "fill"));
            let stroke_attr = stroke.as_ref().map_or(String::new(), stroke_attrs);
            match shape_path(*shape, *center, *radius) {
                Some(p) => write!(out, "<path d=\"{p}\"{fill_attr}{stroke_attr}/>").expect("write"),
                None => write!(
                    out,
                    "<circle cx=\"{}\" cy=\"{}\" r=\"{}\"{fill_attr}{stroke_attr}/>",
                    n(center[0]),
                    n(center[1]),
                    n(*radius)
                )
                .expect("write"),
            }
        }
        Item::Text(t) => {
            let anchor = match t.anchor {
                Anchor::Start => "start",
                Anchor::Middle => "middle",
                Anchor::End => "end",
            };
            let rotate = if t.rotate != 0.0 {
                format!(" transform=\"rotate({} {} {})\"", n(t.rotate), n(t.x), n(t.y))
            } else {
                String::new()
            };
            write!(
                out,
                "<text x=\"{}\" y=\"{}\" text-anchor=\"{anchor}\" font-family=\"{}\" font-size=\"{}\" font-weight=\"{}\" font-variant-numeric=\"tabular-nums\"{}{rotate}>{}</text>",
                n(t.x),
                n(t.y),
                esc(&family(t.face)),
                n(t.size),
                t.weight,
                paint(t.color, "fill"),
                esc(&t.text)
            )
            .expect("write");
        }
    };
    for (i, panel) in g.panels.iter().enumerate() {
        // Marks keep to the plot, grown by the widest mark's half-extent so points and strokes
        // on its edges draw whole.
        let reach = panel
            .items
            .iter()
            .filter(|item| clipped(item.role()))
            .map(|item| match item {
                Item::Point { radius, stroke, .. } => radius + stroke.as_ref().map_or(0.0, |s| s.width / 2.0),
                Item::Line { stroke, .. } => stroke.width / 2.0,
                _ => 0.0,
            })
            .fold(0.0, f64::max);
        let p = panel.plot;
        let visible = Rect::new(
            p.x - 1.0 - reach,
            p.y - 3.0 - reach,
            p.w + 2.0 + 2.0 * reach,
            p.h + 4.0 + 2.0 * reach,
        );
        write!(
            defs,
            "<clipPath id=\"rvc{i}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath>",
            n(p.x - 1.0 - reach),
            n(p.y - 3.0 - reach),
            n(p.w + 2.0 + 2.0 * reach),
            n(p.h + 4.0 + 2.0 * reach)
        )
        .expect("write");
        let mut inside = String::new();
        let mut outside = String::new();
        for item in &panel.items {
            if clipped(item.role()) {
                draw(item, &mut inside, &mut pattern_id, visible);
            } else {
                // Keep paint order: flush clipped marks before later unclipped items.
                if !inside.is_empty() {
                    write!(outside, "<g clip-path=\"url(#rvc{i})\">{inside}</g>").expect("write");
                    inside.clear();
                }
                draw(
                    item,
                    &mut outside,
                    &mut pattern_id,
                    Rect::new(0.0, 0.0, g.width, g.height),
                );
            }
        }
        if !inside.is_empty() {
            write!(outside, "<g clip-path=\"url(#rvc{i})\">{inside}</g>").expect("write");
        }
        body.push_str(&outside);
    }
    for item in &g.figure_items {
        draw(item, &mut body, &mut pattern_id, Rect::new(0.0, 0.0, g.width, g.height));
    }
    for (i, (hatch, _base, ink, pitch)) in patterns.iter().enumerate() {
        let scale = f64::from(*pitch) / 10.0;
        let (transform, width) = if *pitch == 10 {
            (String::new(), 1.0)
        } else {
            (format!(" patternTransform=\"scale({})\"", n(scale)), 1.0 / scale)
        };
        write!(defs, "<pattern id=\"rvp{i}\" width=\"6\" height=\"6\" patternUnits=\"userSpaceOnUse\"{transform}><path d=\"{}\" stroke=\"{ink}\" stroke-width=\"{}\" fill=\"none\"/></pattern>", hatch_lines(*hatch), n(width)).expect("write");
    }
    defs.push_str(&hatch_defs);
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\" role=\"img\" aria-label=\"{label}\"><defs>{defs}</defs><rect width=\"{w}\" height=\"{h}\"{bg}/>{body}</svg>",
        w = n(g.width),
        h = n(g.height),
        label = esc(&g.summary),
        bg = paint(g.background, "fill"),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_share_card_draws_a_standalone_svg_at_the_asked_size() {
        let block = crate::block::ChartBlock::from_value(&serde_json::json!({
            "type": "bar", "data": {"inline": []}, "encoding": {"x": "k", "y": "v"}
        }))
        .expect("block");
        let rows: Vec<serde_json::Value> = (0..5)
            .map(|i| serde_json::json!({"k": format!("c{i}"), "v": i * 3 + 1}))
            .collect();
        let svg = share_svg(&block, &rows, &ChartTheme::default(), 1200.0, 630.0);
        assert!(svg.starts_with("<svg"), "{svg:.80}");
        assert!(svg.contains("width=\"1200\""), "{svg:.200}");
        assert!(svg.contains("height=\"630\""), "{svg:.200}");
    }

    use super::*;
    use crate::block::ChartBlock;
    use crate::layout::{Options, layout};
    use serde_json::json;

    #[test]
    fn text_escaping_drops_characters_xml_cannot_hold() {
        assert_eq!(esc("a\u{8}b<c>&\"d\"\te"), "ab&lt;c&gt;&amp;&quot;d&quot;\te");
    }

    #[test]
    fn the_vector_output_is_self_sufficient() {
        let block =
            ChartBlock::from_value(&json!({"type": "bar", "title": "T", "encoding": {"x": "c", "y": "v"}})).unwrap();
        let rows = vec![json!({"c": "A", "v": 10}), json!({"c": "B & C", "v": 5})];
        let theme = ChartTheme::default();
        let g = layout(
            &block,
            &rows,
            &theme,
            &Options {
                width: 480.0,
                ..Options::default()
            },
        );
        let svg = svg(&g, &theme);
        assert!(svg.starts_with("<svg xmlns"));
        assert!(svg.contains("B &amp; C"));
        assert!(svg.contains("font-family=\"Geist Mono"));
        assert!(!svg.contains("textLength"));
        assert!(!svg.contains("var(--"), "no theme variables needed");
        roxmltree::Document::parse(&svg).expect("well-formed XML");
    }

    #[test]
    fn pdf_hatches_keep_each_tile_phase_scale_width_and_mark_clip() {
        let block =
            ChartBlock::from_value(&json!({"type":"bar","title":"Hatches","encoding":{"x":"x","y":"y"}})).unwrap();
        let rows = vec![json!({"x":"A","y":4}), json!({"x":"B","y":7})];
        let theme = ChartTheme::default();
        for hatch in Hatch::ALL {
            let mut g = layout(&block, &rows, &theme, &Options::default());
            let mut expected = 0;
            for item in &mut g.panels[0].items {
                if let Item::Rect {
                    fill, role: Role::Mark, ..
                } = item
                {
                    fill.hatch = Some(hatch);
                    fill.hatch_ink = Some(Rgba([1.0, 1.0, 1.0, 1.0]));
                    fill.hatch_scale = 1.3;
                    expected += 1;
                }
            }
            assert!(expected > 0);
            let screen = svg(&g, &theme);
            assert!(
                screen.contains("<pattern"),
                "the screen/export renderer keeps its existing pattern path"
            );
            assert!(screen.contains("patternTransform=\"scale(1.3)\""));
            let output = svg_for_pdf(&g, &theme, "figure");
            let doc = roxmltree::Document::parse(&output).unwrap();
            assert!(
                !doc.descendants()
                    .any(|node| node.has_tag_name("pattern") || node.has_tag_name("image"))
            );
            let strokes: Vec<_> = doc
                .descendants()
                .filter(|node| node.has_tag_name("path") && node.attribute("transform") == Some("scale(1.3)"))
                .collect();
            assert_eq!(strokes.len(), expected);
            for stroke in strokes {
                assert_eq!(stroke.attribute("stroke"), Some("#ffffff"));
                assert_eq!(stroke.attribute("stroke-width"), Some("0.77"));
                assert_eq!(stroke.attribute("stroke-linecap"), Some("butt"));
                assert!(
                    stroke
                        .ancestors()
                        .filter(|node| node.attribute("clip-path").is_some())
                        .count()
                        >= 2
                );
                let point = |s: &str| -> [f64; 2] {
                    let (x, y) = s.split_once(',').unwrap();
                    [x.parse().unwrap(), y.parse().unwrap()]
                };
                let on_grid = |value: f64| (value / 6.0).fract() == 0.0;
                for segment in stroke.attribute("d").unwrap().split('M').skip(1) {
                    let (a, b) = segment.split_once('L').unwrap();
                    let (a, b) = (point(a), point(b));
                    match hatch {
                        Hatch::Diagonal => {
                            assert_eq!(a[0] + a[1], b[0] + b[1]);
                            assert!(on_grid(a[0] + a[1]));
                        }
                        Hatch::Back => {
                            assert_eq!(a[1] - a[0], b[1] - b[0]);
                            assert!(on_grid(a[1] - a[0]));
                        }
                        Hatch::Horizontal => {
                            assert_eq!(a[1], b[1]);
                            assert!(on_grid(a[1] - 3.0));
                        }
                        Hatch::Vertical => {
                            assert_eq!(a[0], b[0]);
                            assert!(on_grid(a[0] - 3.0));
                        }
                        Hatch::Cross => {
                            assert!((a[1] == b[1] && on_grid(a[1] - 3.0)) || (a[0] == b[0] && on_grid(a[0] - 3.0)))
                        }
                        Hatch::Dots => {
                            assert_eq!(b[0] - a[0], 1.0);
                            assert_eq!(a[1], b[1]);
                            assert!(on_grid(a[0] - 2.5) && on_grid(a[1] - 3.0));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn pdf_area_hatches_preserve_holes_and_one_translucent_stroke() {
        let block =
            ChartBlock::from_value(&json!({"type":"area","title":"Area","encoding":{"x":"x","y":"y"}})).unwrap();
        let rows = vec![json!({"x":0,"y":1}), json!({"x":1,"y":2})];
        let theme = ChartTheme::default();
        let mut g = layout(&block, &rows, &theme, &Options::default());
        let mut path = Path::polyline(&[[10.0, 10.0], [110.0, 10.0], [110.0, 110.0], [10.0, 110.0]]);
        path.close();
        path.move_to([30.0, 30.0]);
        path.line_to([30.0, 90.0]);
        path.line_to([90.0, 90.0]);
        path.line_to([90.0, 30.0]);
        path.close();
        let item = g.panels[0]
            .items
            .iter_mut()
            .find(|item| matches!(item, Item::Area { role: Role::Mark, .. }))
            .unwrap();
        if let Item::Area { path: mark, fill, .. } = item {
            *mark = path;
            fill.hatch = Some(Hatch::Cross);
        }
        let output = svg_for_pdf(&g, &theme, "area");
        let doc = roxmltree::Document::parse(&output).unwrap();
        let stroke = doc
            .descendants()
            .find(|node| node.has_tag_name("path") && node.attribute("stroke-opacity") == Some("0.6"))
            .unwrap();
        let parent = stroke.parent().unwrap();
        let id = parent
            .attribute("clip-path")
            .unwrap()
            .trim_start_matches("url(#")
            .trim_end_matches(')');
        let clip = doc
            .descendants()
            .find(|node| node.has_tag_name("clipPath") && node.attribute("id") == Some(id))
            .unwrap();
        assert_eq!(
            clip.first_element_child().unwrap().attribute("d"),
            Some("M10,10L110,10L110,110L10,110ZM30,30L30,90L90,90L90,30Z")
        );
        assert!(
            doc.descendants().all(|node| node.attribute("opacity").is_none()),
            "one compound stroke preserves opacity at intersections without a rasterized group"
        );
    }
}
