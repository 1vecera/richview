//! One placement search for every optional label: end names, point names and annotation text
//! (FEA-CH-27 to FEA-CH-29, FEA-CH-32). Placement never moves an observation; a label without a
//! clear position is dropped and recorded with its reason.

use super::{Ctx, Style};
use crate::block::AnnotationKind;
use crate::diag::Level;
use crate::geometry::{Anchor, Cmd, DropReason, Face, Fill, Item, Panel, Path, Rect, Rgba, Role, Shape, Stroke, Text};
use crate::rows;
use serde_json::{Value, json};

/// Least gap between a label and anything it must keep clear of.
const CLEAR: f64 = 2.0;
/// The least room between two of a line's value labels that stand side by side.
const VALUE_GAP: f64 = 8.0;
/// A point annotation's ring: radius and stroke width.
const RING: f64 = 5.0;
const RING_STROKE: f64 = 1.5;
/// How far two texts' line boxes may share space before they read as colliding: the label
/// check's own allowance (`layout/checks.rs`).
const TEXT_OVERLAP: f64 = 1.0;

/// Automatic values use an explicit family measure; other families keep their existing policy.
#[derive(Clone, Copy)]
pub(super) enum ValueLabels {
    /// A horizontal bar's value end label may cross soft gridlines; the caller breaks them
    /// round it, as round a name above its bar.
    Bars {
        horizontal: bool,
        stacked: bool,
    },
    Series,
    Density,
}

#[derive(Clone)]
struct ValueLabel<'a> {
    text: &'a str,
    series: usize,
    observation: usize,
    at: [f64; 2],
}

enum ValueInk {
    Box(Rect),
    Stroke([f64; 2], [f64; 2], f64),
    Area(Vec<Vec<[f64; 2]>>),
}

/// Spatial buckets keep a bounded label search from scanning the entire dense plot per try.
#[derive(Default)]
struct ValueField {
    ink: Vec<(ValueInk, Option<usize>, bool)>,
    cells: std::collections::HashMap<(i32, i32), Vec<usize>>,
    window: Option<Rect>,
    /// The line boxes of the level texts, as the label check compares them.
    texts: Vec<Rect>,
}

fn value_cells(r: Rect) -> impl Iterator<Item = (i32, i32)> {
    let (x0, x1) = ((r.x / 64.0).floor() as i32, (r.right() / 64.0).floor() as i32);
    let (y0, y1) = ((r.y / 64.0).floor() as i32, (r.bottom() / 64.0).floor() as i32);
    (x0..=x1).flat_map(move |x| (y0..=y1).map(move |y| (x, y)))
}

fn polygon_contains(points: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut inside = false;
    for edge in points.windows(2) {
        let [a, b] = [edge[0], edge[1]];
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            inside = !inside;
        }
    }
    inside
}

impl ValueField {
    fn insert(&mut self, ink: ValueInk, bounds: Rect, own: Option<usize>, grid: bool) {
        let bounds = if let Some(window) = self.window {
            let (x, y) = (bounds.x.max(window.x), bounds.y.max(window.y));
            let (right, bottom) = (bounds.right().min(window.right()), bounds.bottom().min(window.bottom()));
            if right < x || bottom < y {
                return;
            }
            Rect::new(x, y, right - x, bottom - y)
        } else {
            bounds
        };
        let index = self.ink.len();
        for cell in value_cells(bounds) {
            self.cells.entry(cell).or_default().push(index);
        }
        self.ink.push((ink, own, grid));
    }

    /// Whether `r` keeps clear of the ink, besides the mark of observation `own` and, with
    /// `over_grid`, the gridlines.
    fn clear(&self, r: Rect, own: Option<usize>, over_grid: bool) -> bool {
        self.free(r.inflate(CLEAR + 0.25), own, over_grid)
    }

    /// Whether `r` inside the bar of observation `own` keeps clear of everything drawn over that
    /// bar. The bar sets the text off from what lies round it, so only the part of `r`'s
    /// clearance within `bar` counts: a grouped bar's sibling, 1 px away, does not block it.
    /// A line box taller than a thin bar still reaches past it, so `r` itself must not meet
    /// another text, such as the sibling's value.
    fn clear_within(&self, r: Rect, bar: Rect, own: usize) -> bool {
        if self.texts.iter().any(|t| r.overlaps(t, TEXT_OVERLAP)) {
            return false;
        }
        let r = r.inflate(CLEAR + 0.25);
        let (left, top) = (r.x.max(bar.x), r.y.max(bar.y));
        let (right, bottom) = (r.right().min(bar.right()), r.bottom().min(bar.bottom()));
        right > left && bottom > top && self.free(Rect::new(left, top, right - left, bottom - top), Some(own), true)
    }

    /// Reserve a placed value: `rect` is its footprint, `text` its line box.
    fn insert_value(&mut self, rect: Rect, text: Rect) {
        self.insert(ValueInk::Box(rect), rect, None, false);
        self.texts.push(text);
    }

    /// Whether the area `r` touches no ink, besides the mark of observation `own` and, with
    /// `over_grid`, the gridlines.
    fn free(&self, r: Rect, own: Option<usize>, over_grid: bool) -> bool {
        let mut indices = Vec::new();
        for cell in value_cells(r) {
            if let Some(bucket) = self.cells.get(&cell) {
                indices.extend_from_slice(bucket);
            }
        }
        // Reuse the existing integer sort and retain one contact check per ink item.
        indices.sort_unstable();
        indices.dedup();
        indices.into_iter().all(|index| {
            let (ink, mark, grid) = &self.ink[index];
            if (own.is_some() && own == *mark) || (over_grid && *grid) {
                return true;
            }
            match ink {
                ValueInk::Box(b) => !r.overlaps(b, 0.0),
                ValueInk::Stroke(a, b, half) => !segment_touches_rect(*a, *b, &r.inflate(*half)),
                ValueInk::Area(rings) => !rings.iter().any(|ring| {
                    [[r.x, r.y], [r.right(), r.y], [r.right(), r.bottom()], [r.x, r.bottom()]]
                        .into_iter()
                        .any(|p| polygon_contains(ring, p))
                        || ring.windows(2).any(|e| segment_touches_rect(e[0], e[1], &r))
                }),
            }
        })
    }

    fn from_panel(ctx: &Ctx, panel: &Panel) -> Self {
        Self::from_items(ctx, panel, &|_| true)
    }

    /// The ink of the panel's items that `keep` holds: a layout leaves out what a value may
    /// stand on, such as a bullet's background bands.
    fn from_items(ctx: &Ctx, panel: &Panel, keep: &dyn Fn(&Item) -> bool) -> Self {
        let mut field = Self {
            window: Some(panel.frame.inflate(32.0)),
            ..Self::default()
        };
        for item in panel.items.iter().filter(|item| keep(item)) {
            match item {
                Item::Text(t) => {
                    let mut bounds = t.bounds;
                    if t.rotate != 0.0 {
                        let (sin, cos) = t.rotate.to_radians().sin_cos();
                        let points = [
                            [bounds.x, bounds.y],
                            [bounds.right(), bounds.y],
                            [bounds.right(), bounds.bottom()],
                            [bounds.x, bounds.bottom()],
                        ]
                        .map(|p| {
                            let (x, y) = (p[0] - t.x, p[1] - t.y);
                            [t.x + x * cos - y * sin, t.y + x * sin + y * cos]
                        });
                        let (left, top) = points
                            .iter()
                            .fold((f64::INFINITY, f64::INFINITY), |(x, y), p| (x.min(p[0]), y.min(p[1])));
                        let (right, bottom) =
                            points.iter().fold((f64::NEG_INFINITY, f64::NEG_INFINITY), |(x, y), p| {
                                (x.max(p[0]), y.max(p[1]))
                            });
                        bounds = Rect::new(left, top, right - left, bottom - top);
                    }
                    if let Some(s) = t.series.and_then(|s| ctx.series.get(s))
                        && s.name == t.text
                        && (bounds.right() < panel.plot.x || bounds.x > panel.plot.right())
                    {
                        let [x, y] = bounds.center();
                        let (w, h) = ((bounds.w + 4.0).max(24.0), (bounds.h + 4.0).max(24.0));
                        bounds = Rect::new(x - w / 2.0, y - h / 2.0, w, h);
                    }
                    if t.rotate == 0.0 {
                        field.texts.push(t.bounds);
                    }
                    field.insert(ValueInk::Box(bounds), bounds, None, false);
                }
                Item::Rect { rect, role, obs, .. } => {
                    field.insert(ValueInk::Box(*rect), *rect, *obs, *role == Role::Grid);
                }
                Item::Point {
                    center, radius, stroke, ..
                } => {
                    let r = radius + stroke.as_ref().map_or(0.0, |s| s.width / 2.0);
                    let bounds = Rect::new(center[0] - r, center[1] - r, 2.0 * r, 2.0 * r);
                    field.insert(ValueInk::Box(bounds), bounds, None, false);
                }
                Item::Line { path, stroke, role, .. } => {
                    for ring in path.flatten(0.25) {
                        for e in ring.windows(2) {
                            let bounds = Rect::new(
                                e[0][0].min(e[1][0]),
                                e[0][1].min(e[1][1]),
                                (e[0][0] - e[1][0]).abs(),
                                (e[0][1] - e[1][1]).abs(),
                            )
                            .inflate(stroke.width / 2.0 + 0.25);
                            field.insert(
                                ValueInk::Stroke(e[0], e[1], stroke.width / 2.0),
                                bounds,
                                None,
                                *role == Role::Grid,
                            );
                        }
                    }
                }
                Item::Area { path, .. } => {
                    if let Some(bounds) = path.bounds() {
                        field.insert(ValueInk::Area(path.flatten(0.25)), bounds, None, false);
                    }
                }
            }
        }
        field
    }
}

fn composite(foreground: Rgba, background: Rgba) -> Rgba {
    let a = foreground.0[3];
    Rgba(std::array::from_fn(|i| {
        if i == 3 {
            1.0
        } else {
            foreground.0[i] * a + background.0[i] * (1.0 - a)
        }
    }))
}

/// A vertical bar's value above its bar: its width, the height it takes from the bar's top (the
/// 3 px gap and the text), and the clearance it keeps from other text.
pub(super) fn bar_value_above(ctx: &Ctx, text: &str) -> [f64; 3] {
    let probe = ctx.text(text, 0.0, 0.0, Anchor::Start, Face::Figures, Role::Label, ctx.style.ink);
    [probe.bounds.w, 3.0 + probe.bounds.h, CLEAR + 0.25]
}

/// A value's text in the box `r`, attached by `anchor`: the runtime replaces a number laid out
/// from grid values with the shown one (TYP-36), which may be a digit shorter or longer, and the
/// text then keeps this side of its box: its middle on a column, the edge at a bar's end.
fn value_text(ctx: &Ctx, c: &ValueLabel, r: Rect, anchor: Anchor, background: Rgba) -> Option<Text> {
    let background = background.oklch();
    let ink = [ctx.style.ink, ctx.style.surface]
        .into_iter()
        .max_by(|a, b| crate::color::contrast(*a, background).total_cmp(&crate::color::contrast(*b, background)))?;
    if crate::color::contrast(ink, background) < 4.5 {
        return None;
    }
    let mut t = ctx.text(c.text, 0.0, 0.0, anchor, Face::Figures, Role::Label, ink);
    t.x += r.x - t.bounds.x;
    t.y += r.y - t.bounds.y;
    t.bounds = r;
    t.series = Some(c.series);
    Some(t)
}

/// The side of the box `r` that a value's text keeps beside its point at `x`: its middle when
/// the box is centred on the point, else the edge nearer the point.
fn side(r: Rect, x: f64) -> Anchor {
    let centre = r.center()[0];
    if (centre - x).abs() < 0.5 {
        Anchor::Middle
    } else if centre < x {
        Anchor::End
    } else {
        Anchor::Start
    }
}

/// Exact observation identity, independent of the candidate's displaced text box (TYP-36).
fn value_mark(panel: &Panel, index: usize, candidate: &ValueLabel) -> String {
    let o = &panel.observations[candidate.observation];
    if let Some(row) = o.row {
        format!("row:{row}")
    } else {
        let n = panel.observations[..candidate.observation]
            .iter()
            .filter(|q| q.series == o.series)
            .count();
        let series = o.series.map_or_else(|| "-".into(), |s| s.to_string());
        format!("{index}/{series}/{n}")
    }
}

fn value_positions(
    ctx: &Ctx,
    panel: &Panel,
    field: &ValueField,
    c: &ValueLabel,
    policy: ValueLabels,
    far: bool,
    blocked: &dyn Fn(Rect) -> bool,
) -> Result<Vec<(Text, Option<Rect>)>, DropReason> {
    let probe = ctx.text(
        c.text,
        0.0,
        0.0,
        Anchor::Start,
        Face::Figures,
        Role::Label,
        ctx.style.ink,
    );
    let (w, h) = (probe.bounds.w, probe.bounds.h);
    let mut at = c.at;
    // A bar's value end: a horizontal bar's right edge, or its left edge when it runs below
    // zero; a vertical bar's top, or its bottom below zero. The observation anchors at that end;
    // a zero has no mark and anchors past the line.
    let mut negative = false;
    let mut bar = None;
    if let ValueLabels::Bars { horizontal, stacked } = policy {
        if let Some((rect, fill)) = panel.items.iter().find_map(|i| match i {
            Item::Rect {
                rect,
                fill,
                role: Role::Mark,
                obs: Some(o),
                ..
            } if *o == c.observation => Some((*rect, fill)),
            _ => None,
        }) {
            negative = if horizontal {
                c.at[0] < rect.center()[0]
            } else {
                c.at[1] > rect.center()[1]
            };
            // Authored bounds can clip away the baseline and most of the original rectangle.
            // Only its visible part offers an interior position; the mark itself stays exact.
            let (left, top) = (rect.x.max(panel.plot.x), rect.y.max(panel.plot.y));
            let (right, bottom) = (
                rect.right().min(panel.plot.right()),
                rect.bottom().min(panel.plot.bottom()),
            );
            if right <= left || bottom <= top {
                return Err(DropReason::OutOfBounds);
            }
            let rect = Rect::new(left, top, right - left, bottom - top);
            let r = Rect::new(rect.x + (rect.w - w) / 2.0, rect.y + (rect.h - h) / 2.0, w, h);
            if r.inflate(4.0).inside(&rect, 0.0) && field.clear(r, Some(c.observation), true) {
                let mut background = composite(fill.color, Style::rgba(ctx.style.surface));
                let halo = fill.hatch.map(|_| r.inflate(1.5));
                if halo.is_some() {
                    background = composite(Style::rgba(ctx.style.surface).with_alpha(0.88), background);
                }
                if let Some(t) = value_text(ctx, c, r, Anchor::Middle, background) {
                    return Ok(vec![(t, halo)]);
                }
            }
            if stacked {
                return Err(DropReason::Density);
            }
            bar = Some((rect, fill));
            at = if horizontal {
                [if negative { rect.x } else { rect.right() }, rect.center()[1]]
            } else {
                [rect.center()[0], if negative { rect.bottom() } else { rect.y }]
            };
        } else if stacked {
            return Err(DropReason::Density);
        }
    }
    let bounds = Rect::new(
        panel.plot.x,
        panel.frame.y,
        panel.plot.w,
        panel.plot.bottom() - panel.frame.y,
    );
    if let ValueLabels::Bars { horizontal, .. } = policy {
        let end = BarEnd {
            at,
            horizontal,
            negative,
            size: [w, h],
            bounds,
        };
        return bar_end_value(ctx, panel, field, c, bar, &end);
    }
    let mut inside_any = false;
    let mut positions = candidates(at, 0.0, w, h);
    // At an endpoint, align the complete above/below box inside the plot.
    // Four pixels includes the curve's half-stroke and flattening clearance.
    for inset in [4.0, 8.0, 16.0] {
        if w + 2.0 * inset <= bounds.w {
            let x = (at[0] - w / 2.0).clamp(bounds.x + inset, bounds.right() - w - inset);
            for gap in [4.0, 8.0, 16.0] {
                positions.extend([Rect::new(x, at[1] - h - gap, w, h), Rect::new(x, at[1] + gap, w, h)]);
            }
        }
    }
    let mut available: Vec<(Text, Option<Rect>)> = Vec::new();
    let distance = |r: Rect| (at[0] - at[0].clamp(r.x, r.right())).hypot(at[1] - at[1].clamp(r.y, r.bottom()));
    for r in positions {
        if !r.inside(&bounds, 0.0) {
            continue;
        }
        inside_any = true;
        if field.clear(r, None, false)
            && !blocked(r)
            && let Some(t) = value_text(ctx, c, r, side(r, at[0]), Style::rgba(ctx.style.surface))
        {
            if !available.iter().any(|(other, _)| other.bounds == r) {
                // Insert after equal distances to preserve the original stable candidate order.
                let d = distance(r);
                let index = available.partition_point(|(other, _)| distance(other.bounds).total_cmp(&d).is_le());
                available.insert(index, (t, None));
            }
            if !matches!(policy, ValueLabels::Series) {
                return Ok(available);
            }
        }
    }
    // A single line's lowest or highest value keeps its label at every zoom: with no near place
    // clear, look further beside it, to its left and above it, a step at a time and clear of the
    // plot's edges, before dropping it; the nearest clear place wins (FEA-CH-28; Chart design,
    // 15 Nov).
    if available.is_empty() && far {
        let (top, bottom) = (bounds.y + 4.0, bounds.bottom() - h - 4.0);
        let (left, right) = (bounds.x + 4.0, bounds.right() - w - 4.0);
        let mut far: Vec<Rect> = Vec::new();
        for step in 1..=6 {
            let dx = 16.0 * step as f64;
            for dy in [0.0, 1.0, 2.0, 3.0, 4.0, 5.0] {
                let y = (at[1] - h / 2.0 - dy * h).clamp(top, bottom.max(top));
                far.push(Rect::new(at[0] + dx, y, w, h));
                far.push(Rect::new(at[0] - dx - w, y, w, h));
            }
            far.push(Rect::new(
                at[0] - w / 2.0,
                (at[1] - dx - h).clamp(top, bottom.max(top)),
                w,
                h,
            ));
        }
        for r in far {
            if r.x < left || r.x > right.max(left) {
                continue;
            }
            inside_any = true;
            if field.clear(r, None, false)
                && !blocked(r)
                && !available.iter().any(|(other, _)| other.bounds == r)
                && let Some(t) = value_text(ctx, c, r, side(r, at[0]), Style::rgba(ctx.style.surface))
            {
                let d = distance(r);
                let index = available.partition_point(|(other, _)| distance(other.bounds).total_cmp(&d).is_le());
                available.insert(index, (t, None));
            }
        }
        available.truncate(4);
    }
    if !available.is_empty() {
        return Ok(available);
    }
    Err(if inside_any {
        DropReason::Collision
    } else {
        DropReason::OutOfBounds
    })
}

/// Height of a value's digits, overshoot included, as a share of the text size: the tallest
/// digit of the faces charts set figures in (Geist 0.742 em, Space Mono 0.728, Patrick Hand
/// 0.667, RichView Code 0.662).
const DIGIT_HEIGHT: f64 = 0.75;

/// Where a bar's value would go: its value end, the bar's direction and which side of zero it
/// runs, the text's box size and the area values may use.
struct BarEnd {
    at: [f64; 2],
    horizontal: bool,
    negative: bool,
    size: [f64; 2],
    bounds: Rect,
}

/// A bar's value that does not fit inside the bar keeps to the bar's own row or column
/// (FEA-CH-28): never beside a vertical bar, above or below a horizontal one, or diagonal to
/// either, where the neighbouring bars and names sit. It takes the first clear place of:
/// - 3 px past its value end, level with the bar: right of a horizontal bar (left of a negative
///   one), above a vertical bar (below a negative one); it may cross soft gridlines, which then
///   break round it, and above or below an edge column it slides back into the plot while its
///   centre stays over the bar;
/// - for a vertical bar with one sibling beside it, that place slid away from the sibling until
///   it keeps the usual clearance, its centre still over the bar;
/// - for a horizontal bar in a group, that place slid away from its sibling until flush with
///   the bar's far edge, its centre still within the bar and its box at least 1 px clear of
///   the sibling; where the line box is taller than the bars, it slides on until the sibling's
///   own value has room inside the sibling, and takes the flush place when that one is taken;
/// - for a horizontal bar, inside it at its value end, 4 px in, when the bar holds the digits
///   with 2 px to spare; what lies round the bar, such as a sibling 1 px away, does not block
///   it.
fn bar_end_value(
    ctx: &Ctx,
    panel: &Panel,
    field: &ValueField,
    c: &ValueLabel,
    bar: Option<(Rect, &Fill)>,
    end: &BarEnd,
) -> Result<Vec<(Text, Option<Rect>)>, DropReason> {
    let ([w, h], [x, y]) = (end.size, end.at);
    // The place past the value end, and the side that holds the text there: the edge at a
    // horizontal bar's end, the middle over a column. A horizontal bar's box may slide to
    // another `top`.
    let past = |top: f64| match (end.horizontal, end.negative) {
        (true, false) => (Rect::new(x + 3.0, top, w, h), Anchor::Start),
        (true, true) => (Rect::new(x - 3.0 - w, top, w, h), Anchor::End),
        (false, negative) => {
            // A label wider than an edge column would leave the plot: it slides back in, as far
            // as its centre stays over its own bar.
            let mut left = x - w / 2.0;
            let mut anchor = Anchor::Middle;
            if let Some((rect, _)) = bar {
                let slid = left.clamp(end.bounds.x, (end.bounds.right() - w).max(end.bounds.x));
                if slid != left && rect.x <= slid + w / 2.0 && slid + w / 2.0 <= rect.right() {
                    // A slid box is held by the edge it has on the plot's edge.
                    anchor = if slid < left { Anchor::End } else { Anchor::Start };
                    left = slid;
                }
            }
            (
                Rect::new(left, if negative { y + 3.0 } else { y - 3.0 - h }, w, h),
                anchor,
            )
        }
    };
    let inside_any = std::cell::Cell::new(false);
    let place = |(r, anchor): (Rect, Anchor), except: Option<(usize, Rect)>| {
        if !r.inside(&end.bounds, 0.0) {
            return None;
        }
        inside_any.set(true);
        let clear = match except {
            // The sibling may come within 1 px; everything else keeps the usual clearance.
            Some((sibling, rect)) => field.clear(r, Some(sibling), true) && !r.inflate(0.99).overlaps(&rect, 0.0),
            None => field.clear(r, None, true),
        };
        clear
            .then(|| value_text(ctx, c, r, anchor, Style::rgba(ctx.style.surface)))
            .flatten()
    };
    if let Some(t) = place(past(y - h / 2.0), None) {
        return Ok(vec![(t, None)]);
    }
    let dropped = || {
        if inside_any.get() {
            DropReason::Collision
        } else {
            DropReason::OutOfBounds
        }
    };
    // A column in a group with one sibling beside it: its value slides away from that sibling
    // until it keeps the usual clearance from it, its centre still over its own column. A face
    // with wide digits otherwise loses every value over the shorter column of a pair.
    if let Some((rect, _)) = bar.filter(|_| !end.horizontal) {
        let beside = |left: bool| {
            panel.items.iter().find_map(|i| match i {
                Item::Rect {
                    rect: other,
                    role: Role::Mark,
                    obs: Some(o),
                    ..
                } if *o != c.observation && other.y < rect.bottom() && rect.y < other.bottom() => {
                    let gap = if left {
                        rect.x - other.right()
                    } else {
                        other.x - rect.right()
                    };
                    (-0.01..=1.5).contains(&gap).then_some(*other)
                }
                _ => None,
            })
        };
        let away = CLEAR + 0.75;
        let left = match (beside(true), beside(false)) {
            (Some(sibling), None) => Some(sibling.right() + away),
            (None, Some(sibling)) => Some(sibling.x - away - w),
            _ => None,
        };
        if let Some(left) = left
            && (rect.x..=rect.right()).contains(&(left + w / 2.0))
        {
            let top = if end.negative { y + 3.0 } else { y - 3.0 - h };
            if let Some(t) = place((Rect::new(left, top, w, h), Anchor::Middle), None) {
                return Ok(vec![(t, None)]);
            }
        }
    }
    let Some((rect, fill)) = bar.filter(|_| end.horizontal) else {
        return Err(dropped());
    };
    // A bar's siblings in its group lie a pixel above or below it.
    let sibling = |above: bool| {
        panel.items.iter().find_map(|i| match i {
            Item::Rect {
                rect: other,
                role: Role::Mark,
                obs: Some(o),
                ..
            } if *o != c.observation && other.x < rect.right() && rect.x < other.right() => {
                let gap = if above {
                    rect.y - other.bottom()
                } else {
                    other.y - rect.bottom()
                };
                (-0.01..=1.5).contains(&gap).then_some((*o, *other))
            }
            _ => None,
        })
    };
    // The first bar of a group slides up, its box's bottom on the bar's bottom; the last slides
    // down, its top on the bar's top. A middle bar keeps the centre line.
    let slide = match (sibling(true), sibling(false)) {
        (None, Some(below)) => Some((rect.bottom() - h, below, true)),
        (Some(above), None) => Some((rect.y, above, false)),
        _ => None,
    };
    if let Some((flush, sibling, up)) = slide
        && h <= 2.0 * rect.h
    {
        // The sibling's value, set inside the sibling, has a line box centred on it that
        // reaches past a thin bar: this box keeps within half the allowed overlap of it.
        let within = sibling.1.center()[1] - h / 2.0;
        let roomy = if up {
            flush.min(within + TEXT_OVERLAP / 2.0 - h)
        } else {
            flush.max(within + h - TEXT_OVERLAP / 2.0)
        };
        let tops = [roomy, flush];
        let tried = if roomy == flush || !(rect.y..=rect.bottom()).contains(&(roomy + h / 2.0)) {
            &tops[1..]
        } else {
            &tops[..]
        };
        for top in tried {
            if let Some(t) = place(past(*top), Some(sibling)) {
                return Ok(vec![(t, None)]);
            }
        }
    }
    if rect.h >= DIGIT_HEIGHT * ctx.size() + 2.0 && w + 8.0 <= rect.w {
        let r = Rect::new(
            if end.negative {
                rect.x + 4.0
            } else {
                rect.right() - 4.0 - w
            },
            rect.center()[1] - h / 2.0,
            w,
            h,
        );
        if field.clear_within(r, rect, c.observation) {
            let mut background = composite(fill.color, Style::rgba(ctx.style.surface));
            // A hatched bar's backing keeps to the bar, which holds the digits.
            let halo = fill.hatch.map(|_| {
                let (top, bottom) = ((r.y - 1.5).max(rect.y), (r.bottom() + 1.5).min(rect.bottom()));
                Rect::new(r.x - 1.5, top, w + 3.0, bottom - top)
            });
            if halo.is_some() {
                background = composite(Style::rgba(ctx.style.surface).with_alpha(0.88), background);
            }
            // Inside, the text is held by the edge at the bar's value end.
            let anchor = if end.negative { Anchor::Start } else { Anchor::End };
            if let Some(t) = value_text(ctx, c, r, anchor, background) {
                return Ok(vec![(t, halo)]);
            }
        }
    }
    Err(dropped())
}

/// Choose the constrained series first, with a bounded joint search of at most five texts.
fn choose_values(options: &[Vec<(Text, Option<Rect>)>]) -> Option<Vec<usize>> {
    fn search(
        options: &[Vec<(Text, Option<Rect>)>],
        order: &[usize],
        depth: usize,
        chosen: &mut [Option<usize>],
        probes: &mut usize,
    ) -> bool {
        if depth == order.len() {
            return true;
        }
        let index = order[depth];
        for (slot, (text, halo)) in options[index].iter().enumerate() {
            if *probes >= 512 {
                return false;
            }
            *probes += 1;
            let rect = halo.unwrap_or(text.bounds).inflate(CLEAR + 0.25);
            if chosen.iter().enumerate().any(|(other, slot)| {
                slot.is_some_and(|slot| {
                    let (t, h) = &options[other][slot];
                    rect.overlaps(&h.unwrap_or(t.bounds), 0.0)
                })
            }) {
                continue;
            }
            chosen[index] = Some(slot);
            if search(options, order, depth + 1, chosen, probes) {
                return true;
            }
            chosen[index] = None;
        }
        false
    }
    let mut order: Vec<_> = (0..options.len()).collect();
    // At most five texts: stable insertion keeps the original series order for equal counts.
    for i in 1..order.len() {
        let index = order[i];
        let mut at = i;
        while at > 0 && options[order[at - 1]].len() > options[index].len() {
            order[at] = order[at - 1];
            at -= 1;
        }
        order[at] = index;
    }
    let mut chosen = vec![None; options.len()];
    let mut probes = 0;
    search(options, &order, 0, &mut chosen, &mut probes).then(|| chosen.into_iter().flatten().collect())
}

/// Record a value drop against its observation, so replay can restore its exact readout text.
fn drop_value(ctx: &mut Ctx, panel: usize, candidate: &ValueLabel<'_>, reason: DropReason) {
    ctx.drop_label(
        candidate.text,
        reason,
        "value",
        Some(Rect::new(candidate.at[0], candidate.at[1], 0.0, 0.0)),
    );
    if let Some(dropped) = ctx.dropped.last_mut() {
        dropped.observation = Some([panel, candidate.observation]);
        dropped.series = Some(candidate.series);
    }
}

/// Whether a line's value labels try every point kind, and its extremes look past their near
/// places: true for a single series, whose labels Daniel asked to stay put across zooms. Two or more series
/// are named at their line ends, beside which the last value is not repeated (FEA-CH-27).
fn every_kind_tried(ctx: &Ctx) -> bool {
    ctx.series.len() == 1
}

/// A value a chart with few rows wants printed for one observation: the exact readout text and
/// the places it may take, in order of preference. The layout offers only places its own rules
/// allow; [`sparse_values`] keeps them inside the plot and clear of every mark, its own
/// included, and of every text.
pub(super) struct SparseValue {
    pub text: String,
    pub series: usize,
    pub observation: usize,
    pub places: Vec<Text>,
}

/// Automatic values of charts with few rows, such as a dumbbell's two ends or a bullet's measure
/// (FEA-CH-28 as chart design ruled on 16 Nov 2026). `rows` rows share the budget of five, three
/// under 480 px of plot width. Every value is printed or none: one that has no clear place
/// inside the plot takes the others with it, and each is then recorded as dropped. A chart over
/// the budget records each of its values as dropped too, so the numbers a chart prints or
/// records are the same at every width. `keep` leaves out the items a value may stand on. A value may stand across a soft gridline, which then stops short of it. The author's
/// `auto_annotations: false` prints nothing and records nothing.
pub(super) fn sparse_values(
    ctx: &mut Ctx,
    index: usize,
    panel: &mut Panel,
    rows: usize,
    wanted: Vec<SparseValue>,
    keep: &dyn Fn(&Item) -> bool,
) {
    let plot = panel.plot;
    sparse_values_in(ctx, index, panel, rows, wanted, keep, plot);
}

/// [`sparse_values`] for a layout that keeps room for its values beside the plot: a value
/// stands inside `bounds`, the plot and that room, instead of the plot alone.
pub(super) fn sparse_values_in(
    ctx: &mut Ctx,
    index: usize,
    panel: &mut Panel,
    rows: usize,
    wanted: Vec<SparseValue>,
    keep: &dyn Fn(&Item) -> bool,
    bounds: Rect,
) {
    if !ctx.block.auto_annotations || wanted.is_empty() {
        return;
    }
    let limit = if panel.plot.w < 480.0 { 3 } else { 5 };
    let mut field = ValueField::from_items(ctx, panel, keep);
    let mut chosen: Vec<Text> = Vec::with_capacity(wanted.len());
    let mut failure = (rows > limit).then_some(DropReason::Density);
    for value in wanted.iter().filter(|_| rows <= limit) {
        let inside: Vec<&Text> = value.places.iter().filter(|t| t.bounds.inside(&bounds, 0.5)).collect();
        match inside.iter().find(|t| field.clear(t.bounds, None, true)) {
            Some(t) => {
                field.insert(ValueInk::Box(t.bounds), t.bounds, None, false);
                chosen.push((*t).clone());
            }
            None => {
                // No place inside the bounds is out of bounds; a place the layout ruled out, or
                // one that is taken, is a collision.
                failure = Some(if inside.is_empty() && !value.places.is_empty() {
                    DropReason::OutOfBounds
                } else {
                    DropReason::Collision
                });
                break;
            }
        }
    }
    if let Some(reason) = failure {
        for value in &wanted {
            let at = value.places.first().map(|t| t.bounds).or_else(|| {
                let [x, y] = panel.observations[value.observation].at;
                Some(Rect::new(x, y, 0.0, 0.0))
            });
            ctx.drop_label(&value.text, reason, "value", at);
            // A mark that prints two values of its series (a dumbbell's ends) says which readout
            // line each is, so the record gets that line's text back and not the first one's.
            let lines = &panel.observations[value.observation].readout.lines;
            let mut own = lines
                .iter()
                .enumerate()
                .filter(|(_, line)| line.series == Some(value.series));
            let line = match own.next() {
                Some((_, first)) if first.printed() != value.text => {
                    own.find(|(_, line)| line.printed() == value.text).map(|(n, _)| n)
                }
                _ => None,
            };
            if let Some(dropped) = ctx.dropped.last_mut() {
                dropped.observation = Some([index, value.observation]);
                dropped.series = Some(value.series);
                dropped.line = line;
            }
        }
        return;
    }
    let boxes: Vec<Rect> = chosen.iter().map(|t| t.bounds).collect();
    break_grid_at(panel, &boxes);
    for (value, mut t) in wanted.iter().zip(chosen) {
        let at = panel.observations[value.observation].at;
        t.series = Some(value.series);
        t.mark = Some(value_mark(
            panel,
            index,
            &ValueLabel {
                text: &value.text,
                series: value.series,
                observation: value.observation,
                at,
            },
        ));
        panel.items.push(Item::Text(t));
    }
}

/// A combo's bar values, all in one place (chart design, 16 Nov 2026): past the ends of their
/// bars when every one of those places is clear, of the line above all; otherwise inside the
/// foot of their bars, on one line. There a value that does not fit its bar, or that the line
/// crosses, is dropped and recorded; a value never moves onto the line. The author's
/// `auto_annotations: false` prints nothing and records nothing.
pub(super) fn combo_bar_values(ctx: &mut Ctx, index: usize, panel: &mut Panel, horizontal: bool) {
    /// The gap between a bar's end and a value past it.
    const GAP: f64 = 4.0;
    /// How far inside the foot of its bar a value starts.
    const FOOT: f64 = 6.0;
    if !ctx.block.auto_annotations {
        return;
    }
    struct Bar {
        text: String,
        series: usize,
        observation: usize,
        at: [f64; 2],
        rect: Rect,
        background: Rgba,
        negative: bool,
    }
    let surface = Style::rgba(ctx.style.surface);
    let mut bars: Vec<Bar> = Vec::new();
    for (observation, o) in panel.observations.iter().enumerate() {
        let Some(series) = o.series else { continue };
        let Some(line) = o.readout.lines.iter().find(|l| l.series == Some(series) && !l.derived) else {
            continue;
        };
        let Some((rect, fill)) = panel.items.iter().find_map(|i| match i {
            Item::Rect {
                rect,
                fill,
                role: Role::Mark,
                obs: Some(q),
                ..
            } if *q == observation => Some((*rect, fill)),
            _ => None,
        }) else {
            continue;
        };
        let mut background = composite(fill.color, surface);
        if fill.hatch.is_some() {
            background = composite(surface.with_alpha(0.88), background);
        }
        bars.push(Bar {
            text: line.printed().to_owned(),
            series,
            observation,
            at: o.at,
            rect,
            background,
            negative: if horizontal {
                o.at[0] < rect.center()[0]
            } else {
                o.at[1] > rect.center()[1]
            },
        });
    }
    if bars.is_empty() {
        return;
    }
    let field = ValueField::from_panel(ctx, panel);
    let lh = ctx.line_height();
    let ink = ctx.style.ink;
    let figure =
        |text: &str, x: f64, y: f64, anchor: Anchor| ctx.text_mid(text, x, y, anchor, Face::Figures, Role::Label, ink);
    // Past the bar's end: above an upright bar, right of a lying one, and the other way for a
    // bar below zero.
    let bounds = Rect::new(
        panel.plot.x,
        panel.frame.y,
        panel.plot.w,
        panel.plot.bottom() - panel.frame.y,
    );
    let beyond: Vec<Text> = bars
        .iter()
        .map(|b| {
            let [cx, cy] = b.rect.center();
            match (horizontal, b.negative) {
                (true, false) => figure(&b.text, b.rect.right() + GAP, cy, Anchor::Start),
                (true, true) => figure(&b.text, b.rect.x - GAP, cy, Anchor::End),
                (false, false) => figure(&b.text, cx, b.rect.y - GAP - lh / 2.0, Anchor::Middle),
                (false, true) => figure(&b.text, cx, b.rect.bottom() + GAP + lh / 2.0, Anchor::Middle),
            }
        })
        .collect();
    let all_beyond = beyond.iter().enumerate().all(|(i, t)| {
        t.bounds.inside(&bounds, 0.5)
            && field.clear(t.bounds, None, true)
            && beyond[..i]
                .iter()
                .all(|other| !t.bounds.inflate(CLEAR).overlaps(&other.bounds, 0.0))
    });
    let chosen: Vec<Result<Text, DropReason>> = if all_beyond {
        beyond.into_iter().map(Ok).collect()
    } else {
        bars.iter()
            .map(|b| {
                // Only the part of the bar inside the plot offers a place.
                let (left, top) = (b.rect.x.max(panel.plot.x), b.rect.y.max(panel.plot.y));
                let (right, bottom) = (
                    b.rect.right().min(panel.plot.right()),
                    b.rect.bottom().min(panel.plot.bottom()),
                );
                if right <= left || bottom <= top {
                    return Err(DropReason::OutOfBounds);
                }
                let rect = Rect::new(left, top, right - left, bottom - top);
                let [cx, cy] = rect.center();
                let probe = match (horizontal, b.negative) {
                    (true, false) => figure(&b.text, rect.x + FOOT, cy, Anchor::Start),
                    (true, true) => figure(&b.text, rect.right() - FOOT, cy, Anchor::End),
                    (false, false) => figure(&b.text, cx, rect.bottom() - FOOT - lh / 2.0, Anchor::Middle),
                    (false, true) => figure(&b.text, cx, rect.y + FOOT + lh / 2.0, Anchor::Middle),
                };
                if !probe.bounds.inflate(4.0).inside(&rect, 0.5) {
                    return Err(DropReason::OutOfBounds);
                }
                if !field.clear(probe.bounds, Some(b.observation), true) {
                    return Err(DropReason::Collision);
                }
                let label = ValueLabel {
                    text: &b.text,
                    series: b.series,
                    observation: b.observation,
                    at: b.at,
                };
                value_text(ctx, &label, probe.bounds, probe.anchor, b.background).ok_or(DropReason::Collision)
            })
            .collect()
    };
    let mut across: Vec<Rect> = Vec::new();
    for (b, place) in bars.iter().zip(chosen) {
        let label = ValueLabel {
            text: &b.text,
            series: b.series,
            observation: b.observation,
            at: b.at,
        };
        match place {
            Ok(mut t) => {
                if all_beyond {
                    across.push(t.bounds);
                }
                t.series = Some(b.series);
                t.mark = Some(value_mark(panel, index, &label));
                panel.items.push(Item::Text(t));
            }
            Err(reason) => drop_value(ctx, index, &label, reason),
        }
    }
    break_grid_at(panel, &across);
}

/// Gridlines stop short of the values set across them, 2 px past the text's box, and run on
/// everywhere else. No knockout box is painted, so a tinted plot stays true (FEA-CH-26).
fn break_grid_at(panel: &mut Panel, boxes: &[Rect]) {
    const GAP: f64 = 2.0;
    for item in &mut panel.items {
        let Item::Line {
            path,
            stroke,
            role: Role::Grid,
            ..
        } = item
        else {
            continue;
        };
        let [Cmd::M { p: a }, Cmd::L { p: b }] = path.cmds[..] else {
            continue;
        };
        // The axis the line runs along: 1 for a vertical line, 0 for a horizontal one.
        let along = if a[0] == b[0] {
            1
        } else if a[1] == b[1] {
            0
        } else {
            continue;
        };
        let (from, to) = (a[along].min(b[along]), a[along].max(b[along]));
        let fixed = a[1 - along];
        let reach = GAP + stroke.width / 2.0;
        let mut gaps: Vec<[f64; 2]> = boxes
            .iter()
            .filter_map(|r| {
                let (lo, hi, side_lo, side_hi) = if along == 1 {
                    (r.y, r.bottom(), r.x, r.right())
                } else {
                    (r.x, r.right(), r.y, r.bottom())
                };
                (fixed >= side_lo - reach && fixed <= side_hi + reach && hi + GAP > from && lo - GAP < to)
                    .then_some([lo - GAP, hi + GAP])
            })
            .collect();
        if gaps.is_empty() {
            continue;
        }
        gaps.sort_by(|x, y| x[0].total_cmp(&y[0]));
        let point = |v: f64| if along == 1 { [fixed, v] } else { [v, fixed] };
        let mut broken = Path::new();
        let mut at = from;
        for [start, end] in gaps.into_iter().chain([[to, to]]) {
            let stop = start.min(to);
            // A stub shorter than a pixel reads as a speck, not a line.
            if stop - at >= 1.0 {
                broken.move_to(point(at));
                broken.line_to(point(stop));
            }
            at = at.max(end);
        }
        *path = broken;
    }
}

/// Add exact primary readout text after names/annotations, without changing any observation.
pub(super) fn value_labels(ctx: &mut Ctx, index: usize, panel: &mut Panel, policy: ValueLabels) {
    // The author's `auto_annotations: false` is a choice, not a drop: nothing is recorded.
    if !ctx.block.auto_annotations {
        return;
    }
    let mut groups: Vec<Vec<ValueLabel<'_>>> = Vec::new();
    match policy {
        ValueLabels::Bars { .. } => {
            for (observation, o) in panel.observations.iter().enumerate() {
                if let Some(series) = o.series
                    && let Some(line) = o.readout.lines.iter().find(|l| l.series == Some(series) && !l.derived)
                    && line.value != "No observation"
                {
                    groups.push(vec![ValueLabel {
                        text: line.printed(),
                        series,
                        observation,
                        at: o.at,
                    }]);
                }
            }
        }
        ValueLabels::Series if !ctx.value_kinds.is_empty() => {
            // Each series' own last, first, lowest and highest value over all its data, wherever
            // the window shows them, so zooming moves a label with its observation and never
            // names a window's own extremes (FEA-CH-28, FEA-CH-49).
            let mut series: Vec<[Option<ValueLabel<'_>>; 4]> = vec![[None, None, None, None]; ctx.series.len()];
            let mut present = vec![false; ctx.series.len()];
            for o in &panel.observations {
                for dot in &o.dots {
                    if let Some(p) = present.get_mut(dot.series) {
                        *p = true;
                    }
                }
            }
            for (s, kinds) in ctx.value_kinds.iter().enumerate().take(series.len()) {
                for (k, observation) in kinds.iter().enumerate() {
                    let Some(observation) = *observation else { continue };
                    let o = &panel.observations[observation];
                    if let Some(dot) = o.dots.iter().find(|d| d.series == s)
                        && let Some(line) = o.readout.lines.iter().find(|l| l.series == Some(s) && !l.derived)
                    {
                        series[s][k] = Some(ValueLabel {
                            text: line.printed(),
                            series: s,
                            observation,
                            at: dot.at,
                        });
                    }
                }
            }
            if panel.items.iter().any(|item| {
                item.role() == Role::Mark
                    && item.series().is_some_and(|s| !present.get(s).copied().unwrap_or(true))
                    && item.bounds().is_some_and(|b| b.overlaps(&panel.plot, 0.0))
            }) {
                // A bridged curve may cross the window without an exact source value in it.
                // No other series receives a partial kind and no interpolated value is invented.
                for c in series.iter().flatten().flatten() {
                    drop_value(ctx, index, c, DropReason::Density);
                }
                return;
            }
            // A point kind is atomic across the shown series: last, first, low, high.
            for kind in 0..4 {
                groups.push(series.iter().filter_map(|s| s[kind].as_ref()).cloned().collect());
            }
        }
        ValueLabels::Series | ValueLabels::Density => {
            // Keep four candidates per series, not another copy of every dense observation.
            let mut series: Vec<[Option<ValueLabel<'_>>; 4]> = vec![[None, None, None, None]; ctx.series.len()];
            for (observation, o) in panel.observations.iter().enumerate() {
                for dot in &o.dots {
                    if let Some(line) =
                        o.readout.lines.iter().find(|l| {
                            l.series == Some(dot.series) && l.derived == matches!(policy, ValueLabels::Density)
                        })
                    {
                        let c = ValueLabel {
                            text: line.printed(),
                            series: dot.series,
                            observation,
                            at: dot.at,
                        };
                        let s = &mut series[dot.series];
                        s[1].get_or_insert_with(|| c.clone());
                        if s[2].as_ref().is_none_or(|v| c.at[1] >= v.at[1]) {
                            s[2] = Some(c.clone());
                        }
                        if s[3].as_ref().is_none_or(|v| c.at[1] < v.at[1]) {
                            s[3] = Some(c.clone());
                        }
                        s[0] = Some(c);
                    }
                }
            }
            if matches!(policy, ValueLabels::Series)
                && panel.items.iter().any(|item| {
                    item.role() == Role::Mark
                        && item.series().is_some_and(|s| series[s][0].is_none())
                        && item.bounds().is_some_and(|b| b.overlaps(&panel.plot, 0.0))
                })
            {
                // A bridged curve may cross the window without an exact source value in it.
                // No other series receives a partial kind and no interpolated value is invented.
                for c in series.iter().filter_map(|s| s[0].as_ref()) {
                    drop_value(ctx, index, c, DropReason::Density);
                }
                return;
            }
            let series: Vec<_> = series.iter().filter(|s| s[0].is_some()).collect();
            if matches!(policy, ValueLabels::Density) {
                for s in series {
                    if let Some(c) = &s[3] {
                        groups.push(vec![c.clone()]);
                    }
                }
            } else {
                // A point kind is atomic across the shown series: last, first, low, high.
                for kind in 0..4 {
                    groups.push(series.iter().filter_map(|s| s[kind].as_ref()).cloned().collect());
                }
            }
        }
    }
    let mut field = ValueField::from_panel(ctx, panel);
    let limit = if matches!(policy, ValueLabels::Bars { .. }) {
        usize::MAX
    } else if panel.plot.w < 480.0 {
        3
    } else {
        5
    };
    let mut planned: Vec<(ValueLabel<'_>, Text, Option<Rect>)> = Vec::new();
    // The leaders of the values placed so far on a single line: a later value keeps off them,
    // and its own leader keeps off the values already placed.
    let lead = matches!(policy, ValueLabels::Series) && every_kind_tried(ctx);
    let mut leaders: Vec<[[f64; 2]; 2]> = Vec::new();
    // Where a single line's lowest or highest point lies so close to its last point that the two
    // values, each set over its own point, would run into each other, they want the same room at
    // the line's end. The last value is the one a reader looks for there, so it may then stand
    // off on a leader as an extreme may, and it is placed first: when both cannot stand, the last
    // value stays (chart design, 16 Nov 2026). An extreme further off leaves the rule as it was.
    let contested = lead
        && groups.first().and_then(|g| g.first()).is_some_and(|last| {
            let reach = 2.0 * ctx.line_height();
            groups.iter().skip(2).flatten().any(|c| {
                c.observation != last.observation
                    && (c.at[0] - last.at[0]).abs()
                        <= (ctx.text_width(c.text) + ctx.text_width(last.text)) / 2.0 + VALUE_GAP
                    && (c.at[1] - last.at[1]).abs() <= reach
            })
        });
    let mut failed = false;
    let mut seen = if matches!(policy, ValueLabels::Series) {
        vec![[None; 4]; ctx.series.len()]
    } else {
        Vec::new()
    };
    for (kind, group) in groups.into_iter().enumerate() {
        // Only the four series point kinds can repeat an observation.
        if matches!(policy, ValueLabels::Series) {
            if group.iter().any(|c| seen[c.series].contains(&Some(c.observation))) {
                continue;
            }
            for c in &group {
                seen[c.series][kind] = Some(c.observation);
            }
        }
        if group.is_empty() {
            continue;
        }
        if failed || planned.len() + group.len() > limit {
            for c in group {
                drop_value(ctx, index, &c, DropReason::Density);
            }
            continue;
        }
        // Groups come in point-kind order: last, first, lowest, highest.
        let far = lead && (kind >= 2 || (kind == 0 && contested));
        let mut options = Vec::with_capacity(group.len());
        let mut failure = None;
        for c in &group {
            // A leader struck through another value reads as that value crossed out, and two
            // values side by side with less than a character between them read as one number.
            let beside = |r: Rect| Rect::new(r.x - VALUE_GAP, r.y, r.w + 2.0 * VALUE_GAP, r.h);
            let blocked = |r: Rect| {
                lead && (planned.iter().any(|(_, t, _)| beside(r).overlaps(&t.bounds, 0.0))
                    || leaders
                        .iter()
                        .any(|[a, b]| segment_touches_rect(*a, *b, &r.inflate(1.0)))
                    || leader_end(c.at, r).is_some_and(|end| {
                        planned
                            .iter()
                            .any(|(_, t, _)| segment_touches_rect(c.at, end, &t.bounds.inflate(1.0)))
                    }))
            };
            match value_positions(ctx, panel, &field, c, policy, far, &blocked) {
                Ok(positions) => options.push(positions),
                Err(reason) => {
                    failure = Some(reason);
                    break;
                }
            }
        }
        let chosen = if let Some(reason) = failure {
            Err(reason)
        } else {
            choose_values(&options)
                .map(|slots| {
                    slots
                        .into_iter()
                        .enumerate()
                        .map(|(i, slot)| options[i][slot].clone())
                        .collect::<Vec<_>>()
                })
                .ok_or(DropReason::Collision)
        };
        // On a single line a kind that does not fit is left out and the next is tried, so a dense
        // line's lowest and highest values still show when its last value has no room. Named
        // series keep the rule that a kind with no room labels no later kind (FEA-CH-27, -28).
        if let Err(reason) = chosen {
            for c in group {
                drop_value(ctx, index, &c, reason);
            }
            failed = matches!(policy, ValueLabels::Series) && !every_kind_tried(ctx);
        } else {
            for (c, (t, halo)) in group.into_iter().zip(chosen.unwrap_or_default()) {
                field.insert_value(halo.unwrap_or(t.bounds), t.bounds);
                if lead && let Some(end) = leader_end(c.at, t.bounds) {
                    leaders.push([c.at, end]);
                }
                planned.push((c, t, halo));
            }
        }
    }
    // The common finish pass paints the single hatch backing. Only its footprint is reserved here.
    for (c, mut t, _) in planned {
        // A value not right beside its own point gets the leader a playback's kept name uses,
        // so it cannot read as a neighbouring point's value (Chart design, 15 Nov).
        if lead && let Some(end) = leader_end(c.at, t.bounds) {
            panel.items.push(Item::Line {
                path: Path::polyline(&[c.at, end]),
                stroke: Stroke::solid(Style::rgba(ctx.style.ink_faint), 1.0),
                role: Role::Leader,
                series: None,
                obs: None,
            });
        }
        t.mark = Some(value_mark(panel, index, &c));
        panel.items.push(Item::Text(t));
    }
}

/// Where a leader from `at` meets the label `r`: the middle of the side facing the point, 2 px
/// out, or `None` when the label is within 4 px of the point and needs no leader.
pub(super) fn leader_end(at: [f64; 2], r: Rect) -> Option<[f64; 2]> {
    let near = [at[0].clamp(r.x, r.right()), at[1].clamp(r.y, r.bottom())];
    if (at[0] - near[0]).hypot(at[1] - near[1]) <= 4.0 {
        return None;
    }
    Some(if at[0] > r.right() {
        [r.right() + 2.0, r.y + r.h / 2.0]
    } else if at[0] < r.x {
        [r.x - 2.0, r.y + r.h / 2.0]
    } else if at[1] < r.y {
        [at[0], r.y - 2.0]
    } else {
        [at[0], r.bottom() + 2.0]
    })
}

/// Least distance between the centres of two names at the line ends (WCAG 2.5.8 target size).
pub(super) const END_NAME_PITCH: f64 = 24.0;

/// Test closed segment/rectangle contact by clipping its parameter interval on both axes.
pub(super) fn segment_touches_rect(a: [f64; 2], b: [f64; 2], rect: &Rect) -> bool {
    let (mut start, mut end) = (0.0_f64, 1.0_f64);
    for (axis, (low, high)) in [(rect.x, rect.right()), (rect.y, rect.bottom())]
        .into_iter()
        .enumerate()
    {
        let delta = b[axis] - a[axis];
        if delta == 0.0 {
            if a[axis] < low || a[axis] > high {
                return false;
            }
        } else {
            let (u, v) = ((low - a[axis]) / delta, (high - a[axis]) / delta);
            start = start.max(u.min(v));
            end = end.min(u.max(v));
            if start > end {
                return false;
            }
        }
    }
    true
}

fn name_height(ctx: &Ctx) -> f64 {
    (ctx.line_height() + 4.0).max(END_NAME_PITCH)
}

/// Whether the measured line-name band holds every full button, including the gaps.
pub(super) fn line_names_fit(ctx: &Ctx, plot: Rect) -> bool {
    let count = ctx.series.len();
    // Line names can move freely within this band. Capacity therefore does not depend on the
    // anchors or their reading order; every series counts, including hidden ones.
    count as f64 * name_height(ctx) + count.saturating_sub(1) as f64 <= plot.h + ctx.line_height()
}

fn name_order_key(ctx: &Ctx, all_shown: bool) -> String {
    let key = format!("cartesian.end-order.{:?}", ctx.opts.zoom);
    if all_shown || ctx.opts.hidden.is_empty() {
        key
    } else {
        format!("{key}.hidden.{:?}", ctx.opts.hidden)
    }
}

/// Sort label priorities with an in-place heap and no scratch allocation.
/// Callers keep ties explicit or merge equal keys, so stable sorting is unnecessary.
fn sort_labels<T>(values: &mut [T], compare: impl Fn(&T, &T) -> std::cmp::Ordering) {
    fn sift<T>(values: &mut [T], mut root: usize, end: usize, compare: &impl Fn(&T, &T) -> std::cmp::Ordering) {
        while root < end / 2 {
            let mut child = root * 2 + 1;
            if child + 1 < end && compare(&values[child], &values[child + 1]).is_lt() {
                child += 1;
            }
            if !compare(&values[root], &values[child]).is_lt() {
                break;
            }
            values.swap(root, child);
            root = child;
        }
    }
    for root in (0..values.len() / 2).rev() {
        sift(values, root, values.len(), &compare);
    }
    for end in (1..values.len()).rev() {
        values.swap(0, end);
        sift(values, 0, end, &compare);
    }
}

fn sorted_name_order(ends: &[(usize, f64, f64)]) -> Vec<usize> {
    let mut ordered = ends.to_vec();
    // Equal keys emit the same series id, so their internal order has no effect.
    sort_labels(&mut ordered, |a, b| b.2.total_cmp(&a.2).then(a.0.cmp(&b.0)));
    ordered.into_iter().map(|(s, _, _)| s).collect()
}

/// Record the data order even when the style presents series in a legend (TYP-36).
pub(super) fn end_name_order(ctx: &Ctx, panel: usize, ends: &[(usize, f64, f64)]) -> Vec<usize> {
    ctx.derive_visible(panel, &name_order_key(ctx, true), || sorted_name_order(ends))
}

fn name_indices(ends: &[(usize, f64, f64)], series_order: &[usize]) -> Vec<usize> {
    let mut order = Vec::with_capacity(ends.len());
    for &s in series_order {
        if let Some(i) = ends.iter().position(|(series, _, _)| *series == s)
            && !order.contains(&i)
        {
            order.push(i);
        }
    }
    for i in (0..ends.len()).filter(|i| !order.contains(i)).collect::<Vec<_>>() {
        let at = order
            .iter()
            .position(|&j| ends[i].2 > ends[j].2 || (ends[i].2 == ends[j].2 && ends[i].0 < ends[j].0))
            .unwrap_or(order.len());
        order.insert(at, i);
    }
    order
}

/// Name lines in a column at their right ends, separating the buttons; a name moved more than
/// 4 px gets a thin leader (FEA-CH-27).
pub(super) fn end_names(
    ctx: &Ctx,
    plot: Rect,
    ends: &[(usize, f64, f64)],
    series_order: &[usize],
    items: &mut Vec<Item>,
    taken: &mut Vec<Rect>,
) {
    let lh = ctx.line_height();
    // Each name is a button: its target is 24 px high, so neighbours sit at least that far apart.
    // A grid step can exchange nearly equal anchors. Replay their shown-value order instead of
    // deciding a new order at a pixel-rounding or tolerance boundary (TYP-36).
    let height = name_height(ctx);
    let anchors: Vec<(f64, f64)> = ends.iter().map(|(_, y, _)| (*y, height)).collect();
    let order = name_indices(ends, series_order);
    let column_x = plot.right() + 10.0;
    let placed =
        crate::labels::place_column_ordered(&anchors, &order, plot.y - lh / 2.0, plot.bottom() + lh / 2.0, 1.0)
            .expect("line-name capacity was checked before painting");
    for ((s, y, _), cy) in ends.iter().zip(placed) {
        let hidden = ctx.is_hidden(*s);
        let mut color = ctx.readable(ctx.series_color(*s));
        // A hidden series' name stays as its way back, at reduced strength (FEA-CH-33).
        if hidden {
            color = crate::color::mix(color, ctx.style.surface, 0.55);
        }
        let mut text = ctx.text_mid(
            &ctx.series[*s].name,
            column_x,
            cy,
            Anchor::Start,
            Face::Data,
            Role::Label,
            color,
        );
        text.series = Some(*s);
        if (cy - y).abs() > 4.0 && !hidden {
            items.push(Item::Line {
                path: Path::polyline(&[[plot.right() + 2.0, *y], [column_x - 3.0, cy]]),
                stroke: Stroke::solid(Style::rgba(ctx.style.ink_faint), 1.0),
                role: Role::Leader,
                series: Some(*s),
                obs: None,
            });
        }
        taken.push(text.bounds);
        items.push(Item::Text(text));
    }
}

/// A series' line as the name search reads it: its points in pixels in x order, and whether the
/// series is hidden. A hidden line is not drawn; its dimmed name marks where it runs.
pub(super) struct NameLine {
    pub series: usize,
    pub points: Vec<[f64; 2]>,
    pub hidden: bool,
}

/// Room between a name inside the plot and its own line.
const NAME_GAP: f64 = 5.0;
/// Room every other line keeps from a name inside the plot.
const NAME_CLEAR: f64 = 4.0;
/// Most places tried along one line.
const NAME_TRIES: usize = 96;

/// The highest and lowest pixel a line reaches between two x positions, its ends interpolated;
/// `None` where the line does not span the whole stretch.
fn line_reach(points: &[[f64; 2]], from: f64, to: f64) -> Option<[f64; 2]> {
    let (first, last) = (points.first()?, points.last()?);
    if first[0] > from || last[0] < to {
        return None;
    }
    let at = |x: f64| {
        let after = points.partition_point(|p| p[0] < x).clamp(1, points.len() - 1);
        let (a, b) = (points[after - 1], points[after]);
        if b[0] > a[0] {
            a[1] + (b[1] - a[1]) * ((x - a[0]) / (b[0] - a[0])).clamp(0.0, 1.0)
        } else {
            a[1]
        }
    };
    let (mut top, mut bottom) = (at(from).min(at(to)), at(from).max(at(to)));
    let start = points.partition_point(|p| p[0] <= from);
    for p in points[start..].iter().take_while(|p| p[0] < to) {
        top = top.min(p[1]);
        bottom = bottom.max(p[1]);
    }
    Some([top, bottom])
}

/// The box a name's button covers: at least 24 px each way, centred on the name (WCAG 2.5.8).
fn name_target(text: Rect) -> Rect {
    let (w, h) = ((text.w + 4.0).max(END_NAME_PITCH), (text.h + 4.0).max(END_NAME_PITCH));
    Rect::new(text.x + text.w / 2.0 - w / 2.0, text.y + text.h / 2.0 - h / 2.0, w, h)
}

/// Where every series' name stands inside the plot, as the centre of its text: right beside its
/// own line, on a stretch where no other line comes near and the name is nearer to its own line
/// than to any other. The plot then needs no column of names beside it and takes that width
/// (Daniel, 16 Nov 2026; the streamgraph's names are the model). Chart design's ruling of 16 Nov:
/// - a line's name stands above its line, and below only where above has no place; an area's
///   name stands below its line, inside its own fill, and above only where that has no place
///   (`inside_fill`);
/// - 5 px off the line, and as far right as a free place exists, so the names read as a loose
///   column near the line ends;
/// - clear of every text already in the panel (`texts`), with 8 px beside it, so a name and a
///   value never read as one;
/// - all or nothing: `None` when any name has no such place, as where lines lie on top of each
///   other, and the caller names the series another way.
///
/// The lines with the fewest free places choose first. With a series hidden, a name may also
/// take a steep stretch. A hidden series' dimmed name stands where its data is: right of its
/// line's end when that is inside the plot, against the plot's edge on its data's side when the
/// refitted axis leaves it outside, and never on another series' line (chart design, 16 Nov).
pub(super) fn inside_name_places(
    ctx: &Ctx,
    plot: Rect,
    lines: &[NameLine],
    texts: &[Rect],
    inside_fill: bool,
) -> Option<Vec<(usize, [f64; 2])>> {
    let h = ctx.line_height();
    // With a series hidden the value axis follows the rest, and a line that was flat may be
    // steep: a name then takes a steep stretch rather than leave the plot under the reader's
    // finger.
    let steep_too = lines.iter().any(|l| l.hidden);
    let free = |text: &Rect| {
        let beside = Rect::new(text.x - VALUE_GAP, text.y - 2.0, text.w + 2.0 * VALUE_GAP, text.h + 4.0);
        text.y >= plot.y + 1.0
            && text.bottom() <= plot.bottom() - 1.0
            && !texts.iter().any(|t| t.overlaps(&beside, 0.0))
    };
    // The nearest shown line above or below a name other than its own, over the name's width.
    let nearest = |own: usize, text: &Rect| {
        let (left, right) = (text.x, text.right());
        let mut nearest = f64::INFINITY;
        for other in lines.iter().filter(|o| o.series != own && !o.hidden) {
            let reach = other.points.first().map_or(left, |p| p[0]).max(left - NAME_CLEAR);
            let end = other.points.last().map_or(right, |p| p[0]).min(right + NAME_CLEAR);
            if end < reach {
                continue;
            }
            let Some([o_top, o_bottom]) = line_reach(&other.points, reach, end) else {
                continue;
            };
            nearest = nearest.min(if o_bottom < text.y {
                text.y - o_bottom
            } else if o_top > text.bottom() {
                o_top - text.bottom()
            } else {
                -1.0
            });
        }
        nearest
    };
    let mut options: Vec<(usize, Vec<Rect>)> = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        let w = ctx.text_width(&ctx.series[line.series].name);
        let (from, to) = (plot.x + w / 2.0 + 6.0, plot.right() - w / 2.0 - 6.0);
        // Free places on the preferred side of the line, right to left, then on the other side;
        // after those, with a series hidden, the same on steep stretches.
        let mut found: [Vec<Rect>; 4] = Default::default();
        if line.hidden {
            let places = hidden_name_places(plot, line, w, h, from, to, &|text: &Rect| {
                free(text) && nearest(line.series, text) >= NAME_CLEAR
            });
            if places.is_empty() {
                return None;
            }
            options.push((i, places));
            continue;
        }
        if to >= from && line.points.len() >= 2 {
            let steps = (((to - from) / 6.0).floor() as usize).clamp(1, NAME_TRIES);
            for step in (0..=steps).rev() {
                let cx = from + (to - from) * step as f64 / steps as f64;
                let (left, right) = (cx - w / 2.0, cx + w / 2.0);
                let Some([top, bottom]) = line_reach(&line.points, left - 2.0, right + 2.0) else {
                    continue;
                };
                let spread = bottom - top;
                // On a steep stretch a name floats far from most of its line.
                let steep = spread > 1.5 * h;
                if steep && !steep_too {
                    continue;
                }
                for (above, y) in [(true, top - NAME_GAP - h), (false, bottom + NAME_GAP)] {
                    let text = Rect::new(left, y, w, h);
                    if !free(&text) {
                        continue;
                    }
                    // Clear of every other line, and nearer to its own line than to any other.
                    let nearest = nearest(line.series, &text);
                    if nearest >= NAME_CLEAR && nearest >= NAME_GAP + spread / 2.0 + 3.0 {
                        found[2 * usize::from(steep) + usize::from(above == inside_fill)].push(text);
                    }
                }
            }
        }
        let places: Vec<Rect> = found.into_iter().flatten().collect();
        if places.is_empty() {
            return None;
        }
        options.push((i, places));
    }
    sort_labels(&mut options, |a, b| a.1.len().cmp(&b.1.len()).then(a.0.cmp(&b.0)));
    let mut chosen: Vec<(usize, Rect)> = Vec::with_capacity(lines.len());
    for (i, found) in &options {
        let text = found.iter().copied().find(|text| {
            chosen
                .iter()
                .all(|(_, c)| !name_target(*text).overlaps(&name_target(*c), 0.0))
        })?;
        chosen.push((*i, text));
    }
    sort_labels(&mut chosen, |a, b| a.0.cmp(&b.0));
    Some(
        chosen
            .into_iter()
            .map(|(i, text)| (lines[i].series, [text.x + text.w / 2.0, text.y + text.h / 2.0]))
            .collect(),
    )
}

/// The places a hidden series' name may take, best first: at the right, beside where its data
/// ends. The line's points are clamped to the plot, so a series the refitted axis leaves out
/// ends on the plot's edge on its data's side; its name then stands just inside that edge. From
/// there the name steps away from the edge, then leftwards, until `clear` holds.
fn hidden_name_places(
    plot: Rect,
    line: &NameLine,
    w: f64,
    h: f64,
    from: f64,
    to: f64,
    clear: &dyn Fn(&Rect) -> bool,
) -> Vec<Rect> {
    let Some(&[end_x, end_y]) = line.points.last() else {
        return Vec::new();
    };
    if to < from {
        return Vec::new();
    }
    let (top, bottom) = (plot.y + 2.0, plot.bottom() - 2.0 - h);
    // Rows to try, nearest the data first: beside the end, or along the edge it is clamped to.
    let rows: Vec<f64> = if end_y <= plot.y + 0.5 {
        (0..12).map(|k| top + k as f64 * 4.0).collect()
    } else if end_y >= plot.bottom() - 0.5 {
        (0..12).map(|k| bottom - k as f64 * 4.0).collect()
    } else {
        let (above, below) = (end_y - NAME_GAP - h, end_y + NAME_GAP);
        (0..12)
            .flat_map(|k| [above - k as f64 * 4.0, below + k as f64 * 4.0])
            .collect()
    };
    let start = end_x.min(to + w / 2.0) - w / 2.0;
    let steps = (((start - from) / 6.0).floor().max(0.0) as usize).min(NAME_TRIES);
    let mut out = Vec::new();
    for step in 0..=steps {
        let cx = start - step as f64 * 6.0;
        for &y in &rows {
            let text = Rect::new(cx - w / 2.0, y.clamp(top, bottom.max(top)), w, h);
            if clear(&text) {
                out.push(text);
            }
        }
        if out.len() >= 8 {
            break;
        }
    }
    out
}

/// Draw the names at the places [`inside_name_places`] found; a gridline stops short of a name,
/// as it does of a value. A hidden series keeps its name, at reduced strength, as its way back
/// (FEA-CH-33).
pub(super) fn inside_names(ctx: &Ctx, places: &[(usize, [f64; 2])], panel: &mut Panel) {
    let mut boxes = Vec::with_capacity(places.len());
    for &(s, [cx, cy]) in places {
        let mut color = ctx.readable(ctx.series_color(s));
        if ctx.is_hidden(s) {
            color = crate::color::mix(color, ctx.style.surface, 0.55);
        }
        let mut text = ctx.text_mid(
            &ctx.series[s].name,
            cx,
            cy,
            Anchor::Middle,
            Face::Data,
            Role::Label,
            color,
        );
        text.series = Some(s);
        boxes.push(text.bounds);
        panel.items.push(Item::Text(text));
    }
    break_grid_at(panel, &boxes);
}

/// Candidate boxes around an anchor, measured from the mark's edge: right, left, above, below,
/// then the diagonals, on growing rings.
fn candidates(anchor: [f64; 2], radius: f64, w: f64, h: f64) -> Vec<Rect> {
    let mut out = Vec::new();
    for ring in [3.0, 8.0, 16.0] {
        let d = radius + ring;
        let [x, y] = anchor;
        out.extend([
            Rect::new(x + d, y - h / 2.0, w, h),
            Rect::new(x - d - w, y - h / 2.0, w, h),
            Rect::new(x - w / 2.0, y - d - h, w, h),
            Rect::new(x - w / 2.0, y + d, w, h),
            Rect::new(x + d * 0.7, y - d * 0.7 - h, w, h),
            Rect::new(x - d * 0.7 - w, y - d * 0.7 - h, w, h),
            Rect::new(x + d * 0.7, y + d * 0.7, w, h),
            Rect::new(x - d * 0.7 - w, y + d * 0.7, w, h),
        ]);
    }
    out
}

/// The marks a name keeps clear of: each one's row, centre and radius.
type Marks<'a> = &'a [(usize, [f64; 2], f64)];

/// The first clear candidate inside `bounds`, or why there is none.
fn search(
    anchor: [f64; 2],
    radius: f64,
    size: [f64; 2],
    bounds: Rect,
    taken: &[Rect],
    (marks, own): (Marks, usize),
    blocked: &dyn Fn(Rect) -> bool,
) -> Result<Rect, DropReason> {
    let mut inside_any = false;
    for c in candidates(anchor, radius, size[0], size[1]) {
        if !c.inside(&bounds, 0.0) {
            continue;
        }
        inside_any = true;
        let hits_label = taken.iter().any(|t| c.inflate(CLEAR).overlaps(t, 0.0));
        let hits_mark = marks.iter().any(|(row, p, r)| {
            *row != own
                && c.inflate(CLEAR)
                    .overlaps(&Rect::new(p[0] - r, p[1] - r, 2.0 * r, 2.0 * r), 0.0)
        });
        if !hits_label && !hits_mark && !blocked(c) {
            return Ok(c);
        }
    }
    Err(if inside_any {
        DropReason::Collision
    } else {
        DropReason::OutOfBounds
    })
}

/// Whether another mark lies between `anchor` and the label `r`: on the straight way from the
/// point to the nearest part of its name. Such a name reads as the mark it stands against, not
/// as its own point's (Chart design, 16 Nov). A mark that covers the point itself lies in every
/// direction and does not count.
fn mark_between(anchor: [f64; 2], r: Rect, marks: &[(usize, [f64; 2], f64)], own: usize) -> bool {
    let to = [anchor[0].clamp(r.x, r.right()), anchor[1].clamp(r.y, r.bottom())];
    let (dx, dy) = (to[0] - anchor[0], to[1] - anchor[1]);
    let length = dx * dx + dy * dy;
    marks.iter().any(|(row, p, radius)| {
        if *row == own || (p[0] - anchor[0]).hypot(p[1] - anchor[1]) <= *radius {
            return false;
        }
        let along = if length > 0.0 {
            (((p[0] - anchor[0]) * dx + (p[1] - anchor[1]) * dy) / length).clamp(0.0, 1.0)
        } else {
            0.0
        };
        (p[0] - anchor[0] - along * dx).hypot(p[1] - anchor[1] - along * dy) < *radius
    })
}

/// Point names: author highlights first in priority order, then at most five points (three on
/// plots under 480 px), extremes first, then the most isolated (FEA-CH-28, TYP-78).
/// `auto_annotations: false` names the highlights only.
pub(super) fn point_labels(
    ctx: &mut Ctx,
    plot: Rect,
    anchors: &[(usize, [f64; 2], f64)],
    items: &mut Vec<Item>,
    taken: &mut Vec<Rect>,
) {
    let block = ctx.block;
    let label_field = block.encoding.label.clone();
    let text_of = |row: usize| -> Option<String> {
        match (&label_field, &block.highlight) {
            (Some(f), _) => rows::text(&ctx.rows[row], f),
            (None, Some(h)) => rows::text(&ctx.rows[row], &h.by),
            _ => None,
        }
    };
    let mut order: Vec<usize> = Vec::new();
    let mut highlighted: Vec<usize> = Vec::new();
    if let Some(h) = &block.highlight {
        let mut wanted: Vec<String> = h.priority.clone();
        for v in &h.include {
            if !wanted.iter().any(|w| w.eq_ignore_ascii_case(v)) {
                wanted.push(v.clone());
            }
        }
        for v in &wanted {
            let found: Vec<usize> = anchors
                .iter()
                .enumerate()
                .filter(|(_, a)| rows::text(&ctx.rows[a.0], &h.by).is_some_and(|t| t.eq_ignore_ascii_case(v)))
                .map(|(i, _)| i)
                .collect();
            if found.is_empty() && h.include.iter().any(|i| i.eq_ignore_ascii_case(v)) {
                ctx.check(
                    "unresolved-highlight",
                    Level::Warning,
                    format!("Highlight '{v}' matches no row"),
                    Some("Check the spelling against the data."),
                    Some("chart.highlight.include".into()),
                    None,
                    json!({"value": v}),
                );
            }
            for i in found {
                if !highlighted.contains(&i) {
                    highlighted.push(i);
                }
            }
        }
    }
    order.extend(&highlighted);
    let kept = ctx.opts.fit.as_ref().map_or(&[][..], |f| &f.names[..]);
    // A kept name shows in every frame, with a leader when it has no room beside its point.
    let mut leading = highlighted.clone();
    if block.auto_annotations && label_field.is_some() && !anchors.is_empty() && !kept.is_empty() {
        // A playback names the points its opening frame named, in every frame (FEA-BR-31).
        let limit = if plot.w < 480.0 { 3 } else { 5 };
        for name in kept {
            let found = (0..anchors.len()).find(|&i| text_of(anchors[i].0).as_deref() == Some(name.as_str()));
            if let Some(i) = found
                && !order.contains(&i)
                && order.len() - highlighted.len() < limit
            {
                order.push(i);
                leading.push(i);
            }
        }
    } else if block.auto_annotations && label_field.is_some() && !anchors.is_empty() {
        let limit = if plot.w < 480.0 { 3 } else { 5 };
        let mut default: Vec<usize> = Vec::new();
        let by = |f: &dyn Fn(&[f64; 2]) -> f64, max: bool| {
            let mut best = 0;
            let mut value = f(&anchors[0].1);
            for (i, anchor) in anchors.iter().enumerate().skip(1) {
                let next = f(&anchor.1);
                let order = next.total_cmp(&value);
                // The stable sorted order picked the first minimum and the last maximum.
                let better = if max { order.is_ge() } else { order.is_lt() };
                if better {
                    best = i;
                    value = next;
                }
            }
            best
        };
        // Smallest and largest x, and highest and lowest value (y grows downwards on screen).
        for i in [
            by(&|p| p[0], false),
            by(&|p| p[0], true),
            by(&|p| p[1], true),
            by(&|p| p[1], false),
        ] {
            if !default.contains(&i) {
                default.push(i);
            }
        }
        let mut isolation: Vec<(usize, f64)> = nearest_distances(anchors).into_iter().enumerate().collect();
        // Source indices retain the stable input order when isolation distances tie.
        sort_labels(&mut isolation, |a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        default.extend(
            isolation
                .iter()
                .map(|(i, _)| *i)
                .filter(|i| !default.contains(i))
                .collect::<Vec<_>>(),
        );
        let mut count = 0;
        for i in default {
            if count >= limit {
                if let Some(t) = text_of(anchors[i].0)
                    && !order.contains(&i)
                {
                    ctx.drop_label(&t, DropReason::Density, "point", None);
                }
                continue;
            }
            if !order.contains(&i) {
                order.push(i);
                count += 1;
            }
        }
    }
    let marks: Vec<(usize, [f64; 2], f64)> = anchors.to_vec();
    // The leaders drawn so far: a later name keeps off them.
    let mut leaders: Vec<[[f64; 2]; 2]> = Vec::new();
    for i in order {
        let (row, at, r) = anchors[i];
        let Some(label) = text_of(row) else { continue };
        let w = ctx.text_width(&label);
        let h = ctx.line_height();
        // A point's name never stands with another point between the two, nor on the leader of
        // a name placed before it.
        let on_leader = |c: Rect| {
            leaders
                .iter()
                .any(|[a, b]| segment_touches_rect(*a, *b, &c.inflate(CLEAR)))
        };
        let behind = |c: Rect| mark_between(at, c, &marks, row) || on_leader(c);
        match search(at, r, [w, h], plot, taken, (&marks, row), &behind) {
            Ok(rect) => {
                // The names a playback keeps in its other frames (FEA-BR-31).
                if label_field.is_some() && !highlighted.contains(&i) {
                    ctx.fit.names.push(label.clone());
                }
                let t = ctx.text_mid(
                    &label,
                    rect.x,
                    rect.y + h / 2.0,
                    Anchor::Start,
                    Face::Data,
                    Role::Label,
                    ctx.style.ink,
                );
                taken.push(t.bounds);
                items.push(Item::Text(t));
            }
            Err(reason) if leading.contains(&i) => {
                // Highlights and a playback's kept names may use a leader to the nearest free
                // spot.
                if let Some(rect) = leader_spot(at, r, [w, h], plot, taken, (&marks, row), &on_leader) {
                    let t = ctx.text_mid(
                        &label,
                        rect.x,
                        rect.y + h / 2.0,
                        Anchor::Start,
                        Face::Data,
                        Role::Label,
                        ctx.style.ink,
                    );
                    // To the side of the name that faces the point, never through the name.
                    let end = leader_end(at, t.bounds).unwrap_or([rect.x - 2.0, rect.y + h / 2.0]);
                    leaders.push([at, end]);
                    items.push(Item::Line {
                        path: Path::polyline(&[at, end]),
                        stroke: Stroke::solid(Style::rgba(ctx.style.ink_faint), 1.0),
                        role: Role::Leader,
                        series: None,
                        obs: None,
                    });
                    taken.push(t.bounds);
                    items.push(Item::Text(t));
                } else {
                    ctx.drop_label(&label, reason, "point", None);
                }
            }
            Err(reason) => ctx.drop_label(&label, reason, "point", None),
        }
    }
}

/// Each anchor's distance to its nearest other anchor, exactly as comparing every pair would
/// give, but scanning outward in x order only until the gap in x alone reaches the best distance
/// found, so thousands of points cost far fewer than their square of comparisons.
fn nearest_distances(anchors: &[(usize, [f64; 2], f64)]) -> Vec<f64> {
    let mut order: Vec<usize> = (0..anchors.len()).collect();
    order.sort_by(|&a, &b| anchors[a].1[0].total_cmp(&anchors[b].1[0]).then(a.cmp(&b)));
    let mut nearest = vec![f64::INFINITY; anchors.len()];
    for (k, &i) in order.iter().enumerate() {
        let p = anchors[i].1;
        let mut best = f64::INFINITY;
        for &j in &order[k + 1..] {
            let q = anchors[j].1;
            if q[0] - p[0] >= best {
                break;
            }
            best = best.min((q[0] - p[0]).hypot(q[1] - p[1]));
        }
        for &j in order[..k].iter().rev() {
            let q = anchors[j].1;
            if p[0] - q[0] >= best {
                break;
            }
            best = best.min((q[0] - p[0]).hypot(q[1] - p[1]));
        }
        nearest[i] = best;
    }
    nearest
}

/// A place for a name further from its point, on a leader: clear like any name's place, off the
/// places `blocked` rules out, and with a leader that crosses no name already placed. A place
/// with no other point between point and name comes first; in a crowd that leaves none, the
/// leader may pass a neighbour, so a playback's kept name still shows (FEA-BR-31).
pub(super) fn leader_spot(
    at: [f64; 2],
    r: f64,
    size: [f64; 2],
    bounds: Rect,
    taken: &[Rect],
    (marks, own): (Marks, usize),
    blocked: &dyn Fn(Rect) -> bool,
) -> Option<Rect> {
    for open in [true, false] {
        let unfit = |c: Rect| {
            blocked(c)
                || (open && mark_between(at, c, marks, own))
                || leader_end(at, c)
                    .is_some_and(|end| taken.iter().any(|t| segment_touches_rect(at, end, &t.inflate(CLEAR))))
        };
        for ring in [28.0, 44.0, 64.0, 90.0] {
            if let Ok(rect) = search(at, r + ring, size, bounds, taken, (marks, own), &unfit) {
                return Some(rect);
            }
        }
    }
    None
}

/// How far inside its band's edge a range or band label starts or ends.
const BAND_INSET: f64 = 4.0;
/// Least gap between a range or band label and its neighbours in the same row.
const BAND_GAP: f64 = 12.0;
/// Rows a range or band label may use, counted from the top of its band.
const BAND_ROWS: usize = 2;

/// The places a range or band's label may take, in order of preference, so that it reads as
/// that band's label (chart design, 15 Nov 2026). It lies over its band: with `ends` it ends
/// 4 px inside the band's right edge, otherwise it starts 4 px inside its left edge; a band
/// thinner than 8 px takes half its width instead. A chart's labels all hang on one side, so no
/// label reaches from its own band to the next label's (chart design, 16 Nov 2026). There are
/// two rows from the band's top and no third. A place is left out
/// when it leaves the plot, comes within 12 px of a label already placed in its row, or touches
/// a drawn bar, a data point or a drawn line (`strokes`: each line flattened, with half its
/// width). The flag says whether any place lay inside the plot.
#[allow(clippy::too_many_arguments)]
fn band_places(
    band: Rect,
    ends: bool,
    size: [f64; 2],
    plot: Rect,
    taken: &[Rect],
    bars: &[Rect],
    marks: &[(usize, [f64; 2], f64)],
    strokes: &[(Vec<[f64; 2]>, f64)],
) -> (Vec<Rect>, bool) {
    let [w, h] = size;
    let inset = BAND_INSET.min(band.w / 2.0);
    let top = band.y + (10.0 - h / 2.0).max(CLEAR);
    let mut places = Vec::new();
    let mut inside_any = false;
    for row in 0..BAND_ROWS {
        let y = top + row as f64 * (h + CLEAR);
        let x = if ends { band.right() - inset - w } else { band.x + inset };
        let c = Rect::new(x, y, w, h);
        if !c.inside(&plot, 0.0) {
            continue;
        }
        inside_any = true;
        let near = c.inflate(CLEAR);
        let hits_label = taken.iter().any(|t| band_reach(c).overlaps(t, 0.0));
        let hits_mark = bars.iter().any(|b| near.overlaps(b, 0.0))
            || marks
                .iter()
                .any(|(_, p, r)| near.overlaps(&Rect::new(p[0] - r, p[1] - r, 2.0 * r, 2.0 * r), 0.0))
            || strokes.iter().any(|(line, half)| {
                let reach = near.inflate(*half);
                line.windows(2).any(|s| segment_touches_rect(s[0], s[1], &reach))
            });
        if !hits_label && !hits_mark {
            places.push(c);
        }
    }
    (places, inside_any)
}

/// The box a band label keeps free: 12 px to each side in its row, and the usual clearance above
/// and below, which the next row just meets.
fn band_reach(c: Rect) -> Rect {
    Rect::new(c.x - BAND_GAP, c.y - CLEAR, c.w + 2.0 * BAND_GAP, c.h + 2.0 * CLEAR)
}

/// One place per band label, or none, chosen together: the choice that keeps the most labels,
/// and among equals the one that gives the earlier labels their earlier places. The search is
/// bounded; when the bound is reached the best choice found so far stands, and the first one
/// found gives each label in turn its first free place.
fn choose_band_places(options: &[&[Rect]]) -> Vec<Option<usize>> {
    fn search(
        options: &[&[Rect]],
        depth: usize,
        current: &mut Vec<Option<usize>>,
        placed: usize,
        best: &mut (usize, Vec<Option<usize>>),
        probes: &mut usize,
    ) {
        if depth == options.len() {
            if placed > best.0 {
                *best = (placed, current.clone());
            }
            return;
        }
        if placed + (options.len() - depth) <= best.0 {
            return;
        }
        for (slot, c) in options[depth].iter().enumerate() {
            if *probes == 0 {
                return;
            }
            *probes -= 1;
            let free = current[..depth]
                .iter()
                .enumerate()
                .all(|(other, s)| s.is_none_or(|s| !band_reach(*c).overlaps(&options[other][s], 0.0)));
            if free {
                current[depth] = Some(slot);
                search(options, depth + 1, current, placed + 1, best, probes);
                current[depth] = None;
            }
        }
        search(options, depth + 1, current, placed, best, probes);
    }
    let mut best = (0, vec![None; options.len()]);
    let mut probes = 4096;
    search(options, 0, &mut vec![None; options.len()], 0, &mut best, &mut probes);
    best.1
}

/// Annotations anchored to data coordinates (FEA-CH-32): rules, shaded ranges, marked points
/// and notes, with text through the shared search, clear of the data points, the drawn bars
/// and a point annotation's own ring. A range or band's label stays over its band
/// ([`band_places`]), and those labels are chosen together after the others.
pub(super) fn annotations(
    ctx: &mut Ctx,
    plot: Rect,
    xmap: &dyn Fn(&Value) -> Option<f64>,
    ymap: &dyn Fn(f64) -> f64,
    items: &mut Vec<Item>,
    taken: &mut Vec<Rect>,
    points: &[[f64; 2]],
) {
    let block = ctx.block;
    // Drawn bars are marks the text keeps clear of, like the data points.
    let bars: Vec<Rect> = items
        .iter()
        .filter_map(|i| match i {
            Item::Rect {
                rect, role: Role::Mark, ..
            } => Some(*rect),
            _ => None,
        })
        .collect();
    let x_field = block.encoding.x.clone().unwrap_or_default();
    let y_field = block.encoding.y.clone().unwrap_or_default();
    let ink = ctx.style.ink;
    let soft = Style::rgba(ctx.style.ink_soft);
    let marks: Vec<(usize, [f64; 2], f64)> = points.iter().map(|p| (usize::MAX - 1, *p, 2.0)).collect();
    let mut band_texts: Vec<(&str, Rect)> = Vec::new();
    for (index, a) in block.annotations.iter().enumerate() {
        let path = format!("chart.annotations[{index}]");
        let find = |anchor: &[(String, Value)], axis: char| -> Option<Value> {
            anchor
                .iter()
                .find(|(k, _)| {
                    if axis == 'x' {
                        k == "x" || *k == x_field
                    } else {
                        k == "y" || *k == y_field
                    }
                })
                .map(|(_, v)| v.clone())
        };
        let y_of = |v: &Value| -> Option<f64> {
            match v {
                Value::Number(n) => n.as_f64().map(ymap),
                Value::String(s) => rows::parse_decimal(s).and_then(Result::ok).map(ymap),
                _ => None,
            }
        };
        // A position outside the plot matches nothing on this chart.
        let inside_x = |x: f64| (plot.x - 0.5..=plot.right() + 0.5).contains(&x);
        let inside_y = |y: f64| (plot.y - 0.5..=plot.bottom() + 0.5).contains(&y);
        let unresolved = |ctx: &mut Ctx| {
            ctx.check(
                "unresolved-annotation",
                Level::Warning,
                format!("Annotation {} matches no position on this chart", index + 1),
                Some(
                    "Use a value inside the chart's data range; a range or band needs `from` and `to` at two different positions.",
                ),
                Some(path.clone()),
                None,
                Value::Null,
            );
        };
        let place_text = |ctx: &mut Ctx,
                          text: &str,
                          at: [f64; 2],
                          radius: f64,
                          rotate: f64,
                          items: &mut Vec<Item>,
                          taken: &mut Vec<Rect>| {
            let w = ctx.text_width(text);
            let h = ctx.line_height();
            if rotate != 0.0 {
                let mut t = ctx.text(text, at[0], at[1], Anchor::Start, Face::Data, Role::Annotation, ink);
                t.rotate = rotate;
                items.push(Item::Text(t));
                return;
            }
            let avoid: Vec<Rect> = taken.iter().chain(&bars).copied().collect();
            match search(at, radius, [w, h], plot, &avoid, (&marks, usize::MAX), &|_| false) {
                Ok(rect) => {
                    let t = ctx.text_mid(
                        text,
                        rect.x,
                        rect.y + h / 2.0,
                        Anchor::Start,
                        Face::Data,
                        Role::Annotation,
                        ink,
                    );
                    taken.push(t.bounds);
                    items.push(Item::Text(t));
                }
                Err(reason) => ctx.drop_label(
                    text,
                    reason,
                    "annotation",
                    Some(Rect::new(at[0], at[1] - h / 2.0, w, h)),
                ),
            }
        };
        match a.kind {
            AnnotationKind::Line => {
                if let Some(x) = find(&a.at, 'x').as_ref().and_then(xmap).filter(|x| inside_x(*x)) {
                    items.push(Item::Line {
                        path: Path::polyline(&[[x, plot.y], [x, plot.bottom()]]),
                        stroke: Stroke {
                            dash: vec![4.0, 3.0],
                            ..Stroke::solid(soft, 1.0)
                        },
                        role: Role::Annotation,
                        series: None,
                        obs: None,
                    });
                    if let Some(t) = &a.text {
                        place_text(ctx, t, [x, plot.y + 8.0], 2.0, 0.0, items, taken);
                    }
                } else if let Some(y) = find(&a.at, 'y').as_ref().and_then(y_of).filter(|y| inside_y(*y)) {
                    items.push(Item::Line {
                        path: Path::polyline(&[[plot.x, y], [plot.right(), y]]),
                        stroke: Stroke {
                            dash: vec![4.0, 3.0],
                            ..Stroke::solid(soft, 1.0)
                        },
                        role: Role::Annotation,
                        series: None,
                        obs: None,
                    });
                    if let Some(t) = &a.text {
                        place_text(ctx, t, [plot.right() - ctx.text_width(t), y], 2.0, 0.0, items, taken);
                    }
                } else {
                    unresolved(ctx);
                }
            }
            AnnotationKind::Range | AnnotationKind::Band => {
                let xs = (
                    find(&a.from, 'x').as_ref().and_then(xmap),
                    find(&a.to, 'x').as_ref().and_then(xmap),
                );
                let ys = (
                    find(&a.from, 'y').as_ref().and_then(y_of),
                    find(&a.to, 'y').as_ref().and_then(y_of),
                );
                let rect = match (xs, ys) {
                    ((Some(x0), Some(x1)), _) => {
                        let (a, b) = (x0.min(x1).max(plot.x), x0.max(x1).min(plot.right()));
                        (b > a).then(|| Rect::new(a, plot.y, b - a, plot.h))
                    }
                    (_, (Some(y0), Some(y1))) => {
                        let (a, b) = (y0.min(y1).max(plot.y), y0.max(y1).min(plot.bottom()));
                        (b > a).then(|| Rect::new(plot.x, a, plot.w, b - a))
                    }
                    _ => None,
                };
                match rect {
                    Some(rect) => {
                        let fill = Fill::solid(Style::rgba(ctx.style.ink_soft).with_alpha(0.08));
                        items.insert(
                            0,
                            Item::Rect {
                                rect,
                                fill,
                                stroke: None,
                                radius: [0.0; 4],
                                role: Role::Annotation,
                                series: None,
                                obs: None,
                            },
                        );
                        if let Some(t) = &a.text {
                            band_texts.push((t.as_str(), rect));
                        }
                    }
                    None => unresolved(ctx),
                }
            }
            AnnotationKind::Point | AnnotationKind::Text => {
                let x = find(&a.at, 'x').as_ref().and_then(xmap).filter(|x| inside_x(*x));
                let y = find(&a.at, 'y').as_ref().and_then(y_of).filter(|y| inside_y(*y));
                match (x, y) {
                    (Some(x), Some(y)) => {
                        // Text keeps clear of the ring's outer edge, not just its centre.
                        let mut radius = 2.0;
                        if a.kind == AnnotationKind::Point {
                            radius = RING + RING_STROKE / 2.0;
                            items.push(Item::Point {
                                center: [x, y],
                                radius: RING,
                                shape: Shape::Circle,
                                fill: None,
                                stroke: Some(Stroke::solid(Style::rgba(ink), RING_STROKE)),
                                role: Role::Annotation,
                                series: None,
                                obs: None,
                            });
                        }
                        if let Some(t) = &a.text {
                            place_text(ctx, t, [x, y], radius, a.rotate, items, taken);
                        }
                    }
                    _ => unresolved(ctx),
                }
            }
        }
    }
    // Range and band labels have fixed places, so they are chosen together, after the notes that
    // can move: the choice that keeps the most labels wins, and among equals the earlier places.
    // All of a chart's labels hang on one side of their bands: the side that prints more, and
    // on a tie the labels end at their bands, which keeps the latest event near the plot's
    // right edge.
    let h = ctx.line_height();
    let sized: Vec<(&str, Rect, f64)> = band_texts
        .iter()
        .map(|(text, band)| (*text, *band, ctx.text_width(text)))
        .collect();
    // A drawn line is a mark between its data points too, so these labels keep clear of its
    // stroke; the notes above move away from the points only.
    let strokes: Vec<(Vec<[f64; 2]>, f64)> = if sized.is_empty() {
        Vec::new()
    } else {
        items
            .iter()
            .filter_map(|i| match i {
                Item::Line {
                    path,
                    stroke,
                    role: Role::Mark,
                    ..
                } => Some((path, stroke.width / 2.0)),
                _ => None,
            })
            .flat_map(|(path, half)| path.flatten(0.5).into_iter().map(move |line| (line, half)))
            .collect()
    };
    let side = |ends: bool| {
        let options: Vec<(Vec<Rect>, bool)> = sized
            .iter()
            .map(|(_, band, w)| band_places(*band, ends, [*w, h], plot, taken, &bars, &marks, &strokes))
            .collect();
        let places: Vec<&[Rect]> = options.iter().map(|(places, _)| places.as_slice()).collect();
        let chosen = choose_band_places(&places);
        (options, chosen)
    };
    let (ending, starting) = (side(true), side(false));
    let printed = |chosen: &[Option<usize>]| chosen.iter().flatten().count();
    let (options, chosen) = if printed(&starting.1) > printed(&ending.1) {
        starting
    } else {
        ending
    };
    for (((text, band, w), (places, inside_any)), slot) in sized.iter().zip(&options).zip(chosen) {
        if let Some(r) = slot.map(|slot| places[slot]) {
            let t = ctx.text_mid(
                text,
                r.x,
                r.y + r.h / 2.0,
                Anchor::Start,
                Face::Data,
                Role::Annotation,
                ink,
            );
            taken.push(t.bounds);
            items.push(Item::Text(t));
        } else {
            let reason = if *inside_any {
                DropReason::Collision
            } else {
                DropReason::OutOfBounds
            };
            ctx.drop_label(
                text,
                reason,
                "annotation",
                Some(Rect::new(band.x + BAND_INSET, band.y, *w, h)),
            );
        }
    }
}

#[cfg(test)]
mod band_choice_tests {
    use super::{Rect, choose_band_places};

    /// The labels are chosen together: taking places one label at a time, the first label's
    /// first place leaves the second, which has only that row, none. Together both stay.
    #[test]
    fn band_labels_are_chosen_together_so_the_most_stay() {
        let first = [Rect::new(0.0, 0.0, 100.0, 16.0), Rect::new(0.0, 20.0, 100.0, 16.0)];
        let second = [Rect::new(60.0, 0.0, 100.0, 16.0)];
        assert_eq!(choose_band_places(&[&first, &second]), [Some(1), Some(0)]);
        // With room for both in their first places, each keeps its first.
        let apart = [Rect::new(120.0, 0.0, 100.0, 16.0)];
        assert_eq!(choose_band_places(&[&first, &apart]), [Some(0), Some(0)]);
    }
}

#[cfg(test)]
mod value_order_tests {
    use super::sort_labels;

    #[test]
    fn priority_order_matches_standard_sort_with_index_ties_and_float_total_order() {
        let keys = [f64::NAN, f64::NEG_INFINITY, -1.0, -0.0, 0.0, 1.0, f64::INFINITY];
        for len in [0, 1, 2, 3, 4, 5, 8, 15, 16, 17, 31, 32, 33, 64, 257, 1024] {
            for reversed in [false, true] {
                let mut values: Vec<_> = (0..len).map(|i| (i, keys[(i * 17 + i / 3) % keys.len()])).collect();
                if reversed {
                    values.reverse();
                }
                let compare = |a: &(usize, f64), b: &(usize, f64)| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0));
                let mut expected = values.clone();
                expected.sort_by(compare);
                sort_labels(&mut values, compare);
                assert_eq!(
                    values.iter().map(|(i, x)| (*i, x.to_bits())).collect::<Vec<_>>(),
                    expected.iter().map(|(i, x)| (*i, x.to_bits())).collect::<Vec<_>>(),
                    "{len}/{reversed}"
                );
            }
        }
    }
}

#[cfg(test)]
mod segment_tests {
    use super::segment_touches_rect;
    use crate::geometry::Rect;

    #[test]
    fn closed_contact_includes_crossings_and_tangencies_without_box_hull_false_positives() {
        let rect = Rect::new(4.0, 4.0, 2.0, 2.0);
        for (a, b, touches) in [
            ([0.0, 5.0], [10.0, 5.0], true),
            ([5.0, 0.0], [5.0, 10.0], true),
            ([0.0, 0.0], [10.0, 10.0], true),
            ([0.0, 7.0], [10.0, 7.0], false),
            ([7.0, 0.0], [7.0, 10.0], false),
            ([0.0, 0.0], [4.0, 4.0], true),
            ([4.0, 0.0], [4.0, 4.0], true),
            ([0.0, 8.0], [8.0, 0.0], true),
            ([0.0, 7.0], [7.0, 0.0], false),
            ([5.0, 5.0], [5.0, 5.0], true),
            ([2.0, 2.0], [2.0, 2.0], false),
        ] {
            assert_eq!(segment_touches_rect(a, b, &rect), touches, "{a:?} to {b:?}");
            assert_eq!(segment_touches_rect(b, a, &rect), touches, "reversed {b:?} to {a:?}");
        }
        assert!(
            segment_touches_rect([0.0, 5.0], [1_000_000.0, 5.0], &Rect::new(500_000.125, 4.0, 0.001, 2.0)),
            "even a very short contact on a long link is found without sampling"
        );
    }
}
