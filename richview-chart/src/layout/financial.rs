//! Candlestick and OHLC charts: one mark per row that shows where a price opened, peaked, bottomed
//! and closed. A candle draws a wick from low to high and a body from open to close; an OHLC bar
//! draws a vertical line from low to high with a tick left at the open and right at the close.
//! Marks are coloured by direction (close at or above the open is Up), and direction is also in
//! the shape: Up bodies are hollow and Down bodies filled, ticks sit at their values (FEA-CH-57).
//! A row whose high or low contradicts its open and close fails the panel and names the row;
//! RichView never repairs a price (FEA-CH-42, FEA-CH-70).

use super::kit::{self, ValueSide};
use super::types::Facets;
use super::{Ctx, Painted, Style};
use crate::axis::Zero;
use crate::diag::Level;
use crate::geometry::{Anchor, Face, Fill, Item, Observation, Path, Readout, ReadoutLine, Rect, Role, Stroke, Swatch};
use crate::numbers;
use crate::rows::{self, Num};
use serde_json::json;

const RATIO: f64 = 16.0 / 9.0;
const CHANNELS: [&str; 4] = ["open", "high", "low", "close"];
const KINDS: [&str; 2] = ["Up", "Down"];
/// A body is hollow from this width on; thinner bodies are too small to read as an outline.
const HOLLOW_FROM: f64 = 4.0;
/// The gap between the plot's right edge and the last close printed beside it.
const TAG_GAP: f64 = 6.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Glyph {
    Candle,
    Bar,
}

impl Glyph {
    fn noun(self) -> &'static str {
        match self {
            Glyph::Candle => "candlestick",
            Glyph::Bar => "OHLC chart",
        }
    }
    /// Widest a mark grows, in pixels.
    fn cap(self) -> f64 {
        match self {
            Glyph::Candle => 28.0,
            Glyph::Bar => 18.0,
        }
    }
}

/// `extent` of `candles` from the exact rows; empty if there are none.
fn derived_extent(
    ctx: &Ctx,
    panel: usize,
    key: &str,
    candles: &[&Candle],
    extent: impl Fn(&[&Candle]) -> [f64; 2],
) -> [f64; 2] {
    let found: Option<[f64; 2]> = ctx.derive(panel, key, || {
        let e = crate::derived::coarse(extent(candles));
        (e[0].is_finite() && e[1].is_finite()).then_some(e)
    });
    found.unwrap_or([f64::INFINITY, f64::NEG_INFINITY])
}

pub(super) fn candlestick(ctx: &mut Ctx, facets: &Facets) -> Painted {
    paint(ctx, facets, Glyph::Candle)
}

pub(super) fn ohlc(ctx: &mut Ctx, facets: &Facets) -> Painted {
    paint(ctx, facets, Glyph::Bar)
}

struct Candle {
    row: usize,
    name: String,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
}

impl Candle {
    /// 0 for Up (close at or above the open), 1 for Down.
    fn kind(&self) -> usize {
        usize::from(self.close < self.open)
    }
}

fn signed(v: f64, format: &numbers::Format) -> String {
    let text = numbers::value(v, format);
    if v > 0.0 { format!("+{text}") } else { text }
}

/// `rect` at least one pixel thick along the value axis, so a flat mark stays visible.
fn visible(rect: Rect, horizontal: bool) -> Rect {
    if horizontal {
        Rect::new(rect.x, rect.y, rect.w.max(1.0), rect.h)
    } else {
        Rect::new(rect.x, rect.y, rect.w, rect.h.max(1.0))
    }
}

fn paint(ctx: &mut Ctx, facets: &Facets, glyph: Glyph) -> Painted {
    let block = ctx.block;
    let noun = glyph.noun();
    let x_field = block.encoding.x.clone().unwrap_or_default();
    let fields: Vec<Option<String>> = CHANNELS.iter().map(|c| block.channel(c)).collect();
    if fields.iter().any(Option::is_none) {
        let missing: Vec<&str> = CHANNELS
            .iter()
            .zip(&fields)
            .filter(|(_, f)| f.is_none())
            .map(|(c, _)| *c)
            .collect();
        ctx.check(
            "source-error",
            Level::Error,
            format!("The {noun} needs the encoding channels {}", missing.join(", ")),
            Some("Map open, high, low and close to numeric fields."),
            Some("chart.encoding".into()),
            None,
            json!({"missing": missing}),
        );
        return kit::failed(
            ctx,
            &format!("Cannot draw this {noun} · encoding lacks {}", missing.join(", ")),
            RATIO,
        );
    }
    let fields: Vec<String> = fields.into_iter().flatten().collect();
    let readout = block.readout_format(&block.y, ctx.locale);
    let label_format = block.label_format(&block.y, ctx.locale);
    let source = ctx.rows;
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut excluded = 0usize;
    let mut panels_candles: Vec<Vec<Candle>> = Vec::new();
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut candles = Vec::new();
        for &i in members {
            let row = &source[i];
            let mut values = [0.0f64; 4];
            let mut missing = false;
            let mut bad = false;
            for (k, field) in fields.iter().enumerate() {
                match rows::num(row, field) {
                    Num::Value(v) => values[k] = v,
                    Num::Missing => missing = true,
                    Num::Underflow(t) => {
                        ctx.underflow(i, field, &t);
                        failures[fi].get_or_insert_with(|| {
                            format!("Cannot draw this {noun} · {field} has a value too small to represent")
                        });
                        bad = true;
                    }
                    Num::Invalid(t) => {
                        ctx.check(
                            "non-numeric",
                            Level::Error,
                            format!("Row {} has {field} '{t}', which is not a number", i + 1),
                            Some("Write plain numbers; missing values may be empty or NA."),
                            Some(Ctx::row_path(i, field)),
                            None,
                            json!({"row": i}),
                        );
                        failures[fi].get_or_insert_with(|| {
                            format!("Cannot draw this {noun} · {field} has text that is not a number")
                        });
                        bad = true;
                    }
                }
            }
            if bad {
                continue;
            }
            if missing {
                excluded += 1;
                continue;
            }
            let [open, high, low, close] = values;
            let mut contradicts = false;
            if high < open.max(close) {
                let (which, v) = if open >= close {
                    ("open", open)
                } else {
                    ("close", close)
                };
                contradicts = true;
                ctx.check(
                    "source-error",
                    Level::Error,
                    format!(
                        "Row {} has high {} below its {which} {}",
                        i + 1,
                        numbers::value(high, &readout),
                        numbers::value(v, &readout)
                    ),
                    Some("The high is the largest price of the period; fix the row."),
                    Some(Ctx::row_path(i, &fields[1])),
                    None,
                    json!({"row": i, "high": high, "open": open, "close": close}),
                );
            }
            if low > open.min(close) {
                let (which, v) = if open <= close {
                    ("open", open)
                } else {
                    ("close", close)
                };
                contradicts = true;
                ctx.check(
                    "source-error",
                    Level::Error,
                    format!(
                        "Row {} has low {} above its {which} {}",
                        i + 1,
                        numbers::value(low, &readout),
                        numbers::value(v, &readout)
                    ),
                    Some("The low is the smallest price of the period; fix the row."),
                    Some(Ctx::row_path(i, &fields[2])),
                    None,
                    json!({"row": i, "low": low, "open": open, "close": close}),
                );
            }
            if contradicts {
                failures[fi].get_or_insert_with(|| {
                    format!(
                        "Cannot draw this {noun} · row {} has a high or low that contradicts its open and close",
                        i + 1
                    )
                });
                continue;
            }
            candles.push(Candle {
                row: i,
                name: rows::text(row, &x_field).unwrap_or_else(|| format!("Row {}", i + 1)),
                open,
                high,
                low,
                close,
            });
        }
        panels_candles.push(candles);
    }
    if failures.iter().all(Option::is_some) {
        let reason = failures.iter().flatten().next().cloned().unwrap_or_default();
        return kit::failed(ctx, &reason, RATIO);
    }
    if excluded > 0 {
        ctx.check(
            "excluded-observations",
            Level::Warning,
            format!(
                "{} without all four prices {} left out",
                super::count(excluded, "row", "rows"),
                super::verb(excluded, "is", "are")
            ),
            Some("Give every row an open, high, low and close."),
            Some("chart.encoding".into()),
            None,
            json!({"count": excluded}),
        );
        ctx.note.push(format!(
            "{} without all four prices {} not shown",
            super::count(excluded, "row", "rows"),
            super::verb(excluded, "is", "are")
        ));
    }
    // Whether a candle is up or down, and by how much it moved, jump with the data (a grid step
    // can make a close and an open equal), so the exact rows decide them (TYP-36).
    let moves: Vec<Vec<(usize, String)>> = panels_candles
        .iter()
        .enumerate()
        .map(|(fi, candles)| {
            ctx.derive(fi, "financial.moves", || {
                let moved = |c: &Candle| (c.kind(), signed(c.close - c.open, &readout));
                candles.iter().map(moved).collect()
            })
        })
        .collect();
    let present: Vec<String> = (0..2)
        .filter(|k| moves.iter().flatten().any(|m| m.0 == *k))
        .map(|k| KINDS[k].to_owned())
        .collect();
    ctx.set_series(&present);
    let slot_of = |kind: usize| present.iter().position(|n| n == KINDS[kind]).unwrap_or(0);
    // The longest panel sets the names, in source order.
    let longest = panels_candles.iter().map(Vec::len).max().unwrap_or(0);
    let names: Vec<String> = (0..longest)
        .map(|k| {
            panels_candles
                .iter()
                .find_map(|s| s.get(k).map(|c| c.name.clone()))
                .unwrap_or_default()
        })
        .collect();
    let plan = kit::plan_with(ctx, facets.len(), &names, 1.0, block.style.ratio.unwrap_or(RATIO), true);
    let extent = |candles: &[&Candle]| {
        candles.iter().fold([f64::INFINITY, f64::NEG_INFINITY], |[lo, hi], c| {
            [lo.min(c.low), hi.max(c.high)]
        })
    };
    let all: Vec<&Candle> = panels_candles.iter().flatten().collect();
    // The price extent sets the axis, so the exact rows decide it (TYP-36).
    let shared = derived_extent(ctx, 0, "financial.shared", &all, extent);
    let frames = plan.frames(facets.len(), ctx.opts.width);
    let title = block.y.label.clone().unwrap_or_else(|| fields[3].clone());
    // Each panel's last drawn period, the one whose close is printed (FEA-CH-28).
    let closing: Vec<Option<usize>> = panels_candles
        .iter()
        .zip(&moves)
        .map(|(candles, moves)| {
            (0..candles.len()).rev().find(|&c| {
                let kind = moves.get(c).map_or_else(|| candles[c].kind(), |m| m.0);
                !ctx.is_hidden(slot_of(kind))
            })
        })
        .collect();
    // The last close stands right of the plot at the close's own height, where a price tag
    // stands, so an upright chart keeps the width of the widest one beside its plot. The
    // author's `auto_annotations: false` keeps the whole width for the plot.
    let tag_room = if block.auto_annotations && !plan.horizontal {
        closing
            .iter()
            .zip(&panels_candles)
            .filter_map(|(c, candles)| c.map(|c| ctx.text_width(&numbers::value(candles[c].close, &readout))))
            .fold(0.0f64, f64::max)
    } else {
        0.0
    };
    let mut panels = Vec::new();
    for (fi, ((name, _), outer)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *outer, plan.title_room, name, reason));
            continue;
        }
        let candles = &panels_candles[fi];
        let mine: Vec<&Candle> = candles.iter().collect();
        let mut data = if block.style.free_scales {
            derived_extent(ctx, fi, "financial.own", &mine, extent)
        } else {
            shared
        };
        if !data[0].is_finite() {
            data = ctx.wanted().y.unwrap_or([0.0, 1.0]);
        } else {
            data = super::Fit::widen(data, ctx.wanted().y);
        }
        ctx.fit.y = Some(data);
        let side = ValueSide {
            data,
            zero: Zero::None,
            title: title.clone(),
            format: &label_format,
            min: block.y.min,
            max: block.y.max,
        };
        let inner = Rect::new(outer.x, outer.y, (outer.w - tag_room).max(outer.w / 2.0), outer.h);
        let frame = kit::frame(ctx, &plan, &inner, &side, fi == 0);
        let t = frame.thickness(1, glyph.cap()).max(1.0);
        let line_width = if t >= 6.0 { 1.5 } else { 1.0 };
        let surface = Style::rgba(ctx.style.surface);
        let mut marks: Vec<Item> = Vec::new();
        let mut observations = Vec::new();
        // The last drawn period: its series slot, observation, close and the far edge of its
        // mark along the category axis.
        let mut last: Option<(usize, usize, f64, f64)> = None;
        for (c, s) in candles.iter().enumerate() {
            let (kind, change_text) = moves[fi].get(c).cloned().unwrap_or_else(|| (s.kind(), String::new()));
            let slot = slot_of(kind);
            if ctx.is_hidden(slot) {
                continue;
            }
            let color = Style::rgba(ctx.series_color(slot));
            let hatch = ctx.series[slot].hatch;
            let c0 = frame.slot_start(c, 0, 1, t);
            let cm = c0 + t / 2.0;
            let band0 = frame.center(c) - frame.band / 2.0;
            let area = visible(frame.span(band0, band0 + frame.band, s.low, s.high), frame.horizontal);
            let wick = |from: f64, to: f64| Item::Line {
                path: Path::polyline(&[frame.pt(cm, from), frame.pt(cm, to)]),
                stroke: Stroke::solid(color, line_width),
                role: Role::Mark,
                series: Some(slot),
                obs: Some(observations.len()),
            };
            match glyph {
                Glyph::Candle => {
                    marks.push(wick(s.low, s.high));
                    let body = visible(frame.span(c0, c0 + t, s.open, s.close), frame.horizontal);
                    let up = kind == 0;
                    let hollow = up && hatch.is_none() && t >= HOLLOW_FROM;
                    marks.push(Item::Rect {
                        rect: body,
                        fill: Fill {
                            color: if hollow { surface } else { color },
                            hatch,
                            hatch_ink: hatch.map(|_| surface),
                            hatch_scale: 1.0,
                        },
                        stroke: (hollow || (up && hatch.is_some())).then(|| Stroke::solid(color, line_width)),
                        radius: [0.0; 4],
                        role: Role::Mark,
                        series: Some(slot),
                        obs: Some(observations.len()),
                    });
                }
                Glyph::Bar => {
                    marks.push(wick(s.low, s.high));
                    let tick = (t / 2.0).max(1.0);
                    for (value, from, to) in [
                        (s.open, c0 - (tick - t / 2.0), cm),
                        (s.close, cm, c0 + t + (tick - t / 2.0)),
                    ] {
                        marks.push(Item::Line {
                            path: Path::polyline(&[frame.pt(from, value), frame.pt(to, value)]),
                            stroke: Stroke::solid(color, line_width),
                            role: Role::Mark,
                            series: Some(slot),
                            obs: Some(observations.len()),
                        });
                    }
                }
            }
            let given = |name: &str, v: f64| ReadoutLine {
                name: name.into(),
                value: numbers::value(v, &readout),
                series: None,
                names_series: false,
                derived: false,
            };
            if closing[fi] == Some(c) {
                last = Some((slot, observations.len(), s.close, c0 + t));
            }
            observations.push(Observation {
                row: Some(s.row),
                dots: vec![],
                series: Some(slot),
                at: frame.pt(cm, s.high),
                wedge: None,
                area: Some(area),
                radius: 0.0,
                x: cm,
                readout: Readout {
                    heading: crate::time::parse_iso(&s.name)
                        .map(|(ms, precision)| crate::axis::full_date(ms, precision, ctx.calendar_locale()))
                        .unwrap_or_else(|| s.name.clone()),
                    lines: vec![
                        given("Open", s.open),
                        given("High", s.high),
                        given("Low", s.low),
                        ReadoutLine {
                            series: Some(slot),
                            ..given("Close", s.close)
                        },
                        ReadoutLine {
                            name: "Change".into(),
                            value: change_text,
                            series: Some(slot),
                            names_series: false,
                            derived: true,
                        },
                    ],
                }
                .into(),
            });
        }
        // The one automatic value: the last close at the close's own height (chart design,
        // 16 Nov 2026). Four numbers a period would be noise, so no other value is printed. An
        // upright chart sets it in the room it kept beside the plot, as a price tag, or 4 px
        // past the mark when periods this panel lacks follow it. A horizontal chart prints it
        // under the last row where the plot has room, and records it as dropped where it has
        // none.
        let plot = frame.plot;
        let value = last.map(|(slot, observation, close, edge)| {
            let text = numbers::value(close, &readout);
            let [x, y] = frame.pt(edge + 4.0, close);
            let ink = ctx.style.ink;
            let mut place = if frame.horizontal {
                let lh = ctx.line_height();
                ctx.text_mid(&text, x, y + lh / 2.0, Anchor::Middle, Face::Figures, Role::Label, ink)
            } else {
                let x = if closing[fi].is_some_and(|c| c + 1 == names.len()) {
                    plot.right() + TAG_GAP
                } else {
                    x
                };
                ctx.text_mid(&text, x, y, Anchor::Start, Face::Figures, Role::Label, ink)
            };
            // A close at the plot's end keeps its value inside the plot along the value axis.
            let b = place.bounds;
            if frame.horizontal {
                let dx = (plot.x - b.x).max(0.0) - (b.right() - plot.right()).max(0.0);
                place.x += dx;
                place.bounds.x += dx;
            } else {
                let dy = (plot.y - b.y).max(0.0) - (b.bottom() - plot.bottom()).max(0.0);
                place.y += dy;
                place.bounds.y += dy;
            }
            super::place::SparseValue {
                text,
                series: slot,
                observation,
                places: vec![place],
            }
        });
        let mut panel = kit::panel(ctx, outer, frame, marks, observations, name);
        let bounds = Rect::new(plot.x, plot.y, (outer.right() - plot.x).max(plot.w), plot.h);
        super::place::sparse_values_in(ctx, fi, &mut panel, 1, value.into_iter().collect(), &|_| true, bounds);
        panels.push(panel);
    }
    ctx.summary = match all.last() {
        Some(l) => format!(
            "{} {} from {} to {}; the last closed at {}.",
            all.len(),
            if all.len() == 1 { "period" } else { "periods" },
            all[0].name,
            l.name,
            numbers::value(l.close, &label_format)
        ),
        None => "No periods.".into(),
    };
    ctx.note.push(match glyph {
        Glyph::Candle => "wicks run from the low to the high, bodies from the open to the close; Up closed at or above its open".into(),
        Glyph::Bar => "each bar runs from the low to the high, with a tick left at the open and right at the close; Up closed at or above its open".into(),
    });
    let legend = kit::legend(
        ctx,
        if glyph == Glyph::Candle {
            Swatch::Square
        } else {
            Swatch::Line
        },
    );
    kit::finish(&plan, facets.len(), panels, legend)
}
