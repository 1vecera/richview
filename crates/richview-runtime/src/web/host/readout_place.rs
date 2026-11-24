//! Place a readout using the selected mark, visible bounds, and chart geometry.

#[cfg(not(test))]
use super::wedge_readout;
#[cfg(test)]
use crate::test_wedge_readout as wedge_readout;
use richview_chart::geometry::{Item, Panel, Rect, Role, Wedge};

/// The painted outlines of the selected mark where its observation's box holds empty space a
/// readout may use, flattened to 0.05 px: an alluvial or chord ribbon, a venn set's disc or an
/// overlap's lens, and a gauge's whole dial with its needle (P1b). The readout then keeps clear of
/// the ink and its 4 px halo, not of the box's empty corners. Every other kind returns nothing
/// and keeps its box.
pub(super) fn painted_paths(kind: &str, panel: &Panel, observation: usize) -> Vec<Vec<[f64; 2]>> {
    if !matches!(kind, "alluvial" | "gauge" | "chord" | "venn") {
        return Vec::new();
    }
    panel
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Area {
                path, obs, role, ..
            } if *role == Role::Mark && (kind == "gauge" || *obs == Some(observation)) => {
                Some(path.flatten(0.05))
            }
            _ => None,
        })
        .flatten()
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Side {
    Up,
    Right,
    Down,
    Left,
    Top,
    Bottom,
    BandTop,
    BandBottom,
    /// Inside a gauge's arc, in the half away from its needle.
    Hollow,
    FallbackUp,
    FallbackRight,
    FallbackDown,
    FallbackLeft,
    FallbackTop,
    FallbackBottom,
}

impl Side {
    fn canonical(self) -> Self {
        match self {
            Self::FallbackUp => Self::Up,
            Self::FallbackRight => Self::Right,
            Self::FallbackDown => Self::Down,
            Self::FallbackLeft => Self::Left,
            Self::FallbackTop => Self::Top,
            Self::FallbackBottom => Self::Bottom,
            side => side,
        }
    }
    fn fallback(self) -> Self {
        match self.canonical() {
            Self::Up => Self::FallbackUp,
            Self::Right => Self::FallbackRight,
            Self::Down => Self::FallbackDown,
            Self::Left => Self::FallbackLeft,
            Self::Top | Self::BandTop => Self::FallbackTop,
            Self::Bottom | Self::BandBottom => Self::FallbackBottom,
            _ => Self::FallbackUp,
        }
    }
    pub(super) fn is_fallback(self) -> bool {
        self != self.canonical()
    }
    pub(super) fn narrow_placement(self) -> &'static str {
        match self {
            Self::Bottom => "dock-below",
            Self::Top => "dock-above",
            Self::BandTop | Self::BandBottom => "plot-band",
            Self::Hollow => "hollow",
            _ => "fallback",
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum MarkShape<'a> {
    Rect {
        rect: Rect,
        end: Option<Side>,
    },
    Composite {
        anchor: Rect,
        parts: &'a [Rect],
        end: Option<Side>,
    },
    Polygon {
        anchor: Rect,
        paths: &'a [Vec<[f64; 2]>],
        parts: &'a [Rect],
        edge_slack: f64,
        hollow: Option<Hollow>,
    },
    Point {
        center: [f64; 2],
        radius: f64,
    },
    Wedge(Wedge),
    NearestX {
        x: f64,
        dots: &'a [[f64; 2]],
        lines: &'a [Vec<[f64; 2]>],
    },
}

/// The empty disc inside a gauge's arc.
#[derive(Clone, Copy, Debug)]
pub(super) struct Hollow {
    /// The hub the needle turns on.
    pub center: [f64; 2],
    /// The inner radius of the arc.
    pub radius: f64,
    /// A point the needle, or the value's end on a gauge without one, points to.
    pub needle: [f64; 2],
}

/// What a readout leaves clear besides its own mark. When no place clears everything it gives
/// them up in this order: a label's gap, which hides nothing, then other marks, those that
/// stepped back first, then chart text, the
/// note, a legend it stands in for, the subtitle and a gauge's key; its own mark, the title and
/// the toolbar go last (P1b). Right after its mark it keeps every number whole: a text with a
/// digit is left clear or covered wholly, never cut, while some place allows it (P1c).
#[derive(Clone, Copy, Default)]
pub(super) struct Keep<'a> {
    /// The title and the toolbar, and the legend unless the readout stands in for it.
    pub fixed: &'a [Rect],
    pub subtitle: &'a [Rect],
    /// A gauge's key, which repeats the band its readout names: given up after the subtitle and
    /// before the gauge's own needle and track.
    pub key: &'a [Rect],
    /// The legend of a wide chart whose shared readout carries every entry. On a narrow chart
    /// such a readout covers the legend freely, so it is left out.
    pub legend: &'a [Rect],
    pub note: &'a [Rect],
    /// The chart's own text: tick labels, axis titles, value labels and annotations.
    pub text: &'a [Rect],
    /// The bounds of every other filled or point mark; beside a bar, of those that step back
    /// while it is read.
    pub others: &'a [Rect],
    /// Beside a bar, the other marks that stay at full strength while it is read. A bar's
    /// readout takes a place over no other mark while there is one, and gives up the marks that
    /// stepped back before these (P1c).
    pub strong: &'a [Rect],
    /// Every other line mark, costed by the length a readout hides.
    pub lines: &'a [Vec<[f64; 2]>],
    /// Every text with a digit, in the chart and the figure: tick, value and data labels, a
    /// key's values, annotations, and a title, subtitle or note that has one.
    pub numbers: &'a [Rect],
    /// Each entry, swatch and name, of a legend a phone's readout stands in for and may cover:
    /// like a number, an entry is covered wholly or left clear, never cut (P1c).
    pub entries: &'a [Rect],
    /// Beside a bar, the other bars of its own group, read under the same heading. On a phone,
    /// a place over another group's bars goes before one over its own group's (P1c).
    pub group: &'a [Rect],
    /// Whether the mark is one tile among tiles: a pivot's, heatmap's, calendar's, treemap's,
    /// bin2d's, hexbin's, waffle's or icicle's. Every place there covers some number, so a place
    /// beside the tile that cuts none wins over any farther one (P1c).
    pub tiled: bool,
    /// Each line of the note on a phone. A dock may cover the note, but only whole lines of it:
    /// one that would end inside a line widens to the content width, as over a legend (P1c).
    pub note_lines: &'a [Rect],
    /// Whether the search counts an entry cut only across its width as whole, because the
    /// surface will widen over it. `place_whole` asks for this on its second attempt alone.
    pub across: bool,
}

/// Hidden line length counts as this much area: about a stroke's width.
const LINE_WEIGHT: f64 = 2.0;
/// Below this, two costs are the same.
const EPS: f64 = 1e-6;
/// On a wide chart a readout keeps this far from a label it does not cover, so the two never
/// read as one phrase (P1c).
const LABEL_GAP: f64 = 6.0;

fn overlap(a: Rect, b: Rect) -> f64 {
    (a.right().min(b.right()) - a.x.max(b.x)).max(0.0)
        * (a.bottom().min(b.bottom()) - a.y.max(b.y)).max(0.0)
}

/// Whether a readout at `r` cuts `text`: hides some of it but not all.
fn cuts(r: Rect, text: Rect) -> bool {
    overlap(r, text) > EPS && !text.inside(&r, EPS)
}

/// The whole area of each text a readout at `r` touches: half a label costs as much as all of it.
fn whole<'t>(texts: impl IntoIterator<Item = &'t Rect>, r: Rect) -> f64 {
    texts
        .into_iter()
        .filter(|text| overlap(r, **text) > EPS)
        .map(|text| text.w * text.h)
        .sum()
}

/// Texts sorted by their top edge, so a place visits only those level with it. The fallback
/// scores thousands of places on every hover; this keeps that cost near what one text's costs.
struct Rows {
    texts: Vec<Rect>,
    tallest: f64,
}

impl Rows {
    fn new(texts: &[Rect]) -> Self {
        let mut texts = texts.to_vec();
        texts.sort_by(|a, b| a.y.total_cmp(&b.y));
        let tallest = texts.iter().map(|text| text.h).fold(0.0, f64::max);
        Self { texts, tallest }
    }
    /// The texts whose rows meet `r`'s, in order of their tops.
    fn level(&self, r: Rect) -> impl Iterator<Item = &Rect> {
        let start = self
            .texts
            .partition_point(|text| text.y + self.tallest <= r.y);
        let end = self.texts.partition_point(|text| text.y < r.bottom());
        self.texts[start..end.max(start)]
            .iter()
            .filter(move |text| text.bottom() > r.y)
    }
}

fn bounds(mark: &MarkShape<'_>, plot: Rect) -> Rect {
    match mark {
        MarkShape::Rect { rect, .. } => *rect,
        MarkShape::Composite { anchor, .. } => *anchor,
        MarkShape::Polygon { anchor, .. } => *anchor,
        MarkShape::Point {
            center: [x, y],
            radius,
        } => Rect::new(x - radius, y - radius, radius * 2.0, radius * 2.0),
        MarkShape::Wedge(w) => Rect::new(
            w.center[0] - w.radius,
            w.center[1] - w.radius,
            w.radius * 2.0,
            w.radius * 2.0,
        ),
        MarkShape::NearestX { x, .. } => Rect::new(*x, plot.y, 0.0, plot.h),
    }
}

fn touches(mark: &MarkShape<'_>, rect: Rect) -> bool {
    intersects(mark, rect.inflate(4.0))
}

fn circle_intersects(center: [f64; 2], radius: f64, r: Rect) -> bool {
    let x = center[0].clamp(r.x, r.right());
    let y = center[1].clamp(r.y, r.bottom());
    (x - center[0]).hypot(y - center[1]) < radius
}

fn polygon_intersects(path: &[[f64; 2]], r: Rect) -> bool {
    if path.len() < 3 {
        return false;
    }
    if path.iter().any(|&point| r.contains(point))
        || path
            .windows(2)
            .any(|edge| segment_length(edge[0], edge[1], r) > 0.0)
    {
        return true;
    }
    let corners = [
        [r.x, r.y],
        [r.right(), r.y],
        [r.right(), r.bottom()],
        [r.x, r.bottom()],
    ];
    corners.iter().any(|&[x, y]| {
        let mut inside = false;
        for (&a, &b) in path
            .iter()
            .zip(path.iter().cycle().skip(1))
            .take(path.len())
        {
            if (a[1] > y) != (b[1] > y) && x < (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]) + a[0] {
                inside = !inside;
            }
        }
        inside
    })
}

fn intersects(mark: &MarkShape<'_>, r: Rect) -> bool {
    match mark {
        MarkShape::Rect { rect, .. } => rect.overlaps(&r, 0.0),
        MarkShape::Composite { parts, .. } => parts.iter().any(|part| part.overlaps(&r, 0.0)),
        // Include the flattening error and any outline around a filled shape.
        MarkShape::Polygon {
            paths,
            parts,
            edge_slack,
            ..
        } => {
            let r = r.inflate(*edge_slack);
            paths.iter().any(|path| polygon_intersects(path, r))
                || parts.iter().any(|part| part.overlaps(&r, 0.0))
        }
        MarkShape::Point { center, radius } => circle_intersects(*center, *radius, r),
        MarkShape::Wedge(w) => wedge_readout::touches(w, r),
        // The selected shared-line dot is drawn with a six-pixel core by `hover_dot`.
        MarkShape::NearestX { dots, .. } => dots.iter().any(|&p| circle_intersects(p, 6.0, r)),
    }
}

/// The area of a closed path inside `r`: Sutherland–Hodgman clipping by the convex box is exact
/// in area even for a concave path.
fn clipped_area(path: &[[f64; 2]], r: Rect) -> f64 {
    let mut points = path.to_vec();
    let edges: [fn([f64; 2], Rect) -> f64; 4] = [
        |p, r| p[0] - r.x,
        |p, r| r.right() - p[0],
        |p, r| p[1] - r.y,
        |p, r| r.bottom() - p[1],
    ];
    for inside in edges {
        if points.len() < 3 {
            return 0.0;
        }
        let mut kept = Vec::with_capacity(points.len() + 4);
        for (i, &p) in points.iter().enumerate() {
            let q = points[(i + 1) % points.len()];
            let (dp, dq) = (inside(p, r), inside(q, r));
            if dp >= 0.0 {
                kept.push(p);
            }
            if (dp >= 0.0) != (dq >= 0.0) {
                let t = dp / (dp - dq);
                kept.push([p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1])]);
            }
        }
        points = kept;
    }
    let twice: f64 = points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
        .sum();
    twice.abs() / 2.0
}

/// About how much of the selected mark a readout at `r` hides.
fn covered(mark: &MarkShape<'_>, r: Rect) -> f64 {
    // A disc hides about as much as its bounding square, scaled by the square's share.
    let disc = |center: [f64; 2], radius: f64| {
        overlap(
            r,
            Rect::new(
                center[0] - radius,
                center[1] - radius,
                radius * 2.0,
                radius * 2.0,
            ),
        ) * std::f64::consts::FRAC_PI_4
    };
    match mark {
        MarkShape::Rect { rect, .. } => overlap(r, *rect),
        MarkShape::Composite { parts, .. } => parts.iter().map(|part| overlap(r, *part)).sum(),
        MarkShape::Polygon { paths, parts, .. } => {
            paths.iter().map(|path| clipped_area(path, r)).sum::<f64>()
                + parts.iter().map(|part| overlap(r, *part)).sum::<f64>()
        }
        MarkShape::Point { center, radius } => disc(*center, *radius),
        MarkShape::NearestX { dots, .. } => dots.iter().map(|&p| disc(p, 6.0)).sum(),
        // Sample a sector on a 2 px grid.
        MarkShape::Wedge(w) => {
            let mut hidden = 0.0;
            let mut y = r.y + 1.0;
            while y < r.bottom() {
                let mut x = r.x + 1.0;
                while x < r.right() {
                    let (dx, dy) = (x - w.center[0], y - w.center[1]);
                    let radius = dx.hypot(dy);
                    let angle = (dx.atan2(-dy) - w.start).rem_euclid(std::f64::consts::TAU);
                    if radius >= w.inner && radius <= w.radius && angle <= w.end - w.start {
                        hidden += 4.0;
                    }
                    x += 2.0;
                }
                y += 2.0;
            }
            hidden
        }
    }
}

// Liang–Barsky clipping gives the length of a segment hidden by a readout.
fn segment_length(a: [f64; 2], b: [f64; 2], r: Rect) -> f64 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let (mut lo, mut hi): (f64, f64) = (0.0, 1.0);
    for (p, q) in [
        (-dx, a[0] - r.x),
        (dx, r.right() - a[0]),
        (-dy, a[1] - r.y),
        (dy, r.bottom() - a[1]),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return 0.0;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                lo = lo.max(t);
            } else {
                hi = hi.min(t);
            }
        }
    }
    if lo > hi {
        0.0
    } else {
        dx.hypot(dy) * (hi - lo)
    }
}

/// A floor under the length each position's box hides in `line_y`, found for every position in
/// one sweep, so a position sure to hide more than the best so far is not read segment by
/// segment (P11).
///
/// As the box moves down, a segment crossing its columns hides a length that rises, holds and
/// falls with the box's overlap of the rows the segment spans: a trapezoid in y. The floor sums
/// those trapezoids, each shrunk by a margin far above the rounding of `segment_length`, leaves
/// out segments too flat to bound tightly, and subtracts a bound on the sweep's own rounding.
fn hidden_floors(x: f64, size: [f64; 2], segments: &[&[[f64; 2]]], positions: &[f64]) -> Vec<f64> {
    let mut floors = vec![0.0; positions.len()];
    let h = size[1];
    let finite = x.is_finite()
        && size.iter().all(|v| v.is_finite())
        && positions.iter().all(|y| y.is_finite())
        && segments
            .iter()
            .all(|s| s.iter().all(|p| p[0].is_finite() && p[1].is_finite()));
    if !finite || segments.is_empty() {
        return floors;
    }
    let scale = positions
        .iter()
        .chain(segments.iter().flat_map(|s| s.iter().map(|p| &p[1])))
        .fold(x.abs().max((x + size[0]).abs()), |m, v| m.max(v.abs()))
        + h;
    let eta = 1e-9 * (1.0 + 2.0 * scale);
    if h <= 2.0 * eta {
        return floors;
    }
    let mut events = Vec::with_capacity(segments.len() * 4);
    let mut slopes = 0.0;
    for s in segments {
        let [a, b] = [s[0], s[1]];
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        if dx.abs() < 1e-12 || dy == 0.0 {
            continue;
        }
        // The same clip to the box's columns as `segment_length` makes.
        let t1 = (x - a[0]) / dx;
        let t2 = (x + size[0] - a[0]) / dx;
        let (lo, hi) = (t1.min(t2).max(0.0), t1.max(t2).min(1.0));
        let k = dx.hypot(dy) / dy.abs();
        if lo >= hi || k > 1e3 {
            continue;
        }
        let k = k * (1.0 - 1e-6);
        let (p, q) = (a[1] + dy * lo, a[1] + dy * hi);
        let (y0, y1) = (p.min(q), p.max(q));
        if y1 - y0 <= 2.0 * eta {
            continue;
        }
        events.extend([
            (y0 - h + eta, k),
            ((y1 - h).min(y0), -k),
            ((y1 - h).max(y0), -k),
            (y1 - eta, k),
        ]);
        slopes += k;
    }
    if events.is_empty() {
        return floors;
    }
    events.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut order: Vec<usize> = (0..positions.len()).collect();
    order.sort_by(|&i, &j| positions[i].total_cmp(&positions[j]));
    let rounding = 8.0
        * (events.len() + positions.len() + 2) as f64
        * f64::EPSILON
        * (1.0 + slopes)
        * (1.0 + 2.0 * scale + 2.0 * h);
    let (mut value, mut slope, mut at, mut e) = (0.0, 0.0, events[0].0, 0);
    for i in order {
        let y = positions[i];
        while e < events.len() && events[e].0 <= y {
            value += slope * (events[e].0 - at);
            at = events[e].0;
            slope += events[e].1;
            e += 1;
        }
        let here = if e == 0 {
            0.0
        } else {
            value + slope * (y - at)
        };
        floors[i] = ((here - rounding) * (1.0 - 1e-9)).max(0.0);
    }
    floors
}

fn line_y(x: f64, size: [f64; 2], plot: Rect, dots: &[[f64; 2]], lines: &[Vec<[f64; 2]>]) -> f64 {
    let max = (plot.bottom() - size[1]).max(plot.y);
    let target = if dots.is_empty() {
        plot.center()[1]
    } else {
        dots.iter().map(|p| p[1]).sum::<f64>() / dots.len() as f64
    };
    let segments: Vec<_> = lines
        .iter()
        .flat_map(|line| line.windows(2))
        .filter(|s| s[0][0].min(s[1][0]) <= x + size[0] && s[0][0].max(s[1][0]) >= x)
        .collect();
    // The covered length is piecewise linear as the box moves vertically. Its slope changes
    // only at the y coordinates where a segment enters or leaves the box's horizontal span.
    let mut positions = vec![plot.y, max, (target - size[1] / 2.0).clamp(plot.y, max)];
    for segment in &segments {
        let [a, b] = [segment[0], segment[1]];
        let dx = b[0] - a[0];
        let (lo, hi) = if dx.abs() < 1e-12 {
            (0.0, 1.0)
        } else {
            let t1 = (x - a[0]) / dx;
            let t2 = (x + size[0] - a[0]) / dx;
            (t1.min(t2).max(0.0), t1.max(t2).min(1.0))
        };
        if lo > hi {
            continue;
        }
        for t in [lo, hi] {
            let y = a[1] + (b[1] - a[1]) * t;
            positions.push(y.clamp(plot.y, max));
            positions.push((y - size[1]).clamp(plot.y, max));
        }
    }
    let mut best = (f64::INFINITY, f64::INFINITY, plot.y);
    let prune = !reference();
    // Each run of BLOCK segments' rows, so a box skips the runs above or below it whole; a run
    // with a row that is not a number is always read, as its segments are.
    let rows: Vec<[f64; 2]> = segments
        .chunks(BLOCK)
        .map(|run| {
            if run.iter().any(|s| s[0][1].is_nan() || s[1][1].is_nan()) {
                return [f64::NEG_INFINITY, f64::INFINITY];
            }
            run.iter()
                .fold([f64::INFINITY, f64::NEG_INFINITY], |[lo, hi], s| {
                    [lo.min(s[0][1].min(s[1][1])), hi.max(s[0][1].max(s[1][1]))]
                })
        })
        .collect();
    // A point inside the span ends one segment and starts the next, so its positions come
    // twice, two apart; the second reads the first's sum, or the limit that sum passed.
    let mut recent: [Option<(u64, Result<f64, f64>)>; 4] = [None; 4];
    let floors = if prune {
        hidden_floors(x, size, &segments, &positions)
    } else {
        vec![0.0; positions.len()]
    };
    'positions: for (n, y) in positions.into_iter().enumerate() {
        let rect = Rect::new(x, y, size[0], size[1]);
        // A segment whose rows miss the box adds nothing, and a sum past the best so far by more
        // than the tolerance cannot win, so neither needs reading to the end (P11).
        let limit = best.0 + 1e-6;
        if floors[n] > limit {
            continue;
        }
        let known = recent
            .iter()
            .flatten()
            .find(|(bits, _)| *bits == y.to_bits())
            .map(|(_, reading)| *reading);
        let covered = match known {
            Some(Ok(covered)) => covered,
            Some(Err(passed)) if limit <= passed => continue,
            _ => {
                let mut covered = 0.0;
                for (run, segments) in segments.chunks(BLOCK).enumerate() {
                    if prune && (rows[run][1] < rect.y || rows[run][0] > rect.bottom()) {
                        continue;
                    }
                    for s in segments {
                        if prune
                            && (s[0][1].max(s[1][1]) < rect.y
                                || s[0][1].min(s[1][1]) > rect.bottom())
                        {
                            continue;
                        }
                        covered += segment_length(s[0], s[1], rect);
                        if prune && covered > limit {
                            recent[n % 4] = Some((y.to_bits(), Err(limit)));
                            continue 'positions;
                        }
                    }
                }
                covered
            }
        };
        recent[n % 4] = Some((y.to_bits(), Ok(covered)));
        let distance = (y + size[1] / 2.0 - target).abs();
        if covered < best.0 - 1e-6 || ((covered - best.0).abs() < 1e-6 && distance < best.1) {
            best = (covered, distance, y);
        }
    }
    best.2
}

/// Place a readout: past the mark's value end, beside it or opposite, in the chart's margins, or
/// on a narrow chart in a dock above or below the plot. The first clear candidate wins, keeping
/// the previous side while it stays clear; when none clears, the place that gives up least wins.
#[allow(clippy::too_many_arguments)] // The geometry policy takes its independent inputs explicitly.
pub(super) fn place(
    mark: MarkShape<'_>,
    size: [f64; 2],
    chart: [f64; 2],
    plot: Rect,
    visible: Rect,
    keep: &Keep<'_>,
    narrow: bool,
    previous: Option<Side>,
) -> (Rect, Side) {
    let b = bounds(&mark, plot);
    let c = b.center();
    let at = |side, gap| match side {
        Side::Up => Rect::new(c[0] - size[0] / 2.0, b.y - gap - size[1], size[0], size[1]),
        Side::Down => Rect::new(c[0] - size[0] / 2.0, b.bottom() + gap, size[0], size[1]),
        Side::Right => Rect::new(b.right() + gap, c[1] - size[1] / 2.0, size[0], size[1]),
        Side::Left => Rect::new(b.x - gap - size[0], c[1] - size[1] / 2.0, size[0], size[1]),
        Side::Top => Rect::new(plot.x, plot.y - gap - size[1], size[0], size[1]),
        Side::Bottom => Rect::new(plot.x, plot.bottom() + gap, size[0], size[1]),
        _ => unreachable!(),
    };
    let end = match &mark {
        MarkShape::Rect { end, .. } | MarkShape::Composite { end, .. } => {
            end.unwrap_or(Side::Right)
        }
        MarkShape::Wedge(w) => {
            let mid = (w.start + w.end) / 2.0;
            if mid.sin().abs() > mid.cos().abs() {
                if mid.sin() >= 0.0 {
                    Side::Right
                } else {
                    Side::Left
                }
            } else if mid.cos() >= 0.0 {
                Side::Up
            } else {
                Side::Down
            }
        }
        _ => Side::Up,
    };
    let opposite = match end {
        Side::Up => Side::Down,
        Side::Down => Side::Up,
        Side::Left => Side::Right,
        _ => Side::Left,
    };
    let mut candidates = if narrow {
        vec![
            (at(Side::Bottom, 0.0), Side::Bottom),
            (at(Side::Top, 0.0), Side::Top),
        ]
    } else {
        vec![
            (at(end, 8.0), end),
            (at(Side::Right, 12.0), Side::Right),
            (at(Side::Left, 12.0), Side::Left),
            (at(opposite, 8.0), opposite),
            (at(Side::Top, 0.0), Side::Top),
            (at(Side::Bottom, 0.0), Side::Bottom),
        ]
    };
    if matches!(
        mark,
        MarkShape::Rect { end: None, .. } | MarkShape::Composite { end: None, .. }
    ) && !narrow
    {
        candidates = vec![
            (at(Side::Right, 12.0), Side::Right),
            (at(Side::Left, 12.0), Side::Left),
            (at(Side::Top, 0.0), Side::Top),
            (at(Side::Bottom, 0.0), Side::Bottom),
        ];
    }
    if let MarkShape::Wedge(w) = &mark
        && !narrow
    {
        let mid = (w.start + w.end) / 2.0;
        let direction = [mid.sin(), -mid.cos()];
        let support = (size[0] * direction[0].abs() + size[1] * direction[1].abs()) / 2.0;
        let distance = w.radius + 8.0 + support;
        candidates[0].0 = Rect::new(
            w.center[0] + direction[0] * distance - size[0] / 2.0,
            w.center[1] + direction[1] * distance - size[1] / 2.0,
            size[0],
            size[1],
        );
    }
    // Beside a line's points the readout keeps to the right, at the height that hides least of
    // the lines; the left side's height is found only when the right one is not taken (P11).
    let mut left = None;
    if let MarkShape::NearestX { dots, lines, .. } = &mark
        && !narrow
    {
        let mut right = at(Side::Right, 12.0);
        right.y = line_y(right.x, size, plot, dots, lines);
        candidates = vec![(right, Side::Right)];
        left = Some(at(Side::Left, 12.0));
    }
    let visible_inset = Rect::new(
        visible.x + 8.0,
        visible.y + 8.0,
        (visible.w - 16.0).max(0.0),
        (visible.h - 16.0).max(0.0),
    );
    let allowed = if narrow {
        visible_inset
    } else {
        let left = visible_inset.x.max(8.0);
        let top = visible_inset.y.max(8.0);
        Rect::new(
            left,
            top,
            (visible_inset.right().min(chart[0] - 8.0) - left).max(0.0),
            (visible_inset.bottom().min(chart[1] - 8.0) - top).max(0.0),
        )
    };
    // Edge points move only along the side on which the panel is centred.
    if !narrow {
        for (r, side) in &mut candidates {
            match side {
                Side::Up | Side::Down => {
                    r.x = r.x.clamp(allowed.x, (allowed.right() - r.w).max(allowed.x))
                }
                Side::Left | Side::Right if !matches!(mark, MarkShape::NearestX { .. }) => {
                    r.y =
                        r.y.clamp(allowed.y, (allowed.bottom() - r.h).max(allowed.y))
                }
                _ => {}
            }
        }
    }
    let covers = |rects: &[Rect], r: Rect| rects.iter().any(|text| text.overlaps(&r, 0.0));
    // A phone's docks sit flush under the axis text; elsewhere the chart's labels keep a gap.
    let gap = if narrow { 0.0 } else { LABEL_GAP };
    // Docks and bands may cover the note and a legend the readout stands in for.
    let clear_by = |r: Rect, gap: f64| {
        !covers(keep.fixed, r)
            && !covers(keep.key, r.inflate(gap))
            && !covers(keep.subtitle, r)
            && !covers(keep.text, r.inflate(gap))
    };
    let clear_of_text = |r: Rect| clear_by(r, gap);
    // A bar's readout is clear only where it covers no other mark: beside a bar in a group the
    // usual places lie on its neighbours, while the paper above them is free (P1c). Tiles,
    // points and shared lines keep their readout beside them. On a phone the docks still come
    // first; a bar that cannot dock stays with its bar instead of a band over other rows (P13).
    let bar = match &mark {
        MarkShape::Rect {
            rect,
            end: Some(end),
        } => Some((*rect, *end)),
        MarkShape::Composite {
            anchor,
            end: Some(end),
            ..
        } => Some((*anchor, *end)),
        _ => None,
    };
    let on_line = |r: Rect| {
        keep.lines
            .iter()
            .flat_map(|line| line.windows(2))
            .any(|s| segment_length(s[0], s[1], r) > 0.0)
    };
    // Over nothing but marks that stepped back, and over no other mark at all.
    let dimmed_only = |r: Rect| bar.is_none() || (!covers(keep.strong, r) && !on_line(r));
    let free = |r: Rect| dimmed_only(r) && (bar.is_none() || !covers(keep.others, r));
    let with_bar = |r: Rect| bar.is_none_or(|(bar, end)| stays(bar, end, r));
    // Clear of everything but other marks.
    let usual_by = |r: Rect, gap: f64| {
        r.inside(&allowed, 0.0)
            && !touches(&mark, r)
            && clear_by(r, gap)
            && !covers(keep.legend, r)
            && !covers(keep.note, r)
    };
    let valid_by = |r: Rect, gap: f64| usual_by(r, gap) && free(r);
    let valid = |r: Rect| valid_by(r, gap);
    // A margin is a bar's place only where it stays with the bar.
    let in_reach = |r: Rect, side: Side| !matches!(side, Side::Top | Side::Bottom) || with_bar(r);
    let takes = |r: Rect, side: Side| valid(r) && in_reach(r, side);
    // A gauge's readout tries the half of its hollow away from the needle first; on a narrow
    // chart, right after the docks. The hollow is tight around the gauge's own value, which the
    // readout repeats, so there it only has to stay off the text.
    let hollow = || match &mark {
        MarkShape::Polygon {
            hollow: Some(hollow),
            ..
        } => in_hollow(hollow, size, |r| valid_by(r, 0.0)),
        _ => None,
    };
    // The places a search tries, as columns and rows: the readout's room, 8 px in, on an 8 px
    // grid: the visible figure on narrow charts, the visible chart on wider ones (f614f4ec).
    let (min_x, max_x, min_y, max_y) = (
        allowed.x,
        allowed.right() - size[0],
        allowed.y,
        allowed.bottom() - size[1],
    );
    let lattice = || {
        if max_x < min_x || max_y < min_y {
            return None;
        }
        let nx = ((max_x - min_x) / 8.0).ceil() as usize;
        let ny = ((max_y - min_y) / 8.0).ceil() as usize;
        let mut xs: Vec<_> = (0..=nx)
            .map(|i| (min_x + i as f64 * 8.0).min(max_x))
            .collect();
        let mut ys: Vec<_> = (0..=ny)
            .map(|i| (min_y + i as f64 * 8.0).min(max_y))
            .collect();
        // A clear gap can be narrower than the grid. Include the boundaries of everything the
        // readout keeps clear, and of its mark and that mark's halo, so the search can land
        // exactly beside them: a label's boundary is the gap around it.
        let mut boundaries: Vec<Rect> = [keep.fixed, keep.subtitle, keep.legend, keep.note]
            .concat()
            .into_iter()
            .chain(keep.key.iter().chain(keep.text).copied())
            .collect();
        // A label's edges come twice: flush, and a gap away where the readout stands clear of
        // reading on from it.
        if gap > 0.0 {
            let apart: Vec<Rect> = keep
                .key
                .iter()
                .chain(keep.text)
                .map(|label| label.inflate(gap))
                .collect();
            boundaries.extend(apart);
        }
        boundaries.extend([b, b.inflate(4.0)]);
        if let MarkShape::NearestX { dots, .. } = &mark {
            for p in dots.iter() {
                boundaries
                    .extend([6.0, 10.0].map(|r| Rect::new(p[0] - r, p[1] - r, r * 2.0, r * 2.0)));
            }
        }
        if let MarkShape::Composite { parts, .. } | MarkShape::Polygon { parts, .. } = &mark {
            for part in parts.iter() {
                boundaries.extend([*part, part.inflate(4.0)]);
            }
        }
        for boundary in boundaries {
            xs.extend(
                [boundary.x - size[0] - 0.01, boundary.right() + 0.01]
                    .map(|x| x.clamp(min_x, max_x)),
            );
            ys.extend(
                [boundary.y - size[1] - 0.01, boundary.bottom() + 0.01]
                    .map(|y| y.clamp(min_y, max_y)),
            );
        }
        xs.sort_by(f64::total_cmp);
        xs.dedup_by(|a, b| (*a - *b).abs() < 0.01);
        ys.sort_by(f64::total_cmp);
        ys.dedup_by(|a, b| (*a - *b).abs() < 0.01);
        Some((xs, ys))
    };
    // The nearest place that is clear of every label and every other mark and stays with the
    // bar: in the bar's own row or column, within reach. The other marks' edges join the grid
    // here, so it can land exactly beside them. Staying with the bar counts for more than a
    // label's gap.
    let by_bar = |xs: &[f64], ys: &[f64]| {
        let (bar, end) = bar?;
        let (mut near_xs, mut near_ys) = (xs.to_vec(), ys.to_vec());
        for other in keep.strong.iter().chain(keep.others).take(512) {
            near_xs.extend(
                [other.x - size[0] - 0.01, other.right() + 0.01].map(|x| x.clamp(min_x, max_x)),
            );
            near_ys.extend(
                [other.y - size[1] - 0.01, other.bottom() + 0.01].map(|y| y.clamp(min_y, max_y)),
            );
        }
        // Staying with the bar is one condition on a place's column and one on its row, so
        // the columns and rows that cannot stay are left out before they are paired: a chart
        // of hundreds of bars would otherwise pair a million places on every hover.
        // A place over the bar's own corner meets both, so each is tried with the other met.
        let probe = |x: f64, y: f64| stays(bar, end, Rect::new(x, y, size[0], size[1]));
        let corner = [bar.x - size[0] / 2.0, bar.y - size[1] / 2.0];
        near_xs.retain(|&x| probe(x, corner[1]));
        near_ys.retain(|&y| probe(corner[0], y));
        let mut near: Vec<Rect> = near_xs
            .iter()
            .flat_map(|&x| {
                near_ys
                    .iter()
                    .map(move |&y| Rect::new(x, y, size[0], size[1]))
            })
            .filter(|r| with_bar(*r))
            .collect();
        let from = |r: &Rect| {
            let center = r.center();
            (center[0] - c[0]).hypot(center[1] - c[1])
        };
        // On a phone the thumb hides what is below the bar: a place level with the bar's
        // middle or above it goes before one below.
        let below = |r: &Rect| narrow && r.center()[1] > c[1];
        sort_by_key_once(
            &mut near,
            |r| (below(r), from(r)),
            |a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)),
        );
        near.iter()
            .find(|r| valid(**r))
            .or_else(|| near.iter().find(|r| valid_by(**r, 0.0)))
            .map(|&r| {
                let side = if r.center()[0] >= c[0] {
                    Side::Right
                } else {
                    Side::Left
                };
                (r, side.fallback())
            })
    };
    // On a phone, with no free place that stays with the bar or lies outside the plot: the
    // nearest place within reach of the bar over other bars, those that stepped back before those
    // at full strength, another group's before its own group's, cutting no number. The side
    // candidates are centred on the bar and often lie on a label there, while a place a little
    // along the bar is clear (P13). The thumb hides what is below the bar, so a place there
    // counts as a fingertip farther away: above wins unless below is much nearer.
    let over_bars = |xs: &[f64], ys: &[f64]| {
        let (bar, _) = bar?;
        let (mut near_xs, mut near_ys) = (xs.to_vec(), ys.to_vec());
        for other in keep.strong.iter().chain(keep.others).take(512) {
            near_xs.extend(
                [other.x - size[0] - 0.01, other.right() + 0.01].map(|x| x.clamp(min_x, max_x)),
            );
            near_ys.extend(
                [other.y - size[1] - 0.01, other.bottom() + 0.01].map(|y| y.clamp(min_y, max_y)),
            );
        }
        near_xs.retain(|&x| x + size[0] >= bar.x - REACH && x <= bar.right() + REACH);
        near_ys.retain(|&y| y + size[1] >= bar.y - REACH && y <= bar.bottom() + REACH);
        for values in [&mut near_xs, &mut near_ys] {
            values.sort_by(f64::total_cmp);
            values.dedup_by(|a, b| (*a - *b).abs() < 0.01);
        }
        let from = |r: &Rect| {
            let center = r.center();
            (center[0] - c[0]).hypot(center[1] - c[1])
        };
        let mut near: Vec<Rect> = near_xs
            .iter()
            .flat_map(|&x| {
                near_ys
                    .iter()
                    .map(move |&y| Rect::new(x, y, size[0], size[1]))
            })
            .filter(|r| apart(bar, *r) <= REACH)
            .collect();
        // Nearest by the space between bar and readout, in steps of the grid; then by centres.
        let step = |r: &Rect| {
            let thumb = if r.center()[1] > c[1] { THUMB } else { 0.0 };
            ((apart(bar, *r) + thumb) / 8.0).floor()
        };
        sort_by_key_once(
            &mut near,
            |r| (step(r), from(r)),
            |a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)),
        );
        // Over dimmed bars only, of other groups; then over its own group's too; then over bars
        // at full strength, other groups' before its own group's.
        let class =
            |r: Rect| 2 * u8::from(covers(keep.strong, r)) + u8::from(covers(keep.group, r));
        let whole_entries = |r: Rect| !keep.entries.iter().any(|entry| cuts(r, *entry));
        // Tried up to a bound, so a chart of hundreds of bars stays within a frame.
        let mut best: Option<(u8, Rect)> = None;
        for &r in near.iter().take(3000) {
            if !(usual_by(r, gap) && !on_line(r) && whole_entries(r)) {
                continue;
            }
            let class = class(r);
            if best.is_none_or(|(b, _)| class < b) {
                best = Some((class, r));
                if class == 0 {
                    break;
                }
            }
        }
        best.map(|(_, r)| {
            let side = if r.center()[0] >= c[0] {
                Side::Right
            } else {
                Side::Left
            };
            (r, side.fallback())
        })
    };
    // On a phone, a free place outside the plot reads as a dock however far it is from the bar:
    // the row beside the toolbar, say. The nearest goes first.
    let outside = |xs: &[f64], ys: &[f64]| {
        bar?;
        let from = |r: &Rect| {
            let center = r.center();
            (center[0] - c[0]).hypot(center[1] - c[1])
        };
        let mut places: Vec<Rect> = xs
            .iter()
            .flat_map(|&x| ys.iter().map(move |&y| Rect::new(x, y, size[0], size[1])))
            .filter(|r| !r.overlaps(&plot, 0.0))
            .collect();
        sort_by_key_once(&mut places, from, f64::total_cmp);
        places
            .into_iter()
            .find(|r| valid(*r) && !covers(keep.entries, *r))
            .map(|r| {
                let side = if r.center()[0] >= c[0] {
                    Side::Right
                } else {
                    Side::Left
                };
                (r, side.fallback())
            })
    };
    // Align narrow candidates with the plot where possible while respecting the figure's inset.
    let dock_x = plot
        .x
        .clamp(allowed.x, (allowed.right() - size[0]).max(allowed.x));
    // The y of the lower and upper docks.
    let mut docks = [plot.bottom(), plot.y - size[1]];
    if narrow {
        for (rect, _) in &mut candidates {
            rect.x = dock_x;
        }
        // The lower dock follows axis text beneath the plot; the note itself may be covered.
        let below = keep
            .text
            .iter()
            .filter(|label| {
                label.center()[1] >= plot.bottom()
                    && label.right() > plot.x
                    && label.x < plot.right()
            })
            .map(|label| label.bottom())
            .fold(plot.bottom(), f64::max);
        candidates[0].0.y = below;
        docks = [below, plot.y - size[1]];
        let clear_dock = |r: Rect| {
            r.inside(&allowed, 0.0)
                && !r.overlaps(&plot, 0.0)
                && !touches(&mark, r)
                && clear_of_text(r)
        };
        if let Some(&candidate) = candidates.iter().find(|(r, _)| clear_dock(*r)) {
            return candidate;
        }
        if let Some(r) = hollow() {
            return (r, Side::Hollow);
        }
        // A bar that cannot dock: past its end or beside it where that lies on no other mark,
        // else the nearest free place that stays with it, else the nearest place within reach
        // over marks that stepped back. These stand in for the bands, which lie over other rows
        // and may cover a bar at full strength: a bar with none of them is searched for.
        if bar.is_some() {
            let mut sides = [
                (at(end, 8.0), end),
                (at(Side::Right, 12.0), Side::Right),
                (at(Side::Left, 12.0), Side::Left),
                (at(opposite, 8.0), opposite),
            ];
            for (r, side) in &mut sides {
                if matches!(side, Side::Up | Side::Down) {
                    r.x = r.x.clamp(allowed.x, (allowed.right() - r.w).max(allowed.x));
                } else {
                    r.y =
                        r.y.clamp(allowed.y, (allowed.bottom() - r.h).max(allowed.y));
                }
            }
            // The thumb hides what is below the bar: for a bar whose value end points down,
            // the place above it goes first and the one past its end last.
            if end == Side::Down {
                sides.swap(0, 3);
            }
            let found = sides.iter().find(|(r, _)| valid(*r)).copied().or_else(|| {
                let (xs, ys) = lattice()?;
                by_bar(&xs, &ys)
                    .or_else(|| outside(&xs, &ys))
                    .or_else(|| over_bars(&xs, &ys))
            });
            if let Some((r, side)) = found {
                return (r, side.fallback());
            }
        }
        let top = (Rect::new(dock_x, plot.y, size[0], size[1]), Side::BandTop);
        let bottom = (
            Rect::new(dock_x, plot.bottom() - size[1], size[0], size[1]),
            Side::BandBottom,
        );
        let anchor_y = match &mark {
            MarkShape::NearestX { dots, .. } if !dots.is_empty() => {
                dots.iter().map(|point| point[1]).sum::<f64>() / dots.len() as f64
            }
            _ => c[1],
        };
        let mut bands = [top, bottom];
        bands.sort_by(|a, b| {
            (b.0.center()[1] - anchor_y)
                .abs()
                .total_cmp(&(a.0.center()[1] - anchor_y).abs())
        });
        let clear_band = |r: Rect| {
            r.inside(&allowed, 0.0)
                && r.inside(&plot, 0.0)
                && !touches(&mark, r)
                && clear_of_text(r)
        };
        if bar.is_none()
            && let Some(&candidate) = bands.iter().find(|(r, _)| clear_band(*r))
        {
            return candidate;
        }
        candidates.extend(bands);
    }
    if !narrow && let Some(r) = hollow() {
        return (r, Side::Hollow);
    }
    if let Some(mut rect) = left
        && let MarkShape::NearestX { dots, lines, .. } = &mark
    {
        let right = candidates[0].0;
        if valid(right) && previous.map(Side::canonical) != Some(Side::Left) {
            return (right, Side::Right);
        }
        rect.y = line_y(rect.x, size, plot, dots, lines);
        candidates.push((rect, Side::Left));
    }
    // Among tiles every place covers some number, so the amount of text must not send a tile's
    // readout away: beside the tile wins over any farther place, a margin's too, while it cuts
    // no number, and least text decides only between its two sides, the previous one first on a
    // tie (P1c).
    if keep.tiled && !narrow {
        let search = Search::new(&mark, keep, gap, bar);
        let mut beside: Vec<_> = candidates
            .iter()
            .copied()
            .filter(|&(r, s)| {
                matches!(s, Side::Right | Side::Left)
                    && r.inside(&allowed, 0.0)
                    && !touches(&mark, r)
                    && !covers(keep.fixed, r)
                    && search.cut(r) <= EPS
            })
            .collect();
        if let Some(p) = previous {
            beside.sort_by_key(|&(_, s)| s != p.canonical());
        }
        if !beside.is_empty() {
            let (r, s) = beside[search.rank(&beside)];
            return (r, if takes(r, s) { s } else { s.fallback() });
        }
    }
    if !narrow
        && let Some(p) = previous
        && let Some(&(r, s)) = candidates
            .iter()
            .find(|(r, s)| *s == p.canonical() && takes(*r, *s))
    {
        return (r, s);
    }
    if !narrow && let Some(&(r, s)) = candidates.iter().find(|(r, s)| takes(*r, *s)) {
        return (r, s);
    }
    let usual = if bar.is_some() && !narrow {
        candidates.clone()
    } else {
        Vec::new()
    };
    let mut fallback: Vec<_> = candidates
        .into_iter()
        .map(|(mut r, s)| {
            r.x = r.x.clamp(allowed.x, (allowed.right() - r.w).max(allowed.x));
            r.y =
                r.y.clamp(allowed.y, (allowed.bottom() - r.h).max(allowed.y));
            (r, s)
        })
        .collect();
    if let Some((xs, ys)) = lattice() {
        if bar.is_some()
            && !narrow
            && let Some(found) = by_bar(&xs, &ys)
        {
            return found;
        }
        for &x in &xs {
            for &y in &ys {
                let r = Rect::new(x, y, size[0], size[1]);
                let side = if r.center()[0] >= c[0] {
                    Side::Right
                } else {
                    Side::Left
                };
                fallback.push((r, side));
            }
        }
    }
    // On a phone, a place inside the plot farther than reach from the bar reads as another bar's
    // and no longer counts (P1c), while a place that does count covers no text. Where every
    // such place would lie on a label or the figure's text, the far places stay in and the
    // least-coverage search decides, as it did before the rule: the placement check holds a
    // readout to any place that clears a text it covers (the lead, 19 Nov 2026).
    if narrow && let Some((bar, _)) = bar {
        let counts = |r: &Rect| !r.overlaps(&plot, 0.0) || apart(bar, *r) <= REACH;
        let clear =
            |r: &Rect| clear_by(*r, 0.0) && !covers(keep.legend, *r) && !covers(keep.note, *r);
        if fallback.iter().any(|(r, _)| counts(r) && clear(r)) {
            fallback.retain(|(r, _)| counts(r));
        }
    }
    // With no free place that stays with the bar, the first usual place over nothing but marks
    // that stepped back: a free place far from the bar would read as another bar's.
    if let Some(&(r, s)) = usual
        .iter()
        .find(|(r, s)| in_reach(*r, *s) && usual_by(*r, gap) && dimmed_only(*r))
    {
        return (r, s);
    }
    // Staying with the bar counts for more than a label's gap: the same place, off every label
    // but nearer one than the gap, before any place away from the bar.
    if let Some(&(r, s)) = usual
        .iter()
        .find(|(r, s)| in_reach(*r, *s) && usual_by(*r, 0.0) && dimmed_only(*r))
    {
        return (r, s.fallback());
    }
    // Ties go to the place nearest the docks on a narrow chart, and nearest the mark otherwise;
    // a bar keeps its readout near it on a narrow chart too.
    // A wedge is near where its outer arc is: a rose's wedge touches the centre, and the middle
    // of its box can lie on another wedge (P1c).
    let anchor = match &mark {
        MarkShape::NearestX { x, dots, .. } if !dots.is_empty() => [
            *x,
            dots.iter().map(|p| p[1]).sum::<f64>() / dots.len() as f64,
        ],
        MarkShape::Wedge(w) => {
            let mid = (w.start + w.end) / 2.0;
            [
                w.center[0] + w.radius * mid.sin(),
                w.center[1] - w.radius * mid.cos(),
            ]
        }
        _ => c,
    };
    let distance = |r: Rect| {
        if narrow && bar.is_none() {
            docks
                .iter()
                .map(|&y| (r.x - dock_x).abs() + (r.y - y).abs())
                .fold(f64::INFINITY, f64::min)
        } else {
            let center = r.center();
            (center[0] - anchor[0]).hypot(center[1] - anchor[1])
        }
    };
    sort_by_key_once(&mut fallback, |p| distance(p.0), f64::total_cmp);
    let search = Search::new(&mark, keep, gap, bar);
    let mut chosen = fallback[search.rank(&fallback)];
    let cut = |r: Rect| {
        search
            .cost(NUMBERS, r, f64::INFINITY)
            .expect("no limit, so a cost")
    };
    // The grid can miss the narrow stretch where a number is clear or wholly covered. Slide the
    // best place to each cut number's edges, a few times over, and keep what ranks better; ties
    // stay with the nearer place (P1c).
    for _ in 0..3 {
        if cut(chosen.0) <= EPS {
            break;
        }
        let r = chosen.0;
        let mut shifts = vec![chosen];
        let mut nearby: Vec<(Rect, Side)> = Vec::new();
        for number in search.numbers.level(r).filter(|number| cuts(r, **number)) {
            // Beside the number, flush and with a label's gap, or covering all of it.
            let xs = [
                number.x - size[0] - 0.01,
                number.right() + 0.01,
                number.x - size[0] - gap - 0.01,
                number.right() + gap + 0.01,
                number.x,
                number.right() - size[0],
                r.x,
            ];
            let ys = [
                number.y - size[1] - 0.01,
                number.bottom() + 0.01,
                number.y - size[1] - gap - 0.01,
                number.bottom() + gap + 0.01,
                number.y,
                number.bottom() - size[1],
                r.y,
            ];
            for x in xs {
                for y in ys {
                    let x = x.clamp(allowed.x, (allowed.right() - size[0]).max(allowed.x));
                    let y = y.clamp(allowed.y, (allowed.bottom() - size[1]).max(allowed.y));
                    let shifted = Rect::new(x, y, size[0], size[1]);
                    let side = if shifted.center()[0] >= c[0] {
                        Side::Right
                    } else {
                        Side::Left
                    };
                    nearby.push((shifted, side));
                }
            }
        }
        sort_by_key_once(&mut nearby, |p| distance(p.0), f64::total_cmp);
        shifts.extend(nearby);
        let next = shifts[search.rank(&shifts)];
        if next.0 == chosen.0 {
            break;
        }
        chosen = next;
    }
    (chosen.0, chosen.1.fallback())
}

/// A placed readout: where its rows sit, and the surface behind them. The surface is larger than
/// the rows only where a phone's readout would otherwise cut a legend entry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Placed {
    pub rows: Rect,
    pub surface: Rect,
    pub side: Side,
}

/// Place a readout whole (`place_once`). Where the place found still cuts an entry of a legend
/// a phone's readout stands in for, a second search counts an entry cut only across its width as
/// whole and lets the surface widen over it: a legend between the subtitle and the plot, where
/// no dock fits and every place of the readout's own width cuts an entry. The second place
/// stands only if its surface cuts no entry and no more numbers than the first; otherwise the
/// first does, and the figure has no place that keeps every entry whole.
#[allow(clippy::too_many_arguments)] // The same independent inputs as `place`.
pub(super) fn place_whole(
    mark: MarkShape<'_>,
    size: [f64; 2],
    chart: [f64; 2],
    plot: Rect,
    visible: Rect,
    keep: &Keep<'_>,
    narrow: bool,
    previous: Option<Side>,
) -> Placed {
    let first = place_once(mark, size, chart, plot, visible, keep, narrow, previous);
    let entries = |s: Rect| keep.entries.iter().filter(|entry| cuts(s, **entry)).count();
    if !narrow || keep.across || entries(first.surface) == 0 {
        return first;
    }
    let across = Keep {
        across: true,
        ..*keep
    };
    let second = place_once(mark, size, chart, plot, visible, &across, narrow, previous);
    let numbers = |s: Rect| {
        keep.numbers
            .iter()
            .filter(|number| cuts(s, **number))
            .count()
    };
    if entries(second.surface) == 0 && numbers(second.surface) <= numbers(first.surface) {
        second
    } else {
        first
    }
}

/// Place a readout as `place` does, then keep every entry of a legend it stands in for whole on
/// a phone (P1c): a readout over half an entry reads as a collision. It first slides a little, to
/// an entry's edges, where that gives up nothing more and lies on no further label. If an entry
/// is still cut across its width, the surface widens to the chart's width, out to the figure's
/// edge where the legend reaches it, with the rows at their natural width at the plot's left
/// edge: a dock's, and a searched place's that covers no more of the plot for it. Its top and
/// bottom edges then fall between lines: it slides a little to where they do, or else an edge
/// moves outward past the entry or label it would run through.
#[allow(clippy::too_many_arguments)] // The same independent inputs as `place`.
fn place_once(
    mark: MarkShape<'_>,
    size: [f64; 2],
    chart: [f64; 2],
    plot: Rect,
    visible: Rect,
    keep: &Keep<'_>,
    narrow: bool,
    previous: Option<Side>,
) -> Placed {
    let (r, side) = place(mark, size, chart, plot, visible, keep, narrow, previous);
    let natural = |r: Rect| Placed {
        rows: r,
        surface: r,
        side,
    };
    // A dock may cover the note too, a whole line of it or none, like a legend entry.
    let docked = narrow && matches!(side, Side::Top | Side::Bottom);
    let pieces: Vec<Rect> = keep
        .entries
        .iter()
        .chain(keep.note_lines.iter().filter(|_| docked))
        .copied()
        .collect();
    let cut = |r: Rect| pieces.iter().any(|piece| cuts(r, *piece));
    if !narrow || !cut(r) {
        return natural(r);
    }
    let allowed = Rect::new(
        visible.x + 8.0,
        visible.y + 8.0,
        (visible.w - 16.0).max(0.0),
        (visible.h - 16.0).max(0.0),
    );
    let search = Search::new(&mark, keep, 0.0, None);
    // How many of the chart's labels a place lies on.
    let labels = |s: Rect| {
        keep.text
            .iter()
            .filter(|label| overlap(s, **label) > EPS)
            .count()
    };
    let mut shifts = Vec::new();
    for entry in pieces.iter().filter(|entry| cuts(r, **entry)) {
        let xs = [
            entry.x - r.w - 0.01,
            entry.right() + 0.01,
            entry.x,
            entry.right() - r.w,
            r.x,
        ];
        let ys = [
            entry.y - r.h - 0.01,
            entry.bottom() + 0.01,
            entry.y,
            entry.bottom() - r.h,
            r.y,
        ];
        for x in xs {
            for y in ys {
                let x = x.clamp(allowed.x, (allowed.right() - r.w).max(allowed.x));
                let y = y.clamp(allowed.y, (allowed.bottom() - r.h).max(allowed.y));
                let shifted = Rect::new(x, y, r.w, r.h);
                // A slide is small: past half its own size the readout has left its place. A
                // dock stays a dock: it covers no more of the plot.
                let small = (x - r.x).abs() <= r.w / 2.0 && (y - r.y).abs() <= r.h / 2.0;
                // Nor does it come to lie on a label the place found leaves clear: a dock that
                // slid up over the axis labels to spare a legend entry hid the axis instead.
                if small
                    && !cut(shifted)
                    && overlap(shifted, plot) <= overlap(r, plot) + EPS
                    && labels(shifted) <= labels(r)
                {
                    shifts.push(shifted);
                }
            }
        }
    }
    let moved = |s: &Rect| (s.x - r.x).abs() + (s.y - r.y).abs();
    shifts.sort_by(|a, b| moved(a).total_cmp(&moved(b)));
    // The nearest slide that gives up no more than the place found.
    if let Some(&shifted) = shifts
        .iter()
        .find(|shifted| search.rank(&[(**shifted, side), (r, side)]) == 0)
    {
        return natural(shifted);
    }
    // The surface widens over the entries it cuts across their width: the readout stands in for
    // the legend, so the room beside it there is the legend's. A dock's rows start at the plot's
    // left edge; a searched place where no dock fits, a legend between the subtitle and the
    // plot, widens the same way.
    let across = |entry: &&Rect| {
        overlap(r, **entry) > EPS && (entry.x < r.x - EPS || entry.right() > r.right() + EPS)
    };
    let (mut rows, mut surface) = (r, r);
    if pieces.iter().any(|entry| across(&entry)) {
        let left = pieces
            .iter()
            .filter(across)
            .map(|entry| entry.x)
            .fold(plot.x.min(r.x), f64::min)
            .max(visible.x);
        let right = pieces
            .iter()
            .filter(across)
            .map(|entry| entry.right())
            .fold(chart[0].max(r.right()), f64::max)
            .min(visible.right());
        surface = Rect::new(left, r.y, right - left, r.h);
        rows.x = plot.x.clamp(left, (right - r.w).max(left));
    }
    // The lines of text an edge must not run through. A box much taller than a line, such as a
    // turned axis title, is not one.
    let lines: Vec<Rect> = pieces
        .iter()
        .chain(keep.text)
        .filter(|line| line.h <= LINE)
        .copied()
        .collect();
    let through = |s: Rect| {
        lines.iter().any(|line| {
            overlap(s, *line) > EPS && (line.y < s.y - EPS || line.bottom() > s.bottom() + EPS)
        })
    };
    // A surface stays in the visible figure, whose edge a legend can reach, and covers no more
    // of the title, toolbar or subtitle. It may come to stand flush beside its own mark, a node
    // at the foot of the plot above a dock, but hides no more of it than the place found.
    let on_mark = |s: Rect| search.cost(1, s, f64::INFINITY).unwrap_or(f64::INFINITY);
    let fits = |s: Rect| {
        s.inside(&visible, 0.5)
            && on_mark(s) <= on_mark(r).max(1.0) + EPS
            && (docked || overlap(s, plot) <= overlap(r, plot) + EPS)
            && area(keep.fixed, s) <= area(keep.fixed, r) + EPS
            && area(keep.subtitle, s) <= area(keep.subtitle, r) + EPS
    };
    // First it slides a little, up or down, to where both edges fall between lines, without
    // covering more of the plot or another of the chart's labels.
    // The slides that bring an edge of the surface to an edge of a line it lies on, nearest
    // first. A slide is small: past half the readout's height it has left its place.
    let mut slides: Vec<f64> = lines
        .iter()
        .filter(|line| overlap(surface, **line) > EPS)
        .flat_map(|line| {
            [
                line.y - surface.y,
                line.bottom() - surface.y,
                line.y - surface.bottom(),
                line.bottom() - surface.bottom(),
            ]
        })
        .filter(|dy| dy.abs() <= r.h / 2.0)
        .collect();
    slides.sort_by(|a, b| a.abs().total_cmp(&b.abs()));
    let moved = |dy: f64| Rect::new(surface.x, surface.y + dy, surface.w, surface.h);
    let whole = |s: Rect| !pieces.iter().any(|entry| cuts(s, *entry));
    // An edge through a line moves outward to that line's far edge, and a side that now runs
    // through an entry the place itself never met moves out past it, as far as the visible
    // figure goes.
    let grow = |mut surface: Rect| {
        for _ in 0..4 {
            let (mut top, mut bottom) = (surface.y, surface.bottom());
            for line in &lines {
                if overlap(surface, *line) > EPS {
                    top = top.min(line.y);
                    bottom = bottom.max(line.bottom());
                }
            }
            let (mut left, mut right) = (surface.x, surface.right());
            for entry in &pieces {
                if overlap(surface, *entry) > EPS {
                    left = left.min(entry.x.max(visible.x));
                    right = right.max(entry.right().min(visible.right()));
                }
            }
            let grown = Rect::new(left, top, right - left, bottom - top);
            if grown == surface {
                break;
            }
            surface = grown;
        }
        surface
    };
    let placed = |dy: f64, surface: Rect| Placed {
        rows: Rect::new(rows.x, rows.y + dy, rows.w, rows.h),
        surface,
        side,
    };
    // First the nearest slide to where both edges fall between lines; else the edges move out.
    let between = slides
        .iter()
        .copied()
        .filter(|_| through(surface))
        .find(|&dy| {
            let s = moved(dy);
            !through(s)
                && fits(s)
                && overlap(s, plot) <= overlap(surface, plot) + EPS
                && labels(s) <= labels(surface)
                && whole(s)
        })
        .unwrap_or(0.0);
    let grown = grow(moved(between));
    if fits(grown) && grown.w >= r.w {
        return placed(between, grown);
    }
    // The surface runs into the title or toolbar where it stands: a legend with the title row
    // right under it leaves no room for a readout that is taller. A searched place then slides
    // to where it fits, covering the labels it comes to lie on whole and no entry in part: an
    // axis under the readout is hidden, a legend entry cut in half reads as a collision. A dock
    // keeps off the axis labels and stays.
    if !docked {
        for &dy in &slides {
            let s = moved(dy);
            if !whole(s) || overlap(s, plot) > overlap(surface, plot) + EPS {
                continue;
            }
            let grown = grow(s);
            if fits(grown) && grown.w >= r.w && whole(grown) {
                return placed(dy, grown);
            }
        }
    }
    natural(r)
}

/// The tallest box that still counts as one line of text, for a surface's edges to fall between.
const LINE: f64 = 32.0;
/// What a place gives up, from what the readout keeps longest to what it gives up first.
const TIERS: usize = 11;
/// The tier that counts the numbers a place cuts (P1c).
const NUMBERS: usize = 2;

#[cfg(test)]
thread_local! {
    /// Score every place in full, as before the search was pruned (P11), to compare against.
    static REFERENCE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether this run scores every place in full; only tests ask for it.
fn reference() -> bool {
    #[cfg(test)]
    {
        REFERENCE.with(std::cell::Cell::get)
    }
    #[cfg(not(test))]
    {
        false
    }
}

/// The least-coverage search over the readout's room. It picks exactly the place a full scoring
/// of every place would, but reads far less: hovering a pie or a dense line scored thousands of
/// places against every wedge sample and every segment on each pointer move (P11, PRF-25).
struct Search<'s> {
    mark: &'s MarkShape<'s>,
    keep: &'s Keep<'s>,
    strong: Grid<'s>,
    others: Grid<'s>,
    numbers: Rows,
    text: Rows,
    lines: Vec<Line>,
    /// How near a label a place may come before it counts as touching it.
    gap: f64,
    /// On a wide chart, the bar being read and its value end: a place that does not stay with
    /// the bar gives up more than any that does.
    bar: Option<(Rect, Side)>,
}

/// How far from its bar a readout still reads as that bar's.
const REACH: f64 = 120.0;
/// How much farther a place below a phone's bar counts than one above it: about a fingertip.
const THUMB: f64 = 40.0;
/// What a place away from its bar costs among the marks that stepped back: more than any area.
const AWAY: f64 = 1e9;

/// The space between a bar and a readout at `r`: zero where they meet.
fn apart(bar: Rect, r: Rect) -> f64 {
    let dx = (bar.x - r.right()).max(r.x - bar.right()).max(0.0);
    let dy = (bar.y - r.bottom()).max(r.y - bar.bottom()).max(0.0);
    dx.hypot(dy)
}

/// Whether a readout at `r` stays with a bar whose value end is at `end`: in the bar's own
/// column, or row for a bar lying down, and within reach of it.
fn stays(bar: Rect, end: Side, r: Rect) -> bool {
    if matches!(end, Side::Up | Side::Down) {
        r.x < bar.right()
            && bar.x < r.right()
            && (bar.y - r.bottom()).max(r.y - bar.bottom()) <= REACH
    } else {
        r.y < bar.bottom()
            && bar.y < r.bottom()
            && (bar.x - r.right()).max(r.x - bar.right()) <= REACH
    }
}

/// One line of other marks, its segments with their bounding boxes, and whether its points run
/// left to right, so the segments a box can meet are found by binary search.
struct Line {
    segments: Vec<([f64; 2], [f64; 2], Rect)>,
    /// Each run of `BLOCK` segments' extent, left, top, right and bottom, taken from the
    /// segments' own box edges, so a run whose extent misses a box holds no segment that meets it.
    blocks: Vec<[f64; 4]>,
    xs: Option<Vec<f64>>,
}

/// The segments in one run of a line's index.
const BLOCK: usize = 16;

impl Line {
    fn new(points: &[[f64; 2]]) -> Self {
        let segments: Vec<_> = points
            .windows(2)
            .map(|s| {
                let (a, b) = (s[0], s[1]);
                let bounds = Rect::new(
                    a[0].min(b[0]),
                    a[1].min(b[1]),
                    (a[0] - b[0]).abs(),
                    (a[1] - b[1]).abs(),
                );
                (a, b, bounds)
            })
            .collect();
        let blocks = segments
            .chunks(BLOCK)
            .map(|run| {
                run.iter().fold(
                    [
                        f64::INFINITY,
                        f64::INFINITY,
                        f64::NEG_INFINITY,
                        f64::NEG_INFINITY,
                    ],
                    |e, (_, _, b)| {
                        [
                            e[0].min(b.x),
                            e[1].min(b.y),
                            e[2].max(b.right()),
                            e[3].max(b.bottom()),
                        ]
                    },
                )
            })
            .collect();
        let sorted = points.windows(2).all(|s| s[0][0] <= s[1][0]);
        Self {
            segments,
            blocks,
            xs: sorted.then(|| points.iter().map(|p| p[0]).collect()),
        }
    }
    /// The indices of the segments whose boxes may meet `r`, in order; a little wide, since a
    /// box's right edge is computed and can differ from the point's x in the last bit.
    fn near(&self, r: Rect) -> std::ops::Range<usize> {
        let Some(xs) = &self.xs else {
            return 0..self.segments.len();
        };
        let slack = 1e-6 * (1.0 + r.x.abs().max(r.right().abs()));
        let first = xs.partition_point(|x| *x < r.x - slack).saturating_sub(1);
        let last = xs
            .partition_point(|x| *x <= r.right() + slack)
            .min(self.segments.len());
        first.min(last)..last
    }
    /// Add the length of this line that `r` hides to `hidden`, segment by segment in order, and
    /// say whether `others` plus twice the total stays within `limit`. A segment whose box misses
    /// `r` is skipped, alone or with its whole run, as a full scoring skips it.
    fn hide(&self, r: Rect, others: f64, hidden: &mut f64, limit: f64) -> bool {
        let near = self.near(r);
        let mut i = near.start;
        while i < near.end {
            let end = ((i / BLOCK + 1) * BLOCK).min(near.end);
            let e = self.blocks[i / BLOCK];
            if e[0] <= r.right() && r.x <= e[2] && e[1] <= r.bottom() && r.y <= e[3] {
                for &(a, b, bounds) in &self.segments[i..end] {
                    if meets(bounds, r) {
                        *hidden += segment_length(a, b, r);
                        if others + LINE_WEIGHT * *hidden > limit {
                            return false;
                        }
                    }
                }
            }
            i = end;
        }
        true
    }
}

/// The boxes of many marks or texts in square cells, so a place reads only those it can meet.
/// Sums still run in the boxes' own order, so they come out exactly as a full scan's.
struct Grid<'s> {
    rects: &'s [Rect],
    origin: [f64; 2],
    cell: [f64; 2],
    size: [usize; 2],
    cells: Vec<Vec<u32>>,
    scratch: std::cell::RefCell<Vec<u32>>,
}

impl<'s> Grid<'s> {
    fn new(rects: &'s [Rect]) -> Self {
        let mut grid = Self {
            rects,
            origin: [0.0; 2],
            cell: [1.0; 2],
            size: [0; 2],
            cells: Vec::new(),
            scratch: std::cell::RefCell::new(Vec::new()),
        };
        let finite = rects
            .iter()
            .all(|r| r.x.is_finite() && r.y.is_finite() && r.w.is_finite() && r.h.is_finite());
        // A short list costs less to read whole than to index.
        if rects.len() <= 32 || !finite {
            return grid;
        }
        let x0 = rects.iter().map(|r| r.x).fold(f64::INFINITY, f64::min);
        let y0 = rects.iter().map(|r| r.y).fold(f64::INFINITY, f64::min);
        let x1 = rects
            .iter()
            .map(|r| r.right())
            .fold(f64::NEG_INFINITY, f64::max);
        let y1 = rects
            .iter()
            .map(|r| r.bottom())
            .fold(f64::NEG_INFINITY, f64::max);
        grid.origin = [x0, y0];
        grid.cell = [((x1 - x0) / 128.0).max(24.0), ((y1 - y0) / 128.0).max(24.0)];
        grid.size = [
            ((x1 - x0) / grid.cell[0]) as usize + 1,
            ((y1 - y0) / grid.cell[1]) as usize + 1,
        ];
        grid.cells = vec![Vec::new(); grid.size[0] * grid.size[1]];
        for (i, r) in rects.iter().enumerate() {
            let ([c0, c1], [r0, r1]) = grid.span(*r);
            for row in r0..=r1 {
                for column in c0..=c1 {
                    grid.cells[row * grid.size[0] + column].push(i as u32);
                }
            }
        }
        grid
    }
    /// The columns and rows of cells `r` reaches, clamped to the grid.
    fn span(&self, r: Rect) -> ([usize; 2], [usize; 2]) {
        let index = |v: f64, axis: usize| {
            (((v - self.origin[axis]) / self.cell[axis]).floor().max(0.0) as usize)
                .min(self.size[axis] - 1)
        };
        (
            [index(r.x, 0), index(r.right(), 0)],
            [index(r.y, 1), index(r.bottom(), 1)],
        )
    }
    /// The area of every box that `r` covers, summed in the boxes' order.
    fn area(&self, r: Rect) -> f64 {
        if self.cells.is_empty() {
            return self.rects.iter().map(|other| overlap(r, *other)).sum();
        }
        let mut near = self.scratch.borrow_mut();
        near.clear();
        let ([c0, c1], [r0, r1]) = self.span(r);
        for row in r0..=r1 {
            for column in c0..=c1 {
                near.extend(&self.cells[row * self.size[0] + column]);
            }
        }
        near.sort_unstable();
        near.dedup();
        // The boxes left out cover none of `r`; adding their zeros would change nothing.
        near.iter()
            .map(|&i| overlap(r, self.rects[i as usize]))
            .sum()
    }
}

/// Sort stably by a key reckoned once per item. The order is the one `sort_by` gives when it
/// compares the same keys, without reckoning both again for every comparison: a key is often a
/// `hypot`, a software `fma` in Wasm, and the fallback sorts thousands of places on every hover.
fn sort_by_key_once<T: Copy, K>(
    items: &mut [T],
    key: impl Fn(&T) -> K,
    order: impl Fn(&K, &K) -> std::cmp::Ordering,
) {
    let mut keyed: Vec<(K, T)> = items.iter().map(|item| (key(item), *item)).collect();
    keyed.sort_by(|a, b| order(&a.0, &b.0));
    for (slot, (_, item)) in items.iter_mut().zip(keyed) {
        *slot = item;
    }
}

fn area(rects: &[Rect], r: Rect) -> f64 {
    rects.iter().map(|other| overlap(r, *other)).sum()
}

/// Whether a segment's box meets `r`, edges included.
fn meets(bounds: Rect, r: Rect) -> bool {
    bounds.x <= r.right()
        && r.x <= bounds.right()
        && bounds.y <= r.bottom()
        && r.y <= bounds.bottom()
}

impl<'s> Search<'s> {
    fn new(
        mark: &'s MarkShape<'s>,
        keep: &'s Keep<'s>,
        gap: f64,
        bar: Option<(Rect, Side)>,
    ) -> Self {
        Self {
            mark,
            keep,
            strong: Grid::new(keep.strong),
            others: Grid::new(keep.others),
            numbers: Rows::new(keep.numbers),
            text: Rows::new(keep.text),
            lines: keep.lines.iter().map(|line| Line::new(line)).collect(),
            gap,
            bar,
        }
    }

    /// The whole area of each label a place stands beside, nearer than the gap, without
    /// touching it: the two would read as one phrase. It hides nothing, so it is the first
    /// thing a place gives up.
    fn beside(&self, r: Rect) -> f64 {
        if self.gap <= 0.0 {
            return 0.0;
        }
        let near = r.inflate(self.gap);
        self.keep
            .key
            .iter()
            .chain(self.text.level(near))
            .filter(|label| overlap(near, **label) > EPS && overlap(r, **label) <= EPS)
            .map(|label| label.w * label.h)
            .sum()
    }

    /// How many numbers a place cuts, and entries of a legend the readout stands in for: each
    /// is left clear or covered wholly. On `place_whole`'s second attempt (`Keep::across`) an
    /// entry counts only where the place's top or bottom edge runs through its line: one cut
    /// across its width is covered whole by the surface, which widens over it.
    fn cut(&self, r: Rect) -> f64 {
        // On a phone a shared readout can fit between the zero tick and the year labels even
        // when their font boxes touch. Allow half a pixel of leading at the top or bottom of a
        // number's box so the search can use that place and keep the legend entries whole.
        let covered_numbers = if self.keep.entries.is_empty() {
            r
        } else {
            Rect::new(r.x, r.y - 0.5, r.w, r.h + 1.0)
        };
        let numbers = self
            .numbers
            .level(r)
            .filter(|number| overlap(r, **number) > EPS && !number.inside(&covered_numbers, EPS));
        let across = self.keep.across;
        let entries = self.keep.entries.iter().filter(|entry| {
            if across {
                overlap(r, **entry) > EPS
                    && (entry.y < r.y - EPS || entry.bottom() > r.bottom() + EPS)
            } else {
                cuts(r, **entry)
            }
        });
        (numbers.count() + entries.count()) as f64
    }

    /// What a place adds to its cost in marks for standing away from the bar it reads.
    fn away(&self, r: Rect) -> f64 {
        match self.bar {
            Some((bar, end)) if !stays(bar, end, r) => AWAY,
            _ => 0.0,
        }
    }

    /// How the mark meets a place: clear of it and its halo, in the halo only, or on it.
    fn class(&self, r: Rect) -> u8 {
        if !touches(self.mark, r) {
            0
        } else if !intersects(self.mark, r) {
            1
        } else {
            2
        }
    }

    /// A tier's cost at `r`, or `None` once it is sure to exceed `limit`. Every cost is a sum of
    /// parts that are never negative, so a running sum past the limit settles it.
    fn cost(&self, tier: usize, r: Rect, limit: f64) -> Option<f64> {
        let keep = self.keep;
        Some(match tier {
            0 => area(keep.fixed, r),
            1 => match self.class(r) {
                class @ (0 | 1) => class as f64,
                // When every place hides some of the mark, hide as little of it as possible.
                _ => match self.mark {
                    MarkShape::Wedge(w) => 2.0 + wedge_hidden(w, r, limit - 2.0)?,
                    mark => 2.0 + covered(mark, r),
                },
            },
            // A cut number reads as another value (P1c). A label it touches costs its whole
            // area, so half a label never looks cheaper than all of it; a legend is a block of
            // entries and costs what is covered.
            2 => self.cut(r),
            3 => whole(keep.key, r),
            4 => whole(keep.subtitle, r),
            5 => area(keep.legend, r),
            6 => whole(keep.note, r),
            7 => whole(self.text.level(r), r),
            8 => self.strong.area(r),
            9 => {
                let others = self.away(r) + self.others.area(r);
                if others > limit {
                    return None;
                }
                let mut hidden = 0.0;
                for line in &self.lines {
                    if !line.hide(r, others, &mut hidden, limit) {
                        return None;
                    }
                }
                others + LINE_WEIGHT * hidden
            }
            _ => self.beside(r),
        })
    }

    /// The index of the first place, in order of preference, that gives up least.
    ///
    /// The places that cover more of the title and toolbar than the least any place covers, or
    /// meet the mark more closely than the closest any of those does, can never win; they are
    /// set aside after two cheap tests. The others are compared tier by tier, each cost reckoned
    /// only when the comparison reaches it and abandoned once it exceeds the best place's.
    fn rank(&self, places: &[(Rect, Side)]) -> usize {
        if reference() {
            return self.rank_in_full(places);
        }
        let first: Vec<(f64, u8)> = places
            .iter()
            .map(|&(r, _)| (area(self.keep.fixed, r), self.class(r)))
            .collect();
        let fixed = first.iter().map(|f| f.0).fold(f64::INFINITY, f64::min);
        let class = first
            .iter()
            .filter(|f| f.0 <= fixed + EPS)
            .map(|f| f.1)
            .min()
            .unwrap_or(0);
        let full = |slot: &mut Option<f64>, tier: usize, r: Rect| {
            *slot.get_or_insert_with(|| {
                self.cost(tier, r, f64::INFINITY)
                    .expect("no limit, so a cost")
            })
        };
        let mut best: Option<(usize, [Option<f64>; TIERS])> = None;
        'places: for (i, &(r, _)) in places.iter().enumerate() {
            if first[i].0 > fixed + EPS || first[i].1 != class {
                continue;
            }
            let mut costs = [None; TIERS];
            costs[0] = Some(first[i].0);
            if class < 2 {
                costs[1] = Some(class as f64);
            }
            if let Some((b, kept)) = &mut best {
                let at = places[*b].0;
                let mut better = false;
                for tier in 0..TIERS {
                    let bound = full(&mut kept[tier], tier, at);
                    let cost = match costs[tier] {
                        Some(cost) => cost,
                        None => match self.cost(tier, r, bound + EPS) {
                            Some(cost) => *costs[tier].insert(cost),
                            None => continue 'places,
                        },
                    };
                    if cost > bound + EPS {
                        continue 'places;
                    }
                    if cost < bound - EPS {
                        better = true;
                        break;
                    }
                }
                if !better {
                    continue;
                }
            }
            best = Some((i, costs));
            // Places are in order of preference, so one that gives up nothing is the answer.
            if let Some((_, kept)) = &mut best
                && (0..TIERS).all(|tier| full(&mut kept[tier], tier, r) <= EPS)
            {
                break;
            }
        }
        best.map_or(0, |(i, _)| i)
    }

    /// The same choice made by scoring every place in every tier, as before P11.
    fn rank_in_full(&self, places: &[(Rect, Side)]) -> usize {
        let segments: Vec<_> = self.lines.iter().flat_map(|line| &line.segments).collect();
        let keep = self.keep;
        let cost = |tier: usize, r: Rect| match tier {
            0 => area(keep.fixed, r),
            1 if !touches(self.mark, r) => 0.0,
            1 if !intersects(self.mark, r) => 1.0,
            1 => 2.0 + covered(self.mark, r),
            2 => self.cut(r),
            3 => whole(keep.key, r),
            4 => whole(keep.subtitle, r),
            5 => area(keep.legend, r),
            6 => whole(keep.note, r),
            7 => whole(self.text.level(r), r),
            8 => area(keep.strong, r),
            9 => {
                self.away(r)
                    + area(keep.others, r)
                    + LINE_WEIGHT
                        * segments
                            .iter()
                            .filter(|(_, _, bounds)| meets(*bounds, r))
                            .map(|&&(a, b, _)| segment_length(a, b, r))
                            .sum::<f64>()
            }
            _ => self.beside(r),
        };
        let mut best: Option<(usize, [f64; TIERS])> = None;
        'places: for (i, &(r, _)) in places.iter().enumerate() {
            let mut costs = [0.0; TIERS];
            let mut better = best.is_none();
            for tier in 0..TIERS {
                costs[tier] = cost(tier, r);
                if let Some((_, kept)) = &best
                    && !better
                {
                    if costs[tier] > kept[tier] + EPS {
                        continue 'places;
                    }
                    better = costs[tier] < kept[tier] - EPS;
                }
            }
            if better {
                best = Some((i, costs));
                if costs.iter().all(|cost| *cost <= EPS) {
                    break;
                }
            }
        }
        best.map_or(0, |(i, _)| i)
    }
}

/// The part of a sector a readout at `r` hides, sampled on a 2 px grid as `covered` does, or
/// `None` once it passes `limit`.
fn wedge_hidden(w: &Wedge, r: Rect, limit: f64) -> Option<f64> {
    let mut hidden = 0.0;
    let mut y = r.y + 1.0;
    while y < r.bottom() {
        let dy = y - w.center[1];
        // A row wholly outside the ring hides nothing.
        if dy.abs() <= w.radius {
            let mut x = r.x + 1.0;
            while x < r.right() {
                let dx = x - w.center[0];
                let radius = dx.hypot(dy);
                if radius >= w.inner && radius <= w.radius {
                    let angle = (dx.atan2(-dy) - w.start).rem_euclid(std::f64::consts::TAU);
                    if angle <= w.end - w.start {
                        hidden += 4.0;
                        if hidden > limit {
                            return None;
                        }
                    }
                }
                x += 2.0;
            }
        }
        y += 2.0;
    }
    Some(hidden)
}

/// The readout in the half of a gauge's hollow away from its needle, as near the hub as it
/// clears everything `valid` asks of it.
fn in_hollow(hollow: &Hollow, size: [f64; 2], valid: impl Fn(Rect) -> bool) -> Option<Rect> {
    let [cx, cy] = hollow.center;
    let left = hollow.needle[0] >= cx;
    let steps = (hollow.radius / 4.0).ceil() as usize;
    let mut spots: Vec<Rect> = (0..=steps)
        .flat_map(|i| {
            let x = if left {
                cx - i as f64 * 4.0 - size[0]
            } else {
                cx + i as f64 * 4.0
            };
            (0..=2 * steps)
                .map(move |j| Rect::new(x, cy - hollow.radius + j as f64 * 4.0, size[0], size[1]))
        })
        .filter(|r| {
            [
                [r.x, r.y],
                [r.right(), r.y],
                [r.x, r.bottom()],
                [r.right(), r.bottom()],
            ]
            .iter()
            .all(|p| (p[0] - cx).hypot(p[1] - cy) <= hollow.radius)
        })
        .collect();
    let distance = |r: &Rect| {
        let [x, y] = r.center();
        (x - cx).hypot(y - cy)
    };
    sort_by_key_once(&mut spots, distance, f64::total_cmp);
    spots.into_iter().find(|r| valid(*r))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn keep<'a>(others: &'a [Rect], text: &'a [Rect], fixed: &'a [Rect]) -> Keep<'a> {
        Keep {
            fixed,
            text,
            others,
            ..Keep::default()
        }
    }
    fn rect(r: Rect) -> MarkShape<'static> {
        MarkShape::Rect {
            rect: r,
            end: Some(Side::Up),
        }
    }
    /// A mark with no value end, such as a tile: its readout never looks for free paper.
    fn tile(r: Rect) -> MarkShape<'static> {
        MarkShape::Rect { rect: r, end: None }
    }
    fn sample(mark: MarkShape<'_>, labels: &[Rect], previous: Option<Side>) -> (Rect, Side) {
        place(
            mark,
            [80.0, 40.0],
            [400.0, 300.0],
            Rect::new(30.0, 30.0, 340.0, 240.0),
            Rect::new(0.0, 0.0, 400.0, 300.0),
            &keep(&[], labels, &[]),
            false,
            previous,
        )
    }
    #[test]
    fn past_value_end_then_beside_and_opposite() {
        let mark = Rect::new(180.0, 130.0, 30.0, 80.0);
        assert_eq!(sample(rect(mark), &[], None).1, Side::Up);
        assert_eq!(
            sample(rect(mark), &[Rect::new(140.0, 80.0, 110.0, 50.0)], None).1,
            Side::Right
        );
        assert_eq!(
            sample(
                rect(mark),
                &[
                    Rect::new(140.0, 80.0, 110.0, 50.0),
                    Rect::new(220.0, 100.0, 100.0, 110.0),
                    Rect::new(70.0, 100.0, 100.0, 110.0)
                ],
                None
            )
            .1,
            Side::Down
        );
    }
    #[test]
    fn point_readout_clears_its_radius_and_halo() {
        let (r, side) = sample(
            MarkShape::Point {
                center: [200.0, 150.0],
                radius: 10.0,
            },
            &[],
            None,
        );
        assert_eq!(side, Side::Up);
        assert!(r.bottom() <= 150.0 - 10.0 - 4.0);
    }
    #[test]
    fn composite_mark_protects_each_part_without_filling_the_space_between() {
        let bar = Rect::new(100.0, 100.0, 40.0, 20.0);
        let point = Rect::new(190.0, 106.0, 8.0, 8.0);
        let parts = [bar, point];
        let mark = MarkShape::Composite {
            anchor: bar,
            parts: &parts,
            end: Some(Side::Right),
        };
        assert!(intersects(&mark, Rect::new(130.0, 105.0, 20.0, 10.0)));
        assert!(intersects(&mark, Rect::new(185.0, 105.0, 20.0, 10.0)));
        assert!(!intersects(&mark, Rect::new(150.0, 105.0, 20.0, 10.0)));
        let (readout, _) = sample(mark, &[], None);
        assert!(!readout.overlaps(&bar, 0.0));
        assert!(!readout.overlaps(&point, 0.0));
    }
    #[test]
    fn ribbon_collision_follows_its_polygon_not_its_bounding_box() {
        let paths = vec![vec![
            [0.0, 0.0],
            [100.0, 100.0],
            [100.0, 110.0],
            [0.0, 10.0],
            [0.0, 0.0],
        ]];
        let mark = MarkShape::Polygon {
            anchor: Rect::new(0.0, 0.0, 100.0, 110.0),
            paths: &paths,
            parts: &[],
            edge_slack: 0.05,
            hollow: None,
        };
        assert!(intersects(&mark, Rect::new(45.0, 48.0, 10.0, 10.0)));
        assert!(!intersects(&mark, Rect::new(5.0, 80.0, 10.0, 10.0)));
        assert!(intersects(&mark, Rect::new(-1.0, -1.0, 102.0, 112.0)));
    }
    #[test]
    fn curved_mark_also_protects_its_separate_hub() {
        let paths = vec![vec![
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 10.0],
            [0.0, 10.0],
            [0.0, 0.0],
        ]];
        let parts = [Rect::new(45.0, 45.0, 10.0, 10.0)];
        let mark = MarkShape::Polygon {
            anchor: Rect::new(0.0, 0.0, 60.0, 60.0),
            paths: &paths,
            parts: &parts,
            edge_slack: 0.8,
            hollow: None,
        };
        assert!(intersects(&mark, Rect::new(40.0, 40.0, 10.0, 10.0)));
        assert!(!intersects(&mark, Rect::new(20.0, 20.0, 10.0, 10.0)));
    }
    #[test]
    fn fallback_finds_a_clear_gap_narrower_than_its_grid() {
        let mark = MarkShape::Point {
            center: [156.2, 60.0],
            radius: 6.0,
        };
        let protected = [
            Rect::new(0.0, 0.0, 50.0, 120.0),
            Rect::new(0.0, 0.0, 200.0, 20.0),
            Rect::new(0.0, 100.0, 200.0, 20.0),
        ];
        let (readout, _) = place(
            mark,
            [100.0, 40.0],
            [200.0, 120.0],
            Rect::new(0.0, 0.0, 200.0, 120.0),
            Rect::new(0.0, 0.0, 200.0, 120.0),
            &keep(&[], &[], &protected),
            true,
            None,
        );
        assert!(!intersects(&mark, readout));
        assert!(protected.iter().all(|text| !text.overlaps(&readout, 0.0)));
        assert!(readout.x > 50.0 && readout.x < 50.2);
    }
    #[test]
    fn previous_side_stays_only_while_clear() {
        let mark = Rect::new(180.0, 130.0, 30.0, 80.0);
        assert_eq!(sample(rect(mark), &[], Some(Side::Left)).1, Side::Left);
        assert_eq!(
            sample(
                rect(mark),
                &[Rect::new(70.0, 150.0, 100.0, 40.0)],
                Some(Side::Left)
            )
            .1,
            Side::Up
        );
    }
    #[test]
    fn shared_line_prefers_clear_plot_above_series() {
        let dots = [[200.0, 220.0]];
        let lines = vec![vec![[40.0, 200.0], [360.0, 200.0]]];
        let (r, side) = sample(
            MarkShape::NearestX {
                x: 200.0,
                dots: &dots,
                lines: &lines,
            },
            &[],
            None,
        );
        assert_eq!(side, Side::Right);
        assert!(r.bottom() <= 200.0 || r.y >= 200.0);
    }
    #[test]
    fn shared_line_dots_keep_their_visible_radius_and_halo_clear() {
        let dots = [[97.0, 125.0]];
        let mark = MarkShape::NearestX {
            x: 97.0,
            dots: &dots,
            lines: &[],
        };
        let readout = Rect::new(100.0, 100.0, 50.0, 50.0);
        assert!(intersects(&mark, readout));
        let dots = [[91.0, 125.0]];
        let mark = MarkShape::NearestX {
            x: 91.0,
            dots: &dots,
            lines: &[],
        };
        assert!(!intersects(&mark, readout));
        assert!(touches(&mark, readout));
    }
    #[test]
    fn narrow_readout_docks_below_plot_when_visible() {
        let plot = Rect::new(30.0, 30.0, 300.0, 150.0);
        let (r, side) = place(
            rect(Rect::new(140.0, 80.0, 30.0, 70.0)),
            [100.0, 40.0],
            [360.0, 280.0],
            plot,
            Rect::new(0.0, 0.0, 360.0, 280.0),
            &keep(&[], &[], &[]),
            true,
            None,
        );
        assert_eq!(side, Side::Bottom);
        assert_eq!(side.narrow_placement(), "dock-below");
        assert_eq!(r.x, plot.x);
        assert!(r.y >= plot.bottom());
    }
    #[test]
    fn narrow_plot_band_respects_inset_when_plot_reaches_figure_edge() {
        let plot = Rect::new(0.0, 40.0, 300.0, 180.0);
        let (r, side) = place(
            tile(Rect::new(120.0, 170.0, 20.0, 30.0)),
            [80.0, 40.0],
            [300.0, 260.0],
            plot,
            Rect::new(0.0, 0.0, 300.0, 260.0),
            &keep(&[], &[], &[]),
            true,
            None,
        );
        assert_eq!(side, Side::BandTop);
        assert_eq!(r.x, 8.0);
        assert!(!side.is_fallback());
    }
    #[test]
    fn narrow_fallback_can_use_clear_figure_space_above_the_canvas() {
        let protected = Rect::new(0.0, -45.0, 300.0, 305.0);
        let (r, side) = place(
            tile(Rect::new(120.0, 60.0, 40.0, 60.0)),
            [80.0, 40.0],
            [300.0, 200.0],
            Rect::new(0.0, 0.0, 300.0, 200.0),
            Rect::new(0.0, -100.0, 300.0, 360.0),
            &keep(&[], &[], &[protected]),
            true,
            None,
        );
        assert!(side.is_fallback());
        assert_eq!(r.x, 8.0);
        assert!(r.bottom() <= protected.y);
    }
    #[test]
    fn narrow_fallback_can_move_sideways_to_avoid_a_mark_and_protected_text() {
        let protected = [
            Rect::new(0.0, -100.0, 300.0, 80.0),
            Rect::new(0.0, 160.0, 300.0, 100.0),
            Rect::new(0.0, -20.0, 80.0, 180.0),
        ];
        let mark = MarkShape::Point {
            center: [110.0, 100.0],
            radius: 4.0,
        };
        let (r, side) = place(
            mark,
            [150.0, 80.0],
            [300.0, 200.0],
            Rect::new(0.0, 0.0, 300.0, 200.0),
            Rect::new(0.0, -100.0, 300.0, 360.0),
            &keep(&[], &[], &protected),
            true,
            None,
        );
        assert!(side.is_fallback());
        assert!(r.x > 8.0);
        assert!(!touches(&mark, r));
        assert!(protected.iter().all(|text| !text.overlaps(&r, 0.0)));
    }
    #[test]
    fn narrow_oversized_plot_band_is_classified_as_fallback() {
        let (_, side) = place(
            rect(Rect::new(120.0, 90.0, 40.0, 40.0)),
            [80.0, 140.0],
            [300.0, 240.0],
            Rect::new(0.0, 70.0, 300.0, 100.0),
            Rect::new(0.0, 0.0, 300.0, 240.0),
            &keep(&[], &[], &[]),
            true,
            None,
        );
        assert!(side.is_fallback());
        assert_eq!(side.narrow_placement(), "fallback");
    }
    #[test]
    fn fallback_keeps_the_mark_clear_before_other_coverage_when_halo_cannot_fit() {
        let mark = rect(Rect::new(150.0, 0.0, 150.0, 200.0));
        let (r, side) = place(
            mark,
            [140.0, 80.0],
            [300.0, 200.0],
            Rect::new(0.0, 0.0, 300.0, 200.0),
            Rect::new(0.0, 0.0, 300.0, 200.0),
            &keep(&[Rect::new(0.0, 0.0, 149.0, 200.0)], &[], &[]),
            true,
            None,
        );
        assert!(side.is_fallback());
        assert!(!intersects(&mark, r));
        assert!(touches(&mark, r));
    }
    #[test]
    fn fallback_clears_the_selected_mark_before_avoiding_labels() {
        let mark = Rect::new(180.0, 130.0, 30.0, 80.0);
        let (r, side) = sample(rect(mark), &[Rect::new(0.0, 0.0, 400.0, 300.0)], None);
        assert!(side.is_fallback());
        assert!(!touches(&rect(mark), r));
    }
    #[test]
    fn narrow_dock_follows_bottom_axis_labels() {
        let plot = Rect::new(30.0, 30.0, 300.0, 150.0);
        let axis = Rect::new(40.0, 180.0, 80.0, 16.0);
        let (r, side) = place(
            rect(Rect::new(140.0, 80.0, 30.0, 70.0)),
            [100.0, 40.0],
            [360.0, 280.0],
            plot,
            Rect::new(0.0, 0.0, 360.0, 280.0),
            &keep(&[], &[axis], &[axis]),
            true,
            None,
        );
        assert_eq!(side, Side::Bottom);
        assert_eq!(r.y, axis.bottom());
    }
    #[test]
    fn narrow_dock_skips_a_legend_and_uses_the_upper_space() {
        let plot = Rect::new(30.0, 100.0, 300.0, 140.0);
        let legend = Rect::new(30.0, 240.0, 300.0, 50.0);
        let (r, side) = place(
            rect(Rect::new(140.0, 130.0, 30.0, 70.0)),
            [100.0, 40.0],
            [360.0, 320.0],
            plot,
            Rect::new(0.0, 0.0, 360.0, 320.0),
            &keep(&[], &[], &[legend]),
            true,
            None,
        );
        assert_eq!(side, Side::Top);
        assert_eq!(r.bottom(), plot.y);
    }
    #[test]
    fn narrow_readout_uses_a_plot_band_when_docks_do_not_fit() {
        let plot = Rect::new(30.0, 30.0, 300.0, 220.0);
        let (r, side) = place(
            tile(Rect::new(140.0, 80.0, 30.0, 70.0)),
            [100.0, 60.0],
            [360.0, 280.0],
            plot,
            Rect::new(0.0, 0.0, 360.0, 280.0),
            &keep(&[], &[], &[]),
            true,
            None,
        );
        assert!(matches!(side, Side::BandTop | Side::BandBottom));
        assert!(r.inside(&plot, 0.0));
    }
    #[test]
    fn wedge_fallback_stays_in_the_room_and_hides_the_least_of_the_wedge() {
        // The wedge fills the chart and the visible figure reaches well past it, but a wide
        // chart's readout stays in its room, the visible chart 8 px in (f614f4ec).
        let wedge = Wedge {
            center: [100.0, 100.0],
            radius: 90.0,
            inner: 0.0,
            start: 0.0,
            end: std::f64::consts::TAU,
        };
        let ribbon = Rect::new(70.0, 70.0, 20.0, 60.0);
        let (r, _) = place(
            MarkShape::Wedge(wedge),
            [80.0, 80.0],
            [200.0, 200.0],
            Rect::new(0.0, 0.0, 200.0, 200.0),
            Rect::new(-70.0, -70.0, 340.0, 340.0),
            &keep(&[ribbon], &[], &[]),
            false,
            None,
        );
        assert!(r.inside(&Rect::new(8.0, 8.0, 184.0, 184.0), 0.0), "{r:?}");
        // A corner of the room hides the least of the wedge; the two clear of the ribbon win.
        assert!(r.x >= 111.5 && (r.y <= 8.5 || r.y >= 111.5), "{r:?}");
        assert_eq!(overlap(r, ribbon), 0.0);
    }
    #[test]
    fn protected_figure_text_is_avoided_in_fallback() {
        let mark = Rect::new(180.0, 130.0, 30.0, 80.0);
        let title = Rect::new(0.0, 0.0, 400.0, 100.0);
        let (r, _) = place(
            rect(mark),
            [80.0, 40.0],
            [400.0, 300.0],
            Rect::new(30.0, 30.0, 340.0, 240.0),
            Rect::new(0.0, 0.0, 400.0, 300.0),
            &keep(&[], &[Rect::new(0.0, 100.0, 400.0, 200.0)], &[title]),
            false,
            None,
        );
        assert!(!r.overlaps(&title, 0.0));
    }
    #[test]
    fn wedge_value_end_is_near_its_bisector() {
        let w = Wedge {
            center: [200.0, 150.0],
            radius: 50.0,
            inner: 0.0,
            start: 0.0,
            end: std::f64::consts::FRAC_PI_2,
        };
        let (r, _) = sample(MarkShape::Wedge(w), &[], None);
        assert!(r.x > w.center[0] && r.y < w.center[1], "{r:?}");
    }

    #[derive(Clone, Copy, Debug)]
    enum Kind {
        Line,
        Other,
        Text,
        Note,
        Legend,
        Subtitle,
        Mark,
        Fixed,
    }

    /// Put `first` in the left half and `last` in the right half of a figure where a readout
    /// fits only by covering one of them, and return where it went.
    fn give_up(first: Kind, last: Kind) -> (Rect, Rect, Rect, [f64; 2]) {
        let left = Rect::new(20.0, 20.0, 60.0, 60.0);
        let right = Rect::new(136.0, 20.0, 60.0, 60.0);
        let mut lists: [Vec<Rect>; 6] = Default::default();
        let mut lines = Vec::new();
        let mut center = [-50.0, -50.0];
        for (kind, r) in [(first, left), (last, right)] {
            match kind {
                Kind::Fixed => lists[0].push(r),
                Kind::Subtitle => lists[1].push(r),
                Kind::Legend => lists[2].push(r),
                Kind::Note => lists[3].push(r),
                Kind::Text => lists[4].push(r),
                Kind::Other => lists[5].push(r),
                Kind::Line => lines.push(vec![[r.x, r.y], [r.right(), r.bottom()]]),
                Kind::Mark => center = r.center(),
            }
        }
        let keep = Keep {
            fixed: &lists[0],
            key: &[],
            subtitle: &lists[1],
            legend: &lists[2],
            note: &lists[3],
            text: &lists[4],
            others: &lists[5],
            strong: &[],
            lines: &lines,
            numbers: &[],
            entries: &[],
            ..Keep::default()
        };
        let figure = Rect::new(0.0, 0.0, 216.0, 116.0);
        let (r, side) = place(
            MarkShape::Point {
                center,
                radius: 6.0,
            },
            [100.0, 100.0],
            [216.0, 116.0],
            figure,
            figure,
            &keep,
            true,
            None,
        );
        assert!(side.is_fallback(), "{first:?} before {last:?}: {side:?}");
        (r, left, right, center)
    }

    fn covered(kind: Kind, r: Rect, readout: Rect, center: [f64; 2]) -> bool {
        match kind {
            Kind::Mark => circle_intersects(center, 6.0, readout),
            Kind::Line => segment_length([r.x, r.y], [r.right(), r.bottom()], readout) > 0.0,
            _ => readout.overlaps(&r, 0.0),
        }
    }

    #[test]
    fn when_nothing_clears_it_gives_things_up_in_order() {
        let order = [
            Kind::Other,
            Kind::Text,
            Kind::Note,
            Kind::Legend,
            Kind::Subtitle,
            Kind::Mark,
            Kind::Fixed,
        ];
        for pair in order.windows(2) {
            let (readout, left, right, center) = give_up(pair[0], pair[1]);
            assert!(
                covered(pair[0], left, readout, center),
                "{pair:?}: {readout:?}"
            );
            assert!(
                !covered(pair[1], right, readout, center),
                "{pair:?}: {readout:?}"
            );
        }
        // Lines count with other marks, by the length hidden.
        let (readout, left, right, center) = give_up(Kind::Line, Kind::Text);
        assert!(covered(Kind::Line, left, readout, center));
        assert!(!covered(Kind::Text, right, readout, center));
    }

    #[test]
    fn narrow_docks_leave_the_subtitle_clear_and_may_cover_the_note() {
        let plot = Rect::new(30.0, 120.0, 300.0, 120.0);
        let subtitle = [Rect::new(30.0, 60.0, 300.0, 40.0)];
        let note = [Rect::new(30.0, 240.0, 300.0, 60.0)];
        let mark = rect(Rect::new(140.0, 150.0, 30.0, 70.0));
        // The lower dock covers the note.
        let (r, side) = place(
            mark,
            [100.0, 50.0],
            [360.0, 320.0],
            plot,
            Rect::new(0.0, 0.0, 360.0, 320.0),
            &Keep {
                subtitle: &subtitle,
                note: &note,
                ..Keep::default()
            },
            true,
            None,
        );
        assert_eq!(side, Side::Bottom);
        assert!(r.overlaps(&note[0], 0.0));
        // Without room below, the upper dock would cover the subtitle, so it is not taken.
        let (r, side) = place(
            mark,
            [100.0, 50.0],
            [360.0, 260.0],
            plot,
            Rect::new(0.0, 0.0, 360.0, 260.0),
            &Keep {
                subtitle: &subtitle,
                ..Keep::default()
            },
            true,
            None,
        );
        assert_ne!(side, Side::Top);
        assert!(!r.overlaps(&subtitle[0], 0.0));
    }

    #[test]
    fn narrow_dock_covers_a_legend_left_out_for_a_standing_in_readout() {
        let plot = Rect::new(30.0, 100.0, 300.0, 140.0);
        let legend = Rect::new(30.0, 50.0, 300.0, 50.0);
        let mark = rect(Rect::new(140.0, 130.0, 30.0, 70.0));
        let figure = Rect::new(0.0, 0.0, 360.0, 260.0);
        let (r, side) = place(
            mark,
            [100.0, 60.0],
            [360.0, 260.0],
            plot,
            figure,
            &Keep::default(),
            true,
            None,
        );
        assert_eq!(side, Side::Top);
        assert!(r.overlaps(&legend, 0.0));
        let (r, side) = place(
            mark,
            [100.0, 60.0],
            [360.0, 260.0],
            plot,
            figure,
            &keep(&[], &[], &[legend]),
            true,
            None,
        );
        assert_ne!(side, Side::Top);
        assert!(!r.overlaps(&legend, 0.0));
    }

    #[test]
    fn a_gauge_key_is_given_up_after_the_subtitle_and_before_the_mark() {
        let figure = Rect::new(0.0, 0.0, 200.0, 120.0);
        // Every place 8 px inside the figure covers either the mark above, with its 4 px halo,
        // or the key below: the key goes.
        let key = [Rect::new(0.0, 64.0, 200.0, 56.0)];
        let mark = rect(Rect::new(0.0, 0.0, 200.0, 60.0));
        let (r, _) = place(
            mark,
            [100.0, 40.0],
            [200.0, 120.0],
            figure,
            figure,
            &Keep {
                key: &key,
                ..Keep::default()
            },
            false,
            None,
        );
        assert!(!r.overlaps(&Rect::new(0.0, 0.0, 200.0, 64.0), 0.0), "{r:?}");
        assert!(r.overlaps(&key[0], 0.0));
        // Every place covers either the key above or the subtitle below: the subtitle goes.
        let key = [Rect::new(0.0, 0.0, 200.0, 30.0)];
        let subtitle = [Rect::new(0.0, 70.0, 200.0, 50.0)];
        let mark = MarkShape::Point {
            center: [4.0, 50.0],
            radius: 2.0,
        };
        let (r, _) = place(
            mark,
            [150.0, 50.0],
            [200.0, 120.0],
            figure,
            figure,
            &Keep {
                key: &key,
                subtitle: &subtitle,
                ..Keep::default()
            },
            false,
            None,
        );
        assert!(!r.overlaps(&key[0], 0.0), "{r:?}");
        assert!(r.overlaps(&subtitle[0], 0.0));
    }

    /// A gauge: a 270-degree ring around (200, 150), radii 80 and 100, its needle at `needle`
    /// radians clockwise from up, a hub and the value and its name under the hub.
    fn gauge(needle: f64, size: [f64; 2]) -> (Rect, Side, Vec<Rect>, f64, bool) {
        let polar = |r: f64, a: f64| [200.0 + r * a.sin(), 150.0 - r * a.cos()];
        let (start, sweep) = (-0.75 * std::f64::consts::PI, 1.5 * std::f64::consts::PI);
        let ring: Vec<_> = (0..=60)
            .map(|i| polar(100.0, start + sweep * i as f64 / 60.0))
            .chain(
                (0..=60)
                    .rev()
                    .map(|i| polar(80.0, start + sweep * i as f64 / 60.0)),
            )
            .collect();
        let tip = polar(78.0, needle);
        let paths = vec![
            ring,
            vec![tip, polar(5.0, needle + 1.57), polar(5.0, needle - 1.57)],
        ];
        let parts = [Rect::new(192.0, 142.0, 16.0, 16.0)];
        let text = vec![
            Rect::new(180.0, 170.0, 40.0, 28.0),
            Rect::new(165.0, 200.0, 70.0, 14.0),
        ];
        let mark = MarkShape::Polygon {
            anchor: Rect::new(100.0, 50.0, 200.0, 171.0),
            paths: &paths,
            parts: &parts,
            edge_slack: 0.8,
            hollow: Some(Hollow {
                center: [200.0, 150.0],
                radius: 80.0,
                needle: tip,
            }),
        };
        let figure = Rect::new(0.0, 0.0, 400.0, 300.0);
        let (r, side) = place(
            mark,
            size,
            [400.0, 300.0],
            figure,
            figure,
            &keep(&[], &text, &[]),
            false,
            None,
        );
        (r, side, text, tip[0], touches(&mark, r))
    }

    #[test]
    fn gauge_readout_sits_in_the_hollow_away_from_its_needle() {
        for needle in [1.0, -1.0, 0.0] {
            let (r, side, text, tip, touching) = gauge(needle, [54.0, 36.0]);
            assert_eq!(side, Side::Hollow);
            assert!(!touching, "{r:?}");
            assert!(text.iter().all(|t| !t.overlaps(&r, 0.0)), "{r:?}");
            assert!(
                if tip >= 200.0 {
                    r.right() <= 200.0
                } else {
                    r.x >= 200.0
                },
                "{r:?}"
            );
            for corner in [
                [r.x, r.y],
                [r.right(), r.y],
                [r.x, r.bottom()],
                [r.right(), r.bottom()],
            ] {
                assert!((corner[0] - 200.0).hypot(corner[1] - 150.0) <= 80.0);
            }
        }
        // A readout too large for the hollow tries the usual places instead.
        let (_, side, ..) = gauge(1.0, [120.0, 90.0]);
        assert_ne!(side, Side::Hollow);
    }

    #[test]
    fn clipped_area_measures_a_concave_path_inside_a_box() {
        // An L shape of area 3, two units of it inside the box.
        let l = [
            [0.0, 0.0],
            [2.0, 0.0],
            [2.0, 1.0],
            [1.0, 1.0],
            [1.0, 2.0],
            [0.0, 2.0],
        ];
        assert!((clipped_area(&l, Rect::new(0.0, 0.0, 2.0, 2.0)) - 3.0).abs() < 1e-9);
        assert!((clipped_area(&l, Rect::new(0.0, 0.0, 2.0, 1.0)) - 2.0).abs() < 1e-9);
        assert!((clipped_area(&l, Rect::new(1.5, 1.5, 1.0, 1.0))).abs() < 1e-9);
    }

    #[test]
    fn a_mark_that_cannot_be_cleared_is_hidden_as_little_as_possible() {
        // A wide bar fills the figure but for a sliver; the readout covers the least of it.
        let mark = rect(Rect::new(8.0, 30.0, 200.0, 86.0));
        let figure = Rect::new(0.0, 0.0, 216.0, 124.0);
        let (r, side) = place(
            mark,
            [100.0, 40.0],
            [216.0, 124.0],
            figure,
            figure,
            &Keep::default(),
            true,
            None,
        );
        assert!(side.is_fallback());
        assert!(r.y <= 8.0 + 0.01, "{r:?}");
    }

    /// Run `f` with every place scored in full, as before the search was pruned.
    fn in_full<T>(f: impl FnOnce() -> T) -> T {
        REFERENCE.with(|reference| reference.set(true));
        let result = f();
        REFERENCE.with(|reference| reference.set(false));
        result
    }

    #[test]
    fn line_height_matches_full_scoring() {
        // Three random walks of 2,000 points, a staircase of flat and upright segments, and a
        // line that climbs in short upright runs, under boxes of three sizes across the plot.
        let plot = Rect::new(60.0, 40.0, 1180.0, 420.0);
        let mut seed = 986_u64;
        let mut random = move || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as f64 / 2147483648.0
        };
        let mut lines: Vec<Vec<[f64; 2]>> = (0..3)
            .map(|s| {
                let mut v = 50.0 + 10.0 * s as f64;
                (0..2000)
                    .map(|i| {
                        v += (random() - 0.5) * 2.0;
                        let y = plot.y + plot.h * (1.0 - (v - 20.0) / 80.0);
                        [plot.x + plot.w * i as f64 / 1999.0, y]
                    })
                    .collect()
            })
            .collect();
        lines.push(
            (0..400)
                .flat_map(|i| {
                    let x = plot.x + 2.95 * i as f64;
                    let y = plot.y + 30.0 * (i % 13) as f64;
                    [[x, y], [x + 2.95, y]]
                })
                .collect(),
        );
        lines.push(
            (0..900)
                .map(|i| [plot.x + (i / 3) as f64 * 3.9, 250.0 + (i % 7) as f64])
                .collect(),
        );
        let dots = [[700.0, 250.0]];
        for i in 0..60 {
            let x = plot.x - 100.0 + 22.0 * i as f64;
            for size in [[180.0, 96.0], [64.0, 30.0], [420.0, 300.0]] {
                let fast = line_y(x, size, plot, &dots, &lines);
                let full = in_full(|| line_y(x, size, plot, &dots, &lines));
                assert_eq!(fast.to_bits(), full.to_bits(), "x {x}, size {size:?}");
            }
        }
    }

    #[test]
    fn pruned_search_matches_full_scoring_on_a_dense_line() {
        // A 5,000-segment shared line under a readout too wide for either side of the cursor.
        let plot = Rect::new(40.0, 20.0, 520.0, 300.0);
        let y_at = |x: f64| {
            let t = (x - plot.x) / plot.w * 5000.0;
            plot.y + plot.h * (0.5 + 0.4 * (t * 0.013).sin() * (t * 0.0007).cos())
        };
        let line: Vec<[f64; 2]> = (0..=5000)
            .map(|i| plot.x + plot.w * i as f64 / 5000.0)
            .map(|x| [x, y_at(x)])
            .collect();
        let lines = vec![line];
        let ticks: Vec<Rect> = (0..12)
            .map(|i| Rect::new(4.0, plot.y + 25.0 * i as f64, 30.0, 12.0))
            .collect();
        let keep = Keep {
            text: &ticks,
            lines: &lines,
            ..Keep::default()
        };
        for x in [200.0, 260.0, 333.0, 400.0] {
            let dots = [[x, y_at(x)]];
            let run = || {
                place(
                    MarkShape::NearestX {
                        x,
                        dots: &dots,
                        lines: &lines,
                    },
                    [400.0, 120.0],
                    [600.0, 340.0],
                    plot,
                    Rect::new(0.0, 0.0, 600.0, 340.0),
                    &keep,
                    false,
                    None,
                )
            };
            let fast = run();
            assert!(fast.1.is_fallback(), "x {x}: {fast:?}");
            assert_eq!(fast, in_full(run), "x {x}");
        }
    }

    #[test]
    fn pruned_search_matches_full_scoring_on_pie_wedges() {
        // A pie filling its chart, under a readout no side of the slice can hold; the last slice
        // is 99% of the pie, so every place hides some of it.
        let title = [Rect::new(20.0, 4.0, 200.0, 14.0)];
        let keep = Keep {
            text: &title,
            ..Keep::default()
        };
        for (start, end) in [
            (0.0, 1.0),
            (1.0, 2.5),
            (2.5, 6.2),
            (0.05, std::f64::consts::TAU - 0.02),
        ] {
            let wedge = Wedge {
                center: [150.0, 150.0],
                radius: 130.0,
                inner: 0.0,
                start,
                end,
            };
            let run = || {
                place(
                    MarkShape::Wedge(wedge),
                    [120.0, 90.0],
                    [300.0, 300.0],
                    Rect::new(10.0, 10.0, 280.0, 280.0),
                    Rect::new(0.0, 0.0, 300.0, 300.0),
                    &keep,
                    false,
                    None,
                )
            };
            let fast = run();
            assert!(fast.1.is_fallback(), "{start}..{end}: {fast:?}");
            assert_eq!(fast, in_full(run), "{start}..{end}");
        }
    }

    #[test]
    fn pruned_search_matches_full_scoring_among_many_marks() {
        // 3,000 other points and a lattice of labels that every candidate touches.
        let others: Vec<Rect> = (0..3000)
            .map(|i| {
                let f = i as f64;
                Rect::new(
                    20.0 + (f * 37.0) % 560.0,
                    20.0 + (f * 61.0) % 300.0,
                    6.0,
                    6.0,
                )
            })
            .collect();
        let labels: Vec<Rect> = (0..96)
            .map(|i| {
                Rect::new(
                    10.0 + 48.0 * (i % 12) as f64,
                    10.0 + 40.0 * (i / 12) as f64,
                    24.0,
                    10.0,
                )
            })
            .collect();
        let keep = Keep {
            text: &labels,
            numbers: &labels,
            others: &others,
            ..Keep::default()
        };
        for center in [[100.0, 100.0], [300.0, 180.0], [520.0, 60.0]] {
            let run = || {
                place(
                    MarkShape::Point {
                        center,
                        radius: 4.0,
                    },
                    [420.0, 150.0],
                    [600.0, 340.0],
                    Rect::new(20.0, 20.0, 560.0, 300.0),
                    Rect::new(0.0, 0.0, 600.0, 340.0),
                    &keep,
                    false,
                    None,
                )
            };
            let fast = run();
            assert!(fast.1.is_fallback(), "{center:?}: {fast:?}");
            assert_eq!(fast, in_full(run), "{center:?}");
        }
    }

    /// The docs page's chord or venn chart, laid out at a 390 px phone's figure width.
    fn phone_chart(kind: &str) -> richview_chart::geometry::Geometry {
        let fixture = match kind {
            "chord" => include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../richview-chart/tests/fixtures/types/chord.json"
            )),
            _ => include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../richview-chart/tests/fixtures/types/venn.json"
            )),
        };
        let input: serde_json::Value = serde_json::from_str(fixture).unwrap();
        richview_chart::layout::layout(
            &richview_chart::block::ChartBlock::from_value(&input["spec"]).unwrap(),
            input["rows"].as_array().unwrap(),
            &richview_chart::layout::ChartTheme::default(),
            &richview_chart::layout::Options {
                width: 358.0,
                ..Default::default()
            },
        )
    }

    /// A boxed mark: its observation, its box and its painted outlines.
    type BoxedMark = (usize, Rect, Vec<Vec<[f64; 2]>>);

    /// Each chord ribbon, venn set and venn overlap.
    fn painted_marks(kind: &str, g: &richview_chart::geometry::Geometry) -> Vec<BoxedMark> {
        let panel = &g.panels[0];
        let marks: Vec<_> = panel
            .observations
            .iter()
            .enumerate()
            .filter(|(_, o)| o.wedge.is_none())
            .filter_map(|(i, o)| Some((i, o.area?, painted_paths(kind, panel, i))))
            .collect();
        assert!(!marks.is_empty(), "{kind}: no boxed marks");
        marks
    }

    fn polygon<'a>(anchor: Rect, paths: &'a [Vec<[f64; 2]>]) -> MarkShape<'a> {
        MarkShape::Polygon {
            anchor,
            paths,
            parts: &[],
            edge_slack: 0.05,
            hollow: None,
        }
    }

    #[test]
    fn a_chord_ribbon_or_venn_mark_is_its_ink_and_a_readout_on_the_ink_still_meets_it() {
        for kind in ["chord", "venn"] {
            let g = phone_chart(kind);
            let mut open_corners = 0;
            for (i, area, paths) in painted_marks(kind, &g) {
                assert!(!paths.is_empty(), "{kind} {i}: no painted outline");
                let mark = polygon(area, &paths);
                // Every point of the outline is ink: a readout over it meets the mark.
                for path in &paths {
                    for p in path.iter().step_by(7) {
                        let on_ink = Rect::new(p[0] - 1.0, p[1] - 1.0, 2.0, 2.0);
                        assert!(
                            touches(&mark, on_ink),
                            "{kind} {i}: ink at {p:?} counts as clear"
                        );
                    }
                }
                // A box's empty corner is no longer the mark.
                let corners = [
                    Rect::new(area.x, area.y, 8.0, 8.0),
                    Rect::new(area.right() - 8.0, area.y, 8.0, 8.0),
                    Rect::new(area.x, area.bottom() - 8.0, 8.0, 8.0),
                    Rect::new(area.right() - 8.0, area.bottom() - 8.0, 8.0, 8.0),
                ];
                open_corners += corners.iter().filter(|c| !touches(&mark, **c)).count();
            }
            assert!(open_corners > 0, "{kind}: every box corner is ink");
        }
        let bar = phone_chart("chord");
        assert!(painted_paths("bar", &bar.panels[0], 0).is_empty());
    }

    #[test]
    fn a_phone_readout_clears_a_chord_or_venn_mark_when_the_title_lies_flush_under_its_plot() {
        // Hand-drawn on a phone (5f27965a): the plot starts at the figure's top and the title row
        // lies right under it, then a two-line subtitle; neither dock fits.
        let size = [150.0, 60.0];
        let mut boxed_on_ink = 0;
        for kind in ["chord", "venn"] {
            let g = phone_chart(kind);
            let panel = &g.panels[0];
            let chart = [358.0, g.height];
            let title = [Rect::new(0.0, g.height, 358.0, 48.0)];
            let subtitle = [Rect::new(0.0, g.height + 48.0, 358.0, 42.0)];
            let visible = Rect::new(0.0, 0.0, 358.0, g.height + 90.0);
            let text: Vec<_> = panel
                .items
                .iter()
                .filter_map(|item| match item {
                    Item::Text(text) => Some(text.bounds),
                    _ => None,
                })
                .collect();
            let keep = Keep {
                fixed: &title,
                subtitle: &subtitle,
                text: &text,
                ..Keep::default()
            };
            for (i, area, paths) in painted_marks(kind, &g) {
                let mark = polygon(area, &paths);
                let (r, _) = place(mark, size, chart, panel.plot, visible, &keep, true, None);
                // Whether any place 8 px inside the figure clears both the ink and the title.
                let clear = (0..)
                    .map(|n| 8.0 + 4.0 * f64::from(n))
                    .take_while(|y| y + size[1] <= visible.bottom() - 8.0)
                    .any(|y| {
                        (0..)
                            .map(|n| 8.0 + 4.0 * f64::from(n))
                            .take_while(|x| x + size[0] <= 350.0)
                            .any(|x| {
                                let p = Rect::new(x, y, size[0], size[1]);
                                !touches(&mark, p) && !covers_any(&title, p)
                            })
                    });
                if clear {
                    assert!(
                        !touches(&mark, r),
                        "{kind} {i}: on the ink at {r:?} though a place clears it"
                    );
                    assert!(!covers_any(&title, r), "{kind} {i}: on the title at {r:?}");
                }
                let boxed = MarkShape::Rect {
                    rect: area,
                    end: None,
                };
                let (old, _) = place(boxed, size, chart, panel.plot, visible, &keep, true, None);
                if clear && touches(&boxed, old) && !touches(&mark, r) {
                    boxed_on_ink += 1;
                }
            }
        }
        assert!(
            boxed_on_ink > 0,
            "no mark the box left without a clear place"
        );
    }

    fn covers_any(rects: &[Rect], r: Rect) -> bool {
        rects.iter().any(|t| t.overlaps(&r, 0.0))
    }

    /// The text that holds the right of a 200 by 120 chart, beyond x 112.
    const RIGHT: Rect = Rect {
        x: 112.0,
        y: 0.0,
        w: 88.0,
        h: 120.0,
    };

    /// Place a readout 30 wide and as tall as its room, left of a point at (100, 60) whose halo
    /// reaches x 90, in a 200 by 120 chart: only places from x 8 to 60 at y 8 remain.
    fn squeezed(keep: &Keep<'_>) -> Rect {
        let chart = Rect::new(0.0, 0.0, 200.0, 120.0);
        let (r, side) = place(
            MarkShape::Point {
                center: [100.0, 60.0],
                radius: 6.0,
            },
            [30.0, 104.0],
            [200.0, 120.0],
            chart,
            chart,
            keep,
            false,
            None,
        );
        assert!(side.is_fallback(), "{side:?}");
        r
    }

    #[test]
    fn a_fallback_beside_stacked_tick_labels_covers_or_clears_them() {
        // Two tick labels, 80 above 60, from x 45 to 71. Clearing them is out of reach; covering
        // them wholly takes x from 41 to 45, a stretch between the 8 px grid's places.
        let ticks = [
            Rect::new(45.0, 20.0, 26.0, 12.0),
            Rect::new(45.0, 52.0, 26.0, 12.0),
        ];
        let fixed = [Rect::new(0.0, 0.0, 30.0, 120.0), RIGHT];
        let r = squeezed(&Keep {
            fixed: &fixed,
            text: &ticks,
            numbers: &ticks,
            ..Keep::default()
        });
        for tick in ticks {
            assert!(
                overlap(r, tick) == 0.0 || tick.inside(&r, EPS),
                "{r:?} cuts {tick:?}"
            );
        }
        assert!(r.x >= 41.0 - EPS && r.x <= 45.0 + EPS, "{r:?}");
    }

    #[test]
    fn a_number_covered_wholly_beats_two_cut_in_half() {
        // From x 30 the readout cuts two numbers that reach past its room; at x 8 it covers a
        // larger one whole and leaves those two clear.
        let large = Rect::new(10.0, 20.0, 20.0, 80.0);
        let top = Rect::new(45.0, 2.0, 35.0, 10.0);
        let bottom = Rect::new(45.0, 100.0, 35.0, 20.0);
        let numbers = [large, top, bottom];
        let fixed = [RIGHT];
        let r = squeezed(&Keep {
            fixed: &fixed,
            text: &numbers,
            numbers: &numbers,
            ..Keep::default()
        });
        assert!(large.inside(&r, EPS), "{r:?}");
        assert_eq!(overlap(r, top), 0.0, "{r:?}");
        assert_eq!(overlap(r, bottom), 0.0, "{r:?}");
    }

    #[test]
    fn a_word_costs_its_whole_area_when_touched() {
        // Beyond a fixed bar at x 40 to 50 the readout would touch a sliver of a large word;
        // before it, most of a small one. The small word is given up.
        let small = Rect::new(8.0, 40.0, 30.0, 10.0);
        let large = Rect::new(78.0, 2.0, 40.0, 25.0);
        let words = [small, large];
        let fixed = [Rect::new(40.0, 0.0, 10.0, 120.0), RIGHT];
        let r = squeezed(&Keep {
            fixed: &fixed,
            text: &words,
            ..Keep::default()
        });
        assert_eq!(overlap(r, large), 0.0, "{r:?}");
        assert!(overlap(r, small) > 0.0, "{r:?}");
    }

    #[test]
    fn a_side_place_keeps_a_gap_from_a_label_it_does_not_cover() {
        // Past the bar's end the readout runs from x 155 to 235. A label 3 px to its right
        // would read on as part of it, so the readout goes beside the bar; 7 px away it stays.
        let mark = Rect::new(180.0, 130.0, 30.0, 80.0);
        let near = [Rect::new(238.0, 90.0, 22.0, 12.0)];
        let (r, side) = sample(rect(mark), &near, None);
        assert_eq!(side, Side::Right, "{r:?}");
        assert!(!near[0].overlaps(&r.inflate(LABEL_GAP), 0.0), "{r:?}");
        let far = [Rect::new(242.0, 90.0, 22.0, 12.0)];
        assert_eq!(sample(rect(mark), &far, None).1, Side::Up);
    }

    #[test]
    fn a_fallback_keeps_a_gap_from_a_label_it_does_not_cover() {
        // Beside the point the readout would end at x 82, 3 px from a word; the nearest place
        // that keeps the gap ends 6 px before the word.
        let word = [Rect::new(85.0, 20.0, 10.0, 10.0)];
        let fixed = [RIGHT];
        let r = squeezed(&Keep {
            fixed: &fixed,
            text: &word,
            ..Keep::default()
        });
        assert!(r.right() <= 85.0 - LABEL_GAP, "{r:?}");
        assert!(r.right() > 85.0 - LABEL_GAP - 1.0, "{r:?}");
    }

    /// Place the readout of the fourth bar in a row of six, each 30 wide and 40 apart from
    /// x 60, with tops as given, in a plot from y 30 to 270 whose value labels sit on the bars.
    fn grouped(tops: [f64; 6], strong: &[usize]) -> (Rect, Side, Vec<Rect>, Vec<Rect>) {
        let bar = |i: usize| Rect::new(60.0 + 40.0 * i as f64, tops[i], 30.0, 270.0 - tops[i]);
        let labels: Vec<_> = (0..6)
            .map(|i| Rect::new(bar(i).x + 5.0, tops[i] - 16.0, 20.0, 14.0))
            .collect();
        let (mut full, mut dim) = (Vec::new(), Vec::new());
        for i in [0, 1, 2, 4, 5] {
            if strong.contains(&i) {
                full.push(bar(i));
            } else {
                dim.push(bar(i));
            }
        }
        let (r, side) = place(
            rect(bar(3)),
            [120.0, 30.0],
            [400.0, 300.0],
            Rect::new(30.0, 30.0, 340.0, 240.0),
            Rect::new(0.0, 0.0, 400.0, 300.0),
            &Keep {
                text: &labels,
                others: &dim,
                strong: &full,
                ..Keep::default()
            },
            false,
            None,
        );
        (r, side, full, dim)
    }

    #[test]
    fn a_bars_readout_takes_free_paper_before_a_place_over_its_neighbours() {
        // Left of the short fourth bar the readout would lie on three taller bars; the paper
        // above the low bars to its right is free.
        let (r, side, full, dim) = grouped([60.0, 70.0, 80.0, 200.0, 170.0, 180.0], &[1]);
        assert!(side.is_fallback(), "{side:?}");
        assert_eq!(area(&full, r) + area(&dim, r), 0.0, "{r:?}");
        assert!(r.x >= 150.0, "{r:?}");
    }

    #[test]
    fn a_bars_readout_gives_up_bars_that_stepped_back_before_those_at_full_strength() {
        // Every bar reaches the top of the plot, so every place covers some; the readout lies
        // on the two that stepped back, not on the three at full strength.
        let (r, _, full, dim) = grouped([34.0; 6], &[0, 1, 2]);
        assert_eq!(area(&full, r), 0.0, "{r:?}");
        assert!(area(&dim, r) > 0.0, "{r:?}");
    }

    #[test]
    fn a_free_place_far_from_its_bar_loses_to_the_bars_end_over_bars_that_stepped_back() {
        // A bar lying down between two longer ones that stepped back: past its end the readout
        // lies on both, and no free place is left in its own row. The paper far below is free,
        // but there the readout would read as another bar's.
        let bar = Rect::new(60.0, 40.0, 200.0, 16.0);
        let dim = [
            Rect::new(60.0, 22.0, 280.0, 16.0),
            Rect::new(60.0, 58.0, 280.0, 16.0),
        ];
        let (r, side) = place(
            MarkShape::Rect {
                rect: bar,
                end: Some(Side::Right),
            },
            [120.0, 30.0],
            [400.0, 300.0],
            Rect::new(60.0, 20.0, 320.0, 250.0),
            Rect::new(0.0, 0.0, 400.0, 300.0),
            &Keep {
                others: &dim,
                ..Keep::default()
            },
            false,
            None,
        );
        assert_eq!(side, Side::Right);
        assert_eq!(r, Rect::new(268.0, 33.0, 120.0, 30.0));
        assert!(area(&dim, r) > 0.0, "{r:?}");
    }

    #[test]
    fn a_free_place_with_its_bar_may_stand_flush_between_two_labels() {
        // Past the bar's end two value labels leave 38 px, too little for the readout's 30 and
        // a gap on both sides: it stands flush under the upper label, not away from its bar.
        let bar = Rect::new(60.0, 40.0, 200.0, 16.0);
        let labels = [
            Rect::new(300.0, 20.0, 30.0, 14.0),
            Rect::new(300.0, 72.0, 30.0, 14.0),
        ];
        let (r, side) = place(
            MarkShape::Rect {
                rect: bar,
                end: Some(Side::Right),
            },
            [120.0, 30.0],
            [400.0, 300.0],
            Rect::new(60.0, 20.0, 320.0, 250.0),
            Rect::new(0.0, 0.0, 400.0, 300.0),
            &Keep {
                text: &labels,
                ..Keep::default()
            },
            false,
            None,
        );
        assert!(side.is_fallback(), "{side:?}");
        assert!(
            labels.iter().all(|label| overlap(r, *label) == 0.0),
            "{r:?}"
        );
        assert!(
            (264.0..265.0).contains(&r.x) && (34.0..35.0).contains(&r.y),
            "{r:?}"
        );
    }

    #[test]
    fn a_bars_end_nearer_a_label_than_the_gap_still_beats_a_place_away_from_the_bar() {
        // The same bar between two longer ones, with a value label 4 px above the place past
        // its end: the readout stays there, counted as a fallback.
        let bar = Rect::new(60.0, 40.0, 200.0, 16.0);
        let dim = [
            Rect::new(60.0, 22.0, 280.0, 16.0),
            Rect::new(60.0, 58.0, 280.0, 16.0),
        ];
        let label = [Rect::new(344.0, 15.0, 20.0, 14.0)];
        let (r, side) = place(
            MarkShape::Rect {
                rect: bar,
                end: Some(Side::Right),
            },
            [120.0, 30.0],
            [400.0, 300.0],
            Rect::new(60.0, 20.0, 320.0, 250.0),
            Rect::new(0.0, 0.0, 400.0, 300.0),
            &Keep {
                others: &dim,
                text: &label,
                ..Keep::default()
            },
            false,
            None,
        );
        assert!(side.is_fallback(), "{side:?}");
        assert_eq!(r, Rect::new(268.0, 33.0, 120.0, 30.0));
    }

    #[test]
    fn a_phone_bar_that_cannot_dock_keeps_its_readout_in_its_own_row() {
        // Rows of bars fill a phone's plot and neither dock has room. Past the bar's end the
        // readout would lie on the longer bar under it and that bar's label; the paper just
        // above is free and in the bar's own row. A band at the bottom would lie on other rows.
        let bar = Rect::new(10.0, 120.0, 170.0, 12.0);
        let dim = [Rect::new(10.0, 134.0, 190.0, 12.0)];
        let strong = [Rect::new(10.0, 290.0, 300.0, 12.0)];
        let label = [Rect::new(204.0, 132.0, 14.0, 14.0)];
        let (r, side) = place(
            MarkShape::Rect {
                rect: bar,
                end: Some(Side::Right),
            },
            [120.0, 32.0],
            [360.0, 340.0],
            Rect::new(10.0, 20.0, 340.0, 300.0),
            Rect::new(0.0, 0.0, 360.0, 340.0),
            &Keep {
                others: &dim,
                strong: &strong,
                text: &label,
                ..Keep::default()
            },
            true,
            None,
        );
        assert!(side.is_fallback(), "{side:?}");
        assert!(stays(bar, Side::Right, r), "{r:?}");
        assert_eq!(
            area(&dim, r) + area(&strong, r) + area(&label, r),
            0.0,
            "{r:?}"
        );
        assert!(r.x < 190.0 && r.y > 88.0, "{r:?}");
    }

    #[test]
    fn a_long_phone_bar_keeps_its_readout_by_it_over_a_bar_that_stepped_back() {
        // Grouped rows on a phone, as in the every-block example: each group's name stands above
        // its two bars, the bars are too long for a readout past their ends, and neither dock
        // has room. Centred under the first bar the readout would lie on the next group's name.
        // A little along the bar it lies on the bar's dimmed partner alone; the band at the
        // bottom is 190 px away, over another group.
        let bar = Rect::new(24.0, 60.0, 214.0, 13.0);
        let dim = [
            Rect::new(24.0, 74.0, 178.0, 13.0),
            Rect::new(24.0, 126.0, 158.0, 13.0),
            Rect::new(24.0, 282.0, 224.0, 13.0),
        ];
        let strong = [
            Rect::new(24.0, 112.0, 194.0, 13.0),
            Rect::new(24.0, 268.0, 260.0, 13.0),
        ];
        let labels = [
            Rect::new(30.0, 40.0, 94.0, 15.0),
            Rect::new(242.0, 60.0, 14.0, 13.0),
            Rect::new(30.0, 92.0, 42.0, 15.0),
            Rect::new(222.0, 112.0, 14.0, 13.0),
            Rect::new(30.0, 246.0, 158.0, 15.0),
            Rect::new(288.0, 268.0, 14.0, 13.0),
        ];
        let size = [170.0, 26.0];
        let (r, side) = place(
            MarkShape::Rect {
                rect: bar,
                end: Some(Side::Right),
            },
            size,
            [358.0, 306.0],
            Rect::new(24.0, 36.0, 310.0, 262.0),
            Rect::new(0.0, 28.0, 358.0, 278.0),
            &Keep {
                others: &dim,
                strong: &strong,
                text: &labels,
                ..Keep::default()
            },
            true,
            None,
        );
        assert!(side.is_fallback(), "{side:?}");
        assert_eq!(area(&strong, r) + area(&labels, r), 0.0, "{r:?}");
        assert!(overlap(r, dim[0]) > 0.0, "{r:?}");
        let space = (r.y - bar.bottom()).max(bar.y - r.bottom());
        assert!((0.0..=16.0).contains(&space), "{r:?}");
        // With that bar at full strength too, nothing within reach is left: the readout is
        // searched for, and never takes the band over the full-strength bar at the bottom.
        let full: Vec<Rect> = strong.iter().chain(&dim).copied().collect();
        let (r, side) = place(
            MarkShape::Rect {
                rect: bar,
                end: Some(Side::Right),
            },
            size,
            [358.0, 306.0],
            Rect::new(24.0, 36.0, 310.0, 262.0),
            Rect::new(0.0, 28.0, 358.0, 278.0),
            &Keep {
                strong: &full,
                text: &labels,
                ..Keep::default()
            },
            true,
            None,
        );
        assert!(side.is_fallback(), "{side:?}");
        assert_eq!(area(&labels, r), 0.0, "{r:?}");
    }

    #[test]
    fn a_phone_bars_readout_goes_above_its_row_before_below() {
        // A label at the bar's end pushes the readout past it. On the 8 px grid the place a
        // little below the row's middle is nearer than the one above; the thumb hides below.
        let bar = Rect::new(10.0, 120.0, 170.0, 12.0);
        let label = [Rect::new(204.0, 118.0, 14.0, 14.0)];
        let (r, side) = place(
            MarkShape::Rect {
                rect: bar,
                end: Some(Side::Right),
            },
            [120.0, 32.0],
            [360.0, 340.0],
            Rect::new(10.0, 20.0, 340.0, 300.0),
            Rect::new(0.0, 0.0, 360.0, 340.0),
            &Keep {
                text: &label,
                ..Keep::default()
            },
            true,
            None,
        );
        assert!(side.is_fallback(), "{side:?}");
        assert!(stays(bar, Side::Right, r), "{r:?}");
        assert_eq!(r.y, 104.0, "{r:?}");
    }

    #[test]
    fn pruned_search_matches_full_scoring_beside_bars() {
        // Rows of six bars with random tops, some at full strength, some stepped back, with a
        // cumulative line across them: the pruned search and the full scoring agree on the
        // place of every bar's readout, on a wide chart and on a phone.
        let mut seed = 42_u64;
        let mut random = move || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as f64 / 2147483648.0
        };
        for round in 0..40 {
            let tops: Vec<f64> = (0..6).map(|_| 34.0 + 200.0 * random()).collect();
            let bar = |i: usize| Rect::new(60.0 + 40.0 * i as f64, tops[i], 30.0, 270.0 - tops[i]);
            let labels: Vec<_> = (0..6)
                .map(|i| Rect::new(bar(i).x + 5.0, tops[i] - 16.0, 20.0, 14.0))
                .collect();
            let line: Vec<[f64; 2]> = (0..6)
                .map(|i| [bar(i).x + 15.0, 250.0 - 40.0 * i as f64])
                .collect();
            let lines = if round % 2 == 0 {
                vec![line]
            } else {
                Vec::new()
            };
            for narrow in [false, true] {
                for hovered in 0..6 {
                    let (mut full, mut dim) = (Vec::new(), Vec::new());
                    for i in (0..6).filter(|i| *i != hovered) {
                        if (i + round) % 3 == 0 {
                            full.push(bar(i));
                        } else {
                            dim.push(bar(i));
                        }
                    }
                    let keep = Keep {
                        text: &labels,
                        numbers: &labels,
                        others: &dim,
                        strong: &full,
                        lines: &lines,
                        ..Keep::default()
                    };
                    let run = || {
                        place(
                            rect(bar(hovered)),
                            [120.0, 30.0],
                            [400.0, 300.0],
                            Rect::new(30.0, 30.0, 340.0, 240.0),
                            Rect::new(0.0, 0.0, 400.0, 300.0),
                            &keep,
                            narrow,
                            None,
                        )
                    };
                    assert_eq!(
                        run(),
                        in_full(run),
                        "round {round}, bar {hovered}, {narrow}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_free_margin_far_from_its_bar_loses_to_a_place_with_the_bar() {
        // A bar lying down between two longer ones that stepped back, with a label past its
        // end: none of its usual places is clear, and both margins are free paper far from it.
        // The readout stays in the bar's row, on the bar that stepped back below it.
        let bar = Rect::new(60.0, 140.0, 200.0, 16.0);
        let dim = [
            Rect::new(60.0, 122.0, 320.0, 16.0),
            Rect::new(60.0, 158.0, 320.0, 16.0),
        ];
        let label = [Rect::new(300.0, 140.0, 30.0, 14.0)];
        let (r, side) = place(
            MarkShape::Rect {
                rect: bar,
                end: Some(Side::Right),
            },
            [120.0, 30.0],
            [400.0, 300.0],
            Rect::new(60.0, 50.0, 320.0, 200.0),
            Rect::new(0.0, 0.0, 400.0, 300.0),
            &Keep {
                others: &dim,
                text: &label,
                ..Keep::default()
            },
            false,
            None,
        );
        assert!(side.is_fallback(), "{side:?}");
        assert!(stays(bar, Side::Right, r), "{r:?}");
        assert_eq!(overlap(r, label[0]), 0.0, "{r:?}");
        assert!(area(&dim, r) > 0.0, "{r:?}");
    }

    #[test]
    fn a_phone_bar_hanging_down_keeps_its_readout_above_it() {
        // Neither dock has room. Past the value end of a bar that points down is below it,
        // where the thumb is: the place above the bar goes first.
        let bar = Rect::new(150.0, 150.0, 40.0, 60.0);
        let (r, side) = place(
            MarkShape::Rect {
                rect: bar,
                end: Some(Side::Down),
            },
            [120.0, 32.0],
            [360.0, 340.0],
            Rect::new(10.0, 20.0, 340.0, 300.0),
            Rect::new(0.0, 0.0, 360.0, 340.0),
            &Keep::default(),
            true,
            None,
        );
        assert!(side.is_fallback(), "{side:?}");
        assert!(r.bottom() <= bar.y, "{r:?}");
        assert!(stays(bar, Side::Down, r), "{r:?}");
    }

    #[test]
    fn the_bar_search_leaves_out_no_place_that_stays_with_the_bar() {
        // The columns and rows left out before pairing are exactly those `stays` refuses, at
        // the reach's edge too, for a bar standing and one lying down.
        for (bar, end) in [
            (Rect::new(100.3, 200.7, 30.0, 60.0), Side::Up),
            (Rect::new(60.1, 100.9, 200.0, 16.0), Side::Right),
        ] {
            let size = [80.0, 30.0];
            let corner = [bar.x - size[0] / 2.0, bar.y - size[1] / 2.0];
            let at = |x: f64, y: f64| Rect::new(x, y, size[0], size[1]);
            let steps: Vec<f64> = (0..=800).map(|i| -200.0 + i as f64 * 0.75).collect();
            for &x in &steps {
                for &y in &steps {
                    assert_eq!(
                        stays(bar, end, at(x, y)),
                        stays(bar, end, at(x, corner[1])) && stays(bar, end, at(corner[0], y)),
                        "{bar:?} {x} {y}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_place_stays_with_its_bar_in_its_own_row_or_column_within_reach() {
        let upright = Rect::new(100.0, 200.0, 30.0, 60.0);
        let above = |gap: f64| Rect::new(90.0, 200.0 - gap - 30.0, 80.0, 30.0);
        assert!(stays(upright, Side::Up, above(REACH)));
        assert!(!stays(upright, Side::Up, above(REACH + 1.0)));
        assert!(!stays(
            upright,
            Side::Up,
            Rect::new(131.0, 150.0, 80.0, 30.0)
        ));
        let lying = Rect::new(60.0, 100.0, 200.0, 16.0);
        let beyond = |gap: f64| Rect::new(260.0 + gap, 95.0, 80.0, 30.0);
        assert!(stays(lying, Side::Right, beyond(REACH)));
        assert!(!stays(lying, Side::Right, beyond(REACH + 1.0)));
        assert!(!stays(
            lying,
            Side::Right,
            Rect::new(270.0, 117.0, 80.0, 30.0)
        ));
    }

    #[test]
    fn a_points_readout_stays_beside_it_over_other_marks() {
        let others = [Rect::new(150.0, 100.0, 200.0, 100.0)];
        let (_, side) = place(
            MarkShape::Point {
                center: [200.0, 150.0],
                radius: 6.0,
            },
            [80.0, 40.0],
            [400.0, 300.0],
            Rect::new(30.0, 30.0, 340.0, 240.0),
            Rect::new(0.0, 0.0, 400.0, 300.0),
            &keep(&others, &[], &[]),
            false,
            None,
        );
        assert_eq!(side, Side::Up);
    }

    /// A phone's chart 360 wide with its plot from y 40 to 240, tick labels to y 256, and a
    /// legend of two rows of two entries below; the readout docks under the tick labels.
    fn docked(size: [f64; 2], entries: &[Rect]) -> Placed {
        let ticks = [Rect::new(40.0, 242.0, 300.0, 14.0)];
        place_whole(
            MarkShape::NearestX {
                x: 180.0,
                dots: &[[180.0, 120.0]],
                lines: &[],
            },
            size,
            [360.0, 400.0],
            Rect::new(30.0, 40.0, 320.0, 200.0),
            Rect::new(-12.0, -12.0, 384.0, 424.0),
            &Keep {
                text: &ticks,
                entries,
                ..Keep::default()
            },
            true,
            None,
        )
    }

    const LEGEND: [Rect; 4] = [
        Rect {
            x: 0.0,
            y: 262.0,
            w: 150.0,
            h: 24.0,
        },
        Rect {
            x: 164.0,
            y: 262.0,
            w: 160.0,
            h: 24.0,
        },
        Rect {
            x: 0.0,
            y: 286.0,
            w: 130.0,
            h: 24.0,
        },
        Rect {
            x: 144.0,
            y: 286.0,
            w: 120.0,
            h: 24.0,
        },
    ];

    #[test]
    fn a_dock_over_a_legend_widens_to_cover_its_entries_whole() {
        // The dock runs from x 30 to 220 and y 256 to 316: it would leave the first letters of
        // two entries at its left and the end of one at its right.
        let placed = docked([190.0, 60.0], &LEGEND);
        assert_eq!(placed.side, Side::Bottom);
        assert_eq!(placed.rows, Rect::new(30.0, 256.0, 190.0, 60.0));
        assert_eq!(placed.surface, Rect::new(0.0, 256.0, 360.0, 60.0));
        assert!(LEGEND.iter().all(|entry| !cuts(placed.surface, *entry)));
    }

    #[test]
    fn a_widened_dock_ends_between_the_legends_lines() {
        // 40 tall, the dock would end 6 px into the second row; its surface ends below it.
        let placed = docked([190.0, 40.0], &LEGEND);
        assert_eq!(placed.rows, Rect::new(30.0, 256.0, 190.0, 40.0));
        assert_eq!(placed.surface, Rect::new(0.0, 256.0, 360.0, 54.0));
    }

    #[test]
    fn a_widened_readout_slides_between_lines_before_it_grows() {
        // No room below the plot, so the readout docks above it, from y 70 to 140, through the
        // first of two legend rows. Ten pixels up it covers both rows whole.
        let rows = [
            Rect::new(0.0, 60.0, 150.0, 24.0),
            Rect::new(164.0, 60.0, 160.0, 24.0),
            Rect::new(0.0, 84.0, 130.0, 24.0),
        ];
        let placed = place_whole(
            MarkShape::NearestX {
                x: 180.0,
                dots: &[[180.0, 200.0]],
                lines: &[],
            },
            [190.0, 70.0],
            [360.0, 300.0],
            Rect::new(30.0, 140.0, 320.0, 140.0),
            Rect::new(-12.0, -12.0, 384.0, 324.0),
            &Keep {
                entries: &rows,
                ..Keep::default()
            },
            true,
            None,
        );
        assert_eq!(placed.side, Side::Top);
        assert_eq!(placed.rows, Rect::new(30.0, 60.0, 190.0, 70.0));
        assert_eq!(placed.surface, Rect::new(0.0, 60.0, 360.0, 70.0));
    }

    #[test]
    fn a_widened_readout_reaches_a_legend_at_the_figures_edge() {
        // The figure ends with its legend, which starts at its left edge. Docked 8 px short of
        // that end the readout would run through the second row; it slides down to the figure's
        // edge and its surface covers both rows from their start.
        let rows = [
            Rect::new(0.0, 200.0, 150.0, 24.0),
            Rect::new(0.0, 224.0, 130.0, 24.0),
        ];
        let plot = Rect::new(30.0, 20.0, 320.0, 150.0);
        let figure = Rect::new(0.0, -60.0, 360.0, 308.0);
        let placed = place_whole(
            MarkShape::NearestX {
                x: 180.0,
                dots: &[[180.0, 100.0]],
                lines: &[],
            },
            [190.0, 60.0],
            [360.0, 190.0],
            plot,
            figure,
            &Keep {
                entries: &rows,
                ..Keep::default()
            },
            true,
            None,
        );
        assert_eq!(placed.rows, Rect::new(30.0, 188.0, 190.0, 60.0));
        assert_eq!(placed.surface, Rect::new(0.0, 188.0, 360.0, 60.0));
    }

    #[test]
    fn a_searched_place_cuts_no_legend_entry_and_never_widens() {
        // The title takes the upper dock's room and a key the lower one's; labels block both
        // bands and other marks fill the plot. The nearest place that covers nothing lies
        // across two legend entries: the readout gives up other marks instead, at its own size.
        let title = [Rect::new(0.0, 0.0, 360.0, 38.0)];
        let key = [Rect::new(30.0, 250.0, 125.0, 160.0)];
        let text = [
            Rect::new(40.0, 242.0, 300.0, 14.0),
            Rect::new(30.0, 40.0, 10.0, 10.0),
            Rect::new(30.0, 225.0, 10.0, 10.0),
        ];
        let others = [Rect::new(30.0, 40.0, 320.0, 200.0)];
        let placed = place_whole(
            MarkShape::NearestX {
                x: 180.0,
                dots: &[[180.0, 120.0]],
                lines: &[],
            },
            [150.0, 40.0],
            [360.0, 400.0],
            Rect::new(30.0, 40.0, 320.0, 200.0),
            Rect::new(-12.0, -12.0, 384.0, 424.0),
            &Keep {
                fixed: &title,
                key: &key,
                text: &text,
                others: &others,
                entries: &LEGEND,
                ..Keep::default()
            },
            true,
            None,
        );
        assert!(placed.side.is_fallback(), "{placed:?}");
        assert_eq!(placed.surface, placed.rows);
        assert!(
            LEGEND.iter().all(|entry| !cuts(placed.rows, *entry)),
            "{placed:?}"
        );
    }

    #[test]
    fn a_dock_that_cuts_no_entry_keeps_its_natural_size() {
        // One entry wholly under the dock, and none: the surface is the rows.
        for entries in [&[Rect::new(40.0, 262.0, 150.0, 24.0)][..], &[][..]] {
            let placed = docked([190.0, 60.0], entries);
            assert_eq!(placed.rows, Rect::new(30.0, 256.0, 190.0, 60.0));
            assert_eq!(placed.surface, placed.rows);
        }
    }

    #[test]
    fn a_dock_slides_to_cover_an_entry_before_it_widens() {
        // The one entry starts 10 px left of the dock: sliding there covers it whole and gives
        // up nothing more.
        let placed = docked([190.0, 60.0], &[Rect::new(20.0, 262.0, 150.0, 24.0)]);
        assert_eq!(placed.rows, Rect::new(20.0, 256.0, 190.0, 60.0));
        assert_eq!(placed.surface, placed.rows);
    }

    #[test]
    fn a_fallback_stands_beside_a_label_before_it_covers_one() {
        // Between a fixed strip and the point every place covers the first word, or stands
        // nearer than the gap to it or to the larger second one. Beside the first costs least.
        let words = [
            Rect::new(44.0, 20.0, 6.0, 6.0),
            Rect::new(91.0, 20.0, 10.0, 10.0),
        ];
        let fixed = [Rect::new(0.0, 0.0, 30.0, 120.0), RIGHT];
        let r = squeezed(&Keep {
            fixed: &fixed,
            text: &words,
            ..Keep::default()
        });
        assert!(words.iter().all(|word| overlap(r, *word) == 0.0), "{r:?}");
        assert!(r.x > 50.0 && r.right() <= 91.0 - LABEL_GAP, "{r:?}");
    }

    #[test]
    fn a_wedge_with_no_clear_place_keeps_its_readout_on_its_own_side() {
        // A rose wedge pointing lower left from the centre of a chart whose every place meets a
        // label: the nearest place is by the outer arc, not by the centre.
        let chart = Rect::new(0.0, 0.0, 400.0, 400.0);
        let wedge = Wedge {
            center: [200.0, 200.0],
            radius: 150.0,
            inner: 0.0,
            start: 1.2 * std::f64::consts::PI,
            end: 1.3 * std::f64::consts::PI,
        };
        let text = [chart];
        let (r, side) = place(
            MarkShape::Wedge(wedge),
            [90.0, 50.0],
            [400.0, 400.0],
            chart,
            chart,
            &Keep {
                text: &text,
                ..Keep::default()
            },
            false,
            None,
        );
        assert!(side.is_fallback(), "{side:?}");
        let mid = 1.25 * std::f64::consts::PI;
        let arc = [200.0 + 150.0 * mid.sin(), 200.0 - 150.0 * mid.cos()];
        let center = r.center();
        let from_arc = (center[0] - arc[0]).hypot(center[1] - arc[1]);
        let from_hub = (center[0] - 200.0).hypot(center[1] - 200.0);
        assert!(from_arc < from_hub, "{r:?}");
        assert!(center[0] < 200.0 && center[1] > 200.0, "{r:?}");
    }

    #[test]
    fn a_key_reckoned_once_sorts_as_comparing_it_every_time() {
        // Places on a lattice around a mark, many at equal distances, in a scrambled order.
        let mut places: Vec<(Rect, Side)> = (0..400)
            .map(|i| {
                let k = (i * 151) % 400;
                let side = if k % 3 == 0 { Side::Left } else { Side::Right };
                (
                    Rect::new((k % 20) as f64 * 8.0, (k / 20) as f64 * 8.0, 40.0, 20.0),
                    side,
                )
            })
            .collect();
        let distance = |r: Rect| {
            let c = r.center();
            (c[0] - 80.0).hypot(c[1] - 80.0)
        };
        let mut expected = places.clone();
        expected.sort_by(|a, b| distance(a.0).total_cmp(&distance(b.0)));
        sort_by_key_once(&mut places, |p| distance(p.0), f64::total_cmp);
        assert_eq!(places, expected);
    }

    /// Rows of bars 12 px tall every 14 px fill a phone's plot from y 20 to 320, so no readout
    /// 32 px tall lies free inside it, and neither dock has room. The bar read is the eighth row.
    fn packed_rows(width: f64) -> (Rect, Vec<Rect>) {
        let bar = Rect::new(10.0, 118.0, 170.0, 12.0);
        let rows = (0..21)
            .filter(|&k| k != 7)
            .map(|k| Rect::new(10.0, 20.0 + 14.0 * k as f64, width, 12.0))
            .collect();
        (bar, rows)
    }

    fn phone_bar(bar: Rect, plot: Rect, keep: &Keep<'_>) -> (Rect, Side) {
        place(
            MarkShape::Rect {
                rect: bar,
                end: Some(Side::Right),
            },
            [120.0, 32.0],
            [360.0, 340.0],
            plot,
            Rect::new(0.0, 0.0, 360.0, 340.0),
            keep,
            true,
            None,
        )
    }

    #[test]
    fn a_phone_bar_takes_a_free_place_outside_the_plot_before_one_over_other_bars() {
        // The plot ends at x 210 and an axis title fills the margin beside the bar's rows; the
        // margin below the title is free, 128 px from the bar. It reads as a dock (P1c (a)).
        let (bar, rows) = packed_rows(200.0);
        let title = [Rect::new(214.0, 20.0, 130.0, 230.0)];
        let (r, side) = phone_bar(
            bar,
            Rect::new(10.0, 20.0, 200.0, 300.0),
            &Keep {
                others: &rows,
                text: &title,
                ..Keep::default()
            },
        );
        assert!(side.is_fallback(), "{side:?}");
        assert!(
            !r.overlaps(&Rect::new(10.0, 20.0, 200.0, 300.0), 0.0),
            "{r:?}"
        );
        assert_eq!(area(&rows, r) + area(&title, r), 0.0, "{r:?}");
        assert!(apart(bar, r) > REACH, "{r:?}");
    }

    #[test]
    fn a_phone_bars_readout_lies_on_another_groups_bars_before_its_own_groups() {
        // Every place within reach lies on other bars, all stepped back. Its group is the five
        // rows just above it, so above, where the thumb does not reach, a near place lies on its
        // own group; below it lies on other groups' bars alone (P1c (a)).
        let (bar, rows) = packed_rows(300.0);
        let group = &rows[2..7];
        let (r, side) = phone_bar(
            bar,
            Rect::new(10.0, 20.0, 340.0, 300.0),
            &Keep {
                others: &rows,
                group,
                ..Keep::default()
            },
        );
        assert!(side.is_fallback(), "{side:?}");
        assert_eq!(area(group, r), 0.0, "{r:?}");
        assert!(area(&rows, r) > 0.0 && r.y > bar.bottom(), "{r:?}");
        assert!(apart(bar, r) <= REACH, "{r:?}");
        // Without the group, above goes first again.
        let (r, _) = phone_bar(
            bar,
            Rect::new(10.0, 20.0, 340.0, 300.0),
            &Keep {
                others: &rows,
                ..Keep::default()
            },
        );
        assert!(r.bottom() < bar.y, "{r:?}");
    }

    #[test]
    fn a_free_place_inside_a_phones_plot_beyond_reach_no_longer_counts() {
        // Lines run through every place within reach of the bar, so none is free, but none
        // covers a text; the plot is free from y 216 down, farther than 120 px from the bar.
        // That place would read as another bar's (P1c (a)).
        let bar = Rect::new(10.0, 40.0, 100.0, 12.0);
        let lines: Vec<Vec<[f64; 2]>> = (0..24)
            .map(|i| {
                let y = 24.0 + 8.0 * i as f64;
                vec![[10.0, y], [350.0, y]]
            })
            .collect();
        let plot = Rect::new(10.0, 20.0, 340.0, 300.0);
        let (r, side) = phone_bar(
            bar,
            plot,
            &Keep {
                lines: &lines,
                ..Keep::default()
            },
        );
        assert!(side.is_fallback(), "{side:?}");
        assert!(apart(bar, r) <= REACH, "{r:?}");
    }

    #[test]
    fn a_phone_bars_readout_takes_a_far_free_place_before_it_covers_a_label() {
        // Labels cover everything within reach of the bar; the plot is free from y 216 down.
        // Staying near would hide a label, and the far place hides none: the readout takes it,
        // as the placement check asks (the lead, 19 Nov 2026).
        let bar = Rect::new(10.0, 40.0, 100.0, 12.0);
        let labels = [Rect::new(8.0, 8.0, 344.0, 200.0)];
        let plot = Rect::new(10.0, 20.0, 340.0, 300.0);
        let (r, side) = phone_bar(
            bar,
            plot,
            &Keep {
                text: &labels,
                ..Keep::default()
            },
        );
        assert!(side.is_fallback(), "{side:?}");
        assert!(!labels[0].overlaps(&r, 0.0), "{r:?}");
        assert!(apart(bar, r) > REACH, "{r:?}");
    }

    #[test]
    fn a_tiles_readout_stands_beside_it_while_that_cuts_no_number() {
        // A heatmap of 80 by 60 cells, each with its number in the middle. Beside the cell the
        // readout covers the next cell's number whole; the margin right of the plot is free,
        // but there it would read as no cell's (P1c (d)).
        let tile = Rect::new(200.0, 160.0, 80.0, 60.0);
        let (mut cells, mut numbers) = (Vec::new(), Vec::new());
        for column in 0..5 {
            for row in 0..5 {
                let cell = Rect::new(
                    40.0 + 80.0 * column as f64,
                    40.0 + 60.0 * row as f64,
                    80.0,
                    60.0,
                );
                numbers.push(Rect::new(cell.x + 30.0, cell.y + 24.0, 20.0, 12.0));
                if cell != tile {
                    cells.push(cell);
                }
            }
        }
        let place_tile = |tiled: bool| {
            place(
                MarkShape::Rect {
                    rect: tile,
                    end: None,
                },
                [90.0, 40.0],
                [600.0, 400.0],
                Rect::new(40.0, 40.0, 400.0, 300.0),
                Rect::new(0.0, 0.0, 600.0, 400.0),
                &Keep {
                    others: &cells,
                    text: &numbers,
                    numbers: &numbers,
                    tiled,
                    ..Keep::default()
                },
                false,
                None,
            )
        };
        let (r, side) = place_tile(true);
        assert_eq!(r, Rect::new(292.0, 170.0, 90.0, 40.0));
        assert_eq!(side, Side::FallbackRight);
        assert!(numbers.iter().all(|number| !cuts(r, *number)));
        // Any other family takes the free margin.
        let (r, _) = place_tile(false);
        assert_eq!(area(&numbers, r) + area(&cells, r), 0.0, "{r:?}");
    }

    #[test]
    fn a_dock_covers_whole_lines_of_the_note_or_widens_to_the_content_width() {
        // The note's two lines lie under the tick labels; the dock, 190 wide from x 30, would
        // end inside the first line and cut the top of the second (P1c).
        let lines = [
            Rect::new(0.0, 262.0, 340.0, 18.0),
            Rect::new(0.0, 280.0, 200.0, 18.0),
        ];
        let placed = place_whole(
            MarkShape::NearestX {
                x: 180.0,
                dots: &[[180.0, 120.0]],
                lines: &[],
            },
            [190.0, 30.0],
            [360.0, 400.0],
            Rect::new(30.0, 40.0, 320.0, 200.0),
            Rect::new(-12.0, -12.0, 384.0, 424.0),
            &Keep {
                text: &[Rect::new(40.0, 242.0, 300.0, 14.0)],
                note: &[Rect::new(0.0, 262.0, 340.0, 36.0)],
                note_lines: &lines,
                ..Keep::default()
            },
            true,
            None,
        );
        assert_eq!(placed.side, Side::Bottom);
        assert_eq!(placed.rows.x, 30.0);
        assert!(
            lines.iter().all(|line| !cuts(placed.surface, *line)),
            "{placed:?}"
        );
        assert!(
            placed.surface.x <= 0.0 && placed.surface.right() >= 340.0,
            "{placed:?}"
        );
    }

    /// The four entries of the flagship's legend on a phone, two rows of one and a row of two,
    /// from `top`, with the legend's left edge at `left`.
    fn flagship_legend(left: f64, top: f64) -> [Rect; 4] {
        [
            Rect::new(left, top, 167.3, 25.2),
            Rect::new(left, top + 27.2, 189.3, 25.2),
            Rect::new(left, top + 54.4, 145.3, 25.2),
            Rect::new(left + 159.3, top + 54.4, 130.6, 25.2),
        ]
    }

    #[test]
    fn a_dock_cutting_a_legend_widens_under_the_axis_labels_and_never_slides_onto_them() {
        // Brutalist's flagship on a phone: the legend lies under the axis title, 22.7 px left
        // of the plot, so the dock cuts every entry across its width. A slide up to the last
        // row's top would clear the legend and lie on the tick labels instead.
        let ticks = [
            Rect::new(8.0, 139.0, 29.4, 17.8),
            Rect::new(83.3, 139.0, 29.4, 17.8),
            Rect::new(158.7, 139.0, 29.4, 17.8),
            Rect::new(234.0, 139.0, 29.4, 17.8),
            Rect::new(270.0, 159.0, 40.0, 17.8),
        ];
        let entries = flagship_legend(0.0, 188.6);
        let placed = place_whole(
            MarkShape::NearestX {
                x: 160.0,
                dots: &[[160.0, 60.0], [160.0, 100.0]],
                lines: &[],
            },
            [233.97, 98.94],
            [314.0, 188.6],
            Rect::new(22.7, 30.6, 282.3, 100.4),
            Rect::new(-22.0, -141.0, 358.0, 433.14),
            &Keep {
                text: &ticks,
                numbers: &ticks[..4],
                entries: &entries,
                ..Keep::default()
            },
            true,
            None,
        );
        assert_eq!(placed.side, Side::Bottom, "{placed:?}");
        assert!(
            ticks
                .iter()
                .all(|tick| overlap(placed.surface, *tick) <= EPS),
            "{placed:?}"
        );
        assert!(
            entries
                .iter()
                .all(|entry| entry.inside(&placed.surface, EPS)),
            "{placed:?}"
        );
        assert_eq!(placed.rows.x, 22.7, "{placed:?}");
    }

    #[test]
    fn a_searched_phone_place_over_the_legend_widens_to_keep_its_entries_whole() {
        // Sleek's flagship on a phone: the legend lies between the subtitle and the axis title
        // over the plot, and no dock fits. The place found stands in for the legend, whose
        // right column it would cut.
        let fixed = [
            Rect::new(-0.6, -181.3, 234.0, 57.3),
            Rect::new(250.0, -187.7, 68.0, 32.0),
        ];
        let subtitle = [Rect::new(0.0, -120.0, 318.0, 40.6)];
        let text = [
            Rect::new(0.0, 1.6, 129.6, 15.6),
            Rect::new(0.0, 19.7, 16.0, 15.6),
            Rect::new(0.0, 56.4, 16.0, 15.6),
            Rect::new(0.0, 93.2, 16.0, 15.6),
            Rect::new(8.0, 129.9, 8.0, 15.6),
            Rect::new(10.0, 142.0, 29.4, 15.6),
            Rect::new(105.0, 142.0, 29.4, 15.6),
            Rect::new(200.0, 142.0, 29.4, 15.6),
            Rect::new(285.0, 158.0, 33.0, 15.6),
        ];
        let entries = [
            Rect::new(0.0, -58.3, 154.2, 25.2),
            Rect::new(168.2, -58.3, 145.2, 25.2),
            Rect::new(0.0, -31.2, 134.3, 25.2),
            Rect::new(148.3, -31.2, 112.1, 25.2),
        ];
        let plot = Rect::new(22.4, 27.4, 286.6, 110.3);
        let placed = place_whole(
            MarkShape::NearestX {
                x: 150.0,
                dots: &[[150.0, 60.0], [150.0, 120.0]],
                lines: &[],
            },
            [205.64, 98.94],
            [318.0, 190.9],
            plot,
            Rect::new(-20.0, -197.3, 358.0, 404.2),
            &Keep {
                fixed: &fixed,
                subtitle: &subtitle,
                text: &text,
                numbers: &text[1..8],
                entries: &entries,
                ..Keep::default()
            },
            true,
            None,
        );
        assert!(
            entries.iter().all(|entry| !cuts(placed.surface, *entry)),
            "{placed:?}"
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.inside(&placed.surface, EPS)),
            "{placed:?}"
        );
        for kept in fixed.iter().chain(&subtitle).chain([&plot]) {
            assert!(overlap(placed.surface, *kept) <= EPS, "{placed:?}");
        }
        assert!(placed.rows.inside(&placed.surface, EPS), "{placed:?}");
    }

    #[test]
    fn a_surface_that_would_run_into_the_toolbar_slides_to_whole_entries_above_it() {
        // Hand-drawn's flagship on a phone: the title row follows the legend, and the toolbar
        // reaches half a pixel into the legend's last row. A readout as tall as the room
        // between the axis title and the title covers the first two rows whole and leaves the
        // last one clear, although it then lies on the tick labels.
        let fixed = [
            Rect::new(-0.5, 298.9375, 274.0, 38.2),
            Rect::new(290.0, 292.5, 68.0, 32.0),
        ];
        let subtitle = [Rect::new(0.0, 341.2, 358.0, 40.6)];
        let text = [
            Rect::new(7.0, 166.9, 23.8, 16.2),
            Rect::new(95.7, 166.9, 23.8, 16.2),
            Rect::new(183.7, 166.9, 23.8, 16.2),
            Rect::new(271.5, 166.9, 23.8, 16.2),
            Rect::new(322.0, 184.0, 34.0, 16.2),
        ];
        let entries = [
            Rect::new(0.0, 213.4, 172.4, 25.2),
            Rect::new(0.0, 240.6, 194.0, 25.2),
            Rect::new(0.0, 267.8, 150.8, 25.2),
            Rect::new(164.8, 267.8, 136.4, 25.2),
        ];
        let plot = Rect::new(19.9, 28.4, 329.1, 130.5);
        let placed = place_whole(
            MarkShape::NearestX {
                x: 150.0,
                dots: &[[150.0, 60.0], [150.0, 120.0]],
                lines: &[],
            },
            [243.22, 98.9375],
            [358.0, 213.4],
            plot,
            Rect::new(0.0, 0.0, 358.0, 402.14),
            &Keep {
                fixed: &fixed,
                subtitle: &subtitle,
                text: &text,
                numbers: &text[..4],
                entries: &entries,
                ..Keep::default()
            },
            true,
            None,
        );
        assert!(
            entries.iter().all(|entry| !cuts(placed.surface, *entry)),
            "{placed:?}"
        );
        assert!(
            text.iter().all(|label| !cuts(placed.surface, *label)),
            "{placed:?}"
        );
        for kept in fixed.iter().chain(&subtitle).chain([&plot]) {
            assert!(overlap(placed.surface, *kept) <= EPS, "{placed:?}");
        }
        assert!(
            entries[..2]
                .iter()
                .all(|entry| entry.inside(&placed.surface, EPS))
        );
        assert!(placed.rows.inside(&placed.surface, EPS), "{placed:?}");
    }

    #[test]
    fn a_phone_readout_spares_its_legend_when_axis_label_boxes_touch() {
        // The zero tick's box extends 0.124 px into the year labels' top edges. A readout
        // just below the zero tick covers those years and the first two legend rows whole.
        let labels = [
            Rect::new(5.94, 128.255, 5.94, 16.248),
            Rect::new(8.0, 144.379, 23.76, 16.248),
            Rect::new(85.08459, 144.379, 23.76, 16.248),
            Rect::new(162.38037, 144.379, 23.76, 16.248),
            Rect::new(239.46495, 144.379, 23.76, 16.248),
            Rect::new(282.432, 162.627, 26.568, 16.248),
        ];
        let entries = [
            Rect::new(0.0, 190.875, 172.40625, 25.1875),
            Rect::new(0.0, 218.0625, 194.0, 25.1875),
            Rect::new(0.0, 245.25, 150.79688, 25.1875),
            Rect::new(164.79688, 245.25, 136.40625, 25.1875),
        ];
        let fixed = [
            Rect::new(-0.5, 276.4375, 234.0, 57.32813),
            Rect::new(250.0, 270.0, 68.0, 32.0),
        ];
        let placed = place_whole(
            MarkShape::NearestX {
                x: 129.06557,
                dots: &[[129.06557, 107.00110], [129.06557, 134.36287]],
                lines: &[],
            },
            [243.21875, 98.9375],
            [318.0, 190.875],
            Rect::new(19.88, 28.372, 289.12, 108.007),
            Rect::new(-20.0, 0.0, 358.0, 398.75),
            &Keep {
                fixed: &fixed,
                text: &labels,
                numbers: &labels[..5],
                entries: &entries,
                ..Keep::default()
            },
            true,
            None,
        );
        assert!(overlap(placed.surface, labels[0]) <= EPS, "{placed:?}");
        assert!(
            entries[..2]
                .iter()
                .all(|entry| entry.inside(&placed.surface, EPS)),
            "{placed:?}"
        );
        assert!(
            entries[2..]
                .iter()
                .all(|entry| overlap(placed.surface, *entry) <= EPS),
            "{placed:?}"
        );
    }

    #[test]
    fn the_first_place_stands_where_no_place_keeps_every_entry_whole() {
        // One legend entry as wide as the figure, with the title right under it: a readout of
        // half that width cuts it wherever it stands, and a surface as wide as the entry would
        // cover more of the plot. The second, widening search changes nothing then.
        let entries = [Rect::new(0.0, 50.0, 200.0, 20.0)];
        let keep = Keep {
            fixed: &[Rect::new(0.0, 70.5, 200.0, 30.0)],
            entries: &entries,
            ..Keep::default()
        };
        let place = |whole: bool| {
            let find = if whole { place_whole } else { place_once };
            find(
                MarkShape::Point {
                    center: [100.0, 5.0],
                    radius: 5.0,
                },
                [100.0, 60.0],
                [200.0, 150.0],
                Rect::new(0.0, 0.0, 200.0, 40.0),
                Rect::new(0.0, 0.0, 200.0, 150.0),
                &keep,
                true,
                None,
            )
        };
        let placed = place(true);
        assert!(cuts(placed.surface, entries[0]), "{placed:?}");
        assert_eq!(placed, place(false));
    }

    #[test]
    fn the_widening_search_is_tried_only_where_the_first_place_cuts_an_entry() {
        // The dock of the flagship's legend, wide enough to hold its entries: the first place
        // cuts none, and stands although a second search would count more places as whole.
        let entries = flagship_legend(22.7, 160.0);
        let keep = Keep {
            entries: &entries,
            ..Keep::default()
        };
        let place = |keep: &Keep<'_>, whole: bool| {
            let find = if whole { place_whole } else { place_once };
            find(
                MarkShape::NearestX {
                    x: 160.0,
                    dots: &[[160.0, 60.0]],
                    lines: &[],
                },
                [300.0, 98.94],
                [330.0, 300.0],
                Rect::new(22.7, 30.6, 290.0, 100.4),
                Rect::new(-12.0, -12.0, 354.0, 324.0),
                keep,
                true,
                None,
            )
        };
        let first = place(&keep, false);
        assert!(
            entries.iter().all(|entry| !cuts(first.surface, *entry)),
            "{first:?}"
        );
        assert_eq!(place(&keep, true), first);
    }

    #[test]
    fn a_dock_widens_over_a_note_line_beside_a_mark_at_the_foot_of_the_plot() {
        // A sankey node stands on the plot's lower edge; the dock, narrow and to its left,
        // lies on part of the note's first line. The widened surface comes flush under the
        // node, which it does not cover.
        let lines = [
            Rect::new(0.0, 191.3, 314.0, 18.0),
            Rect::new(0.0, 211.6, 290.0, 18.0),
        ];
        let node = Rect::new(140.0, 148.0, 14.0, 30.9);
        let plot = Rect::new(0.0, 0.0, 318.0, 178.9);
        let placed = place_whole(
            MarkShape::Rect {
                rect: node,
                end: None,
            },
            [96.11, 30.19],
            [318.0, 190.2],
            plot,
            Rect::new(-20.0, -149.8, 358.0, 417.7),
            &Keep {
                note: &[Rect::new(0.0, 190.3, 318.0, 40.6)],
                note_lines: &lines,
                ..Keep::default()
            },
            true,
            None,
        );
        assert_eq!(placed.side, Side::Bottom, "{placed:?}");
        assert!(
            lines.iter().all(|line| !cuts(placed.surface, *line)),
            "{placed:?}"
        );
        assert!(lines[0].inside(&placed.surface, EPS), "{placed:?}");
        assert!(overlap(placed.surface, node) <= EPS, "{placed:?}");
        assert!(overlap(placed.surface, plot) <= EPS, "{placed:?}");
    }
}
