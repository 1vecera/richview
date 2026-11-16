//! One placement search for every optional label: end names, point names and annotation text
//! (FEA-CH-27 to FEA-CH-29, FEA-CH-32). Placement never moves an observation; a label without a
//! clear position is dropped and recorded with its reason.

use super::{Ctx, Style};
use crate::block::{AnnotationKind, ChartType};
use crate::diag::Level;
use crate::geometry::{Anchor, DropReason, Face, Fill, Item, Panel, Path, Rect, Rgba, Role, Shape, Stroke, Text};
use crate::rows;
use serde_json::{Value, json};

/// Least gap between a label and anything it must keep clear of.
const CLEAR: f64 = 2.0;
/// A point annotation's ring: radius and stroke width.
const RING: f64 = 5.0;
const RING_STROKE: f64 = 1.5;

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
        let r = r.inflate(CLEAR + 0.25);
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
        let mut field = Self {
            window: Some(panel.frame.inflate(32.0)),
            ..Self::default()
        };
        for item in &panel.items {
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

fn value_text(ctx: &Ctx, c: &ValueLabel, r: Rect, background: Rgba) -> Option<Text> {
    let background = background.oklch();
    let ink = [ctx.style.ink, ctx.style.surface]
        .into_iter()
        .max_by(|a, b| crate::color::contrast(*a, background).total_cmp(&crate::color::contrast(*b, background)))?;
    if crate::color::contrast(ink, background) < 4.5 {
        return None;
    }
    let mut t = ctx.text(c.text, 0.0, 0.0, Anchor::Start, Face::Figures, Role::Label, ink);
    t.x += r.x - t.bounds.x;
    t.y += r.y - t.bounds.y;
    t.bounds = r;
    t.series = Some(c.series);
    Some(t)
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
    // A horizontal bar's value end: its right edge, or its left edge when it runs below zero.
    // The observation anchors at that end; a zero has no mark and anchors right of the line.
    let mut negative = false;
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
            negative = horizontal && c.at[0] < rect.center()[0];
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
                if let Some(t) = value_text(ctx, c, r, background) {
                    return Ok(vec![(t, halo)]);
                }
            }
            if stacked {
                return Err(DropReason::Density);
            }
            at = if horizontal {
                [if negative { rect.x } else { rect.right() }, rect.center()[1]]
            } else {
                [rect.center()[0], rect.y]
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
    let mut inside_any = false;
    let horizontal_bar = matches!(policy, ValueLabels::Bars { horizontal: true, .. });
    let positions = if horizontal_bar {
        // A horizontal bar's value stays on its own row: past its value end on its centre line,
        // never above, below or diagonal to it, where the next rows' names and bars sit.
        [3.0, 8.0, 16.0]
            .map(|d| {
                let x = if negative { at[0] - d - w } else { at[0] + d };
                Rect::new(x, at[1] - h / 2.0, w, h)
            })
            .to_vec()
    } else {
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
        positions
    };
    let mut available: Vec<(Text, Option<Rect>)> = Vec::new();
    let distance = |r: Rect| (at[0] - at[0].clamp(r.x, r.right())).hypot(at[1] - at[1].clamp(r.y, r.bottom()));
    for r in positions {
        if !r.inside(&bounds, 0.0) {
            continue;
        }
        inside_any = true;
        if field.clear(r, None, horizontal_bar)
            && let Some(t) = value_text(ctx, c, r, Style::rgba(ctx.style.surface))
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
                && !available.iter().any(|(other, _)| other.bounds == r)
                && let Some(t) = value_text(ctx, c, r, Style::rgba(ctx.style.surface))
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
                        text: &line.value,
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
                            text: &line.value,
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
                            text: &line.value,
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
        let far = matches!(policy, ValueLabels::Series) && every_kind_tried(ctx) && kind >= 2;
        let mut options = Vec::with_capacity(group.len());
        let mut failure = None;
        for c in &group {
            match value_positions(ctx, panel, &field, c, policy, far) {
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
                let rect = halo.unwrap_or(t.bounds);
                field.insert(ValueInk::Box(rect), rect, None, false);
                planned.push((c, t, halo));
            }
        }
    }
    // The common finish pass paints the single hatch backing. Only its footprint is reserved here.
    let lead = matches!(policy, ValueLabels::Series) && every_kind_tried(ctx);
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
    // Unlike areas, line names can move freely within this band. Capacity therefore does not
    // depend on the anchors or their reading order; every series counts, including hidden ones.
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

/// Read the same data order for preflight without duplicating the outgoing derived record.
pub(super) fn preview_name_order(ctx: &Ctx, panel: usize, ends: &[(usize, f64, f64)], all_shown: bool) -> Vec<usize> {
    let key = name_order_key(ctx, all_shown);
    ctx.opts
        .derived
        .iter()
        .find(|d| d.panel == panel && d.key == key)
        .and_then(|d| serde_json::from_value(d.value.clone()).ok())
        .unwrap_or_else(|| sorted_name_order(ends))
}

/// Record the data order even when the style presents series in a legend (TYP-36).
pub(super) fn end_name_order(ctx: &Ctx, panel: usize, ends: &[(usize, f64, f64)]) -> Vec<usize> {
    ctx.derive_visible(panel, &name_order_key(ctx, true), || sorted_name_order(ends))
}

/// Left margin that contains complete area names and their full button targets.
pub(super) fn area_name_room(ctx: &Ctx) -> f64 {
    ctx.series
        .iter()
        .map(|s| {
            let width = ctx.text_width(&s.name);
            // The existing focus outline extends 2 px plus its 1 px offset past the target.
            13.0 + (width + (width + 4.0).max(END_NAME_PITCH)) / 2.0
        })
        .fold(0.0, f64::max)
}

/// Free vertical bands for the entire area-name column, including its invisible hit targets.
fn area_name_bands(ctx: &Ctx, plot: Rect, taken: &[Rect]) -> Vec<[f64; 2]> {
    let x = plot.x - 10.0;
    let (mut left, mut right) = (x, x);
    for s in &ctx.series {
        let width = ctx.text_width(&s.name);
        let target = (width + 4.0).max(END_NAME_PITCH);
        left = left.min(x - width / 2.0 - target / 2.0);
        right = right.max(x - width / 2.0 + target / 2.0);
    }
    let column = Rect::new(left, plot.y, right - left, plot.h);
    let mut blocked: Vec<_> = taken
        .iter()
        .filter_map(|r| {
            // Keep mathematical tangency clear after the text baseline's floating-point round trip.
            let r = r.inflate(CLEAR + plot.h.max(plot.w).max(1.0) * f64::EPSILON * 8.0);
            r.overlaps(&column, 0.0)
                .then_some([r.y.max(plot.y), r.bottom().min(plot.bottom())])
        })
        .collect();
    // Equal starts merge into the same blocked band, independent of their end order.
    sort_labels(&mut blocked, |a, b| a[0].total_cmp(&b[0]));
    let mut bands = Vec::new();
    let mut top = plot.y;
    for [a, b] in blocked {
        if a > top {
            bands.push([top, a]);
        }
        top = top.max(b);
    }
    if top < plot.bottom() {
        bands.push([top, plot.bottom()]);
    }
    bands
}

/// Project an ordered column into clear bands while retaining enough room for every successor.
fn column_in_bands(
    anchors: &[(f64, f64)],
    order: &[usize],
    plot: Rect,
    bands: &[[f64; 2]],
    max_move: f64,
) -> Option<Vec<f64>> {
    let ideal = crate::labels::place_column_ordered(anchors, order, plot.y, plot.bottom(), 1.0)?;
    let mut latest = vec![0.0; anchors.len()];
    let mut next = plot.bottom() + 1.0;
    for &i in order.iter().rev() {
        let half = anchors[i].1 / 2.0;
        let limit = (next - half - 1.0).min(anchors[i].0 + max_move);
        let at = bands.iter().rev().find_map(|[a, b]| {
            let y = (b - half).min(limit);
            (y >= (a + half).max(anchors[i].0 - max_move)).then_some(y)
        })?;
        latest[i] = at;
        next = at - half;
    }
    let mut placed = vec![0.0; anchors.len()];
    let mut bottom = plot.y - 1.0;
    for &i in order {
        let half = anchors[i].1 / 2.0;
        let low = (bottom + half + 1.0).max(anchors[i].0 - max_move);
        let at = bands
            .iter()
            .filter_map(|[a, b]| {
                let lo = (a + half).max(low);
                let hi = (b - half).min(latest[i]);
                (lo <= hi).then(|| ideal[i].clamp(lo, hi))
            })
            .min_by(|a, b| {
                (a - ideal[i])
                    .abs()
                    .total_cmp(&(b - ideal[i]).abs())
                    .then(a.total_cmp(b))
            })?;
        placed[i] = at;
        bottom = at + half;
    }
    Some(placed)
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

/// Whether every area name fits without drifting beyond half a value-tick spacing.
pub(super) fn area_names_fit(
    ctx: &Ctx,
    plot: Rect,
    taken: &[Rect],
    ends: &[(usize, f64, f64)],
    series_order: &[usize],
    max_move: f64,
) -> bool {
    if ends.len() != ctx.series.len() {
        return false;
    }
    let height = name_height(ctx);
    let anchors: Vec<_> = ends.iter().map(|(_, y, _)| (*y, height)).collect();
    column_in_bands(
        &anchors,
        &name_indices(ends, series_order),
        plot,
        &area_name_bands(ctx, plot, taken),
        max_move,
    )
    .is_some()
}

/// Name lines on the right and areas on the left, separating buttons and keeping area names
/// clear of axis ink; a name moved more than 4 px gets a thin leader (FEA-CH-27).
pub(super) fn end_names(
    ctx: &Ctx,
    plot: Rect,
    ends: &[(usize, f64, f64)],
    series_order: &[usize],
    items: &mut Vec<Item>,
    taken: &mut Vec<Rect>,
    name_limit: Option<f64>,
) {
    let lh = ctx.line_height();
    let area = ctx.block.kind == ChartType::Area;
    // Each name is a button: its target is 24 px high, so neighbours sit at least that far apart.
    // A grid step can exchange nearly equal anchors. Replay their shown-value order instead of
    // deciding a new order at a pixel-rounding or tolerance boundary (TYP-36).
    let height = name_height(ctx);
    let anchors: Vec<(f64, f64)> = ends.iter().map(|(_, y, _)| (*y, height)).collect();
    let order = name_indices(ends, series_order);
    let column_x = if area { plot.x - 10.0 } else { plot.right() + 10.0 };
    let placed = if area {
        column_in_bands(
            &anchors,
            &order,
            plot,
            &area_name_bands(ctx, plot, taken),
            name_limit.unwrap_or(f64::INFINITY),
        )
        .expect("area-name feasibility was checked before painting")
    } else {
        crate::labels::place_column_ordered(&anchors, &order, plot.y - lh / 2.0, plot.bottom() + lh / 2.0, 1.0)
            .expect("line-name capacity was checked before painting")
    };
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
            if area { Anchor::End } else { Anchor::Start },
            Face::Data,
            Role::Label,
            color,
        );
        text.series = Some(*s);
        if (cy - y).abs() > 4.0 && !hidden {
            items.push(Item::Line {
                path: Path::polyline(&[
                    if area {
                        [plot.x - 2.0, *y]
                    } else {
                        [plot.right() + 2.0, *y]
                    },
                    [if area { column_x + 3.0 } else { column_x - 3.0 }, cy],
                ]),
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

/// The first clear candidate inside `bounds`, or why there is none.
fn search(
    anchor: [f64; 2],
    radius: f64,
    size: [f64; 2],
    bounds: Rect,
    taken: &[Rect],
    marks: &[(usize, [f64; 2], f64)],
    own: usize,
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
        if !hits_label && !hits_mark {
            return Ok(c);
        }
    }
    Err(if inside_any {
        DropReason::Collision
    } else {
        DropReason::OutOfBounds
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
    for i in order {
        let (row, at, r) = anchors[i];
        let Some(label) = text_of(row) else { continue };
        let w = ctx.text_width(&label);
        let h = ctx.line_height();
        match search(at, r, [w, h], plot, taken, &marks, row) {
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
                if let Some(rect) = leader_spot(at, r, [w, h], plot, taken, &marks) {
                    let t = ctx.text_mid(
                        &label,
                        rect.x,
                        rect.y + h / 2.0,
                        Anchor::Start,
                        Face::Data,
                        Role::Label,
                        ctx.style.ink,
                    );
                    let end = [rect.x - 2.0, rect.y + h / 2.0];
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

fn leader_spot(
    at: [f64; 2],
    r: f64,
    size: [f64; 2],
    bounds: Rect,
    taken: &[Rect],
    marks: &[(usize, [f64; 2], f64)],
) -> Option<Rect> {
    for ring in [28.0, 44.0, 64.0, 90.0] {
        if let Ok(rect) = search(at, r + ring, size, bounds, taken, marks, usize::MAX) {
            return Some(rect);
        }
    }
    None
}

/// Annotations anchored to data coordinates (FEA-CH-32): rules, shaded ranges, marked points
/// and notes, with text through the shared search, clear of the data points, the drawn bars
/// and a point annotation's own ring.
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
            match search(at, radius, [w, h], plot, &avoid, &marks, usize::MAX) {
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
                            place_text(ctx, t, [rect.x + 4.0, rect.y + 10.0], 2.0, 0.0, items, taken);
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
