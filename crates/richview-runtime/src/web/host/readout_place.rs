//! Place a readout using the selected mark, visible bounds, and chart geometry.

#[cfg(not(test))]
use super::wedge_readout;
#[cfg(test)]
use crate::test_wedge_readout as wedge_readout;
use richview_chart::geometry::{Rect, Wedge};

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
/// them up in this order: other marks, then chart text, the note, a legend it stands in for, the
/// subtitle and a gauge's key; its own mark, the title and the toolbar go last (P1b).
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
    /// The bounds of every other filled or point mark.
    pub others: &'a [Rect],
    /// Every other line mark, costed by the length a readout hides.
    pub lines: &'a [Vec<[f64; 2]>],
}

/// Hidden line length counts as this much area: about a stroke's width.
const LINE_WEIGHT: f64 = 2.0;
/// Below this, two costs are the same.
const EPS: f64 = 1e-6;

fn overlap(a: Rect, b: Rect) -> f64 {
    (a.right().min(b.right()) - a.x.max(b.x)).max(0.0)
        * (a.bottom().min(b.bottom()) - a.y.max(b.y)).max(0.0)
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
    'positions: for y in positions {
        let rect = Rect::new(x, y, size[0], size[1]);
        // A segment whose rows miss the box adds nothing, and a sum past the best so far by more
        // than the tolerance cannot win, so neither needs reading to the end (P11).
        let limit = best.0 + 1e-6;
        let mut covered = 0.0;
        for s in &segments {
            if prune && (s[0][1].max(s[1][1]) < rect.y || s[0][1].min(s[1][1]) > rect.bottom()) {
                continue;
            }
            covered += segment_length(s[0], s[1], rect);
            if prune && covered > limit {
                continue 'positions;
            }
        }
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
    if let MarkShape::NearestX { dots, lines, .. } = &mark
        && !narrow
    {
        candidates = [Side::Right, Side::Left]
            .into_iter()
            .map(|side| {
                let mut rect = at(side, 12.0);
                rect.y = line_y(rect.x, size, plot, dots, lines);
                (rect, side)
            })
            .collect();
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
    // Docks and bands may cover the note and a legend the readout stands in for.
    let clear_of_text = |r: Rect| {
        !covers(keep.fixed, r)
            && !covers(keep.key, r)
            && !covers(keep.subtitle, r)
            && !covers(keep.text, r)
    };
    let valid = |r: Rect| {
        r.inside(&allowed, 0.0)
            && !touches(&mark, r)
            && clear_of_text(r)
            && !covers(keep.legend, r)
            && !covers(keep.note, r)
    };
    // A gauge's readout tries the half of its hollow away from the needle first; on a narrow
    // chart, right after the docks.
    let hollow = || match &mark {
        MarkShape::Polygon {
            hollow: Some(hollow),
            ..
        } => in_hollow(hollow, size, valid),
        _ => None,
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
        if let Some(&candidate) = bands.iter().find(|(r, _)| clear_band(*r)) {
            return candidate;
        }
        candidates.extend(bands);
    }
    if !narrow && let Some(r) = hollow() {
        return (r, Side::Hollow);
    }
    if !narrow
        && let Some(p) = previous
        && let Some(&(r, s)) = candidates
            .iter()
            .find(|(r, s)| *s == p.canonical() && valid(*r))
    {
        return (r, s);
    }
    if !narrow && let Some(&(r, s)) = candidates.iter().find(|(r, _)| valid(*r)) {
        return (r, s);
    }
    let mut fallback: Vec<_> = candidates
        .into_iter()
        .map(|(mut r, s)| {
            r.x = r.x.clamp(allowed.x, (allowed.right() - r.w).max(allowed.x));
            r.y =
                r.y.clamp(allowed.y, (allowed.bottom() - r.h).max(allowed.y));
            (r, s)
        })
        .collect();
    // When the usual candidates cannot clear everything, search the readout's room, 8 px in: the
    // visible figure on narrow charts, the visible chart on wider ones (f614f4ec).
    let (min_x, max_x, min_y, max_y) = (
        allowed.x,
        allowed.right() - size[0],
        allowed.y,
        allowed.bottom() - size[1],
    );
    if max_x >= min_x && max_y >= min_y {
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
        // exactly beside them.
        let mut boundaries: Vec<Rect> =
            [keep.fixed, keep.key, keep.subtitle, keep.legend, keep.note]
                .concat()
                .into_iter()
                .chain(keep.text.iter().copied())
                .collect();
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
    // Ties go to the place nearest the docks on a narrow chart, and nearest the mark otherwise.
    let anchor = match &mark {
        MarkShape::NearestX { x, dots, .. } if !dots.is_empty() => [
            *x,
            dots.iter().map(|p| p[1]).sum::<f64>() / dots.len() as f64,
        ],
        _ => c,
    };
    let distance = |r: Rect| {
        if narrow {
            docks
                .iter()
                .map(|&y| (r.x - dock_x).abs() + (r.y - y).abs())
                .fold(f64::INFINITY, f64::min)
        } else {
            let center = r.center();
            (center[0] - anchor[0]).hypot(center[1] - anchor[1])
        }
    };
    fallback.sort_by(|a, b| distance(a.0).total_cmp(&distance(b.0)));
    let (r, s) = fallback[Search::new(&mark, keep).rank(&fallback)];
    (r, s.fallback())
}

/// What a place gives up, from what the readout keeps longest to what it gives up first.
const TIERS: usize = 8;

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
    others: Grid<'s>,
    text: Grid<'s>,
    lines: Vec<Line>,
}

/// One line of other marks, its segments with their bounding boxes, and whether its points run
/// left to right, so the segments a box can meet are found by binary search.
struct Line {
    segments: Vec<([f64; 2], [f64; 2], Rect)>,
    xs: Option<Vec<f64>>,
}

impl Line {
    fn new(points: &[[f64; 2]]) -> Self {
        let segments = points
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
        let sorted = points.windows(2).all(|s| s[0][0] <= s[1][0]);
        Self {
            segments,
            xs: sorted.then(|| points.iter().map(|p| p[0]).collect()),
        }
    }
    /// The segments whose boxes may meet `r`, in order; a little wide, since a box's right edge
    /// is computed and can differ from the point's x in the last bit.
    fn near(&self, r: Rect) -> &[([f64; 2], [f64; 2], Rect)] {
        let Some(xs) = &self.xs else {
            return &self.segments;
        };
        let slack = 1e-6 * (1.0 + r.x.abs().max(r.right().abs()));
        let first = xs.partition_point(|x| *x < r.x - slack).saturating_sub(1);
        let last = xs
            .partition_point(|x| *x <= r.right() + slack)
            .min(self.segments.len());
        &self.segments[first.min(last)..last]
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
    fn new(mark: &'s MarkShape<'s>, keep: &'s Keep<'s>) -> Self {
        Self {
            mark,
            keep,
            others: Grid::new(keep.others),
            text: Grid::new(keep.text),
            lines: keep.lines.iter().map(|line| Line::new(line)).collect(),
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
            2 => area(keep.key, r),
            3 => area(keep.subtitle, r),
            4 => area(keep.legend, r),
            5 => area(keep.note, r),
            6 => self.text.area(r),
            _ => {
                let others = self.others.area(r);
                if others > limit {
                    return None;
                }
                let mut hidden = 0.0;
                for line in &self.lines {
                    for &(a, b, bounds) in line.near(r) {
                        if meets(bounds, r) {
                            hidden += segment_length(a, b, r);
                            if others + LINE_WEIGHT * hidden > limit {
                                return None;
                            }
                        }
                    }
                }
                others + LINE_WEIGHT * hidden
            }
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
            2 => area(keep.key, r),
            3 => area(keep.subtitle, r),
            4 => area(keep.legend, r),
            5 => area(keep.note, r),
            6 => area(keep.text, r),
            _ => {
                area(keep.others, r)
                    + LINE_WEIGHT
                        * segments
                            .iter()
                            .filter(|(_, _, bounds)| meets(*bounds, r))
                            .map(|&&(a, b, _)| segment_length(a, b, r))
                            .sum::<f64>()
            }
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
    spots.sort_by(|a, b| distance(a).total_cmp(&distance(b)));
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
                    Rect::new(220.0, 100.0, 100.0, 115.0),
                    Rect::new(70.0, 100.0, 100.0, 115.0)
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
            rect(Rect::new(120.0, 170.0, 20.0, 30.0)),
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
            rect(Rect::new(120.0, 60.0, 40.0, 60.0)),
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
            rect(Rect::new(140.0, 80.0, 30.0, 70.0)),
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
            lines: &lines,
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
}
