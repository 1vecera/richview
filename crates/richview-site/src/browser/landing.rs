//! Native-scroll homepage choreography. The same sourced monthly series drives the mark, the
//! chart, the monthly handoff and the halftone field; no synthetic chart data or JavaScript
//! policy. The stage is a fixed SVG behind the page; each pose is anchored to a live page
//! element (`[data-stage]`), so the pieces travel between the hero, the editable source, the
//! chart, the five product cards, the workflow and the signup as the visitor scrolls.
use super::{document, error, fetch_text, js_error, now, on, query, style, window};
use std::{cell::RefCell, collections::BTreeMap, fmt::Write};
use wasm_bindgen::{JsCast, prelude::*};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{CanvasRenderingContext2d, Element, HtmlCanvasElement, HtmlElement, Path2d};

const NS: &str = "http://www.w3.org/2000/svg";
const MONTH_DATA: &str = include_str!(
    "../../../../examples/openai-vs-anthropic/data/charts/hn-comment-mentions-monthly.csv"
);
const TITLE: &str = "Anthropic’s share rose every year";
const THEME_COLORS: [[&str; 10]; 3] = [
    [
        "#0E1117", "#161B24", "#141922", "#ECEFF2", "#A0A5AC", "#6B717A", "#292E36", "#195BFF",
        "#7FA0FF", "#C9CED6",
    ],
    [
        "#FEFBF9", "#F3EFEA", "#FFFFFF", "#13110F", "#504C48", "#8B857E", "#DAD7D3", "#1B6F9E",
        "#155A80", "#3A3631",
    ],
    [
        "#FCFAF7", "#EFECE7", "#FFFFFF", "#0F0D0B", "#4A4744", "#85807A", "#0F0D0B", "#124A8C",
        "#0E3A6E", "#0F0D0B",
    ],
];
/// Corner radius of the product cards in CSS pixels, shared with `homepage.css`.
const CARD_RADIUS: f64 = 18.0;
type Color = [f64; 4];
type AnimationCallback = Closure<dyn FnMut(f64)>;
#[derive(Clone, Copy, Debug, Default)]
struct Frame {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}
impl Frame {
    fn local(self) -> Frame {
        Frame {
            x: 0.0,
            y: 0.0,
            ..self
        }
    }
}
#[derive(Clone)]
struct Month {
    month: String,
    openai: f64,
    anthropic: f64,
}
#[derive(Clone)]
struct Year {
    year: String,
    label: String,
    months: usize,
    share: f64,
}
#[derive(Clone, Copy)]
struct Bar {
    x: f64,
    y: f64,
    len: f64,
    th: f64,
    rot: f64,
    cap: f64,
    base: f64,
    color: Color,
    opacity: f64,
}
#[derive(Clone, Copy)]
struct Dot {
    x: f64,
    y: f64,
    r: f64,
    color: Color,
    opacity: f64,
}
#[derive(Clone)]
struct Pose {
    bars: Vec<Bar>,
    lines: Vec<Bar>,
    dot: Dot,
    layers: BTreeMap<&'static str, f64>,
}
#[derive(Clone, Copy)]
struct Colors {
    blue: Color,
    ink: Color,
    soft: Color,
    rule: Color,
    clay: Color,
    openai: Color,
}
#[derive(Clone)]
struct Chapter {
    element: Element,
    top: f64,
    rest: f64,
    name: String,
}
/// Page elements the stage draws into.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Anchor {
    Hero,
    Source,
    Chart,
    Deck,
    Cowork,
    Signup,
}
const ANCHORS: [(Anchor, &str); 6] = [
    (Anchor::Hero, "[data-stage='mark']"),
    (Anchor::Source, "[data-stage='source']"),
    (Anchor::Chart, "[data-stage='chart']"),
    (Anchor::Deck, ".product__deck"),
    (Anchor::Cowork, "[data-stage='cowork']"),
    (Anchor::Signup, "[data-stage='signup']"),
];
/// A pose in the scroll timeline; geometry is resolved from live anchors on every frame.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Key {
    /// Off the stage: the first screen belongs to the painting agent's clip (S27-959) and the live
    /// example to its report (W3 scenes 1 and 2).
    Away,
    Mark(Anchor),
    Columns {
        diag: bool,
    },
    Theme(usize),
    /// The four bars and the dot become the five product cards.
    Cards {
        visible: bool,
    },
    Line {
        monthly: bool,
    },
    Field,
}
/// Chapter start indices, so keys follow the page's actual section order.
#[derive(Clone, Copy, Default, Debug)]
struct Times {
    demo: f64,
    what: f64,
    elements: f64,
    workflow: f64,
    why: f64,
    flagship: f64,
    signup: f64,
    /// Card handoff: pieces arrive, the stage hides, the covers are gone. Measured from the
    /// scroll position where the deck settles, so the handoff happens fully on screen.
    cards: (f64, f64, f64),
}
impl Times {
    fn handoff(&self) -> (f64, f64, f64) {
        self.cards
    }
}
fn timeline(c: Times) -> Vec<(f64, Key)> {
    use Key::*;
    let (cards, hidden, _) = c.handoff();
    vec![
        (0.0, Away),
        (c.what, Away),
        (c.what + 0.14, Columns { diag: false }),
        (c.what + 0.26, Columns { diag: true }),
        (c.what + 0.44, Columns { diag: true }),
        (c.what + 0.54, Theme(0)),
        (c.what + 0.62, Theme(0)),
        (c.what + 0.68, Theme(1)),
        (c.what + 0.76, Theme(1)),
        (c.what + 0.82, Theme(2)),
        (c.what + 0.92, Theme(2)),
        (cards, Cards { visible: true }),
        (hidden, Cards { visible: false }),
        (c.elements + 0.92, Cards { visible: false }),
        (c.workflow + 0.12, Line { monthly: false }),
        (c.workflow + 0.42, Line { monthly: false }),
        (c.workflow + 0.58, Line { monthly: true }),
        (c.workflow + 0.9, Line { monthly: true }),
        (c.why + 0.45, Field),
        (c.flagship + 0.85, Field),
        (c.signup + 0.2, Mark(Anchor::Signup)),
    ]
}
/// Card size and the five "together" placements: centre offset, z-rotation and scale.
#[derive(Clone, Copy, Debug, Default)]
struct DeckLayout {
    w: f64,
    h: f64,
    together: [(f64, f64, f64, f64); 5],
    focus_scale: f64,
}
fn deck_layout(deck: Frame) -> DeckLayout {
    let narrow = deck.w < 520.0;
    let w = (deck.w * if narrow { 0.66 } else { 0.52 }).clamp(200.0, 380.0);
    let h = (w * 1.3).min(deck.h * 0.94);
    let w = w.min(h / 1.2);
    let spread = 0.56;
    let scale = (deck.w / (w * (1.0 + 4.0 * spread))).min(0.78);
    let together = std::array::from_fn(|i| {
        let d = i as f64 - 2.0;
        (
            d * w * spread * scale,
            d.abs() * d.abs() * 6.0 * scale + h * 0.04,
            d * 4.5,
            scale,
        )
    });
    DeckLayout {
        w,
        h,
        together,
        focus_scale: (deck.h * 0.94 / h).min(1.0),
    }
}
struct Stage {
    svg: Element,
    bars: Vec<Element>,
    lines: Vec<Element>,
    dot: Element,
    layers: BTreeMap<&'static str, (Element, Option<Anchor>)>,
    keys: Vec<(f64, Key)>,
}
struct Landing {
    canvas: HtmlCanvasElement,
    ctx: CanvasRenderingContext2d,
    chapters: Vec<Chapter>,
    anchors: BTreeMap<Anchor, Element>,
    months: Vec<Month>,
    years: Vec<Year>,
    colors: Colors,
    stage: Stage,
    times: Times,
    width: f64,
    height: f64,
    /// Chapter-boundary probe captured at the last full `layout()`. Keeps the
    /// chapter starts stable while `height` drifts with mobile URL-bar motion.
    probe: f64,
    dpr: f64,
    reduced: bool,
    target: f64,
    current: f64,
    last_time: f64,
    scheduled: bool,
    force: bool,
    pointer: Option<(f64, f64)>,
    intro_start: Option<f64>,
    last_field: String,
    hud_month: usize,
    active_chapter: usize,
    deck: DeckLayout,
    deck_active: Option<Option<usize>>,
}
thread_local! {static LANDING:RefCell<Option<Landing>>=const{RefCell::new(None)};static RAF:RefCell<Option<AnimationCallback>>=const{RefCell::new(None)};}

fn clamp(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}
fn span(v: f64, a: f64, b: f64) -> f64 {
    clamp((v - a) / (b - a).max(1e-9))
}
fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}
fn ease(t: f64) -> f64 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}
fn out(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(3)
}
fn back(t: f64, s: f64) -> f64 {
    1.0 + (s + 1.0) * (t - 1.0).powi(3) + s * (t - 1.0).powi(2)
}
fn stagger(t: f64, i: usize, n: usize, spread: f64) -> f64 {
    clamp((t - i as f64 / (n - 1).max(1) as f64 * spread) / (1.0 - spread))
}
fn mix(a: Color, b: Color, t: f64) -> Color {
    std::array::from_fn(|i| lerp(a[i], b[i], t))
}
fn color(value: &str) -> Color {
    let text = value.trim();
    if let Some(hex) = text.strip_prefix('#') {
        let hex: String = if hex.len() == 3 {
            hex.chars().flat_map(|c| [c, c]).collect()
        } else {
            hex.into()
        };
        if let Ok(n) = u32::from_str_radix(&hex[..hex.len().min(6)], 16) {
            return [
                ((n >> 16) & 255) as f64,
                ((n >> 8) & 255) as f64,
                (n & 255) as f64,
                1.0,
            ];
        }
    }
    if let Some(args) = text.split_once('(').map(|(_, v)| v.trim_end_matches(')')) {
        let channels: Vec<f64> = args
            .split(|c: char| c == ',' || c.is_whitespace() || c == '/')
            .filter_map(|v| v.parse().ok())
            .collect();
        if channels.len() >= 3 {
            return [
                channels[0],
                channels[1],
                channels[2],
                channels.get(3).copied().unwrap_or(1.0),
            ];
        }
    }
    [128.0, 128.0, 128.0, 1.0]
}
fn css(c: Color) -> String {
    format!("rgba({:.0},{:.0},{:.0},{:.3})", c[0], c[1], c[2], c[3])
}
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn attr(e: &Element, name: &str, value: impl ToString) {
    let _ = e.set_attribute(name, &value.to_string());
}
fn svg(tag: &str, class: &str, parent: &Element) -> Result<Element, JsValue> {
    let e = document().create_element_ns(Some(NS), tag)?;
    e.set_attribute("class", class)?;
    parent.append_child(&e)?;
    Ok(e)
}

fn read_colors() -> Colors {
    let root = document().document_element().unwrap();
    let computed = window().get_computed_style(&root).ok().flatten();
    let prop = |name: &str, default: &str| {
        computed
            .as_ref()
            .and_then(|c| c.get_property_value(name).ok())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| default.into())
    };
    Colors {
        blue: color(&prop("--blue", "#1A6DAC")),
        ink: color(&prop("--ink", "#1F1C19")),
        soft: color(&prop("--soft", "#5E5750")),
        rule: color(&prop("--rule", "#DDD5C9")),
        clay: color(&prop("--field-anthropic", "#C4613F")),
        openai: color(&prop("--field-openai", "#2B2724")),
    }
}
fn rect_of(e: &Element) -> Frame {
    let r = e.get_bounding_client_rect();
    Frame {
        x: r.left(),
        y: r.top(),
        w: r.width(),
        h: r.height(),
    }
}
fn css_px(value: &str) -> f64 {
    value.trim().trim_end_matches("px").parse().unwrap_or(0.0)
}
fn hidden_pose(f: Frame, c: Colors) -> Pose {
    let piece = hidden(Bar {
        x: f.x + f.w / 2.0,
        y: f.y + f.h / 2.0,
        len: 1.0,
        th: 1.0,
        rot: 0.0,
        cap: 0.5,
        base: 0.5,
        color: c.blue,
        opacity: 0.0,
    });
    Pose {
        bars: vec![piece; 4],
        lines: vec![piece; 28],
        dot: Dot {
            x: piece.x,
            y: piece.y,
            r: 0.0,
            color: c.blue,
            opacity: 0.0,
        },
        layers: layers(&[]),
    }
}
/// A rounded card rectangle as a vertical bar: centre, size and z-rotation in degrees.
fn card_bar(cx: f64, cy: f64, w: f64, h: f64, rz: f64, scale: f64, color: Color) -> Bar {
    let rot = -90.0 + rz;
    let (sin, cos) = rot.to_radians().sin_cos();
    Bar {
        x: cx - cos * h / 2.0,
        y: cy - sin * h / 2.0,
        len: h,
        th: w,
        rot,
        cap: CARD_RADIUS * scale,
        base: CARD_RADIUS * scale,
        color,
        opacity: 1.0,
    }
}

fn data() -> (Vec<Month>, Vec<Year>) {
    let months: Vec<_> = MONTH_DATA
        .lines()
        .skip(1)
        .filter_map(|line| {
            let cols: Vec<_> = line.split(',').collect();
            Some(Month {
                month: cols.first()?.to_string(),
                openai: cols.get(1)?.parse().ok()?,
                anthropic: cols.get(2)?.parse().ok()?,
            })
        })
        .collect();
    let mut sums: BTreeMap<String, (f64, f64, usize)> = BTreeMap::new();
    for m in &months {
        let row = sums.entry(m.month[..4].into()).or_default();
        row.0 += m.openai;
        row.1 += m.anthropic;
        row.2 += 1;
    }
    let years = sums
        .into_iter()
        .map(|(year, (o, a, n))| Year {
            label: if n < 12 {
                format!("{year}*")
            } else {
                year.clone()
            },
            year,
            months: n,
            share: (a / (a + o) * 1000.0).round() / 10.0,
        })
        .collect();
    (months, years)
}
fn month_label(month: &str) -> String {
    let names = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let number = month
        .get(5..)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1);
    format!(
        "{} {}",
        names[(number.saturating_sub(1)).min(11)],
        month.get(..4).unwrap_or(month)
    )
}

fn chart_frame(f: Frame) -> Frame {
    Frame {
        x: f.x + f.w * 0.12,
        y: f.y + f.h * 0.24,
        w: f.w * 0.82,
        h: f.h * 0.56,
    }
}
fn cowork_frame(f: Frame) -> (Frame, Frame) {
    let h = f.h * 0.3;
    (
        Frame { h, ..f },
        Frame {
            y: f.y + h + f.h * 0.06,
            h: f.h - h - f.h * 0.06,
            ..f
        },
    )
}
fn field_plot(w: f64, h: f64) -> Frame {
    let ph = (h * 0.36).min(380.0);
    Frame {
        x: 0.0,
        y: h - ph - h * 0.04,
        w,
        h: ph,
    }
}

fn point(plot: Frame, index: usize, series: usize, months: &[Month]) -> (f64, f64) {
    let max = months
        .iter()
        .map(|m| m.openai.max(m.anthropic))
        .fold(1.0, f64::max);
    let m = &months[index];
    (
        plot.x + index as f64 / (months.len() - 1).max(1) as f64 * plot.w,
        plot.y + plot.h - (if series == 0 { m.openai } else { m.anthropic }) / max * plot.h,
    )
}
fn hidden(mut bar: Bar) -> Bar {
    bar.opacity = 0.0;
    bar
}
fn layers(items: &[(&'static str, f64)]) -> BTreeMap<&'static str, f64> {
    items.iter().copied().collect()
}
fn mark(f: Frame, c: Colors, scale: f64) -> Pose {
    let s = f.w.min(f.h) * scale / 269.73;
    let ox = f.x + (f.w - 268.58 * s) / 2.0;
    let oy = f.y + (f.h - 269.73 * s) / 2.0;
    let widths = [221.4, 255.32, 221.4, 93.12];
    let bars: Vec<_> = (0..4)
        .map(|i| Bar {
            x: ox,
            y: oy + (i as f64 * 72.78 + 51.39 / 2.0) * s,
            len: (widths[i] + 13.25) * s,
            th: 51.39 * s,
            rot: 0.0,
            cap: 13.25 * s,
            base: 0.0,
            color: c.blue,
            opacity: 1.0,
        })
        .collect();
    let lines = (0..28)
        .map(|i| {
            let mut b = bars[i % 4];
            b.th *= 0.25;
            b.len *= 0.9;
            b.cap = 2.0;
            b.base = 2.0;
            hidden(b)
        })
        .collect();
    Pose {
        bars,
        lines,
        dot: Dot {
            x: ox + 242.88 * s,
            y: oy + 218.33 * s,
            r: 25.7 * s,
            color: c.blue,
            opacity: 1.0,
        },
        layers: layers(&[]),
    }
}
fn columns(
    f: Frame,
    c: Colors,
    years: &[Year],
    fill: Color,
    radius: f64,
    layer: &[(&'static str, f64)],
) -> Pose {
    let plot = chart_frame(f);
    let step = plot.w / years.len() as f64;
    let th = step * 0.5;
    let bars: Vec<_> = years
        .iter()
        .enumerate()
        .map(|(i, y)| Bar {
            x: plot.x + (i as f64 + 0.5) * step,
            y: plot.y + plot.h,
            len: (y.share / 80.0 * plot.h).max(2.0),
            th,
            rot: -90.0,
            cap: th * radius,
            base: 0.0,
            color: fill,
            opacity: 1.0,
        })
        .collect();
    let mut visible = layers(&[("axes", 1.0)]);
    visible.extend(layer.iter().copied());
    let top = bars[3];
    let lines = (0..28)
        .map(|i| {
            let mut b = bars[i % 4];
            b.len = 0.0;
            b.th *= 0.3;
            hidden(b)
        })
        .collect();
    let _ = c;
    Pose {
        bars,
        lines,
        dot: Dot {
            x: top.x,
            y: top.y - top.len - 44.0,
            r: 9.0f64.min(th * 0.1),
            color: fill,
            opacity: 1.0,
        },
        layers: visible,
    }
}
fn theme_pose(f: Frame, c: Colors, years: &[Year], k: usize) -> Pose {
    let inset = f.w.min(f.h) * 0.04;
    let inner = Frame {
        x: f.x + inset,
        y: f.y + inset,
        w: f.w - inset * 2.0,
        h: f.h - inset * 2.0,
    };
    let pf = Frame {
        y: inner.y + inner.h * 0.08,
        h: inner.h * 0.92,
        ..inner
    };
    columns(
        pf,
        c,
        years,
        color(THEME_COLORS[k][7]),
        [0.3, 0.06, 0.0][k],
        &[("axes", 0.0), (["theme0", "theme1", "theme2"][k], 1.0)],
    )
}
fn line_pose(f: Frame, c: Colors, years: &[Year], months: &[Month], monthly: bool) -> Pose {
    let (_, chart) = cowork_frame(f);
    let plot = chart_frame(chart);
    let step = plot.w / years.len() as f64;
    let pt = 6.0f64.max(step * 0.5 * 0.16);
    let mut first = 0;
    let bars: Vec<_> = years
        .iter()
        .enumerate()
        .map(|(i, y)| {
            let middle = first as f64 + (y.months - 1) as f64 / 2.0;
            first += y.months;
            Bar {
                x: if monthly {
                    plot.x + middle / (months.len() - 1) as f64 * plot.w
                } else {
                    plot.x + (i as f64 + 0.5) * step
                },
                y: plot.y + plot.h - y.share / 80.0 * plot.h,
                len: pt,
                th: pt,
                rot: -90.0,
                cap: pt / 2.0,
                base: pt / 2.0,
                color: c.clay,
                opacity: if monthly { 0.0 } else { 1.0 },
            }
        })
        .collect();
    let last = bars[3];
    let mp = point(plot, months.len() - 1, 1, months);
    Pose {
        lines: (0..28).map(|i| hidden(bars[i % 4])).collect(),
        bars,
        dot: Dot {
            x: if monthly { mp.0 } else { last.x },
            y: if monthly { mp.1 } else { last.y },
            r: if monthly { 5.0 } else { pt * 0.5 },
            color: c.clay,
            opacity: if monthly { 1.0 } else { 0.0 },
        },
        layers: layers(&[
            ("cowork", 1.0),
            ("line4", if monthly { 0.0 } else { 1.0 }),
            ("monthly", if monthly { 1.0 } else { 0.0 }),
        ]),
    }
}

fn interpolate_bar(a: Bar, b: Bar, t: f64) -> Bar {
    Bar {
        x: lerp(a.x, b.x, t),
        y: lerp(a.y, b.y, t),
        len: lerp(a.len, b.len, t),
        th: lerp(a.th, b.th, t),
        rot: lerp(a.rot, b.rot, t),
        cap: lerp(a.cap, b.cap, t),
        base: lerp(a.base, b.base, t),
        color: mix(a.color, b.color, t),
        opacity: lerp(a.opacity, b.opacity, t),
    }
}
fn bar_path(b: Bar) -> String {
    let len = b.len.max(0.01);
    let h = b.th.max(0.01);
    let top = -h / 2.0;
    let rc = b.cap.min(h / 2.0).min(len / 2.0);
    let rb = b.base.min(h / 2.0).min(len / 2.0);
    format!(
        "M{rb:.2},{top:.2} H{:.2} A{rc:.2},{rc:.2} 0 0 1 {len:.2},{:.2} V{:.2} A{rc:.2},{rc:.2} 0 0 1 {:.2},{:.2} H{rb:.2} A{rb:.2},{rb:.2} 0 0 1 0,{:.2} V{:.2} A{rb:.2},{rb:.2} 0 0 1 {rb:.2},{top:.2} Z",
        len - rc,
        top + rc,
        top + h - rc,
        len - rc,
        top + h,
        top + h - rb,
        top + rb
    )
}
fn draw_bar(node: &Element, b: Bar) {
    attr(
        node,
        "display",
        if b.opacity < 0.002 { "none" } else { "inline" },
    );
    if b.opacity < 0.002 {
        return;
    }
    attr(node, "d", bar_path(b));
    attr(
        node,
        "transform",
        format!("translate({:.2} {:.2}) rotate({:.2})", b.x, b.y, b.rot),
    );
    attr(node, "fill", css(b.color));
    attr(node, "opacity", format!("{:.3}", b.opacity));
}
fn rect(out: &mut String, f: Frame, r: f64, class: &str, extra: &str) {
    let _ = write!(
        out,
        "<rect x='{:.2}' y='{:.2}' width='{:.2}' height='{:.2}' rx='{r:.2}' class='{class}' {extra}/>",
        f.x, f.y, f.w, f.h
    );
}
fn line(out: &mut String, x1: f64, y1: f64, x2: f64, y2: f64, extra: &str) {
    let _ = write!(
        out,
        "<line x1='{x1:.2}' y1='{y1:.2}' x2='{x2:.2}' y2='{y2:.2}' {extra}/>"
    );
}
fn text(out: &mut String, x: f64, y: f64, class: &str, body: &str, extra: &str) {
    let _ = write!(
        out,
        "<text x='{x:.2}' y='{y:.2}' class='{class}' {extra}>{}</text>",
        escape(body)
    );
}
#[allow(clippy::too_many_arguments)] // Declarative SVG axes keep their geometry and styling together.
fn axes_markup(
    f: Frame,
    years: &[Year],
    ink: Color,
    soft: Color,
    rule: Color,
    font: &str,
    title_class: &str,
    upper: bool,
) -> String {
    let plot = chart_frame(f);
    let mut out = String::new();
    for value in [0, 20, 40, 60, 80] {
        let y = plot.y + plot.h - value as f64 / 80.0 * plot.h;
        line(
            &mut out,
            plot.x,
            y,
            plot.x + plot.w,
            y,
            &format!(
                "stroke='{}' stroke-width='{}'",
                css(if value == 0 { soft } else { rule }),
                if value == 0 { 1.4 } else { 1.0 }
            ),
        );
        text(
            &mut out,
            plot.x - 10.0,
            y + 4.0,
            &format!("{font} st-tick"),
            &format!("{value}%"),
            &format!("fill='{}' text-anchor='end'", css(soft)),
        );
    }
    let step = plot.w / years.len() as f64;
    for (i, year) in years.iter().enumerate() {
        let x = plot.x + (i as f64 + 0.5) * step;
        text(
            &mut out,
            x,
            plot.y + plot.h + 22.0,
            &format!("{font} st-xlabel"),
            &year.label,
            &format!("fill='{}' text-anchor='middle'", css(ink)),
        );
        text(
            &mut out,
            x,
            plot.y + plot.h - year.share / 80.0 * plot.h - 14.0,
            &format!("{font} st-value"),
            &format!("{:.0}%", year.share),
            &format!("fill='{}' text-anchor='middle' data-value='{i}'", css(ink)),
        );
    }
    text(
        &mut out,
        f.x + f.w * 0.04,
        f.y + f.h * 0.07,
        &format!("{font} {title_class}"),
        &if upper {
            TITLE.to_uppercase()
        } else {
            TITLE.into()
        },
        &format!("fill='{}'", css(ink)),
    );
    text(
        &mut out,
        f.x + f.w * 0.04,
        f.y + f.h * 0.07 + 22.0,
        &format!("{font} st-csub"),
        "Share of HN company mentions naming Anthropic or Claude",
        &format!("fill='{}'", css(soft)),
    );
    text(
        &mut out,
        f.x + f.w * 0.04,
        plot.y + plot.h + 50.0,
        &format!("{font} st-note"),
        &format!(
            "* {} months of {} · Source: D-HN, Hacker News comments",
            years.last().unwrap().months,
            years.last().unwrap().year
        ),
        &format!("fill='{}'", css(soft)),
    );
    out
}

/// Build the anchored SVG layers in local coordinates; `render_stage` translates them.
fn build_layers(
    stage: &mut Stage,
    c: Colors,
    years: &[Year],
    months: &[Month],
    chart_size: Frame,
    cowork_size: Frame,
) -> Result<(), JsValue> {
    stage.layers.clear();
    let behind = svg("g", "st-layers", &stage.svg)?;
    let top = svg("g", "st-top", &stage.svg)?;
    let layer = |id: &str, markup: &str, parent: &Element| -> Result<Element, JsValue> {
        let e = svg("g", &format!("st-layer st-layer--{id}"), parent)?;
        e.set_inner_html(markup);
        attr(&e, "opacity", 0);
        attr(&e, "display", "none");
        Ok(e)
    };
    let f = chart_size.local();
    stage.layers.insert(
        "axes",
        (
            layer(
                "axes",
                &axes_markup(f, years, c.ink, c.soft, c.rule, "f-ui", "st-ctitle", false),
                &behind,
            )?,
            Some(Anchor::Chart),
        ),
    );
    let mut diag = String::new();
    let w = 280.0f64.min(f.w * 0.62);
    let (x, y) = (f.x + f.w - w - f.w * 0.02, f.y + f.h * 0.2);
    rect(&mut diag, Frame { x, y, w, h: 130.0 }, 10.0, "st-diag", "");
    text(
        &mut diag,
        x + 14.0,
        y + 22.0,
        "f-mono st-diag-title",
        "richview check --format json",
        "",
    );
    for (i, body) in [
        format!("data: {} sourced rows", years.len()),
        "encoding: x year, y share".into(),
        "geometry: inspect check output".into(),
        "values: all numeric".into(),
    ]
    .iter()
    .enumerate()
    {
        let _ = write!(
            diag,
            "<g data-diag='{i}'><circle cx='{}' cy='{}' r='6' class='st-ok'/><path d='M{} {} l2.2 2.4 l4 -4.6' class='st-ok-tick'/>",
            x + 20.0,
            y + 44.0 + i as f64 * 24.0,
            x + 17.0,
            y + 44.0 + i as f64 * 24.0
        );
        text(
            &mut diag,
            x + 34.0,
            y + 48.0 + i as f64 * 24.0,
            "f-mono st-diag-row",
            body,
            "",
        );
        diag.push_str("</g>");
    }
    stage
        .layers
        .insert("diag", (layer("diag", &diag, &top)?, Some(Anchor::Chart)));
    for k in 0..3 {
        let inset = f.w.min(f.h) * 0.04;
        let inner = Frame {
            x: f.x + inset,
            y: f.y + inset,
            w: f.w - 2.0 * inset,
            h: f.h - 2.0 * inset,
        };
        let pf = Frame {
            y: inner.y + inner.h * 0.08,
            h: inner.h * 0.92,
            ..inner
        };
        let names = ["sleek", "scientific", "brutalist"];
        let id = ["theme0", "theme1", "theme2"][k];
        let mut content = String::new();
        rect(
            &mut content,
            Frame {
                x: inner.x - 10.0,
                y: inner.y - 10.0,
                w: inner.w + 20.0,
                h: inner.h + 20.0,
            },
            [16.0, 3.0, 0.0][k],
            "",
            &format!(
                "fill='{}' stroke='{}' stroke-width='{}'",
                THEME_COLORS[k][0],
                THEME_COLORS[k][6],
                if k == 2 { 2 } else { 1 }
            ),
        );
        if k == 2 {
            rect(
                &mut content,
                Frame {
                    x: inner.x,
                    y: inner.y + 4.0,
                    w: inner.w,
                    h: 5.0,
                },
                0.0,
                "",
                &format!("fill='{}'", THEME_COLORS[k][3]),
            );
        }
        text(
            &mut content,
            inner.x + inner.w - 12.0,
            inner.y + inner.h - 12.0,
            "f-mono st-tbadge",
            &format!("styles: [{}]", names[k]),
            &format!("fill='{}' text-anchor='end'", THEME_COLORS[k][4]),
        );
        content.push_str(&axes_markup(
            pf,
            years,
            color(THEME_COLORS[k][3]),
            color(THEME_COLORS[k][4]),
            color(THEME_COLORS[k][6]),
            ["f-sleek", "f-sci", "f-brut"][k],
            &format!("st-ttitle st-ttitle--{}", names[k]),
            k == 2,
        ));
        stage
            .layers
            .insert(id, (layer(id, &content, &behind)?, Some(Anchor::Chart)));
    }
    let f = cowork_size.local();
    let (ed, chart) = cowork_frame(f);
    let plot = chart_frame(chart);
    let fs = 15.0f64.min(ed.h / 7.6).min((ed.w - 40.0) / (49.0 * 0.6));
    let lh = fs * 1.75;
    let x0 = ed.x + 20.0;
    let y0 = ed.y + lh * 1.1;
    let mut cowork = String::new();
    rect(&mut cowork, ed, 12.0, "st-panel", "");
    for (i, body) in [
        "type: line",
        "data: { csv: data/hn-monthly.csv }",
        "encoding: { x: month, y: comments, color: company }",
    ]
    .iter()
    .enumerate()
    {
        text(
            &mut cowork,
            x0,
            y0 + i as f64 * lh,
            "f-mono st-code",
            body,
            &format!("font-size='{fs}' data-cowork='{i}'"),
        );
    }
    let _ = write!(
        cowork,
        "<g data-cursor transform='translate({x0} {y0})'><rect x='0' y='{}' width='2' height='{}' class='st-you'/><rect x='2' y='{}' width='34' height='{}' rx='3' class='st-you'/>",
        -fs * 0.9,
        fs * 1.15,
        -fs * 2.05,
        fs * 1.05
    );
    text(
        &mut cowork,
        7.0,
        -fs * 1.25,
        "f-ui st-cursor-label",
        "you",
        "",
    );
    cowork.push_str("</g><g data-agent>");
    let note_width = 250.0f64.min(ed.w * 0.55);
    rect(
        &mut cowork,
        Frame {
            x: ed.x + ed.w - note_width - 12.0,
            y: ed.y + ed.h - lh * 1.35,
            w: note_width,
            h: lh * 1.05,
        },
        6.0,
        "st-agent-note",
        "",
    );
    text(
        &mut cowork,
        ed.x + ed.w - note_width,
        ed.y + ed.h - lh * 0.6,
        "f-ui st-agent-text",
        &format!("agent: pulled {} months from the report", months.len()),
        &format!("font-size='{}'", fs * 0.82),
    );
    cowork.push_str("</g>");
    line(
        &mut cowork,
        plot.x,
        plot.y + plot.h,
        plot.x + plot.w,
        plot.y + plot.h,
        "class='st-baseline'",
    );
    stage.layers.insert(
        "cowork",
        (layer("cowork", &cowork, &behind)?, Some(Anchor::Cowork)),
    );
    let mut line4 = String::new();
    let mut d = String::new();
    for (i, year) in years.iter().enumerate() {
        let x = plot.x + (i as f64 + 0.5) * plot.w / years.len() as f64;
        let y = plot.y + plot.h - year.share / 80.0 * plot.h;
        let _ = write!(d, "{}{x:.1},{y:.1} ", if i == 0 { "M" } else { "L" });
        text(
            &mut line4,
            x,
            plot.y + plot.h + 22.0,
            "f-ui st-xlabel",
            &year.label,
            "text-anchor='middle'",
        );
    }
    let _ = write!(
        line4,
        "<path d='{d}' class='st-line st-line--clay' pathLength='1'/>"
    );
    stage.layers.insert(
        "line4",
        (layer("line4", &line4, &behind)?, Some(Anchor::Cowork)),
    );
    let mut monthly = String::from(
        "<path data-monthly='0' class='st-line st-line--ink' pathLength='1'/><path data-monthly='1' class='st-line st-line--clay' pathLength='1'/>",
    );
    if let Some((_, cross)) = months
        .iter()
        .enumerate()
        .find(|(_, m)| m.anthropic > m.openai)
    {
        monthly.push_str("<g class='st-cross' data-cross><circle r='5'/>");
        text(
            &mut monthly,
            0.0,
            0.0,
            "f-ui st-cross-label",
            &format!("{}: Anthropic passes OpenAI", month_label(&cross.month)),
            "text-anchor='end'",
        );
        monthly.push_str("</g>");
    }
    for (i, body) in ["OpenAI / ChatGPT", "Anthropic / Claude"]
        .iter()
        .enumerate()
    {
        text(
            &mut monthly,
            0.0,
            0.0,
            &format!(
                "f-ui st-series-label st-series-label--{}",
                if i == 0 { "openai" } else { "anthropic" }
            ),
            body,
            &format!("data-series='{i}' text-anchor='end'"),
        );
    }
    monthly.push_str("<g data-monthly-axis>");
    text(
        &mut monthly,
        0.0,
        0.0,
        "f-ui st-tick",
        &month_label(&months[0].month),
        "data-tick='0'",
    );
    text(
        &mut monthly,
        0.0,
        0.0,
        "f-ui st-tick",
        &month_label(&months.last().unwrap().month),
        "data-tick='1' text-anchor='end'",
    );
    monthly.push_str("</g>");
    stage
        .layers
        .insert("monthly", (layer("monthly", &monthly, &behind)?, None));
    // Actors stay above background layers and below diagnostic overlays.
    let actors = svg("g", "st-actors", &stage.svg)?;
    stage.bars = (0..4)
        .map(|_| svg("path", "", &actors))
        .collect::<Result<_, _>>()?;
    stage.lines = (0..28)
        .map(|_| svg("path", "", &actors))
        .collect::<Result<_, _>>()?;
    stage.dot = svg("circle", "", &actors)?;
    stage.svg.append_child(&top)?;
    Ok(())
}

impl Landing {
    fn rect(&self, anchor: Anchor) -> Frame {
        let frame = self.anchors.get(&anchor).map(rect_of).unwrap_or_default();
        if frame.w >= 8.0 && frame.h >= 8.0 {
            return frame;
        }
        // Hidden anchors (for example the workflow stage on phones) keep a plausible
        // viewport region so lines can still expand into the background field.
        Frame {
            x: self.width * 0.06,
            y: self.height * 0.42,
            w: self.width * 0.88,
            h: self.height * 0.42,
        }
    }
    fn anchor_visible(&self, anchor: Anchor) -> bool {
        self.anchors
            .get(&anchor)
            .map(rect_of)
            .is_some_and(|f| f.w >= 8.0 && f.h >= 8.0)
    }
    fn layout(&mut self) -> Result<(), JsValue> {
        self.width = window().inner_width()?.as_f64().unwrap_or(1200.0);
        self.height = window().inner_height()?.as_f64().unwrap_or(800.0);
        self.probe = self.height * if self.width < 900.0 { 0.8 } else { 0.5 };
        self.dpr = window().device_pixel_ratio().clamp(1.0, 2.0);
        self.canvas
            .set_width((self.width * self.dpr).round() as u32);
        self.canvas
            .set_height((self.height * self.dpr).round() as u32);
        attr(
            &self.stage.svg,
            "viewBox",
            format!("0 0 {} {}", self.width, self.height),
        );
        self.stage.svg.set_inner_html("");
        let chart = self.rect(Anchor::Chart);
        let cowork = self.rect(Anchor::Cowork);
        build_layers(
            &mut self.stage,
            self.colors,
            &self.years,
            &self.months,
            chart,
            cowork,
        )?;
        let scroll = window().scroll_y()?;
        for chapter in &mut self.chapters {
            chapter.top = chapter.element.get_bounding_client_rect().top() + scroll;
        }
        let index = |name: &str, fallback: f64| {
            self.chapters
                .iter()
                .position(|c| c.name == name)
                .map(|i| i as f64)
                .unwrap_or(fallback)
        };
        let elements = index("elements", 3.0);
        let starts = self.starts();
        let t_at = |y: f64| {
            let mut i = 0;
            while i + 1 < starts.len() && y >= starts[i + 1] {
                i += 1
            }
            let next = starts
                .get(i + 1)
                .copied()
                .unwrap_or(starts[i] + self.height);
            i as f64 + ((y - starts[i]) / (next - starts[i]).max(1.0)).clamp(0.0, 0.9999)
        };
        let settled = query(".product__stage").map(|stage| {
            let top = window()
                .get_computed_style(&stage)
                .ok()
                .flatten()
                .and_then(|c| c.get_property_value("top").ok())
                .map(|v| css_px(&v))
                .unwrap_or(0.0);
            stage.get_bounding_client_rect().top() + scroll - top
        });
        let fallback = (elements + 0.07, elements + 0.09, elements + 0.16);
        let cards = match settled {
            Some(y) if !self.reduced => {
                let h = self.height;
                let handoff = (t_at(y - 0.32 * h), t_at(y - 0.16 * h), t_at(y + 0.06 * h));
                if handoff.0 >= elements && handoff.2 < elements + 0.9 && handoff.0 < handoff.1 {
                    handoff
                } else {
                    fallback
                }
            }
            _ => fallback,
        };
        self.times = Times {
            demo: index("demo", 1.0),
            what: index("what", 2.0),
            elements,
            workflow: index("workflow", 4.0),
            why: index("why", 5.0),
            flagship: index("flagship", 6.0),
            signup: index("signup", 7.0),
            cards,
        };
        self.stage.keys = timeline(self.times);
        if let Some(deck) = self.anchors.get(&Anchor::Deck) {
            self.deck = deck_layout(rect_of(deck));
        }
        self.deck_active = None;
        self.update_target();
        self.last_field.clear();
        self.force = true;
        Ok(())
    }
    /// Scroll offsets where each chapter begins.
    ///
    /// The probe uses the viewport height captured by the most recent `layout()`
    /// (stored in `self.probe`) rather than `self.height`. On mobile the URL bar
    /// hides and shows during scroll — `self.height` tracks that so canvas and
    /// stage drawing stays sharp — but recomputing the probe from the live
    /// height would walk the chapter starts up and down under the user's finger
    /// and the active chapter would shift without any content moving.
    fn starts(&self) -> Vec<f64> {
        let probe = self.probe;
        self.chapters
            .iter()
            .enumerate()
            .map(|(i, c)| {
                if i == 0 {
                    0.0
                } else {
                    (c.top - probe).max(1.0)
                }
            })
            .collect()
    }
    fn update_target(&mut self) {
        let y = window().scroll_y().unwrap_or(0.0);
        let starts = self.starts();
        let end = (document()
            .document_element()
            .map(|e| e.scroll_height())
            .unwrap_or(0) as f64
            - self.height)
            .max(starts.last().copied().unwrap_or(0.0) + 1.0);
        let mut i = 0;
        while i < starts.len() - 1 && y >= starts[i + 1] {
            i += 1
        }
        let next = starts.get(i + 1).copied().unwrap_or(end);
        let local = ((y - starts[i]) / (next - starts[i]).max(1.0)).clamp(0.0, 0.9999);
        self.target = i as f64
            + if self.reduced {
                self.chapters[i].rest
            } else {
                local
            };
    }
    fn card_centres(&self) -> (Frame, [(f64, f64, f64, f64); 5]) {
        let deck = self.rect(Anchor::Deck);
        let layout = self.deck;
        let centres = layout
            .together
            .map(|(x, y, rz, s)| (deck.x + deck.w / 2.0 + x, deck.y + deck.h / 2.0 + y, rz, s));
        (deck, centres)
    }
    fn cards_pose(&self, visible: bool) -> Pose {
        let c = self.colors;
        let (deck, centres) = self.card_centres();
        if deck.w < 8.0 {
            return hidden_pose(deck, c);
        }
        let (w, h) = (self.deck.w, self.deck.h);
        let card = |i: usize| {
            let (cx, cy, rz, s) = centres[i];
            let mut bar = card_bar(cx, cy, w * s, h * s, rz, s, c.blue);
            bar.opacity = if visible { 1.0 } else { 0.0 };
            bar
        };
        let bars = (0..4).map(card).collect();
        let lines = (0..28)
            .map(|j| {
                let mut bar = card(if j == 0 { 4 } else { j % 4 });
                if j != 0 {
                    bar.opacity = 0.0;
                }
                bar
            })
            .collect();
        let (cx, cy, _, _) = centres[4];
        Pose {
            bars,
            lines,
            dot: Dot {
                x: cx,
                y: cy,
                r: 0.0,
                color: c.blue,
                opacity: if visible { 1.0 } else { 0.0 },
            },
            layers: layers(&[]),
        }
    }
    fn pose(&self, key: Key) -> Pose {
        let c = self.colors;
        match key {
            // Hidden where the columns will stand, so they grow in place when they come.
            Key::Away => hidden_pose(self.rect(Anchor::Chart), c),
            Key::Mark(anchor) => {
                let f = self.rect(anchor);
                let mut pose = mark(f, c, if anchor == Anchor::Hero { 0.8 } else { 0.78 });
                if !self.anchor_visible(anchor) {
                    pose = hidden_pose(f, c);
                }
                pose
            }
            Key::Columns { diag } => columns(
                self.rect(Anchor::Chart),
                c,
                &self.years,
                c.clay,
                0.22,
                if diag { &[("diag", 1.0)] } else { &[] },
            ),
            Key::Theme(k) => theme_pose(self.rect(Anchor::Chart), c, &self.years, k),
            Key::Cards { visible } => self.cards_pose(visible),
            Key::Line { monthly } => {
                let f = self.rect(Anchor::Cowork);
                let mut pose = line_pose(f, c, &self.years, &self.months, monthly);
                if !self.anchor_visible(Anchor::Cowork) {
                    for bar in pose.bars.iter_mut() {
                        bar.opacity = 0.0;
                    }
                    pose.dot.opacity = 0.0;
                    // Without the workflow stage the lines first appear with the field.
                    pose.layers = layers(&[]);
                }
                pose
            }
            Key::Field => {
                let f = self.rect(Anchor::Cowork);
                let mut pose = line_pose(f, c, &self.years, &self.months, true);
                let end = point(
                    field_plot(self.width, self.height),
                    self.months.len() - 1,
                    1,
                    &self.months,
                );
                pose.dot = Dot {
                    x: end.0 - 8.0,
                    y: end.1,
                    r: 6.0,
                    color: c.clay,
                    opacity: 1.0,
                };
                pose.layers = layers(&[("monthly", 1.0)]);
                pose
            }
        }
    }
    fn render_stage(&mut self, intro: f64) {
        let t = self.current;
        let keys = &self.stage.keys;
        let mut i = 0;
        while i < keys.len() - 2 && t > keys[i + 1].0 {
            i += 1
        }
        let ((ta, ka), (tb, kb)) = (keys[i], keys[i + 1]);
        let u = span(t, ta, tb);
        let (a, b) = (self.pose(ka), self.pose(kb));
        let opening = intro < 1.0 && t < self.times.demo;
        for k in 0..4 {
            let mut bar = interpolate_bar(a.bars[k], b.bars[k], ease(stagger(u, k, 4, 0.28)));
            if opening {
                // The mark loads like a bar chart: each bar grows from the axis.
                bar.len *= back(stagger(intro, k, 5, 0.5), 1.15).max(0.0)
            }
            draw_bar(&self.stage.bars[k], bar);
        }
        for j in 0..28 {
            draw_bar(
                &self.stage.lines[j],
                interpolate_bar(a.lines[j], b.lines[j], ease(stagger(u, j, 28, 0.45))),
            );
        }
        let da = a.dot;
        let db = b.dot;
        let dt = out(stagger(u, 3, 4, 0.4));
        let pop = if opening {
            back(clamp((intro - 0.62) / 0.38), 2.2).max(0.0)
        } else {
            1.0
        };
        attr(
            &self.stage.dot,
            "cx",
            format!("{:.2}", lerp(da.x, db.x, dt)),
        );
        attr(
            &self.stage.dot,
            "cy",
            format!("{:.2}", lerp(da.y, db.y, dt)),
        );
        attr(
            &self.stage.dot,
            "r",
            format!("{:.2}", (lerp(da.r, db.r, dt) * pop).max(0.0)),
        );
        attr(&self.stage.dot, "fill", css(mix(da.color, db.color, dt)));
        attr(
            &self.stage.dot,
            "opacity",
            format!("{:.3}", lerp(da.opacity, db.opacity, dt)),
        );
        for (id, (layer, anchor)) in &self.stage.layers {
            let op = lerp(
                a.layers.get(id).copied().unwrap_or(0.0),
                b.layers.get(id).copied().unwrap_or(0.0),
                ease(u),
            );
            attr(layer, "opacity", format!("{op:.3}"));
            attr(layer, "display", if op < 0.002 { "none" } else { "inline" });
            if op >= 0.002 {
                if let Some(anchor) = anchor {
                    let f = self.rect(*anchor);
                    attr(
                        layer,
                        "transform",
                        format!("translate({:.2} {:.2})", f.x, f.y),
                    );
                }
                self.tick_layer(id, layer, t);
            }
        }
        // Hysteresis around chapter boundaries. Without it, scroll jitter near a
        // boundary (common on mobile where the address bar nudges scrollY) flips
        // `data-chapter` and the `is-current` class back and forth every frame,
        // which drives CSS transitions open-and-shut and reads as a visible
        // flicker. Require the progress to cross a small deadband before we
        // commit the new chapter.
        let chapter = {
            let raw = (t + 1e-6).floor().max(0.0) as usize;
            let fract = t - raw as f64;
            if self.active_chapter != usize::MAX {
                let active = self.active_chapter;
                if (raw == active + 1 && fract < 0.05)
                    || (active > 0 && raw + 1 == active && fract > 0.95)
                {
                    active
                } else {
                    raw
                }
            } else {
                raw
            }
        };
        if chapter != self.active_chapter {
            self.active_chapter = chapter;
            if let Some(root) = document().document_element() {
                attr(
                    &root,
                    "data-chapter",
                    &self.chapters[chapter.min(self.chapters.len() - 1)].name,
                );
            }
            for (i, c) in self.chapters.iter().enumerate() {
                class_toggle(&c.element, "is-current", i == chapter);
            }
        }
    }
    fn tick_layer(&self, id: &str, layer: &Element, t: f64) {
        let w = self.times.what;
        let flow = self.times.workflow;
        match id {
            "axes" => {
                if let Ok(nodes) = layer.query_selector_all("[data-value]") {
                    let p = out(span(t, w + 0.02, w + 0.16));
                    for i in 0..nodes.length() {
                        if let Some(n) = nodes.item(i) {
                            n.set_text_content(Some(&format!(
                                "{:.0}%",
                                self.years[i as usize].share * p
                            )));
                        }
                    }
                }
            }
            "diag" => {
                if let Ok(nodes) = layer.query_selector_all("[data-diag]") {
                    let p = span(t, w + 0.18, w + 0.27);
                    for i in 0..nodes.length() {
                        if let Some(n) = nodes.item(i).and_then(|n| n.dyn_into::<Element>().ok()) {
                            attr(&n, "opacity", format!("{:.3}", clamp(p * 5.0 - i as f64)));
                        }
                    }
                }
            }
            "cowork" => {
                let edit = span(t, flow - 0.02, flow + 0.1);
                let word = if edit < 0.5 {
                    &"bar"[..((1.0 - edit * 2.0) * 3.0).ceil() as usize]
                } else {
                    &"line"[..((edit - 0.5) * 2.0 * 4.0).ceil() as usize]
                };
                if let Ok(Some(n)) = layer.query_selector("[data-cowork='0']") {
                    n.set_text_content(Some(&format!("type: {word}")));
                }
                let agent = span(t, flow + 0.3, flow + 0.42);
                for selector in ["[data-cowork='1']", "[data-cowork='2']"] {
                    if let Ok(Some(n)) = layer.query_selector(selector) {
                        attr(&n, "opacity", format!("{:.3}", 0.25 + 0.75 * agent));
                    }
                }
                if let Ok(Some(n)) = layer.query_selector("[data-agent]") {
                    attr(&n, "opacity", format!("{:.3}", clamp(agent * 2.0)));
                }
                if let Ok(Some(n)) = layer.query_selector("[data-cursor]") {
                    let (ed, _) = cowork_frame(self.rect(Anchor::Cowork).local());
                    let fs = 15.0f64.min(ed.h / 7.6).min((ed.w - 40.0) / (49.0 * 0.6));
                    attr(
                        &n,
                        "transform",
                        format!(
                            "translate({} {})",
                            ed.x + 20.0 + (6 + word.len()) as f64 * fs * 0.6 + 2.0,
                            ed.y + fs * 1.75 * 1.1 + fs * 0.12
                        ),
                    );
                }
            }
            "line4" => {
                if let Ok(Some(n)) = layer.query_selector("path") {
                    attr(
                        &n,
                        "stroke-dasharray",
                        format!("{} 1", ease(span(t, flow + 0.02, flow + 0.14))),
                    );
                }
            }
            "monthly" => self.tick_monthly(layer, t),
            _ => {}
        }
    }
    fn tick_monthly(&self, layer: &Element, t: f64) {
        let stage = self.rect(Anchor::Cowork);
        let (_, chart) = cowork_frame(stage);
        let from = chart_frame(chart);
        let to = field_plot(self.width, self.height);
        let factor = ease(span(t, self.times.why + 0.1, self.times.why + 0.45));
        let plot = Frame {
            x: lerp(from.x, to.x, factor),
            y: lerp(from.y, to.y, factor),
            w: lerp(from.w, to.w, factor),
            h: lerp(from.h, to.h, factor),
        };
        let flow = self.times.workflow;
        let draw = if self.anchor_visible(Anchor::Cowork) {
            ease(span(t, flow + 0.46, flow + 0.6))
        } else {
            ease(span(t, self.times.why - 0.2, self.times.why + 0.2))
        };
        for series in 0..2 {
            if let Ok(Some(path)) = layer.query_selector(&format!("[data-monthly='{series}']")) {
                let mut d = String::new();
                for i in 0..self.months.len() {
                    let (x, y) = point(plot, i, series, &self.months);
                    let _ = write!(d, "{}{x:.1},{y:.1} ", if i == 0 { "M" } else { "L" });
                }
                attr(&path, "d", d);
                attr(&path, "stroke-dasharray", format!("{draw} 1"));
            }
            if let Ok(Some(label)) = layer.query_selector(&format!("[data-series='{series}']")) {
                let (x, y) = point(plot, self.months.len() - 1, series, &self.months);
                let ly = if series == 1 {
                    (self.months.len().saturating_sub(8)..self.months.len())
                        .map(|i| point(plot, i, series, &self.months).1)
                        .fold(f64::INFINITY, f64::min)
                        - 12.0
                } else {
                    y + 20.0
                };
                attr(&label, "x", format!("{:.1}", x - 10.0));
                attr(&label, "y", format!("{ly:.1}"));
                attr(&label, "opacity", clamp(draw * 3.0 - 2.0));
            }
        }
        if let Some(index) = self.months.iter().position(|m| m.anthropic > m.openai)
            && let Ok(Some(marker)) = layer.query_selector("[data-cross]")
        {
            let (x, y) = point(plot, index, 1, &self.months);
            if let Ok(Some(dot)) = marker.query_selector("circle") {
                attr(&dot, "cx", x);
                attr(&dot, "cy", y);
            }
            if let Ok(Some(text)) = marker.query_selector("text") {
                attr(&text, "x", x - 8.0);
                attr(&text, "y", y + 26.0);
            }
            attr(&marker, "opacity", clamp(draw * 4.0 - 3.0));
        }
        if let Ok(Some(axis)) = layer.query_selector("[data-monthly-axis]") {
            attr(&axis, "opacity", 1.0 - factor);
            for (i, x) in [plot.x, plot.x + plot.w].iter().enumerate() {
                if let Ok(Some(n)) = axis.query_selector(&format!("[data-tick='{i}']")) {
                    attr(&n, "x", x);
                    attr(&n, "y", plot.y + plot.h + 20.0);
                }
            }
        }
    }
    /// Five DOM cards: hand off from the stage, then focus one story at a time.
    fn update_deck(&mut self) {
        let Some(deck_el) = self.anchors.get(&Anchor::Deck).cloned() else {
            return;
        };
        let Ok(cards) = deck_el.query_selector_all("[data-element-card]") else {
            return;
        };
        if self.reduced {
            for i in 0..cards.length() {
                if let Some(card) = cards.item(i).and_then(|c| c.dyn_into::<HtmlElement>().ok()) {
                    for name in ["--card-transform", "--deck-in", "--cover"] {
                        let _ = card.style().remove_property(name);
                    }
                    class_toggle(&card, "is-focus", true);
                }
            }
            return;
        }
        let deck = rect_of(&deck_el);
        let layout = self.deck;
        let threshold = self.height * if self.width < 900.0 { 0.7 } else { 0.48 };
        let mut active = None;
        if let Ok(stories) = document().query_selector_all("[data-element]") {
            for i in 0..stories.length() {
                if let Some(e) = stories.item(i).and_then(|e| e.dyn_into::<Element>().ok())
                    && e.get_bounding_client_rect().top() < threshold
                {
                    active = Some(i as usize);
                }
            }
        }
        let (start, hidden, revealed) = self.times.handoff();
        let t = self.current;
        let deck_in = span(t, start - 0.002, hidden - 0.012);
        let changed = self.deck_active != Some(active) || self.force;
        for i in 0..cards.length() as usize {
            let Some(card) = cards
                .item(i as u32)
                .and_then(|c| c.dyn_into::<Element>().ok())
            else {
                continue;
            };
            let cover = 1.0 - span(t, hidden + i as f64 * 0.008, revealed + i as f64 * 0.008);
            style(&card, "--deck-in", &format!("{deck_in:.3}"));
            style(&card, "--cover", &format!("{cover:.3}"));
            if !changed {
                continue;
            }
            style(&card, "--card-w", &format!("{:.1}px", layout.w));
            style(&card, "--card-h", &format!("{:.1}px", layout.h));
            style(
                &card,
                "--card-left",
                &format!("{:.1}px", (deck.w - layout.w) / 2.0),
            );
            style(
                &card,
                "--card-top",
                &format!("{:.1}px", (deck.h - layout.h) / 2.0),
            );
            let (transform, z, focus, dim) = match active {
                None => {
                    let (x, y, rz, s) = layout.together[i];
                    (
                        format!(
                            "translate3d({x:.1}px, {y:.1}px, 0px) rotateZ({rz:.2}deg) scale({s:.4})"
                        ),
                        10 + i,
                        false,
                        false,
                    )
                }
                Some(a) => {
                    let d = i as f64 - a as f64;
                    if d == 0.0 {
                        (
                            format!(
                                "translate3d(0px, 0px, 40px) scale({:.4})",
                                layout.focus_scale
                            ),
                            20,
                            true,
                            false,
                        )
                    } else {
                        let side = d.signum();
                        let reach = d.abs().min(2.0);
                        let x = side * layout.w * (0.36 + 0.2 * reach) * layout.focus_scale;
                        let ry = -side * (16.0 + 10.0 * reach);
                        let z = -reach * 130.0;
                        (
                            format!(
                                "translate3d({x:.1}px, {:.1}px, {z:.1}px) rotateY({ry:.1}deg) scale({:.4})",
                                reach * 8.0,
                                0.82 * layout.focus_scale
                            ),
                            20 - d.abs() as usize * 3,
                            false,
                            true,
                        )
                    }
                }
            };
            style(&card, "--card-transform", &transform);
            style(&card, "z-index", &z.to_string());
            class_toggle(&card, "is-focus", focus);
            class_toggle(&card, "is-dim", dim);
        }
        if changed {
            self.deck_active = Some(active);
            if let Ok(links) = document().query_selector_all(".element-nav a") {
                for i in 0..links.length() {
                    if let Some(link) = links.item(i).and_then(|l| l.dyn_into::<Element>().ok()) {
                        class_toggle(&link, "is-active", active == Some(i as usize));
                    }
                }
            }
        }
    }

    fn render_field(&mut self) {
        let t = self.current;
        let times = self.times;
        let cursor = (self.months.len() - 1) as f64 * ease(span(t, 0.2, times.flagship + 0.5));
        let spot = (span(t, times.why + 0.1, times.why + 0.45)
            - span(t, times.signup - 0.2, times.signup + 0.15))
        .max(0.0);
        let presence = 1.0 - 0.85 * span(t, times.signup - 0.1, times.signup + 0.3);
        let root = document().document_element().unwrap();
        let field_color = window()
            .get_computed_style(&root)
            .ok()
            .flatten()
            .and_then(|c| c.get_property_value("--field-openai").ok())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| css(self.colors.openai));
        let key = format!(
            "{cursor:.3}|{spot:.3}|{presence:.3}|{:?}|{field_color}",
            self.pointer
        );
        if key == self.last_field && !self.force {
            return;
        }
        self.last_field = key;
        let plot = field_plot(self.width, self.height);
        let g = if self.width < 640.0 { 9.0 } else { 12.0 };
        let base = plot.y + plot.h;
        let max_r = g * 0.36;
        let max_count = self
            .months
            .iter()
            .map(|m| m.openai.max(m.anthropic))
            .fold(1.0, f64::max);
        let mut paths: Vec<String> = (0..4).map(|_| String::new()).collect();
        for series in 0..2 {
            let (dx, dy) = if series == 0 {
                (0.0, 0.0)
            } else {
                (g / 2.0, g / 2.0)
            };
            let mut x = plot.x + g / 2.0 + dx;
            while x < plot.x + plot.w {
                let mi = (x - plot.x) / plot.w * (self.months.len() - 1) as f64;
                let first = mi.floor() as usize;
                let fraction = mi - first as f64;
                let a = &self.months[first.min(self.months.len() - 1)];
                let b = &self.months[(first + 1).min(self.months.len() - 1)];
                let count = lerp(
                    if series == 0 { a.openai } else { a.anthropic },
                    if series == 0 { b.openai } else { b.anthropic },
                    fraction,
                );
                let top = base - count / max_count * plot.h;
                let ghost = mi > cursor;
                let group = series * 2 + usize::from(ghost);
                let mut y = base - g / 2.0 - dy;
                while y > top {
                    let depth = clamp((y - top) / (plot.h * 0.3));
                    let mut r = max_r * (0.3 + 0.7 * depth);
                    if ghost {
                        r *= 0.55
                    }
                    if let Some((px, py)) = self.pointer {
                        let d = (x - px).hypot(y - py);
                        if d < 110.0 {
                            r *= 1.0 + 0.75 * (1.0 - d / 110.0).powi(2)
                        }
                    }
                    let _ = write!(
                        paths[group],
                        "M{:.1},{y:.1}a{r:.2},{r:.2} 0 1 0 {:.2},0a{r:.2},{r:.2} 0 1 0 {:.2},0",
                        x + r,
                        -2.0 * r,
                        2.0 * r
                    );
                    y -= g;
                }
                x += g;
            }
        }
        let _ = self
            .ctx
            .set_transform(self.dpr, 0.0, 0.0, self.dpr, 0.0, 0.0);
        self.ctx.clear_rect(0.0, 0.0, self.width, self.height);
        // Behind reading content the field is a quiet texture; it comes forward for the
        // flagship chapter, where it is the subject.
        let quiet = lerp(0.42, 1.0, spot);
        for (series, col) in [field_color, css(self.colors.clay)].iter().enumerate() {
            self.ctx.set_fill_style_str(col);
            let alpha = if series == 0 {
                lerp(0.2, 0.62, spot)
            } else {
                lerp(0.32, 0.9, spot)
            } * presence
                * quiet;
            for ghost in 0..2 {
                let p = &paths[series * 2 + ghost];
                if p.is_empty() {
                    continue;
                }
                self.ctx
                    .set_global_alpha(alpha * if ghost == 1 { 0.5 } else { 1.0 });
                if let Ok(path) = Path2d::new_with_path_string(p) {
                    self.ctx.fill_with_path_2d(&path);
                }
            }
        }
        self.ctx.set_global_alpha(1.0);
        let i = cursor.round() as usize;
        if i != self.hud_month {
            self.hud_month = i;
            if let Some(month) = self.months.get(i) {
                for (selector, value) in [
                    ("[data-hud=month]", month_label(&month.month)),
                    ("[data-hud=openai]", group_number(month.openai)),
                    ("[data-hud=anthropic]", group_number(month.anthropic)),
                ] {
                    if let Some(e) = query(selector) {
                        e.set_text_content(Some(&value));
                    }
                }
            }
        }
    }
}
fn frame(timestamp: f64) {
    let moving = LANDING.with(|l| {
        let mut l = l.borrow_mut();
        let Some(state) = l.as_mut() else {
            return false;
        };
        state.scheduled = false;
        let dt = ((timestamp - state.last_time) / 1000.0).clamp(0.0, 0.05);
        state.last_time = timestamp;
        if state.reduced {
            state.current = state.target;
        } else {
            state.current = lerp(state.current, state.target, 1.0 - (-10.0 * dt).exp());
            if (state.current - state.target).abs() < 0.0004 {
                state.current = state.target;
            }
        }
        let intro = state
            .intro_start
            .map(|start| clamp((timestamp - start) / 1300.0))
            .unwrap_or(1.0);
        if intro >= 1.0 {
            state.intro_start = None;
        }
        state.render_stage(intro);
        state.render_field();
        state.update_deck();
        state.force = false;
        state.intro_start.is_some() || (state.current - state.target).abs() > 0.0004
    });
    // The stage shows only after its first frame is drawn: its nodes carry no fill before that.
    if let Some(root) = document().document_element()
        && !root
            .class_name()
            .split_whitespace()
            .any(|c| c == "is-ready")
    {
        class_toggle(&root, "is-ready", true);
    }
    if moving {
        request_frame();
    }
}
/// Redraw anchored poses after a host change, such as editing or scrolling the source.
pub(crate) fn invalidate() {
    LANDING.with(|l| {
        if let Some(state) = l.borrow_mut().as_mut() {
            state.force = true;
        }
    });
    request_frame();
}
fn group_number(value: f64) -> String {
    let input = format!("{value:.0}");
    let mut out = String::new();
    for (i, c) in input.chars().enumerate() {
        if i > 0 && (input.len() - i) % 3 == 0 {
            out.push(',')
        }
        out.push(c)
    }
    out
}
fn request_frame() {
    let schedule = LANDING.with(|l| {
        let mut l = l.borrow_mut();
        let Some(state) = l.as_mut() else {
            return false;
        };
        if state.scheduled {
            return false;
        }
        state.scheduled = true;
        true
    });
    if schedule {
        RAF.with(|r| {
            if let Some(callback) = r.borrow().as_ref() {
                let _ = window().request_animation_frame(callback.as_ref().unchecked_ref());
            }
        });
    }
}
fn class_toggle(element: &Element, name: &str, active: bool) {
    let mut classes = element
        .class_name()
        .split_whitespace()
        .filter(|c| *c != name)
        .map(str::to_string)
        .collect::<Vec<_>>();
    if active {
        classes.push(name.into())
    }
    element.set_class_name(&classes.join(" "));
}
fn data_notes() -> Result<(), JsValue> {
    for (selector, body) in [
        (
            "[data-source]",
            "Background data: Hacker News comments mentioning OpenAI/ChatGPT vs Anthropic/Claude (source D-HN, from the flagship report). One tech community’s comments: this measures what Hacker News commenters talk about, not public opinion.",
        ),
        (
            "[data-source-method]",
            "Exact monthly counts over the complete Hacker News item table (ClickHouse public copy); a comment counts for a company when its text matches openai|chatgpt or anthropic|claude as whole words.",
        ),
        (
            "[data-source-caveat]",
            "One tech community’s comments: this measures what Hacker News commenters talk about, not public opinion.",
        ),
    ] {
        let nodes = document().query_selector_all(selector)?;
        for i in 0..nodes.length() {
            if let Some(node) = nodes.item(i) {
                node.set_text_content(Some(body));
            }
        }
    }
    if let (Some(button), Some(about)) = (query(".hud button"), query("#hud-about")) {
        let b = button.clone();
        let a = about.clone();
        on(&button.clone().into(), "click", move |_| {
            let open = a.has_attribute("hidden");
            if open {
                a.remove_attribute("hidden").ok();
            } else {
                attr(&a, "hidden", "");
            }
            attr(&b, "aria-expanded", open);
        });
        let target: web_sys::EventTarget = document().into();
        on(&target, "keydown", move |event| {
            if let Ok(event) = event.dyn_into::<web_sys::KeyboardEvent>()
                && event.key() == "Escape"
                && !about.has_attribute("hidden")
            {
                attr(&about, "hidden", "");
                attr(&button, "aria-expanded", "false");
                if let Some(button) = button.dyn_ref::<HtmlElement>() {
                    let _ = button.focus();
                }
            }
        });
    }
    let notes = document().query_selector_all("details[data-json]")?;
    for i in 0..notes.length() {
        if let Some(details) = notes.item(i).and_then(|n| n.dyn_into::<Element>().ok()) {
            let d = details.clone();
            on(&details.into(), "toggle", move |_| {
                if !d.has_attribute("open") || d.has_attribute("data-loaded") {
                    return;
                }
                let Some(url) = d.get_attribute("data-json") else {
                    return;
                };
                let Ok(Some(pre)) = d.query_selector("pre") else {
                    return;
                };
                attr(&d, "data-loaded", "true");
                let retry = d.clone();
                spawn_local(async move {
                    match fetch_text(&url).await {
                        Ok(raw) => pre.set_text_content(Some(&raw)),
                        Err(_) => {
                            pre.set_text_content(Some(
                                "Could not load the check output. Close and reopen to retry.",
                            ));
                            retry.remove_attribute("data-loaded").ok();
                        }
                    }
                });
            });
        }
    }
    Ok(())
}

pub(crate) fn start() -> Result<(), JsValue> {
    if query("body[data-richview-page='landing']").is_none() {
        return Ok(());
    }
    // The retained diagnostics disclosure works independently of optional animation nodes.
    data_notes()?;
    let (Some(canvas), Some(stage_svg)) = (query("#rv-field"), query("#rv-stage")) else {
        return Ok(());
    };
    let canvas = canvas.dyn_into::<HtmlCanvasElement>()?;
    let ctx = canvas
        .get_context("2d")?
        .ok_or_else(|| js_error("2D canvas unavailable"))?
        .dyn_into::<CanvasRenderingContext2d>()?;
    let nodes = document().query_selector_all("[data-ch]")?;
    let mut chapters = vec![];
    for i in 0..nodes.length() {
        if let Some(e) = nodes.item(i).and_then(|e| e.dyn_into::<Element>().ok()) {
            chapters.push(Chapter {
                top: 0.0,
                rest: e
                    .get_attribute("data-rest")
                    .and_then(|r| r.parse().ok())
                    .unwrap_or(0.5),
                name: e.get_attribute("data-ch").unwrap_or_default(),
                element: e,
            });
        }
    }
    if chapters.is_empty() {
        return Ok(());
    }
    let anchors = ANCHORS
        .iter()
        .filter_map(|(anchor, selector)| query(selector).map(|e| (*anchor, e)))
        .collect();
    let reduced = window()
        .match_media("(prefers-reduced-motion: reduce)")?
        .is_some_and(|m| m.matches());
    let colors = read_colors();
    let (months, years) = data();
    let dot = svg("circle", "", &stage_svg)?;
    let mut state = Landing {
        canvas,
        ctx,
        chapters,
        anchors,
        months,
        years,
        colors,
        stage: Stage {
            svg: stage_svg,
            bars: vec![],
            lines: vec![],
            dot,
            layers: BTreeMap::new(),
            keys: vec![],
        },
        times: Times::default(),
        width: 0.0,
        height: 0.0,
        probe: 0.0,
        dpr: 1.0,
        reduced,
        target: 0.0,
        current: 0.0,
        last_time: now(),
        scheduled: false,
        force: true,
        pointer: None,
        intro_start: if !reduced && window().scroll_y()? < 2.0 {
            Some(now())
        } else {
            None
        },
        last_field: String::new(),
        hud_month: usize::MAX,
        active_chapter: usize::MAX,
        deck: DeckLayout::default(),
        deck_active: None,
    };
    if let Some(root) = document().document_element() {
        class_toggle(&root, "js", true);
        class_toggle(&root, "motion-reduced", reduced);
        class_toggle(&root, "motion-ok", !reduced);
        attr(&root, "data-landing-engine", "rust");
    }
    state.layout()?;
    state.current = state.target;
    LANDING.with(|l| *l.borrow_mut() = Some(state));
    RAF.with(|r| *r.borrow_mut() = Some(Closure::wrap(Box::new(frame) as Box<dyn FnMut(f64)>)));
    let target: web_sys::EventTarget = window().into();
    on(&target, "scroll", |_| {
        LANDING.with(|l| {
            if let Some(s) = l.borrow_mut().as_mut() {
                s.update_target();
            }
        });
        request_frame();
    });
    on(&target, "resize", |_| {
        // Mobile browsers fire `resize` every time the URL bar hides/shows while
        // scrolling. Running the full `layout()` on each tick reshuffles chapter
        // boundaries under the user's finger — the choreography appears to twitch.
        // Only re-layout when the width changes (orientation / rotation) or the
        // height moves by more than ~15% (true viewport change, not browser
        // chrome). Small height swings only resize the canvas backing store so
        // the drawing stays sharp without recomputing anchor positions.
        let new_width = window()
            .inner_width()
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let new_height = window()
            .inner_height()
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        LANDING.with(|l| {
            let mut guard = l.borrow_mut();
            let Some(s) = guard.as_mut() else { return };
            let width_changed = (new_width - s.width).abs() > 1.0;
            let height_delta = (new_height - s.height).abs();
            let height_tolerance = s.height * 0.15;
            if !width_changed && height_delta <= height_tolerance && s.height > 0.0 {
                s.height = new_height;
                let dpr = s.dpr;
                s.canvas.set_height((new_height * dpr).round() as u32);
                attr(
                    &s.stage.svg,
                    "viewBox",
                    format!("0 0 {} {}", s.width, new_height),
                );
                s.force = true;
                // Reconcile the active chapter against the current scroll
                // position. WebKit emits transient resize events during restore
                // where scrollY lags the final value; without reconciliation
                // the chapter sticks to the pre-resize anchor and reads as
                // drift (`what -> what -> elements`). update_target() reads
                // scrollY and selects the chapter whose range contains it.
                s.update_target();
                return;
            }
            if let Err(e) = s.layout() {
                error(&e)
            }
        });
        request_frame();
    });
    if window()
        .match_media("(pointer: fine)")?
        .is_some_and(|m| m.matches())
    {
        on(&target, "pointermove", |event| {
            if let Ok(event) = event.dyn_into::<web_sys::PointerEvent>() {
                LANDING.with(|l| {
                    if let Some(s) = l.borrow_mut().as_mut()
                        && !s.reduced
                    {
                        s.pointer = Some((event.client_x() as f64, event.client_y() as f64));
                    }
                });
                request_frame();
            }
        });
        if let Some(root) = document().document_element() {
            on(&root.into(), "pointerleave", |_| {
                LANDING.with(|l| {
                    if let Some(s) = l.borrow_mut().as_mut() {
                        s.pointer = None;
                    }
                });
                request_frame();
            });
        }
    }
    if let Some(media) = window().match_media("(prefers-reduced-motion: reduce)")? {
        on(&media.into(), "change", |_| {
            let reduced = window()
                .match_media("(prefers-reduced-motion: reduce)")
                .ok()
                .flatten()
                .is_some_and(|m| m.matches());
            if let Some(root) = document().document_element() {
                class_toggle(&root, "motion-reduced", reduced);
                class_toggle(&root, "motion-ok", !reduced);
            }
            LANDING.with(|l| {
                if let Some(s) = l.borrow_mut().as_mut() {
                    s.reduced = reduced;
                    s.pointer = None;
                    s.intro_start = None;
                    if let Err(e) = s.layout() {
                        error(&e)
                    }
                }
            });
            request_frame();
        });
    }
    spawn_local(async {
        if let Ok(ready) = document().fonts().ready() {
            let _ = JsFuture::from(ready).await;
            LANDING.with(|l| {
                if let Some(s) = l.borrow_mut().as_mut()
                    && let Err(e) = s.layout()
                {
                    error(&e)
                }
            });
            request_frame();
        }
    });
    request_frame();
    Ok(())
}
